//! The heights language: `let` and `override`, `point`, `top=`, `pin`,
//! `slope`, `base`, places by name, and the checks that need no grid
//! (names, types and order).  Today's elements are decoded as before; this
//! reads the words they now also take.

use kdl::{KdlEntry, KdlNode, KdlValue};

use super::entry::Parent;
use super::{Ctx, Decoder, arg_str, sub_span, value_span};
use crate::dim::{Dim, dim_of};
use crate::expr::{self, Expr, FUNCTIONS};
use crate::model::{Base, Callee, Const, ConstKind, Entry, Pin, PlaceRef, PointDecl, Spanned};
use crate::place::{self, BUILTIN_LINES, BUILTIN_POINTS, BUILTIN_REGION, Edge};
use crate::schema::{self, Role, did_you_mean};

/// Help for a format 1 word in a format 0 document.
pub(crate) const NOT_FORMAT_1: &str = "format 1 is in development (read with DT_FORMAT_PREVIEW=1): start the file with `dharmatiles version=1` to use it";

/// What a place name stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaceKind {
    Point,
    Line,
    Region,
}

impl PlaceKind {
    pub(crate) fn describe(self) -> &'static str {
        match self {
            PlaceKind::Point => "a point",
            PlaceKind::Line => "a line",
            PlaceKind::Region => "a region",
        }
    }
}

/// Where an expression is written, which fixes the names it may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// A `let`: anything, `x`, `y` and `bottom` included (the tile's use
    /// decides).
    Const,
    /// A top: a height.
    Top,
    /// A thing's top: a height drawn for each thing (a tree's), so a
    /// weighted range has odds to act on.
    Draw,
    /// A length (`within`): constants and numbers only.
    Length,
}

/// What a stack entry is, for where it may stand and what it may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    /// Dirt, water, cliff, flagstones, planks: a band and its pins.
    Body,
    /// Grass, scatter, ground, a coat: fills a region.
    Area,
    /// A rock, tree, flower, wall, reeds: stands somewhere.
    Thing,
}

/// Reserved: the names an expression already means.
fn reserved(name: &str) -> bool {
    matches!(name, "x" | "y" | "bottom" | BUILTIN_REGION)
        || FUNCTIONS.iter().any(|(f, _)| *f == name)
        || BUILTIN_POINTS.contains(&name)
        || BUILTIN_LINES.contains(&name)
}

