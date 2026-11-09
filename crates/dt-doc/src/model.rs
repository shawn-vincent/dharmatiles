//! The typed document model: what dt-eval consumes.
//!
//! One file is a [`Document`]: `use` lines, a file-level levels table,
//! definitions (`define` and `tile`), and, for the library's contract
//! files, a printer profile or the edge contract.  Every name and value
//! keeps the source span it came from, so later passes can still point at
//! the text, and every stack entry keeps the edit path that addresses it
//! in [`crate::edit`].
//!
//! Values are kept as written, in the reference units: sizes in mm at the
//! DungeonBlocks reference, levels as level expressions, words with the
//! numbers they stand for.  Scaling to a system and filling in
//! defaults are the evaluator's job; [`crate::schema::Control::scaling`]
//! and [`crate::schema::Control::default`] say how.

use miette::SourceSpan;

use crate::dim::Dim;
use crate::expr::{Expr, Unit};
use crate::place::{Along, Point, Side};
use crate::schema::{self, Control, Element, Kind, Word};

/// A value with the source span it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    /// The value.
    pub value: T,
    /// Where it was written.
    pub span: SourceSpan,
}

/// One KDL file.
#[derive(Debug, Clone)]
pub struct Document {
    /// `use "parts/grass" lawn long-grass` lines, in order.
    pub uses: Vec<Use>,
    /// `define` and `tile` nodes, in file order.
    pub definitions: Vec<Definition>,
    /// A `printer` profile (library/printers/*.dtile).
    pub printer: Option<Printer>,
    /// File-level `let` constants (library/parts/heights.dtile).
    pub consts: Vec<Const>,
    /// A `defaults { … }` block: the tops and pins every body of its
    /// element starts with (library/parts/edges.dtile).
    pub defaults: Vec<Entry>,
}

impl Document {
    /// A definition by name.
    #[must_use]
    pub fn definition(&self, name: &str) -> Option<&Definition> {
        self.definitions.iter().find(|d| d.name.value == name)
    }

    /// The file's tile, if it has one.
    #[must_use]
    pub fn tile(&self) -> Option<&Definition> {
        self.definitions
            .iter()
            .find(|d| matches!(d.kind, DefKind::Tile(_)))
    }
}

/// Where a `use` path points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The bundled library: `"parts/grass"` or `"lib:parts/grass"`.  The
    /// string is without `lib:` and without `.dtile`.
    Library(String),
    /// The author's project, relative to the using file: `"./mine"`.
    Project(String),
}

/// `use "parts/grass" lawn long-grass`: a file and the definitions it
/// brings in, so a reader knows where every name came from.
#[derive(Debug, Clone)]
pub struct Use {
    /// The path as written.
    pub path: Spanned<String>,
    /// What it points at.
    pub source: Source,
    /// The definitions brought in.
    pub names: Vec<Spanned<String>>,
    /// The whole node.
    pub span: SourceSpan,
}

impl Use {
    /// The key a file is registered under in [`crate::Known`]: the
    /// library path without `lib:`, or the project path as written.
    #[must_use]
    pub fn key(&self) -> &str {
        match &self.source {
            Source::Library(p) | Source::Project(p) => p,
        }
    }
}

/// A tile's size and seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileHeader {
    /// Squares across (east).
    pub cols: u32,
    /// Squares up the page (north).
    pub rows: u32,
    /// The tile seed, if written.
    pub seed: Option<i64>,
}

/// Whether a definition is a tile or a reusable part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefKind {
    /// `define name { … }`
    Define,
    /// `tile size="2x1" seed=… { … }` (named by its file)
    Tile(TileHeader),
}

/// One definition: parameters, levels, lines, regions and a stack.
#[derive(Debug, Clone)]
pub struct Definition {
    /// Its name.
    pub name: Spanned<String>,
    /// Tile or define.
    pub kind: DefKind,
    /// A description (`about "…"`), trimmed.
    pub about: Option<String>,
    /// `param` nodes.
    pub params: Vec<Param>,
    /// `let` and `override` constants, in written order.
    pub consts: Vec<Const>,
    /// Declared `point`s.
    pub points: Vec<PointDecl>,
    /// Drawn lines.
    pub lines: Vec<Line>,
    /// Regions.
    pub regions: Vec<Region>,
    /// The stack's `base` line, if it has one.
    pub base: Option<Base>,
    /// The stack, in written order.  Order matters among surfaces only.
    pub stack: Vec<Entry>,
    /// Solid, surface or both, from the stack.
    pub summary: Kind,
    /// True when the stack is exactly one thing (an `oak`): it is then a
    /// thing itself, placed with `at=` and scatterable.
    pub thing: bool,
    /// The whole node.
    pub span: SourceSpan,
}

