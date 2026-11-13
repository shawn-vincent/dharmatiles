//! dt-elements: the built-in elements.
//!
//! Each element is one module written only in dt-kit's public operations. A
//! element says what it reads and writes in a plan ([`Element::plan`]), so the
//! scheduler in dt-eval can order the stack and refuse reads nothing can
//! satisfy; then it builds into the scene ([`Element::build`]).
//!
//! Layers of the terrain: [`dirt`], [`water`]. Pieces bodies: [`floor`] (`flagstones`, `planks`). Solids: [`rock`]
//! and its kin, [`grass`], [`reeds`], [`flowers`] (the `flower` thing), [`cliff`], [`wall`],
//! [`tree`]. Bases: [`connector`] (DungeonBlocks) and [`openlock`].

pub mod base;
pub mod cliff;
pub mod connector;
pub mod dirt;
pub mod floor;
pub mod flowers;
pub mod grass;
pub mod mixed;
pub mod openlock;
mod read;
pub mod reeds;
pub mod rock;
pub mod tree;
pub mod wall;
pub mod water;

pub use dt_kit::Kind;
pub use dt_kit::{Dist, Draw, Op, Weighting};
use dt_kit::{Mask, Scene, Seed};

/// A control's value, as the document gave it (levels already resolved
/// to mm by the evaluator).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A number (mm, degrees, …: each control knows its unit).
    Number(f64),
    /// A word: a choice.
    Word(String),
    /// A range `lo..hi`, drawn by the element (once per tile, or per thing).
    Range(f64, f64),
    /// A weighted range, `low(1..6, 4)`: the range and the odds along it.
    /// Only controls that draw take one ([`read::Settings::dist`]); the rest
    /// refuse it (`dt::weighting`).
    Dist(Dist),
    /// Several ranges in one expression (`[1..2] + [3..4]`): each rolls on
    /// its own, then the arithmetic.  Controls that draw take it like a
    /// weighted range; a control that reads a range takes its ends.
    Draw(Draw),
    /// A tree's `top=`: a formula the tile worked out where the tree stands
    /// (see [`CrownTops`]).
    Formula(Formula),
}

/// A formula the build evaluates at the places an element asks: the entry
/// whose `top=` it is, and its text with every name expanded (what the
/// caches and equality read).
#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    /// The stack entry whose formula it is.
    pub owner: dt_kit::EntryId,
    /// The thing of that entry a scatter lists, when it is that thing's.
    pub child: Option<usize>,
    /// The formula with the names it reads expanded.
    pub key: String,
}

/// Where a tree's tip stands, for the formula its `top=` is: the tile's
/// squares from the south-west corner, and the tree's own disc.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tip {
    /// The tip's place, squares.
    pub at: [f64; 2],
    /// The trunk's place, squares.
    pub centre: [f64; 2],
    /// The crown's radius, squares: the tree's region is this disc.
    pub radius: f64,
    /// The ground at the tree's foot, mm at the 35 mm square (`$bottom`).
    pub bottom_dmm: f64,
}

/// A cloud at one tip: its low and high surfaces (mm at the 35 mm square)
/// and its weighting (`None`: every height between equally).
#[derive(Clone, Debug, PartialEq)]
pub struct TopCloud {
    /// The low surface.
    pub lo: f64,
    /// The high surface.
    pub hi: f64,
    /// How often each height between comes up.
    pub odds: Option<Weighting>,
}

/// What works a tree's `top=` formula out at a tip.
pub trait CrownTops {
    /// The cloud `formula` makes at `tip`.
    ///
    /// # Errors
    /// [`LookError`] naming what the formula cannot do there.
    fn cloud(&self, formula: &Formula, tip: &Tip) -> Result<TopCloud, LookError>;
}

/// A number or a range `lo..hi` as an element reads it: one number is the
/// range with both ends equal.  Whether a draw happens at all is the
/// element's (a range is drawn per thing, a number never draws).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    /// The low end.
    pub lo: f64,
    /// The high end.
    pub hi: f64,
}

