//! KDL v2 node tree into the typed model, by hand: one function per node
//! type, so every error message is ours and points at the right text.
//! Decoding goes on past a mistake, so a file's mistakes come together.

mod entry;
mod syntax;
mod value;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kdl::{KdlDocument, KdlEntry, KdlNode};
use miette::{NamedSource, SourceSpan};

use crate::error::{DocError, Src};
use crate::known::{DefInfo, Known};
use crate::model::{
    DefKind, Definition, Document, EdgeRule, Edges, Level, Line, Param, Printer, Region, Source,
    Spanned, TileHeader, Use,
};
use crate::place::{Corner, Edge, Point};
use crate::schema::{self, Axis, Control, Kind, MAX_SQUARES, did_you_mean};

/// Decode one KDL v2 file.  `name` labels the source in diagnostics.
///
/// `known` holds the standard levels and the files this one `use`s (see
/// [`Known`]); decode those first, or find them with [`uses`].
///
/// # Errors
/// Every problem found, each pointing at its text with a hint.
pub fn decode(name: &str, text: &str, known: &Known) -> Result<Document, Vec<DocError>> {
    let src = Arc::new(NamedSource::new(name, text.to_owned()));
    let kdl = match KdlDocument::parse(text) {
        Ok(d) => d,
        Err(e) => return Err(syntax::syntax_errors(&src, text, &e)),
    };
    let mut d = Decoder::new(src, known);
    let doc = d.document(&kdl);
    if d.errors.is_empty() {
        Ok(doc)
    } else {
        Err(d.errors)
    }
}

/// Only the `use` lines of a file, so an evaluator can load what a file
/// needs before decoding it.  Nothing else is checked.
///
/// # Errors
/// If the text isn't KDL v2, or a `use` line is malformed.
pub fn uses(name: &str, text: &str) -> Result<Vec<Use>, Vec<DocError>> {
    let src = Arc::new(NamedSource::new(name, text.to_owned()));
    let kdl = match KdlDocument::parse(text) {
        Ok(d) => d,
        Err(e) => return Err(syntax::syntax_errors(&src, text, &e)),
    };
    let known = Known::new();
    let mut d = Decoder::new(src, &known);
    let out: Vec<Use> = kdl
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "use")
        .filter_map(|n| d.use_line(n))
        .collect();
    if d.errors.is_empty() {
        Ok(out)
    } else {
        Err(d.errors)
    }
}

/// Names visible inside one definition body.
#[derive(Default, Clone)]
pub(crate) struct Ctx {
    pub params: Vec<String>,
    pub levels: Vec<String>,
    pub lines: Vec<String>,
    pub regions: Vec<String>,
    /// The tile's size, inside a tile.
    pub size: Option<(u32, u32)>,
    /// Inside a `tile` (expert controls are refused) or a `define`.
    pub in_tile: bool,
}

pub(crate) struct Decoder<'k> {
    pub src: Src,
    pub errors: Vec<DocError>,
    pub known: &'k Known,
    /// Every `define` in this file, decoded or not yet.
    pub local_names: HashSet<String>,
    /// This file's definitions decoded so far.
    pub local: HashMap<String, DefInfo>,
    /// Definitions brought in by `use` lines: name → (info, file).
    pub imported: HashMap<String, (DefInfo, String)>,
    /// While decoding a define: each parameter → the control it fills.
    pub param_controls: HashMap<String, &'static Control>,
}

/// The span of an entry's value (an entry's span covers `name=value`).
pub(crate) fn value_span(e: &KdlEntry) -> SourceSpan {
    let s = e.span();
    let n = e.format().map_or(0, |f| f.value_repr.len()).min(s.len());
    if n == 0 {
        return s;
    }
    let end = s.offset() + s.len();
    // Include a type annotation such as `(mm)`: it is part of the value.
    let start = e.ty().map_or(end - n, |t| {
        t.span().offset().saturating_sub(1).max(s.offset())
    });
    SourceSpan::new(start.into(), end - start)
}

pub(crate) fn sub_span(outer: SourceSpan, start: usize, end: usize) -> SourceSpan {
    let start = start.min(outer.len());
    let end = end.clamp(start, outer.len());
    SourceSpan::new((outer.offset() + start).into(), (end - start).max(1))
}

pub(crate) fn arg_str(e: &KdlEntry) -> Option<&str> {
    if e.name().is_none() {
        e.value().as_string()
    } else {
        None
    }
}

pub(crate) fn spanned_name(node: &KdlNode) -> Spanned<String> {
    Spanned {
        value: node.name().value().to_owned(),
        span: node.name().span(),
    }
}

