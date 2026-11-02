//! `wall` (masonry walls on a tile edge). P5: being ported; see docs/rust/plan.html, P5.

use dt_kit::Scene;

use crate::{Control, Ctx, Look, LookError, Plan, Read, Write, only};

/// The controls `wall` takes (dt-doc's vocabulary, sub-controls as
/// `group/sub`).
pub const CONTROLS: &[&str] = &[
    "style",
    "height",
    "thickness",
    "stones/height",
    "stones/length",
    "stones/texture",
    "joints/width",
    "joints/depth",
    "edges/rounding",
    "coping",
    "bond",
    "ruin",
    "top",
];

/// The `wall` look.
#[derive(Clone, Debug, PartialEq)]
pub struct Wall {
    controls: Vec<Control>,
}

impl Wall {
    /// From its controls.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("wall", controls, CONTROLS)?;
        Ok(Self {
            controls: controls.to_vec(),
        })
    }
}

impl Look for Wall {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid("wall"),
            reads: vec![Read::Solids("ground"), Read::IfAny("floor")],
            cell_mm: None,
        }
    }

    fn build(&self, _scene: &mut Scene, _ctx: &Ctx<'_>) -> Result<(), LookError> {
        Err(LookError::build("`wall` is being ported (P5)"))
    }
}
