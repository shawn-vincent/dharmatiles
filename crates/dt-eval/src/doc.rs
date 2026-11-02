//! The document adapter: KDL text → the few facts the evaluator needs.
//!
//! **Thin on purpose.** dt-doc is being reworked to the decided R6
//! vocabulary on its own branch (rust/p1-doc); until it lands, today's
//! dt-doc decodes the old words (`hills`, `self=`, `cols=`, heights in h)
//! and cannot read the new ones. This module decodes the minimal subset
//! P1 needs, in the new vocabulary, straight from the `kdl` node tree:
//!
//! - `use "path" name…` (top level; `lib:` or relative), and `define name
//!   { stack { … } }` in imported files (no `param`s yet);
//! - `tile name size="CxR" seed=N { … }` with `about`, `levels name=mm…`,
//!   `line name { through P P… }` (straight; points are `"x,y"` in squares
//!   or edge and corner words), `region name { bounded-by L…; fill P }`,
//!   and one `stack`;
//! - stack entries: a look or definition, `in=region`, `seed=N`, controls
//!   as properties or as child lines (`level 5`).
//!
//! The evaluator only sees [`Tile`]. Re-pointing at the reworked dt-doc
//! means filling [`Tile`] from `dt_doc::model` instead of here; nothing
//! downstream changes.

use std::collections::BTreeMap;
use std::sync::Arc;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::{NamedSource, SourceSpan};

use crate::diag::{Diag, Src, did_you_mean};
use crate::source::{Source, resolve};

/// The standard levels, mm at the DungeonBlocks reference: the prelude
/// every tile sees (R6 Q8). The reworked dt-doc loads them from
/// `library/parts/levels.kdl`.
pub const STANDARD_LEVELS: &[(&str, f64)] = &[
    ("edge", 3.4),
    ("ground", 5.0),
    ("water", 2.6),
    ("wall-top", 33.1),
];

/// A value with its span.
#[derive(Clone, Debug, PartialEq)]
pub struct Spanned<T> {
    /// The value.
    pub value: T,
    /// Where it was written.
    pub span: SourceSpan,
}

/// A control value as written.
#[derive(Clone, Debug, PartialEq)]
pub enum Raw {
    /// A number.
    Number(f64),
    /// A string or bare word.
    Text(String),
    /// `#true` / `#false`.
    Bool(bool),
}

/// One stack entry.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The look or definition named.
    pub what: Spanned<String>,
    /// `in=`: the region it acts in (default the whole tile).
    pub area: Option<Spanned<String>>,
    /// `seed=`: pins its look.
    pub seed: Option<u64>,
    /// Controls, in the order written.
    pub controls: Vec<(Spanned<String>, Spanned<Raw>)>,
    /// The text it came from.
    pub src: Src,
    /// The whole node.
    pub span: SourceSpan,
}

/// A region: lines (by name) and fill points (squares).
#[derive(Clone, Debug)]
pub struct Region {
    /// Its name.
    pub name: String,
    /// The lines that bound it.
    pub bounded_by: Vec<Spanned<String>>,
    /// Fill points.
    pub fills: Vec<At>,
}

/// What the evaluator needs from a tile document.
#[derive(Clone, Debug)]
pub struct Tile {
    /// The tile's name.
    pub name: String,
    /// Squares across.
    pub cols: u32,
    /// Squares up.
    pub rows: u32,
    /// The root seed (1 if none is written).
    pub seed: u64,
    /// Levels: the standard ones, then the tile's own.
    pub levels: Vec<(String, f64)>,
    /// Straight lines, by name, as points in squares.
    pub lines: BTreeMap<String, Vec<At>>,
    /// Regions.
    pub regions: Vec<Region>,
    /// The stack, with definitions inlined.
    pub stack: Vec<Entry>,
    /// The tile's own text.
    pub src: Src,
}

