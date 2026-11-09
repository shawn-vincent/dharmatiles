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
mod relief;
mod stack;
mod stone;
mod util;

use dt_kit::Kind;
use dt_kit::{Band, Field as Lattice, Material, OnFrame, Rng, Scene, Support, Tag};
use rayon::prelude::*;

use crate::{Control, Ctx, Element, LookError, Plan, Value, Write};
use stone::{ColState, Env};

type ColResult = Result<(Vec<(usize, dt_kit::Piece)>, usize), dt_kit::KitError>;

/// The most stones a pile may be made of, per square of its area.
const ROCKS_PER_SQUARE: usize = 400;

/// The cliff element.
#[derive(Clone, Debug)]
pub struct Cliff {
    weights: Option<[f64; 5]>,
    /// stone-size multiplier relative to the form's own (massive = 1).
    size: f64,
    patches: Option<f64>,
    patch_width: Option<(f64, f64)>,
    ledges: u32,
    ledge_height: Option<f64>,
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
            weights: None,
            size: 1.0 / 1.35,
            patches: None,
            patch_width: None,
            ledges: 1,
            ledge_height: None,
            plan: Memo::default(),
        };
        let mut w = [f64::NAN; 5];
        for k in controls {
            match (k.name.as_str(), &k.value) {
                ("stone-size", Value::Word(s)) => {
                    c.size = match s.as_str() {
                        "rubble" => 1.0 / 1.35,
                        "broken" => 1.15 / 1.35,
                        _ => 1.0,
                    };
                }
                ("stone-size", Value::Number(v)) => {
                    c.size = if *v >= 1.0 {
                        *v / 1.35
                    } else {
                        (1.0 + 0.35 * v.clamp(0.0, 1.0)) / 1.35
                    }
                }
                ("rock", Value::Word(s)) => {
                    if let Some(i) = KIND_NAMES.iter().position(|n| n == s) {
                        w = [0.0; 5];
                        w[i] = 1.0;
                    }
                }
                (n, Value::Number(v)) if n.starts_with("rock.") => {
                    if let Some(i) = KIND_NAMES.iter().position(|k| n.ends_with(k)) {
                        w[i] = v.max(0.0);
                    }
                }
                ("ledges/height", Value::Number(v)) => c.ledge_height = Some(*v),
                ("ledges", Value::Number(v)) => c.ledges = v.clamp(0.0, 2.0).round() as u32,
                ("shattered/patches", Value::Number(v)) => c.patches = Some(*v),
                ("shattered/width", Value::Number(v)) => c.patch_width = Some((*v, *v)),
                ("shattered/width", Value::Range(a, b)) => c.patch_width = Some((*a, *b)),
                _ => {}
            }
        }
        if w.iter().any(|x| !x.is_nan()) {
            let w: [f64; 5] = w.map(|x| if x.is_nan() { 0.0 } else { x });
            if w.iter().sum::<f64>() > 0.0 {
                c.weights = Some(w);
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
        let planned = std::sync::Arc::new(self.plan_pile(frame, ctx)?);
        *kept = Some((key, planned.clone()));
        Ok(planned)
    }

    /// Everything the plan reads: the frame, the seed, the land, the area and
    /// the band, hashed.
    fn plan_key(frame: &dt_kit::Frame, ctx: &Ctx<'_>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        format!("{frame:?}").hash(&mut h);
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

    fn plan_pile(&self, frame: &dt_kit::Frame, ctx: &Ctx<'_>) -> Result<Planned, LookError> {
        let s = frame.scale();
        let sq = frame.square_mm;
        let Some(band) = ctx.band else {
            return Err(LookError::general(
                "this cliff has no solved top",
                "write the tile in the heights language: `cliff top=\"dirtEdgeHeight + 35\"`",
            ));
        };
        // the heaved shape in the band: a plane for one height
        let (shape, _) = crate::floor::heave(frame, ctx.seed, band);
        let foot = ctx.land_mm;
        let mut rng = Rng::new(ctx.seed.key("cliff"));
        let fm = form::make_form(
            &mut rng,
            &form::Ask {
                frame,
                mask: ctx.area,
                band,
                shape: &shape,
                foot,
                mass_gain: self.size,
                kinds: self.weights,
                ledges: self.ledges,
                ledge_height: self.ledge_height,
                s,
            },
        );
        let zone = form::Zones::new(&mut rng, fm.w, s, self.patches, self.patch_width);
        let max_rocks = ROCKS_PER_SQUARE * ((fm.w * fm.h / (sq * sq)).ceil() as usize).max(1);
        let fit = fit::Fit::new(band, frame, ctx.area, foot);
        let mut st = stack::stack(&mut rng, &fm, &zone, s, max_rocks, &fit)
            .map_err(|e| LookError::band(&e, "the cliff's stones"))?;
        stack::reconcile(&mut st.rocks, &fm, &mut st.s);
        Ok(Planned {
            fm,
            zone,
            st,
            rng,
            foot,
        })
    }
}

struct Planned {
    fm: form::Form,
    zone: form::Zones,
    st: stack::Stacked,
    rng: Rng,
    foot: f64,
}

/// The node of `frame`'s lattice nearest the cell `(ci, cj)` of the pile's
/// grid, counted by cell centre.
pub(super) fn node_cell(x: f64, y: f64, nx: usize, ny: usize) -> Option<usize> {
    let ci = (x / form::GRID_MM).floor();
    let cj = (y / form::GRID_MM).floor();
    (ci >= 0.0 && cj >= 0.0 && (ci as usize) < nx && (cj as usize) < ny)
        .then(|| cj as usize * nx + ci as usize)
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
                if let Some(c) = node_cell(x, y, st.s.nx, st.s.ny) {
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
        let (fm, zone, st, foot) = (&planned.fm, &planned.zone, &planned.st, planned.foot);
        let mut rng = planned.rng.clone();
        let Some(band) = ctx.band else {
            return Err(LookError::general("this cliff has no solved top", ""));
        };
        progress.fraction("planning the pile", 0.01);
        let placed = &st.rocks;
        if placed.is_empty() {
            return Ok(());
        }
        let mut cols = finish::columns(&mut rng, placed, fm, &st.s, zone);
        let nslots = placed.len() as u32 + 3;
        let slots = scene
            .slots(nslots, Material::Rock)
            .map_err(LookError::build)?;
        let plan_slot = slots.get(0);
        // columns that carry another column's stone stand where they are
        for c in &mut cols {
            let cpoly = &c.poly;
            for (ri, r) in placed.iter().enumerate() {
                if c.stones.contains(&ri) || r.z0 <= c.zlo + 1.0 {
                    continue;
                }
                let rp = r.poly();
                if shared_area_mm2(&rp, cpoly) > 1.0 {
                    c.carries = true;
                    c.back.iter_mut().for_each(|b| b.1 = 0.0);
                    break;
                }
            }
        }
        for c in &mut cols {
            finish::plan_column(&mut rng, c, placed, fm, plan_slot)
                .map_err(|e| LookError::build(format!("plan column: {e}")))?;
        }
        progress.fraction("dressing columns", 0.03);
        let core_field = core::core_field(&st.s, &cols, placed);
        let core_mesh = core::core_mesh(&core_field)
            .map_err(|e| LookError::build(format!("core mesh: {e}")))?;
        let m = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, foot];
        if progress.sketching() {
            let rough = rough_pile(&core_mesh, placed, foot);
            progress.sketch(&rough, &vec![Material::Rock; rough.triangles.len()], true);
        }
        let mut frng = Rng::new(ctx.seed.key("cliff-faces"));
        faces::plan_faces(&mut frng, &mut cols, placed);
        // support cones of resting stones, for the cleaving guard
        let cones: Vec<Option<dt_kit::TriMesh>> = placed
            .iter()
            .map(|r| {
                if r.z0 > 0.0 {
                    finish::support_cone(r)
                } else {
                    None
                }
            })
            .collect();
        let core_support =
            Support::new(std::slice::from_ref(&core_mesh)).map_err(LookError::build)?;
        let env = Env {
            form: fm,
            s: &st.s,
            placed,
            seed: ctx.seed.key("cliff-grain").value(),
            draft: ctx.draft,
            high: band.hi(),
            foot,
        };
        let col_seed = ctx.seed.key("cliff-columns");
        progress.fraction("planning faces and cracks", 0.04);
        let ticks = progress.counter("stones", placed.len(), 0.04, 0.85);
        let results: Vec<ColResult> = cols
            .par_iter()
            .enumerate()
            .map(|(ci, c)| {
                let mut rng = Rng::new(col_seed.index(ci as u64));
                let mut crng = Rng::new(col_seed.key("chips").index(ci as u64));
                let b = util::bounds(&c.poly);
                let near: Vec<dt_kit::TriMesh> = cones
                    .iter()
                    .enumerate()
                    .filter_map(|(k, m)| {
                        let m = m.as_ref()?;
                        if c.stones.contains(&k) {
                            return None;
                        }
                        let pb = util::bounds(&placed[k].poly());
                        (pb[0] < b[2] + 3.0
                            && pb[2] > b[0] - 3.0
                            && pb[1] < b[3] + 3.0
                            && pb[3] > b[1] - 3.0)
                            .then(|| m.clone())
                    })
                    .collect();
                let cone_support = Support::new(&near)
                    .map_err(|e| dt_kit::KitError::Shape(format!("cone support: {e}")))?;
                let sup = [&cone_support, &core_support];
                let mut cs = ColState::default();
                let mut out = Vec::new();
                for &ri in c.stones.iter().rev() {
                    let slot = slots.get(2 + ri as u32);
                    if let Some(p) =
                        stone::rock_piece(&mut rng, &mut crng, ri, c, &mut cs, &env, &sup, slot)?
                    {
                        out.push((ri, p));
                    }
                    ticks.tick();
                }
                Ok((out, cs.tops))
            })
            .collect();
        progress.fraction("stones finished", 0.85);
        let mut pieces: Vec<(usize, dt_kit::Piece)> = Vec::new();
        let mut tops = 0;
        for r in results {
            let (v, t) = r.map_err(LookError::build)?;
            pieces.extend(v);
            tops += t;
        }
        pieces.sort_by_key(|p| p.0);
        // a stone the band holds down at its site adds no height there, so the
        // stack can draw the same one again and again: the same cut stone,
        // solids that coincide (the same size and place to 0.05 mm), which a union of the pile never
        // finishes. Only the first of each is kept; the rest were the same rock.
        let mut seen = std::collections::HashSet::new();
        let stacked = pieces.len();
        pieces.retain(|(_, p)| {
            let m = p.mesh();
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for v in &m.positions {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            let key = |x: f64| (x * 20.0).round() as i64;
            seen.insert((m.triangles.len(), lo.map(key), hi.map(key)))
        });
        let repeated = stacked - pieces.len();
        // a stone that stands over the band is a bug in the body
        let fit = dt_kit::pieces::Pieces::new(band, &frame);
        // a draft carves on grids `DRAFT_COARSE` times the print's: its stones
        // may stand that far over the band's high, and are never written
        let band_tol = if ctx.draft {
            stone::DRAFT_COARSE * faces::RELIEF_GRID_MM
        } else {
            dt_kit::pieces::BAND_TOL_MM
        };
        for (_, p) in &pieces {
            let mut mesh = p.mesh();
            for v in &mut mesh.positions {
                v[2] += foot;
            }
            fit.admits_within(&mesh, band_tol)
                .map_err(|e| LookError::band(&e, "a cliff stone"))?;
        }
        progress.fraction("laying the core", 0.90);
        let meshes: Vec<dt_kit::rays::RayMesh> = pieces
            .iter()
            .map(|(_, p)| dt_kit::rays::RayMesh::new(&p.mesh()))
            .collect();
        Self::lay_core(scene, ctx, fm, &core_field, &st.s, foot, &meshes)?;
        progress.fraction("placing stones", 0.98);
        let n_stones = pieces.len();
        for (_, p) in pieces {
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
        scene.note("cliff_repeated_stones", repeated as f64);
        scene.note("cliff_top_cracks", tops as f64);
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
        core_field: &util::Field,
        s: &util::Field,
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
            let field = Lattice::new(&frame, 0.0).with_z(z);
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

/// The pile as boxes: the core and each rock's footprint stood from its
/// underside to its top, raised to stand on `foot`.  A stand-in for the
/// view while the stones are cut, never printed.
fn rough_pile(core: &dt_kit::TriMesh, placed: &[stack::Placed], foot: f64) -> dt_kit::TriMesh {
    let mut out = core.clone();
    out.extra.clear();
    out.num_extra = 0;
    out.merge_from.clear();
    out.merge_to.clear();
    for r in placed {
        let poly = r.poly();
        if poly.len() < 3 {
            continue;
        }
        let top = poly
            .iter()
            .map(|p| r.top_z(p[0], p[1]))
            .fold(f64::NEG_INFINITY, f64::max);
        if top <= r.z0 {
            continue;
        }
        let b = dt_kit::prism_mesh(&[poly], r.z0, top);
        let off = out.positions.len() as u32;
        out.positions.extend(&b.positions);
        out.triangles
            .extend(b.triangles.iter().map(|t| t.map(|v| v + off)));
    }
    for p in &mut out.positions {
        p[2] += foot;
    }
    out
}

/// The area two outlines share, mm², estimated: the points of a 0.5 mm grid
/// over the overlap of their bounds that are inside both, each counted as
/// 0.25 mm².
fn shared_area_mm2(a: &[util::P2], b: &[util::P2]) -> f64 {
    let (ba, bb) = (util::bounds(a), util::bounds(b));
    let (x0, y0, x1, y1) = (
        ba[0].max(bb[0]),
        ba[1].max(bb[1]),
        ba[2].min(bb[2]),
        ba[3].min(bb[3]),
    );
    if x0 >= x1 || y0 >= y1 {
        return 0.0;
    }
    let mut n = 0.0;
    let mut x = x0 + 0.25;
    while x < x1 {
        let mut y = y0 + 0.25;
        while y < y1 {
            if util::point_in(a, [x, y]) && util::point_in(b, [x, y]) {
                n += 0.25;
            }
            y += 0.5;
        }
        x += 0.5;
    }
    n
}
