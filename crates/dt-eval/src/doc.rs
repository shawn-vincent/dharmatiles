//! Documents through dt-doc: KDL text → the few facts the evaluator needs.
//!
//! dt-doc does the reading: [`dt_doc::uses`] finds a file's `use` lines,
//! the files they name are decoded first (depth first, each once) and
//! registered in a [`Known`] under their [`dt_doc::model::Use::key`], then
//! [`dt_doc::decode`] reads the file itself.  Every dt-doc error becomes a
//! [`Diag`] at its text, hint and all.
//!
//! What this module adds is the evaluator's side of the model:
//!
//! - `use` keys resolve through the [`Source`]: a bare path or `lib:` is
//!   the bundled library, `./` and `../` are the project, relative to the
//!   using file;
//! - levels resolve to mm: on top of dt-doc's standard levels (the
//!   prelude, already in mm), the file's, the tile's, a definition's;
//! - definitions are inlined, with their parameters bound to the caller's
//!   arguments or their defaults;
//! - every control a built look takes is filled in: as written, or its
//!   schema default ([`dt_doc::schema::Control::default_value`]);
//! - the edge contract (library/parts/edges.kdl) is read for every tile;
//! - anything that needs a look not built yet is set aside in
//!   [`Tile::pending`]: `check` accepts it, `build` refuses it.
//!
//! The evaluator only sees [`Tile`].

use std::collections::BTreeMap;
use std::sync::Arc;

use dt_doc::Known;
use dt_doc::model::{self, Callee, Definition, Document, Spanned, Use, Value};
use dt_doc::schema::{Control, Look};
use miette::{NamedSource, SourceSpan};

use crate::diag::{Diag, Src};
use crate::source::{LIB, Source, resolve};

/// The edge contract every tile applies at its root.
pub const EDGES: &str = "lib:parts/edges.kdl";

/// The printer profile builds are checked against by default.
pub const PRINTER: &str = "lib:printers/default.kdl";

/// One decoded file and its text.
#[derive(Debug)]
pub struct File {
    /// Its path, as the [`Source`] knows it.
    pub path: String,
    /// Its text, for diagnostics.
    pub src: Src,
    /// What dt-doc made of it.
    pub doc: Document,
}

/// A control a look is given, resolved to mm or a word.
#[derive(Clone, Debug)]
pub struct Setting {
    /// The control's name.
    pub name: String,
    /// Its value.
    pub value: dt_looks::Value,
    /// Where it was written (the entry's name for a default).
    pub span: SourceSpan,
    /// The text it was written in.
    pub src: Src,
}

/// One stack entry that calls a built look, definitions inlined.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The look named.
    pub what: Spanned<String>,
    /// `in=`: the region it acts in (default the whole tile).
    pub area: Option<Spanned<String>>,
    /// `seed=`: pins its look.
    pub seed: Option<u64>,
    /// `id=`: its name among same-named siblings in seed paths.
    pub id: Option<String>,
    /// The controls the look takes: written, or their defaults.
    pub controls: Vec<Setting>,
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
    /// Fill points, in squares from the south-west corner.
    pub fills: Vec<[f64; 2]>,
    /// Where its name is written (in the tile's text).
    pub span: SourceSpan,
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
    /// Levels in mm: the standard ones, the file's, then the tile's own.
    pub levels: Vec<(String, f64)>,
    /// The edge contract: the level, mm, each surface meets every tile
    /// edge at.
    pub edges: Vec<(String, f64)>,
    /// Straight lines, by name, as points in squares.
    pub lines: BTreeMap<String, Vec<[f64; 2]>>,
    /// Regions.
    pub regions: Vec<Region>,
    /// The entries that call built looks, definitions inlined.
    pub stack: Vec<Entry>,
    /// What the document asks for that is not built yet: one error each,
    /// at its text.  `check` accepts these; `build` refuses them.
    pub pending: Vec<Diag>,
    /// The tile's own text.
    pub src: Src,
    /// Where its name is written.
    pub span: SourceSpan,
}

