//! `floor` (a stone pavement: a wall laid on its side). P5: being ported; see docs/rust/plan.html, P5.

use dt_kit::Scene;

use crate::{Control, Ctx, Look, LookError, Plan, Read, Write, only};

/// The controls `floor` takes (dt-doc's vocabulary, sub-controls as
/// `group/sub`).
pub const CONTROLS: &[&str] = &["style", "texture", "slabs", "level"];

/// The `floor` look.
#[derive(Clone, Debug, PartialEq)]
pub struct Floor {
    controls: Vec<Control>,
}

impl Floor {
    /// From its controls.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("floor", controls, CONTROLS)?;
        Ok(Self {
            controls: controls.to_vec(),
        })
    }
}

impl Look for Floor {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid("floor"),
            reads: vec![Read::Solids("ground")],
            cell_mm: None,
        }
    }

    fn build(&self, _scene: &mut Scene, _ctx: &Ctx<'_>) -> Result<(), LookError> {
        Err(LookError::build("`floor` is being ported (P5)"))
    }
}