fn hint(s: Option<&str>) -> Option<String> {
    s.map(|s| format!("did you mean `{s}`?"))
}

/// Names of local definitions a node's subtree refers to.
fn refs(node: &KdlNode, names: &HashSet<String>, out: &mut Vec<String>) {
    for c in node.iter_children() {
        if names.contains(c.name().value()) {
            out.push(c.name().value().to_owned());
        }
        refs(c, names, out);
    }
}

impl<'k> Decoder<'k> {
    fn new(src: Src, known: &'k Known) -> Self {
        Self {
            src,
            errors: Vec::new(),
            known,
            local_names: HashSet::new(),
            local: HashMap::new(),
            imported: HashMap::new(),
            param_controls: HashMap::new(),
        }
    }

    pub(crate) fn err(
        &mut self,
        code: &'static str,
        span: SourceSpan,
        msg: impl Into<String>,
        label: impl Into<String>,
    ) -> &mut DocError {
        self.errors
            .push(DocError::new(&self.src, code, span, msg, label));
        let n = self.errors.len() - 1;
        &mut self.errors[n]
    }

    pub(crate) fn text(&self) -> &str {
        self.src.inner()
    }

    /// The node's first argument as a name (`define lawn`), or an error.
    fn name_arg(&mut self, node: &KdlNode, what: &str) -> Option<Spanned<String>> {
        if let Some(e) = node.entries().first()
            && let Some(s) = arg_str(e)
        {
            return Some(Spanned {
                value: s.to_owned(),
                span: value_span(e),
            });
        }
        let kw = node.name().value().to_owned();
        self.err(
            "dt::missing-name",
            node.name().span(),
            format!("`{kw}` needs a name"),
            format!("this {what} has no name"),
        )
        .help = Some(format!("write `{kw} my-name …`"));
        None
    }