impl Span {
    /// `lo..hi`.
    #[must_use]
    pub const fn new(lo: f64, hi: f64) -> Self {
        Self { lo, hi }
    }

    /// One number, as a range of no width.
    #[must_use]
    pub const fn point(v: f64) -> Self {
        Self { lo: v, hi: v }
    }

    /// A uniform draw over the range (one draw from `rng`, whatever the
    /// width).
    pub fn draw(self, rng: &mut dt_kit::Rng) -> f64 {
        rng.uniform(self.lo, self.hi)
    }

    /// True for one number (a range of no width): a number never draws.
    #[must_use]
    pub fn is_point(self) -> bool {
        self.hi <= self.lo
    }

    /// As a pair, for the kernels that take `(lo, hi)`.
    #[must_use]
    pub const fn pair(self) -> (f64, f64) {
        (self.lo, self.hi)
    }

    /// Half way.
    #[must_use]
    pub fn mid(self) -> f64 {
        0.5 * (self.lo + self.hi)
    }

    /// Both ends scaled (a size in mm follows the system).
    #[must_use]
    pub fn scaled(self, k: f64) -> Self {
        Self::new(self.lo * k, self.hi * k)
    }
}

/// An angle in degrees, as written; never scales.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Degrees(pub f64);

impl Degrees {
    /// In radians.
    #[must_use]
    pub fn radians(self) -> f64 {
        self.0.to_radians()
    }
}

/// As an author would write it, for messages: `-50`, `∞`, `NaN`, `` `wet` ``.
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(n) if n.is_nan() => f.write_str("NaN"),
            Self::Number(n) if n.is_infinite() => {
                f.write_str(if *n > 0.0 { "∞" } else { "-∞" })
            }
            Self::Number(n) => write!(f, "{n}"),
            Self::Range(a, b) => write!(f, "\"{a}..{b}\""),
            Self::Dist(d) => write!(f, "\"{}\"", d.written()),
            Self::Draw(d) => {
                let (lo, hi) = d.bounds();
                write!(f, "\"{lo}..{hi}\" (sum of {} ranges)", d.ranges())
            }
            Self::Word(w) => write!(f, "`{w}`"),
            Self::Formula(x) => write!(f, "`{}`", x.key),
        }
    }
}

/// One control set on a stack entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    /// The control's name.
    pub name: String,
    /// Its value.
    pub value: Value,
}

/// The code of a build-time refusal the tile can fix (a control's value or the
/// entry's place cannot be built), as opposed to `dt::build`, a failure inside
/// the program.
pub const LOOK: &str = "dt::look";

/// An element refused its controls or failed to build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookError {
    /// The control at fault, if one is.
    pub control: Option<String>,
    /// What is wrong.
    pub message: String,
    /// What to do about it.
    pub help: Option<String>,
    /// The diagnostic code, when it is not a plain build failure
    /// (`dt::band`).
    pub code: Option<&'static str>,
}

impl LookError {
    /// A pieces body's piece outside its band (`dt::band`: a bug in the
    /// body, refused).
    fn band(r: &dt_kit::Refusal, what: &str) -> Self {
        Self {
            control: None,
            message: format!("{what}: {r}"),
            help: Some(
                "this is a bug in the element: a piece is made to fit its band and never trimmed; where the band jumps across a piece (a pin's edge inside a slab) move the pin to a slab's edge"
                    .into(),
            ),
            code: Some(r.code()),
        }
    }

    /// A control's value the element cannot build (`dt::look`: the tile's to
    /// change, shown as a mistake at the entry, not as a bug).
    fn control(name: &str, message: String, help: String) -> Self {
        Self {
            control: Some(name.to_owned()),
            message,
            help: Some(help),
            code: Some(LOOK),
        }
    }

    /// Not one control's fault: what is wrong and what to do.
    fn general(message: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            control: None,
            message: message.into(),
            help: Some(help.into()),
            code: Some(LOOK),
        }
    }

    fn build(e: impl std::fmt::Display) -> Self {
        Self {
            control: None,
            message: e.to_string(),
            help: None,
            code: None,
        }
    }
}

