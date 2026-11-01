//! KDL v2 node tree into the typed model, by hand: one function per node
//! type, so every error message is ours and points at the right text.

use std::collections::HashMap;
use std::sync::Arc;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::{NamedSource, SourceSpan};

use crate::error::DocError;
use crate::expr::{self, Expr, Unit};
use crate::model::{
    Callee, DefKind, Definition, Document, Entry, GroupSetting, Height, Line, Organic, Param,
    Region, Setting, Spanned, Value,
};
use crate::schema::{self, Axis, Control, ENTRY, TAGS, did_you_mean};

/// Definitions and heights visible to a file through its imports.
#[derive(Debug, Default, Clone)]
pub struct Known {
    defs: HashMap<String, Vec<String>>,
    heights: Vec<String>,
}

impl Known {
    /// Nothing known yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Make a decoded file's definitions and file-level heights visible.
    pub fn add(&mut self, doc: &Document) {
        for d in &doc.definitions {
            let params = d.params.iter().map(|p| p.name.value.clone()).collect();
            self.defs.insert(d.name.value.clone(), params);
        }
        self.heights
            .extend(doc.heights.iter().map(|h| h.name.value.clone()));
    }
}

/// Decode one KDL v2 file.  `name` labels the source in diagnostics.
///
/// # Errors
/// Every problem found, each pointing at its text.  Decoding goes on past
/// the first error so a file's mistakes are reported together.
pub fn decode(name: &str, text: &str, known: &Known) -> Result<Document, Vec<DocError>> {
    let src = Arc::new(NamedSource::new(name, text.to_owned()));
    let kdl = match KdlDocument::parse(text) {
        Ok(d) => d,
        Err(e) => return Err(syntax_errors(&src, text, &e)),
    };
    let mut d = Decoder {
        src,
        errors: Vec::new(),
        known,
        local: HashMap::new(),
    };
    let doc = d.document(&kdl);
    if d.errors.is_empty() {
        Ok(doc)
    } else {
        Err(d.errors)
    }
}

/// KDL's own parse errors, with a hint when the text is KDL v1 or a
/// range, percent or unit was left unquoted.
fn syntax_errors(src: &Arc<NamedSource<String>>, text: &str, e: &kdl::KdlError) -> Vec<DocError> {
    let mut out: Vec<DocError> = Vec::new();
    for diag in &e.diagnostics {
        let from = diag.span.offset().min(text.len());
        let to = (from + diag.span.len().max(1)).min(text.len());
        let err = match suspicious_token(text, from, to) {
            Some((at, word)) if matches!(word, "true" | "false" | "null") => DocError::new(
                src,
                "dt::syntax",
                SourceSpan::new(at.into(), word.len()),
                format!("bare `{word}` is KDL v1; dharmatiles documents are KDL v2"),
                "KDL v1 keyword",
            )
            .maybe_help(Some(format!("write #{word}"))),
            Some((at, word)) => DocError::new(
                src,
                "dt::syntax",
                SourceSpan::new(at.into(), word.len()),
                format!("`{word}` must be quoted"),
                "not a KDL number",
            )
            .maybe_help(Some(format!(
                "ranges, percents and units are written as strings: \"{word}\""
            ))),
            None => DocError::new(
                src,
                "dt::syntax",
                diag.span,
                diag.message
                    .clone()
                    .unwrap_or_else(|| "this is not valid KDL v2".to_owned()),
                diag.label.clone().unwrap_or_else(|| "here".to_owned()),
            )
            .maybe_help(diag.help.clone()),
        };
        // KDL often reports one mistake several times; keep it once.
        if !out
            .iter()
            .any(|o| o.span == err.span && o.message == err.message)
        {
            out.push(err);
        }
    }
    // A recognised mistake explains the generic reports around it.
    if out.iter().any(|e| {
        e.help
            .as_deref()
            .is_some_and(|h| h.starts_with("write #") || h.starts_with("ranges,"))
    }) {
        out.retain(|e| {
            e.help
                .as_deref()
                .is_some_and(|h| h.starts_with("write #") || h.starts_with("ranges,"))
        });
    }
    out
}

/// A token in `text[from..to]` that is a known KDL v2 mistake: a bare v1
/// keyword, or a number with `..`, `%` or a unit glued on.
fn suspicious_token(text: &str, from: usize, to: usize) -> Option<(usize, &str)> {
    let mut i = from;
    while i < to {
        let rest = &text[i..];
        let len = rest
            .find(|c: char| c.is_whitespace() || "{};=()\"".contains(c))
            .unwrap_or(rest.len());
        let word = &rest[..len];
        let numberish = word.starts_with(|c: char| c.is_ascii_digit())
            || (word.starts_with(['-', '.'])
                && word[1..].starts_with(|c: char| c.is_ascii_digit()));
        if matches!(word, "true" | "false" | "null")
            || (numberish
                && (word.contains("..")
                    || word.ends_with('%')
                    || word.ends_with(|c: char| c.is_ascii_alphabetic())))
        {
            return Some((i, word));
        }
        i += len.max(rest.chars().next().map_or(1, char::len_utf8));
    }
    None
}

