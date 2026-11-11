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
//! - a tile's `let`s that are one number resolve to mm, for the element
//!   controls and line wobbles that name them;
//! - definitions are inlined, with their parameters bound to the caller's
//!   arguments or their defaults;
//! - every control a built element takes is filled in: as written, or its
//!   schema default ([`dt_doc::schema::Control::default_value`]);
//! - the edge contract (library/parts/edges.dtile) is read for every tile;
//! - anything that needs an element not built yet is set aside in
//!   [`Tile::pending`]: `check` accepts it, `build` refuses it.
//!
//! The evaluator only sees [`Tile`].

use std::collections::BTreeMap;
use std::sync::Arc;

use dt_doc::Known;
use dt_doc::consts::ConstTable;
use dt_doc::expr::Expr;
use dt_doc::model::{self, Callee, Definition, Document, PlaceRef, Spanned, Use, Value};
use dt_doc::place::{Along, Edge, Point, Side};
use dt_doc::schema::{Control, Element, PlaceWord, Role};
use dt_kit::{Draw, Seed};
use miette::{NamedSource, SourceSpan};

use crate::diag::{Diag, Src};
use crate::lower::{Lets, Lowered, element_value};
use crate::source::{LIB, Source, resolve};

/// The edge contract every tile applies at its root.
pub const EDGES: &str = "lib:parts/edges.dtile";

/// The library's heights, every tile's constants: read from disk like any
/// other file, never compiled in.
pub const HEIGHTS: &str = "lib:parts/heights.dtile";

/// A line's wobble follows smooth noise of this sigma along it, mm at the
/// reference square.
const LINE_WOBBLE_SIGMA_MM: f64 = 9.0;

/// The widest strip a line makes, mm at the reference square (two squares).
const MAX_LINE_WIDTH_MM: f64 = 70.0;

/// The name of the strip a line's width makes.  It has a space, which no
/// written name can, so a document never names it.
#[must_use]
pub fn strip_name(line: &str) -> String {
    format!("{line} strip")
}

/// The printer profile builds are checked against by default.
pub const PRINTER: &str = "lib:printers/default.dtile";

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

/// A control an element is given, resolved to mm or a word.
#[derive(Clone, Debug)]
pub struct Setting {
    /// The control's name.
    pub name: String,
    /// Its value.
    pub value: dt_elements::Value,
    /// Where it was written (the entry's name for a default).
    pub span: SourceSpan,
    /// The text it was written in.
    pub src: Src,
}

/// One stack entry that calls a built element, definitions inlined.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The element named.
    pub what: Spanned<String>,
    /// `at=` a region: where it acts in (default the whole tile).
    pub area: Option<Spanned<String>>,
    /// `seed=`: pins its element.
    pub seed: Option<u64>,
    /// `id=`: its name among same-named siblings in seed paths.
    pub id: Option<String>,
    /// The controls the element takes: written, or their defaults.
    pub controls: Vec<Setting>,
    /// `at=` a path of tile edges, as the path in squares walked with
    /// the tile inside on the left (see [`edge_path`]).
    pub along: Option<Vec<[f64; 2]>>,
    /// `at=`: where it stands, the x and the y in squares, each a number
    /// or a range with its odds, drawn once from the entry's seed.
    pub at: Option<[Draw; 2]>,
    /// `at=foot(…)`: the kind of thing it gathers at the foot of.
    pub against: Option<String>,
    /// `at=` a drawn line (a scatter's): its name; the build reads the
    /// line's points from [`Tile::lines`], so a new seed's wobble reaches it.
    pub line: Option<String>,
    /// `side=` of that line (a wall's): `None` is centre.
    pub side: Option<Side>,
    /// A scatter's children: each thing, its controls and its weight.
    pub children: Vec<Child>,
    /// The text it came from.
    pub src: Src,
    /// The whole node.
    pub span: SourceSpan,
}

/// A child of a scatter, as its element takes it.
#[derive(Clone, Debug)]
pub struct Child {
    /// The thing's element.
    pub what: Spanned<String>,
    /// Its controls, written or by default.
    pub controls: Vec<Setting>,
    /// `weight=` (default 1).
    pub weight: f64,
}

/// A region: lines (by name) and fill points (squares), discs round places,
/// less the regions it excepts.
#[derive(Clone, Debug)]
pub struct Region {
    /// Its name.
    pub name: String,
    /// The lines that bound it.
    pub bounded_by: Vec<Spanned<String>>,
    /// Fill points, in squares from the south-west corner.
    pub fills: Vec<[f64; 2]>,
    /// The fill points written with a range, by their place in `fills`:
    /// each is drawn once from the tile seed ([`Tile::reseed`] draws again).
    pub fill_draws: Vec<(usize, [Draw; 2])>,
    /// `within MM of=PLACE`: a distance in mm at the reference square, and
    /// the place it is round.
    pub within: Vec<Within>,
    /// `except REGION…`.
    pub except: Vec<String>,
    /// Where its name is written (in the tile's text).
    pub span: SourceSpan,
    /// Made by a line's width, not written: the strip an element `at=` the
    /// line covers.  Its name (`trail strip`) cannot be written, so no
    /// document names it.
    pub strip: bool,
}

/// A distance round a place.
#[derive(Clone, Debug)]
pub struct Within {
    /// Mm at the reference square (a range is taken at its middle).
    pub mm: f64,
    /// A range `[a .. b]`, mm at the reference square: the footprint's depth
    /// wanders between them along the place.
    pub range: Option<(f64, f64)>,
    /// The place it is round.
    pub of: PlaceRef,
    /// Where it is written.
    pub span: SourceSpan,
}

/// Where a pin or a default was written.
#[derive(Clone, Debug)]
pub struct Provenance {
    /// The file's path.
    pub path: String,
    /// The text.
    pub src: Src,
    /// True for a library default.
    pub default: bool,
}

/// One pin: a place and a band.
#[derive(Clone, Debug)]
pub struct PinDecl {
    /// Its name, if written.
    pub name: Option<String>,
    /// The band.
    pub top: Expr,
    /// Where the band is written.
    pub top_span: SourceSpan,
    /// The place.
    pub at: PlaceRef,
    /// Where the place is written.
    pub at_span: SourceSpan,
    /// The whole pin line.
    pub span: SourceSpan,
    /// Where it comes from.
    pub from: Provenance,
}

/// A body of the stack (dirt, water, cliff, flagstones or planks) as written.
#[derive(Clone, Debug)]
pub struct BodyDecl {
    /// Its place in the tile's stack ([`Tile::stack`]).
    pub entry: usize,
    /// The element.
    pub element: String,
    /// Its name, if written.
    pub label: Option<String>,
    /// Its top, if written.
    pub top: Option<(Expr, SourceSpan)>,
    /// The region it fills: `None` is the whole tile.
    pub region: Option<String>,
    /// Its `slope`, degrees.
    pub slope: Option<f64>,
    /// Its pins, in order.
    pub pins: Vec<PinDecl>,
    /// The context its top and pins are read in: the constants of the scope
    /// the body is written in (the tile's, in format 0).
    pub table: Arc<ConstTable>,
    /// Its element's name in the text.
    pub span: SourceSpan,
}

/// The tops and pins a library default gives every body of an element.
#[derive(Clone, Debug)]
pub struct DefaultBody {
    /// The element.
    pub element: String,
    /// The default top.
    pub top: Option<(Expr, SourceSpan)>,
    /// The default pins.
    pub pins: Vec<PinDecl>,
    /// Where it is written.
    pub from: Provenance,
}

/// What the heights language adds to a tile.
#[derive(Clone, Debug, Default)]
pub struct Heights {
    /// Constants: the library's, the tile's, overrides applied.
    pub consts: ConstTable,
    /// Every point by name, built in and declared, in squares.
    pub points: BTreeMap<String, [f64; 2]>,
    /// Which lines are closed.
    pub closed: Vec<String>,
    /// The tile's bodies, in stack order.
    pub bodies: Vec<BodyDecl>,
    /// The library's defaults.
    pub defaults: Vec<DefaultBody>,
    /// Where each stack entry that is not a body (things, coats) sits, for
    /// order notes and `top=` on things.
    pub things: Vec<ThingDecl>,
}