/// What a stack entry writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Write {
    /// The ground surface (painted in stack order, then lifted once).
    Surface,
    /// A solid of this kind.
    Solid(Kind),
    /// Solids of each of these kinds (a scatter that mixes things).
    Solids(&'static [Kind]),
    /// A solid of this kind laid with the land it stands on: listed after
    /// the surfaces it lies on, and before the lift (a floor), so the lift
    /// sees it and it reads no lifted ground.
    Laid(Kind),
    /// The surface again, filled over the ground listed above it (water):
    /// before the lift, which lifts it with the ground.
    Fill,
}

/// What a stack entry reads before it can build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// Every solid of this kind (`rock`, `tree`, …) listed above it: the
    /// terrain is the surfaces painted above it, as laid so far. The plan
    /// refuses a read when nothing writes the kind, or when the first that
    /// does is listed below it.
    Solids(Kind),
    /// Every other solid in the tile, the lifted ground included: built
    /// last, whatever its place in the stack.
    AllSolids,
}

/// An element's plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// What it writes.
    pub writes: Write,
    /// What it reads.
    pub reads: Vec<Read>,
    /// The surface cell it needs, true mm (`None`: the default 128 per
    /// square). The tile is built at the finest any entry asks for.
    pub cell_mm: Option<CellMm>,
}

/// A cell size in mm, comparable (never NaN).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellMm(u32);

impl CellMm {
    /// From mm, in whole micrometres.
    #[must_use]
    pub fn mm(v: f64) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let um = (v * 1000.0).round().clamp(1.0, 1e6) as u32;
        Self(um)
    }

    /// In mm.
    #[must_use]
    pub fn get(self) -> f64 {
        f64::from(self.0) / 1000.0
    }
}

/// Where and as what an element is built.
pub struct Ctx<'a> {
    /// Its area (the whole tile unless `at=` names a region).
    pub area: &'a Mask,
    /// Who it is, the owner of what it makes.
    pub path: &'a dt_kit::EntryId,
    /// Its seed, keyed by its path.
    pub seed: Seed,
    /// `at=` a line: the path it follows, in squares from the south-west
    /// corner, walked with the tile inside on the left (a run's outer
    /// face). `None` without a line.
    pub along: Option<&'a [[f64; 2]]>,
    /// Which side of the drawn line `along` is a thing stands on (`side=`,
    /// centre if not written); `Some` only when `along` is a drawn line,
    /// `None` for tile edges.
    pub line_side: Option<LineSide>,
    /// `at=`: where it stands, in squares, the x and the y each a number
    /// (a range of no width) or a range with its odds, drawn from the
    /// entry's seed. `None` without `at=`.
    pub at: Option<&'a [Draw; 2]>,
    /// The printer the build is checked against: elements that cut or cull
    /// to the gate's rule (a tree's [`dt_kit::stand`], its leaf cull) use
    /// its numbers.
    pub printer: dt_kit::Printer,
    /// `at=foot(…)`: the kind of thing this element gathers against (a scatter),
    /// if written.
    pub against: Option<&'a str>,
    /// The planned feet of every solid that has one (a cliff's), from the
    /// plan pass that runs before anything is built, so a bank or scatter
    /// listed before the cliff can still lean on it.
    pub foots: &'a [Foot],
    /// The band the solver gave the body this entry is, on the build's
    /// lattice (a dirt's or a water's top, pins applied); `None` for
    /// everything else.
    pub band: Option<&'a dt_kit::Band>,
    /// The surface laid before this body (what `bottom` meant when it was
    /// solved), on the build's lattice; `None` for everything else.
    pub below: Option<&'a dt_kit::Band>,
    /// The lowest the land under this body reaches where it lies (world mm,
    /// the surface laid before it): a cliff's stones are counted from it.
    pub land_mm: f64,
    /// For a dirt: about the height the water laid over it stands at, at
    /// every node (negative infinity where none does), so its grain keeps
    /// off the bed. `None` for anything else.
    pub under: Option<&'a dt_kit::Field>,
    /// The calm water level, mm (the library's `waterSurfaceHeight`, scaled)
    /// when the tile has a water; what waves are drawn about and reeds
    /// stand in.  `None` without one.
    pub calm_mm: Option<f64>,
    /// A draft for the editor's view while a value is dragged: an element
    /// whose cost is in fixed-millimetre detail (a cliff's carving) may
    /// make it coarser.  Never printed.
    pub draft: bool,
    /// Works a tree's `top=` formula out at each branch tip; `None` where
    /// the build has no tile (the element's own tests).
    pub crowns: Option<&'a dyn CrownTops>,
}

