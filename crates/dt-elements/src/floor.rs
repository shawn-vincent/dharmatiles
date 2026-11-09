//! `flagstones` and `planks`: the floors, two pieces bodies.
//!
//! A floor is a wall laid on its side (Shawn: "the floor should just be a
//! wall laying on its side"): the masonry chassis lays the slabs, or the
//! boards, as separate solids (each its own slot), and their union is the
//! body. The body has a band like dirt has (`top=`, solved with its pins,
//! each held to its place, nothing eased: slabs step). What is different
//! from a field body is that it is made of pieces, so each piece is made to
//! fit the band as it is made:
//!
//! 1. the body's heights are a heaved surface inside the band (the same
//!    calculator dirt's hills use, [`dt_kit::span::hills`]): one height is
//!    a flat floor, a wide band a heaved one;
//! 2. each slab or board takes a plane from that surface where it stands
//!    (its height at its middle, its tilt across it) held under the
//!    steepest tilt, then reduced by [`dt_kit::pieces::Pieces::plane`] until
//!    it is inside the band over every node the piece stands on; nothing
//!    is ever trimmed afterwards;
//! 3. a piece whose finished mesh stands over the band is refused
//!    (`dt::band`, a bug).
//!
//! The slabs stand on the land under them (they reach down to its lowest
//! point, or to zero, the base, with no land), so none is left over a
//! hollow; where the land is higher than a slab the land shows, the surface
//! being the higher of what is below and the body's own top. The plan lays a
//! floor after the surfaces and before the lift (`Write::Laid`). A floor
//! meets the tile edge flush at its own height, with its faces at the plane
//! keeping the material's grain and never passing it: a slab or board on the
//! boundary stands a hair past the plane and is cut there (its top edge is
//! square, its relief lies at or just behind the plane), a drystone flag's
//! upper flank is drawn to the plane, and a board's end grain carries the
//! wood's pattern set back from it. Weathered flagstones (`texture=worn`)
//! keep a worn round of their own at the edge instead.
//!
//! Slabs are laid where their middle is inside the entry's area.
//! `style=fieldstone` lays crack-network flagstones.

use dt_kit::span::hills;
use dt_kit::{Field, Kind, Mask, Noise, Scale, Scene, noise};

use crate::read::Settings;
use crate::wall::{self, Family, Height, Laid, Mason};
use crate::{Control, Ctx, Draw, Element, LookError, Plan, Write};

/// The steepest a slab or board tilts, degrees.
const TILT_MAX_DEG: f64 = 4.0;
/// The heave's blur, mm at the 35 mm square: about a slab across.
const HEAVE_SIGMA_MM: f64 = 7.0;
/// How steeply the heave may rise from a pinned place, degrees.
const HEAVE_SLOPE_DEG: f64 = 35.0;
/// Past this a band has room (a thinner one is a pin).
const ROOM_MM: f64 = 0.02;
/// The nominal pavement depth the stones are drawn for, mm at the
/// reference (their feet go on to the land).
const DEPTH_MM: f64 = 7.4;
/// How far a slab on the tile's boundary stands outside the plane before
/// it is cut there, true mm: past its arris rounding, so the top edge is
/// square, and no further than the relief's depth, so its face keeps the
/// stone's chipped texture where the relief cuts it back.
const BLOCK_PAD_MM: f64 = 0.05;
/// How far from the plane a drystone flag's flank is drawn to it, true mm:
/// its pillow shape is kept everywhere else.
/// The same for a board, whose faces carry the grain set back by up to
/// this much more.
const PLANK_PAD_MM: f64 = 0.1;
const FIELDSTONE_PULL_MM: f64 = 3.0;

/// `flagstones`: stone slabs.
#[derive(Clone, Debug, PartialEq)]
pub struct Flagstones {
    mason: Mason,
    slabs: u32,
}

/// `planks`: wooden boards.
#[derive(Clone, Debug, PartialEq)]
pub struct Planks {
    mason: Mason,
    boards: u32,
    even: bool,
}

