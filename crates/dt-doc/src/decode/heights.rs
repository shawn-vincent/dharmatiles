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

/// The fluids: written last.
const FLUIDS: [&str; 1] = ["water"];

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
        self.known
            .consts()
            .iter()
            .map(|c| (c.name.value.clone(), c.dim))
            .collect()
    }

    fn name_dim(ctx: &Ctx, n: &str) -> Option<Dim> {
        match n {
            "x" | "y" => Some(Dim::Squares),
            "bottom" => Some(Dim::Mm),
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
            KdlValue::String(s) => match expr::parse_v2(s) {
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
        // A top, a pin and a distance are bands the solver fits, not draws:
        // the odds of a weighting have nothing to act on.
        if mode != Mode::Const && expr.weighted().is_some() {
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
            let special = matches!(n, "x" | "y" | "bottom");
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
        match dim_of(&expr, &|n| Self::name_dim(ctx, n)) {
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
            Ok(d) => Some((Spanned { value: expr, span }, d)),
        }
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
        if t.starts_with('(') {
            let p = match place::parse_point(t) {
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
            };
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
                self.err(
                    "dt::point",
                    span,
                    format!("`({x}, {y})` is off the tile"),
                    "outside the tile",
                )
                .help = Some(format!("this tile is {c}x{r}: x runs 0 to {c}, y 0 to {r}"));
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
        if !self.declared_name(&name) {
            return None;
        }
        let exists = Self::name_dim(ctx, &name.value).is_some();
        match kind {
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
            PlaceRef::Name(_) => {
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
            } else if self.known.consts().iter().any(|c| c.name.value == name) {
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
            "top" if matches!(class, Class::Body | Class::Thing) && parent != Parent::Scatter => {
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
                if let Some((x, _)) = self.height_expr(e, ctx, &what, Mode::Top) {
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
        let (can_region, can_line, can_point, can_foot, pending_line) = match entry.element() {
            Some(l) => (
                l.place.contains(&W::In),
                l.place.contains(&W::Along),
                l.place.contains(&W::At),
                l.place.contains(&W::Against),
                l.role == Role::Thing,
            ),
            None => (true, true, class == Class::Thing, true, false),
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

        // an edge, or a path of edges round the tile
        let edge_like = t
            .split_whitespace()
            .next()
            .is_some_and(|w| w.chars().all(|c| c.is_ascii_alphabetic()));
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
                    } else if pending_line && !BUILTIN_LINES.contains(&n.as_str()) {
                        // checked here; the evaluator says it is not built yet
                        entry.at_place = Some(r);
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
                "one {owner} stands at a point; several along a line are `scatter at=<line> perSquare=… {{ {owner} }}`"
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
        let v = match e.value() {
            KdlValue::Integer(i) => i.to_string().parse::<f64>().ok(),
            KdlValue::Float(f) => Some(*f),
            _ => None,
        };
        match v {
            Some(d) if d > 0.0 && d < 90.0 => {
                if entry.slope.is_some() {
                    self.err("dt::duplicate", span, "`slope` is set twice", "again here");
                } else {
                    entry.slope = Some(Spanned { value: d, span });
                }
            }
            _ => {
                self.err(
                    "dt::value",
                    span,
                    "a slope is an angle in degrees, above 0 and below 90",
                    "not an angle",
                )
                .help = Some("write `slope 30`".to_owned());
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
                if let Some(p) = self.pin_node(c, ctx) {
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

    /// The order checks over a tile's stack: the base first, fluids last.
    pub(crate) fn check_stack_order(&mut self, base: Option<&Base>, out: &[Entry], heights: bool) {
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
        if !heights {
            return;
        }
        let Some(fluid) = out
            .iter()
            .position(|e| FLUIDS.contains(&e.what.value.as_str()))
        else {
            return;
        };
        for e in &out[fluid + 1..] {
            if !FLUIDS.contains(&e.what.value.as_str()) {
                let f = &out[fluid].what.value;
                self.err(
                    "dt::fluid-order",
                    e.what.span,
                    format!("`{}` is written after the {f}", e.what.value),
                    "after a fluid",
                )
                .help = Some(format!(
                    "fluids are written last, filling round whatever stands: move `{}` above the `{f}`",
                    e.what.value
                ));
            }
        }
    }
}