/// FNV-1a of a document's text (the build record's document hash).
#[must_use]
pub fn text_hash(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Read the tile at `path` and its imports.
///
/// # Errors
/// Every problem found, each at its text.
pub fn read(sources: &dyn Source, path: &str) -> Result<Tile, Vec<Diag>> {
    let text = sources
        .read(path)
        .map_err(|e| vec![Diag::plain("dt::source", e)])?;
    let src: Src = Arc::new(NamedSource::new(path, text.clone()));
    let kdl = parse(&src, &text)?;
    let mut d = Decoder::default();
    let mut defs: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for node in kdl.nodes() {
        if node.name().value() == "use" {
            d.import(sources, path, &src, node, &mut defs);
        }
    }
    let tiles: Vec<&KdlNode> = kdl
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "tile")
        .collect();
    for node in kdl.nodes() {
        match node.name().value() {
            "use" | "tile" | "define" => {}
            other => d.errors.push(
                Diag::at(
                    &src,
                    node.name().span(),
                    "dt::unknown-node",
                    format!("unknown top-level node `{other}`"),
                    "not a tile, use or define",
                )
                .maybe_help(
                    did_you_mean(other, ["tile", "use", "define"])
                        .map(|w| format!("did you mean `{w}`?")),
                ),
            ),
        }
    }
    let tile = match tiles.as_slice() {
        [one] => Some(d.tile(&src, one, &defs)),
        [] => {
            d.errors.push(
                Diag::plain("dt::no-tile", format!("`{path}` has no `tile` node"))
                    .help("a tile document has one `tile name size=\"1x1\" { stack { … } }`"),
            );
            None
        }
        [_, second, ..] => {
            d.errors.push(Diag::at(
                &src,
                second.span(),
                "dt::two-tiles",
                "a document holds one tile",
                "second tile",
            ));
            None
        }
    };
    match tile {
        Some(t) if d.errors.is_empty() => Ok(t),
        _ => Err(d.errors),
    }
}

fn parse(src: &Src, text: &str) -> Result<KdlDocument, Vec<Diag>> {
    KdlDocument::parse(text).map_err(|e| {
        let mut out: Vec<Diag> = Vec::new();
        for g in &e.diagnostics {
            // kdl spans run from the node's start to the mistake: point at
            // the word the mistake ends in.
            let end = (g.span.offset() + g.span.len()).min(text.len());
            let (at, word) = word_before(text, end);
            let d = if numberish(word) {
                Diag::at(src, SourceSpan::new(at.into(), word.len()), "dt::syntax", format!("`{word}` must be quoted"), "not a KDL number")
                    .help(if word.ends_with(|c: char| c.is_ascii_alphabetic()) {
                        format!("sizes are mm and units are not written: write {}", word.trim_end_matches(|c: char| c.is_ascii_alphabetic()))
                    } else {
                        format!("ranges and percents are quoted: write \"{word}\"")
                    })
            } else {
                Diag::at(
                    src,
                    SourceSpan::new(at.into(), word.len().max(1)),
                    "dt::syntax",
                    g.message.clone().unwrap_or_else(|| "this is not valid KDL v2".to_owned()),
                    g.label.clone().unwrap_or_else(|| "here".to_owned()),
                )
                .maybe_help(g.help.clone().or_else(|| Some("tiles are KDL v2: switches are #true/#false, and ranges and percents are quoted".to_owned())))
            };
            if !out.iter().any(|o| o.message == d.message && o.at.as_ref().map(|a| a.1) == d.at.as_ref().map(|a| a.1)) {
                out.push(d);
            }
        }
        out
    })
}

/// The word (no spaces, `=`, braces or quotes) that ends at `end`, and
/// where it starts.
fn word_before(text: &str, end: usize) -> (usize, &str) {
    let stop = |c: char| c.is_whitespace() || "{};=()\"".contains(c);
    let mut end = end;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let tail_end = text[end..].find(stop).map_or(text.len(), |i| end + i);
    let start = text[..end].rfind(stop).map_or(0, |i| i + 1);
    (start, &text[start..tail_end])
}

