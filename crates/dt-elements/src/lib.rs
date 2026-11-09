//! dt-elements: the built-in elements.
//!
//! Each element is one module written only in dt-kit's public operations. A
//! element says what it reads and writes in a plan ([`Element::plan`]), so the
//! scheduler in dt-eval can order the stack and refuse reads nothing can
//! satisfy; then it builds into the scene ([`Element::build`]).
//!
//! Layers of the terrain: [`dirt`], [`water`]. Solids: [`floor`], [`rock`]
//! and its kin, [`grass`], [`reeds`], [`flowers`], [`cliff`], [`wall`],
//! [`tree`]. Bases: [`connector`] (DungeonBlocks) and [`openlock`].

pub mod base;
pub mod cliff;
pub mod connector;
pub mod dirt;
pub mod floor;
pub mod flowers;
pub mod grass;
pub mod openlock;
mod read;
pub mod reeds;
pub mod rock;
pub mod tree;
pub mod wall;
pub mod water;

pub use dt_kit::Kind;
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
            Self::Word(w) => write!(f, "`{w}`"),
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

/// An element refused its controls or failed to build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookError {
    /// The control at fault, if one is.
    pub control: Option<String>,
    /// What is wrong.
    pub message: String,
    /// What to do about it.
    pub help: Option<String>,
}

impl LookError {
    fn control(name: &str, message: String, help: String) -> Self {
        Self {
            control: Some(name.to_owned()),
            message,
            help: Some(help),
        }
    }

    /// Not one control's fault: what is wrong and what to do.
    fn general(message: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            control: None,
            message: message.into(),
            help: Some(help.into()),
        }
    }

    fn build(e: impl std::fmt::Display) -> Self {
        Self {
            control: None,
            message: e.to_string(),
            help: None,
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
    /// The surface again, filled over what it reads (water): after the
    /// solids it reads and before the lift, which lifts it with the ground.
    Fill,
}

/// What a stack entry reads before it can build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// Every solid of this kind (`rock`, `tree`, …).
    Solids(Kind),
    /// Every other solid in the tile, the lifted ground included.
    AllSolids,
    /// Every solid of this kind if the tile has any (water's rocks, what a
    /// thing may stand on such as a wall on a floor); nothing to wait for
    /// if not.
    IfAny(Kind),
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
    /// Its area (the whole tile unless `in=` names a region).
    pub area: &'a Mask,
    /// Its stack path, the owner of what it makes.
    pub path: &'a str,
    /// Its seed, keyed by its path.
    pub seed: Seed,
    /// `along=`: the path it follows, in squares from the south-west
    /// corner, walked with the tile inside on the left (a run's outer
    /// face). `None` without `along=`.
    pub along: Option<&'a [[f64; 2]]>,
    /// `at=`: where it stands, in squares, as `[x_lo, x_hi]` and
    /// `[y_lo, y_hi]` (equal ends for a fixed point; a range is drawn
    /// from the entry's seed). `None` without `at=`.
    pub at: Option<[[f64; 2]; 2]>,
    /// The printer the build is checked against: elements that cut or cull
    /// to the gate's rule (a tree's [`dt_kit::stand`], its leaf cull) use
    /// its numbers.
    pub printer: dt_kit::Printer,
    /// `against=`: the kind of thing this element gathers against (a scatter),
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
    /// For a dirt: about the height the water laid over it stands at, at
    /// every node (negative infinity where none does), so its grain keeps
    /// off the bed. `None` for anything else.
    pub under: Option<&'a dt_kit::Field>,
    /// Where the first land body meets a tile edge, mm: its solved band
    /// where the library's edge pin holds the rim.  `None` when none does.
    /// What a laid floor is cut at.
    pub edge_mm: Option<f64>,
    /// The calm water level, mm (the library's `waterSurfaceHeight`, scaled)
    /// when the tile has a water; what waves are drawn about and reeds
    /// stand in.  `None` without one.
    pub calm_mm: Option<f64>,
    /// A draft for the editor's view while a value is dragged: an element
    /// whose cost is in fixed-millimetre detail (a cliff's carving) may
    /// make it coarser.  Never printed.
    pub draft: bool,
}

impl Ctx<'_> {
    /// The planned foot of the solids of `kind` (a word as written), if
    /// the tile has one.
    #[must_use]
    pub fn foot(&self, kind: &str) -> Option<&Foot> {
        self.foots.iter().find(|f| f.kind.name() == kind)
    }
}

/// Where a planned solid meets the ground: what a bank heaps against and
/// a scatter gathers at (plan, then build).
#[derive(Clone, Debug)]
pub struct Foot {
    /// What it is.
    pub kind: Kind,
    /// Per ground node, row-major: distance from the solid in mm (huge
    /// where it is nowhere near).
    pub dist: Vec<f64>,
    /// Per ground node: how shattered the cliff is there, 0 (quiet) to 1.
    pub zone: Vec<f64>,
    /// Per ground node: 0 at a tile end of the run, rising to 1 clear of it
    /// (every seed's bank is the same at the ends).
    pub end: Vec<f64>,
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
        Ok(Box::new(rock::Scatter::new(c, ch)?))
    }),
    ("rock", |n, c, _| Ok(Box::new(rock::Single::new(n, c)?))),
    ("stone", |n, c, _| Ok(Box::new(rock::Single::new(n, c)?))),
    ("boulder", |n, c, _| Ok(Box::new(rock::Single::new(n, c)?))),
    ("pebble", |n, c, _| Ok(Box::new(rock::Single::new(n, c)?))),
    ("wall", |_, c, _| Ok(Box::new(wall::Wall::new(c)?))),
    ("floor", |_, c, _| Ok(Box::new(floor::Floor::new(c)?))),
    ("tree", |_, c, _| Ok(Box::new(tree::Tree::new(c)?))),
    ("grass", |_, c, _| Ok(Box::new(grass::Grass::new(c)?))),
    ("reeds", |_, c, _| Ok(Box::new(reeds::Reeds::new(c)?))),
    ("cliff", |_, c, _| Ok(Box::new(cliff::Cliff::new(c)?))),
    ("flowers", |_, c, _| Ok(Box::new(flowers::Flowers::new(c)?))),
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
