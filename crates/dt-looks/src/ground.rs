//! `ground`: the bare ground surface at a level (P1: flat, a constant
//! field; P2 adds bumps and bump-size).

use dt_kit::Scene;

use crate::{Control, Ctx, Look, LookError, Plan, Value, Write, only};

/// The controls `ground` takes so far (`bumps` and `bump-size` come in P2).
pub const CONTROLS: &[&str] = &["level"];

/// A flat ground surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Ground {
    /// Where the surface sits, mm above the tile base at the reference.
    pub level_mm: f64,
}

impl Ground {
    /// From controls: `level` (mm; the evaluator resolves named levels
    /// and fills in the schema default, so it is always given).
    ///
    /// # Errors
    /// [`LookError`] for an unknown control, or a level missing or not
    /// above 0.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("ground", controls, CONTROLS)?;
        let bad = |msg: String| {
            LookError::control(
                "level",
                msg,
                "write a named level (ground, edge, water, wall-top) or mm, such as level=5".into(),
            )
        };
        match controls.last().map(|c| &c.value) {
            Some(Value::Number(v)) if *v > 0.0 && v.is_finite() => Ok(Self { level_mm: *v }),
            Some(v) => Err(bad(format!("ground level must be above 0 mm, not {v:?}"))),
            None => Err(bad("ground needs a level".to_owned())),
        }
    }
}

impl Look for Ground {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Surface,
            reads: Vec::new(),
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let level = self.level_mm * scene.frame().scale();
        scene.paint(ctx.area, level);
        Ok(())
    }
}