/// Which side of a drawn line a thing stands on, as you walk it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineSide {
    /// To the left of the direction the line is drawn.
    Left,
    /// To the right of it.
    Right,
    /// Centred on the line.
    Centre,
}

/// How far either side of a line a scatter spreads when it writes no
/// `reach`, mm at the 35 mm square.
pub const LINE_REACH_MM: f64 = 5.0;

impl Ctx<'_> {
    /// Where a scatter spreads: its area, narrowed to the band within
    /// `reach_mm` (mm at the 35 mm square) either side of the line it is
    /// `at=`, when it is.  The band runs to the ends of the line, rounded.
    #[must_use]
    pub fn spread_area(&self, frame: &dt_kit::Frame, reach_mm: f64) -> Mask {
        let mut area = self.area.clone();
        let Some(path) = self.along else {
            return area;
        };
        let reach = reach_mm * frame.scale();
        let sq = frame.square_mm;
        let step = frame.step_mm();
        let mm: Vec<[f64; 2]> = path.iter().map(|p| [p[0] * sq, p[1] * sq]).collect();
        for (k, bit) in area.bits.iter_mut().enumerate().filter(|(_, b)| **b) {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (
                (k % self.area.nx) as f64 * step,
                (k / self.area.nx) as f64 * step,
            );
            *bit = distance_to_path(&mm, x, y) <= reach;
        }
        area
    }

    /// Squares' worth of what a scatter spreads over: the area's squares,
    /// or the line's length in squares.
    #[must_use]
    pub fn spread_squares(&self, frame: &dt_kit::Frame, area: &Mask) -> f64 {
        match self.along {
            Some(path) => path
                .windows(2)
                .map(|w| libm::hypot(w[1][0] - w[0][0], w[1][1] - w[0][1]))
                .sum(),
            #[allow(clippy::cast_precision_loss)]
            None => {
                area.count() as f64
                    / (frame.cells_per_square as f64 * frame.cells_per_square as f64)
            }
        }
    }

    /// The planned foot of the solids of `kind` (a word as written), if
    /// the tile has one.
    #[must_use]
    pub fn foot(&self, kind: &str) -> Option<&Foot> {
        self.foots.iter().find(|f| f.kind.name() == kind)
    }
}

/// Where a planned solid meets the ground: what a scatter gathers at (plan,
/// then build).
#[derive(Clone, Debug)]
pub struct Foot {
    /// What it is.
    pub kind: Kind,
    /// Per ground node, row-major: distance from the solid in mm (infinite
    /// where there is no solid).
    pub dist: Vec<f64>,
    /// Ground cells touching the solid: position, outward direction, weight.
    pub cells: Vec<([f64; 2], [f64; 2], f64)>,
    /// Squares' worth of foot length.
    pub squares: f64,
    /// Nodes across, for indexing `dist`.
    pub nx: usize,
}

/// A child of a scatter: the thing it scatters, its controls, and its
/// share of the mix (`weight=`, default 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Child {
    /// The thing's element.
    pub name: String,
    /// Its controls.
    pub controls: Vec<Control>,
    /// Its weight.
    pub weight: f64,
}

