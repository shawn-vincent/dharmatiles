//! Format 1's numbers: kinds checked and units lowered where a value is
//! read, and points in parentheses.
//!
//! Everything downstream reads the reference numbers format 0 always did
//! (lengths in design millimetres, angles in degrees, points in squares from
//! the south-west corner); the language's side of the boundary is here.

use miette::SourceSpan;

use super::{Ctx, Decoder, sub_span};
use crate::dim::Dim;
use crate::expr::{self, BinOp, Expr};
use crate::place::{Corner, Edge, Point};
use crate::schema::Axis;
use crate::units::{Base, Kind, KindError, Want, expect, kind_of, lower};

/// What a control's axis wants of a value.
pub(crate) fn want_of(axis: Axis) -> Want {
    match axis {
        Axis::Mm => Want::Length,
        Axis::Deg => Want::Angle,
        _ => Want::Plain,
    }
}

/// The kind a stored dimension is read as.
pub(crate) fn kind_of_dim(d: Dim) -> Kind {
    match d {
        Dim::Mm => Kind::Length,
        Dim::Angle => Kind::Angle,
        Dim::Region => Kind::Region,
        Dim::Scalar | Dim::Squares => Kind::Plain,
    }
}

/// The dimension a value of this kind is stored as.
pub(crate) fn dim_of_kind(k: Kind) -> Dim {
    match k {
        Kind::Length => Dim::Mm,
        Kind::Angle => Dim::Angle,
        Kind::Region => Dim::Region,
        _ => Dim::Scalar,
    }
}

