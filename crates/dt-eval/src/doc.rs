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
use dt_doc::place::{Along, Edge, Point};
use dt_doc::schema::{Control, Look, PlaceWord, Role};
use dt_kit::Seed;
use miette::{NamedSource, SourceSpan};

use crate::diag::{Diag, Src};
use crate::source::{LIB, Source, resolve};

/// The edge contract every tile applies at its root.
pub const EDGES: &str = "lib:parts/edges.kdl";

/// A line's wobble follows smooth noise of this sigma along it, mm at the
/// reference square (waterlab's shore, `Scene.__init__` :300).
const LINE_WOBBLE_SIGMA_MM: f64 = 9.0;

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
    /// `along=` a path of tile edges, as the path in squares walked with
    /// the tile inside on the left (see [`edge_path`]).
    pub along: Option<Vec<[f64; 2]>>,
    /// `at=`: where it may stand, `[x_lo, x_hi]` and `[y_lo, y_hi]` in
    /// squares.
    pub at: Option<[[f64; 2]; 2]>,
    /// `against=`: the kind of thing it gathers at the foot of.
    pub against: Option<String>,
    /// A scatter's children: each thing, its controls and its weight.
    pub children: Vec<Child>,
    /// The text it came from.
    pub src: Src,
    /// The whole node.
    pub span: SourceSpan,
}