/// How many across a square: a whole number of at least one, or `default`.
fn whole(s: &Settings, name: &str, default: u32) -> Result<u32, LookError> {
    let Some(v) = s.number(name)? else {
        return Ok(default);
    };
    if v.fract() != 0.0 || v < 1.0 {
        return Err(LookError::control(
            name,
            format!("{name} must be a whole number, not {v}"),
            format!("write how many across a square, such as {name}=2"),
        ));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(v as u32)
}

impl Flagstones {
    /// From its controls.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take or a value out of
    /// range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        let s = Settings::new(controls);
        let fieldstone = s.word("style")? == Some("fieldstone");
        let mut m = if fieldstone {
            // the fieldstone wall's own stone sizes, laid flat
            Mason::new(Family::Fieldstone)
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
        m.across = DEPTH_MM;
        if fieldstone {
            m.edge_pull = FIELDSTONE_PULL_MM;
        } else {
            m.edge_pad = BLOCK_PAD_MM;
        }
        if let Some(w) = s.word("texture")? {
            if let Some(t) = wall::texture(w) {
                if fieldstone {
                    m.relief = Some(t.2);
                } else {
                    m.texture(t);
                }
            }
            if w == "worn" {
                // weathered: the worn edge takes a round of its own
                // at the plane
                m.edge_pad = 0.0;
                m.edge_pull = 0.0;
            }
        }
        let slabs = whole(&s, "slabs", 1)?;
        Ok(Self { mason: m, slabs })
    }
}

/// The plank textures: chip, rounding, relief, grain depth, share with a
/// check.
fn plank_texture(word: &str) -> Option<((f64, f64, f64), f64, f64)> {
    match word {
        "smooth" => Some(((0.10, 0.14, 0.0), 0.03, 0.15)),
        "worn" => Some(((0.25, 0.40, 0.0), 0.05, 0.30)),
        "rough" => Some(((0.35, 0.25, 0.04), 0.09, 0.50)),
        _ => None,
    }
}

impl Planks {
    /// From its controls.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take or a value out of
    /// range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        let mut m = Mason::new(Family::Slab);
        m.laid = true;
        m.across = DEPTH_MM;
        m.joint = 0.3;
        m.reveal = 1.0;
        m.face_recess = 0.0;
        m.missing = 0.0;
        m.spall = 0.0;
        m.edge_pad = PLANK_PAD_MM;
        let (tex, grain, checks) = plank_texture("worn").unwrap_or_default();
        m.texture(tex);
        m.grain = grain;
        m.checks = checks;
        let s = Settings::new(controls);
        let even = s.word("style")? == Some("even");
        if let Some((tex, grain, checks)) = s.word("texture")?.and_then(plank_texture) {
            m.texture(tex);
            m.grain = grain;
            m.checks = checks;
        }
        let boards = whole(&s, "boards", 8)?;
        Ok(Self {
            mason: m,
            boards,
            even,
        })
    }
}

fn plan() -> Plan {
    Plan {
        // it stands on the land the surfaces laid (zero, the base, if the
        // tile has none), so it is laid before the lift
        writes: Write::Laid(Kind::Floor),
        reads: vec![],
        cell_mm: None,
    }
}

/// The heaved surface in a floor's band: the band itself where it is a
/// single height, else the smoothest surface in it with noise spread over
/// the room there is.
pub(crate) fn heave(f: &dt_kit::Frame, seed: dt_kit::Seed, band: &dt_kit::Band) -> (Field, bool) {
    let (lo, hi) = (band.lo(), band.hi());
    if (0..lo.z.len()).all(|k| hi.z[k] - lo.z[k] < ROOM_MM) {
        return (lo.clone(), true);
    }
    let n = noise(
        f,
        seed.key("heave"),
        Noise::Smooth {
            sigma_mm: HEAVE_SIGMA_MM,
        },
        Scale::Layout,
    );
    let h = hills(&n, band, f64::MAX, HEAVE_SLOPE_DEG, f.scale());
    if !h.converged {
        return (h.shape.field().clone(), false);
    }
    (h.shape.field().clone(), true)
}

