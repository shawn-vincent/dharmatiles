//! `floor`: a stone pavement, a wall laid on its side (`walls/floor.py`,
//! Shawn: "the floor should just be a wall laying on its side … with
//! different sized bricks"). The masonry chassis builds it: courses are
//! pavement rows and bays slabs, both pinned to the square grid (`slabs`
//! per square), the wall's thickness is the pavement's depth from the
//! datum to `level` (7.4 mm, the official interior floor), the mortar core
//! one sheet a joint's depth under the top. The floor replaces the
//! ground: slabs run full depth and are cut flat at the datum. Some slabs
//! are missing (dark pits onto the mortar), some spalled (broken
//! corners); slabs run flush at the tile edges.
//!
//! The area is slab-granular: a slab is laid where its centre is in the
//! entry's area. `style=fieldstone` lays crack-network flagstones.

use dt_kit::Scene;

use crate::wall::{self, Family, Height, Mason};
use crate::{Control, Ctx, Look, LookError, Plan, Read, Value, Write, only};

/// The controls `floor` takes (dt-doc's vocabulary, sub-controls as
/// `group/sub`).
pub const CONTROLS: &[&str] = &["style", "texture", "slabs", "level"];

/// Flagstone rows and lengths for a fieldstone floor, mm at the reference.
const FLAG_ROW_MM: (f64, f64) = (5.0, 9.0);
const FLAG_LEN_MM: (f64, f64) = (7.0, 14.0);

/// The `floor` look.
#[derive(Clone, Debug, PartialEq)]
pub struct Floor {
    mason: Mason,
    slabs: u32,
}

impl Floor {
    /// From its controls.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take or a value out of
    /// range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("floor", controls, CONTROLS)?;
        let get = |n: &str| controls.iter().find(|c| c.name == n);
        let fieldstone = match get("style") {
            Some(c) => wall::word(c, &["cut-stone", "fieldstone"])? == "fieldstone",
            None => false,
        };
        let mut m = if fieldstone {
            let mut m = Mason::new(Family::Fieldstone);
            m.course = FLAG_ROW_MM;
            m.bay = FLAG_LEN_MM;
            m
        } else {
            let mut m = Mason::new(Family::Slab);
            m.texture(wall::texture("dressed").unwrap_or_default());
            m.joint = 0.125;
            m.reveal = 1.3;
            m.face_recess = 0.0;
            m.min_bond = 0.0;
            m.missing = 0.04;
            m.spall = 0.10;
            m.crack = 0.20;
            m
        };
        m.laid = true;
        m.thickness = 7.4;
        let mut slabs = 1;
        for c in controls {
            match c.name.as_str() {
                "texture" => {
                    let w = wall::word(c, &["chipped", "worn", "hewn", "dressed"])?;
                    if let Some(t) = wall::texture(w) {
                        if fieldstone {
                            m.relief = Some(t.2);
                        } else {
                            m.texture(t);
                        }
                    }
                }
                "slabs" => {
                    let v = wall::number(c, 1.0, 6.0, "slabs per square")?;
                    if v.fract() != 0.0 {
                        return Err(LookError::control(
                            "slabs",
                            format!("slabs must be a whole number, not {}", c.value),
                            "write how many slabs across a square, such as slabs=2".into(),
                        ));
                    }
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        slabs = v as u32;
                    }
                }
                "level" => {
                    m.thickness = match &c.value {
                        Value::Word(w) if w == "floor" => 7.4,
                        _ => wall::number(c, 1.0, 30.0, "floor level (mm)")?,
                    }
                }
                _ => {}
            }
        }
        Ok(Self { mason: m, slabs })
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

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let f = *scene.frame();
        let s = f.scale();
        let [w, h] = f.size_mm();
        let mut m = self.mason.clone();
        // the level scales with everything else (D2: a photocopy, every
        // level included): Run scales the thickness
        if m.family == Family::Slab {
            let pitch = f.square_mm / f64::from(self.slabs) / s;
            m.course = (pitch, pitch);
            m.bay = (pitch, pitch);
        }
        m.height = Height::Rel(h);
        // the spine along the south edge walking east: the strip covers
        // the tile northward
        let spine = [[0.0, 0.0], [w, 0.0]];
        let area = ctx.area;
        let (nx, ny) = f.nodes();
        let step = f.step_mm();
        let keep = |c: &wall::layout::Cell| {
            // laid flat: t runs east, the rows (cell z) north
            let (x, y) = (0.5 * (c.t0 + c.t1), 0.5 * (c.z0 + c.z1));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (i, j) = (
                (libm::round(x / step) as usize).min(nx - 1),
                (libm::round(y / step) as usize).min(ny - 1),
            );
            area.bits[j * nx + i]
        };
        let (solid, counts, mut fp, top) = wall::run(&m, scene, ctx, &spine, "floor", &keep)?;
        if counts.units == 0 {
            return Err(LookError::build(
                "the floor's area holds no whole slab: no slab centre falls inside it",
            ));
        }
        for (b, a) in fp.bits.iter_mut().zip(&area.bits) {
            *b = *b && *a;
        }
        wall::finish_build(scene, ctx, "floor", solid, counts, &fp, top);
        Ok(())
    }
}
