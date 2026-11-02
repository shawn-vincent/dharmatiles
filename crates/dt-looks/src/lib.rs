//! dt-looks: the built-in looks.
//!
//! Each look is one module written only in dt-kit's public operations. A
//! look says what it reads and writes in a plan ([`Look::plan`]), so the
//! scheduler in dt-eval can order the stack and refuse reads nothing can
//! satisfy; then it builds into the scene ([`Look::build`]).
//!
//! P1: [`ground`] (a flat surface at a level) and [`connector`]
//! (DungeonBlocks).

pub mod connector;
pub mod ground;

use dt_kit::{Mask, Scene, Seed};

/// A control's value, as the document gave it (levels already resolved
/// to mm by the evaluator).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A number (mm, degrees, …: each control knows its unit).
    Number(f64),
    /// A word: a choice.
    Word(String),
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
}

/// A look's plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// What it writes.
    pub writes: Write,
    /// What it reads.
    pub reads: Vec<Read>,
}

/// Where and as what a look is built.
pub struct Ctx<'a> {
    /// Its area (the whole tile unless `in=` names a region).
    pub area: &'a Mask,
    /// Its stack path, the owner of what it makes.
    pub path: &'a str,
    /// Its seed, keyed by its path.
    pub seed: Seed,
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
        _ => None,
    }
}

/// The look `name` with its controls.
///
/// # Errors
/// [`LookError`] for an unknown look or a control it does not take.
pub fn look(name: &str, controls: &[Control]) -> Result<Box<dyn Look>, LookError> {
    match name {
        "ground" => Ok(Box::new(ground::Ground::new(controls)?)),
        _ => Err(LookError {
            control: None,
            message: format!("unknown look `{name}`"),
            help: None,
        }),
    }
}

/// The connector for a system: `db` (DungeonBlocks). OpenLOCK comes in P5.
///
/// # Errors
/// [`LookError`] for a system without a connector yet.
pub fn connector(system: &str) -> Result<Box<dyn Look>, LookError> {
    match system {
        "db" => Ok(Box::new(connector::DungeonBlocks::default())),
        _ => Err(LookError {
            control: None,
            message: format!("no connector for system `{system}` yet"),
            help: Some("P1 builds DungeonBlocks only: --system db".to_owned()),
        }),
    }
}

/// Refuse any control not in `known`.
fn only(look: &str, controls: &[Control], known: &[&str]) -> Result<(), LookError> {
    for c in controls {
        if !known.contains(&c.name.as_str()) {
            return Err(LookError::control(
                &c.name,
                format!("`{look}` has no control `{}`", c.name),
                format!("{look} takes: {}", known.join(", ")),
            ));
        }
    }
    Ok(())
}
