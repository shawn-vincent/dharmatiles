//! Cliff: a stacked pile of rocks (foot 3.4 mm).
//!
//! A cliff is a body (`cliff top=… at=…`): its region is the footprint its
//! stones fill and its band, solved with its pins, holds the stones' tops. It
//! is **stacked under gravity**: the body's heaved shape in its band (a
//! plane for a single height, a jagged crest for a wide band) is the target;
//! rocks are placed lowest pocket first, each resting on the highest
//! supports under it ([`stack`]), and a stone that reaches the band is made
//! to fit it as it is stacked ([`fit`]): its top plane is the nearest one
//! that is inside the band over every node it stands on. Nothing is trimmed
//! after, so there is no hollow behind or under a stone.
//!
//! Stones that share a footprint stand in one **column**, cleaved by whole
//! planes planned once on the column's hull ([`finish`]). Each open face of
//! the pile gets one crack network, planned once so cracks run on across
//! stones ([`faces`]); each stone is then cut to its column's planes,
//! grooved under the stone it carries, **seated** on its bed at the overhang
//! limit (so a stone that rests on rock has no ceiling by construction),
//! chipped, carved with the face relief and cracks ([`relief`]), fitted to
//! the tile and grained ([`stone`]), the grain held under the band's high.
//! A **core** behind the stones makes a gap between them a recess, never a
//! hole: the pile set in from its faces, laid as a rock layer of the terrain
//! with the land, before the lift ([`core`]). The dirt film on the tops lies
//! on it and follows the stones' tops.
//!
//! The engine (planning the pile, dressing its stones) is [`pile`]; this
//! module is the body that lays what it makes.
//!
//! Details (cracks, joints, chips, grain) are printability sizes and never
//! scale; masses scale with the system's square.

// Lattice and grid arithmetic here converts between float positions and
// indices throughout, on values that are small and checked in range.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

mod chips;
mod core;
mod faces;
mod film;
mod finish;
mod fit;
mod form;
mod lines;
mod pile;
mod relief;
mod stack;
mod stone;
mod util;

use dt_kit::Kind;
use dt_kit::{Band, Field, Material, OnFrame, Scene, Tag};

use crate::read::{Pick, Settings};
use crate::{Control, Ctx, Element, LookError, Plan, Write};
use pile::{Look, Planned, Site};

/// The cliff element.
#[derive(Clone, Debug)]
pub struct Cliff {
    look: Look,
    /// The pile planned for this build, kept from the plan pass (a scatter
    /// reads the foot) to the build, which starts from the same plan.
    plan: Memo,
}

/// A pile plan kept under the key of everything that decided it. Cloning a
/// cliff clones no plan.
#[derive(Default)]
struct Memo(std::sync::Mutex<Option<(u64, std::sync::Arc<Planned>)>>);

impl Clone for Memo {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for Memo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Memo")
    }
}

const KIND_NAMES: [&str; 5] = ["columns", "blocks", "slabs", "odd", "monoliths"];

impl Cliff {
    /// From controls.
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        let mut c = Self {
            look: Look {
                weights: None,
                size: 1.0 / 1.35,
                patches: None,
                patch_width: None,
                ledges: 1,
                ledge_height: None,
            },
            plan: Memo::default(),
        };
        let mut w = [f64::NAN; 5];
        let s = Settings::new(controls);
        // a word stands for its number (`stone-size.stone-size`: rubble 1,
        // broken 1.15, massive 1.35); a share under 1 sits between rubble
        // and massive
        let share = match s.pick("stone-size")? {
            Some(Pick::Word(_)) => Some(s.number("stone-size.stone-size")?.unwrap_or(1.0)),
            Some(Pick::Number(v)) => Some(v),
            None => None,
        };
        if let Some(v) = share {
            c.look.size = if v >= 1.0 {
                v / 1.35
            } else {
                (1.0 + 0.35 * v.clamp(0.0, 1.0)) / 1.35
            };
        }
        // the `rock` word and the numbers behind it (`rock.<kind>`): a
        // later write wins, so the word (written after its numbers) stands
        // alone
        for (name, what) in s.family("rock")? {
            match what {
                Pick::Word(word) => {
                    if let Some(i) = KIND_NAMES.iter().position(|n| *n == word) {
                        w = [0.0; 5];
                        w[i] = 1.0;
                    }
                }
                Pick::Number(v) => {
                    let kind = name.strip_prefix("rock.").unwrap_or_default();
                    if let Some(i) = KIND_NAMES.iter().position(|n| *n == kind) {
                        w[i] = v.max(0.0);
                    }
                }
            }
        }
        c.look.ledge_height = s.number("ledges/height")?.or(c.look.ledge_height);
        if let Some(v) = s.number("ledges")? {
            c.look.ledges = v.clamp(0.0, 2.0).round() as u32;
        }
        c.look.patches = s.number("shattered/patches")?.or(c.look.patches);
        c.look.patch_width = s.span("shattered/width")?.or(c.look.patch_width);
        if w.iter().any(|x| !x.is_nan()) {
            let w: [f64; 5] = w.map(|x| if x.is_nan() { 0.0 } else { x });
            if w.iter().sum::<f64>() > 0.0 {
                c.look.weights = Some(w);
            }
        }
        Ok(c)
    }
}

