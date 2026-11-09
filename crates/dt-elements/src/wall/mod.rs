//! `wall`: masonry walls on a tile edge, and the masonry chassis floors
//! share. Openings are not built: there is no vocabulary for
//! them yet.
//!
//! A wall is a spine (`along=`, the outer face, the body to its left)
//! split into segments; courses × bays × quoin alternation make cells
//! ([`layout`]); battlements and ruin edit the cells; every cell becomes
//! one unit: a [`block`] (cut stone, brick, regular brick, floor slab) or
//! a [`fieldstone`] stone. Mortared families stand on a recessed core
//! (the joints are reveals onto it); drystone has no core, rough
//! [`rubble`] hearting fills it. Every unit is its own leaf solid; the
//! wall is their union, cut to the tile (butt joins), tagged `wall`, and
//! its footprint stamped so what grows after it steers round it.
//!
//! Seed and region (the plan's bug fix): the wall follows its entry's
//! seed, and seats over its footprint within its area; a wall whose
//! footprint misses its area is refused.
//!
//! Scale: every author size scales with the square; the joint width
//! and the most a stone may stand proud of the one under it (0.45 mm) are
//! printability floors and do not.

pub(crate) mod block;
mod chassis;
pub(crate) mod fieldstone;
pub(crate) mod layout;
pub(crate) mod rubble;

use dt_kit::{Kind, Scene};

use crate::read;
use crate::{Control, Ctx, Element, LookError, Plan, Read, Value, Write};
#[cfg(test)]
use chassis::percentile;
pub(crate) use chassis::{Laid, Run, build, cell_seed, finish_build, run};
pub(crate) use layout::Bond;

/// The official wall top above the datum, mm at the reference.
const WALL_TOP_MM: f64 = 33.1;
/// `low`: half the wall top (provisional).
const LOW_TOP_MM: f64 = 16.55;
/// Mortar core edge fillet, mm.
const CORE_ROUND_MM: f64 = 0.6;
/// The thinnest mortar core a wall keeps between its joints' depths, mm
/// at the 35 mm square: past the core's rounding on both faces, with
/// room for a line of plastic.
const MIN_CORE_MM: f64 = 2.0 * CORE_ROUND_MM + 0.45;
/// The printer's gap (a joint thinner than this closes in print and
/// needs no pointing), mm.
const GAP_MM: f64 = 0.2;

/// The texture presets: chip, rounding, relief.
pub(crate) fn texture(word: &str) -> Option<(f64, f64, f64)> {
    match word {
        "chipped" => Some((0.55, 0.22, 0.07)),
        "worn" => Some((0.30, 0.60, 0.10)),
        "hewn" => Some((0.80, 0.15, 0.16)),
        "dressed" => Some((0.15, 0.12, 0.03)),
        _ => None,
    }
}

/// The masonry families.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    CutStone,
    Brick,
    RegularBrick,
    Fieldstone,
    /// A floor slab (`StoneFloor`).
    Slab,
}

/// Where the wall's top goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Height {
    /// The top at this level above the datum (mm at the reference).
    Top(f64),
    /// This height above the seat (mm at the reference); laid flat, the
    /// strip's depth in plan (true mm).
    Rel(f64),
}

/// A masonry build, in author mm at the reference (scaled when built).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mason {
    pub family: Family,
    pub laid: bool,
    pub thickness: f64,
    pub height: Height,
    pub course: (f64, f64),
    pub bay: (f64, f64),
    pub joint: f64,
    pub reveal: f64,
    pub min_bond: f64,
    pub chip: f64,
    pub rounding: (f64, f64),
    /// Relief mm (`None`: fieldstone's auto amplitude).
    pub relief: Option<f64>,
    pub face_recess: f64,
    pub yaw_deg: f64,
    pub tilt_deg: f64,
    pub bond: Bond,
    pub ruin: f64,
    pub battlements: bool,
    pub coping_on_edge: bool,
    pub missing: f64,
    pub spall: f64,
    pub crack: f64,
    pub embed: f64,
    /// A laid piece on the tile's boundary runs this far past the plane
    /// (true mm), so the cut at the plane is square and its top meets the
    /// plane at the piece's own height; 0: its rounded edge stays.
    pub edge_pad: f64,
    /// A laid stone's flanks within this far of the tile's boundary are
    /// drawn to the plane (true mm, 0: they keep their pillow shape).
    pub edge_pull: f64,
    /// Wood grain depth, mm, and the share of boards with a check.
    pub grain: f64,
    pub checks: f64,
}