    fn document(&mut self, kdl: &KdlDocument) -> Document {
        let mut doc = Document {
            uses: Vec::new(),
            levels: Vec::new(),
            definitions: Vec::new(),
            printer: None,
            edges: None,
        };
        for n in kdl.nodes() {
            if n.name().value() == "define"
                && let Some(name) = n.entries().first().and_then(arg_str)
            {
                self.local_names.insert(name.to_owned());
            }
        }
        // `use` lines and file-level levels first: definitions see them.
        let mut file_levels: Vec<String> = Vec::new();
        for n in kdl.nodes() {
            match n.name().value() {
                "use" => {
                    if let Some(u) = self.use_line(n) {
                        self.import(&u);
                        doc.uses.push(u);
                    }
                }
                "levels" => {
                    let l = self.levels(n, &Ctx::default());
                    file_levels.extend(l.iter().map(|l| l.name.value.clone()));
                    doc.levels.extend(l);
                }
                _ => {}
            }
        }

        // Defines in call order, so each call knows its callee's
        // parameters; then tiles.  The model keeps file order.
        let defs: Vec<(usize, &KdlNode)> = kdl
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.name().value(), "define" | "tile"))
            .collect();
        let mut order: Vec<usize> = Vec::new();
        let mut state: HashMap<usize, u8> = HashMap::new();
        for (i, _) in &defs {
            self.visit(*i, &defs, &mut state, &mut order);
        }
        let mut decoded: Vec<(usize, Definition)> = Vec::new();
        for i in order {
            let Some((_, node)) = defs.iter().find(|(j, _)| *j == i) else {
                continue;
            };
            if let Some(d) = self.definition(node, &file_levels) {
                if matches!(d.kind, DefKind::Define) {
                    self.local.insert(d.name.value.clone(), DefInfo::from(&d));
                }
                decoded.push((i, d));
            }
        }
        decoded.sort_by_key(|(i, _)| *i);
        doc.definitions = decoded.into_iter().map(|(_, d)| d).collect();
        if doc
            .definitions
            .iter()
            .filter(|d| d.header().is_some())
            .count()
            > 1
            && let Some(second) = doc
                .definitions
                .iter()
                .filter(|d| d.header().is_some())
                .nth(1)
        {
            let span = second.name.span;
            self.err("dt::tile", span, "a file holds one tile", "a second tile")
                .help = Some("put each tile in its own file".to_owned());
        }

        for n in kdl.nodes() {
            match n.name().value() {
                "use" | "levels" | "define" | "tile" => {}
                "printer" => doc.printer = self.printer(n),
                "edges" => doc.edges = self.edges(n, &file_levels),
                other => {
                    let help = crate::looks::renamed(other)
                        .map(str::to_owned)
                        .or_else(|| {
                            hint(did_you_mean(
                                other,
                                ["use", "levels", "define", "tile", "printer", "edges"],
                            ))
                        })
                        .or_else(|| {
                            schema::look(other)
                                .map(|_| "looks go inside a tile's `stack { … }`".to_owned())
                        });
                    self.err(
                        "dt::unknown-node",
                        n.name().span(),
                        format!("unknown top-level node `{other}`"),
                        "not use, levels, define, tile, printer or edges",
                    )
                    .help = help;
                }
            }
        }
        doc
    }

    /// Depth-first order of definitions, callees first; reports loops.
    fn visit(
        &mut self,
        i: usize,
        defs: &[(usize, &KdlNode)],
        state: &mut HashMap<usize, u8>,
        order: &mut Vec<usize>,
    ) {
        match state.get(&i) {
            Some(2) => return,
            Some(1) => {
                if let Some((_, n)) = defs.iter().find(|(j, _)| *j == i) {
                    let span = n.entries().first().map_or(n.span(), value_span);
                    self.err(
                        "dt::loop",
                        span,
                        "this definition calls itself",
                        "part of a loop of definitions",
                    )
                    .help =
                        Some("a definition may call others, but never round to itself".to_owned());
                }
                return;
            }
            _ => {}
        }
        state.insert(i, 1);
        if let Some((_, n)) = defs.iter().find(|(j, _)| *j == i) {
            let mut called = Vec::new();
            refs(n, &self.local_names, &mut called);
            for name in called {
                if let Some((j, _)) = defs.iter().find(|(_, d)| {
                    d.name().value() == "define"
                        && d.entries().first().and_then(arg_str) == Some(name.as_str())
                }) {
                    self.visit(*j, defs, state, order);
                }
            }
        }
        state.insert(i, 2);
        order.push(i);
    }

    pub(crate) fn use_line(&mut self, node: &KdlNode) -> Option<Use> {
        let Some(e) = node.entries().first().filter(|e| arg_str(e).is_some()) else {
            self.err("dt::use", node.span(), "`use` needs a path", "no path")
                .help = Some("write `use \"parts/grass\" lawn`".to_owned());
            return None;
        };
        let span = value_span(e);
        let raw = arg_str(e).unwrap_or_default();
        let trimmed = raw.strip_suffix(".dtile").unwrap_or(raw);
        let source = if let Some(p) = trimmed.strip_prefix("lib:") {
            Source::Library(p.to_owned())
        } else if trimmed.starts_with("./") || trimmed.starts_with("../") {
            Source::Project(trimmed.to_owned())
        } else if trimmed.contains(':') || trimmed.starts_with('/') || trimmed.is_empty() {
            self.err("dt::use", span, "`use` takes a library or relative path", "not a path here")
                .help = Some(
                "`parts/grass` is the bundled library (`lib:parts/grass` says so explicitly); `./mine` is your project".to_owned(),
            );
            return None;
        } else {
            Source::Library(trimmed.to_owned())
        };
        let mut names = Vec::new();
        for n in &node.entries()[1..] {
            match arg_str(n) {
                Some(s) => names.push(Spanned {
                    value: s.to_owned(),
                    span: value_span(n),
                }),
                None => {
                    self.err(
                        "dt::use",
                        n.span(),
                        "`use` lists the definitions it brings in, by name",
                        "not a name",
                    );
                }
            }
        }
        if node.children().is_some() {
            self.err("dt::use", node.span(), "`use` has no block", "here");
        }
        Some(Use {
            path: Spanned {
                value: raw.to_owned(),
                span,
            },
            source,
            names,
            span: node.span(),
        })
    }

    /// Bring a `use` line's names into scope, checking each exists.
    fn import(&mut self, u: &Use) {
        let key = u.key().to_owned();
        let Some(file) = self.known.file(&key) else {
            let keys: Vec<&str> = self.known.file_keys().collect();
            let help = hint(did_you_mean(&key, keys.iter().copied())).unwrap_or_else(|| {
                "the path is from the library root, without `.dtile`, as in `parts/grass`"
                    .to_owned()
            });
            self.err(
                "dt::use",
                u.path.span,
                format!("no file `{key}` is loaded"),
                "not found",
            )
            .help = Some(help);
            return;
        };
        if u.names.is_empty() {
            let all: Vec<&str> = file.keys().map(String::as_str).collect();
            self.err(
                "dt::use",
                u.path.span,
                "`use` names the definitions it brings in",
                "no names",
            )
            .help = Some(format!(
                "write `use \"{}\" {}`",
                u.path.value,
                all.join(" ")
            ));
            return;
        }
        for n in &u.names {
            if let Some(info) = file.get(&n.value) {
                self.imported
                    .insert(n.value.clone(), (info.clone(), key.clone()));
            } else {
                let help = hint(did_you_mean(&n.value, file.keys().map(String::as_str)))
                    .unwrap_or_else(|| {
                        format!(
                            "`{key}` defines: {}",
                            file.keys()
                                .map(String::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    });
                self.err(
                    "dt::use",
                    n.span,
                    format!("`{key}` has no definition `{}`", n.value),
                    "not in that file",
                )
                .help = Some(help);
            }
        }
    }

    fn levels(&mut self, node: &KdlNode, ctx: &Ctx) -> Vec<Level> {
        let mut out = Vec::new();
        // Levels may refer to each other and to the standard ones.
        let mut inner = ctx.clone();
        inner
            .levels
            .extend(self.known.levels().iter().map(|(n, _)| n.clone()));
        inner.levels.extend(
            node.entries()
                .iter()
                .filter_map(|e| e.name())
                .map(|i| i.value().to_owned()),
        );
        inner
            .levels
            .extend(node.iter_children().map(|c| c.name().value().to_owned()));
        let mut add = |this: &mut Self, name: Spanned<String>, e: &KdlEntry| {
            if Edge::parse(&name.value).is_some() && name.value != "edge" {
                this.err(
                    "dt::levels",
                    name.span,
                    format!("`{}` is a tile edge, not a level name", name.value),
                    "an edge word",
                );
                return;
            }
            if let Some(value) = this.num(e, Axis::Level, None, "levels", &name.value, &inner) {
                out.push(Level { name, value });
            }
        };
        for e in node.entries() {
            match e.name() {
                Some(n) => {
                    let name = Spanned {
                        value: n.value().to_owned(),
                        span: n.span(),
                    };
                    add(self, name, e);
                }
                None => {
                    self.err("dt::levels", e.span(), "levels are named", "no name")
                        .help = Some("write `levels foot=4`, in mm".to_owned());
                }
            }
        }
        for child in node.iter_children() {
            match child.entries() {
                [e] if e.name().is_none() => add(self, spanned_name(child), e),
                _ => {
                    self.err(
                        "dt::levels",
                        child.span(),
                        "a level takes one value",
                        "here",
                    )
                    .help = Some("write `water 2.6`, in mm".to_owned());
                }
            }
        }
        out
    }

    fn definition(&mut self, node: &KdlNode, file_levels: &[String]) -> Option<Definition> {
        let is_tile = node.name().value() == "tile";
        let name = self.name_arg(node, if is_tile { "tile" } else { "definition" })?;
        let kind = if is_tile {
            DefKind::Tile(self.tile_header(node))
        } else {
            for e in node.entries().iter().skip(1) {
                self.err(
                    "dt::define",
                    e.span(),
                    "`define` takes only a name",
                    "unexpected",
                )
                .help = Some("parameters go inside: `param length 10`".to_owned());
            }
            DefKind::Define
        };
        let mut def = Definition {
            name,
            kind,
            about: None,
            params: Vec::new(),
            levels: Vec::new(),
            lines: Vec::new(),
            regions: Vec::new(),
            stack: Vec::new(),
            summary: Kind::Surface,
            thing: false,
            span: node.span(),
        };
        let size = def.header().map(|h| (h.cols, h.rows));
        let Some(body) = node.children() else {
            return Some(def);
        };

        // Names first, so references don't depend on order.
        let mut ctx = Ctx {
            levels: self
                .known
                .levels()
                .iter()
                .map(|(n, _)| n.clone())
                .chain(file_levels.iter().cloned())
                .collect(),
            size,
            in_tile: is_tile,
            ..Ctx::default()
        };
        for n in body.nodes() {
            let first = n.entries().first().and_then(arg_str).map(str::to_owned);
            match (n.name().value(), first) {
                ("param", Some(p)) => ctx.params.push(p),
                ("line", Some(l)) => ctx.lines.push(l),
                ("region", Some(r)) => ctx.regions.push(r),
                ("levels", _) => {
                    ctx.levels.extend(
                        n.entries()
                            .iter()
                            .filter_map(|e| e.name())
                            .map(|i| i.value().to_owned()),
                    );
                    ctx.levels
                        .extend(n.iter_children().map(|c| c.name().value().to_owned()));
                }
                _ => {}
            }
        }

        self.param_controls.clear();
        let step = format!("{} {}", node.name().value(), def.name.value);
        let mut stacks = 0;
        let mut params: Vec<&KdlNode> = Vec::new();
        for n in body.nodes() {
            match n.name().value() {
                "param" if !is_tile => params.push(n),
                "param" => {
                    self.err(
                        "dt::param",
                        n.name().span(),
                        "a tile has no parameters",
                        "in a tile",
                    )
                    .help = Some(
                        "parameters belong to a `define`; a tile writes its values directly"
                            .to_owned(),
                    );
                }
                "about" => match n.entries().first().and_then(arg_str) {
                    Some(t) => def.about = Some(t.trim().to_owned()),
                    None => {
                        self.err("dt::about", n.span(), "`about` takes one string", "here");
                    }
                },
                "levels" => {
                    let l = self.levels(n, &ctx);
                    def.levels.extend(l);
                }
                "line" => {
                    if let Some(l) = self.line(n, &ctx) {
                        def.lines.push(l);
                    }
                }
                "region" => {
                    if let Some(r) = self.region(n, &ctx) {
                        def.regions.push(r);
                    }
                }
                "stack" => {
                    stacks += 1;
                    if stacks > 1 {
                        self.err(
                            "dt::stack",
                            n.name().span(),
                            "a definition has one stack",
                            "a second stack",
                        )
                        .help = Some("put every entry in the first `stack { … }`".to_owned());
                    }
                    let path = vec![step.clone(), "stack".to_owned()];
                    def.stack.extend(self.entries(n, &ctx, &path));
                }
                other => {
                    let words = ["param", "about", "levels", "line", "region", "stack"];
                    let help = crate::looks::renamed(other)
                        .map(str::to_owned)
                        .or_else(|| hint(did_you_mean(other, words)))
                        .or_else(|| {
                            (schema::look(other).is_some() || self.is_definition(other))
                                .then(|| format!("`{other}` goes inside `stack {{ … }}`"))
                        });
                    self.err(
                        "dt::unknown-node",
                        n.name().span(),
                        format!(
                            "unknown node `{other}` in a {}",
                            if is_tile { "tile" } else { "definition" }
                        ),
                        "not about, levels, line, region or stack",
                    )
                    .help = help;
                }
            }
        }
        // Parameters last: their defaults are typed by the controls they fill.
        for n in params {
            if let Some(p) = self.param(n, &ctx) {
                def.params.push(p);
            }
        }
        def.summary = Kind::of(def.stack.iter().map(|e| e.kind));
        def.thing = def.stack.len() == 1 && self.is_thing(&def.stack[0]);
        Some(def)
    }

    pub(crate) fn is_definition(&self, name: &str) -> bool {
        self.local_names.contains(name) || self.imported.contains_key(name)
    }

    /// A definition's info, if it is visible and decoded.
    pub(crate) fn def_info(&self, name: &str) -> Option<&DefInfo> {
        self.local
            .get(name)
            .or_else(|| self.imported.get(name).map(|(i, _)| i))
    }

    fn tile_header(&mut self, node: &KdlNode) -> TileHeader {
        let mut h = TileHeader {
            cols: 1,
            rows: 1,
            seed: None,
        };
        for e in node.entries().iter().skip(1) {
            let Some(key) = e.name() else {
                self.err(
                    "dt::tile",
                    e.span(),
                    "a tile takes `size=` and `seed=`",
                    "unexpected value",
                )
                .help = Some("write `tile my-tile size=\"2x1\" seed=1 { … }`".to_owned());
                continue;
            };
            match key.value() {
                "size" => {
                    let parsed = e.value().as_string().and_then(|s| {
                        let (c, r) = s.split_once('x')?;
                        let c: u32 = c.trim().parse().ok()?;
                        let r: u32 = r.trim().parse().ok()?;
                        (c > 0 && r > 0).then_some((c, r))
                    });
                    match parsed {
                        Some((c, r)) if c > MAX_SQUARES || r > MAX_SQUARES => {
                            self.err(
                                "dt::tile",
                                value_span(e),
                                format!(
                                    "a tile is at most {MAX_SQUARES} squares each way, not {c}x{r}"
                                ),
                                "too big",
                            )
                            .help = Some(
                                "build a large board as several tiles that butt at their edges"
                                    .to_owned(),
                            );
                        }
                        Some((c, r)) => (h.cols, h.rows) = (c, r),
                        None => {
                            self.err(
                                "dt::tile",
                                value_span(e),
                                "a tile's size is columns by rows of squares",
                                "not a size",
                            )
                            .help =
                                Some("write `size=\"2x1\"`: two squares across, one up".to_owned());
                        }
                    }
                }
                "seed" => match seed_value(e) {
                    Ok(i) => h.seed = Some(i),
                    Err((msg, label)) => {
                        self.err("dt::seed", value_span(e), msg, label);
                    }
                },
                other => {
                    let help = crate::looks::renamed(other)
                        .map(str::to_owned)
                        .or_else(|| hint(did_you_mean(other, ["size", "seed"])));
                    self.err(
                        "dt::unknown-control",
                        key.span(),
                        format!("a tile has no `{other}`"),
                        "unknown",
                    )
                    .help = help;
                }
            }
        }
        h
    }

    fn param(&mut self, n: &KdlNode, ctx: &Ctx) -> Option<Param> {
        let name = self.name_arg(n, "parameter")?;
        let control = self.param_controls.get(&name.value).copied();
        let default = match n.entries() {
            [_] => None,
            [_, e] if e.name().is_none() => match control {
                Some(k) => self.value(e, k, "param", &name.value, ctx),
                None => self.untyped(e),
            },
            _ => {
                self.err(
                    "dt::param",
                    n.span(),
                    "a parameter is `param name default`",
                    "here",
                )
                .help = Some("write `param length 10`".to_owned());
                None
            }
        };
        Some(Param {
            name,
            default,
            control,
        })
    }

    fn line(&mut self, n: &KdlNode, ctx: &Ctx) -> Option<Line> {
        let name = self.name_arg(n, "line")?;
        if Edge::parse(&name.value).is_some() || Corner::parse(&name.value).is_some() {
            self.err(
                "dt::line",
                name.span,
                format!("`{}` is a tile edge or corner", name.value),
                "taken by the tile",
            )
            .help = Some("give the line its own name, such as `trail`".to_owned());
        }
        let mut line = Line {
            name,
            through: Vec::new(),
            width: None,
            wobble: None,
            span: n.span(),
        };
        let mut set = |this: &mut Self, key: &str, kspan: SourceSpan, e: &KdlEntry| {
            let slot = match key {
                "width" => &mut line.width,
                "wobble" => &mut line.wobble,
                other => {
                    let help = crate::looks::renamed(other)
                        .map(str::to_owned)
                        .or_else(|| hint(did_you_mean(other, ["width", "wobble", "through"])));
                    this.err(
                        "dt::unknown-control",
                        kspan,
                        format!("a line has no `{other}`"),
                        "unknown",
                    )
                    .help = help;
                    return;
                }
            };
            if slot.is_some() {
                this.err(
                    "dt::duplicate",
                    kspan,
                    format!("`{key}` is set twice"),
                    "again here",
                );
                return;
            }
            *slot = this.num(e, Axis::Mm, None, "line", key, ctx);
        };
        for e in &n.entries()[1..] {
            match e.name() {
                Some(k) => set(self, k.value(), k.span(), e),
                None => {
                    self.err(
                        "dt::line",
                        e.span(),
                        "a line takes name=value controls",
                        "no name",
                    );
                }
            }
        }
        for c in n.iter_children() {
            if c.name().value() == "through" {
                for e in c.entries() {
                    if let Some(p) = self.point(e, "through", ctx) {
                        line.through.push(p);
                    }
                }
                if c.entries().len() < 2 {
                    self.err(
                        "dt::line",
                        c.span(),
                        "a line passes through at least two points",
                        format!("{} point(s)", c.entries().len()),
                    )
                    .help = Some(
                        "write `through south \"0.65,1\" north`: points, corners or edge middles"
                            .to_owned(),
                    );
                }
            } else if let [e] = c.entries() {
                set(self, c.name().value(), c.name().span(), e);
            } else {
                let other = c.name().value();
                self.err(
                    "dt::unknown-node",
                    c.name().span(),
                    format!("a line has no `{other}`"),
                    "unknown",
                )
                .help = hint(did_you_mean(other, ["through", "width", "wobble"]));
            }
        }
        if line.through.is_empty() && !n.iter_children().any(|c| c.name().value() == "through") {
            self.err(
                "dt::line",
                line.name.span,
                "a line needs `through`",
                "no points",
            )
            .help = Some("write `line trail { through south \"0.65,1\" north }`".to_owned());
        }
        Some(line)
    }

    fn region(&mut self, n: &KdlNode, ctx: &Ctx) -> Option<Region> {
        let name = self.name_arg(n, "region")?;
        let mut region = Region {
            name,
            bounded_by: Vec::new(),
            fills: Vec::new(),
            span: n.span(),
        };
        for c in n.iter_children() {
            match c.name().value() {
                "bounded-by" => {
                    for e in c.entries() {
                        let span = value_span(e);
                        match arg_str(e) {
                            Some(l) if ctx.lines.iter().any(|x| x == l) => {
                                region.bounded_by.push(Spanned {
                                    value: l.to_owned(),
                                    span,
                                });
                            }
                            Some(l) => {
                                let help =
                                    hint(did_you_mean(l, ctx.lines.iter().map(String::as_str)))
                                        .unwrap_or_else(|| {
                                            format!("draw it first: `line {l} {{ through … }}`")
                                        });
                                self.err(
                                    "dt::unknown-name",
                                    span,
                                    format!("there is no line called `{l}`"),
                                    "not a line here",
                                )
                                .help = Some(help);
                            }
                            None => {
                                self.err(
                                    "dt::region",
                                    span,
                                    "`bounded-by` names lines",
                                    "not a name",
                                );
                            }
                        }
                    }
                }
                "fill" => {
                    for e in c.entries() {
                        if let Some(p) = self.point(e, "fill", ctx) {
                            region.fills.push(p);
                        }
                    }
                    if c.entries().is_empty() {
                        self.err("dt::region", c.span(), "`fill` takes a point", "no point")
                            .help = Some("write `fill \"0.15,1\"`".to_owned());
                    }
                }
                other => {
                    let help = crate::looks::renamed(other)
                        .map(str::to_owned)
                        .or_else(|| hint(did_you_mean(other, ["bounded-by", "fill"])));
                    self.err(
                        "dt::unknown-node",
                        c.name().span(),
                        format!("a region has no `{other}`"),
                        "unknown",
                    )
                    .help = help;
                }
            }
        }
        if region.bounded_by.is_empty()
            && !n.iter_children().any(|c| c.name().value() == "bounded-by")
        {
            self.err(
                "dt::region",
                region.name.span,
                "a region divides the tile, so it needs lines",
                "no `bounded-by`",
            )
            .help = Some(
                "the whole tile is already the default area: drop the region and its `in=`, or write `bounded-by trail`"
                    .to_owned(),
            );
        }
        if region.fills.is_empty() && !n.iter_children().any(|c| c.name().value() == "fill") {
            self.err(
                "dt::region",
                region.name.span,
                "a region needs at least one fill point",
                "no fill",
            )
            .help = Some("the paint bucket starts from `fill \"x,y\"`".to_owned());
        }
        Some(region)
    }

    /// A point argument, checked against the tile's size.
    pub(crate) fn point(
        &mut self,
        e: &KdlEntry,
        control: &str,
        ctx: &Ctx,
    ) -> Option<Spanned<Point>> {
        let span = value_span(e);
        let Some(s) = e.value().as_string() else {
            self.err(
                "dt::point",
                span,
                format!("`{control}` is a point"),
                "not a point",
            )
            .help = Some(
                "a point is a quoted pair \"x,y\" in squares, a corner (north-west, …) or an edge (north, …)"
                    .to_owned(),
            );
            return None;
        };
        let p = match crate::place::parse_point(s) {
            Ok(p) => p,
            Err(err) => {
                let inner = self.inner_at(e, s);
                self.err(
                    "dt::point",
                    sub_span(span, inner + err.start, inner + err.end),
                    format!("can't read `{control}`: {}", err.message),
                    "here",
                )
                .help = Some(
                    "write a point as \"x,y\" in squares from the south-west corner, y north"
                        .to_owned(),
                );
                return None;
            }
        };
        if let Some((cols, rows)) = ctx.size
            && let Ok(((x0, x1), (y0, y1))) = p.bounds(f64::from(cols), f64::from(rows))
        {
            let (c, r) = (f64::from(cols), f64::from(rows));
            if x0 < 0.0 || x1 > c || y0 < 0.0 || y1 > r || x0 > x1 || y0 > y1 {
                self.err(
                    "dt::point",
                    span,
                    format!("`{p}` is off the tile"),
                    "outside the tile",
                )
                .help = Some(format!(
                    "this tile is {cols}x{rows}: x runs 0 to {cols}, y 0 to {rows}, low end first"
                ));
                return None;
            }
        }
        Some(Spanned { value: p, span })
    }

    /// Where a string's text starts inside its written form (past a quote).
    pub(crate) fn inner_at(&self, e: &KdlEntry, s: &str) -> usize {
        let repr = e.format().map_or("", |f| f.value_repr.as_str());
        let ty = e.ty().map_or(0, |t| t.value().len() + 2);
        ty + repr.find(s).unwrap_or(0)
    }

    fn printer(&mut self, n: &KdlNode) -> Option<Printer> {
        let name = self.name_arg(n, "printer")?;
        let mut vals: [Option<Spanned<f64>>; 4] = [None, None, None, None];
        const KEYS: [&str; 4] = ["line", "layer", "overhang", "gap"];
        let mut set = |this: &mut Self, key: &str, kspan: SourceSpan, e: &KdlEntry| {
            let Some(i) = KEYS.iter().position(|k| *k == key) else {
                this.err(
                    "dt::unknown-control",
                    kspan,
                    format!("a printer has no `{key}`"),
                    "unknown",
                )
                .help = hint(did_you_mean(key, KEYS))
                    .or_else(|| Some(format!("a printer takes: {}", KEYS.join(", "))));
                return;
            };
            let span = value_span(e);
            let v = match e.value() {
                kdl::KdlValue::Integer(i) => i.to_string().parse().ok(),
                kdl::KdlValue::Float(f) => Some(*f),
                _ => None,
            };
            let max = if key == "overhang" { 90.0 } else { 5.0 };
            match v {
                Some(v) if v > 0.0 && v < max => vals[i] = Some(Spanned { value: v, span }),
                _ => {
                    this.err(
                        "dt::printer",
                        span,
                        format!("`{key}` is a positive number below {max}"),
                        "not that",
                    );
                }
            }
        };
        for e in &n.entries()[1..] {
            if let Some(k) = e.name() {
                set(self, k.value(), k.span(), e);
            }
        }
        for c in n.iter_children() {
            if let [e] = c.entries() {
                set(self, c.name().value(), c.name().span(), e);
            } else {
                self.err(
                    "dt::printer",
                    c.span(),
                    "a printer setting takes one number",
                    "here",
                );
            }
        }
        let missing: Vec<&str> = KEYS
            .iter()
            .zip(&vals)
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| *k)
            .collect();
        if !missing.is_empty() {
            self.err(
                "dt::printer",
                name.span,
                format!("this printer has no {}", missing.join(", ")),
                "incomplete",
            )
            .help = Some("a printer gives line, layer, overhang and gap".to_owned());
            return None;
        }
        let [Some(line), Some(layer), Some(overhang), Some(gap)] = vals else {
            return None;
        };
        Some(Printer {
            name,
            line,
            layer,
            overhang,
            gap,
        })
    }

    fn edges(&mut self, n: &KdlNode, file_levels: &[String]) -> Option<Edges> {
        let ctx = Ctx {
            levels: self
                .known
                .levels()
                .iter()
                .map(|(n, _)| n.clone())
                .chain(file_levels.iter().cloned())
                .collect(),
            ..Ctx::default()
        };
        let mut rules = Vec::new();
        for c in n.iter_children() {
            let surface = spanned_name(c);
            if surface.value != "ground" && surface.value != "water" {
                self.err(
                    "dt::edges",
                    surface.span,
                    format!("the edge contract has no rule for `{}`", surface.value),
                    "unknown",
                )
                .help = Some("it covers `ground` and `water`".to_owned());
                continue;
            }
            let level = c
                .entries()
                .iter()
                .find(|e| e.name().is_some_and(|k| k.value() == "level"));
            match level {
                Some(e) => {
                    if let Some(level) = self.num(e, Axis::Level, None, "edges", "level", &ctx) {
                        rules.push(EdgeRule { surface, level });
                    }
                }
                None => {
                    self.err(
                        "dt::edges",
                        c.span(),
                        "an edge rule says where the surface meets the edge",
                        "no level",
                    )
                    .help = Some("write `ground level=edge`".to_owned());
                }
            }
        }
        Some(Edges {
            rules,
            span: n.span(),
        })
    }
}

/// A `seed=` value: a whole number from 0 to `i64::MAX`, or why not (the
/// message and its label).
pub(crate) fn seed_value(e: &KdlEntry) -> Result<i64, (String, &'static str)> {
    match e.value().as_integer() {
        Some(i) => i64::try_from(i).ok().filter(|i| *i >= 0).ok_or_else(|| {
            (
                format!("a seed is a whole number from 0 to {}, not {i}", i64::MAX),
                "out of range",
            )
        }),
        None => Err(("`seed` is a whole number".to_owned(), "not a whole number")),
    }
}