impl Cliff {
    /// Everything that decides where the stones stand, with no geometry:
    /// the plan pass (a scatter reads its foot) and the build both start
    /// here, from the same seed.
    fn pile_plan(
        &self,
        frame: &dt_kit::Frame,
        ctx: &Ctx<'_>,
    ) -> Result<std::sync::Arc<Planned>, LookError> {
        let key = Self::plan_key(frame, ctx);
        let mut kept = self
            .plan
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((k, p)) = kept.as_ref()
            && *k == key
        {
            return Ok(p.clone());
        }
        let planned = std::sync::Arc::new(pile::plan(&self.look, &site(frame, ctx)?)?);
        *kept = Some((key, planned.clone()));
        Ok(planned)
    }

    /// Everything the plan reads: the frame, the seed, the land, the area and
    /// the band, hashed.
    fn plan_key(frame: &dt_kit::Frame, ctx: &Ctx<'_>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        frame.hash(&mut h);
        ctx.seed.value().hash(&mut h);
        ctx.land_mm.to_bits().hash(&mut h);
        ctx.area.content_hash().hash(&mut h);
        if let Some(b) = ctx.band {
            for v in b.lo().z.iter().chain(&b.hi().z) {
                v.to_bits().hash(&mut h);
            }
        }
        h.finish()
    }
}

/// Where the pile stands in this build.
fn site<'a>(frame: &'a dt_kit::Frame, ctx: &'a Ctx<'_>) -> Result<Site<'a>, LookError> {
    let Some(band) = ctx.band else {
        return Err(LookError::general(
            "this cliff has no solved top",
            "write the tile in the heights language: `cliff top=\"dirtEdgeHeight + 35\"`",
        ));
    };
    Ok(Site {
        frame,
        seed: ctx.seed,
        area: ctx.area,
        band,
        foot: ctx.land_mm,
        draft: ctx.draft,
    })
}

impl Element for Cliff {
    fn plan(&self) -> Plan {
        Plan {
            // the cliff lays its rock core with the land, before the lift,
            // and its stones with it, so a film of dirt after it in the
            // stack lies on its tops
            writes: Write::Surface,
            reads: vec![],
            cell_mm: None,
        }
    }

    fn foot(&self, frame: &dt_kit::Frame, ctx: &Ctx<'_>) -> Result<Option<crate::Foot>, LookError> {
        let planned = self.pile_plan(frame, ctx)?;
        let (zone, st) = (&planned.zone, &planned.st);
        let (nx, ny) = frame.nodes();
        let step = frame.step_mm();
        let mut rock = vec![false; nx * ny];
        let mut wts = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (i as f64 * step, j as f64 * step);
                let q = j * nx + i;
                if let Some(c) = pile::node_cell(x, y, st.s.nx, st.s.ny) {
                    rock[q] = st.s.z[c] > core::ROCK_MIN_MM;
                }
                wts[q] = 0.5 + (zone.at(x) - form::ZONE_FLOOR) / (1.0 - form::ZONE_FLOOR);
            }
        }
        let (d, _) = dt_kit::distance(frame, &rock);
        let dist = d.z;
        let mut cells = Vec::new();
        for j in 1..ny - 1 {
            for i in 1..nx - 1 {
                let q = j * nx + i;
                if dist[q] > 0.0 && dist[q] <= 1.01 * step {
                    let g = [dist[q + 1] - dist[q - 1], dist[q + nx] - dist[q - nx]];
                    let l = libm::hypot(g[0], g[1]).max(1e-9);
                    cells.push((
                        [i as f64 * step, j as f64 * step],
                        [g[0] / l, g[1] / l],
                        wts[q],
                    ));
                }
            }
        }
        let squares = cells.len() as f64 * step / frame.square_mm;
        Ok(Some(crate::Foot {
            kind: Kind::Cliff,
            dist,
            cells,
            squares,
            nx,
        }))
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let progress = scene.progress();
        let frame = *scene.frame();
        let planned = self.pile_plan(&frame, ctx)?;
        let site = site(&frame, ctx)?;
        progress.fraction("planning the pile", 0.01);
        if planned.st.rocks.is_empty() {
            return Ok(());
        }
        let slots = scene
            .slots(planned.slots_needed(), Material::Rock)
            .map_err(LookError::build)?;
        let dressed = pile::dress(&planned, &site, &slots, &progress)?;
        progress.fraction("laying the core", 0.90);
        let meshes: Vec<dt_kit::rays::RayMesh> = dressed
            .pieces
            .iter()
            .map(|(_, p)| dt_kit::rays::RayMesh::new(&p.mesh()))
            .collect();
        let foot = planned.foot;
        Self::lay_core(
            scene,
            ctx,
            &planned.fm,
            &dressed.core_field,
            &planned.st.s,
            foot,
            &meshes,
        )?;
        progress.fraction("placing stones", 0.98);
        let n_stones = dressed.pieces.len();
        let m = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, foot];
        for (_, p) in dressed.pieces {
            let solid = p.transformed(&m).into_solid();
            scene.add(
                Tag {
                    kind: Kind::Cliff,
                    owner: ctx.path.to_owned(),
                },
                solid,
            );
        }
        scene.note("cliff_stones", n_stones as f64);
        scene.note("cliff_repeated_stones", dressed.repeated as f64);
        scene.note("cliff_top_cracks", dressed.tops as f64);
        Ok(())
    }
}