impl Decoder<'_> {
    /// What a name measures where `ctx` is read: the point names, the
    /// regions, the library's heights and the scope's constants; a
    /// parameter by the control it fills; `None` for a `$` name a caller
    /// installs, which `check` settles where it is called.
    pub(crate) fn kind_of_name(&self, ctx: &Ctx, n: &str) -> Option<Kind> {
        match n {
            "$x" | "$y" | "$d" | "$a" | "$_y" | "x" | "y" => Some(Kind::Plain),
            "$bottom" | "bottom" => Some(Kind::Length),
            "$region" | "$tile" => Some(Kind::Region),
            _ => ctx
                .consts
                .iter()
                .rev()
                .find(|(k, _)| k == n)
                .map(|(_, d)| kind_of_dim(*d))
                .or_else(|| {
                    ctx.params.iter().any(|p| p == n).then(|| {
                        self.param_controls
                            .get(n)
                            .and_then(|k| k.value)
                            .map_or(Kind::Any, |a| match want_of(a) {
                                Want::Length => Kind::Length,
                                Want::Angle => Kind::Angle,
                                _ => Kind::Plain,
                            })
                    })
                }),
        }
    }

    /// Check that `x` measures `want` and lower its units; a mistake is
    /// reported at `span` and gives `None`.
    pub(crate) fn check_lower(
        &mut self,
        x: &Expr,
        want: Want,
        ctx: &Ctx,
        span: SourceSpan,
        what: &str,
    ) -> Option<(Expr, Kind)> {
        let names = |n: &str| self.kind_of_name(ctx, n);
        let kind = match expect(x, want, &names, false) {
            Ok(k) => k,
            Err(ke) => {
                self.kind_error(&ke, span, what);
                return None;
            }
        };
        let base = Base::Dmm;
        match lower(x, base, self.known.units(), None) {
            Ok(l) => Some((l, kind)),
            Err(ke) => {
                self.kind_error(&ke, span, what);
                None
            }
        }
    }

    pub(crate) fn kind_error(&mut self, ke: &KindError, span: SourceSpan, what: &str) {
        self.err(ke.code, span, format!("{what}: {}", ke.message), "here")
            .help = ke.help.clone();
    }

    /// A point written in format 1: a corner or an edge word, `"(x, y)"`,
    /// `point(frame, x, y)` or `polar(frame, d, a)`.  The point is read in
    /// the frame the declaration sits in and returned in squares from the
    /// south-west corner, as geometry keeps it: across is `x`, up is
    /// `rows - y`.
    ///
    /// `at` is where the text starts inside `span`.
    pub(crate) fn point1(
        &mut self,
        s: &str,
        span: SourceSpan,
        at: usize,
        ctx: &Ctx,
    ) -> Option<Point> {
        let t = s.trim();
        let lead = s.len() - s.trim_start().len();
        let inner = |a: usize, b: usize| sub_span(span, at + lead + a, at + lead + b);
        if let Some(c) = Corner::parse(t) {
            return Some(Point::Corner(c));
        }
        if let Some(e) = Edge::parse(t) {
            return Some(Point::Edge(e));
        }
        let Some((cols, rows)) = ctx.size.map(|(c, r)| (f64::from(c), f64::from(r))) else {
            self.err(
                "dt::point",
                span,
                "a point is read in the tile it sits in, and a part has no tile",
                "no tile here",
            )
            .help = Some(
                "write the point in the tile that calls the part, or name a place the tile declares"
                    .to_owned(),
            );
            return None;
        };
        let call = t.starts_with("point(") || t.starts_with("polar(");
        if call && t.ends_with(')') {
            return self.point_call(t, span, inner(0, t.len()), (cols, rows), ctx);
        }
        if t.starts_with('(') && crate::place::encloses(t) {
            if let Some(region) = self.frame_region.clone() {
                self.err(
                    "dt::point",
                    inner(0, t.len()),
                    format!("this point is in the frame of the region `{region}`, whose box is known only when the tile is built"),
                    "in a region's frame",
                )
                .help = Some(
                    "write it against the tile: `point($tile, 0.5sq, 0.5sq)`; a point in a named region's own frame is not available yet"
                        .to_owned(),
                );
                return None;
            }
            let body = &t[1..t.len() - 1];
            let commas = crate::place::top_commas(body);
            let [comma] = commas[..] else {
                self.err(
                    "dt::point",
                    inner(0, t.len()),
                    format!(
                        "a point has two halves, across and down: `{t}` has {}",
                        commas.len() + 1
                    ),
                    "not a pair",
                )
                .help = Some(
                    "write \"(0.65sq, 1sq)\": across from the west edge, down from the north"
                        .to_owned(),
                );
                return None;
            };
            let halves = [(&body[..comma], 1), (&body[comma + 1..], comma + 2)];
            let mut out = Vec::new();
            for (k, (text, off)) in halves.iter().enumerate() {
                let extent = if k == 0 { cols } else { rows };
                let half_span = inner(*off, off + text.len());
                out.push(self.point_half(text, half_span, ctx, extent)?);
            }
            let y = out.pop()?;
            let x = out.pop()?;
            return Some(Point::Pair {
                x,
                y: north_west(rows, y),
            });
        }
        // a point without its parentheses
        let parts: Vec<&str> = t.split(',').map(str::trim).collect();
        let help = match parts[..] {
            [a, b] if !a.is_empty() && !b.is_empty() => {
                let unit = |p: &str| {
                    if p.chars().all(|c| c.is_ascii_digit() || c == '.') {
                        format!("{p}sq")
                    } else {
                        p.to_owned()
                    }
                };
                format!("write it with parentheses and units: \"({}, {})\"", unit(a), unit(b))
            }
            _ => "write \"(0.65sq, 1sq)\": parentheses, across then down, each with its unit; or `point($tile, 50%, 0)`".to_owned(),
        };
        self.err(
            "dt::point",
            inner(0, t.len()),
            format!("a point is written in parentheses: `{t}` has none"),
            "no parentheses",
        )
        .help = Some(help);
        None
    }

    /// One half of a tuple: a length, `%` meaning a share of the frame's
    /// size across or down (`extent`, squares), in squares.
    fn point_half(&mut self, text: &str, span: SourceSpan, ctx: &Ctx, extent: f64) -> Option<Expr> {
        let parsed = match expr::parse_units(text) {
            Ok(x) => x,
            Err(err) => {
                let lead = text.len() - text.trim_start().len();
                let at = span.offset() + lead;
                let _ = at;
                self.err(
                    err.code.unwrap_or("dt::point"),
                    sub_span(span, err.start, err.end),
                    format!("can't read this point: {}", err.message),
                    "here",
                )
                .help = err
                    .help
                    .or_else(|| Some("each half is a length: `0.65sq`, `8mm`, `50%`".to_owned()));
                return None;
            }
        };
        let x = self.plain_names_only(&parsed, span, ctx)?;
        let names = |_: &str| None;
        if let Err(ke) = expect(&x, Want::Length, &names, true) {
            self.kind_error(&ke, span, "a point's half");
            return None;
        }
        match lower(&x, Base::Squares, self.known.units(), Some(extent)) {
            Ok(l) => Some(fold(l)),
            Err(ke) => {
                self.kind_error(&ke, span, "a point's half");
                None
            }
        }
    }

    /// A point is worked out once, where it is declared: it may read plain
    /// names (a `let` of the tile, its value in place), never a `$` name an
    /// element sets while it builds.
    fn plain_names_only(&mut self, x: &Expr, span: SourceSpan, ctx: &Ctx) -> Option<Expr> {
        let mut bad: Option<(String, &'static str)> = None;
        let mut depth = 0;
        let mut cur = x.clone();
        loop {
            let mut again = false;
            cur = cur.map_names(&mut |n| {
                if n.starts_with('$') {
                    bad.get_or_insert((n.to_owned(), "dollar"));
                    return None;
                }
                match ctx.raw.iter().rev().find(|(k, _)| k == n) {
                    Some((_, e)) => {
                        again = true;
                        Some(e.clone())
                    }
                    None => {
                        bad.get_or_insert((n.to_owned(), "unknown"));
                        None
                    }
                }
            });
            depth += 1;
            if bad.is_some() || !again || depth > 16 {
                break;
            }
        }
        match bad {
            Some((n, "dollar")) => {
                self.err(
                    "dt::type",
                    span,
                    format!("`{n}` is worked out while an element builds, and a point is worked out once where it is written"),
                    "not in a point",
                )
                .help = Some(
                    "a point takes plain names (a `let` of the tile), units, `$tile` and `parent()`; `$x $y $d $a $bottom` are for tops".to_owned(),
                );
                None
            }
            Some((n, _)) => {
                let names: Vec<&str> = ctx.raw.iter().map(|(k, _)| k.as_str()).collect();
                let help = crate::schema::did_you_mean(&n, names.iter().copied())
                    .map(|m| format!("did you mean `{m}`?"))
                    .unwrap_or_else(|| {
                        "a point takes the tile's own `let`s (declared above it), units and numbers"
                            .to_owned()
                    });
                self.err(
                    "dt::unknown-name",
                    span,
                    format!("no `let` is called `{n}` here"),
                    "unknown name",
                )
                .help = Some(help);
                None
            }
            None => Some(cur),
        }
    }

    /// `point(frame, x, y)` and `polar(frame, d, a)`.
    fn point_call(
        &mut self,
        t: &str,
        _outer: SourceSpan,
        span: SourceSpan,
        (cols, rows): (f64, f64),
        ctx: &Ctx,
    ) -> Option<Point> {
        let parsed = match expr::parse_v2_units(t) {
            Ok(x) => x,
            Err(err) => {
                self.err(
                    err.code.unwrap_or("dt::point"),
                    sub_span(span, err.start, err.end),
                    format!("can't read this point: {}", err.message),
                    "here",
                )
                .help = err.help;
                return None;
            }
        };
        let Expr::Call { name, args } = &parsed else {
            self.err("dt::point", span, "a point is one call", "here");
            return None;
        };
        if args.len() != 3 {
            let help = if name == "point" {
                "`point($tile, 50%, 2sq)`: the frame, then across and down"
            } else {
                "`polar($tile, 40%, 90deg)`: the frame, how far out, the angle round"
            };
            self.err(
                "dt::point",
                span,
                format!("`{name}` takes 3 values, not {}", args.len()),
                "here",
            )
            .help = Some(help.to_owned());
            return None;
        }
        // the frame: only a region whose box is known where it is written
        self.frame_is_tile(&args[0], span, ctx)?;
        // lengths in the frame's own size, as a tuple's halves are
        let half = |this: &mut Self, e: &Expr, extent: f64| -> Option<Expr> {
            let x = this.plain_names_only(e, span, ctx)?;
            let names = |_: &str| None;
            if let Err(ke) = expect(&x, Want::Length, &names, true) {
                this.kind_error(&ke, span, &format!("`{name}`"));
                return None;
            }
            match lower(&x, Base::Squares, this.known.units(), Some(extent)) {
                Ok(l) => Some(fold(l)),
                Err(ke) => {
                    this.kind_error(&ke, span, &format!("`{name}`"));
                    None
                }
            }
        };
        if name == "point" {
            let x = half(self, &args[1], cols)?;
            let y = half(self, &args[2], rows)?;
            return Some(Point::Pair {
                x,
                y: north_west(rows, y),
            });
        }
        // polar: out from the middle by a share of the way to the nearest
        // edge, at an angle round it (0 east, turning towards the south, the
        // way the page reads: a quarter turn is straight down)
        let d = self.plain_names_only(&args[1], span, ctx)?;
        let a = self.plain_names_only(&args[2], span, ctx)?;
        let names = |_: &str| None;
        if let Err(ke) = expect(&d, Want::Plain, &names, false) {
            self.kind_error(&ke, span, "`polar`");
            return None;
        }
        if let Err(ke) = expect(&a, Want::Angle, &names, false) {
            self.kind_error(&ke, span, "`polar`");
            return None;
        }
        let units = self.known.units();
        let value = |this: &mut Self, e: &Expr| -> Option<f64> {
            let l = lower(e, Base::Dmm, units, None).ok()?;
            match l.sample(&|_| None, 0.0) {
                Ok(v) if !l.is_range() => Some(v),
                _ => {
                    this.err(
                        "dt::point",
                        span,
                        "`polar` is one place: its distance and angle are single values",
                        "not one value",
                    );
                    None
                }
            }
        };
        let (d, a) = (value(self, &d)?, value(self, &a)?);
        let r = d * 0.5 * cols.min(rows);
        let (cx, cy_nw) = (0.5 * cols, 0.5 * rows);
        let (x, y_nw) = (cx + r * expr::cos_deg(a), cy_nw + r * expr::sin_deg(a));
        Some(Point::Pair {
            x: Expr::num(x),
            y: Expr::num(rows - y_nw),
        })
    }

    /// A point's frame must be a region whose box is known where the point
    /// is written: `$tile`, or `$region` where the declaration sits in the
    /// tile itself.
    fn frame_is_tile(&mut self, frame: &Expr, span: SourceSpan, ctx: &Ctx) -> Option<()> {
        let here = if ctx.in_tile && self.frame_region.is_none() {
            "the tile"
        } else {
            ""
        };
        match frame {
            Expr::Name(n) if n == "$tile" => Some(()),
            Expr::Name(n) if n == "$region" && !here.is_empty() => Some(()),
            Expr::Name(n) if n == "$region" => {
                self.err(
                    "dt::point",
                    span,
                    "`$region` here is a region whose box is known only when the tile is built",
                    "not the tile",
                )
                .help = Some(
                    "write the frame as `$tile`: `point($tile, 50%, 50%)`; a point in a named region's own frame is not available yet"
                        .to_owned(),
                );
                None
            }
            Expr::Call { name, args } if name == "parent" && args.len() == 1 => {
                self.err(
                    "dt::point",
                    span,
                    "the frame is the tile here, and the tile has no parent",
                    "no parent",
                )
                .help = Some(
                    "a point at tile level is in `$tile`; `parent(…)` is for a thing inside a region"
                        .to_owned(),
                );
                None
            }
            other => {
                let names = |n: &str| self.kind_of_name(ctx, n);
                let why = match kind_of(other, &names, false) {
                    Ok(Kind::Region) => {
                        "that region's box is not known where the point is written".to_owned()
                    }
                    Ok(k) => format!("a frame is a region, not {}", k.describe()),
                    Err(ke) => ke.message,
                };
                self.err(
                    "dt::point",
                    span,
                    format!("`{other}`: {why}"),
                    "not a frame",
                )
                .help = Some("the frame is the first value: `$tile`".to_owned());
                None
            }
        }
    }
}