/// A thing or a coat in the stack, for the checks that read its top.
#[derive(Clone, Debug)]
pub struct ThingDecl {
    /// The element or definition.
    pub element: String,
    /// Its top (a height limit), if written.
    pub top: Option<(Expr, SourceSpan)>,
    /// Its `at=`, if written.
    pub at: Option<PlaceRef>,
    /// Where it is written.
    pub span: SourceSpan,
}

/// Who a formula is called for: a stack entry, or one of a scatter's
/// children (each thing a scatter places has its own region).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner {
    /// The stack entry ([`Tile::stack`]).
    pub entry: usize,
    /// The child of that entry (a scatter's), when it is one.
    pub child: Option<usize>,
}

/// A formula the tile writes, read where a thing calls it: `check` settles
/// what `parent(…)` in it can reach once the tile's regions are known
/// (format 1).
#[derive(Clone, Debug)]
pub struct RegionRead {
    /// The formula.
    pub expr: Expr,
    /// Where it is written.
    pub span: SourceSpan,
    /// Its text.
    pub src: Src,
    /// The context it is read in.
    pub table: Arc<ConstTable>,
    /// Who calls it.
    pub owner: Owner,
}

/// What the evaluator needs from a tile document.
#[derive(Clone, Debug)]
pub struct Tile {
    /// The document's format (0, or 1 in development).
    pub format: u32,
    /// Formulas read where a thing calls them, for the region checks of
    /// format 1 (empty in format 0).
    pub region_reads: Vec<RegionRead>,
    /// Squares across.
    pub cols: u32,
    /// Squares up.
    pub rows: u32,
    /// The root seed (1 if none is written).
    pub seed: u64,
    /// Straight lines, by name, as points in squares.
    pub lines: BTreeMap<String, Vec<[f64; 2]>>,
    /// The width, mm at the reference square, of each line that has one.
    pub widths: BTreeMap<String, f64>,
    /// The lines with a point drawn from a range.
    pub drawn_lines: BTreeMap<String, DrawnLine>,
    /// The wobbled lines as drawn (points in squares) and their wobble,
    /// mm, so [`Tile::reseed`] can wobble them again.
    pub wobbles: BTreeMap<String, (Vec<[f64; 2]>, f64)>,
    /// Regions.
    pub regions: Vec<Region>,
    /// The heights language: constants, bodies, pins, defaults; `None` for
    /// a tile that uses none of it.
    pub heights: Heights,
    /// The stack has a `base` line: the connector of the build target goes
    /// under zero. Without one the tile has no base and prints flat from zero.
    pub has_base: bool,
    /// Where the `base` line is written, when there is one (the preview
    /// selects it like any entry).
    pub base_span: Option<SourceSpan>,
    /// The entries that call built elements, definitions inlined.
    pub stack: Vec<Entry>,
    /// What the document asks for that is not built yet: one error each,
    /// at its text.  `check` accepts these; `build` refuses them.
    pub pending: Vec<Diag>,
    /// Notes from reading: nothing wrong, but look (a weighting on a range
    /// of no width).
    pub notes: Vec<Diag>,
    /// The tile's own text.
    pub src: Src,
    /// Where its `tile` keyword is written.
    pub span: SourceSpan,
}

impl Tile {
    /// Replace the root seed, wobbling the wobbled lines again from it
    /// (a `--seed` reaches the lines as well as the elements).
    pub fn reseed(&mut self, seed: u64) {
        self.seed = seed;
        for (name, (pts, wobble)) in &self.wobbles {
            self.lines
                .insert(name.clone(), wobbled(pts, *wobble, seed, name));
        }
        for (name, l) in &self.drawn_lines {
            let (base, pts) = l.lay(seed, name);
            if l.wobble > 0.0 {
                self.wobbles.insert(name.clone(), (base, l.wobble));
            }
            self.lines.insert(name.clone(), pts);
        }
        for r in &mut self.regions {
            for (i, d) in &r.fill_draws {
                r.fills[*i] = draw_spot(d, fill_key(seed, &r.name, *i));
            }
        }
    }
}

/// A line some of whose points are drawn from ranges.
#[derive(Clone, Debug)]
pub struct DrawnLine {
    /// Its points in order.
    pub spots: Vec<Spot>,
    /// `closed=#true`.
    pub closed: bool,
    /// How far it wanders off its points, mm.
    pub wobble: f64,
}

/// A point of a line: written, or drawn from the ranges it holds.
#[derive(Clone, Debug)]
pub enum Spot {
    /// A fixed place, in squares.
    Fixed([f64; 2]),
    /// A range for each coordinate, drawn once.
    Drawn([Draw; 2]),
}

impl DrawnLine {
    /// The line for tile seed `seed`: its points as drawn (closed when it
    /// is), and those wobbled.
    fn lay(&self, seed: u64, name: &str) -> (Vec<[f64; 2]>, Vec<[f64; 2]>) {
        let mut pts: Vec<[f64; 2]> = self
            .spots
            .iter()
            .enumerate()
            .map(|(i, s)| match s {
                Spot::Fixed(xy) => *xy,
                Spot::Drawn(d) => draw_spot(d, through_key(seed, name, i)),
            })
            .collect();
        if self.closed && pts.len() > 1 {
            pts.push(pts[0]);
        }
        let out = if self.wobble > 0.0 {
            wobbled(&pts, self.wobble, seed, name)
        } else {
            pts.clone()
        };
        (pts, out)
    }
}

/// The key of the `i`th point of line `name`'s draw.
fn through_key(seed: u64, name: &str, i: usize) -> Seed {
    Seed::new(seed).key("through").key(name).key(&i.to_string())
}

/// The key of the `i`th fill point of region `name`'s draw.
fn fill_key(seed: u64, name: &str, i: usize) -> Seed {
    Seed::new(seed).key("fill").key(name).key(&i.to_string())
}

/// A point drawn once from its ranges (a number never draws), in squares.
fn draw_spot(d: &[Draw; 2], key: Seed) -> [f64; 2] {
    let mut rng = dt_kit::Rng::new(key);
    let mut one = |c: &Draw| {
        if c.is_point() {
            c.lo()
        } else {
            c.draw(&mut rng)
        }
    };
    let x = one(&d[0]);
    let y = one(&d[1]);
    [x, y]
}