/// A built-in element.
pub trait Element: Send + Sync {
    /// What it reads and writes.
    fn plan(&self) -> Plan;

    /// The controls it works out for itself where nothing is written (a
    /// wall's stone size from its style): each as the value this element
    /// is built with, read off the same fields the build reads, so the
    /// editor shows what a build uses and there is no second table of it.
    /// The schema's own defaults and its `absent` words are not listed
    /// here.
    fn shown(&self) -> Vec<Shown> {
        Vec::new()
    }

    /// Build into the scene.
    ///
    /// # Errors
    /// [`LookError`] when the build fails.
    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError>;

    /// Its planned foot, for the plan pass (a cliff plans its footprint
    /// without building any geometry).
    ///
    /// # Errors
    /// [`LookError`] when the plan fails.
    fn foot(&self, _frame: &dt_kit::Frame, _ctx: &Ctx<'_>) -> Result<Option<Foot>, LookError> {
        Ok(None)
    }
}

/// A control's value as its element has it, with where it comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    /// The control's path: `joints/width`.
    pub control: String,
    /// The value as a document writes it: `0.11dmm`, `running`, `30%`.
    pub text: String,
    /// What it comes from, after "from": `style brick`.
    pub from: String,
}

impl Shown {
    /// `control` has value `text`, from `from`.
    pub(crate) fn new(control: &str, text: impl Into<String>, from: impl Into<String>) -> Self {
        Self {
            control: control.to_owned(),
            text: text.into(),
            from: from.into(),
        }
    }
}