impl Mason {
    /// A family's defaults.
    pub fn new(family: Family) -> Self {
        let base = Self {
            family,
            laid: false,
            thickness: 7.0,
            height: Height::Top(WALL_TOP_MM),
            course: (5.5, 8.5),
            bay: (11.0, 20.0),
            joint: 0.11,
            reveal: 1.3,
            min_bond: 3.0,
            chip: 0.55,
            rounding: (0.22, 0.22),
            relief: Some(0.07),
            face_recess: 0.60,
            yaw_deg: 1.2,
            tilt_deg: 0.7,
            bond: Bond::Irregular,
            ruin: 0.0,
            battlements: false,
            coping_on_edge: false,
            missing: 0.0,
            spall: 0.0,
            crack: 0.0,
            embed: 2.5,
            edge_pad: 0.0,
            edge_pull: 0.0,
            grain: 0.0,
            checks: 0.0,
        };
        match family {
            Family::CutStone | Family::Slab => base,
            Family::Brick => Self {
                course: (2.5, 3.0),
                bay: (5.0, 6.6),
                joint: 0.125,
                reveal: 1.0,
                min_bond: 2.0,
                chip: 0.25,
                rounding: (0.35, 0.35),
                relief: Some(0.08),
                missing: 0.04,
                spall: 0.10,
                ..base
            },
            Family::RegularBrick => {
                let (chip, r, rel) = texture("dressed").unwrap_or_default();
                Self {
                    course: (3.2, 3.2),
                    bay: (8.5, 8.5),
                    joint: 0.45,
                    reveal: 1.0,
                    bond: Bond::Running,
                    chip,
                    rounding: (r, r),
                    relief: Some(rel),
                    face_recess: 0.12,
                    yaw_deg: 0.25,
                    tilt_deg: 0.15,
                    ..base
                }
            }
            Family::Fieldstone => Self {
                course: (2.2, 5.2),
                bay: (5.0, 16.0),
                joint: 0.0,
                reveal: 2.8,
                min_bond: 1.8,
                rounding: (1.3, 2.6),
                relief: None,
                ..base
            },
        }
    }

    /// Set a texture preset's chip, rounding and relief.
    pub fn texture(&mut self, (chip, r, rel): (f64, f64, f64)) {
        self.chip = chip;
        self.rounding = (r, r);
        self.relief = Some(rel);
    }
}

/// The `wall` element.
#[derive(Clone, Debug, PartialEq)]
pub struct Wall {
    mason: Mason,
}