/// A number with a unit, `..` or `%` glued on.
fn numberish(w: &str) -> bool {
    let body = w.trim_start_matches(['-', '+']);
    body.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        && (w.contains("..") || w.ends_with('%') || w.ends_with(|c: char| c.is_ascii_alphabetic()))
}

#[derive(Default)]
struct Decoder {
    errors: Vec<Diag>,
}

fn value_span(e: &KdlEntry) -> SourceSpan {
    let s = e.span();
    let n = e.format().map_or(0, |f| f.value_repr.len()).min(s.len());
    if n == 0 {
        return s;
    }
    SourceSpan::new((s.offset() + s.len() - n).into(), n)
}

fn raw(v: &KdlValue) -> Raw {
    match v {
        KdlValue::String(s) => Raw::Text(s.clone()),
        #[allow(clippy::cast_precision_loss)]
        KdlValue::Integer(i) => Raw::Number(*i as f64),
        KdlValue::Float(f) => Raw::Number(*f),
        KdlValue::Bool(b) => Raw::Bool(*b),
        KdlValue::Null => Raw::Text(String::new()),
    }
}

impl Decoder {
    fn err(
        &mut self,
        src: &Src,
        span: SourceSpan,
        code: &'static str,
        msg: impl Into<String>,
        label: &str,
        help: Option<String>,
    ) {
        self.errors
            .push(Diag::at(src, span, code, msg, label).maybe_help(help));
    }

    fn import(
        &mut self,
        sources: &dyn Source,
        from: &str,
        src: &Src,
        node: &KdlNode,
        defs: &mut BTreeMap<String, Vec<Entry>>,
    ) {
        let mut args = node.entries().iter().filter(|e| e.name().is_none());
        let Some(target) = args.next() else {
            self.err(
                src,
                node.span(),
                "dt::use",
                "`use` needs a path",
                "here",
                Some("use \"lib:parts/grass\" lawn".into()),
            );
            return;
        };
        let Some(t) = target.value().as_string() else {
            self.err(
                src,
                value_span(target),
                "dt::use",
                "the path is a string",
                "not a string",
                None,
            );
            return;
        };
        let path = resolve(from, t);
        let text = match sources.read(&path) {
            Ok(t) => t,
            Err(e) => {
                self.err(
                    src,
                    value_span(target),
                    "dt::use",
                    e,
                    "imported here",
                    Some(
                        "library paths start with `lib:`; others are relative to this file".into(),
                    ),
                );
                return;
            }
        };
        let isrc: Src = Arc::new(NamedSource::new(path.clone(), text.clone()));
        let doc = match parse(&isrc, &text) {
            Ok(d) => d,
            Err(e) => {
                self.errors.extend(e);
                return;
            }
        };
        let mut here: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
        for d in doc.nodes().iter().filter(|n| n.name().value() == "define") {
            let Some(name) = d.entries().first().and_then(|e| e.value().as_string()) else {
                self.err(
                    &isrc,
                    d.span(),
                    "dt::define",
                    "a definition needs a name",
                    "here",
                    None,
                );
                continue;
            };
            let mut stack = Vec::new();
            for c in d.children().map(KdlDocument::nodes).unwrap_or_default() {
                match c.name().value() {
                    "stack" => stack = self.stack(&isrc, c),
                    "param" | "about" => self.err(
                        &isrc,
                        c.name().span(),
                        "dt::not-yet",
                        format!(
                            "`{}` in definitions comes with the dt-doc rework",
                            c.name().value()
                        ),
                        "not read yet",
                        None,
                    ),
                    other => self.err(
                        &isrc,
                        c.name().span(),
                        "dt::unknown-node",
                        format!("unknown node `{other}` in a definition"),
                        "here",
                        None,
                    ),
                }
            }
            here.insert(name.to_owned(), stack);
        }
        for want in args {
            let Some(n) = want.value().as_string() else {
                continue;
            };
            match here.remove(n) {
                Some(stack) => {
                    defs.insert(n.to_owned(), stack);
                }
                None => self.err(
                    src,
                    value_span(want),
                    "dt::use",
                    format!("`{path}` defines no `{n}`"),
                    "not defined there",
                    did_you_mean(n, here.keys().map(String::as_str))
                        .map(|w| format!("did you mean `{w}`?")),
                ),
            }
        }
    }