/// `long-path` → `longPath`.
fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in name.chars() {
        if c == '-' || c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(|c| c.is_ascii_alphabetic())
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The built-in places, as a context's starting list.
pub(crate) fn builtin_places() -> Vec<(String, PlaceKind)> {
    let mut v: Vec<(String, PlaceKind)> = BUILTIN_POINTS
        .iter()
        .map(|n| ((*n).to_owned(), PlaceKind::Point))
        .collect();
    v.extend(
        BUILTIN_LINES
            .iter()
            .map(|n| ((*n).to_owned(), PlaceKind::Line)),
    );
    v.push((BUILTIN_REGION.to_owned(), PlaceKind::Region));
    v
}

impl Decoder<'_> {
    /// The library's constants as a context's starting list.
    pub(crate) fn library_consts(&self) -> Vec<(String, Dim)> {
        // in format 1 the library's heights are `$` names, installed at the root
        let dollar = if self.format1 { "$" } else { "" };
        self.known
            .consts()
            .iter()
            // format 1's library heights are lengths, whatever format 0 let
            // a plain number stand for
            .map(|c| {
                // a library written in format 1 has its `$` already
                let name = if c.name.value.starts_with('$') {
                    c.name.value.clone()
                } else {
                    format!("{dollar}{}", c.name.value)
                };
                (name, if self.format1 { Dim::Mm } else { c.dim })
            })
            .collect()
    }

    fn name_dim(ctx: &Ctx, n: &str) -> Option<Dim> {
        match n {
            "x" | "y" | "$x" | "$y" => Some(Dim::Squares),
            "bottom" | "$bottom" => Some(Dim::Mm),
            "$region" | "$tile" => Some(Dim::Region),
            _ => ctx
                .consts
                .iter()
                .rev()
                .find(|(k, _)| k == n)
                .map(|(_, d)| *d),
        }
    }

    /// A height or length: a KDL number, or a quoted expression of the
    /// heights language, its names and measures checked.
    pub(crate) fn height_expr(
        &mut self,
        e: &KdlEntry,
        ctx: &Ctx,
        what: &str,
        mode: Mode,
    ) -> Option<(Spanned<Expr>, Dim)> {
        let span = value_span(e);
        if let Some(t) = e.ty() {
            if self.format1 {
                self.err(
                    "dt::unit",
                    t.span(),
                    format!("`({})` is not written in format 1", t.value()),
                    "an annotation",
                )
                .help =
                    Some("write the unit after the number, in a string: `\"4.5dmm\"`".to_owned());
                return None;
            }
            self.err(
                "dt::unit",
                t.span(),
                format!("`({})` is not written: {what} is in mm", t.value()),
                "not written",
            )
            .help = Some("units are not written; every height is mm above zero".to_owned());
            return None;
        }
        let expr = match e.value() {
            KdlValue::Integer(i) => Expr::num(i.to_string().parse().unwrap_or(f64::NAN)),
            KdlValue::Float(f) => Expr::num(*f),
            KdlValue::String(s) => match if self.format1 {
                expr::parse_v2_units(s)
            } else {
                expr::parse_v2(s)
            } {
                Ok(x) => x,
                Err(err) => {
                    let at = self.inner_at(e, s);
                    let help = err.help.clone().unwrap_or_else(|| {
                        "a height is mm above zero: a number, a name such as groundHeight, a range [4 .. 6], and + - * /"
                            .to_owned()
                    });
                    self.err(
                        err.code.unwrap_or("dt::value"),
                        sub_span(span, at + err.start, at + err.end),
                        format!("can't read {what}: {}", err.message),
                        "here",
                    )
                    .help = Some(help);
                    return None;
                }
            },
            _ => {
                self.err(
                    "dt::value",
                    span,
                    format!("{what} is a number or an expression"),
                    "not a number",
                );
                return None;
            }
        };
        if !self.check_name_forms(&expr, e, span) {
            return None;
        }
        // A top, a pin and a distance are bands the solver fits, not draws:
        // the odds of a weighting have nothing to act on.
        if !matches!(mode, Mode::Const | Mode::Draw) && expr.weighted().is_some() {
            self.err(
                "dt::weighting",
                span,
                "a weighted range does not go on a band",
                format!("{what} is a band the solver fits, not a draw"),
            )
            .help = Some(format!("write the range alone: {}", expr.unweighted_text()));
            return None;
        }
        for n in expr.names() {
            let special = matches!(
                n,
                "x" | "y" | "bottom" | "$x" | "$y" | "$bottom" | "$d" | "$a" | "$_y"
            );
            if special && mode == Mode::Length {
                self.err(
                    "dt::type",
                    span,
                    format!("{what} is a length: `{n}` is not known here"),
                    "not a length",
                )
                .help = Some("a length is a number or a constant".to_owned());
                return None;
            }
            // a `$` name is read from the context of whoever calls: `check`
            // finds out where it is installed, call site by call site
            if self.format1 && n.starts_with('$') {
                continue;
            }
            if !special && Self::name_dim(ctx, n).is_none() {
                let names: Vec<&str> = ctx.consts.iter().map(|(k, _)| k.as_str()).collect();
                let help = did_you_mean(n, names.iter().copied())
                    .map(|m| format!("did you mean `{m}`?"))
                    .unwrap_or_else(|| {
                        format!(
                            "declare it first: `let {n} 5`, or use one of the library's heights"
                        )
                    });
                self.err(
                    "dt::unknown-name",
                    span,
                    format!("no height called `{n}`"),
                    "unknown name",
                )
                .help = Some(help);
                return None;
            }
        }
        if self.format1 {
            return self.height_v1(expr, ctx, what, mode, span);
        }
        let dollar_dim = |n: &str| {
            Self::name_dim(ctx, n)
                .or_else(|| (self.format1 && n.starts_with('$')).then_some(Dim::Scalar))
        };
        match dim_of(&expr, &dollar_dim) {
            Err(de) => {
                self.err("dt::type", span, format!("{what}: {}", de.message), "here")
                    .help = de.help;
                None
            }
            Ok(Dim::Squares) if mode != Mode::Const => {
                self.err(
                    "dt::type",
                    span,
                    format!("{what} is a height in mm, this is squares (x or y)"),
                    "squares",
                )
                .help = Some(
                    "scale x or y inside sin or cos, as in waterSurfaceHeight + 0.6 * cos(720 * x)"
                        .to_owned(),
                );
                None
            }
            Ok(Dim::Region) if mode != Mode::Const => {
                self.err(
                    "dt::type",
                    span,
                    format!("{what} is a number, and this is a region"),
                    "a region",
                )
                .help = Some("a region goes to `parent(…)`; nothing else reads one yet".to_owned());
                None
            }
            Ok(d) => Some((Spanned { value: expr, span }, d)),
        }
    }

    /// A height or length of format 1: its kind checked (a top is a length,
    /// a `let` holds any), its units lowered to design millimetres and
    /// degrees.  The value as written is kept in `last_raw` for the points
    /// that read a `let`.
    fn height_v1(
        &mut self,
        expr: Expr,
        ctx: &Ctx,
        what: &str,
        mode: Mode,
        span: miette::SourceSpan,
    ) -> Option<(Spanned<Expr>, Dim)> {
        use crate::units::{Kind, Want};
        let want = if mode == Mode::Const {
            Want::Any
        } else {
            Want::Length
        };
        let (lowered, kind) = self.check_lower(&expr, want, ctx, span, what)?;
        if kind == Kind::Point {
            self.err(
                "dt::type",
                span,
                format!("{what} is a number, and this is a point"),
                "a point",
            )
            .help =
                Some("a point is written where a place goes: `fill`, `through`, `at=`".to_owned());
            return None;
        }
        self.last_raw = Some(expr);
        let dim = super::units::dim_of_kind(kind);
        Some((
            Spanned {
                value: lowered,
                span,
            },
            dim,
        ))
    }

    /// Where `name` is written inside the quoted value of `e`.
    pub(crate) fn name_span(&self, e: &KdlEntry, name: &str) -> miette::SourceSpan {
        let span = value_span(e);
        let Some(s) = e.value().as_string() else {
            return span;
        };
        let at = self.inner_at(e, s);
        let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
        let mut from = 0;
        while let Some(i) = s[from..].find(name) {
            let (a, b) = (from + i, from + i + name.len());
            let before = s[..a].chars().next_back().is_none_or(|c| !is_ident(c));
            let after = s[b..].chars().next().is_none_or(|c| !is_ident(c));
            if before && after {
                return sub_span(span, at + a, at + b);
            }
            from = b;
        }
        span
    }

    /// The forms a name takes in the document's format: `$` names are
    /// format 1's, and a bare `x`, `y` or `bottom` is format 0's.
    pub(crate) fn check_name_forms(
        &mut self,
        expr: &Expr,
        e: &KdlEntry,
        span: miette::SourceSpan,
    ) -> bool {
        let mut ok = true;
        for n in expr.names() {
            let at = if matches!(n, "x" | "y" | "bottom") || n.starts_with('$') {
                self.name_span(e, n)
            } else {
                span
            };
            if self.format1 {
                match n {
                    "x" | "y" | "bottom" => {
                        ok = false;
                        self.err(
                            "dt::name",
                            at,
                            format!("`{n}` is written `${n}` in format 1"),
                            "a position or height the element sets",
                        )
                        .help = Some(format!(
                            "`${n}` is read from the context: the element working out the height at each point sets it"
                        ));
                    }
                    _ => {}
                }
            } else if n.starts_with('$') {
                ok = false;
                self.err(
                    "dt::name",
                    at,
                    format!("`{n}` is a format 1 name; this document is format 0"),
                    "a context name",
                )
                .help = Some(NOT_FORMAT_1.to_owned());
            }
        }
        ok
    }

    /// `x`, `y`: a place named or written: a built-in or declared name, or
    /// a literal point `"(0.3, 0.7)"`.
    pub(crate) fn place_value(
        &mut self,
        e: &KdlEntry,
        ctx: &Ctx,
        what: &str,
    ) -> Option<(Spanned<PlaceRef>, PlaceKind)> {
        let span = value_span(e);
        let Some(s) = e.value().as_string() else {
            self.err(
                "dt::place",
                span,
                format!("`{what}` is a place: a name or \"(x, y)\""),
                "not a place",
            );
            return None;
        };
        let t = s.trim();
        let format1_point = self.format1
            && (t.starts_with('(') || t.starts_with("point(") || t.starts_with("polar("));
        // format 1: a bare pair is a mistake worth its own help
        if self.format1
            && !format1_point
            && t.chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '-')
        {
            let at = self.inner_at(e, s);
            self.point1(s, span, at, ctx)?;
            return None;
        }
        if t.starts_with('(') || format1_point {
            let p = if self.format1 {
                let at = self.inner_at(e, s);
                self.point1(s, span, at, ctx)?
            } else {
                match place::parse_point(t) {
                    Ok(p) => p,
                    Err(err) => {
                        let at = self.inner_at(e, s);
                        self.err(
                            "dt::place",
                            sub_span(span, at + err.start, at + err.end),
                            format!("can't read `{what}`: {}", err.message),
                            "here",
                        )
                        .help = Some(
                            "write a point as \"(x, y)\" in squares from the south-west corner"
                                .to_owned(),
                        );
                        return None;
                    }
                }
            };
            if let place::Point::Framed(f) = p {
                if what == "point" || what == "of" {
                    self.err(
                        "dt::point",
                        span,
                        format!(
                            "a `{what}` is worked out once, in the tile: this is in the frame of a region"
                        ),
                        "in a region's frame",
                    )
                    .help = Some(format!(
                        "write it against the tile: `{what} \"(0.5sq, 0.5sq)\"` or `point($tile, …)`"
                    ));
                    return None;
                }
                return Some((
                    Spanned {
                        value: PlaceRef::Framed(f),
                        span,
                    },
                    PlaceKind::Point,
                ));
            }
            let none = |_: &str| None;
            let xy = match &p {
                place::Point::Pair { x, y } if !x.is_range() && !y.is_range() => {
                    match (x.sample(&none, 0.0), y.sample(&none, 0.0)) {
                        (Ok(x), Ok(y)) => Some((x, y)),
                        _ => None,
                    }
                }
                _ => None,
            };
            let Some((x, y)) = xy else {
                self.err(
                    "dt::place",
                    span,
                    format!("`{what}` is one point: two numbers, no range"),
                    "not a point",
                );
                return None;
            };
            if let Some((c, r)) = ctx.size
                && (x < 0.0 || y < 0.0 || x > f64::from(c) || y > f64::from(r))
            {
                let (shown, help) = if self.format1 {
                    (
                        t.to_owned(),
                        format!(
                            "this tile is {c}x{r} squares: across runs 0 to {c}, down from the north 0 to {r}"
                        ),
                    )
                } else {
                    (
                        format!("({x}, {y})"),
                        format!("this tile is {c}x{r}: x runs 0 to {c}, y 0 to {r}"),
                    )
                };
                self.err(
                    "dt::point",
                    span,
                    format!("`{shown}` is off the tile"),
                    "outside the tile",
                )
                .help = Some(help);
                return None;
            }
            return Some((
                Spanned {
                    value: PlaceRef::Point(x, y),
                    span,
                },
                PlaceKind::Point,
            ));
        }
        match ctx.places.iter().find(|(n, _)| n == t) {
            Some((n, k)) => Some((
                Spanned {
                    value: PlaceRef::Name(n.clone()),
                    span,
                },
                *k,
            )),
            None => {
                let mut names: Vec<&str> = ctx.places.iter().map(|(k, _)| k.as_str()).collect();
                names.extend(Edge::ALL.iter().map(|e| e.as_str()));
                let help = did_you_mean(t, names.iter().copied())
                    .map(|m| format!("did you mean `{m}`?"))
                    .unwrap_or_else(|| {
                        format!(
                            "declare it at the top of the tile (`point`, `line` or `region`), or write a point \"(x, y)\"; built in: {}",
                            BUILTIN_POINTS.join(", ")
                        )
                    });
                self.err(
                    "dt::place",
                    span,
                    format!("no place is called `{t}`"),
                    "unknown place",
                )
                .help = Some(help);
                None
            }
        }
    }

    /// A declared name, camelCase and not already taken by anything.
    fn declared_name(&mut self, name: &Spanned<String>) -> bool {
        let n = name.value.as_str();
        if !is_ident(n) {
            let help = if n.contains('-') || n.contains('_') {
                format!(
                    "names are camelCase, since a hyphen subtracts: `{}`",
                    camel(n)
                )
            } else {
                "a name is letters and digits, starting with a letter".to_owned()
            };
            self.err(
                "dt::name",
                name.span,
                format!("`{n}` is not a name"),
                "not camelCase",
            )
            .help = Some(help);
            return false;
        }
        if reserved(n) || schema::element(n).is_some() {
            self.err(
                "dt::name",
                name.span,
                format!("`{n}` is already a word of the language"),
                "taken",
            )
            .help = Some("built-in places and element names cannot be reused or overridden; pick another name".to_owned());
            return false;
        }
        true
    }

    /// A name for a `let` or `override` in format 1: a `$` name, or a plain
    /// camelCase one that is not a word of the language.
    fn declared_name_v1(&mut self, name: &Spanned<String>, kind: ConstKind) -> bool {
        let n = name.value.as_str();
        if let Some(rest) = n.strip_prefix('$') {
            if !is_ident(rest) {
                self.err(
                    "dt::name",
                    name.span,
                    format!("`{n}` is not a name"),
                    "not a `$` name",
                )
                .help =
                    Some("a `$` name is `$` and then letters and digits: `$treeLine`".to_owned());
                return false;
            }
            if crate::words::is_element_set(n) {
                self.err(
                    "dt::name",
                    name.span,
                    format!("`{n}` is set by the element that builds, so it can't be declared"),
                    "set by an element",
                )
                .help = Some(format!(
                    "read `{n}` where a height is worked out at each point (a top, a pin, a level); `let` and `override` can't change it"
                ));
                return false;
            }
            return true;
        }
        if kind == ConstKind::Override {
            self.err(
                "dt::name",
                name.span,
                format!("`override` replaces a `$` name, and `{n}` is a plain name"),
                "a plain name",
            )
            .help = Some(format!(
                "write `override ${n} …` to replace the context's `${n}`, or `let {n} …` for a plain name of this scope"
            ));
            return false;
        }
        if !is_ident(n) {
            let help = if n.contains('-') || n.contains('_') {
                format!(
                    "names are camelCase, since a hyphen subtracts: `{}`",
                    camel(n)
                )
            } else {
                "a name is letters and digits, starting with a letter".to_owned()
            };
            self.err(
                "dt::name",
                name.span,
                format!("`{n}` is not a name"),
                "not camelCase",
            )
            .help = Some(help);
            return false;
        }
        if crate::words::is_reserved(n) {
            self.err(
                "dt::name",
                name.span,
                format!("`{n}` is a word of the language"),
                crate::words::kind(n),
            )
            .help = Some(format!(
                "a plain name can't take a coordinate, function, weighting or unit; choose another name (a `$` name is a separate namespace, so `${n}` is allowed)"
            ));
            return false;
        }
        if reserved(n) || schema::element(n).is_some() {
            self.err(
                "dt::name",
                name.span,
                format!("`{n}` is already a word of the language"),
                "taken",
            )
            .help = Some("built-in places and element names cannot be reused or overridden; pick another name".to_owned());
            return false;
        }
        true
    }

    /// `let NAME VALUE` / `override NAME VALUE`.
    pub(crate) fn const_node(
        &mut self,
        n: &KdlNode,
        ctx: &mut Ctx,
        taken: &[String],
    ) -> Option<Const> {
        let kind = if n.name().value() == "override" {
            ConstKind::Override
        } else {
            ConstKind::Let
        };
        let kw = n.name().value().to_owned();
        let (name_e, value_e) = match n.entries() {
            [a, b] if a.name().is_none() && b.name().is_none() => (a, b),
            _ => {
                self.err(
                    "dt::value",
                    n.span(),
                    format!("`{kw}` is `{kw} name value`"),
                    "here",
                )
                .help = Some(format!("write `{kw} groundHeight 5`"));
                return None;
            }
        };
        let Some(s) = arg_str(name_e) else {
            self.err(
                "dt::name",
                name_e.span(),
                format!("`{kw}` needs a name"),
                "not a name",
            );
            return None;
        };
        let name = Spanned {
            value: s.to_owned(),
            span: value_span(name_e),
        };
        if self.format1 {
            if !self.declared_name_v1(&name, kind) {
                return None;
            }
            // a `$` name is the context's: whether it is there depends on
            // who calls, so `check` settles it where the scope is entered
            if !name.value.starts_with('$') && Self::name_dim(ctx, &name.value).is_some()
                || (!name.value.starts_with('$') && taken.contains(&name.value))
            {
                self.err(
                    "dt::name",
                    name.span,
                    format!("`{}` is already declared", name.value),
                    "a second time",
                )
                .help = Some(
                    "a plain name is declared once and never shadowed: choose another name"
                        .to_owned(),
                );
                return None;
            }
        } else if !self.declared_name(&name) {
            return None;
        }
        let exists = Self::name_dim(ctx, &name.value).is_some();
        match kind {
            _ if self.format1 => {}
            ConstKind::Let if exists || taken.contains(&name.value) => {
                let is_const = exists;
                self.err(
                    "dt::name",
                    name.span,
                    format!("`{}` is already declared", name.value),
                    "a second time",
                )
                .help = Some(if is_const {
                    format!(
                        "`override {} …` replaces it for this tile; or choose another name",
                        name.value
                    )
                } else {
                    "every name is declared once, of any kind: choose another name".to_owned()
                });
                return None;
            }
            ConstKind::Override if !exists => {
                let names: Vec<&str> = ctx.consts.iter().map(|(k, _)| k.as_str()).collect();
                let help = match did_you_mean(&name.value, names.iter().copied()) {
                    Some(m) => format!(
                        "did you mean `override {m} …`? or `let {} …` declares a new one",
                        name.value
                    ),
                    None if taken.contains(&name.value) => {
                        "only a constant can be overridden, not a point, line or region".to_owned()
                    }
                    None => format!(
                        "`let {} …` declares a new constant; `override` only replaces an existing one",
                        name.value
                    ),
                };
                self.err(
                    "dt::name",
                    name.span,
                    format!("there is nothing called `{}` to override", name.value),
                    "no such constant",
                )
                .help = Some(help);
                return None;
            }
            _ => {}
        }
        let (value, dim) =
            self.height_expr(value_e, ctx, &format!("`{}`", name.value), Mode::Const)?;
        match ctx.consts.iter_mut().find(|(k, _)| *k == name.value) {
            Some(slot) => slot.1 = dim,
            None => ctx.consts.push((name.value.clone(), dim)),
        }
        if self.format1
            && !name.value.starts_with('$')
            && let Some(raw) = self.last_raw.take()
        {
            ctx.raw.retain(|(k, _)| *k != name.value);
            ctx.raw.push((name.value.clone(), raw));
        }
        Some(Const {
            kind,
            name,
            value,
            dim,
            span: n.span(),
        })
    }

    /// `point NAME "(x, y)"`.
    pub(crate) fn point_node(&mut self, n: &KdlNode, ctx: &Ctx) -> Option<PointDecl> {
        let (name_e, value_e) = match n.entries() {
            [a, b] if a.name().is_none() && b.name().is_none() => (a, b),
            _ => {
                self.err(
                    "dt::value",
                    n.span(),
                    "`point` is `point name \"(x, y)\"`",
                    "here",
                )
                .help = Some("write `point knoll \"(0.28, 0.72)\"`, in squares".to_owned());
                return None;
            }
        };
        let s = arg_str(name_e)?;
        let name = Spanned {
            value: s.to_owned(),
            span: value_span(name_e),
        };
        if !self.declared_name(&name) {
            return None;
        }
        let (r, _) = self.place_value(value_e, ctx, "point")?;
        match r.value {
            PlaceRef::Point(x, y) => Some(PointDecl {
                name,
                x,
                y,
                span: n.span(),
            }),
            PlaceRef::Name(_) | PlaceRef::Framed(_) => {
                self.err(
                    "dt::place",
                    r.span,
                    "a point is written \"(x, y)\"",
                    "not a pair",
                )
                .help = Some("write `point knoll \"(0.28, 0.72)\"`".to_owned());
                None
            }
        }
    }

    /// Check that the names a tile declares are each declared once, and
    /// start the context's place list.
    pub(crate) fn declare_places(&mut self, body: &kdl::KdlDocument, ctx: &mut Ctx) -> Vec<String> {
        ctx.places = builtin_places();
        let mut taken: Vec<String> = Vec::new();
        for n in body.nodes() {
            let kind = match n.name().value() {
                "point" => PlaceKind::Point,
                "line" => PlaceKind::Line,
                "region" => PlaceKind::Region,
                _ => continue,
            };
            let Some(e) = n.entries().first().filter(|e| arg_str(e).is_some()) else {
                continue;
            };
            let name = arg_str(e).unwrap_or_default().to_owned();
            let span = value_span(e);
            let clash = if taken.contains(&name) {
                Some("another place already has this name")
            } else if reserved(&name) && kind != PlaceKind::Point {
                Some("a built-in place has this name")
            } else if !self.format1 && self.known.consts().iter().any(|c| c.name.value == name) {
                Some("a library constant has this name")
            } else {
                None
            };
            if let Some(why) = clash {
                self.err(
                    "dt::name",
                    span,
                    format!("`{name}` is already declared"),
                    why,
                )
                .help = Some(
                    "every name is declared once, of any kind: choose another name".to_owned(),
                );
                continue;
            }
            taken.push(name.clone());
            ctx.places.push((name, kind));
        }
        taken
    }

    // ---- stack entries ----------------------------------------------

    /// A tree, or a definition of one thing (which may be a tree): what a
    /// scatter's `top=` may stand on.
    fn is_tree_like(&self, entry: &Entry) -> bool {
        match &entry.callee {
            Callee::Element(l) => l.name == "tree",
            Callee::Definition(_) => self.class_of(entry) == Class::Thing,
        }
    }

    pub(crate) fn class_of(&self, entry: &Entry) -> Class {
        match &entry.callee {
            Callee::Element(l) => match l.name {
                "dirt" | "water" | "cliff" | "flagstones" | "planks" => Class::Body,
                "ground" | "grass" | "scatter" => Class::Area,
                _ => match l.role {
                    Role::Thing | Role::Run => Class::Thing,
                    Role::Surface | Role::Scatter => Class::Area,
                },
            },
            Callee::Definition(d) => {
                if self.def_info(d).is_some_and(|i| i.thing) {
                    Class::Thing
                } else {
                    Class::Area
                }
            }
        }
    }

    /// A positional name after the element (`dirt hillDirt`).
    pub(crate) fn heights_label(&mut self, entry: &mut Entry, e: &KdlEntry) -> bool {
        let Some(s) = arg_str(e) else {
            return false;
        };
        if entry.label.is_some() || !is_ident(s) {
            return false;
        }
        // a word of one of its controls stays a mistake with today's help
        if entry.element().is_some_and(|l| {
            l.controls
                .iter()
                .any(|k| k.words.iter().any(|w| w.word == s))
        }) {
            return false;
        }
        if matches!(self.class_of(entry), Class::Body | Class::Area)
            && !matches!(&entry.callee, Callee::Definition(_))
        {
            entry.label = Some(Spanned {
                value: s.to_owned(),
                span: value_span(e),
            });
            return true;
        }
        false
    }

    /// `top=`, `at=` (a name or "(x, y)") and `slope=` on an entry's line;
    /// false when `key` is for today's decoder.
    pub(super) fn heights_property(
        &mut self,
        entry: &mut Entry,
        key: &Spanned<String>,
        e: &KdlEntry,
        ctx: &Ctx,
        parent: Parent,
    ) -> bool {
        let class = self.class_of(entry);
        match key.value.as_str() {
            "top"
                if class == Class::Body
                    || (class == Class::Thing
                        && (parent != Parent::Scatter || self.is_tree_like(entry))) =>
            {
                // an element's own `top` words (a cliff's `top=dirt`) stay theirs
                if let (Some(l), Some(s)) = (entry.element(), e.value().as_string())
                    && l.control("top")
                        .is_some_and(|k| k.words.iter().any(|w| w.word == s))
                {
                    return false;
                }
                if entry.top.is_some() {
                    self.err(
                        "dt::duplicate",
                        key.span,
                        "`top` is set twice",
                        "again here",
                    );
                    return true;
                }
                let what = format!("`{} top`", entry.what.value);
                let mode = if class == Class::Thing {
                    Mode::Draw
                } else {
                    Mode::Top
                };
                if let Some((x, _)) = self.height_expr(e, ctx, &what, mode) {
                    entry.top = Some(x);
                }
                true
            }
            "slope" if class == Class::Body => {
                self.slope_value(entry, e);
                true
            }
            _ => false,
        }
    }

    /// `at=`: the one word for where an entry goes.  It takes a point, a
    /// line (a name, an edge or an edge path) or a region, or
    /// `foot(cliff)`, the band at a kind of thing's foot; each element
    /// takes the kinds [`schema::Element::place`] lists and refuses the
    /// rest with `dt::type`.
    #[allow(clippy::too_many_lines)]
    pub(super) fn at_word(
        &mut self,
        entry: &mut Entry,
        key: &Spanned<String>,
        e: &KdlEntry,
        ctx: &Ctx,
    ) {
        use crate::place::Along;
        use crate::schema::PlaceWord as W;
        let span = value_span(e);
        if entry.at_place.is_some()
            || entry.place.at.is_some()
            || entry.place.region.is_some()
            || entry.place.along.is_some()
            || entry.place.against.is_some()
        {
            self.err("dt::duplicate", key.span, "`at` is set twice", "again here")
                .help = Some(
                "an entry has one place: build the one you mean with `within`, `except` and `boundedBy`"
                    .to_owned(),
            );
            return;
        }
        let class = self.class_of(entry);
        let (can_region, can_line, can_point, can_foot) = match entry.element() {
            Some(l) => (
                l.place.contains(&W::In),
                l.place.contains(&W::Along),
                l.place.contains(&W::At),
                l.place.contains(&W::Against),
            ),
            None => (true, true, class == Class::Thing, true),
        };
        let Some(s) = e.value().as_string() else {
            if can_point {
                entry.place.at = self.point(e, "at", ctx);
            } else {
                self.err(
                    "dt::place",
                    span,
                    "`at` is a place: a name, \"(x, y)\", an edge or `foot(cliff)`",
                    "not a place",
                );
            }
            return;
        };
        let t = s.trim();

        // the band at a kind of thing's foot
        if let Some(inner) = t.strip_prefix("foot(").and_then(|r| r.strip_suffix(')')) {
            let kind = inner.trim();
            if !crate::elements::TAGS.contains(&kind) {
                self.err(
                    "dt::unknown-name",
                    span,
                    format!("`{kind}` is not a kind of thing"),
                    "unknown kind",
                )
                .help = Some(
                    did_you_mean(kind, crate::elements::TAGS.iter().copied()).map_or_else(
                        || {
                            format!(
                                "things gather at the foot of: {}",
                                crate::elements::TAGS.join(", ")
                            )
                        },
                        |m| format!("did you mean `{m}`?"),
                    ),
                );
            } else if !can_foot {
                self.place_refusal(entry, span, t, "the foot of a thing", can_line);
            } else {
                entry.place.against = Some(Spanned {
                    value: kind.to_owned(),
                    span,
                });
            }
            return;
        }

        // an edge, or a path of edges round the tile; a place the tile
        // declares wins over an edge word of the same name (a region
        // named `west` is that region, not the west edge)
        let edge_like = t
            .split_whitespace()
            .next()
            .is_some_and(|w| w.chars().all(|c| c.is_ascii_alphabetic()))
            && !ctx.places.iter().any(|(n, _)| n == t);
        match if edge_like {
            place::parse_edges(t)
        } else {
            Ok(None)
        } {
            Ok(Some(edges)) => {
                // one edge word on a thing is the middle of that edge
                if edges.len() == 1 && can_point && !can_line {
                    entry.place.at = self.point(e, "at", ctx);
                } else if can_line {
                    entry.place.along = Some(Spanned {
                        value: Along::Edges(edges),
                        span,
                    });
                } else {
                    self.place_refusal(entry, span, t, "a line", can_line);
                }
                return;
            }
            Ok(None) => {}
            Err(err) => {
                let at = self.inner_at(e, s);
                self.err(
                    "dt::place",
                    sub_span(span, at + err.start, at + err.end),
                    format!("can't read `at`: {}", err.message),
                    "here",
                )
                .help = Some(
                    "an edge path walks round the tile: \"north west\", \"north west south\""
                        .to_owned(),
                );
                return;
            }
        }

        // a name, or a literal point in brackets
        if t.starts_with('(') || is_ident(t) {
            let Some((r, kind)) = self.place_value(e, ctx, "at") else {
                return;
            };
            match kind {
                PlaceKind::Point => {
                    if !can_point {
                        self.place_refusal(entry, span, t, "a point", can_line);
                        return;
                    }
                    match &r.value {
                        PlaceRef::Point(x, y) => {
                            entry.place.at = Some(Spanned {
                                value: place::Point::Pair {
                                    x: Expr::num(*x),
                                    y: Expr::num(*y),
                                },
                                span: r.span,
                            });
                        }
                        PlaceRef::Name(n) => {
                            entry.place.at = Some(Spanned {
                                value: place::Point::Named(n.clone()),
                                span: r.span,
                            });
                        }
                        PlaceRef::Framed(f) => {
                            entry.place.at = Some(Spanned {
                                value: place::Point::Framed(f.clone()),
                                span: r.span,
                            });
                        }
                    }
                    entry.at_place = Some(r);
                }
                PlaceKind::Region => {
                    if !can_region {
                        self.place_refusal(entry, span, t, "a region", can_line);
                        return;
                    }
                    if let PlaceRef::Name(n) = &r.value
                        && (class != Class::Thing || n != BUILTIN_REGION)
                    {
                        entry.place.region = Some(Spanned {
                            value: n.clone(),
                            span: r.span,
                        });
                    }
                    entry.at_place = Some(r);
                }
                PlaceKind::Line => {
                    let PlaceRef::Name(n) = &r.value else {
                        return;
                    };
                    if can_line {
                        let along = match n.as_str() {
                            "northEdge" => Along::Edges(vec![Edge::North]),
                            "eastEdge" => Along::Edges(vec![Edge::East]),
                            "southEdge" => Along::Edges(vec![Edge::South]),
                            "westEdge" => Along::Edges(vec![Edge::West]),
                            "edges" => {
                                Along::Edges(vec![Edge::North, Edge::East, Edge::South, Edge::West])
                            }
                            _ => Along::Line(n.clone()),
                        };
                        entry.place.along = Some(Spanned {
                            value: along,
                            span: r.span,
                        });
                    } else {
                        self.place_refusal(entry, span, t, "a line", can_line);
                    }
                }
            }
            return;
        }

        // today's quoted pair, or a corner word
        if can_point {
            entry.place.at = self.point(e, "at", ctx);
        } else {
            self.place_refusal(entry, span, t, "a point", can_line);
        }
    }

    /// An `at=` this entry's element can't take: `dt::type`, with the kinds
    /// it does take.
    fn place_refusal(
        &mut self,
        entry: &Entry,
        span: miette::SourceSpan,
        shown: &str,
        kind: &str,
        can_line: bool,
    ) {
        let owner = entry.what.value.clone();
        let takes = match entry.element() {
            Some(l) if l.role == Role::Thing => "stands at a point or fills a region",
            Some(l) if l.role == Role::Run => "is built along a line or fills a region",
            Some(_) if can_line => "fills a region or follows a line",
            Some(_) => "fills a region",
            None if self.class_of(entry) == Class::Thing => "stands at a point or fills a region",
            None => "fills a region or follows a line",
        };
        let run = entry.element().is_some_and(|l| l.role == Role::Run);
        let name = if is_ident(shown) {
            shown.to_owned()
        } else {
            "<place>".to_owned()
        };
        let help = if kind == "the foot of a thing" {
            "`foot(cliff)` gathers things at a foot: `scatter at=\"foot(cliff)\" perSquare=20 { stone }`"
                .to_owned()
        } else if kind == "a line" && entry.element().is_some_and(|l| l.role == Role::Thing) {
            format!(
                "one {owner} stands at a point, and a line has no one place on it; several along a line are `scatter at=<line> count=3 {{ {owner} }}`"
            )
        } else if run {
            "a wall is built along a line: `at=north`, an edge path such as \"north west\", or a drawn line"
                .to_owned()
        } else {
            format!(
                "declare a region round it (`region {name}Area {{ within 5 of={name} }}`) and write `at=` that, or leave `at=` out for the whole tile"
            )
        };
        self.err(
            "dt::type",
            span,
            format!("`{owner}` {takes}, and `{shown}` is {kind}"),
            "not a place it takes",
        )
        .help = Some(help);
    }

    fn slope_value(&mut self, entry: &mut Entry, e: &KdlEntry) {
        let span = value_span(e);
        let v = if self.format1 {
            self.angle_v1(e)
        } else {
            match e.value() {
                KdlValue::Integer(i) => i.to_string().parse::<f64>().ok(),
                KdlValue::Float(f) => Some(*f),
                _ => None,
            }
        };
        match v {
            Some(d) if d > 0.0 && d < 90.0 => {
                if entry.slope.is_some() {
                    self.err("dt::duplicate", span, "`slope` is set twice", "again here");
                } else {
                    entry.slope = Some(Spanned { value: d, span });
                }
            }
            None if self.format1 => {}
            _ => {
                self.err(
                    "dt::value",
                    span,
                    "a slope is an angle in degrees, above 0 and below 90",
                    "not an angle",
                )
                .help = Some(
                    if self.format1 {
                        "write `slope \"30deg\"`"
                    } else {
                        "write `slope 30`"
                    }
                    .to_owned(),
                );
            }
        }
    }

    /// An angle written in format 1 as a string with its unit, in degrees;
    /// a mistake is reported and gives `None`.
    pub(crate) fn angle_v1(&mut self, e: &KdlEntry) -> Option<f64> {
        let span = value_span(e);
        let text = match e.value() {
            KdlValue::String(s) => s.clone(),
            KdlValue::Integer(i) => i.to_string(),
            KdlValue::Float(f) => f.to_string(),
            _ => {
                self.err(
                    "dt::value",
                    span,
                    "an angle is a number with its unit",
                    "not an angle",
                )
                .help = Some("write an angle with its unit: \"30deg\"".to_owned());
                return None;
            }
        };
        let at = self.inner_at(e, &text);
        let x = match expr::parse_units(&text) {
            Ok(x) => x,
            Err(err) => {
                self.err(
                    err.code.unwrap_or("dt::value"),
                    sub_span(span, at + err.start, at + err.end),
                    format!("can't read an angle: {}", err.message),
                    "here",
                )
                .help = err.help;
                return None;
            }
        };
        let names = |_: &str| None;
        match crate::units::expect(&x, crate::units::Want::Angle, &names, false).and_then(|_| {
            crate::units::lower(&x, crate::units::Base::Dmm, self.known.units(), None)
        }) {
            Ok(l) => l.sample(&|_| None, 0.0).ok(),
            Err(ke) => {
                self.kind_error(&ke, span, "an angle");
                None
            }
        }
    }

    /// `pin` and `slope` lines inside a body's block; false for any other
    /// child.
    pub(crate) fn heights_child(&mut self, entry: &mut Entry, c: &KdlNode, ctx: &Ctx) -> bool {
        let class = self.class_of(entry);
        match c.name().value() {
            "slope" if class == Class::Body => {
                match c.entries() {
                    [e] if e.name().is_none() && c.children().is_none() => {
                        self.slope_value(entry, e);
                    }
                    _ => {
                        self.err("dt::value", c.span(), "`slope` takes one angle", "here")
                            .help = Some("write `slope 30`".to_owned());
                    }
                }
                true
            }
            "pin" => {
                if class != Class::Body {
                    self.err(
                        "dt::pin",
                        c.name().span(),
                        format!("`{}` has no pins", entry.what.value),
                        "not a body",
                    )
                    .help = Some(
                        "pins hold a body's top (dirt, water, cliff, flagstones, planks) to a height at a place"
                            .to_owned(),
                    );
                    return true;
                }
                // a point in a pin is in the frame of the region it acts in
                // (a drawn line's strip is the region a body on it acts in)
                self.frame_region = entry
                    .place
                    .region
                    .as_ref()
                    .map(|r| r.value.clone())
                    .or_else(|| match entry.place.along.as_ref().map(|a| &a.value) {
                        Some(crate::place::Along::Line(n)) => Some(n.clone()),
                        _ => None,
                    });
                let pin = self.pin_node(c, ctx);
                self.frame_region = None;
                if let Some(p) = pin {
                    entry.pins.push(p);
                }
                true
            }
            _ => false,
        }
    }

    pub(crate) fn pin_node(&mut self, c: &KdlNode, ctx: &Ctx) -> Option<Pin> {
        let mut name = None;
        let mut top = None;
        let mut at = None;
        let mut ok = true;
        for (i, e) in c.entries().iter().enumerate() {
            match e.name().map(|k| k.value()) {
                None if i == 0 && arg_str(e).is_some_and(is_ident) => {
                    name = Some(Spanned {
                        value: arg_str(e).unwrap_or_default().to_owned(),
                        span: value_span(e),
                    });
                }
                Some("top") => {
                    if top.is_some() {
                        self.err(
                            "dt::duplicate",
                            e.span(),
                            "`top` is set twice",
                            "again here",
                        );
                        ok = false;
                    } else {
                        match self.height_expr(e, ctx, "a pin's top", Mode::Top) {
                            Some((x, _)) => top = Some(x),
                            None => ok = false,
                        }
                    }
                }
                Some("at") => {
                    if at.is_some() {
                        self.err("dt::duplicate", e.span(), "`at` is set twice", "again here");
                        ok = false;
                    } else {
                        match self.place_value(e, ctx, "at") {
                            Some((r, _)) => at = Some(r),
                            None => ok = false,
                        }
                    }
                }
                _ => {
                    self.err(
                        "dt::pin",
                        e.span(),
                        "a pin is `pin name top=… at=…`",
                        "unexpected",
                    )
                    .help = Some("a pin says where and how high, nothing else".to_owned());
                    ok = false;
                }
            }
        }
        if c.children().is_some() {
            self.err("dt::pin", c.span(), "a pin has no block", "here");
            ok = false;
        }
        if !ok {
            return None;
        }
        match (top, at) {
            (Some(top), Some(at)) => Some(Pin {
                name,
                top,
                at,
                span: c.span(),
            }),
            (t, a) => {
                let missing = match (t.is_none(), a.is_none()) {
                    (true, true) => "top= and at=",
                    (true, false) => "top=",
                    _ => "at=",
                };
                self.err(
                    "dt::pin",
                    c.span(),
                    format!("a pin needs {missing}"),
                    format!("no {missing}"),
                )
                .help =
                    Some("write `pin top=dirtEdgeHeight at=edges`: what height, where".to_owned());
                None
            }
        }
    }

    /// A `base` line in a stack.
    pub(crate) fn base_node(&mut self, n: &KdlNode, index: usize) -> Option<Base> {
        if !n.entries().is_empty() || n.children().is_some() {
            self.err("dt::base", n.span(), "`base` takes no settings yet", "here")
                .help = Some("write just `base`: the connector is the build target's".to_owned());
        }
        Some(Base {
            index,
            span: n.span(),
        })
    }

    /// The order checks over a tile's stack: the base first. Every other layer
    /// is built where it is listed (the plan refuses what that cannot satisfy).
    pub(crate) fn check_stack_order(&mut self, base: Option<&Base>) {
        if let Some(b) = base
            && b.index != 0
        {
            self.err(
                "dt::base-order",
                b.span,
                "the base is the first line of the stack",
                "not first",
            )
            .help = Some("the connector is laid first and everything rests on it: move `base` above the other lines".to_owned());
        }
    }
}
