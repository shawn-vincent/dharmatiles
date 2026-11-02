//! `tree` (a space-colonisation tree with foliage clumps and leaves). P5: being ported; see docs/rust/plan.html, P5.

use dt_kit::Scene;

use crate::{Control, Ctx, Look, LookError, Plan, Read, Write, only};

/// The controls `tree` takes (dt-doc's vocabulary, sub-controls as
/// `group/sub`).
pub const CONTROLS: &[&str] = &[
    "height",
    "trunk/height",
    "trunk/width",
    "crown/width",
    "crown/shape",
    "crown/clumps",
    "crown/clump-size",
    "crown/branching",
    "leaves",
    "leaf/length",
    "leaf/width",
    "branch/split",
    "branch/eagerness",
    "branch/target",
    "branch/balance",
    "branch/exponent",
    "branch/min-radius",
];

/// The `tree` look.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    controls: Vec<Control>,
}

impl Tree {
    /// From its controls.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("tree", controls, CONTROLS)?;
        Ok(Self {
            controls: controls.to_vec(),
        })
    }
}

impl Look for Tree {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid("tree"),
            reads: vec![
                Read::Solids("ground"),
                Read::IfAny("wall"),
                Read::IfAny("floor"),
            ],
            cell_mm: None,
        }
    }

    fn build(&self, _scene: &mut Scene, _ctx: &Ctx<'_>) -> Result<(), LookError> {
        Err(LookError::build("`tree` is being ported (P5)"))
    }
}