impl Definition {
    /// The tile header, for a tile.
    #[must_use]
    pub fn header(&self) -> Option<TileHeader> {
        match self.kind {
            DefKind::Tile(h) => Some(h),
            DefKind::Define => None,
        }
    }

    /// Every entry, depth first (scatter children and scatters inside
    /// surfaces included).
    #[must_use]
    pub fn entries(&self) -> Vec<&Entry> {
        fn walk<'a>(es: &'a [Entry], out: &mut Vec<&'a Entry>) {
            for e in es {
                out.push(e);
                walk(&e.children, out);
            }
        }
        let mut out = Vec::new();
        walk(&self.stack, &mut out);
        out
    }
}

/// `param name default`.  No default means the caller must give it.
#[derive(Debug, Clone)]
pub struct Param {
    /// Its name.
    pub name: Spanned<String>,
    /// Its default, typed by the control it fills when that is known.
    pub default: Option<Spanned<Value>>,
    /// The control the parameter fills inside the definition (`grass
    /// length=length` makes `length` grass's length), if one was found.
    pub control: Option<&'static Control>,
}

/// A drawn line: `line trail width=9 wobble=3 { through south "0.65,1" north }`.
#[derive(Debug, Clone)]
pub struct Line {
    /// Its name.
    pub name: Spanned<String>,
    /// The points it passes through.
    pub through: Vec<Spanned<Point>>,
    /// A width makes it a strip that an element `at=` it covers.
    pub width: Option<Spanned<Num>>,
    /// How far it wanders off its points, mm.
    pub wobble: Option<Spanned<Num>>,
    /// `closed=#true`: the last point joins the first.
    pub closed: bool,
    /// The whole node.
    pub span: SourceSpan,
}

/// A region of a divided tile: the lines that bound it, and paint-bucket
/// fill points.
#[derive(Debug, Clone)]
pub struct Region {
    /// Its name.
    pub name: Spanned<String>,
    /// The lines that divide it off.
    pub bounded_by: Vec<Spanned<String>>,
    /// Fill points.
    pub fills: Vec<Spanned<Point>>,
    /// `within MM of=PLACE`: a disc, a strip or a ring.
    pub within: Vec<Within>,
    /// `except REGION…`: taken out.
    pub except: Vec<Spanned<String>>,
    /// The whole node.
    pub span: SourceSpan,
}

/// A place as written: a name (built in or declared) or a literal point.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaceRef {
    /// `center`, `stream`, `northEdge`, `tile`.
    Name(String),
    /// `"(0.3, 0.7)"`, in squares from the south-west corner.
    Point(f64, f64),
}

impl std::fmt::Display for PlaceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaceRef::Name(n) => f.write_str(n),
            PlaceRef::Point(x, y) => write!(f, "({x}, {y})"),
        }
    }
}

/// `within 4.5 of=stream`: every place within a distance of a place.
#[derive(Debug, Clone)]
pub struct Within {
    /// The distance, mm at the 35 mm square.  A range is drawn along the
    /// edge by the kit (not available yet).
    pub mm: Spanned<Expr>,
    /// The place it is round.
    pub of: Spanned<PlaceRef>,
}

/// `let` adds a constant; `override` replaces a library one for this tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstKind {
    /// `let NAME VALUE`.
    Let,
    /// `override NAME VALUE`.
    Override,
}

/// A named height, length or number: `let groundHeight 5`.
#[derive(Debug, Clone)]
pub struct Const {
    /// `let` or `override`.
    pub kind: ConstKind,
    /// Its name.
    pub name: Spanned<String>,
    /// Its value, an expression of earlier names.
    pub value: Spanned<Expr>,
    /// What the value measures.
    pub dim: Dim,
    /// The whole node.
    pub span: SourceSpan,
}

/// `point knoll "(0.28, 0.72)"`.
#[derive(Debug, Clone)]
pub struct PointDecl {
    /// Its name.
    pub name: Spanned<String>,
    /// Across, squares.
    pub x: f64,
    /// Up, squares.
    pub y: f64,
    /// The whole node.
    pub span: SourceSpan,
}

/// The stack's `base` line: the connector, which must come first.
#[derive(Debug, Clone, Copy)]
pub struct Base {
    /// How many entries were written before it.
    pub index: usize,
    /// The node.
    pub span: SourceSpan,
}