/// A document as read: the file, and its tile if it has one.
#[derive(Debug)]
pub struct Read {
    /// The file.
    pub file: Arc<File>,
    /// Its tile, if it has one (a parts or printer file has none).
    pub tile: Option<Tile>,
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

/// The path a `use` line in `from` names: the library for a bare or
/// `lib:` path, the project (relative to `from`) for `./` or `../`.
#[must_use]
pub fn use_path(from: &str, u: &Use) -> String {
    match &u.source {
        model::Source::Library(p) => format!("{LIB}{p}.kdl"),
        model::Source::Project(p) => resolve(from, p),
    }
}

/// Read the document at `path` and its imports.  A tile is flattened
/// into a [`Tile`]; any other file (parts, printers, levels, edges) is
/// only decoded.
///
/// # Errors
/// Every problem found, each at its text.
pub fn read(sources: &dyn Source, path: &str) -> Result<Read, Vec<Diag>> {
    let mut r = Reader {
        sources,
        files: BTreeMap::new(),
        loading: Vec::new(),
        errors: Vec::new(),
        entries: 0,
        top: None,
    };
    let Some(file) = r.load(path, None) else {
        return Err(r.errors);
    };
    let tile = r.tile(&file);
    if r.errors.is_empty() {
        Ok(Read { file, tile })
    } else {
        Err(r.errors)
    }
}

/// The printer profile at `path`, for the audit gate.
///
/// # Errors
/// Every problem found; a file with no `printer` is one.
pub fn printer(sources: &dyn Source, path: &str) -> Result<dt_kit::Printer, Vec<Diag>> {
    let r = read(sources, path)?;
    let Some(p) = &r.file.doc.printer else {
        return Err(vec![
            Diag::plain("dt::printer", format!("`{path}` has no `printer`"))
                .help("a printer file holds `printer name { line …; layer …; overhang …; gap … }`"),
        ]);
    };
    Ok(dt_kit::Printer::new(
        p.line.value,
        p.layer.value,
        p.overhang.value,
        p.gap.value,
    ))
}

/// The phase a look not built yet arrives in (plan.html).
fn phase(look: &str) -> Option<u8> {
    Some(match look {
        "dirt" => 2,
        "rock" | "stone" | "boulder" | "pebble" | "scatter" => 3,
        "grass" => 4,
        "wall" | "tree" => 5,
        "cliff" => 7,
        _ => return None,
    })
}

/// "`what` not available yet (Phase N)", at `span`.
fn not_yet(src: &Src, span: SourceSpan, what: &str, phase: Option<u8>) -> Diag {
    let when = phase.map_or_else(|| "a later phase".to_owned(), |p| format!("Phase {p}"));
    Diag::at(
        src,
        span,
        "dt::not-yet",
        format!("{what} not available yet ({when})"),
        "not built yet",
    )
    .help(format!(
        "`check` accepts this; `build` needs it, which comes in {when}"
    ))
}

/// A parameter's value and the text it was written in.
#[derive(Clone)]
struct Bound {
    value: Spanned<Value>,
    src: Src,
}

/// What an inlined definition's entries see.
#[derive(Clone)]
struct Scope {
    file: Arc<File>,
    params: BTreeMap<String, Bound>,
    levels: Vec<(String, f64)>,
    area: Option<Spanned<String>>,
    seed: Option<u64>,
    id: Option<String>,
    depth: usize,
}

struct Reader<'s> {
    sources: &'s dyn Source,
    files: BTreeMap<String, Arc<File>>,
    loading: Vec<String>,
    errors: Vec<Diag>,
    /// Stack entries met so far while inlining, definitions included.
    entries: usize,
    /// The tile's own entry being inlined (where a blow-up is reported).
    top: Option<(Src, SourceSpan)>,
}

/// The most stack entries a tile may expand to, definitions inlined.  A
/// definition calling another eight times, six deep, is 262 144 entries
/// from seven short lines; this stops it with one error.
pub const MAX_ENTRIES: usize = 10_000;

