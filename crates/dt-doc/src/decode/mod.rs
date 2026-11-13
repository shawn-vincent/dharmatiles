//! KDL v2 node tree into the typed model, by hand: one function per node
//! type, so every error message is ours and points at the right text.
//! Decoding goes on past a mistake, so a file's mistakes come together.

mod entry;
mod heights;
mod machine;
pub(crate) mod syntax;
mod units;
mod value;
pub use units::point_text;
pub use value::check_text_in;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::{NamedSource, SourceSpan};

use crate::dim::Dim;
use crate::error::{DocError, Src};
use crate::expr;
use crate::known::{DefInfo, Known};
use crate::model::{
    Base, DefKind, Definition, Document, Line, Param, Printer, Region, Source, Spanned, TileHeader,
    Use, Within,
};
use crate::place::{Corner, Edge, Point};
use crate::schema::{self, Axis, Control, Kind, MAX_SQUARES, did_you_mean};
use heights::{Mode, PlaceKind};

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
    // the header line first: a format this program doesn't read stops here,
    // since the rest of the file may not mean what this decoder thinks
    let (header, header_errors) = crate::header::read(&src, &kdl);
    if header_errors.iter().any(|e| e.code == "dt::format-newer") {
        return Err(header_errors);
    }
    // this decoder reads format 1 only
    if header
        .as_ref()
        .is_none_or(|h| h.format < crate::header::FORMAT)
    {
        let mut errors = header_errors;
        errors.push(crate::header::format_old(&src, &kdl, header.as_ref()));
        return Err(errors);
    }
    let mut d = Decoder::new(src, known);
    d.errors.extend(header_errors);
    let mut doc = d.document(&kdl);
    doc.header = header;
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
    pub lines: Vec<String>,
    pub regions: Vec<String>,
    /// The heights language's constants visible here, with what each
    /// measures: the library's, then the tile's own in order.
    pub consts: Vec<(String, Dim)>,
    /// Format 1: each plain name's value as written, units and all, for the
    /// points that read it (a point is worked out in squares, which the
    /// lowered value, in design millimetres, no longer says).
    pub raw: Vec<(String, crate::expr::Expr)>,
    /// Every place by name: the built-in ones and the declared ones.
    pub places: Vec<(String, PlaceKind)>,
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
    /// The `base` line of the stack last decoded.
    pub stack_base: Option<Base>,
    /// The `let`s of the stack block last decoded (format 1).
    pub stack_consts: Vec<crate::model::Const>,
    /// Format 1: the named region (or drawn line) the entry being read acts
    /// in, where a pin is written: its points are in that region's frame,
    /// whose box is known only once `check` has the region tree, so they are
    /// kept as written ([`crate::framed::Framed`]) and worked out there.
    pub frame_region: Option<String>,
    /// Format 1: the last height read, as written (units unlowered).
    pub last_raw: Option<crate::expr::Expr>,
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
    pub(crate) fn new(src: Src, known: &'k Known) -> Self {
        Self {
            src,
            errors: Vec::new(),
            known,
            local_names: HashSet::new(),
            local: HashMap::new(),
            imported: HashMap::new(),
            param_controls: HashMap::new(),
            stack_base: None,
            stack_consts: Vec::new(),
            frame_region: None,
            last_raw: None,
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
            definitions: Vec::new(),
            printer: None,
            consts: Vec::new(),
            defaults: Vec::new(),
            header: None,
        };
        for n in kdl.nodes() {
            if n.name().value() == "define"
                && let Some(name) = n.entries().first().and_then(arg_str)
            {
                self.local_names.insert(name.to_owned());
            }
        }
        // `use` lines first: definitions see them.
        for n in kdl.nodes() {
            if n.name().value() == "use"
                && let Some(u) = self.use_line(n)
            {
                self.import(&u);
                doc.uses.push(u);
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
            if let Some(d) = self.definition(node) {
                if matches!(d.kind, DefKind::Define) {
                    self.local.insert(d.name.value.clone(), DefInfo::from(&d));
                }
                decoded.push((i, d));
            }
        }
        decoded.sort_by_key(|(i, _)| *i);
        doc.definitions = decoded.into_iter().map(|(_, d)| d).collect();

        // File-level constants (the library's heights), then the defaults
        // that read them.
        // (a file's own `let`s are declarations of its own, so they are not
        // checked against the library's: the library's heights file is one)
        let mut file_ctx = Ctx {
            places: heights::builtin_places(),
            in_tile: true,
            ..Ctx::default()
        };
        for n in kdl.nodes() {
            if matches!(n.name().value(), "let" | "override")
                && let Some(c) = self.const_node(n, &mut file_ctx, &[])
            {
                doc.consts.push(c);
            }
        }
        let mut defaults_ctx = Ctx {
            consts: self.library_consts(),
            ..file_ctx
        };
        for c in &doc.consts {
            if !defaults_ctx.consts.iter().any(|(k, _)| *k == c.name.value) {
                defaults_ctx.consts.push((c.name.value.clone(), c.dim));
            }
        }
        for n in kdl.nodes() {
            if n.name().value() == "defaults" {
                let path = vec!["defaults".to_owned()];
                doc.defaults.extend(self.entries(n, &defaults_ctx, &path));
            }
        }
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
                "use" | "define" | "tile" | "let" | "override" | "defaults" => {}
                // read and checked by `header`
                crate::header::NODE => {}
                "printer" => doc.printer = self.printer(n),
                other => {
                    let help = crate::elements::camel_of(other)
                        .map(|new| format!("control names are camelCase: write `{new}`"))
                        .or_else(|| crate::elements::renamed(other).map(str::to_owned))
                        .or_else(|| {
                            hint(did_you_mean(
                                other,
                                ["use", "define", "tile", "printer", "let", "defaults"],
                            ))
                        })
                        .or_else(|| {
                            schema::element(other)
                                .map(|_| "elements go inside a tile's `stack { … }`".to_owned())
                        });
                    self.err(
                        "dt::unknown-node",
                        n.name().span(),
                        format!("unknown top-level node `{other}`"),
                        "not use, define, tile, printer, let, override or defaults",
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
        for n in node.entries().iter().skip(1) {
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

    fn definition(&mut self, node: &KdlNode) -> Option<Definition> {
        let is_tile = node.name().value() == "tile";
        // A tile is named by its file: the node is just `tile`.
        let name = if is_tile {
            Spanned {
                value: "tile".to_owned(),
                span: node.name().span(),
            }
        } else {
            self.name_arg(node, "definition")?
        };
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
            consts: Vec::new(),
            stack_consts: Vec::new(),
            points: Vec::new(),
            lines: Vec::new(),
            regions: Vec::new(),
            base: None,
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
                _ => {}
            }
        }

        // The heights language: place names first (a name may be used before
        // it is declared), then the constants in order, each using earlier ones.
        ctx.consts = self.library_consts();
        let taken = self.declare_places(body, &mut ctx);
        for n in body.nodes() {
            match n.name().value() {
                "let" | "override" => {
                    if let Some(c) = self.const_node(n, &mut ctx, &taken) {
                        def.consts.push(c);
                    }
                }
                "point" => {
                    if let Some(p) = self.point_node(n, &ctx) {
                        def.points.push(p);
                    }
                }
                _ => {}
            }
        }

        self.param_controls.clear();
        let step = if is_tile {
            "tile".to_owned()
        } else {
            format!("{} {}", node.name().value(), def.name.value)
        };
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
                "let" | "override" | "point" => {}
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
                    def.stack_consts.append(&mut self.stack_consts);
                    if let Some(b) = self.stack_base.take() {
                        def.base = Some(b);
                    }
                }
                other => {
                    let words = [
                        "param", "about", "let", "override", "point", "line", "region", "stack",
                    ];
                    let help = crate::elements::renamed(other)
                        .map(str::to_owned)
                        .or_else(|| hint(did_you_mean(other, words)))
                        .or_else(|| {
                            (schema::element(other).is_some() || self.is_definition(other))
                                .then(|| format!("`{other}` goes inside `stack {{ … }}`"))
                        });
                    self.err(
                        "dt::unknown-node",
                        n.name().span(),
                        format!(
                            "unknown node `{other}` in a {}",
                            if is_tile { "tile" } else { "definition" }
                        ),
                        "not about, let, override, point, line, region or stack",
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
        for (i, e) in node.entries().iter().enumerate() {
            let Some(key) = e.name() else {
                if i == 0 && arg_str(e).is_some() {
                    self.err(
                        "dt::tile-name",
                        e.span(),
                        "a tile takes no name: its file name is its name",
                        "remove this",
                    )
                    .help = Some(
                        "write `tile size=\"2x1\" seed=1 { … }`; rename the file to rename the tile"
                            .to_owned(),
                    );
                    continue;
                }
                self.err(
                    "dt::tile",
                    e.span(),
                    "a tile takes `size=` and `seed=`",
                    "unexpected value",
                )
                .help = Some("write `tile size=\"2x1\" seed=1 { … }`".to_owned());
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
                    let help = crate::elements::renamed(other)
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
        if name.value.starts_with('$') {
            self.err(
                    "dt::name",
                    name.span,
                    format!("`{}` is a `$` name, and a parameter is a plain name", name.value),
                    "a context name",
                )
                .help = Some("a parameter is lexical: write `param length 10`; the context's `$` names are read, not passed".to_owned());
            return None;
        }
        if crate::words::is_reserved(&name.value) {
            self.err(
                    "dt::name",
                    name.span,
                    format!("`{}` is a word of the language", name.value),
                    crate::words::kind(&name.value),
                )
                .help = Some(
                    "a plain name can't take a coordinate, function, weighting or unit; choose another name".to_owned(),
                );
            return None;
        }
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
            closed: false,
            span: n.span(),
        };
        let mut set = |this: &mut Self, key: &str, kspan: SourceSpan, e: &KdlEntry| {
            let slot = match key {
                "width" => &mut line.width,
                "wobble" => &mut line.wobble,
                other => {
                    let help = crate::elements::camel_of(other)
                        .map(|new| format!("control names are camelCase: write `{new}`"))
                        .or_else(|| crate::elements::renamed(other).map(str::to_owned))
                        .or_else(|| {
                            hint(did_you_mean(
                                other,
                                ["width", "wobble", "closed", "through"],
                            ))
                        });
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
            // a wobble is a length that may name a `let`
            *slot = this.num(e, Axis::Mm, key == "wobble", false, None, "line", key, ctx);
        };
        let mut closed: Option<bool> = None;
        let mut set_closed = |this: &mut Self, kspan: SourceSpan, e: &KdlEntry| {
            if closed.is_some() {
                this.err(
                    "dt::duplicate",
                    kspan,
                    "`closed` is set twice",
                    "again here",
                );
                return;
            }
            match e.value() {
                KdlValue::Bool(b) => closed = Some(*b),
                _ => {
                    this.err(
                        "dt::value",
                        value_span(e),
                        "`closed` is #true or #false",
                        "not a switch",
                    )
                    .help = Some("write `line ring closed=#true { through … }`".to_owned());
                }
            }
        };
        for e in n.entries().iter().skip(1) {
            match e.name() {
                Some(k) if k.value() == "closed" => set_closed(self, k.span(), e),
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
                        "write `through south \"(0.65sq, 0sq)\" north`: points, corners or edge middles"
                        .to_owned(),
                    );
                }
            } else if let [e] = c.entries()
                && c.name().value() == "closed"
            {
                set_closed(self, c.name().span(), e);
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
        line.closed = closed.unwrap_or(false);
        if line.closed
            && line.through.len() < 3
            && n.iter_children().any(|c| c.name().value() == "through")
        {
            self.err(
                "dt::line",
                line.name.span,
                "a closed line passes through at least three points",
                format!("{} point(s)", line.through.len()),
            )
            .help = Some("a loop needs a corner to turn: add a point, or open the line".to_owned());
        }
        if line.through.is_empty() && !n.iter_children().any(|c| c.name().value() == "through") {
            self.err(
                "dt::line",
                line.name.span,
                "a line needs `through`",
                "no points",
            )
            .help = Some("write `line trail { through south \"(0.65sq, 0sq)\" north }`".to_owned());
        }
        Some(line)
    }

    fn region(&mut self, n: &KdlNode, ctx: &Ctx) -> Option<Region> {
        let name = self.name_arg(n, "region")?;
        let mut region = Region {
            name,
            bounded_by: Vec::new(),
            fills: Vec::new(),
            within: Vec::new(),
            except: Vec::new(),
            span: n.span(),
        };
        for c in n.iter_children() {
            match c.name().value() {
                "within" => {
                    if let Some(w) = self.within_node(c, ctx) {
                        region.within.push(w);
                    }
                }
                "except" => {
                    for e in c.entries() {
                        let span = value_span(e);
                        match arg_str(e) {
                            Some(r) if ctx.regions.iter().any(|x| x == r) || r == "tile" => {
                                region.except.push(Spanned {
                                    value: r.to_owned(),
                                    span,
                                });
                            }
                            Some(r) => {
                                let help =
                                    hint(did_you_mean(r, ctx.regions.iter().map(String::as_str)))
                                        .unwrap_or_else(|| {
                                            format!("declare it first: `region {r} {{ … }}`")
                                        });
                                self.err(
                                    "dt::unknown-name",
                                    span,
                                    format!("there is no region called `{r}`"),
                                    "not a region here",
                                )
                                .help = Some(help);
                            }
                            None => {
                                self.err(
                                    "dt::region",
                                    span,
                                    "`except` names regions",
                                    "not a name",
                                );
                            }
                        }
                    }
                    if c.entries().is_empty() {
                        self.err("dt::region", c.span(), "`except` takes regions", "none")
                            .help = Some("write `except path`".to_owned());
                    }
                }
                "boundedBy" => {
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
                                    "`boundedBy` names lines",
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
                            .help = Some("write `fill \"(0.15sq, 0sq)\"`".to_owned());
                    }
                }
                other => {
                    let help = crate::elements::camel_of(other)
                        .map(|new| format!("control names are camelCase: write `{new}`"))
                        .or_else(|| crate::elements::renamed(other).map(str::to_owned))
                        .or_else(|| {
                            hint(did_you_mean(
                                other,
                                ["boundedBy", "fill", "within", "except"],
                            ))
                        });
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
        // a region is its bucket fills and its discs, less what it excepts; one
        // that only excepts starts from the whole tile
        let shaped = !region.within.is_empty() || !region.except.is_empty();
        let has = |w: &str| n.iter_children().any(|c| c.name().value() == w);
        if region.bounded_by.is_empty() && !has("boundedBy") && !shaped {
            self.err(
                "dt::region",
                region.name.span,
                "a region divides the tile, so it needs lines",
                "no `boundedBy`",
            )
            .help = Some(
                "the whole tile is already the default area: drop the region and its `at=`, or write `boundedBy trail`"
                    .to_owned(),
            );
        }
        if region.fills.is_empty() && !has("fill") && (!shaped || has("boundedBy")) {
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

    /// `within MM of=PLACE`: a disc round a point, a strip beside a line, a
    /// ring round a region.
    fn within_node(&mut self, c: &KdlNode, ctx: &Ctx) -> Option<Within> {
        let help = "write `within \"4.5dmm\" of=stream`: a distance round a place";
        let mut mm = None;
        let mut of = None;
        for e in c.entries() {
            match e.name().map(|k| k.value()) {
                None if mm.is_none() => {
                    mm = self
                        .height_expr(e, ctx, "a distance", Mode::Length)
                        .map(|(x, _)| x);
                    mm.as_ref()?;
                }
                Some("of") if of.is_none() => {
                    of = Some(self.place_value(e, ctx, "of")?.0);
                }
                _ => {
                    self.err(
                        "dt::region",
                        e.span(),
                        "`within` is a distance and `of=` a place",
                        "unexpected",
                    )
                    .help = Some(help.to_owned());
                    return None;
                }
            }
        }
        match (mm, of) {
            (Some(mm), Some(of)) => Some(Within { mm, of }),
            _ => {
                self.err(
                    "dt::region",
                    c.span(),
                    "`within` needs a distance and `of=`",
                    "incomplete",
                )
                .help = Some(help.to_owned());
                None
            }
        }
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
        // a built-in or declared point, by name
        if let Some((n, k)) = ctx.places.iter().find(|(n, _)| n == s.trim()) {
            if *k == PlaceKind::Point {
                return Some(Spanned {
                    value: Point::Named(n.clone()),
                    span,
                });
            }
            self.err(
                "dt::type",
                span,
                format!("`{control}` is a point, and `{n}` is {}", k.describe()),
                "not a point",
            )
            .help = Some("a point is a name from `point`, or \"(x, y)\" in squares".to_owned());
            return None;
        }
        let p = {
            let at = self.inner_at(e, s);
            self.point1(s, span, at, ctx)?
        };
        if let Some((cols, rows)) = ctx.size
            && let Ok(((x0, x1), (y0, y1))) = p.bounds(f64::from(cols), f64::from(rows))
        {
            let (c, r) = (f64::from(cols), f64::from(rows));
            if x0 < 0.0 || x1 > c || y0 < 0.0 || y1 > r || x0 > x1 || y0 > y1 {
                let (shown, help) = (
                    s.trim().to_owned(),
                    format!(
                        "this tile is {cols}x{rows} squares: across runs 0 to {cols}, down from the north 0 to {rows}"
                    ),
                );
                self.err(
                    "dt::point",
                    span,
                    format!("`{shown}` is off the tile"),
                    "outside the tile",
                )
                .help = Some(help);
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

    /// A printer's number in format 1: printed millimetres (`0.4mm`), or an
    /// angle (`55deg`).  A mistake is reported and gives `None`.
    fn printer_value(&mut self, key: &str, e: &KdlEntry) -> Option<f64> {
        use crate::units::{Base, Want, expect, lower};
        let span = value_span(e);
        let KdlValue::String(s) = e.value() else {
            self.err(
                "dt::unit",
                span,
                format!("`{key}` is written with its unit"),
                "no unit",
            )
            .help = Some(if matches!(key, "overhang" | "steepest") {
                format!("write `{key} \"55deg\"`")
            } else {
                format!("write `{key} \"0.4mm\"`")
            });
            return None;
        };
        let at = self.inner_at(e, s);
        let x = match expr::parse_units(s) {
            Ok(x) => x,
            Err(err) => {
                self.err(
                    err.code.unwrap_or("dt::printer"),
                    sub_span(span, at + err.start, at + err.end),
                    format!("can't read `{key}`: {}", err.message),
                    "here",
                )
                .help = err.help;
                return None;
            }
        };
        let want = if matches!(key, "overhang" | "steepest") {
            Want::Angle
        } else {
            Want::Length
        };
        let names = |_: &str| None;
        match expect(&x, want, &names, false)
            .and_then(|_| lower(&x, Base::Mm, self.known.units(), None))
        {
            Ok(l) => l.sample(&|_| None, 0.0).ok(),
            Err(ke) => {
                self.kind_error(&ke, span, &format!("`{key}`"));
                None
            }
        }
    }

    fn printer(&mut self, n: &KdlNode) -> Option<Printer> {
        let name = self.name_arg(n, "printer")?;
        let mut vals: [Option<Spanned<f64>>; 5] = [None, None, None, None, None];
        const KEYS: [&str; 5] = ["line", "layer", "overhang", "steepest", "gap"];
        let mut machine = crate::model::Machine::default();
        let mut set = |this: &mut Self, key: &str, kspan: SourceSpan, e: &KdlEntry| {
            let Some(i) = KEYS.iter().position(|k| *k == key) else {
                this.err(
                    "dt::unknown-control",
                    kspan,
                    format!("a printer has no `{key}`"),
                    "unknown",
                )
                .help = hint(did_you_mean(
                    key,
                    KEYS.iter().chain(machine::KEYS.iter()).copied(),
                ))
                .or_else(|| {
                    Some(format!(
                        "a printer takes: {}; and for a machine: {}",
                        KEYS.join(", "),
                        machine::KEYS.join(", ")
                    ))
                });
                return;
            };
            let span = value_span(e);
            let v = this.printer_value(key, e);
            let max = if matches!(key, "overhang" | "steepest") {
                90.0
            } else {
                5.0
            };
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
        for e in n.entries().iter().skip(1) {
            if let Some(k) = e.name() {
                set(self, k.value(), k.span(), e);
            }
        }
        for c in n.iter_children() {
            if machine::KEYS.contains(&c.name().value()) {
                self.machine_key(c, &mut machine);
            } else if let [e] = c.entries() {
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
        let floors_given = [0, 1, 2, 3, 4].map(|i| vals[i].is_some());
        // a machine (a profile with a bed, or one that starts from another)
        // may leave the floors to the default profile's
        if machine.bed.is_some() || machine.from.is_some() {
            let d = crate::model::DEFAULT_FLOORS;
            for (v, d) in vals.iter_mut().zip(d) {
                v.get_or_insert(Spanned {
                    value: d,
                    span: name.span,
                });
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
            .help = Some(
                "a printer gives line, layer, overhang, steepest and gap (or a `bed`, and then the default profile's)"
                    .to_owned(),
            );
            return None;
        }
        let [
            Some(line),
            Some(layer),
            Some(overhang),
            Some(steepest),
            Some(gap),
        ] = vals
        else {
            return None;
        };
        if steepest.value <= overhang.value {
            self.err(
                "dt::printer",
                steepest.span,
                format!(
                    "`steepest` ({}°) must be past `overhang` ({}°)",
                    steepest.value, overhang.value
                ),
                "not past it",
            );
            return None;
        }
        // past this angle a layer steps out more than one line
        let reach = (line.value / layer.value).atan().to_degrees();
        if steepest.value > reach {
            self.err(
                "dt::printer",
                steepest.span,
                format!(
                    "at {}° a {} mm layer steps out {:.2} mm, more than one {} mm line, so it would not rest on the layer below",
                    steepest.value,
                    layer.value,
                    layer.value * steepest.value.to_radians().tan(),
                    line.value
                ),
                "too steep",
            )
            .help = Some(format!(
                "with this line and layer, `steepest` is at most {reach:.1}"
            ));
            return None;
        }
        Some(Printer {
            name,
            line,
            layer,
            overhang,
            steepest,
            gap,
            floors_given,
            machine,
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