/// Names visible inside one definition body.
#[derive(Default)]
struct Scope {
    params: Vec<String>,
    heights: Vec<String>,
    lines: Vec<String>,
    regions: Vec<String>,
}

struct Decoder<'k> {
    src: Arc<NamedSource<String>>,
    errors: Vec<DocError>,
    known: &'k Known,
    /// Definitions in this file: name → parameter names.
    local: HashMap<String, Vec<String>>,
}

/// The span of an entry's value (an entry's span covers `name=value`).
fn value_span(e: &KdlEntry) -> SourceSpan {
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

fn sub_span(outer: SourceSpan, start: usize, end: usize) -> SourceSpan {
    let start = start.min(outer.len());
    let end = end.clamp(start, outer.len());
    SourceSpan::new((outer.offset() + start).into(), (end - start).max(1))
}

fn arg_str(e: &KdlEntry) -> Option<&str> {
    if e.name().is_none() {
        e.value().as_string()
    } else {
        None
    }
}

impl Decoder<'_> {
    fn err(
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

    fn text(&self) -> &str {
        self.src.inner()
    }

    fn spanned_name(node: &KdlNode) -> Spanned<String> {
        Spanned {
            value: node.name().value().to_owned(),
            span: node.name().span(),
        }
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
        // Definitions in this file may call each other in any order.
        for node in kdl.nodes() {
            if matches!(node.name().value(), "define" | "tile")
                && let Some(name) = node.entries().first().and_then(arg_str)
            {
                let params = node
                    .children()
                    .map(|c| {
                        c.nodes()
                            .iter()
                            .filter(|n| n.name().value() == "param")
                            .filter_map(|n| n.entries().first().and_then(arg_str))
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                self.local.insert(name.to_owned(), params);
            }
        }

        let mut doc = Document {
            uses: Vec::new(),
            heights: Vec::new(),
            definitions: Vec::new(),
        };
        for node in kdl.nodes() {
            match node.name().value() {
                "use" => self.use_node(node, &mut doc),
                "heights" => {
                    let h = self.heights(node);
                    doc.heights.extend(h);
                }
                "define" | "tile" => {
                    if let Some(d) = self.definition(node) {
                        doc.definitions.push(d);
                    }
                }
                other => {
                    let help = did_you_mean(other, ["use", "heights", "define", "tile"])
                        .map(|s| format!("did you mean `{s}`?"))
                        .or_else(|| {
                            schema::look(other)
                                .map(|_| "looks go inside a definition's `stack { … }`".to_owned())
                        });
                    self.err(
                        "dt::unknown-node",
                        node.name().span(),
                        format!("unknown top-level node `{other}`"),
                        "not use, heights, define or tile",
                    )
                    .help = help;
                }
            }
        }
        doc
    }

    fn use_node(&mut self, node: &KdlNode, doc: &mut Document) {
        let Some(e) = node.entries().first() else {
            self.err("dt::use", node.span(), "`use` needs a path", "no path")
                .help = Some("write `use \"lib:parts/grass\"`".to_owned());
            return;
        };
        let span = value_span(e);
        match arg_str(e) {
            Some(p) if p.starts_with("lib:") || p.starts_with("./") || p.starts_with("../") => {
                doc.uses.push(Spanned {
                    value: p.to_owned(),
                    span,
                });
            }
            _ => {
                self.err(
                    "dt::use",
                    span,
                    "`use` takes a library or relative path",
                    "not lib:… or ./…",
                )
                .help = Some(
                    "`lib:` is the bundled library; `./` and `../` are your project".to_owned(),
                );
            }
        }
    }

    fn heights(&mut self, node: &KdlNode) -> Vec<Height> {
        let mut out = Vec::new();
        let scope = Scope::default();
        let mut add = |this: &mut Self, name: Spanned<String>, e: &KdlEntry| {
            if let Some(value) = this.typed_value(e, Axis::Up, "heights", &name.value, &scope) {
                out.push(Height { name, value });
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
                    self.err("dt::heights", e.span(), "heights are named", "no name")
                        .help = Some("write `heights water=0.08 bed=0.01`".to_owned());
                }
            }
        }
        for child in node.iter_children() {
            match child.entries() {
                [e] if e.name().is_none() => add(self, Self::spanned_name(child), e),
                _ => {
                    self.err(
                        "dt::heights",
                        child.span(),
                        "a named height takes one value",
                        "here",
                    )
                    .help = Some("write `water 0.08`".to_owned());
                }
            }
        }
        out
    }

    fn definition(&mut self, node: &KdlNode) -> Option<Definition> {
        let is_tile = node.name().value() == "tile";
        let name = self.name_arg(node, if is_tile { "tile" } else { "definition" })?;
        let kind = if is_tile {
            self.tile_header(node)
        } else {
            for e in node.entries().iter().skip(1) {
                self.err(
                    "dt::define",
                    e.span(),
                    "`define` takes only a name",
                    "unexpected",
                )
                .help = Some("parameters go inside: `param rocks \"2..4\"`".to_owned());
            }
            DefKind::Define
        };
        let mut def = Definition {
            name,
            kind,
            about: None,
            params: Vec::new(),
            heights: Vec::new(),
            lines: Vec::new(),
            regions: Vec::new(),
            stack: Vec::new(),
            span: node.span(),
        };
        let Some(body) = node.children() else {
            return Some(def);
        };

        // Names first, so references don't depend on order.
        let mut scope = Scope {
            heights: self.known.heights.clone(),
            ..Scope::default()
        };
        for n in body.nodes() {
            let first = n.entries().first().and_then(arg_str).map(str::to_owned);
            match (n.name().value(), first) {
                ("param", Some(p)) => scope.params.push(p),
                ("line", Some(l)) => scope.lines.push(l),
                ("region", Some(r)) => scope.regions.push(r),
                ("heights", _) => {
                    scope.heights.extend(
                        n.entries()
                            .iter()
                            .filter_map(|e| e.name())
                            .map(|i| i.value().to_owned()),
                    );
                    scope
                        .heights
                        .extend(n.iter_children().map(|c| c.name().value().to_owned()));
                }
                _ => {}
            }
        }

        for n in body.nodes() {
            match n.name().value() {
                "param" => {
                    if let Some(p) = self.param(n) {
                        def.params.push(p);
                    }
                }
                "about" => match n.entries().first().and_then(arg_str) {
                    Some(t) => def.about = Some(t.trim().to_owned()),
                    None => {
                        self.err("dt::about", n.span(), "`about` takes one string", "here");
                    }
                },
                "heights" => {
                    let h = self.heights(n);
                    def.heights.extend(h);
                }
                "line" => {
                    if let Some(l) = self.line(n) {
                        def.lines.push(l);
                    }
                }
                "region" => {
                    if let Some(r) = self.region(n, &scope) {
                        def.regions.push(r);
                    }
                }
                "stack" => {
                    for e in n.iter_children() {
                        if let Some(entry) = self.entry(e, &scope) {
                            def.stack.push(entry);
                        }
                    }
                }
                other => {
                    let words = ["param", "about", "heights", "line", "region", "stack"];
                    let help = did_you_mean(other, words)
                        .map(|s| format!("did you mean `{s}`?"))
                        .or_else(|| {
                            (schema::look(other).is_some() || self.is_definition(other))
                                .then(|| format!("`{other}` goes inside `stack {{ … }}`"))
                        });
                    self.err(
                        "dt::unknown-node",
                        n.name().span(),
                        format!("unknown node `{other}` in a definition"),
                        "not param, about, heights, line, region or stack",
                    )
                    .help = help;
                }
            }
        }
        Some(def)
    }

    fn is_definition(&self, name: &str) -> bool {
        self.local.contains_key(name) || self.known.defs.contains_key(name)
    }

    fn tile_header(&mut self, node: &KdlNode) -> DefKind {
        let (mut cols, mut rows, mut seed) = (1, 1, None);
        for e in node.entries().iter().skip(1) {
            let Some(key) = e.name() else {
                self.err(
                    "dt::tile",
                    e.span(),
                    "a tile takes `cols=`, `rows=` and `seed=`",
                    "unexpected value",
                );
                continue;
            };
            let int = e.value().as_integer();
            match key.value() {
                "cols" | "rows" => match int.and_then(|i| u32::try_from(i).ok()).filter(|i| *i > 0)
                {
                    Some(i) if key.value() == "cols" => cols = i,
                    Some(i) => rows = i,
                    None => {
                        self.err(
                            "dt::tile",
                            value_span(e),
                            format!("`{}` is a whole number of squares", key.value()),
                            "not a positive whole number",
                        );
                    }
                },
                "seed" => match int.and_then(|i| i64::try_from(i).ok()) {
                    Some(i) => seed = Some(i),
                    None => {
                        self.err(
                            "dt::tile",
                            value_span(e),
                            "`seed` is a whole number",
                            "not a whole number",
                        );
                    }
                },
                other => {
                    self.err(
                        "dt::unknown-control",
                        key.span(),
                        format!("a tile has no `{other}`"),
                        "unknown",
                    )
                    .help = did_you_mean(other, ["cols", "rows", "seed"])
                        .map(|s| format!("did you mean `{s}`?"));
                }
            }
        }
        DefKind::Tile { cols, rows, seed }
    }

    fn param(&mut self, n: &KdlNode) -> Option<Param> {
        let name = self.name_arg(n, "parameter")?;
        let default = match n.entries() {
            [_] => None,
            [_, e] if e.name().is_none() => self.untyped_value(e),
            _ => {
                self.err(
                    "dt::param",
                    n.span(),
                    "a parameter is `param name default`",
                    "here",
                );
                None
            }
        };
        Some(Param { name, default })
    }

    fn line(&mut self, n: &KdlNode) -> Option<Line> {
        let name = self.name_arg(n, "line")?;
        let mut line = Line {
            name,
            through: Vec::new(),
            organic: None,
            span: n.span(),
        };
        let scope = Scope::default();
        for c in n.iter_children() {
            match c.name().value() {
                "through" => {
                    let xs: Vec<Expr> = c
                        .entries()
                        .iter()
                        .filter_map(|e| {
                            self.typed_value(e, Axis::Across, "line", "through", &scope)
                        })
                        .filter_map(|v| match v.value {
                            Value::Expr(x) => Some(x),
                            _ => None,
                        })
                        .collect();
                    if xs.len() < 4 || xs.len() % 2 == 1 {
                        self.err(
                            "dt::line",
                            c.span(),
                            "a line passes through pairs of numbers",
                            format!("{} numbers", xs.len()),
                        )
                        .help = Some(
                            "write `through x y  x y …`, at least two points, in squares"
                                .to_owned(),
                        );
                    }
                    let mut it = xs.into_iter();
                    while let (Some(x), Some(y)) = (it.next(), it.next()) {
                        line.through.push((x, y));
                    }
                }
                "organic" => {
                    let (mut amount, mut scale) = (None, None);
                    for e in c.entries() {
                        let key = e.name().map(|k| k.value().to_owned()).unwrap_or_default();
                        let slot = match key.as_str() {
                            "amount" => &mut amount,
                            "scale" => &mut scale,
                            _ => {
                                self.err(
                                    "dt::unknown-control",
                                    e.span(),
                                    format!("`organic` has no `{key}`"),
                                    "unknown",
                                )
                                .help = did_you_mean(&key, ["amount", "scale"])
                                    .map(|s| format!("did you mean `{s}`?"));
                                continue;
                            }
                        };
                        if let Some(Spanned {
                            value: Value::Expr(x),
                            ..
                        }) = self.typed_value(e, Axis::Across, "organic", &key, &scope)
                        {
                            *slot = Some(x);
                        }
                    }
                    let zero = || Expr::Num {
                        value: 0.0,
                        unit: None,
                    };
                    line.organic = Some(Organic {
                        amount: amount.unwrap_or_else(zero),
                        scale: scale.unwrap_or_else(zero),
                    });
                }
                other => {
                    self.err(
                        "dt::unknown-node",
                        c.name().span(),
                        format!("a line has no `{other}`"),
                        "unknown",
                    )
                    .help = did_you_mean(other, ["through", "organic"])
                        .map(|s| format!("did you mean `{s}`?"));
                }
            }
        }
        if line.through.is_empty() {
            self.err(
                "dt::line",
                line.name.span,
                "a line needs `through`",
                "no points",
            )
            .help = Some("write `line name { through x y  x y }`".to_owned());
        }
        Some(line)
    }

    fn region(&mut self, n: &KdlNode, scope: &Scope) -> Option<Region> {
        let name = self.name_arg(n, "region")?;
        let mut region = Region {
            name,
            lines: Vec::new(),
            fills: Vec::new(),
            span: n.span(),
        };
        for c in n.iter_children() {
            match c.name().value() {
                "lines" => {
                    for e in c.entries() {
                        if let Some(Spanned {
                            value: Value::Word(w),
                            span,
                        }) = self.typed_value(e, Axis::Line, "region", "lines", scope)
                        {
                            region.lines.push(Spanned { value: w, span });
                        }
                    }
                }
                "fill" => {
                    let xs: Vec<Expr> = c
                        .entries()
                        .iter()
                        .filter_map(|e| self.typed_value(e, Axis::Across, "region", "fill", scope))
                        .filter_map(|v| match v.value {
                            Value::Expr(x) => Some(x),
                            _ => None,
                        })
                        .collect();
                    if xs.is_empty() || xs.len() % 2 == 1 {
                        self.err(
                            "dt::region",
                            c.span(),
                            "a fill point is two numbers",
                            "here",
                        )
                        .help = Some("write `fill x y`, in squares".to_owned());
                    }
                    let mut it = xs.into_iter();
                    while let (Some(x), Some(y)) = (it.next(), it.next()) {
                        region.fills.push((x, y));
                    }
                }
                other => {
                    self.err(
                        "dt::unknown-node",
                        c.name().span(),
                        format!("a region has no `{other}`"),
                        "unknown",
                    )
                    .help = did_you_mean(other, ["lines", "fill"])
                        .map(|s| format!("did you mean `{s}`?"));
                }
            }
        }
        if region.fills.is_empty() {
            self.err(
                "dt::region",
                region.name.span,
                "a region needs at least one fill point",
                "no fill",
            )
            .help = Some("the paint bucket starts from `fill x y`".to_owned());
        }
        Some(region)
    }

    fn entry(&mut self, node: &KdlNode, scope: &Scope) -> Option<Entry> {
        let what = Self::spanned_name(node);
        let callee = if let Some(l) = schema::look(&what.value) {
            Callee::Look(l)
        } else if self.is_definition(&what.value) {
            Callee::Definition(what.value.clone())
        } else {
            let mut names: Vec<&str> = schema::LOOKS.iter().map(|l| l.name).collect();
            names.extend(self.local.keys().map(String::as_str));
            names.extend(self.known.defs.keys().map(String::as_str));
            let help = did_you_mean(&what.value, names.iter().copied())
                .map(|s| format!("did you mean `{s}`?"))
                .unwrap_or_else(|| {
                    "a definition from another file needs `use \"…\"` at the top".to_owned()
                });
            self.err(
                "dt::unknown-look",
                what.span,
                format!("no look or definition is called `{}`", what.value),
                "unknown",
            )
            .help = Some(help);
            return None;
        };
        let mut entry = Entry {
            what,
            callee,
            region: None,
            seed: None,
            id: None,
            settings: Vec::new(),
            groups: Vec::new(),
            span: node.span(),
        };
        let owner = entry.what.value.clone();
        let (controls, groups): (&[Control], &[schema::Group]) = match entry.callee {
            Callee::Look(l) => (l.controls, l.groups),
            Callee::Definition(_) => (&[], &[]),
        };
        let params: Vec<String> = match &entry.callee {
            Callee::Definition(d) => self
                .local
                .get(d)
                .or_else(|| self.known.defs.get(d))
                .cloned()
                .unwrap_or_default(),
            Callee::Look(_) => Vec::new(),
        };

        for e in node.entries() {
            let Some(key) = e.name() else {
                self.err(
                    "dt::positional",
                    e.span(),
                    format!("`{owner}` takes name=value controls"),
                    "no name",
                )
                .help = Some("write `control=value`".to_owned());
                continue;
            };
            let key = Spanned {
                value: key.value().to_owned(),
                span: key.span(),
            };
            self.setting(&mut entry, key, e, controls, &params, scope);
        }
        for c in node.iter_children() {
            let key = Self::spanned_name(c);
            if let Some(g) = groups.iter().find(|g| g.name == key.value) {
                let gs = self.group(c, g, &owner, scope);
                entry.groups.push(gs);
                continue;
            }
            let scalar = controls.iter().any(|k| k.name == key.value)
                || params.contains(&key.value)
                || ENTRY.iter().any(|k| k.name == key.value);
            match c.entries() {
                [e] if scalar && e.name().is_none() && c.children().is_none() => {
                    self.setting(&mut entry, key, e, controls, &params, scope);
                }
                _ if scalar => {
                    self.err(
                        "dt::control-node",
                        c.span(),
                        format!("`{}` takes one value", key.value),
                        "here",
                    )
                    .help = Some(format!(
                        "write `{} value`, or `{}=value` on the `{owner}` line",
                        key.value, key.value
                    ));
                }
                _ => self.unknown_control(&owner, &key, controls, groups, &params),
            }
        }

        // Required controls, at the top and inside groups that are present.
        for k in controls.iter().filter(|k| k.required) {
            if entry.setting(k.name).is_none() {
                self.err(
                    "dt::missing-control",
                    entry.what.span,
                    format!("`{owner}` needs `{}`", k.name),
                    format!("this {owner} has no `{}`", k.name),
                )
                .help = Some(format!("add `{}=…`: {}", k.name, k.about));
            }
        }
        for gs in &entry.groups {
            let Some(g) = groups.iter().find(|g| g.name == gs.name.value) else {
                continue;
            };
            for k in g.controls.iter().filter(|k| k.required) {
                if !gs.settings.iter().any(|s| s.name.value == k.name) {
                    self.err(
                        "dt::missing-control",
                        gs.name.span,
                        format!("`{}` needs `{}`", g.name, k.name),
                        format!("no `{}`", k.name),
                    )
                    .help = Some(format!("add `{}=…`: {}", k.name, k.about));
                }
            }
        }
        Some(entry)
    }

    fn setting(
        &mut self,
        entry: &mut Entry,
        key: Spanned<String>,
        e: &KdlEntry,
        controls: &[Control],
        params: &[String],
        scope: &Scope,
    ) {
        let owner = entry.what.value.clone();
        if entry.settings.iter().any(|s| s.name.value == key.value)
            || (key.value == "self" && entry.region.is_some())
        {
            self.err(
                "dt::duplicate",
                key.span,
                format!("`{}` is set twice", key.value),
                "again here",
            );
            return;
        }
        match key.value.as_str() {
            "self" => {
                if let Some(Spanned {
                    value: Value::Word(w),
                    span,
                }) = self.typed_value(e, Axis::Region, &owner, "self", scope)
                {
                    entry.region = Some(Spanned { value: w, span });
                }
            }
            "seed" => match e.value().as_integer().and_then(|i| i64::try_from(i).ok()) {
                Some(s) => entry.seed = Some(s),
                None => {
                    self.err(
                        "dt::seed",
                        value_span(e),
                        "`seed` is a whole number",
                        "not a whole number",
                    );
                }
            },
            "id" => entry.id = e.value().as_string().map(str::to_owned),
            name => {
                if let Some(k) = controls.iter().find(|k| k.name == name) {
                    if let Some(value) = self.typed_value(e, k.axis, &owner, name, scope) {
                        entry.settings.push(Setting { name: key, value });
                    }
                } else if params.iter().any(|p| p == name) {
                    if let Some(value) = self.untyped_value(e) {
                        entry.settings.push(Setting { name: key, value });
                    }
                } else {
                    let groups = match entry.callee {
                        Callee::Look(l) => l.groups,
                        Callee::Definition(_) => &[],
                    };
                    self.unknown_control(&owner, &key, controls, groups, params);
                }
            }
        }
    }

    fn unknown_control(
        &mut self,
        owner: &str,
        key: &Spanned<String>,
        controls: &[Control],
        groups: &[schema::Group],
        params: &[String],
    ) {
        let mut names: Vec<&str> = controls.iter().map(|k| k.name).collect();
        names.extend(groups.iter().map(|g| g.name));
        names.extend(params.iter().map(String::as_str));
        names.extend(ENTRY.iter().map(|k| k.name));
        // A control that lives in a group, written at the top level.
        let in_group = groups
            .iter()
            .find(|g| g.controls.iter().any(|k| k.name == key.value))
            .map(|g| {
                format!(
                    "`{}` belongs in the `{}` group: `{} {}=…`",
                    key.value, g.name, g.name, key.value
                )
            });
        let help = in_group
            .or_else(|| {
                did_you_mean(&key.value, names.iter().copied())
                    .map(|s| format!("did you mean `{s}`?"))
            })
            .or_else(|| {
                (!names.is_empty()).then(|| format!("`{owner}` takes: {}", names.join(", ")))
            });
        self.err(
            "dt::unknown-control",
            key.span,
            format!("`{owner}` has no control `{}`", key.value),
            "unknown control",
        )
        .help = help;
    }

    fn group(
        &mut self,
        node: &KdlNode,
        g: &schema::Group,
        owner: &str,
        scope: &Scope,
    ) -> GroupSetting {
        let mut out = GroupSetting {
            name: Self::spanned_name(node),
            settings: Vec::new(),
        };
        for e in node.entries() {
            let Some(key) = e.name() else {
                self.err(
                    "dt::positional",
                    e.span(),
                    format!("`{}` takes name=value controls", g.name),
                    "no name",
                );
                continue;
            };
            let key = Spanned {
                value: key.value().to_owned(),
                span: key.span(),
            };
            let Some(k) = g.controls.iter().find(|k| k.name == key.value) else {
                let names: Vec<&str> = g.controls.iter().map(|k| k.name).collect();
                let help = did_you_mean(&key.value, names.iter().copied()).map_or_else(
                    || format!("`{}` takes: {}", g.name, names.join(", ")),
                    |s| format!("did you mean `{s}`?"),
                );
                self.err(
                    "dt::unknown-control",
                    key.span,
                    format!("`{owner} {}` has no control `{}`", g.name, key.value),
                    "unknown control",
                )
                .help = Some(help);
                continue;
            };
            if out.settings.iter().any(|s| s.name.value == key.value) {
                self.err(
                    "dt::duplicate",
                    key.span,
                    format!("`{}` is set twice", key.value),
                    "again here",
                );
                continue;
            }
            if let Some(value) =
                self.typed_value(e, k.axis, &format!("{owner} {}", g.name), k.name, scope)
            {
                out.settings.push(Setting { name: key, value });
            }
        }
        if let Some(kids) = node.children() {
            self.err(
                "dt::group",
                kids.span(),
                format!("`{}` has no children", g.name),
                "here",
            )
            .help = Some(format!(
                "write its controls on one line: `{} name=value …`",
                g.name
            ));
        }
        out
    }

    /// A value whose axis is known from the control it sets.
    fn typed_value(
        &mut self,
        e: &KdlEntry,
        axis: Axis,
        owner: &str,
        control: &str,
        scope: &Scope,
    ) -> Option<Spanned<Value>> {
        let span = value_span(e);
        let unit = match e.ty() {
            Some(t) => {
                if let Some(u) = Unit::parse(t.value()) {
                    Some(u)
                } else {
                    self.err(
                        "dt::unit",
                        t.span(),
                        format!("unknown unit `({})`", t.value()),
                        "not a unit",
                    )
                    .help = Some("units are (mm), (sq), (h), (deg) and (%)".to_owned());
                    return None;
                }
            }
            None => None,
        };
        let value = match e.value() {
            KdlValue::Bool(b) if axis == Axis::Bool => Value::Bool(*b),
            KdlValue::Null => {
                self.err(
                    "dt::value",
                    span,
                    format!("`{control}` can't be #null"),
                    "null",
                )
                .help = Some(format!(
                    "`{control}` is {}; leave it out to use the default",
                    axis.describe()
                ));
                return None;
            }
            v @ (KdlValue::Integer(_) | KdlValue::Float(_)) if axis.is_numeric() => {
                let value = match v {
                    KdlValue::Integer(i) => i.to_string().parse().unwrap_or(f64::NAN),
                    KdlValue::Float(f) => *f,
                    _ => f64::NAN,
                };
                let x = Expr::Num { value, unit };
                self.check_expr(&x, axis, owner, control, scope, span, span)?;
                Value::Expr(x)
            }
            KdlValue::String(s) if axis.is_numeric() => {
                // Point errors inside the quotes when the string is quoted.
                let repr = e.format().map_or("", |f| f.value_repr.as_str());
                let inner_at = repr.find(s.as_str()).unwrap_or(0);
                let x = match expr::parse(s) {
                    Ok(x) => x,
                    Err(err) => {
                        let at = sub_span(span, inner_at + err.start, inner_at + err.end);
                        self.err(
                            "dt::expr",
                            at,
                            format!("can't read `{control}`: {}", err.message),
                            "here",
                        )
                        .help = Some(format!("`{control}` is {}", axis.describe()));
                        return None;
                    }
                };
                let x = match (unit, x) {
                    (Some(u), Expr::Num { value, unit: None }) => Expr::Num {
                        value,
                        unit: Some(u),
                    },
                    (_, x) => x,
                };
                self.check_expr(
                    &x,
                    axis,
                    owner,
                    control,
                    scope,
                    span,
                    sub_span(span, inner_at, inner_at + s.len()),
                )?;
                Value::Expr(x)
            }
            KdlValue::String(s) => match axis {
                Axis::Choice(options) if options.contains(&s.as_str()) => Value::Word(s.clone()),
                Axis::Choice(options) => {
                    self.err(
                        "dt::choice",
                        span,
                        format!("`{control}` can't be `{s}`"),
                        "not an option",
                    )
                    .help = Some(did_you_mean(s, options.iter().copied()).map_or_else(
                        || format!("`{control}` is one of: {}", options.join(", ")),
                        |m| format!("did you mean `{m}`?"),
                    ));
                    return None;
                }
                Axis::Line | Axis::Region => {
                    let (kind, names) = if axis == Axis::Line {
                        ("line", &scope.lines)
                    } else {
                        ("region", &scope.regions)
                    };
                    if !names.iter().any(|n| n == s) {
                        let help = did_you_mean(s, names.iter().map(String::as_str))
                            .map(|m| format!("did you mean `{m}`?"))
                            .unwrap_or_else(|| format!("define it first: `{kind} {s} {{ … }}`"));
                        self.err(
                            "dt::unknown-name",
                            span,
                            format!("there is no {kind} called `{s}`"),
                            format!("not a {kind} here"),
                        )
                        .help = Some(help);
                        return None;
                    }
                    Value::Word(s.clone())
                }
                Axis::Tag => {
                    if !TAGS.contains(&s.as_str()) {
                        self.err(
                            "dt::unknown-name",
                            span,
                            format!("`{s}` is not a kind tag"),
                            "unknown tag",
                        )
                        .help = Some(did_you_mean(s, TAGS.iter().copied()).map_or_else(
                            || format!("tags: {}", TAGS.join(", ")),
                            |m| format!("did you mean `{m}`?"),
                        ));
                        return None;
                    }
                    Value::Word(s.clone())
                }
                Axis::Bool => {
                    self.err(
                        "dt::value",
                        span,
                        format!("`{control}` is #true or #false"),
                        "a string",
                    )
                    .help = Some(format!("write {control}=#{s}"));
                    return None;
                }
                _ => Value::Text(s.clone()),
            },
            _ => {
                self.err(
                    "dt::value",
                    span,
                    format!("`{control}` is {}", axis.describe()),
                    "wrong kind of value",
                );
                return None;
            }
        };
        Some(Spanned { value, span })
    }

    /// Check units and names in an expression against the control's axis.
    #[allow(clippy::too_many_arguments)]
    fn check_expr(
        &mut self,
        x: &Expr,
        axis: Axis,
        owner: &str,
        control: &str,
        scope: &Scope,
        span: SourceSpan,
        inner: SourceSpan,
    ) -> Option<()> {
        let allowed = axis.units();
        for u in x.units() {
            if allowed.contains(&u) {
                continue;
            }
            let mut help = if allowed.is_empty() {
                format!(
                    "`{control}` is {}; write it without a unit",
                    axis.describe()
                )
            } else {
                format!("write it in {} or without a unit", allowed[0].as_str())
            };
            if axis == Axis::Up && u == Unit::Mm {
                if let Expr::Num { value, .. } = x {
                    help = format!(
                        "layout heights are in h, where 1 h is the 33.1 mm wall top: {value} mm is about {:.3}",
                        value / 33.1
                    );
                }
            } else if axis == Axis::Mm && matches!(u, Unit::Sq | Unit::H) {
                help = "print-scale details are true millimetres, so an OpenLOCK build keeps their size (confirmed default 1)".to_owned();
            } else if axis == Axis::Across && u == Unit::Mm {
                help = "layout across is in squares, so the tile builds at 35 mm and 25.4 mm alike"
                    .to_owned();
            }
            self.err(
                "dt::wrong-unit",
                span,
                format!("`{owner} {control}` can't be in {}", u.as_str()),
                format!("{control} is {}", axis.describe()),
            )
            .help = Some(help);
            return None;
        }
        for name in x.names() {
            let ok = scope.params.iter().any(|p| p == name)
                || (axis == Axis::Up && scope.heights.iter().any(|h| h == name));
            if !ok {
                let mut cands: Vec<&str> = scope.params.iter().map(String::as_str).collect();
                if axis == Axis::Up {
                    cands.extend(scope.heights.iter().map(String::as_str));
                }
                let what = if axis == Axis::Up {
                    "parameter or named height"
                } else {
                    "parameter"
                };
                let help = did_you_mean(name, cands.iter().copied())
                    .map(|m| format!("did you mean `{m}`?"))
                    .unwrap_or_else(|| format!("`{control}` is {}", axis.describe()));
                let at = self.text()[inner.offset()..inner.offset() + inner.len()]
                    .find(name)
                    .map_or(inner, |i| {
                        SourceSpan::new((inner.offset() + i).into(), name.len())
                    });
                self.err(
                    "dt::unknown-name",
                    at,
                    format!("no {what} is called `{name}`"),
                    "unknown name",
                )
                .help = Some(help);
                return None;
            }
        }
        Some(())
    }

    /// A value whose axis isn't known here: a parameter default or a
    /// definition argument.  It is checked where the parameter is used.
    fn untyped_value(&mut self, e: &KdlEntry) -> Option<Spanned<Value>> {
        let span = value_span(e);
        let unit = e.ty().and_then(|t| Unit::parse(t.value()));
        let value = match e.value() {
            KdlValue::Bool(b) => Value::Bool(*b),
            KdlValue::Integer(i) => Value::Expr(Expr::Num {
                value: i.to_string().parse().unwrap_or(f64::NAN),
                unit,
            }),
            KdlValue::Float(f) => Value::Expr(Expr::Num { value: *f, unit }),
            KdlValue::String(s) => match expr::parse(s) {
                Ok(Expr::Name(w)) => Value::Word(w),
                Ok(x) => Value::Expr(x),
                Err(_) => Value::Text(s.clone()),
            },
            KdlValue::Null => {
                self.err("dt::value", span, "a value can't be #null", "null");
                return None;
            }
        };
        Some(Spanned { value, span })
    }
}