impl Reader<'_> {
    /// Decode `path` after the files it uses; each file once.
    fn load(&mut self, path: &str, from: Option<(&Src, SourceSpan)>) -> Option<Arc<File>> {
        if let Some(f) = self.files.get(path) {
            return Some(Arc::clone(f));
        }
        let at = |code, msg: String, label: &str, help: Option<String>| match from {
            Some((src, span)) => Diag::at(src, span, code, msg, label).maybe_help(help),
            None => Diag::plain(code, msg).maybe_help(help),
        };
        if self.loading.iter().any(|p| p == path) {
            self.errors.push(at(
                "dt::use",
                format!("`{path}` uses itself, through {}", self.loading.join(" → ")),
                "a loop of files",
                Some("files may use each other, but never round to themselves".into()),
            ));
            return None;
        }
        let text = match self.sources.read(path) {
            Ok(t) => t,
            Err(e) => {
                self.errors.push(at(
                    "dt::use",
                    e,
                    "used here",
                    from.map(|_| {
                        "a bare path or `lib:` is the bundled library; `./` and `../` are your project, relative to this file".into()
                    }),
                ));
                return None;
            }
        };
        let src: Src = Arc::new(NamedSource::new(path, text.clone()));
        let uses = match dt_doc::uses(path, &text) {
            Ok(u) => u,
            Err(es) => {
                self.errors.extend(es.iter().map(Diag::from));
                return None;
            }
        };
        let mut known = Known::standard();
        let mut ok = true;
        self.loading.push(path.to_owned());
        for u in &uses {
            match self.load(&use_path(path, u), Some((&src, u.path.span))) {
                Some(f) => known.add(u.key(), &f.doc),
                None => ok = false,
            }
        }
        self.loading.pop();
        if !ok {
            return None;
        }
        match dt_doc::decode(path, &text, &known) {
            Ok(doc) => {
                let f = Arc::new(File {
                    path: path.to_owned(),
                    src,
                    doc,
                });
                self.files.insert(path.to_owned(), Arc::clone(&f));
                Some(f)
            }
            Err(es) => {
                self.errors.extend(es.iter().map(Diag::from));
                None
            }
        }
    }

    /// `levels` on top of `base`, each resolved against those before it.
    fn levels<'a>(
        &mut self,
        src: &Src,
        base: &[(String, f64)],
        levels: impl IntoIterator<Item = &'a model::Level>,
    ) -> Vec<(String, f64)> {
        let mut out = base.to_vec();
        for l in levels {
            match l.value.value.mm(&out) {
                Ok(mm) => {
                    out.retain(|(n, _)| *n != l.name.value);
                    out.push((l.name.value.clone(), mm));
                }
                Err(msg) => self.errors.push(Diag::at(
                    src,
                    l.value.span,
                    "dt::levels",
                    msg,
                    "not one number",
                )),
            }
        }
        out
    }

    fn tile(&mut self, file: &Arc<File>) -> Option<Tile> {
        let def = file.doc.tile()?;
        let header = def.header()?;
        let src = &file.src;
        let standard = Known::standard().levels().to_vec();
        let levels = self.levels(src, &standard, file.doc.levels.iter().chain(&def.levels));
        let seed = match header.seed {
            None => 1,
            Some(s) => u64::try_from(s).unwrap_or_else(|_| {
                self.errors.push(Diag::at(
                    src,
                    def.name.span,
                    "dt::seed",
                    format!("a tile seed is a whole number from 0, not {s}"),
                    "this tile",
                ));
                1
            }),
        };
        let mut t = Tile {
            name: def.name.value.clone(),
            cols: header.cols,
            rows: header.rows,
            seed,
            edges: self.edges(&standard),
            levels,
            lines: BTreeMap::new(),
            regions: Vec::new(),
            stack: Vec::new(),
            pending: Vec::new(),
            src: Arc::clone(src),
            span: def.name.span,
        };
        let (cols, rows) = (f64::from(t.cols), f64::from(t.rows));
        for l in &def.lines {
            if let Some(w) = l.width.as_ref().or(l.wobble.as_ref()) {
                t.pending.push(not_yet(
                    src,
                    w.span,
                    "a line's width and wobble are",
                    Some(4),
                ));
            }
            let mut pts = Vec::new();
            for p in &l.through {
                match fixed_point(&p.value, cols, rows) {
                    Some(xy) => pts.push(xy),
                    None => {
                        t.pending
                            .push(not_yet(src, p.span, "a point drawn from a range is", None))
                    }
                }
            }
            t.lines.insert(l.name.value.clone(), pts);
        }
        for r in &def.regions {
            let mut fills = Vec::new();
            for p in &r.fills {
                match fixed_point(&p.value, cols, rows) {
                    Some(xy) => fills.push(xy),
                    None => t.pending.push(not_yet(
                        src,
                        p.span,
                        "a fill point drawn from a range is",
                        None,
                    )),
                }
            }
            t.regions.push(Region {
                name: r.name.value.clone(),
                bounded_by: r.bounded_by.clone(),
                fills,
                span: r.name.span,
            });
        }
        let scope = Scope {
            file: Arc::clone(file),
            params: BTreeMap::new(),
            levels: t.levels.clone(),
            area: None,
            seed: None,
            id: None,
            depth: 0,
        };
        let mut stack = Vec::new();
        let mut pending = Vec::new();
        self.expand(&def.stack, &scope, &mut stack, &mut pending);
        t.stack = stack;
        t.pending.extend(pending);
        Some(t)
    }

    /// The edge contract, levels in mm.
    fn edges(&mut self, standard: &[(String, f64)]) -> Vec<(String, f64)> {
        let Some(f) = self.load(EDGES, None) else {
            return Vec::new();
        };
        let Some(edges) = &f.doc.edges else {
            self.errors.push(Diag::plain(
                "dt::edges",
                format!("`{EDGES}` holds no `edges` contract"),
            ));
            return Vec::new();
        };
        let levels = self.levels(&f.src, standard, &f.doc.levels);
        let mut out = Vec::new();
        for r in &edges.rules {
            match r.level.value.mm(&levels) {
                Ok(mm) => out.push((r.surface.value.clone(), mm)),
                Err(msg) => self.errors.push(Diag::at(
                    &f.src,
                    r.level.span,
                    "dt::edges",
                    msg,
                    "not one level",
                )),
            }
        }
        out
    }

    /// Inline `entries` into `out`: built looks become [`Entry`]s, what
    /// is not built yet goes to `pending`.
    fn expand(
        &mut self,
        entries: &[model::Entry],
        scope: &Scope,
        out: &mut Vec<Entry>,
        pending: &mut Vec<Diag>,
    ) {
        let src = &scope.file.src;
        for e in entries {
            if scope.depth == 0 {
                self.top = Some((Arc::clone(src), e.span));
            }
            if self.entries >= MAX_ENTRIES {
                if self.entries == MAX_ENTRIES {
                    self.entries += 1;
                    let (at, span) = self.top.clone().unwrap_or((Arc::clone(src), e.span));
                    self.errors.push(
                        Diag::at(
                            &at,
                            span,
                            "dt::too-many",
                            format!("this expands to more than {MAX_ENTRIES} stack entries"),
                            "expands too far",
                        )
                        .help("a definition that calls others several times each multiplies at every level; use `scatter` or fewer calls"),
                    );
                }
                return;
            }
            self.entries += 1;
            let area = e.place.region.clone().or_else(|| scope.area.clone());
            let seed = match &e.place.seed {
                Some(s) => {
                    if let Ok(v) = u64::try_from(s.value) {
                        Some(v)
                    } else {
                        self.errors.push(Diag::at(
                            src,
                            s.span,
                            "dt::seed",
                            "a seed is a whole number from 0",
                            "negative",
                        ));
                        None
                    }
                }
                None => scope.seed,
            };
            let id = e
                .place
                .id
                .as_ref()
                .map(|i| i.value.clone())
                .or_else(|| scope.id.clone());
            match &e.callee {
                Callee::Look(look) => {
                    let Some(takes) = dt_looks::takes(look.name) else {
                        pending.push(not_yet(
                            src,
                            e.what.span,
                            &format!("look `{}`", look.name),
                            phase(look.name),
                        ));
                        continue;
                    };
                    let mut ok = true;
                    for (word, span) in [
                        ("along=", e.place.along.as_ref().map(|s| s.span)),
                        ("side=", e.place.side.as_ref().map(|s| s.span)),
                        ("at=", e.place.at.as_ref().map(|s| s.span)),
                        ("against=", e.place.against.as_ref().map(|s| s.span)),
                    ] {
                        if let Some(span) = span {
                            ok = false;
                            pending.push(not_yet(
                                src,
                                span,
                                &format!("`{word}` on `{}` is", look.name),
                                None,
                            ));
                        }
                    }
                    for c in &e.children {
                        ok = false;
                        pending.push(not_yet(
                            src,
                            c.what.span,
                            &format!("`{}` inside `{}` is", c.what.value, look.name),
                            phase(&c.what.value).or(Some(3)),
                        ));
                    }
                    let controls = self.controls(e, look, takes, scope, pending);
                    if ok && let Some(controls) = controls {
                        out.push(Entry {
                            what: e.what.clone(),
                            area,
                            seed,
                            id,
                            controls,
                            src: Arc::clone(src),
                            span: e.span,
                        });
                    }
                }
                Callee::Definition(name) => {
                    let Some((file, def)) = self.definition(&scope.file, name) else {
                        self.errors.push(Diag::at(
                            src,
                            e.what.span,
                            "dt::unknown-name",
                            format!("cannot find the definition `{name}`"),
                            "not loaded",
                        ));
                        continue;
                    };
                    if scope.depth > 64 {
                        self.errors.push(Diag::at(
                            src,
                            e.what.span,
                            "dt::loop",
                            "definitions call each other too deeply",
                            "here",
                        ));
                        continue;
                    }
                    let mut params = BTreeMap::new();
                    for p in &def.params {
                        let arg = e
                            .control(&p.name.value)
                            .and_then(|s| s.value.as_ref())
                            .map(|v| bind(v, src, scope))
                            .or_else(|| {
                                p.default.as_ref().map(|v| Bound {
                                    value: v.clone(),
                                    src: Arc::clone(&file.src),
                                })
                            });
                        if let Some(a) = arg {
                            params.insert(p.name.value.clone(), a);
                        }
                    }
                    let levels = self.levels(
                        &file.src,
                        &scope.levels,
                        file.doc.levels.iter().chain(&def.levels),
                    );
                    let inner = Scope {
                        file: Arc::clone(&file),
                        params,
                        levels,
                        area,
                        seed,
                        id,
                        depth: scope.depth + 1,
                    };
                    self.expand(&def.stack, &inner, out, pending);
                }
            }
        }
    }

    /// The definition `name` as `file` sees it: its own, or one it uses.
    fn definition(&self, file: &Arc<File>, name: &str) -> Option<(Arc<File>, Definition)> {
        if let Some(d) = file.doc.definition(name)
            && d.header().is_none()
        {
            return Some((Arc::clone(file), d.clone()));
        }
        file.doc
            .uses
            .iter()
            .filter(|u| u.names.iter().any(|n| n.value == name))
            .find_map(|u| {
                let f = self.files.get(&use_path(&file.path, u))?;
                let d = f.doc.definition(name)?.clone();
                Some((Arc::clone(f), d))
            })
    }

    /// The controls a built look takes, as written or by default; `None`
    /// when one cannot be given yet (it is in `pending`).
    fn controls(
        &mut self,
        e: &model::Entry,
        look: &'static Look,
        takes: &[&str],
        scope: &Scope,
        pending: &mut Vec<Diag>,
    ) -> Option<Vec<Setting>> {
        let src = &scope.file.src;
        let mut out = Vec::new();
        let mut ok = true;
        for s in &e.controls {
            let Some(v) = &s.value else {
                ok = false;
                pending.push(not_yet(
                    src,
                    s.name.span,
                    &format!("the group `{}` on `{}` is", s.name.value, look.name),
                    None,
                ));
                continue;
            };
            if let Some(sub) = s.subs.first() {
                ok = false;
                pending.push(not_yet(
                    src,
                    sub.name.span,
                    &format!("sub-controls of `{}` are", s.name.value),
                    phase_of_control(look.name, &s.name.value),
                ));
                continue;
            }
            let b = bind(v, src, scope);
            let value = match value(&b, &scope.levels) {
                Ok(v) => v,
                Err(why) => {
                    ok = false;
                    pending.push(*why);
                    continue;
                }
            };
            if !takes.contains(&s.name.value.as_str()) {
                // a control written at its default changes nothing
                let default = s.control.and_then(|c| default_value(c, &scope.levels));
                if default.as_ref() != Some(&value) {
                    ok = false;
                    pending.push(not_yet(
                        src,
                        s.span,
                        &format!("`{}`'s `{}` is", look.name, s.name.value),
                        phase_of_control(look.name, &s.name.value),
                    ));
                }
                continue;
            }
            out.push(Setting {
                name: s.name.value.clone(),
                value,
                span: b.value.span,
                src: Arc::clone(&b.src),
            });
        }
        for c in look.controls {
            if takes.contains(&c.name)
                && e.control(c.name).is_none()
                && let Some(value) = default_value(c, &scope.levels)
            {
                out.push(Setting {
                    name: c.name.to_owned(),
                    value,
                    span: e.what.span,
                    src: Arc::clone(src),
                });
            }
        }
        ok.then_some(out)
    }
}