/// A written point as a [`Spot`]: fixed, or its ranges to draw from.
fn spot(
    p: &dt_doc::place::Point,
    cols: f64,
    rows: f64,
    named: &BTreeMap<String, [f64; 2]>,
) -> Result<Spot, crate::lower::Refused> {
    if let Some(xy) = fixed_point(p, cols, rows, named) {
        return Ok(Spot::Fixed(xy));
    }
    match p {
        Point::Pair { x, y } => Ok(Spot::Drawn([
            crate::lower::coordinate(x)?,
            crate::lower::coordinate(y)?,
        ])),
        // a name no point declares: decode found it already
        _ => Err(crate::lower::Refused::mistake(
            "this point has no place".to_owned(),
        )),
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
    crate::hash::bytes(crate::hash::OFFSET, text.as_bytes())
}

/// The path a `use` line in `from` names: the library for a bare or
/// `lib:` path, the project (relative to `from`) for `./` or `../`.
#[must_use]
pub fn use_path(from: &str, u: &Use) -> String {
    match &u.source {
        model::Source::Library(p) => format!("{LIB}{p}.dtile"),
        model::Source::Project(p) => resolve(from, p),
    }
}

/// Read the document at `path` and its imports.  A tile is flattened
/// into a [`Tile`]; any other file (parts, printers, heights, edges) is
/// only decoded.
///
/// # Errors
/// Every problem found, each at its text.
pub fn read(sources: &dyn Source, path: &str, preview: bool) -> Result<Read, Vec<Diag>> {
    read_with(sources, path, preview, dt_doc::units::Units::default())
}

/// [`read`] for a build target: `units` says what `mm`, `in` and `layer`
/// are worth in a format 1 document (the system's square, the printer's
/// layer).  A format 0 document does not read them.
///
/// # Errors
/// Every problem found, each at its text.
pub fn read_with(
    sources: &dyn Source,
    path: &str,
    preview: bool,
    units: dt_doc::units::Units,
) -> Result<Read, Vec<Diag>> {
    let mut r = Reader {
        units,
        preview,
        entry_tables: BTreeMap::new(),
        pending_reads: Vec::new(),
        reads: Vec::new(),
        sources,
        files: BTreeMap::new(),
        loading: Vec::new(),
        errors: Vec::new(),
        entries: 0,
        notes: Vec::new(),
        top: None,
        dims: (1.0, 1.0),
        points: BTreeMap::new(),
        widths: BTreeMap::new(),
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
    let r = read(sources, path, false)?;
    let Some(p) = &r.file.doc.printer else {
        return Err(vec![
            Diag::plain("dt::printer", format!("`{path}` has no `printer`"))
                .help("a printer file holds `printer name { line …; layer …; overhang …; steepest …; gap … }`"),
        ]);
    };
    Ok(
        dt_kit::Printer::new(p.line.value, p.layer.value, p.overhang.value, p.gap.value)
            .steepest(p.steepest.value),
    )
}

/// A place word an element does not take, where it is written.
fn misplaced(src: &Src, span: SourceSpan, word: &str, on: &str) -> Diag {
    Diag::at(
        src,
        span,
        "dt::place",
        format!("`{word}` does not place `{on}`"),
        "not a place word for this",
    )
    .help("a scattered thing is placed by its scatter: put `at=` on the `scatter` line")
}

/// "`what` not available yet", at `span`: the language has it, the
/// elements do not build it yet.
fn not_yet(src: &Src, span: SourceSpan, what: &str) -> Diag {
    Diag::at(
        src,
        span,
        "dt::not-yet",
        format!("{what} not available yet"),
        "not built yet",
    )
    .help("`check` accepts this; `build` does not, yet")
}

/// A read that cannot be satisfied, at its text.
fn read_flaw(src: &Src, span: SourceSpan, flaw: &dt_doc::consts::ReadFlaw) -> Diag {
    use dt_doc::consts::ReadFlaw;
    match flaw {
        ReadFlaw::Unknown { name, near } => Diag::at(
            src,
            span,
            "dt::unknown-name",
            format!("nothing installed `{name}` where this is read"),
            "not in the context",
        )
        .help(match near {
            Some(n) => format!("did you mean `{n}`? a `$` name is read from the context of whoever calls, and the library installs its heights at the root"),
            None => format!("a `$` name is read from the context of whoever calls: the library installs its heights at the root, and `let {name} …` in an enclosing block installs one of your own"),
        }),
        ReadFlaw::PerPoint { name } => Diag::at(
            src,
            span,
            "dt::type",
            format!("`{name}` is worked out at each point, and this is drawn once"),
            "a count or size is not per point",
        )
        .help("a position or a height under goes where a height is worked out at each point: a top, a pin or a level"),
        ReadFlaw::NotSet {
            name,
            element,
            sets,
        } => Diag::at(
            src,
            span,
            "dt::unknown-name",
            format!("`{element}` does not set `{name}`"),
            "not set here",
        )
        .help(format!(
            "`{element}` sets {}; a body (dirt, water, a floor, a cliff) works its top out at each node and sets the position and the depth and angle round its region, a tree's top is worked out once over the ground at its foot",
            sets.join(", ")
        )),
        ReadFlaw::Loop => Diag::at(
            src,
            span,
            "dt::value",
            "names read each other in a loop",
            "a loop",
        )
        .help("a `let` that reads itself needs `override`, which reads the value it replaces"),
    }
}

/// Where a formula is read: drawn once (a count, a size), or worked out at
/// each point by an element, which sets only the names
/// [`dt_doc::words::sets`] says.
#[derive(Clone, Copy)]
enum Reads<'a> {
    Once,
    At(&'a str),
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
    /// The tile's `let`s (library heights included): numbers in mm, and the
    /// ranges they hold.
    lets: Arc<Lets>,
    area: Option<Spanned<String>>,
    /// Place words a definition call passes to the elements inside it.
    at: Option<Spanned<Point>>,
    along: Option<Spanned<Along>>,
    /// A definition call's `top=`, for the tree inside it, and where it
    /// was written.  The outermost call's wins.
    top: Option<(Spanned<Expr>, Src)>,
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
    /// Notes (nothing wrong, but look): a weighting on a range of no width.
    notes: Vec<Diag>,
    /// The tile's own entry being inlined (where a blow-up is reported).
    top: Option<(Src, SourceSpan)>,
    /// The tile's squares across and up, for edge paths and points.
    dims: (f64, f64),
    /// Every point of the tile by name (built in and declared), squares.
    points: BTreeMap<String, [f64; 2]>,
    /// The width of each line that has one, mm at the reference square.
    widths: BTreeMap<String, f64>,
    /// Read the format in development (format 1) too.
    preview: bool,
    /// What `mm`, `in` and `layer` are worth (format 1).
    units: dt_doc::units::Units,
    /// The context each of the tile's own stack entries is read in, by the
    /// offset of its text.
    entry_tables: BTreeMap<usize, Arc<ConstTable>>,
    /// Formulas read for the entry being expanded, until it is pushed.
    pending_reads: Vec<RegionRead>,
    /// Formulas read for the tile's entries, owners settled.
    reads: Vec<RegionRead>,
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
        let mut known = Known::new()
            .with_format_preview(self.preview)
            .with_units(self.units);
        // this file's format, to hold the files it uses to it (a header
        // that is wrong is reported when the file is decoded)
        let format = dt_doc::header::check_text_with(path, &text, self.preview)
            .ok()
            .flatten()
            .map_or(0, |h| h.format);
        if path != HEIGHTS {
            // every file sees the library's heights
            let heights = self.load(HEIGHTS, None)?;
            known.add_consts(&heights.doc);
        }
        let mut ok = true;
        self.loading.push(path.to_owned());
        for u in &uses {
            match self.load(&use_path(path, u), Some((&src, u.path.span))) {
                Some(f) if f.doc.format() != format => {
                    ok = false;
                    self.errors.push(
                        Diag::at(
                            &src,
                            u.path.span,
                            "dt::format-mixed",
                            format!(
                                "`{}` is format {}, and this document is format {format}",
                                u.key(),
                                f.doc.format()
                            ),
                            "a part of another format",
                        )
                        .help("a tile and the parts it uses share a format; bring the part to this document's format (`dharmatiles migrate` will do it, once format 1 ships)"),
                    );
                }
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

    /// What went wrong entering a scope, each at its text in `src`.
    fn scope_errors(&mut self, src: &Src, errors: Vec<dt_doc::consts::ScopeError>) {
        for e in errors {
            self.errors
                .push(Diag::at(src, e.span, e.code, e.message, e.label).maybe_help(e.help));
        }
    }

    /// The scope a block makes: `scope` with the block's `let`s and
    /// `override`s (format 1; nothing else has any).
    fn scoped(&mut self, scope: &Scope, consts: &[model::Const]) -> Scope {
        let Some(table) = scope.lets.table.as_ref().filter(|t| t.is_dollar()) else {
            return scope.clone();
        };
        if consts.is_empty() {
            return scope.clone();
        }
        let (table, errors) = table.enter(consts);
        self.scope_errors(&scope.file.src, errors);
        Scope {
            lets: Arc::new(Lets::from_consts(&table)),
            ..scope.clone()
        }
    }

    /// Whether `expr`, written at `span` in `src`, can be read in the
    /// context `table` (format 1): every `$` name is installed, and a
    /// position or height under is only read where a height is worked out
    /// at each point.  A mistake is reported at its text.
    fn readable(
        &mut self,
        table: &ConstTable,
        expr: &Expr,
        span: SourceSpan,
        src: &Src,
        reads: Reads<'_>,
    ) -> bool {
        if !table.is_dollar() {
            return true;
        }
        let checked = match reads {
            Reads::Once => table.check_reads(expr, false),
            Reads::At(element) => table.check_reads_for(expr, element),
        };
        match checked {
            Ok(()) => true,
            Err(flaw) => {
                self.errors.push(read_flaw(src, span, &flaw));
                false
            }
        }
    }

    fn tile(&mut self, file: &Arc<File>) -> Option<Tile> {
        let def = file.doc.tile()?;
        let header = def.header()?;
        let src = &file.src;
        let library = self.load(HEIGHTS, None)?;
        let library_consts: Vec<dt_doc::model::Const> = library
            .doc
            .consts
            .iter()
            .filter(|c| c.kind == dt_doc::model::ConstKind::Let)
            .cloned()
            .collect();
        let format1 = file.doc.format() >= 1;
        let consts = if format1 {
            // format 1: the library's heights are `$` names at the root, and
            // the tile's `let`s and `override`s are its first scope
            let (t, errors) = ConstTable::root(&library_consts).enter(&def.consts);
            self.scope_errors(src, errors);
            t
        } else {
            ConstTable::build(&library_consts, &def.consts)
        };
        let lets = Arc::new(Lets::from_consts(&consts));
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
            format: file.doc.format(),
            region_reads: Vec::new(),
            cols: header.cols,
            rows: header.rows,
            seed,
            lines: BTreeMap::new(),
            widths: BTreeMap::new(),
            drawn_lines: BTreeMap::new(),
            wobbles: BTreeMap::new(),
            regions: Vec::new(),
            heights: Heights::default(),
            has_base: def.base.is_some(),
            base_span: def.base.as_ref().map(|b| b.span),
            stack: Vec::new(),
            pending: Vec::new(),
            notes: Vec::new(),
            src: Arc::clone(src),
            span: def.name.span,
        };
        let (cols, rows) = (f64::from(t.cols), f64::from(t.rows));
        self.dims = (cols, rows);
        self.points = BTreeMap::new();
        self.widths = BTreeMap::new();
        for n in dt_doc::place::BUILTIN_POINTS {
            if let Some((x, y)) = dt_doc::place::builtin_point(n, cols, rows) {
                self.points.insert(n.to_owned(), [x, y]);
            }
        }
        for p in &def.points {
            self.points.insert(p.name.value.clone(), [p.x, p.y]);
        }
        for l in &def.lines {
            if let Some(w) = l.width.as_ref()
                && self.readable(&consts, &w.value.expr, w.span, src, Reads::Once)
            {
                match wobble_mm(&w.value, &lets) {
                    Ok(v) if v.is_finite() && v > 0.0 && v <= MAX_LINE_WIDTH_MM => {
                        self.widths.insert(l.name.value.clone(), v);
                        t.widths.insert(l.name.value.clone(), v);
                        t.regions.push(Region {
                            name: strip_name(&l.name.value),
                            bounded_by: Vec::new(),
                            fills: Vec::new(),
                            fill_draws: Vec::new(),
                            within: vec![Within {
                                mm: v / 2.0,
                                range: None,
                                of: PlaceRef::Name(l.name.value.clone()),
                                span: w.span,
                            }],
                            except: Vec::new(),
                            span: l.name.span,
                            strip: true,
                        });
                    }
                    got => {
                        let why = got.err().map_or_else(
                            || format!("a line's width is above 0 and up to {MAX_LINE_WIDTH_MM} mm"),
                            |e| format!("a line's width is a length above 0, up to {MAX_LINE_WIDTH_MM} mm: {e}"),
                        );
                        self.errors
                            .push(Diag::at(src, w.span, "dt::value", why, "here"));
                    }
                }
            }
            let wobble_read = l
                .wobble
                .as_ref()
                .is_none_or(|w| self.readable(&consts, &w.value.expr, w.span, src, Reads::Once));
            let wobble = match l
                .wobble
                .as_ref()
                .filter(|_| wobble_read)
                .map(|w| (w, wobble_mm(&w.value, &lets)))
            {
                None => 0.0,
                Some((_, Ok(v))) if v.is_finite() && (0.0..=35.0).contains(&v) => v,
                Some((w, got)) => {
                    // a value out of range, or one that is not a length
                    // (an unknown level): say which
                    let why = got.err().map_or_else(
                        || "a line's wobble is from 0 to 35 mm".to_owned(),
                        |e| format!("a line's wobble is a length from 0 to 35 mm: {e}"),
                    );
                    self.errors
                        .push(Diag::at(src, w.span, "dt::value", why, "here"));
                    0.0
                }
            };
            let mut spots = Vec::new();
            for p in &l.through {
                match spot(&p.value, cols, rows, &self.points) {
                    Ok(sp) => spots.push(sp),
                    Err(r) => {
                        let mut d = Diag::at(src, p.span, r.code, r.message, "here");
                        d.help = r.help;
                        self.errors.push(d);
                    }
                }
            }
            let line = DrawnLine {
                spots,
                closed: l.closed,
                wobble,
            };
            let (base, pts) = line.lay(t.seed, &l.name.value);
            if wobble > 0.0 {
                t.wobbles.insert(l.name.value.clone(), (base, wobble));
            }
            if line.spots.iter().any(|s| matches!(s, Spot::Drawn(_))) {
                t.drawn_lines.insert(l.name.value.clone(), line);
            }
            t.lines.insert(l.name.value.clone(), pts);
        }
        for r in &def.regions {
            let mut fills = Vec::new();
            let mut fill_draws = Vec::new();
            for p in &r.fills {
                match spot(&p.value, cols, rows, &self.points) {
                    Ok(Spot::Fixed(xy)) => fills.push(xy),
                    Ok(Spot::Drawn(d)) => {
                        fill_draws.push((fills.len(), d.clone()));
                        fills.push(draw_spot(&d, fill_key(t.seed, &r.name.value, fills.len())));
                    }
                    Err(e) => {
                        let mut d = Diag::at(src, p.span, e.code, e.message, "here");
                        d.help = e.help;
                        self.errors.push(d);
                    }
                }
            }
            let mut within = Vec::new();
            for w in &r.within {
                if !self.readable(&consts, &w.mm.value, w.mm.span, src, Reads::Once) {
                    continue;
                }
                // a distance is a band: the odds of a weighting have nothing to act on
                if let Some(text) = consts.weighted_in(&w.mm.value) {
                    let mut d = Diag::at(
                        src,
                        w.mm.span,
                        "dt::weighting",
                        format!("a weighted range does not go on a band: `{text}` in a distance"),
                        "within is a band, not a draw",
                    );
                    d.help = Some("write the range alone".to_owned());
                    self.errors.push(d);
                    continue;
                }
                let range = if w.mm.value.is_range() {
                    match (
                        consts.scalar(&w.mm.value, 0.0, &|_| None),
                        consts.scalar(&w.mm.value, 1.0, &|_| None),
                    ) {
                        (Ok(a), Ok(b)) if a.is_finite() && b.is_finite() && a >= 0.0 => {
                            Some((a.min(b), a.max(b)))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                match consts.scalar(&w.mm.value, 0.5, &|_| None) {
                    Ok(mm) if mm >= 0.0 && mm.is_finite() => within.push(Within {
                        mm,
                        range,
                        of: w.of.value.clone(),
                        span: w.of.span,
                    }),
                    Ok(_) => self.errors.push(Diag::at(
                        src,
                        w.mm.span,
                        "dt::value",
                        "a distance is not negative",
                        "here",
                    )),
                    Err(msg) => self.errors.push(Diag::at(
                        src,
                        w.mm.span,
                        "dt::value",
                        format!("can't work out this distance: {msg}"),
                        "here",
                    )),
                }
            }
            t.regions.push(Region {
                name: r.name.value.clone(),
                bounded_by: r.bounded_by.clone(),
                fills,
                fill_draws,
                within,
                except: r.except.iter().map(|e| e.value.clone()).collect(),
                span: r.name.span,
                strip: false,
            });
        }
        let scope = Scope {
            file: Arc::clone(file),
            params: BTreeMap::new(),
            lets,
            area: None,
            at: None,
            along: None,
            top: None,
            seed: None,
            id: None,
            depth: 0,
        };
        let mut stack = Vec::new();
        let mut pending = Vec::new();
        let stack_scope = self.scoped(&scope, &def.stack_consts);
        self.expand(&def.stack, &stack_scope, &mut stack, &mut pending);
        t.stack = stack;
        t.pending.extend(pending);
        t.notes = std::mem::take(&mut self.notes);
        let root = stack_scope
            .lets
            .table
            .clone()
            .unwrap_or_else(|| Arc::new(consts.clone()));
        t.heights = self.heights(file, def, consts, &root, &t.stack, &mut t.pending);
        t.region_reads = std::mem::take(&mut self.reads);
        Some(t)
    }

    /// What the heights language wrote in the tile: its bodies, pins and
    /// the library's defaults.  What `build` does not do yet goes to
    /// `pending`.
    fn heights(
        &mut self,
        file: &Arc<File>,
        def: &Definition,
        consts: ConstTable,
        root: &Arc<ConstTable>,
        stack: &[Entry],
        pending: &mut Vec<Diag>,
    ) -> Heights {
        let src = &file.src;
        let from = Provenance {
            path: file.path.clone(),
            src: Arc::clone(src),
            default: false,
        };
        let pin = |p: &model::Pin, from: &Provenance, dollar: bool| PinDecl {
            name: p.name.as_ref().map(|n| n.value.clone()),
            top: if dollar {
                p.top
                    .value
                    .map_names(&mut |n| Some(Expr::Name(dt_doc::consts::dollar_of(n))))
            } else {
                p.top.value.clone()
            },
            top_span: p.top.span,
            at: p.at.value.clone(),
            at_span: p.at.span,
            span: p.span,
            from: from.clone(),
        };
        let mut bodies = Vec::new();
        let mut things = Vec::new();
        for e in &def.stack {
            let el = match &e.callee {
                Callee::Element(el)
                    if matches!(
                        el.name,
                        "dirt" | "water" | "flagstones" | "planks" | "cliff"
                    ) =>
                {
                    el
                }
                _ => {
                    // a thing, a coat or a part: its top is not built yet,
                    // except a tree's (its height) and a definition call's
                    // (for the tree inside it), which the stack reads
                    let name = e.what.value.as_str();
                    let read = match &e.callee {
                        Callee::Element(el) => el.name == "tree",
                        Callee::Definition(_) => true,
                    };
                    if let Some(t) = e.top.as_ref().filter(|_| !read) {
                        pending.push(not_yet(src, t.span, &format!("`top=` on `{name}` is")));
                    }
                    things.push(thing(e));
                    continue;
                }
            };
            // the entry this body is, found by where it is written
            let Some(entry) = stack
                .iter()
                .position(|s| s.span == e.span && Arc::ptr_eq(&s.src, src))
            else {
                // not an entry of the stack: `expand` left it out after
                // reporting why (an error, or something not built)
                continue;
            };
            let table = self
                .entry_tables
                .get(&e.span.offset())
                .cloned()
                .unwrap_or_else(|| Arc::clone(root));
            // format 1: what the body reads must be there, at each point
            let mut readable = true;
            if let Some(t) = &e.top {
                readable &= self.readable(&table, &t.value, t.span, src, Reads::At(&e.what.value));
            }
            for p in &e.pins {
                readable &= self.readable(
                    &table,
                    &p.top.value,
                    p.top.span,
                    src,
                    Reads::At(&e.what.value),
                );
            }
            if !readable {
                continue;
            }
            if table.is_dollar() {
                let owner = Owner { entry, child: None };
                let formulas = e
                    .top
                    .iter()
                    .map(|t| (&t.value, t.span))
                    .chain(e.pins.iter().map(|p| (&p.top.value, p.top.span)));
                for (expr, span) in formulas {
                    self.reads.push(RegionRead {
                        expr: expr.clone(),
                        span,
                        src: Arc::clone(src),
                        table: Arc::clone(&table),
                        owner,
                    });
                }
            }
            bodies.push(BodyDecl {
                entry,
                element: el.name.to_owned(),
                label: e.label.as_ref().map(|l| l.value.clone()),
                table,
                top: e.top.as_ref().map(|t| (t.value.clone(), t.span)),
                region: e
                    .place
                    .region
                    .as_ref()
                    .map(|r| r.value.clone())
                    .or_else(|| match e.place.along.as_ref().map(|a| &a.value) {
                        Some(Along::Line(n)) if self.widths.contains_key(n) => Some(strip_name(n)),
                        _ => None,
                    }),
                slope: e.slope.as_ref().map(|s| s.value),
                pins: e.pins.iter().map(|p| pin(p, &from, false)).collect(),
                span: e.what.span,
            });
        }
        let mut defaults = Vec::new();
        // the library's defaults are written in format 0: in a format 1 tile
        // their names are the context's `$` names
        let dollar = root.is_dollar();
        let rename = |e: &Expr| {
            if dollar {
                e.map_names(&mut |n| Some(Expr::Name(dt_doc::consts::dollar_of(n))))
            } else {
                e.clone()
            }
        };
        if let Some(f) = self.load(EDGES, None) {
            let dfrom = Provenance {
                path: f.path.clone(),
                src: Arc::clone(&f.src),
                default: true,
            };
            // written in format 1 they have their `$` names already
            let old = dollar && f.doc.format() == 0;
            for d in &f.doc.defaults {
                defaults.push(DefaultBody {
                    element: d.what.value.clone(),
                    top: d.top.as_ref().map(|t| {
                        (
                            if old {
                                rename(&t.value)
                            } else {
                                t.value.clone()
                            },
                            t.span,
                        )
                    }),
                    pins: d.pins.iter().map(|p| pin(p, &dfrom, old)).collect(),
                    from: dfrom.clone(),
                });
            }
        }
        Heights {
            consts,
            points: self.points.clone(),
            closed: def
                .lines
                .iter()
                .filter(|l| l.closed)
                .map(|l| l.name.value.clone())
                .collect(),
            bodies,
            defaults,
            things,
        }
    }

    /// A point written in a part (which has no tile) now that the tile it is
    /// called in is known: in squares from the south-west corner.  Its frame
    /// is `$tile`, or `$region` where the call acts on the whole tile
    /// (`in_region` false); a named region's box is known only once `check`
    /// has the region tree, and a thing is placed before that.
    fn in_tile_frame(
        &self,
        f: &dt_doc::framed::Framed,
        in_region: bool,
    ) -> Result<(Expr, Expr), crate::lower::Refused> {
        use dt_doc::expr::Expr as E;
        let (cols, rows) = self.dims;
        let tile = match &f.frame {
            E::Name(n) if n == "$tile" => true,
            E::Name(n) if n == "$region" => !in_region,
            _ => false,
        };
        if !tile {
            return Err(crate::lower::Refused::mistake(format!(
                "`{}` is in the frame of a region, and this part is called where the thing is placed before the region's box is known",
                f.frame
            ))
            .with_help(
                "write the point against the tile: `point($tile, 50%, 50%)`, or call the part on the whole tile",
            ));
        }
        match f.xy_in([0.0, 0.0, cols, rows], self.units) {
            Ok(Some(xy)) => Ok(xy),
            Ok(None) => {
                let dt_doc::framed::FramedAt::Polar { d, a_deg } = &f.at else {
                    return Err(crate::lower::Refused::mistake("this point has no place"));
                };
                let r = d * 0.5 * cols.min(rows);
                let (cx, cy) = (0.5 * cols, 0.5 * rows);
                Ok((
                    E::num(cx + r * dt_doc::expr::cos_deg(*a_deg)),
                    E::num(cy - r * dt_doc::expr::sin_deg(*a_deg)),
                ))
            }
            Err(ke) => {
                let r = crate::lower::Refused::mistake(ke.message);
                Err(match ke.help {
                    Some(h) => r.with_help(h),
                    None => r,
                })
            }
        }
    }

    /// Inline `entries` into `out`: built elements become [`Entry`]s, what
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
            // an entry's own block of `let`s and `override`s holds for it
            let entry_scope;
            let scope = if e.consts.is_empty() {
                scope
            } else {
                entry_scope = self.scoped(scope, &e.consts);
                &entry_scope
            };
            if scope.depth == 0 {
                self.top = Some((Arc::clone(src), e.span));
                if let Some(t) = &scope.lets.table {
                    self.entry_tables.insert(e.span.offset(), Arc::clone(t));
                }
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
            let mut area = e.place.region.clone().or_else(|| scope.area.clone());
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
                Callee::Element(element) => {
                    if !dt_elements::built().any(|n| n == element.name) {
                        pending.push(not_yet(
                            src,
                            e.what.span,
                            &format!("element `{}`", element.name),
                        ));
                        continue;
                    }
                    let mut ok = true;
                    // inside a definition the decoder leaves a required
                    // place word to the call; it must arrive from there
                    for need in element.needs {
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
                                    element.name
                                ),
                                format!("no `{w}=` here or at the call"),
                            );
                            d.help = Some(format!(
                                "write it here, or pass it at the call: `name {w}=…`"
                            ));
                            self.errors.push(d);
                        }
                    }
                    // at= a path of edges and at= a point, for the elements
                    // that place by them; the rest is not built yet
                    let mut path = None;
                    let mut line = None;
                    let mut side = None;
                    let mut point = None;
                    let against = if element.honours(PlaceWord::Against) {
                        e.place.against.as_ref().map(|a| a.value.clone())
                    } else {
                        None
                    };
                    let mut unplaced = vec![("side=", e.place.side.as_ref().map(|s| s.span))];
                    if !element.honours(PlaceWord::Against) {
                        unplaced.push(("at=foot(…)", e.place.against.as_ref().map(|s| s.span)));
                    }
                    match &along {
                        Some(a) if element.honours(PlaceWord::Along) => match &a.value {
                            Along::Edges(edges) => path = Some(edge_path(edges, self.dims)),
                            Along::Line(name) if element.name == "scatter" => {
                                line = Some(name.clone());
                            }
                            Along::Line(name) if element.name == "wall" => {
                                line = Some(name.clone());
                                side = e.place.side.as_ref().map(|s| s.value);
                                unplaced.retain(|(w, _)| *w != "side=");
                            }
                            Along::Line(_) => unplaced.push(("at= a drawn line", Some(a.span))),
                        },
                        // a dirt, grass or cliff follows a line by its strip
                        Some(a) if element.place.contains(&PlaceWord::Along) => {
                            match self.strip(&a.value, element.name) {
                                Ok(name) => {
                                    area = Some(Spanned {
                                        value: name,
                                        span: a.span,
                                    });
                                }
                                Err((msg, help)) => {
                                    ok = false;
                                    self.errors.push(
                                        Diag::at(src, a.span, "dt::type", msg, "here").help(help),
                                    );
                                }
                            }
                        }
                        // inherited from a definition call: for the elements
                        // that place by it only, the others ignore it
                        Some(a) if e.place.along.is_some() => {
                            unplaced.push(("at= a line", Some(a.span)));
                        }
                        _ => {}
                    }
                    match &at {
                        Some(p) if element.honours(PlaceWord::At) => {
                            let fixed = |((x0, x1), (y0, y1)): dt_doc::place::Bounds| {
                                [Draw::flat(x0, x1), Draw::flat(y0, y1)]
                            };
                            let drawn = match &p.value {
                                Point::Named(n) => self
                                    .points
                                    .get(n)
                                    .map(|q| fixed(((q[0], q[0]), (q[1], q[1]))))
                                    .ok_or_else(|| {
                                        ("dt::at", format!("no point is called `{n}`"), None)
                                    }),
                                Point::Pair { x, y } => crate::lower::coordinate(x)
                                    .and_then(|x| Ok([x, crate::lower::coordinate(y)?]))
                                    .map_err(|r| (r.code, r.message, r.help)),
                                // a part's point: its frame is the tile of
                                // the call site, so it is worked out here
                                Point::Framed(f) => self
                                    .in_tile_frame(f, area.is_some())
                                    .and_then(|(x, y)| {
                                        Ok([
                                            crate::lower::coordinate(&x)?,
                                            crate::lower::coordinate(&y)?,
                                        ])
                                    })
                                    .map_err(|r| (r.code, r.message, r.help)),
                                v => v
                                    .bounds(self.dims.0, self.dims.1)
                                    .map(fixed)
                                    .map_err(|msg| ("dt::at", msg, None)),
                            };
                            match drawn {
                                Ok(d) => point = Some(d),
                                Err((code, msg, help)) => self.errors.push(
                                    Diag::at(src, p.span, code, msg, "here").maybe_help(help),
                                ),
                            }
                        }
                        Some(p) if e.place.at.is_some() => unplaced.push(("at=", Some(p.span))),
                        _ => {}
                    }
                    for (word, span) in unplaced {
                        if let Some(span) = span {
                            ok = false;
                            self.errors.push(misplaced(src, span, word, element.name));
                        }
                    }
                    let mut children = Vec::new();
                    // formulas read for this entry, until it is pushed
                    let mark = self.pending_reads.len();
                    // a scatter inside a surface scatters on that surface:
                    // its own entry, in the surface's area, listed right after it
                    let mut hoisted: Vec<model::Entry> = Vec::new();
                    for c in &e.children {
                        if element.role == Role::Surface
                            && matches!(&c.callee, Callee::Element(cl) if cl.name == "scatter")
                        {
                            hoisted.push(c.clone());
                            continue;
                        }
                        if element.name != "scatter" {
                            ok = false;
                            self.errors.push(
                                Diag::at(
                                    src,
                                    c.what.span,
                                    "dt::nesting",
                                    format!(
                                        "`{}` can't go inside `{}`",
                                        c.what.value, element.name
                                    ),
                                    "not a control here",
                                )
                                .help(format!(
                                    "list `{}` in the stack beside `{}`",
                                    c.what.value, element.name
                                )),
                            );
                            continue;
                        }
                        let before = self.pending_reads.len();
                        match self.scatter_child(c, scope, pending) {
                            Some(child) => {
                                for r in &mut self.pending_reads[before..] {
                                    r.owner.child = Some(children.len());
                                }
                                children.push(child);
                            }
                            None => ok = false,
                        }
                    }
                    let mut controls = self.controls(e, element, scope, pending);
                    // a scatter along a line with a width covers its strip: half
                    // the width either side, unless `reach=` says otherwise
                    if let (Some(cs), Some(name)) = (controls.as_mut(), &line)
                        && let Some(w) = self.widths.get(name)
                        && e.control("reach").is_none()
                    {
                        cs.retain(|c| c.name != "reach");
                        cs.push(Setting {
                            name: "reach".to_owned(),
                            value: dt_elements::Value::Number(w / 2.0),
                            span: e.what.span,
                            src: Arc::clone(src),
                        });
                    }
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
                            line,
                            side,
                            children,
                            src: Arc::clone(src),
                            span: e.span,
                        });
                        let entry = out.len() - 1;
                        for mut r in self.pending_reads.drain(mark..) {
                            r.owner.entry = entry;
                            self.reads.push(r);
                        }
                        if !hoisted.is_empty() {
                            // listed right after the surface it is written in
                            self.expand(&hoisted, &inner, out, pending);
                        }
                    } else {
                        self.pending_reads.truncate(mark);
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
                    let params = Self::params(e, &def, &file, scope);
                    let top = self.call_top(e, &def, scope, pending);
                    let inner = Scope {
                        file: Arc::clone(&file),
                        params,
                        lets: scope.lets.clone(),
                        area,
                        at,
                        along,
                        top,
                        seed,
                        id,
                        depth: scope.depth + 1,
                    };
                    // a definition's own `let`s and `override`s hold for its body
                    let inner = self.scoped(&inner, &def.consts);
                    let inner = self.scoped(&inner, &def.stack_consts);
                    self.expand(&def.stack, &inner, out, pending);
                }
            }
        }
    }

    /// The `top=` a call of `def` passes to the tree inside it: the
    /// outermost call's, else this one's.  A `top=` on a call holding no
    /// tree has nothing to stand on yet.
    fn call_top(
        &mut self,
        e: &model::Entry,
        def: &Definition,
        scope: &Scope,
        pending: &mut Vec<Diag>,
    ) -> Option<(Spanned<Expr>, Src)> {
        let src = &scope.file.src;
        if let Some(t) = &e.top {
            let holds_tree = def.stack.iter().any(|x| match &x.callee {
                Callee::Element(el) => el.name == "tree",
                Callee::Definition(_) => true,
            });
            if !holds_tree {
                pending.push(not_yet(
                    src,
                    t.span,
                    &format!("`top=` on `{}`, which holds no tree, is", e.what.value),
                ));
            }
        }
        scope
            .top
            .clone()
            .or_else(|| e.top.clone().map(|t| (t, Arc::clone(src))))
    }

    /// What a call of `def` (defined in `file`) binds its parameters to: the
    /// call's arguments, else the parameters' defaults.
    fn params(
        e: &model::Entry,
        def: &Definition,
        file: &Arc<File>,
        scope: &Scope,
    ) -> BTreeMap<String, Bound> {
        let mut params = BTreeMap::new();
        for p in &def.params {
            let arg = e
                .control(&p.name.value)
                .and_then(|s| s.value.as_ref())
                .map(|v| bind(v, &scope.file.src, scope))
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
        params
    }

    /// One child of a scatter: a thing, or a definition whose stack is one
    /// thing (`scatter count=2 { oak }`), with its controls.  `None` when
    /// it cannot be given yet or is refused (both are reported).
    fn scatter_child(
        &mut self,
        c: &model::Entry,
        scope: &Scope,
        pending: &mut Vec<Diag>,
    ) -> Option<Child> {
        // a child's own block of `let`s and `override`s holds for it
        let child_scope;
        let scope = if c.consts.is_empty() {
            scope
        } else {
            child_scope = self.scoped(scope, &c.consts);
            &child_scope
        };
        let src = &scope.file.src;
        let (cl, inner, thing) = match &c.callee {
            Callee::Element(cl) if cl.scattered => (*cl, scope.clone(), c.clone()),
            Callee::Element(cl) => {
                self.errors.push(
                    Diag::at(
                        src,
                        c.what.span,
                        "dt::nesting",
                        format!("`{}` can't be scattered", cl.name),
                        "not a thing",
                    )
                    .help("a scatter holds rocks, flowers and trees"),
                );
                return None;
            }
            Callee::Definition(name) => {
                let Some((file, def)) = self.definition(&scope.file, name) else {
                    self.errors.push(Diag::at(
                        src,
                        c.what.span,
                        "dt::unknown-name",
                        format!("cannot find the definition `{name}`"),
                        "not loaded",
                    ));
                    return None;
                };
                let one = match def.stack.as_slice() {
                    [one] if scope.depth < 64 => one,
                    _ => {
                        self.errors.push(
                            Diag::at(
                                src,
                                c.what.span,
                                "dt::nesting",
                                format!("`{name}` is not one thing, so a scatter cannot hold it"),
                                "not one thing",
                            )
                            .help(
                                "a scatter holds a definition whose stack is one rock, flower or tree",
                            ),
                        );
                        return None;
                    }
                };
                let top = self.call_top(c, &def, scope, pending);
                let inner = Scope {
                    file: Arc::clone(&file),
                    params: Self::params(c, &def, &file, scope),
                    lets: scope.lets.clone(),
                    area: None,
                    at: None,
                    along: None,
                    top,
                    seed: None,
                    id: None,
                    depth: scope.depth + 1,
                };
                let inner = self.scoped(&inner, &def.consts);
                let inner = self.scoped(&inner, &def.stack_consts);
                let mut child = self.scatter_child(one, &inner, pending)?;
                // the call's own name is where it is shown
                child.what.span = c.what.span;
                child.weight = c.place.weight.as_ref().map_or(1.0, |w| w.value);
                return Some(child);
            }
        };
        let mut ok = true;
        for (word, span) in [
            ("at= a line", c.place.along.as_ref().map(|s| s.span)),
            ("side=", c.place.side.as_ref().map(|s| s.span)),
            ("at=", c.place.at.as_ref().map(|s| s.span)),
            ("at=foot(…)", c.place.against.as_ref().map(|s| s.span)),
            ("at= a region", c.place.region.as_ref().map(|s| s.span)),
        ] {
            if let Some(span) = span {
                ok = false;
                self.errors.push(misplaced(
                    &inner.file.src,
                    span,
                    word,
                    &format!("a scatter's `{}`", cl.name),
                ));
            }
        }
        if cl.name != "tree"
            && let Some(t) = &thing.top
        {
            ok = false;
            pending.push(not_yet(
                &inner.file.src,
                t.span,
                &format!("`top=` on `{}` is", cl.name),
            ));
        }
        let controls = self.controls(&thing, cl, &inner, pending)?;
        ok.then(|| Child {
            what: Spanned {
                value: cl.name.to_owned(),
                span: c.what.span,
            },
            controls,
            weight: c.place.weight.as_ref().map_or(1.0, |w| w.value),
        })
    }

    /// The strip a surface follows when it is placed `at=` `along`: the
    /// name of the region the line's width makes.
    ///
    /// # Errors
    /// What is wrong and how to write it: an edge has no width, and a
    /// line needs one.
    fn strip(&self, along: &Along, element: &str) -> Result<String, (String, String)> {
        match along {
            Along::Line(name) if self.widths.contains_key(name) => Ok(strip_name(name)),
            Along::Line(name) => Err((
                format!("`{element}` follows the strip of a line, and `{name}` has no width"),
                format!("give it one: `line {name} width=9 {{ through … }}`"),
            )),
            Along::Edges(_) => Err((
                format!("`{element}` follows the strip of a line, and a tile edge has no width"),
                "write a region beside the edge and use it: `region rim { within 6 of=northEdge }`, then `at=rim`"
                    .to_owned(),
            )),
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

    /// The controls a built element takes, as written or by default; `None`
    /// when one cannot be given yet (it is in `pending`).
    fn controls(
        &mut self,
        e: &model::Entry,
        element: &'static Element,
        scope: &Scope,
        pending: &mut Vec<Diag>,
    ) -> Option<Vec<Setting>> {
        let src = &scope.file.src;
        let mut out = Vec::new();
        let mut ok = true;
        // a tree's height is its `top`: the outermost call's, else its own
        if element.name == "tree"
            && let Some((t, at)) = scope
                .top
                .clone()
                .or_else(|| e.top.clone().map(|t| (t, Arc::clone(src))))
        {
            if let Some(table) = scope.lets.table.clone() {
                if !self.readable(&table, &t.value, t.span, &at, Reads::At("tree")) {
                    return None;
                }
                if table.is_dollar() {
                    self.pending_reads.push(RegionRead {
                        expr: t.value.clone(),
                        span: t.span,
                        src: Arc::clone(&at),
                        table,
                        owner: Owner {
                            entry: usize::MAX,
                            child: None,
                        },
                    });
                }
            }
            match crate::lower::tree_top(&t.value, &scope.lets) {
                Ok((name, Lowered { value, note })) => {
                    if let Some(note) = note {
                        self.notes
                            .push(Diag::at(&at, t.span, "dt::weighting", note, "here"));
                    }
                    out.push(Setting {
                        name: name.to_owned(),
                        value,
                        span: t.span,
                        src: at,
                    });
                }
                Err(why) => {
                    let mut d = Diag::at(&at, t.span, why.code, why.message, "here");
                    d.help = why.help;
                    if why.hard {
                        self.errors.push(d);
                    } else {
                        pending.push(d);
                    }
                    return None;
                }
            }
        }
        for s in &e.controls {
            // the control's own value, then its sub-controls as `control/sub`
            let mut items: Vec<Item<'_>> = Vec::new();
            // numbers written with no word adjust the default word's numbers
            // (`rock columns=50` leans the mixed cliff), so that word comes first
            if s.value.is_none()
                && !s.subs.is_empty()
                && let Some(c) = s.control
                && !c.words.is_empty()
                && let Some(raw) = c.default_value()
                && let Ok(Lowered { value, .. }) = element_value(&raw, &scope.lets)
            {
                push_setting(&mut out, &s.name.value, &raw, value, s.span, src);
            }
            if let Some(v) = &s.value {
                items.push(Item {
                    name: s.name.value.clone(),
                    owner: element.name,
                    value: v,
                    span: s.span,
                    control: s.control,
                });
            }
            for sub in &s.subs {
                let name = format!("{}/{}", s.name.value, sub.name.value);
                match &sub.value {
                    Some(v) if element.takes(&name) => {
                        items.push(Item {
                            name,
                            owner: &s.name.value,
                            value: v,
                            span: sub.span,
                            control: sub.control,
                        });
                    }
                    _ => {
                        ok = false;
                        pending.push(not_yet(
                            src,
                            sub.name.span,
                            &format!("sub-controls of `{}` are", s.name.value),
                        ));
                    }
                }
            }
            if s.value.is_none() && s.subs.is_empty() {
                ok = false;
                pending.push(not_yet(
                    src,
                    s.name.span,
                    &format!("the group `{}` on `{}` is", s.name.value, element.name),
                ));
            }
            for Item {
                name,
                owner,
                value: v,
                span,
                control,
            } in items
            {
                let b = bind(v, src, scope);
                // format 1: a count or size reads installed names, and none
                // that is worked out at each point
                if let Value::Num(n) = &b.value.value
                    && let Some(table) = scope.lets.table.clone()
                    && !self.readable(&table, &n.expr, b.value.span, &b.src, Reads::Once)
                {
                    ok = false;
                    continue;
                }
                let value = match value(&b, &scope.lets) {
                    Ok(Lowered { value, note }) => {
                        if let Some(note) = note {
                            self.notes.push(Diag::at(
                                &b.src,
                                b.value.span,
                                "dt::weighting",
                                note,
                                "here",
                            ));
                        }
                        value
                    }
                    Err((why, hard)) => {
                        ok = false;
                        // a mistake is shown where it was made; a name with
                        // no value yet waits for its caller
                        if hard {
                            self.errors.push(*why);
                        } else {
                            pending.push(*why);
                        }
                        continue;
                    }
                };
                if let Some(c) = control
                    && let Err(why) = checked(c, owner, &value, &b)
                {
                    self.errors.push(*why);
                    continue;
                }
                if !element.takes(&name) {
                    // a control written at its default changes nothing
                    let default = control.and_then(|c| default_value(c, &scope.lets));
                    if default.as_ref() != Some(&value) {
                        ok = false;
                        pending.push(not_yet(
                            src,
                            span,
                            &format!("`{}`'s `{}` is", element.name, s.name.value),
                        ));
                    }
                    continue;
                }
                push_setting(&mut out, &name, &b.value.value, value, b.value.span, &b.src);
            }
        }
        for c in element.controls {
            if element.takes(c.name)
                && e.control(c.name).is_none()
                && let Some(raw) = c.default_value()
                && let Ok(Lowered { value, .. }) = element_value(&raw, &scope.lets)
            {
                push_setting(&mut out, c.name, &raw, value, e.what.span, src);
            }
        }
        ok.then_some(out)
    }
}

/// One value written for a control: its own, or a sub-control's
/// (`crown/width`, owned by `crown`).
struct Item<'a> {
    name: String,
    owner: &'a str,
    value: &'a Spanned<Value>,
    span: SourceSpan,
    control: Option<&'static Control>,
}

/// The schema's check of a value once its names are resolved (decode
/// checked it already when it was a plain number).
fn checked(
    c: &'static Control,
    owner: &str,
    v: &dt_elements::Value,
    b: &Bound,
) -> Result<(), Box<Diag>> {
    let (lo, hi, range) = match v {
        dt_elements::Value::Number(x) => (*x, *x, false),
        dt_elements::Value::Range(lo, hi) => (*lo, *hi, true),
        dt_elements::Value::Dist(d) => (d.lo, d.hi, true),
        dt_elements::Value::Draw(d) => {
            let (lo, hi) = d.bounds();
            (lo, hi, true)
        }
        dt_elements::Value::Word(_) => return Ok(()),
    };
    c.check(owner, lo, hi, range).map_err(|r| {
        let mut d = Diag::at(&b.src, b.value.span, "dt::range", r.message, r.label);
        d.help = r.help;
        Box::new(d)
    })
}

/// One setting for an element, and, for a word that stands for numbers
/// (`clods some`), each number as `control.name` (`clods.density`).
fn push_setting(
    out: &mut Vec<Setting>,
    name: &str,
    raw: &Value,
    value: dt_elements::Value,
    span: SourceSpan,
    src: &Src,
) {
    if let Value::Word(w) = raw {
        for (k, v) in w.sets {
            out.push(Setting {
                name: format!("{name}.{k}"),
                value: dt_elements::Value::Number(*v),
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

/// A written value for a built element: a range stays a range (the element
/// draws from it), a weighted range keeps its odds.
fn value(b: &Bound, lets: &Lets) -> Result<Lowered, (Box<Diag>, bool)> {
    element_value(&b.value.value, lets).map_err(|why| {
        let mut d = Diag::at(&b.src, b.value.span, why.code, why.message, "here");
        d.help = why.help;
        (Box::new(d), why.hard)
    })
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
/// range, which no built element takes yet).
fn default_value(c: &'static Control, lets: &Lets) -> Option<dt_elements::Value> {
    element_value(&c.default_value()?, lets)
        .ok()
        .map(|l| l.value)
}

fn thing(e: &model::Entry) -> ThingDecl {
    ThingDecl {
        element: e.what.value.clone(),
        top: e.top.as_ref().map(|t| (t.value.clone(), t.span)),
        at: e.at_place.as_ref().map(|a| a.value.clone()),
        span: e.what.span,
    }
}

/// A point in squares, if it is not drawn from a range.
fn fixed_point(
    p: &dt_doc::place::Point,
    cols: f64,
    rows: f64,
    named: &BTreeMap<String, [f64; 2]>,
) -> Option<[f64; 2]> {
    if let dt_doc::place::Point::Named(n) = p {
        return named.get(n).copied();
    }
    if p.is_range() {
        return None;
    }
    let ((x, _), (y, _)) = p.bounds(cols, rows).ok()?;
    Some([x, y])
}

/// A path of tile edges (each meeting the next at a corner) as one
/// polyline in squares, walked with the tile inside on the left: the
/// path a run's outer face follows (`north west` on a 1×1 is
/// `(1,1) → (0,1) → (0,0)`, the spine of a north-west corner
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

/// A line's wobble: one number of mm, its names the tile's `let`s.
fn wobble_mm(n: &model::Num, lets: &Lets) -> Result<f64, String> {
    if let Some(x) = n
        .expr
        .names()
        .into_iter()
        .find(|x| lets.draws.iter().any(|(k, _)| k == x))
    {
        return Err(format!("`{x}` is a range; one number is needed here"));
    }
    n.mm(&lets.nums)
}
