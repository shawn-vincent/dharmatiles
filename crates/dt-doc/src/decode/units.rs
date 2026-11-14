//! Format 1's numbers: kinds checked and units lowered where a value is
//! read, and points in parentheses.
//!
//! Everything downstream reads reference numbers (lengths in design
//! millimetres, angles in degrees, points in squares from the south-west
//! corner); the language's side of the boundary is here.

use miette::SourceSpan;

use super::{Ctx, Decoder, sub_span};
use crate::dim::Dim;
use crate::expr::{self, Expr};
use crate::framed::{Framed, FramedAt, fold, north_west};
use crate::place::{Corner, Edge, Point};
use crate::schema::Axis;
use crate::units::{Base, Kind, KindError, Want, expect, lower};

/// What a control's axis wants of a value.
pub(crate) fn want_of(axis: Axis) -> Want {
    match axis {
        Axis::Dmm => Want::Length,
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
    /// the frame the declaration sits in.  Against the tile it is returned in
    /// squares from the south-west corner, as geometry keeps it: across is
    /// `x`, up is `rows - y`.  Against a named region (a pin of a body that
    /// acts in one, `parent(...)`, any point in a part, which has no tile)
    /// it is kept as written, [`Point::Framed`], and worked out per call site
    /// or once `check` has the region tree: the decoder runs before the tree
    /// exists, so a point records what it said rather than the reader
    /// guessing a box (the same way a formula's `parent(...)` is recorded
    /// for `check`).
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
        let size = ctx.size.map(|(c, r)| (f64::from(c), f64::from(r)));
        let call = t.starts_with("point(") || t.starts_with("polar(");
        if call && t.ends_with(')') {
            return self.point_call(t, inner(0, t.len()), size, ctx);
        }
        if t.starts_with('(') && crate::place::encloses(t) {
            let body = &t[1..t.len() - 1];
            let commas = crate::place::top_commas(body);
            let [comma] = commas[..] else {
                self.err(
                    dt_lang::codes::POINT,
                    inner(0, t.len()),
                    format!(
                        "a point has two halves, across and down: `{t}` has {}",
                        commas.len() + 1
                    ),
                    "not a pair",
                )
                .help = Some(
                    "write \"(0.65sq, 0.5sq)\": across from the west edge, down from the north"
                        .to_owned(),
                );
                return None;
            };
            let halves = [(&body[..comma], 1), (&body[comma + 1..], comma + 2)];
            let mut out = Vec::new();
            for (text, off) in &halves {
                out.push(self.half_expr(text, inner(*off, off + text.len()), ctx)?);
            }
            let y = out.pop()?;
            let x = out.pop()?;
            // the frame is the region the declaration sits in
            let here = Expr::Name("$region".to_owned());
            return self.framed_xy(&here, x, y, span, size);
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
            _ => "write \"(0.65sq, 0.5sq)\": parentheses, across then down, each with its unit; or `point($tile, 50%, 0)`".to_owned(),
        };
        self.err(
            dt_lang::codes::POINT,
            inner(0, t.len()),
            format!("a point is written in parentheses: `{t}` has none"),
            "no parentheses",
        )
        .help = Some(help);
        None
    }

    /// One half of a point as written: a length (a `%` is a share of the
    /// frame's size across or down), kinds checked, plain names in place.
    fn half_expr(&mut self, text: &str, span: SourceSpan, ctx: &Ctx) -> Option<Expr> {
        let parsed = match expr::parse_units(text) {
            Ok(x) => x,
            Err(err) => {
                self.err(
                    err.code.unwrap_or(dt_lang::codes::POINT),
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
        self.length_half(&parsed, span, ctx, "a point's half")
    }

    /// A half already parsed: kind-checked as a length, plain names put in.
    fn length_half(
        &mut self,
        parsed: &Expr,
        span: SourceSpan,
        ctx: &Ctx,
        what: &str,
    ) -> Option<Expr> {
        let x = self.plain_names_only(parsed, span, ctx)?;
        let names = |_: &str| None;
        if let Err(ke) = expect(&x, Want::Length, &names, true) {
            self.kind_error(&ke, span, what);
            return None;
        }
        // a unit the target does not have is found here, not at check
        if let Err(ke) = lower(&x, Base::Squares, self.known.units(), Some(1.0)) {
            self.kind_error(&ke, span, what);
            return None;
        }
        Some(x)
    }

    /// A point's halves in `frame`: against the tile worked out now, else
    /// kept as written.
    fn framed_xy(
        &mut self,
        frame: &Expr,
        x: Expr,
        y: Expr,
        span: SourceSpan,
        size: Option<(f64, f64)>,
    ) -> Option<Point> {
        match (self.frame_kind(frame, span, size)?, size) {
            (FrameKind::Tile, Some((cols, rows))) => {
                let units = self.known.units();
                let x = lower(&x, Base::Squares, units, Some(cols)).ok()?;
                let y = lower(&y, Base::Squares, units, Some(rows)).ok()?;
                Some(Point::Pair {
                    x: fold(x),
                    y: north_west(rows, y),
                })
            }
            _ => Some(Point::Framed(Box::new(Framed {
                frame: frame.clone(),
                at: FramedAt::Xy { x, y },
            }))),
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
                    dt_lang::codes::TYPE,
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
                let near = crate::schema::did_you_mean(&n, names.iter().copied());
                let at = self.word_span(span, &n);
                self.err(
                    dt_lang::codes::UNKNOWN_NAME,
                    span,
                    format!("no `let` is called `{n}` here"),
                    "unknown name",
                )
                .suggest_or(at, near, || {
                    "a point takes the tile's own `let`s (declared above it), units and numbers"
                        .to_owned()
                });
                None
            }
            None => Some(cur),
        }
    }

    /// `point(frame, x, y)` and `polar(frame, d, a)`.
    fn point_call(
        &mut self,
        t: &str,
        span: SourceSpan,
        size: Option<(f64, f64)>,
        ctx: &Ctx,
    ) -> Option<Point> {
        let parsed = match expr::parse_v2_units(t) {
            Ok(x) => x,
            Err(err) => {
                self.err(
                    err.code.unwrap_or(dt_lang::codes::POINT),
                    sub_span(span, err.start, err.end),
                    format!("can't read this point: {}", err.message),
                    "here",
                )
                .help = err.help;
                return None;
            }
        };
        let Expr::Call { name, args } = &parsed else {
            self.err(dt_lang::codes::POINT, span, "a point is one call", "here");
            return None;
        };
        if args.len() != 3 {
            let help = if name == "point" {
                "`point($tile, 50%, 2sq)`: the frame, then across and down"
            } else {
                "`polar($tile, 40%, 90deg)`: the frame, how far out, the angle round"
            };
            self.err(
                dt_lang::codes::POINT,
                span,
                format!("`{name}` takes 3 values, not {}", args.len()),
                "here",
            )
            .help = Some(help.to_owned());
            return None;
        }
        let what = format!("`{name}`");
        if name == "point" {
            let x = self.length_half(&args[1], span, ctx, &what)?;
            let y = self.length_half(&args[2], span, ctx, &what)?;
            return self.framed_xy(&args[0], x, y, span, size);
        }
        // polar: out from the deepest point by a share of the depth, at an
        // angle round it (0 east, turning towards the south, the way the page
        // reads: a quarter turn is straight down)
        let frame = self.frame_kind(&args[0], span, size)?;
        let d = self.plain_names_only(&args[1], span, ctx)?;
        let a = self.plain_names_only(&args[2], span, ctx)?;
        let names = |_: &str| None;
        if let Err(ke) = expect(&d, Want::Plain, &names, false) {
            self.kind_error(&ke, span, &what);
            return None;
        }
        if let Err(ke) = expect(&a, Want::Angle, &names, false) {
            self.kind_error(&ke, span, &what);
            return None;
        }
        let units = self.known.units();
        let value = |this: &mut Self, e: &Expr| -> Option<f64> {
            let l = lower(e, Base::Dmm, units, None).ok()?;
            match l.sample(&|_| None, 0.0) {
                Ok(v) if !l.is_range() => Some(v),
                _ => {
                    this.err(
                        dt_lang::codes::POINT,
                        span,
                        "`polar` is one place: its distance and angle are single values",
                        "not one value",
                    );
                    None
                }
            }
        };
        let (d, a) = (value(self, &d)?, value(self, &a)?);
        match (frame, size) {
            (FrameKind::Tile, Some((cols, rows))) => {
                let r = d * 0.5 * cols.min(rows);
                let (cx, cy_nw) = (0.5 * cols, 0.5 * rows);
                let (x, y_nw) = (cx + r * expr::cos_deg(a), cy_nw + r * expr::sin_deg(a));
                Some(Point::Pair {
                    x: Expr::num(x),
                    y: Expr::num(rows - y_nw),
                })
            }
            _ => Some(Point::Framed(Box::new(Framed {
                frame: args[0].clone(),
                at: FramedAt::Polar { d, a_deg: a },
            }))),
        }
    }

    /// A point's frame must be a region: `$tile`, `$region` (the one the
    /// declaration sits in) or `parent(…)` of one.  Against the tile it is
    /// worked out now; against anything else, or in a part (which has no tile
    /// yet), later.
    fn frame_kind(
        &mut self,
        frame: &Expr,
        span: SourceSpan,
        size: Option<(f64, f64)>,
    ) -> Option<FrameKind> {
        let at_tile = size.is_some() && self.frame_region.is_none();
        let known = size.is_some();
        match frame {
            Expr::Name(n) if n == "$tile" => Some(if known {
                FrameKind::Tile
            } else {
                FrameKind::Later
            }),
            Expr::Name(n) if n == "$region" => Some(if at_tile {
                FrameKind::Tile
            } else {
                FrameKind::Later
            }),
            Expr::Call { name, args } if name == "parent" && args.len() == 1 => {
                // the chain of parents ends at `$tile` or `$region`
                let mut inner = &args[0];
                while let Expr::Call { name, args } = inner
                    && name == "parent"
                    && args.len() == 1
                {
                    inner = &args[0];
                }
                let tile_itself = matches!(inner, Expr::Name(n) if n == "$tile")
                    || (at_tile && matches!(inner, Expr::Name(n) if n == "$region"));
                if tile_itself {
                    self.err(
                        dt_lang::codes::POINT,
                        span,
                        "the frame is the tile here, and the tile has no parent",
                        "no parent",
                    )
                    .help = Some(
                        "a point at tile level is in `$tile`; `parent(…)` is for a thing inside a region"
                            .to_owned(),
                    );
                    return None;
                }
                if !matches!(inner, Expr::Name(n) if n == "$region" || n == "$tile") {
                    return self.not_a_frame(frame, span);
                }
                Some(FrameKind::Later)
            }
            other => self.not_a_frame(other, span),
        }
    }

    fn not_a_frame(&mut self, frame: &Expr, span: SourceSpan) -> Option<FrameKind> {
        self.err(
            dt_lang::codes::POINT,
            span,
            format!("`{frame}` is not a frame"),
            "not a frame",
        )
        .help = Some(
            "the frame is the first value: `$tile`, `$region` or `parent($region)`".to_owned(),
        );
        None
    }
}

/// Whether a point's frame is known where it is written.
enum FrameKind {
    /// The tile, whose size is known: the point is worked out now.
    Tile,
    /// A region (or a part's tile): worked out per call site, or by `check`.
    Later,
}

/// A point as written in a format 1 document (`"(0.5sq, 25%)"`,
/// `point($tile, …)`, a corner or edge word), read against a tile of `size`
/// squares: the program's own reading, for what shows a written point (the
/// editor's map) rather than builds it.  The point comes back in squares
/// from the south-west corner, as geometry keeps it.  A `let`'s name is not
/// known to this reading.
///
/// # Errors
/// In words: what is wrong with the point.
pub fn point_text(
    text: &str,
    size: (u32, u32),
    units: crate::units::Units,
) -> Result<Point, String> {
    let known = crate::known::Known::new().with_units(units);
    let src = std::sync::Arc::new(miette::NamedSource::new("point", text.to_owned()));
    let mut d = Decoder::new(src, &known);
    let ctx = Ctx {
        size: Some(size),
        in_tile: true,
        ..Ctx::default()
    };
    let span = SourceSpan::new(0.into(), text.len().max(1));
    match d.point1(text, span, 0, &ctx) {
        Some(p) => Ok(p),
        None => Err(d
            .errors
            .first()
            .map_or_else(|| "not a point".to_owned(), |e| e.message.clone())),
    }
}