    fn tile(&mut self, src: &Src, node: &KdlNode, defs: &BTreeMap<String, Vec<Entry>>) -> Tile {
        let mut t = Tile {
            name: String::new(),
            cols: 1,
            rows: 1,
            seed: 1,
            levels: STANDARD_LEVELS
                .iter()
                .map(|(n, v)| ((*n).to_owned(), *v))
                .collect(),
            lines: BTreeMap::new(),
            regions: Vec::new(),
            stack: Vec::new(),
            src: Arc::clone(src),
        };
        for e in node.entries() {
            match (e.name().map(kdl::KdlIdentifier::value), e.value()) {
                (None, KdlValue::String(s)) if t.name.is_empty() => t.name.clone_from(s),
                (Some("size"), KdlValue::String(s)) => match s
                    .split_once('x')
                    .and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?)))
                {
                    Some((c, r)) if c > 0 && r > 0 => (t.cols, t.rows) = (c, r),
                    _ => self.err(
                        src,
                        value_span(e),
                        "dt::size",
                        format!("`{s}` is not a size"),
                        "columns x rows",
                        Some("write size=\"2x1\": columns first".into()),
                    ),
                },
                (Some("seed"), KdlValue::Integer(i)) if *i >= 0 => {
                    t.seed = u64::try_from(*i).unwrap_or(u64::MAX)
                }
                (Some(k), _) => self.err(
                    src,
                    e.span(),
                    "dt::tile",
                    format!("a tile has no `{k}`"),
                    "not a tile property",
                    did_you_mean(k, ["size", "seed"])
                        .map(|w| format!("did you mean `{w}`?"))
                        .or_else(|| Some("a tile takes size=\"CxR\" and seed=N".into())),
                ),
                _ => self.err(
                    src,
                    e.span(),
                    "dt::tile",
                    "unexpected value on the tile line",
                    "here",
                    Some("tile name size=\"1x1\" seed=1 { … }".into()),
                ),
            }
        }
        if t.name.is_empty() {
            self.err(
                src,
                node.name().span(),
                "dt::tile",
                "a tile needs a name",
                "here",
                Some("tile flat size=\"1x1\" { … }".into()),
            );
        }
        let mut stacks = 0;
        for c in node.children().map(KdlDocument::nodes).unwrap_or_default() {
            match c.name().value() {
                "about" => {}
                "levels" => {
                    for e in c.entries() {
                        match (e.name(), raw(e.value())) {
                            (Some(n), Raw::Number(v)) => {
                                t.levels.retain(|(k, _)| k != n.value());
                                t.levels.push((n.value().to_owned(), v));
                            }
                            _ => self.err(
                                src,
                                e.span(),
                                "dt::levels",
                                "a level is name=mm",
                                "here",
                                Some("levels foot=4".into()),
                            ),
                        }
                    }
                }
                "line" => {
                    if let Some((name, pts)) = self.line(src, c) {
                        t.lines.insert(name, pts);
                    }
                }
                "region" => {
                    if let Some(r) = self.region(src, c) {
                        t.regions.push(r);
                    }
                }
                "stack" => {
                    stacks += 1;
                    for e in self.stack(src, c) {
                        match defs.get(&e.what.value) {
                            Some(def) if e.controls.is_empty() => {
                                t.stack.extend(def.iter().cloned().map(|mut d| {
                                    if d.area.is_none() {
                                        d.area.clone_from(&e.area);
                                    }
                                    d
                                }))
                            }
                            Some(_) => self.err(
                                src,
                                e.span,
                                "dt::not-yet",
                                "setting a definition's params comes with the dt-doc rework",
                                "params given",
                                None,
                            ),
                            None => t.stack.push(e),
                        }
                    }
                }
                other => self.err(
                    src,
                    c.name().span(),
                    "dt::unknown-node",
                    format!("a tile has no `{other}` node"),
                    "not part of a tile",
                    did_you_mean(other, ["about", "levels", "line", "region", "stack"])
                        .map(|w| format!("did you mean `{w}`?"))
                        .or_else(|| {
                            Some("a tile holds about, levels, line, region and one stack".into())
                        }),
                ),
            }
        }
        if stacks != 1 {
            self.err(
                src,
                node.span(),
                "dt::stack",
                "a tile has exactly one `stack`",
                "this tile",
                Some("tile flat { stack { ground } }".into()),
            );
        }
        for r in &t.regions {
            for l in &r.bounded_by {
                if !t.lines.contains_key(&l.value) {
                    self.errors.push(
                        Diag::at(
                            src,
                            l.span,
                            "dt::unknown-line",
                            format!("no line named `{}`", l.value),
                            "not drawn",
                        )
                        .maybe_help(
                            did_you_mean(&l.value, t.lines.keys().map(String::as_str))
                                .map(|w| format!("did you mean `{w}`?")),
                        ),
                    );
                }
            }
        }
        t
    }

    fn stack(&mut self, src: &Src, node: &KdlNode) -> Vec<Entry> {
        let mut out = Vec::new();
        for c in node.children().map(KdlDocument::nodes).unwrap_or_default() {
            let mut e = Entry {
                what: Spanned {
                    value: c.name().value().to_owned(),
                    span: c.name().span(),
                },
                area: None,
                seed: None,
                controls: Vec::new(),
                src: Arc::clone(src),
                span: c.span(),
            };
            for p in c.entries() {
                let Some(name) = p.name() else {
                    self.err(
                        src,
                        p.span(),
                        "dt::control",
                        "a control is name=value",
                        "a bare value",
                        Some(format!(
                            "write {} level=5, or a child line `level 5`",
                            e.what.value
                        )),
                    );
                    continue;
                };
                let value = Spanned {
                    value: raw(p.value()),
                    span: value_span(p),
                };
                self.control(
                    &mut e,
                    Spanned {
                        value: name.value().to_owned(),
                        span: name.span(),
                    },
                    value,
                );
            }
            for g in c.children().map(KdlDocument::nodes).unwrap_or_default() {
                let args: Vec<&KdlEntry> = g.entries().iter().collect();
                match args.as_slice() {
                    [a] if a.name().is_none() => {
                        let value = Spanned {
                            value: raw(a.value()),
                            span: value_span(a),
                        };
                        self.control(
                            &mut e,
                            Spanned {
                                value: g.name().value().to_owned(),
                                span: g.name().span(),
                            },
                            value,
                        );
                    }
                    _ => self.err(
                        src,
                        g.span(),
                        "dt::not-yet",
                        format!(
                            "the group `{}` comes with the looks that take it",
                            g.name().value()
                        ),
                        "not read yet",
                        None,
                    ),
                }
            }
            out.push(e);
        }
        out
    }

    fn control(&mut self, e: &mut Entry, name: Spanned<String>, value: Spanned<Raw>) {
        match (name.value.as_str(), &value.value) {
            ("in", Raw::Text(r)) => {
                e.area = Some(Spanned {
                    value: r.clone(),
                    span: value.span,
                })
            }
            ("seed", Raw::Number(n)) if *n >= 0.0 && n.fract() == 0.0 => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let s = *n as u64;
                e.seed = Some(s);
            }
            ("in" | "seed", _) => self.err(
                &e.src,
                value.span,
                "dt::place",
                format!(
                    "`{}` takes a {}",
                    name.value,
                    if name.value == "in" {
                        "region name"
                    } else {
                        "whole number"
                    }
                ),
                "here",
                None,
            ),
            _ => e.controls.push((name, value)),
        }
    }

    fn line(&mut self, src: &Src, node: &KdlNode) -> Option<(String, Vec<At>)> {
        let name = node
            .entries()
            .first()
            .and_then(|e| e.value().as_string())?
            .to_owned();
        if let Some(p) = node.entries().iter().find(|e| e.name().is_some()) {
            self.err(
                src,
                p.span(),
                "dt::not-yet",
                "line width, wobble and side come with the looks that use them",
                "not read yet",
                None,
            );
        }
        let mut pts = Vec::new();
        for c in node.children().map(KdlDocument::nodes).unwrap_or_default() {
            if c.name().value() != "through" {
                self.err(
                    src,
                    c.name().span(),
                    "dt::line",
                    format!("a line holds `through`, not `{}`", c.name().value()),
                    "here",
                    None,
                );
                continue;
            }
            for e in c.entries() {
                match e.value().as_string().and_then(point) {
                    Some(p) => pts.push(p),
                    None => self.err(src, value_span(e), "dt::point", "not a point", "here",
                        Some("a point is \"x,y\" in squares from the south-west corner, or north, south-east, …".into())),
                }
            }
        }
        if pts.len() < 2 {
            self.err(
                src,
                node.span(),
                "dt::line",
                "a line goes through at least two points",
                "this line",
                Some("line edge { through west east }".into()),
            );
            return None;
        }
        Some((name, pts))
    }

    fn region(&mut self, src: &Src, node: &KdlNode) -> Option<Region> {
        let name = node
            .entries()
            .first()
            .and_then(|e| e.value().as_string())?
            .to_owned();
        let mut r = Region {
            name,
            bounded_by: Vec::new(),
            fills: Vec::new(),
        };
        for c in node.children().map(KdlDocument::nodes).unwrap_or_default() {
            for e in c.entries() {
                let Some(s) = e.value().as_string() else {
                    continue;
                };
                match c.name().value() {
                    "bounded-by" => r.bounded_by.push(Spanned {
                        value: s.to_owned(),
                        span: value_span(e),
                    }),
                    "fill" => match point(s) {
                        Some(p) => r.fills.push(p),
                        None => self.err(
                            src,
                            value_span(e),
                            "dt::point",
                            "not a point",
                            "here",
                            Some("fill \"0.25,0.5\"".into()),
                        ),
                    },
                    other => self.err(
                        src,
                        c.name().span(),
                        "dt::region",
                        format!("a region holds bounded-by and fill, not `{other}`"),
                        "here",
                        None,
                    ),
                }
            }
        }
        if r.fills.is_empty() {
            self.err(
                src,
                node.span(),
                "dt::region",
                "a region needs a fill point",
                "this region",
                Some("fill \"0.5,0.25\"".into()),
            );
        }
        Some(r)
    }
}