/// Lay a floor of `m`'s pieces in `material`: heave, fit, build, stamp.
fn lay(
    m: &Mason,
    scene: &mut Scene,
    ctx: &Ctx<'_>,
    material: dt_kit::Material,
    what: &str,
) -> Result<(), LookError> {
    let f = *scene.frame();
    let Some(band) = ctx.band else {
        return Err(LookError::general(
            format!("the {what} has no solved band"),
            "the tile was not solved: run check to see why",
        ));
    };
    let [w, h] = f.size_mm();
    let (surface, settled) = heave(scene.frame(), ctx.seed, band);
    if !settled {
        scene.note("relax_unconverged", 1.0);
    }
    let laid = Laid {
        fit: dt_kit::pieces::Pieces::new(band, &f),
        heave: &surface,
        tilt_max: libm::tan(TILT_MAX_DEG.to_radians()),
        material,
        step: f.step_mm(),
        pull: m.edge_pull,
    };
    let mut m = m.clone();
    m.height = Height::Rel(h);
    // the spine along the south edge walking east: the strip covers the
    // tile northward
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
    let (solid, counts, _, _, placed) =
        wall::run(&m, scene, ctx, &spine, Kind::Floor, &keep, Some(&laid))?;
    if counts.units == 0 {
        return Err(LookError::general(
            format!("the {what}'s area holds no whole piece: no piece centre falls inside it"),
            "make its area bigger",
        ));
    }
    // what stands on the floor sees each piece's top where it stands; the
    // gaps between (joints, a missing slab) the heaved surface
    let mut tops = vec![f64::NEG_INFINITY; nx * ny];
    for (k, t) in tops.iter_mut().enumerate() {
        if area.bits[k] {
            *t = surface.z[k];
        }
    }
    for p in &placed {
        for j in 0..ny {
            #[allow(clippy::cast_precision_loss)]
            let y = j as f64 * step;
            if y < p.lo[1] || y > p.hi[1] {
                continue;
            }
            for i in 0..nx {
                #[allow(clippy::cast_precision_loss)]
                let x = i as f64 * step;
                if x >= p.lo[0] && x <= p.hi[0] {
                    let k = j * nx + i;
                    if area.bits[k] {
                        tops[k] = tops[k].max(p.plane.height([x, y]));
                    }
                }
            }
        }
    }
    // the pieces' own stamp replaces the flat one
    let none = Mask::new(nx, ny, false);
    wall::finish_build(scene, ctx, Kind::Floor, solid, counts, &none, 0.0);
    scene.stamp_heights(&tops, dt_kit::Under::Bedding);
    Ok(())
}

impl Element for Flagstones {
    fn plan(&self) -> Plan {
        plan()
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let s = scene.frame().scale();
        let mut m = self.mason.clone();
        if m.family == Family::Slab {
            let pitch = scene.frame().square_mm / f64::from(self.slabs) / s;
            m.course = Draw::flat(pitch, pitch);
            m.bay = Draw::flat(pitch, pitch);
        }
        lay(&m, scene, ctx, dt_kit::Material::Masonry, "flagstones")
    }
}

impl Element for Planks {
    fn plan(&self) -> Plan {
        plan()
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let f = scene.frame();
        let s = f.scale();
        let mut m = self.mason.clone();
        // boards run east to west: a board's width is one course
        let width = f.square_mm / f64::from(self.boards) / s;
        m.course = Draw::flat(width, width);
        if self.even {
            // every board half a square long, each course set half a board
            // on from the last
            m.bay = Draw::flat(17.5, 17.5);
            m.bond = wall::Bond::Running;
        } else {
            m.bay = Draw::flat(9.0, 26.0);
            m.min_bond = 4.0;
        }
        lay(&m, scene, ctx, dt_kit::Material::Wood, "planks")
    }
}