/// `rows - y`: a north-west `y` in the geometry's own frame.  A number is
/// worked out here, exactly as the mirror image written by hand would be.
fn north_west(rows: f64, y: Expr) -> Expr {
    fold(Expr::Bin {
        op: BinOp::Sub,
        lhs: Box::new(Expr::num(rows)),
        rhs: Box::new(y),
    })
}

/// A sum or product of plain numbers, worked out: the same operations in the
/// same order, so no float differs.
fn fold(e: Expr) -> Expr {
    match &e {
        Expr::Bin { op, lhs, rhs } => {
            let (a, b) = (fold(*lhs.clone()), fold(*rhs.clone()));
            if let (
                Expr::Num {
                    value: x,
                    percent: false,
                },
                Expr::Num {
                    value: y,
                    percent: false,
                },
            ) = (&a, &b)
            {
                return Expr::num(match op {
                    BinOp::Add => x + y,
                    BinOp::Sub => x - y,
                    BinOp::Mul => x * y,
                    BinOp::Div => x / y,
                });
            }
            Expr::Bin {
                op: *op,
                lhs: Box::new(a),
                rhs: Box::new(b),
            }
        }
        Expr::Neg(a) => match fold(*a.clone()) {
            Expr::Num {
                value,
                percent: false,
            } => Expr::num(-value),
            other => Expr::Neg(Box::new(other)),
        },
        _ => e,
    }
}