/// `pin [NAME] top=BAND at=PLACE`: a body's band held to a height at a place.
#[derive(Debug, Clone)]
pub struct Pin {
    /// Its optional name.
    pub name: Option<Spanned<String>>,
    /// The band, a height or a range.
    pub top: Spanned<Expr>,
    /// Where it holds.
    pub at: Spanned<PlaceRef>,
    /// The whole node.
    pub span: SourceSpan,
}

/// What a stack entry applies.
#[derive(Debug, Clone)]
pub enum Callee {
    /// A built-in element (including `scatter`).
    Element(&'static Element),
    /// A definition, from this file or a `use`.
    Definition(String),
}

/// The place words written on an entry.
#[derive(Debug, Clone, Default)]
pub struct Place {
    /// `at=` a region; `None` is the caller's area (the whole tile).
    pub region: Option<Spanned<String>>,
    /// `at=` a line: an edge, an edge path or a line.
    pub along: Option<Spanned<Along>>,
    /// `side=` of a drawn line; `None` is centre.
    pub side: Option<Spanned<Side>>,
    /// `at=`: a point, possibly a range drawn once; `None` is anywhere.
    pub at: Option<Spanned<Point>>,
    /// `at=foot(…)`: a kind of thing.
    pub against: Option<Spanned<String>>,
    /// `seed=`: pins this thing's element while the tile seed changes the rest.
    pub seed: Option<Spanned<i64>>,
    /// `id=`: a stable name among same-named siblings, for seed paths.
    pub id: Option<Spanned<String>>,
    /// `weight=`: a scatter child's share of the mix; `None` is 1.
    pub weight: Option<Spanned<f64>>,
}

/// One stack entry.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The name as written.
    pub what: Spanned<String>,
    /// What it resolved to.
    pub callee: Callee,
    /// Solid or surface, for planning solids first.
    pub kind: Kind,
    /// Place words.
    pub place: Place,
    /// Controls (an element's) or arguments (a definition's), in written
    /// order, from both the line and child lines.
    pub controls: Vec<Setting>,
    /// A scatter's things, or the scatters inside a surface.
    pub children: Vec<Entry>,
    /// `top=`: a body's band, or a thing's height limit.
    pub top: Option<Spanned<Expr>>,
    /// `at=` as the heights language writes it (a name or `"(x, y)"`).
    /// An area-filler's region is also in `place.region`, and a point in
    /// `place.at`, so today's build reads them.
    pub at_place: Option<Spanned<PlaceRef>>,
    /// `pin` lines in a body's block.
    pub pins: Vec<Pin>,
    /// `slope DEG` in a body's block: how steeply it lies.
    pub slope: Option<Spanned<f64>>,
    /// The optional name written after the element (`dirt hillDirt`).
    pub label: Option<Spanned<String>>,
    /// The path that addresses this node in [`crate::edit`], such as
    /// `["tile", "stack", "scatter#2", "boulder"]`.
    pub path: Vec<String>,
    /// The whole node.
    pub span: SourceSpan,
}

impl Entry {
    /// True when the entry uses the heights language: a top, pins, a slope,
    /// a name, or `at=` by name.
    #[must_use]
    pub fn uses_heights(&self) -> bool {
        self.top.is_some()
            || !self.pins.is_empty()
            || self.slope.is_some()
            || self.at_place.is_some()
            || self.label.is_some()
    }

    /// The built-in element, if this calls one.
    #[must_use]
    pub fn element(&self) -> Option<&'static Element> {
        match self.callee {
            Callee::Element(l) => Some(l),
            Callee::Definition(_) => None,
        }
    }

    /// True for a `scatter`.
    #[must_use]
    pub fn is_scatter(&self) -> bool {
        self.element()
            .is_some_and(|l| l.role == schema::Role::Scatter)
    }

    /// A control by name.
    #[must_use]
    pub fn control(&self, name: &str) -> Option<&Setting> {
        self.controls.iter().find(|s| s.name.value == name)
    }

    /// A control's value by name.
    #[must_use]
    pub fn value(&self, name: &str) -> Option<&Value> {
        self.control(name)?.value.as_ref().map(|v| &v.value)
    }

    /// A sub-control's value: `sub("leaf", "length")`.
    #[must_use]
    pub fn sub(&self, control: &str, name: &str) -> Option<&Value> {
        self.control(control)?.sub(name)
    }
}

/// How a control was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// On its owner's line: `cliff height=35`.
    Property,
    /// On its own line: `height 35` inside the block.
    Child,
}