/// A child of a scatter, as its look takes it.
#[derive(Clone, Debug)]
pub struct Child {
    /// The thing's look.
    pub what: Spanned<String>,
    /// Its controls, written or by default.
    pub controls: Vec<Setting>,
    /// `weight=` (default 1).
    pub weight: f64,
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
    /// The wobbled lines as drawn (points in squares) and their wobble,
    /// mm, so [`Tile::reseed`] can wobble them again.
    pub wobbles: BTreeMap<String, (Vec<[f64; 2]>, f64)>,
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

impl Tile {
    /// Replace the root seed, wobbling the wobbled lines again from it
    /// (a `--seed` reaches the lines as well as the looks).
    pub fn reseed(&mut self, seed: u64) {
        self.seed = seed;
        for (name, (pts, wobble)) in &self.wobbles {
            self.lines
                .insert(name.clone(), wobbled(pts, *wobble, seed, name));
        }
    }
}

/// A line's points (in squares) wobbled `wobble` mm, at the reference
/// square, keyed by the tile seed and the line's name.
fn wobbled(pts: &[[f64; 2]], wobble: f64, seed: u64, name: &str) -> Vec<[f64; 2]> {
    let sq = dt_kit::REFERENCE_SQUARE_MM;
    let mm: Vec<[f64; 2]> = pts.iter().map(|p| [p[0] * sq, p[1] * sq]).collect();
    let seed = Seed::new(seed).key("line").key(name);
    dt_kit::wobble(&mm, wobble, LINE_WOBBLE_SIGMA_MM, 0.25, seed)
        .into_iter()
        .map(|p| [p[0] / sq, p[1] / sq])
        .collect()
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
        dims: (1.0, 1.0),
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
        "wall" | "floor" | "tree" => 5,
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
    /// Place words a definition call passes to the looks inside it.
    at: Option<Spanned<Point>>,
    along: Option<Spanned<Along>>,
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
    /// The tile's squares across and up, for edge paths and points.
    dims: (f64, f64),
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
                // the document asked for itself, or a file a `use` names
                let code = if from.is_some() {
                    "dt::use"
                } else {
                    "dt::source"
                };
                self.errors.push(at(
                    code,
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
            wobbles: BTreeMap::new(),
            regions: Vec::new(),
            stack: Vec::new(),
            pending: Vec::new(),
            src: Arc::clone(src),
            span: def.name.span,
        };
        let (cols, rows) = (f64::from(t.cols), f64::from(t.rows));
        self.dims = (cols, rows);
        for l in &def.lines {
            if let Some(w) = l.width.as_ref() {
                t.pending
                    .push(not_yet(src, w.span, "a line's width is", Some(4)));
            }
            let wobble = match l.wobble.as_ref().map(|w| (w, w.value.mm(&t.levels))) {
                None => 0.0,
                Some((_, Ok(v))) if v.is_finite() && (0.0..=35.0).contains(&v) => v,
                Some((w, Ok(_) | Err(_))) => {
                    self.errors.push(Diag::at(
                        src,
                        w.span,
                        "dt::value",
                        "a line's wobble is from 0 to 35 mm",
                        "here",
                    ));
                    0.0
                }
            };
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
            if wobble > 0.0 {
                t.wobbles
                    .insert(l.name.value.clone(), (pts.clone(), wobble));
                pts = wobbled(&pts, wobble, t.seed, &l.name.value);
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
            at: None,
            along: None,
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
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
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
            let at = e.place.at.clone().or_else(|| scope.at.clone());
            let along = e.place.along.clone().or_else(|| scope.along.clone());
            // this entry's path under an enclosing definition call: its
            // name, then its id or its count among same-named siblings
            // (an id is never all digits, so the two cannot meet)
            let own = match &e.place.id {
                Some(i) => format!("{}/id:{}", e.what.value, i.value),
                None => {
                    let k = seen.entry(e.what.value.clone()).or_default();
                    *k += 1;
                    format!("{}/{}", e.what.value, *k - 1)
                }
            };
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
                // inherited from a definition call: one seed per inner
                // entry, keyed by its path under the call
                None => scope.seed.map(|s| Seed::new(s).path(&own).value()),
            };
            let id = match (&e.place.id, &scope.id) {
                (_, Some(base)) => Some(format!("{base}/{own}")),
                (Some(i), None) => Some(i.value.clone()),
                (None, None) => None,
            };
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
                    // inside a definition the decoder leaves a required
                    // place word to the call; it must arrive from there
                    for need in look.needs {
                        let has = match need {
                            PlaceWord::Along => along.is_some(),
                            PlaceWord::At => at.is_some(),
                            _ => true,
                        };
                        if !has {
                            ok = false;
                            let w = need.as_str();
                            let mut d = Diag::at(
                                src,
                                e.what.span,
                                "dt::missing-control",
                                format!(
                                    "`{}` needs `{w}=`, and the definition call gives none",
                                    look.name
                                ),
                                format!("no `{w}=` here or at the call"),
                            );
                            d.help = Some(format!(
                                "write it here, or pass it at the call: `name {w}=…`"
                            ));
                            self.errors.push(d);
                        }
                    }
                    let honours = dt_looks::places(look.name);
                    // along= a path of edges and at= a point, for the looks
                    // that place by them; the rest is not built yet
                    let mut path = None;
                    let mut point = None;
                    let against = if honours.contains(&"against") {
                        e.place.against.as_ref().map(|a| a.value.clone())
                    } else {
                        None
                    };
                    let mut unplaced = vec![("side=", e.place.side.as_ref().map(|s| s.span))];
                    if !honours.contains(&"against") {
                        unplaced.push(("against=", e.place.against.as_ref().map(|s| s.span)));
                    }
                    match &along {
                        Some(a) if honours.contains(&"along") => match &a.value {
                            Along::Edges(edges) => path = Some(edge_path(edges, self.dims)),
                            Along::Line(_) => unplaced.push(("along= a drawn line", Some(a.span))),
                        },
                        // inherited from a definition call: for the looks
                        // that place by it only, the others ignore it
                        Some(a) if e.place.along.is_some() => {
                            unplaced.push(("along=", Some(a.span)));
                        }
                        _ => {}
                    }
                    match &at {
                        Some(p) if honours.contains(&"at") => {
                            match p.value.bounds(self.dims.0, self.dims.1) {
                                Ok(((x0, x1), (y0, y1))) => point = Some([[x0, x1], [y0, y1]]),
                                Err(msg) => self
                                    .errors
                                    .push(Diag::at(src, p.span, "dt::at", msg, "here")),
                            }
                        }
                        Some(p) if e.place.at.is_some() => unplaced.push(("at=", Some(p.span))),
                        _ => {}
                    }
                    for (word, span) in unplaced {
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
                    let mut children = Vec::new();
                    // a scatter inside a surface scatters on that surface:
                    // its own entry, in the surface's area, built after the lift
                    let mut hoisted: Vec<model::Entry> = Vec::new();
                    for c in &e.children {
                        if look.role == Role::Surface
                            && matches!(&c.callee, Callee::Look(cl) if cl.name == "scatter")
                        {
                            hoisted.push(c.clone());
                            continue;
                        }
                        let built = match &c.callee {
                            Callee::Look(cl) if dt_looks::holds(look.name, cl.name) => {
                                dt_looks::takes(cl.name).map(|t| (*cl, t))
                            }
                            _ => None,
                        };
                        let Some((cl, ctakes)) = built else {
                            ok = false;
                            pending.push(not_yet(
                                src,
                                c.what.span,
                                &format!("`{}` inside `{}` is", c.what.value, look.name),
                                phase(&c.what.value).or(Some(3)),
                            ));
                            continue;
                        };
                        for (word, span) in [
                            ("along=", c.place.along.as_ref().map(|s| s.span)),
                            ("side=", c.place.side.as_ref().map(|s| s.span)),
                            ("at=", c.place.at.as_ref().map(|s| s.span)),
                            ("against=", c.place.against.as_ref().map(|s| s.span)),
                            ("in=", c.place.region.as_ref().map(|s| s.span)),
                        ] {
                            if let Some(span) = span {
                                ok = false;
                                pending.push(not_yet(
                                    src,
                                    span,
                                    &format!("`{word}` on a scatter's `{}` is", cl.name),
                                    None,
                                ));
                            }
                        }
                        match self.controls(c, cl, ctakes, scope, pending) {
                            Some(controls) => children.push(Child {
                                what: c.what.clone(),
                                controls,
                                weight: c.place.weight.as_ref().map_or(1.0, |w| w.value),
                            }),
                            None => ok = false,
                        }
                    }
                    let controls = self.controls(e, look, takes, scope, pending);
                    if ok && let Some(controls) = controls {
                        let inner = Scope {
                            area: area.clone(),
                            ..scope.clone()
                        };
                        out.push(Entry {
                            what: e.what.clone(),
                            area,
                            seed,
                            id,
                            controls,
                            along: path,
                            at: point,
                            against,
                            children,
                            src: Arc::clone(src),
                            span: e.span,
                        });
                        if !hoisted.is_empty() {
                            self.expand(&hoisted, &inner, out, pending);
                        }
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
                        at,
                        along,
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
            // the control's own value, then its sub-controls as `control/sub`
            let mut items: Vec<(
                String,
                &Spanned<Value>,
                SourceSpan,
                Option<&'static Control>,
            )> = Vec::new();
            if let Some(v) = &s.value {
                items.push((s.name.value.clone(), v, s.span, s.control));
            }
            for sub in &s.subs {
                let name = format!("{}/{}", s.name.value, sub.name.value);
                match &sub.value {
                    Some(v) if takes.contains(&name.as_str()) => {
                        items.push((name, v, sub.span, sub.control));
                    }
                    _ => {
                        ok = false;
                        pending.push(not_yet(
                            src,
                            sub.name.span,
                            &format!("sub-controls of `{}` are", s.name.value),
                            phase_of_control(look.name, &s.name.value),
                        ));
                    }
                }
            }
            if s.value.is_none() && s.subs.is_empty() {
                ok = false;
                pending.push(not_yet(
                    src,
                    s.name.span,
                    &format!("the group `{}` on `{}` is", s.name.value, look.name),
                    phase_of_control(look.name, &s.name.value),
                ));
            }
            for (name, v, span, control) in items {
                let b = bind(v, src, scope);
                let value = match value(&b, &scope.levels) {
                    Ok(v) => v,
                    Err(why) => {
                        ok = false;
                        pending.push(*why);
                        continue;
                    }
                };
                if !takes.contains(&name.as_str()) {
                    // a control written at its default changes nothing
                    let default = control.and_then(|c| default_value(c, &scope.levels));
                    if default.as_ref() != Some(&value) {
                        ok = false;
                        pending.push(not_yet(
                            src,
                            span,
                            &format!("`{}`'s `{}` is", look.name, s.name.value),
                            phase_of_control(look.name, &s.name.value),
                        ));
                    }
                    continue;
                }
                push_setting(&mut out, &name, &b.value.value, value, b.value.span, &b.src);
            }
        }
        for c in look.controls {
            if takes.contains(&c.name)
                && e.control(c.name).is_none()
                && let Some(raw) = c.default_value()
                && let Ok(value) = look_value(&raw, &scope.levels)
            {
                push_setting(&mut out, c.name, &raw, value, e.what.span, src);
            }
        }
        ok.then_some(out)
    }
}

/// One setting for a look, and, for a word that stands for numbers
/// (`clods some`), each number as `control.name` (`clods.density`).
fn push_setting(
    out: &mut Vec<Setting>,
    name: &str,
    raw: &Value,
    value: dt_looks::Value,
    span: SourceSpan,
    src: &Src,
) {
    if let Value::Word(w) = raw {
        for (k, v) in w.sets {
            out.push(Setting {
                name: format!("{name}.{k}"),
                value: dt_looks::Value::Number(*v),
                span,
                src: Arc::clone(src),
            });
        }
    }
    out.push(Setting {
        name: name.to_owned(),
        value,
        span,
        src: Arc::clone(src),
    });
}

/// The phase a control of a built look arrives in.
fn phase_of_control(look: &str, control: &str) -> Option<u8> {
    match (look, control) {
        _ => None,
    }
}

/// A written value for a built look: a range stays a range (the look
/// draws from it).
fn value(b: &Bound, levels: &[(String, f64)]) -> Result<dt_looks::Value, Box<Diag>> {
    look_value(&b.value.value, levels)
        .map_err(|msg| Box::new(Diag::at(&b.src, b.value.span, "dt::value", msg, "here")))
}

/// A value as a built look takes it: one number in mm, or a word.
fn look_value(v: &Value, levels: &[(String, f64)]) -> Result<dt_looks::Value, String> {
    match v {
        Value::Num(n) if n.expr.is_range() => {
            let env = |name: &str| {
                levels
                    .iter()
                    .rev()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| *v)
            };
            n.bounds(&env).map(|(a, b)| dt_looks::Value::Range(a, b))
        }
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

/// A path of tile edges (each meeting the next at a corner) as one
/// polyline in squares, walked with the tile inside on the left: the
/// path a run's outer face follows (`north west` on a 1×1 is
/// `(1,1) → (0,1) → (0,0)`, the Python spine of a north-west corner
/// wall). A path written clockwise is walked the other way round.
#[must_use]
pub fn edge_path(edges: &[Edge], (cols, rows): (f64, f64)) -> Vec<[f64; 2]> {
    // each edge anticlockwise round the tile: the inside on the left
    let walk = |e: Edge| match e {
        Edge::South => ([0.0, 0.0], [cols, 0.0]),
        Edge::East => ([cols, 0.0], [cols, rows]),
        Edge::North => ([cols, rows], [0.0, rows]),
        Edge::West => ([0.0, rows], [0.0, 0.0]),
    };
    let mut order: Vec<Edge> = edges.to_vec();
    #[allow(clippy::float_cmp)]
    let follows = |a: Edge, b: Edge| walk(a).1 == walk(b).0;
    if order.len() > 1 && !follows(order[0], order[1]) {
        order.reverse();
    }
    let mut out = Vec::with_capacity(order.len() + 1);
    if let Some(&first) = order.first() {
        out.push(walk(first).0);
    }
    out.extend(order.iter().map(|&e| walk(e).1));
    out
}