/// A point as written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum At {
    /// `"x,y"`: squares from the south-west corner, y north.
    Squares([f64; 2]),
    /// An edge or corner word: a fraction of the tile (`north` is the
    /// middle of the north edge).
    OfTile([f64; 2]),
}

impl At {
    /// In squares on a `cols` × `rows` tile.
    #[must_use]
    pub fn squares(self, cols: u32, rows: u32) -> [f64; 2] {
        match self {
            Self::Squares(p) => p,
            Self::OfTile(f) => [f[0] * f64::from(cols), f[1] * f64::from(rows)],
        }
    }
}

/// `"x,y"` in squares, or an edge or corner word.
fn point(s: &str) -> Option<At> {
    let word = match s {
        "south-west" => [0.0, 0.0],
        "south" => [0.5, 0.0],
        "south-east" => [1.0, 0.0],
        "east" => [1.0, 0.5],
        "north-east" => [1.0, 1.0],
        "north" => [0.5, 1.0],
        "north-west" => [0.0, 1.0],
        "west" => [0.0, 0.5],
        _ => {
            let (a, b) = s.split_once(',')?;
            return Some(At::Squares([
                a.trim().parse().ok()?,
                b.trim().parse().ok()?,
            ]));
        }
    };
    Some(At::OfTile(word))
}