/// The phase a control of a built look arrives in.
fn phase_of_control(look: &str, control: &str) -> Option<u8> {
    match (look, control) {
        ("ground", "bumps" | "bump-size") => Some(2),
        _ => None,
    }
}

/// A written value for a built look; a range is not built yet.
fn value(b: &Bound, levels: &[(String, f64)]) -> Result<dt_looks::Value, Box<Diag>> {
    if let Value::Num(n) = &b.value.value
        && n.expr.is_range()
    {
        return Err(Box::new(not_yet(
            &b.src,
            b.value.span,
            "a range here is",
            Some(2),
        )));
    }
    look_value(&b.value.value, levels)
        .map_err(|msg| Box::new(Diag::at(&b.src, b.value.span, "dt::value", msg, "here")))
}

/// A value as a built look takes it: one number in mm, or a word.
fn look_value(v: &Value, levels: &[(String, f64)]) -> Result<dt_looks::Value, String> {
    match v {
        Value::Num(n) => n.mm(levels).map(dt_looks::Value::Number),
        Value::Word(w) => Ok(dt_looks::Value::Word(w.word.to_owned())),
        Value::Bool(v) => Ok(dt_looks::Value::Word(
            if *v { "#true" } else { "#false" }.to_owned(),
        )),
        Value::Tag(t) | Value::Text(t) => Ok(dt_looks::Value::Word(t.clone())),
        Value::Param(p) => Err(format!("the parameter `{p}` has no value here")),
    }
}

/// `v` with a parameter replaced by what the caller bound to it.
fn bind(v: &Spanned<Value>, src: &Src, scope: &Scope) -> Bound {
    if let Value::Param(p) = &v.value
        && let Some(b) = scope.params.get(p)
    {
        return b.clone();
    }
    Bound {
        value: v.clone(),
        src: Arc::clone(src),
    }
}

/// A control's schema default, resolved; `None` without one (or for a
/// range, which no built look takes yet).
fn default_value(c: &'static Control, levels: &[(String, f64)]) -> Option<dt_looks::Value> {
    look_value(&c.default_value()?, levels).ok()
}

/// A point in squares, if it is not drawn from a range.
fn fixed_point(p: &dt_doc::place::Point, cols: f64, rows: f64) -> Option<[f64; 2]> {
    if p.is_range() {
        return None;
    }
    let ((x, _), (y, _)) = p.bounds(cols, rows).ok()?;
    Some([x, y])
}