impl Wall {
    /// From its controls.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take or a value out of
    /// range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        let get = |n: &str| controls.iter().find(|c| c.name == n);
        let family = match get("style") {
            Some(c) => match read::word(c)? {
                "fieldstone" => Family::Fieldstone,
                "brick" => Family::Brick,
                "regular-brick" => Family::RegularBrick,
                _ => Family::CutStone,
            },
            None => Family::CutStone,
        };
        let mut m = Mason::new(family);
        let mut depth_set = false;
        for c in controls {
            match c.name.as_str() {
                "height" => {
                    m.height = match &c.value {
                        Value::Word(w) if w == "full" => Height::Top(WALL_TOP_MM),
                        Value::Word(w) if w == "low" => Height::Top(LOW_TOP_MM),
                        _ => Height::Rel(read::number(c)?),
                    }
                }
                "thickness" => m.thickness = read::number(c)?,
                "stones/height" => m.course = read::span(c)?,
                "stones/length" => m.bay = read::span(c)?,
                "stones/texture" => {
                    let w = read::word(c)?;
                    if let Some(t) = texture(w) {
                        if family == Family::Fieldstone {
                            // fieldstone keeps its pebble roundings; the
                            // preset sets the relief only
                            m.relief = Some(t.2);
                        } else {
                            m.texture(t);
                        }
                    }
                }
                "joints/width" => m.joint = read::number(c)?,
                "joints/depth" => {
                    m.reveal = read::number(c)?;
                    depth_set = true;
                }
                "edges/rounding" => m.rounding = read::span(c)?,
                "coping" => m.coping_on_edge = read::word(c)? == "on-edge",
                "bond" => {
                    m.bond = if read::word(c)? == "running" {
                        Bond::Running
                    } else {
                        Bond::Irregular
                    }
                }
                "ruin" => {
                    // a share, written as a percent: the element reads 0..1
                    m.ruin = read::number(c)?;
                }
                "top" => m.battlements = read::word(c)? == "battlements",
                _ => {}
            }
        }
        // the mortar core sits a joint's depth in from both faces; the
        // units are bedded on it. A core thinner than MIN_CORE_MM (or none)
        // leaves the units floating, so the joints may be no deeper than
        // half of what the wall's thickness leaves past it. A depth the
        // author wrote is refused (they asked for something the wall
        // cannot have); a family's default depth is made shallower to fit
        // (they asked for a thin wall, and its joints follow).
        if family != Family::Fieldstone {
            let deepest = 0.5 * (m.thickness - MIN_CORE_MM);
            if m.reveal > deepest {
                if depth_set {
                    return Err(LookError::control(
                        "joints/depth",
                        format!(
                            "joints {} mm deep leave no mortar core in a wall {} mm thick, so the stones would float",
                            m.reveal, m.thickness
                        ),
                        format!(
                            "make joints depth at most {:.2} mm, or the wall thicker than {:.2} mm",
                            deepest.max(0.0),
                            2.0 * m.reveal + MIN_CORE_MM
                        ),
                    ));
                }
                m.reveal = deepest.max(0.0);
            }
        }
        if m.bond == Bond::Running && family != Family::RegularBrick {
            // a running bond on any family: uniform courses of the mean
            let mid = 0.5 * (m.course.0 + m.course.1);
            m.course = (mid, mid);
        }
        Ok(Self { mason: m })
    }
}

impl Element for Wall {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Wall),
            reads: vec![Read::Solids(Kind::Terrain), Read::IfAny(Kind::Floor)],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let Some(along) = ctx.along else {
            return Err(LookError {
                control: None,
                message: "a wall needs along= the tile edges it stands on".into(),
                help: Some("write along=\"north\" or along=\"north west\"".into()),
                code: None,
            });
        };
        let sq = scene.frame().square_mm;
        let spine: Vec<[f64; 2]> = along.iter().map(|p| [p[0] * sq, p[1] * sq]).collect();
        build(&self.mason, scene, ctx, &spine, Kind::Wall)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn c(name: &str, v: Value) -> Control {
        Control {
            name: name.into(),
            value: v,
        }
    }

    #[test]
    fn controls_set_the_family_and_its_defaults() {
        let w = Wall::new(&[c("style", Value::Word("brick".into()))]).unwrap();
        assert_eq!(w.mason.family, Family::Brick);
        assert!((w.mason.course.0 - 2.5).abs() < 1e-12);
        let w = Wall::new(&[
            c("style", Value::Word("cut-stone".into())),
            c("stones/texture", Value::Word("hewn".into())),
            // `ruin="40%"`, as the evaluator hands it on
            c("ruin", Value::Number(0.4)),
        ])
        .unwrap();
        assert!((w.mason.chip - 0.80).abs() < 1e-12);
        assert!((w.mason.ruin - 0.4).abs() < 1e-12);
    }

    #[test]
    fn joints_never_leave_the_core_too_thin() {
        // a thin brick wall: the default joint depth is made to fit
        let w = Wall::new(&[
            c("style", Value::Word("brick".into())),
            c("thickness", Value::Number(2.0)),
        ])
        .unwrap();
        assert!(w.mason.thickness - 2.0 * w.mason.reveal >= MIN_CORE_MM - 1e-12);
        // a depth the author wrote that leaves no core is refused
        let e = Wall::new(&[
            c("thickness", Value::Number(7.0)),
            c("joints/depth", Value::Number(3.5)),
        ])
        .unwrap_err();
        assert_eq!(e.control.as_deref(), Some("joints/depth"));
        // the default walls keep their depth
        let w = Wall::new(&[]).unwrap();
        assert!((w.mason.reveal - 1.3).abs() < 1e-12);
    }

    #[test]
    fn percentiles_interpolate() {
        assert!((percentile(vec![1.0, 2.0, 3.0, 4.0, 5.0], 80.0) - 4.2).abs() < 1e-12);
    }
}