impl Cliff {
    /// The core as a rock layer of the terrain (set in from the faces, down
    /// from the tops, clear of the tile's sides), and what the finished
    /// stones' tops give a film laid on them.
    fn lay_core(
        scene: &mut Scene,
        ctx: &Ctx<'_>,
        fm: &form::Form,
        core_field: &Field,
        s: &Field,
        foot: f64,
        stones: &[dt_kit::rays::RayMesh],
    ) -> Result<(), LookError> {
        let frame = *scene.frame();
        let (nx, ny) = frame.nodes();
        let tops = film::stone_tops(&frame, stones, foot);
        let (mut mask, mut z) = core::core_mask(&frame, fm, core_field, s, foot);
        // the core stands under the stones as they are, not as planned
        for k in 0..nx * ny {
            if mask.bits[k] && tops[k].is_finite() {
                z[k] = z[k].min(tops[k] - core::CORE_DROP_MM);
            }
        }
        core::draw_back_from_faces(&frame, &mut mask, &z, foot, stones);
        let flat = film::film_ground(&frame, &tops, ctx.area, &mask, frame.scale());
        if mask.bits.iter().any(|&b| b) {
            let field = Field::new(&frame, 0.0).with_z(z);
            let band = Band::point(&field);
            let shape = band.fit(&field);
            scene
                .lay(&dt_kit::LayerSpec {
                    owner: ctx.path,
                    material: Material::Rock,
                    fluid: false,
                    mask: &mask,
                    shape: &shape,
                    grain: None,
                    skin_mm: 0.0,
                    round_mm: 0.0,
                    side: None,
                })
                .map_err(LookError::build)?;
        }
        scene.stamp_heights(&tops, dt_kit::Under::Earth);
        scene.set_rest(dt_kit::Rest {
            tops,
            flat,
            core: mask,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use dt_kit::{Band, Field, Frame, Scene, Seed};
    use std::sync::Arc;

    /// A flat band 20 mm up over the whole tile.
    fn flat(frame: &Frame) -> Band {
        let (nx, ny) = frame.nodes();
        let f = |z| Field::filled(nx, ny, frame.step_mm(), z);
        Band::new(f(20.0), f(20.0)).unwrap()
    }

    /// One plan from `cliff` for the frame, seed and band given.
    fn plan(cliff: &Cliff, frame: Frame, seed: u64, band: &Band) -> Arc<Planned> {
        let scene = Scene::new(frame, Seed::new(seed));
        let area = scene.whole();
        let ctx = Ctx {
            area: &area,
            path: "tile/t/cliff",
            seed: Seed::new(seed),
            along: None,
            at: None,
            printer: dt_kit::Printer::new(0.45, 0.2, 55.0, 0.2),
            against: None,
            foots: &[],
            band: Some(band),
            below: None,
            land_mm: 0.0,
            under: None,
            calm_mm: None,
            draft: false,
        };
        cliff.pile_plan(&frame, &ctx).unwrap()
    }

    #[test]
    fn the_pile_is_planned_once_for_what_decided_it() {
        let cliff = Cliff::new(&[]).unwrap();
        let frame = Frame::dungeonblocks(1, 1);
        let band = flat(&frame);
        let first = plan(&cliff, frame, 7, &band);
        // the same inputs: the kept plan itself, and it is what a fresh
        // plan comes to
        let again = plan(&cliff, frame, 7, &band);
        assert!(Arc::ptr_eq(&first, &again));
        let fresh = plan(&Cliff::new(&[]).unwrap(), frame, 7, &band);
        assert_eq!(first.slots_needed(), fresh.slots_needed());
        assert_eq!(first.foot.to_bits(), fresh.foot.to_bits());
        // each thing that decides the plan is in its key
        let other = plan(&cliff, frame, 8, &band);
        assert!(!Arc::ptr_eq(&first, &other), "the seed");
        let wider = Frame::dungeonblocks(2, 1);
        let moved = plan(&cliff, wider, 8, &flat(&wider));
        assert!(!Arc::ptr_eq(&other, &moved), "the frame");
        let mut ol = wider;
        ol.cells_per_square = 64;
        let coarse = plan(&cliff, ol, 8, &flat(&ol));
        assert!(!Arc::ptr_eq(&moved, &coarse), "the resolution");
        let high = {
            let (nx, ny) = frame.nodes();
            let f = |z| Field::filled(nx, ny, frame.step_mm(), z);
            Band::new(f(25.0), f(25.0)).unwrap()
        };
        let taller = plan(&cliff, frame, 8, &high);
        assert!(!Arc::ptr_eq(&other, &taller), "the band");
    }
}
