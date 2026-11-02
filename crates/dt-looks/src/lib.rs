//! dt-looks: the built-in looks.
//!
//! Each look is one module written only in dt-kit's public operations. A
//! look says what it reads and writes in a plan ([`Look::plan`]), so the
//! scheduler in dt-eval can order the stack and refuse reads nothing can
//! satisfy; then it builds into the scene ([`Look::build`]).
//!
//! P1: [`ground`] (then a flat surface at a level) and [`connector`]
//! (DungeonBlocks). P2: [`ground`] in full (levels, hills, the edge
//! contract) and [`dirt`] (dirtlab's textured earth on it). P5:
//! [`openlock`] (the OpenLOCK connector).

pub mod connector;
pub mod dirt;
pub mod floor;
pub mod ground;
pub mod openlock;
pub mod tree;
pub mod wall;

use dt_kit::{Mask, Scene, Seed};

/// A control's value, as the document gave it (levels already resolved
/// to mm by the evaluator).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A number (mm, degrees, …: each control knows its unit).
    Number(f64),
    /// A word: a choice.
    Word(String),
    /// A range `lo..hi`, drawn by the look (once per tile, or per thing).
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

/// A look refused its controls or failed to build.
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
    Solid(&'static str),
}

/// What a stack entry reads before it can build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    /// Every solid of this kind (`rock`, `tree`, …).
    Solids(&'static str),
    /// Every other solid in the tile, the lifted ground included.
    AllSolids,
    /// Every solid of this kind if the tile has any (what a thing may
    /// stand on, such as a wall on a floor); nothing to wait for if not.
    IfAny(&'static str),
}

/// A look's plan.
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

/// Where and as what a look is built.
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
    /// The printer the build is checked against: looks that cut or cull
    /// to the gate's rule (a tree's [`dt_kit::stand`], its leaf cull) use
    /// its numbers.
    pub printer: dt_kit::Printer,
}

/// A built-in look.
pub trait Look: Send + Sync {
    /// What it reads and writes.
    fn plan(&self) -> Plan;

    /// Build into the scene.
    ///
    /// # Errors
    /// [`LookError`] when the build fails.
    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError>;
}

/// The controls a built look takes so far, or `None` for a look not built
/// yet.  The evaluator passes a look only these (written, or their
/// defaults from dt-doc's schema).
#[must_use]
pub fn takes(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "ground" => Some(ground::CONTROLS),
        "dirt" => Some(dirt::CONTROLS),
        "wall" => Some(wall::CONTROLS),
        "floor" => Some(floor::CONTROLS),
        "tree" => Some(tree::CONTROLS),
        _ => None,
    }
}

/// The place words a built look honours (`along`, `at`); the evaluator
/// sets every other one aside as not built yet.
#[must_use]
pub fn places(name: &str) -> &'static [&'static str] {
    match name {
        "wall" => &["along"],
        "tree" => &["at"],
        _ => &[],
    }
}

/// The look `name` with its controls.
///
/// # Errors
/// [`LookError`] for an unknown look or a control it does not take.
pub fn look(name: &str, controls: &[Control]) -> Result<Box<dyn Look>, LookError> {
    match name {
        "ground" => Ok(Box::new(ground::Ground::new(controls)?)),
        "dirt" => Ok(Box::new(dirt::Dirt::new(controls)?)),
        "wall" => Ok(Box::new(wall::Wall::new(controls)?)),
        "floor" => Ok(Box::new(floor::Floor::new(controls)?)),
        "tree" => Ok(Box::new(tree::Tree::new(controls)?)),
        _ => Err(LookError {
            control: None,
            message: format!("unknown look `{name}`"),
            help: None,
        }),
    }
}

/// The connector for a system: `db` (DungeonBlocks) or `ol` (OpenLOCK).
///
/// # Errors
/// [`LookError`] for an unknown system.
pub fn connector(system: &str) -> Result<Box<dyn Look>, LookError> {
    match system {
        "db" => Ok(Box::new(connector::DungeonBlocks::default())),
        "ol" => Ok(Box::new(openlock::OpenLock::default())),
        _ => Err(LookError {
            control: None,
            message: format!("no connector for system `{system}`"),
            help: Some("the systems are db (DungeonBlocks) and ol (OpenLOCK)".to_owned()),
        }),
    }
}

/// Refuse any control not in `known` (a word's numbers, `clods.density`,
/// come with their control).
fn only(look: &str, controls: &[Control], known: &[&str]) -> Result<(), LookError> {
    for c in controls {
        let base = c.name.split('.').next().unwrap_or(&c.name);
        if !known.contains(&base) {
            return Err(LookError::control(
                &c.name,
                format!("`{look}` has no control `{}`", c.name),
                format!("{look} takes: {}", known.join(", ")),
            ));
        }
    }
    Ok(())
}