/// One control and what it was set to.
#[derive(Debug, Clone)]
pub struct Setting {
    /// The control's name.
    pub name: Spanned<String>,
    /// The control's schema entry; `None` for a definition argument whose
    /// parameter fills no known control.
    pub control: Option<&'static Control>,
    /// Its value, if it has one (a group has none).
    pub value: Option<Spanned<Value>>,
    /// Sub-controls written as properties on its line.
    pub subs: Vec<Setting>,
    /// Line or child line.
    pub form: Form,
    /// The whole text: `name=value`, or the child node.
    pub span: SourceSpan,
}

impl Setting {
    /// A sub-control's value.
    #[must_use]
    pub fn sub(&self, name: &str) -> Option<&Value> {
        self.subs
            .iter()
            .find(|s| s.name.value == name)?
            .value
            .as_ref()
            .map(|v| &v.value)
    }
}

/// A number as written: an expression, with the `(mm)` or `(h)`
/// annotation if one was written.
#[derive(Debug, Clone, PartialEq)]
pub struct Num {
    /// The expression; a percent is divided by 100 when sampled.
    pub expr: Expr,
    /// `(mm)` (a check) or `(h)` (wall tops).
    pub unit: Option<Unit>,
}

impl Num {
    /// The value at the DungeonBlocks reference for the uniform draw `u`:
    /// `(h)` values are multiplied by [`schema::H_MM`].  Names resolve
    /// through `env` (levels in mm, parameters).
    ///
    /// # Errors
    /// The first name `env` does not know.
    pub fn sample(&self, env: &dyn Fn(&str) -> Option<f64>, u: f64) -> Result<f64, String> {
        let v = self.expr.sample(env, u)?;
        Ok(if self.unit == Some(Unit::H) {
            v * schema::H_MM
        } else {
            v
        })
    }

    /// One number in mm at the reference, its names resolved as the tile's
    /// `let`s (`lets`, in mm).
    ///
    /// # Errors
    /// A range (one number is needed), or a name `lets` lacks.
    pub fn mm(&self, lets: &[(String, f64)]) -> Result<f64, String> {
        if self.expr.is_range() {
            return Err(format!(
                "`{}` is a range; one number is needed here",
                self.expr
            ));
        }
        let env = |name: &str| lets.iter().rev().find(|(k, _)| k == name).map(|(_, v)| *v);
        self.sample(&env, 0.0)
    }

    /// Low and high ends at the reference.
    ///
    /// # Errors
    /// As [`Num::sample`].
    pub fn bounds(&self, env: &dyn Fn(&str) -> Option<f64>) -> Result<(f64, f64), String> {
        Ok((self.sample(env, 0.0)?, self.sample(env, 1.0)?))
    }
}

/// A decoded value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A number, range, percent or level expression.
    Num(Num),
    /// A word, with the numbers (or level) it stands for.
    Word(&'static Word),
    /// `#true` / `#false`.
    Bool(bool),
    /// A kind tag (`foot(cliff)`).
    Tag(String),
    /// The enclosing definition's parameter, filled in by the caller.
    Param(String),
    /// Free text, or a definition argument nothing could type.
    Text(String),
}

impl Value {
    /// The number, if this is one.
    #[must_use]
    pub fn num(&self) -> Option<&Num> {
        match self {
            Value::Num(n) => Some(n),
            _ => None,
        }
    }

    /// The word, if this is one.
    #[must_use]
    pub fn word(&self) -> Option<&'static Word> {
        match self {
            Value::Word(w) => Some(w),
            _ => None,
        }
    }
}

/// A printer profile: the hidden printability floors.
///
/// Every size an author writes scales with the system; these never do.
/// Elements clamp their hidden thicknesses to them after scaling: blade and
/// leaf thickness to whole layers and lines, the least branch radius to a
/// line, gaps to `gap`, faces to the `overhang` limit.
#[derive(Debug, Clone)]
pub struct Printer {
    /// Its name.
    pub name: Spanned<String>,
    /// One extruded line, mm.
    pub line: Spanned<f64>,
    /// One layer, mm.
    pub layer: Spanned<f64>,
    /// The overhang every element aims for, degrees from vertical.
    pub overhang: Spanned<f64>,
    /// The steepest overhang allowed in small patches, degrees from
    /// vertical: past it a layer no longer rests on the one below.
    pub steepest: Spanned<f64>,
    /// The narrowest gap that stays open, mm.
    pub gap: Spanned<f64>,
}