/// A number as a document writes it: at most four decimals, no trailing zeros.
pub(crate) fn num_text(v: f64) -> String {
    let t = format!("{v:.4}");
    t.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A draw as a document writes it, each end (times `scale`: 100 for a share
/// held as a fraction) followed by `unit`: `5.5dmm .. 8.5dmm`, or one
/// number where the ends are the same.
pub(crate) fn draw_text(d: &Draw, unit: &str, scale: f64) -> String {
    let (lo, hi) = (num_text(d.lo() * scale), num_text(d.hi() * scale));
    if lo == hi {
        format!("{lo}{unit}")
    } else {
        format!("{lo}{unit} .. {hi}{unit}")
    }
}

/// What `name` works out for itself where nothing is written (see
/// [`Element::shown`]), built from the controls and children it has.
///
/// # Errors
/// [`LookError`] for an unknown element, or one its controls or children
/// cannot build.
pub fn shown(
    name: &str,
    controls: &[Control],
    children: &[Child],
) -> Result<Vec<Shown>, LookError> {
    Ok(element_with(name, controls, children)?.shown())
}

/// Builds an element from the name as written (a rock row serves four
/// names), its controls and its children.  What each takes, where it may
/// be placed and what a scatter may hold are the schema's
/// (`dt_doc::schema`); the evaluator passes only those.
type Builder = fn(&str, &[Control], &[Child]) -> Result<Box<dyn Element>, LookError>;

/// Every built element: its name and how it is built.  An element not
/// listed is not built yet.
static ELEMENTS: &[(&str, Builder)] = &[
    ("dirt", |_, c, _| Ok(Box::new(dirt::Dirt::new(c)?))),
    ("water", |_, c, _| Ok(Box::new(water::Water::new(c)?))),
    ("scatter", |_, c, ch| {
        // one kind of thing builds as its own scatter; two or more are mixed
        let kinds = mixed::kinds(ch);
        if kinds >= 2 {
            Ok(Box::new(mixed::Scatter::new(c, ch)?))
        } else if !ch.is_empty() && ch.iter().all(|k| k.name == "flower") {
            Ok(Box::new(flowers::Scatter::new(c, ch)?))
        } else if !ch.is_empty() && ch.iter().all(|k| k.name == "tree") {
            Ok(Box::new(tree::Scatter::new(c, ch)?))
        } else {
            Ok(Box::new(rock::Scatter::new(c, ch)?))
        }
    }),
    ("rock", |n, c, _| Ok(Box::new(rock::Single::new(n, c)?))),
    ("stone", |n, c, _| Ok(Box::new(rock::Single::new(n, c)?))),
    ("boulder", |n, c, _| Ok(Box::new(rock::Single::new(n, c)?))),
    ("pebble", |n, c, _| Ok(Box::new(rock::Single::new(n, c)?))),
    ("wall", |_, c, _| Ok(Box::new(wall::Wall::new(c)?))),
    ("flagstones", |_, c, _| {
        Ok(Box::new(floor::Flagstones::new(c)?))
    }),
    ("planks", |_, c, _| Ok(Box::new(floor::Planks::new(c)?))),
    ("tree", |_, c, _| Ok(Box::new(tree::Tree::new(c)?))),
    ("grass", |_, c, _| Ok(Box::new(grass::Grass::new(c)?))),
    ("reeds", |_, c, _| Ok(Box::new(reeds::Reeds::new(c)?))),
    ("cliff", |_, c, _| Ok(Box::new(cliff::Cliff::new(c)?))),
    ("flower", |_, c, _| Ok(Box::new(flowers::Flower::new(c)?))),
];

/// The element `name` with its controls and children (a scatter's things).
///
/// # Errors
/// [`LookError`] for an unknown element, or one its controls or children
/// cannot build.
pub fn element_with(
    name: &str,
    controls: &[Control],
    children: &[Child],
) -> Result<Box<dyn Element>, LookError> {
    match ELEMENTS.iter().find(|(n, _)| *n == name) {
        Some((_, build)) => build(name, controls, children),
        None => Err(LookError {
            control: None,
            message: format!("unknown element `{name}`"),
            help: None,
            code: None,
        }),
    }
}

/// The names of every built element, in table order.
pub fn built() -> impl Iterator<Item = &'static str> {
    ELEMENTS.iter().map(|(n, _)| *n)
}

/// What a cut into the base of `system` may not reach ([`base::KeepOut`]),
/// keeping `wall_mm` from the connector's sides; `content_top_mm` picks the
/// DungeonBlocks peg. Data for the `dt::connector` check, which has no
/// caller until cuts exist.
///
/// # Errors
/// [`LookError`] for an unknown system.
pub fn keep_out(
    system: &str,
    frame: &dt_kit::Frame,
    content_top_mm: f64,
    wall_mm: f64,
) -> Result<Vec<base::KeepOut>, LookError> {
    match system {
        "db" => Ok(connector::DungeonBlocks::default().keep_out(frame, content_top_mm, wall_mm)),
        "ol" => Ok(openlock::OpenLock::default().keep_out(frame, wall_mm)),
        _ => Err(LookError::general(
            format!("no connector for system `{system}`"),
            "the systems are db (DungeonBlocks) and ol (OpenLOCK)",
        )),
    }
}

/// The connector for a system: `db` (DungeonBlocks) or `ol` (OpenLOCK).
///
/// # Errors
/// [`LookError`] for an unknown system.
pub fn connector(system: &str) -> Result<Box<dyn Element>, LookError> {
    match system {
        "db" => Ok(Box::new(connector::DungeonBlocks::default())),
        "ol" => Ok(Box::new(openlock::OpenLock::default())),
        _ => Err(LookError::general(
            format!("no connector for system `{system}`"),
            "the systems are db (DungeonBlocks) and ol (OpenLOCK)",
        )),
    }
}

/// The distance from `(x, y)` to the nearest point of a polyline, mm.
fn distance_to_path(path: &[[f64; 2]], x: f64, y: f64) -> f64 {
    if let [only] = path {
        return libm::hypot(x - only[0], y - only[1]);
    }
    path.windows(2)
        .map(|w| {
            let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let len2 = dx * dx + dy * dy;
            let t = if len2 > 0.0 {
                (((x - w[0][0]) * dx + (y - w[0][1]) * dy) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            libm::hypot(x - (w[0][0] + t * dx), y - (w[0][1] + t * dy))
        })
        .fold(f64::INFINITY, f64::min)
}
