//! The evaluator's side of the model: what a tile is once read.

use std::collections::BTreeMap;
use std::sync::Arc;

use dt_doc::consts::ConstTable;
use dt_doc::expr::Expr;
use dt_doc::model::{Document, PlaceRef, Spanned};
use dt_doc::place::{Point, Side};
use dt_kit::{Draw, EntryId, Seed};
use miette::SourceSpan;

use super::reader::fixed_point;
use crate::diag::{Diag, Src};

/// The edge contract every tile applies at its root.
pub const EDGES: &str = "lib:parts/edges.dtile";

/// The library's heights, every tile's constants: read from disk like any
/// other file, never compiled in.
pub const HEIGHTS: &str = "lib:parts/heights.dtile";

/// A line's wobble follows smooth noise of this sigma along it, mm at the
/// reference square.
pub(super) const LINE_WOBBLE_SIGMA_MM: f64 = 9.0;

/// The widest strip a line makes, mm at the reference square (two squares).
pub(super) const MAX_LINE_WIDTH_MM: f64 = 70.0;

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

/// Where a control an element is given came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The document wrote it.
    Written,
    /// A number a written word stands for (`clods some` gives
    /// `clods.density`): never a control of its own.
    Word,
    /// The schema's default, filled in where nothing was written.
    Default,
    /// Worked out from the line a scatter runs along (its strip's half
    /// width is the default `reach`).
    LineWidth,
}

/// A control an element is given, resolved to mm or a word.
#[derive(Clone, Debug)]
pub struct Setting {
    /// Where it came from: only a written one is a row of the sheet.
    pub origin: Origin,
    /// The control's name.
    pub name: String,
    /// Its value.
    pub value: dt_elements::Value,
    /// Where it was written (the entry's name for a default).
    pub span: SourceSpan,
    /// The text it was written in.
    pub src: Src,
}

/// One stack entry that calls a built element, definitions inlined.  It
/// carries everything the document says of it, the heights language's `top=`,
/// pins and `slope` included: there is no table beside the stack that holds
/// them.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Who it is: its seed path, `tile/<element>/<k or id>`, assigned once
    /// when the document is read (see [`EntryId`]).
    pub id: EntryId,
    /// The element named.
    pub what: Spanned<String>,
    /// `at=` a region: where it acts in (default the whole tile).
    pub area: Option<Spanned<String>>,
    /// `seed=`: pins its element.
    pub seed: Option<u64>,
    /// Its name, if written (a body's, for the preview).
    pub label: Option<String>,
    /// The controls the element takes: written, or their defaults.
    pub controls: Vec<Setting>,
    /// Its `top=`, where its element reads one: a body's band, or the height
    /// of a tree or rock (its formula).
    pub top: Option<Top>,
    /// A body's pins, in order.
    pub pins: Vec<PinDecl>,
    /// A body's `slope`, degrees.
    pub slope: Option<f64>,
    /// The context its `top=` and pins are read in: the constants of the
    /// scope it is written in, after its own `let`s.
    pub table: Arc<ConstTable>,
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
    /// For an entry a definition call makes: the call, as the node the
    /// document being read writes it (its text and span); `None` for an entry
    /// the document writes itself.  Entries one call makes share it: a move,
    /// a hide or a delete of one of them is of the call.
    pub call: Option<(Src, SourceSpan)>,
}

/// A `top=` as the document wrote it, with the context it is read in.
#[derive(Clone, Debug)]
pub struct Top {
    /// The formula as written.
    pub expr: Expr,
    /// Where the value is written.
    pub span: SourceSpan,
    /// The whole property, `top=…`: what the editor selects for the cloud.
    pub prop: SourceSpan,
    /// Its text.
    pub src: Src,
    /// For a tree or a rock, the formula its tips or peak are worked out
    /// from, every name it reads expanded (what the control's [`Formula`]
    /// stands for); `None` for a body's.
    ///
    /// [`Formula`]: dt_elements::Formula
    pub crown: Option<Expr>,
}

/// A child of a scatter, as its element takes it.
#[derive(Clone, Debug)]
pub struct Child {
    /// The thing's element.
    pub what: Spanned<String>,
    /// Its controls, written or by default.
    pub controls: Vec<Setting>,
    /// Its `top=` (a tree's or rock's), when written.
    pub top: Option<Top>,
    /// The context that `top=` is read in.
    pub table: Arc<ConstTable>,
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
    /// The library's defaults.
    pub defaults: Vec<DefaultBody>,
}

/// A stack entry as the page names it: who it is, what it is and the words
/// that stand for it (the preview's stack line, the mesh header's stack).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    /// Who it is.
    pub id: EntryId,
    /// The element as written (`scatter`).
    pub what: String,
    /// The element whose page documents it (a scatter's first thing's).
    pub element: String,
    /// What it places, in words: `boulders at the cliff`, `dirt in crag`.
    pub label: String,
}

/// The connector's label and the page that documents it, for the target.
#[must_use]
pub fn base_label(system: &str) -> (&'static str, &'static str) {
    if system == "ol" {
        ("OpenLOCK base", "openlock")
    } else {
        ("DungeonBlocks base", "dungeonblocks")
    }
}

pub(super) fn plural(word: &str) -> String {
    if word.ends_with('s') {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

pub(super) fn and_list(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

impl Entry {
    /// True when `self` is written inside `outer` (a scatter in a surface,
    /// lifted out into its own entry).
    fn inside(&self, outer: &Entry) -> bool {
        let (a, b) = (self.span.offset(), outer.span.offset());
        Arc::ptr_eq(&self.src, &outer.src) && a > b && a + self.span.len() <= b + outer.span.len()
    }

    /// Whether it is a body of the stack (the element schema says which).
    #[must_use]
    pub fn is_body(&self) -> bool {
        dt_doc::words::is_body(&self.what.value)
    }
}

/// A formula the tile writes, as it is read where a thing calls it: `check`
/// settles what `parent(…)` in it can reach once the tile's regions are known
/// (format 1).  Made from the stack ([`Tile::region_reads`]), never stored.
#[derive(Clone, Copy, Debug)]
pub struct RegionRead<'a> {
    /// The formula.
    pub expr: &'a Expr,
    /// Where it is written.
    pub span: SourceSpan,
    /// Its text.
    pub src: &'a Src,
    /// The context it is read in.
    pub table: &'a Arc<ConstTable>,
    /// The stack entry that calls it ([`Tile::stack`]).
    pub entry: usize,
    /// The thing of that entry (a scatter's) that calls it, when it is one.
    pub child: Option<usize>,
}

/// What the evaluator needs from a tile document.
#[derive(Clone, Debug)]
pub struct Tile {
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
    /// Where each entry stands in [`Tile::stack`], by its id.
    pub(super) ids: BTreeMap<EntryId, usize>,
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
    /// The stack entries as the page names them, in order, then the
    /// connector's when the tile has a `base` line (for build target
    /// `system`).
    #[must_use]
    pub fn names(&self, system: &str) -> Vec<Named> {
        let mut out = Vec::with_capacity(self.stack.len() + 1);
        for (i, e) in self.stack.iter().enumerate() {
            let outer = self.stack[..i].iter().rev().find(|p| e.inside(p));
            let what = e.what.value.as_str();
            let (mut label, element) = if what == "scatter" && !e.children.is_empty() {
                let mut names: Vec<String> = Vec::new();
                for c in &e.children {
                    let n = plural(&c.what.value);
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
                // a mixed scatter links to its first thing's page
                (and_list(&names), e.children[0].what.value.as_str())
            } else {
                (what.to_owned(), what)
            };
            if let Some(outer) = outer {
                label = format!("{label} in {}", outer.what.value);
            } else if let Some(against) = &e.against {
                label = format!("{label} at the {against}");
            } else if let Some(area) = &e.area {
                label = format!("{label} in {}", area.value);
            }
            out.push(Named {
                id: e.id.clone(),
                what: what.to_owned(),
                element: element.to_owned(),
                label,
            });
        }
        if self.has_base {
            out.push(Named {
                id: EntryId::new("tile/connector"),
                what: "base".to_owned(),
                element: "connector".to_owned(),
                label: base_label(system).0.to_owned(),
            });
        }
        out
    }

    /// Where the entry `id` stands in [`Tile::stack`]; `None` for an id the
    /// stack does not hold (the connector's, a ground's).
    #[must_use]
    pub fn index_of(&self, id: &EntryId) -> Option<usize> {
        self.ids.get(id).copied()
    }

    /// The formulas the stack's entries call, in the order of the stack, for
    /// the region checks: a body's `top=` and each pin's, a tree's or rock's
    /// `top=`, a scatter's things'.
    #[must_use]
    pub fn region_reads(&self) -> Vec<RegionRead<'_>> {
        let mut out = Vec::new();
        for (entry, e) in self.stack.iter().enumerate() {
            let formulas = e
                .top
                .iter()
                .map(|t| (&t.expr, t.span, &t.src))
                .chain(e.pins.iter().map(|p| (&p.top, p.top_span, &p.from.src)));
            for (expr, span, src) in formulas {
                out.push(RegionRead {
                    expr,
                    span,
                    src,
                    table: &e.table,
                    entry,
                    child: None,
                });
            }
            for (k, c) in e.children.iter().enumerate() {
                if let Some(t) = c.top.as_ref() {
                    out.push(RegionRead {
                        expr: &t.expr,
                        span: t.span,
                        src: &t.src,
                        table: &c.table,
                        entry,
                        child: Some(k),
                    });
                }
            }
        }
        out
    }

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
    pub(super) fn lay(&self, seed: u64, name: &str) -> (Vec<[f64; 2]>, Vec<[f64; 2]>) {
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
pub(super) fn through_key(seed: u64, name: &str, i: usize) -> Seed {
    Seed::new(seed).key("through").key(name).key(&i.to_string())
}

/// The key of the `i`th fill point of region `name`'s draw.
pub(super) fn fill_key(seed: u64, name: &str, i: usize) -> Seed {
    Seed::new(seed).key("fill").key(name).key(&i.to_string())
}

/// A point drawn once from its ranges (a number never draws), in squares.
pub(super) fn draw_spot(d: &[Draw; 2], key: Seed) -> [f64; 2] {
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
pub(super) fn spot(
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
pub(super) fn wobbled(pts: &[[f64; 2]], wobble: f64, seed: u64, name: &str) -> Vec<[f64; 2]> {
    let sq = dt_kit::REFERENCE_SQUARE_MM;
    let mm: Vec<[f64; 2]> = pts.iter().map(|p| [p[0] * sq, p[1] * sq]).collect();
    let seed = Seed::new(seed).key("line").key(name);
    dt_kit::wobble(&mm, wobble, LINE_WOBBLE_SIGMA_MM, 0.25, seed)
        .into_iter()
        .map(|p| [p[0] / sq, p[1] / sq])
        .collect()
}
