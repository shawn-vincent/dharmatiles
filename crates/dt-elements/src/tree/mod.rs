//! `tree`: a space-colonisation tree with a barked trunk, foliage clumps
//! and (from the leaf placer) separate leaves. Envelope, skeleton, wood and clumps
//! here and in dt-kit's [`graph`](dt_kit::graph); leaves through the
//! placer [`leaves`].
//!
//! 1. The crown envelope and its attractors ([`crown`]): sampled over its
//!    side surface by area, grouped about 20 mm across and bulged out.
//! 2. The skeleton: [`dt_kit::graph::colonise`], every attractor an end,
//!    every branch landing on its attractor, forks only in between, and no
//!    edge below the strict 26° (S1–S4, by construction); pipe radii.
//! 3. The wood: a C1 tube along every edge ([`dt_kit::graph::tubes`]),
//!    barked ([`bark`]), the trunk flared and sunk into the ground, all
//!    unioned into one closed volume.
//! 4. The clumps ([`clump`]): the swept rounded cones at the branch ends,
//!    noised and unioned without clipping their naturally rounded undersides.
//! 5. Leaves on the clumps ([`leaves`]: the organic union-surface placer).
//!
//! Sizes follow the system like a photocopy: every length here is
//! mm at the 35 mm square, times the frame's scale; bark and clump grain
//! keep true mm. Placement: `at=` (a range is drawn once from the
//! seed), or anywhere in the area with the trunk a crown radius from the
//! tile edge and clear of stamped solids.

pub mod bark;
pub mod clump;
pub mod crown;

mod leaf;
pub mod leaves;
mod site;

use dt_kit::Kind;
use dt_kit::graph::{self, Colonise, TubeStyle};
use dt_kit::{Material, Rng, Scene, Seed, Solid, Tag, TriMesh, form, union};
use rayon::prelude::*;

use crate::read;
use crate::{Control, Ctx, Element, LookError, Plan, Read, Value, Write, only};
use site::{crumbs, fill_pockets, ground_drop, place};
pub(crate) use site::{drop_crumbs, ground_box};

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

// The tree's fixed numbers, mm at the 35 mm square.

const SEGMENT_MM: f64 = 1.0;
const TARGET_FDM_DEG: f64 = 35.0;
/// S4: no branch edge below this, by construction.
pub const STRICT_FDM_DEG: f64 = 26.0;
/// In-between skeleton steps never climb flatter than this: 35° above
/// horizontal is the 55°-from-vertical overhang rule, so wood outside the
/// clumps prints. Only the landings on attractors (inside their clumps)
/// use the band down to the strict floor.
const CLIMB_DEG: f64 = 35.0;
const MAX_BRANCHES: usize = 3;
const SMOOTHING: f64 = 0.1;
const GROUP_MM: f64 = 20.0;
const BULGE_MM: f64 = 6.0;
const CLUSTER_LENGTH_MM: f64 = 10.5;
const HANDLE: f64 = 0.45;
const MIN_TUBE_RADIUS_MM: f64 = 0.42;
/// The thinnest branch tip a photocopy may shrink to (a printability
/// floor).
const TIP_FLOOR_MM: f64 = 0.6;
/// The trunk flares by this share of its radius at the ground, fading over
/// one radius up, and is sunk this far into the ground.
const FLARE: f64 = 0.45;
const SINK_MM: f64 = 1.0;
/// `crown base radius` is the crown radius less this (both library tiles).
const BASE_INSET_MM: f64 = 1.0;
/// Grass keeps out of a disc this many trunk radii across.
const STAMP_RADII: f64 = 1.8;
const STAMP_MIN_MM: f64 = 1.6;

/// What the tree's controls set (mm at the 35 mm square; ranges drawn at
/// build).
#[derive(Clone, Debug, PartialEq)]
struct Params {
    height: Value,
    trunk: Value,
    trunk_width: Option<Value>,
    width: Value,
    shape: crown::Shape,
    clumps: usize,
    clump_size: Value,
    split_deg: f64,
    eagerness: f64,
    target: f64,
    balance: f64,
    exponent: f64,
    min_radius: f64,
    leaves: bool,
    clumps_on: bool,
    leaf_length: Value,
    leaf_width: Value,
}

/// Where in the tree's stage the leaves' build lies (progress fractions).
const LEAVES_FROM: f64 = 0.5;
const LEAVES_TO: f64 = 0.61;

impl Default for Params {
    fn default() -> Self {
        Self {
            height: Value::Number(40.0),
            trunk: Value::Number(30.0),
            trunk_width: None,
            width: Value::Number(30.0),
            shape: crown::ROUND,
            clumps: 32,
            clump_size: Value::Number(5.5),
            split_deg: 30.0,
            eagerness: 0.8,
            target: 0.33,
            balance: 1.0,
            exponent: 3.0,
            min_radius: 1.0,
            // `leaves on` (dt-doc's default)
            leaves: true,
            clumps_on: true,
            leaf_length: Value::Number(4.5),
            leaf_width: Value::Number(3.0),
        }
    }
}

/// The `tree` element.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    p: Params,
}

/// `leaves`: whether there are leaves, and clumps.
fn leaves_word(c: &Control) -> Result<(bool, bool), LookError> {
    Ok(match read::word(c, &["on", "clumps-only", "bare"])? {
        "on" => (true, true),
        "clumps-only" => (false, true),
        _ => (false, false),
    })
}

/// A size, or a range of them, kept as written (drawn when the tree grows).
fn length(c: &Control, lo: f64, hi: f64) -> Result<Value, LookError> {
    read::span(c, lo, hi).map(|_| c.value.clone())
}

impl Tree {
    /// From its controls. A `branching` word sets the six branch knobs;
    /// an expert `branch/…` knob (parts files) overrides its word's number.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take or a value out of
    /// range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("tree", controls, CONTROLS)?;
        let mut p = Params::default();
        // the word's numbers first, then the expert knobs over them
        let mut ordered: Vec<&Control> = controls
            .iter()
            .filter(|c| !c.name.starts_with("branch/"))
            .collect();
        ordered.extend(controls.iter().filter(|c| c.name.starts_with("branch/")));
        for c in ordered {
            match c.name.as_str() {
                "height" => p.height = length(c, 10.0, 200.0)?,
                "trunk/height" => p.trunk = length(c, 0.0, 200.0)?,
                "trunk/width" => p.trunk_width = Some(length(c, 0.5, 40.0)?),
                "crown/width" => p.width = length(c, 2.0, 200.0)?,
                "crown/clump-size" => p.clump_size = length(c, 1.0, 20.0)?,
                "leaf/length" => p.leaf_length = length(c, 0.5, 20.0)?,
                "leaf/width" => p.leaf_width = length(c, 0.5, 20.0)?,
                "crown/clumps" => {
                    let v = read::number(c, 1.0, 400.0)
                        .map_err(|e| e.with_help("the library tiles use 32"))?;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    {
                        p.clumps = v.round() as usize;
                    }
                }
                "crown/shape.top-pointiness" => p.shape.top_pointiness = read::any_number(c)?,
                "crown/shape.top-curve" => p.shape.top_curve = read::any_number(c)?,
                "crown/shape.bottom-pointiness" => p.shape.bottom_pointiness = read::any_number(c)?,
                "crown/shape.bottom-curve" => p.shape.bottom_curve = read::any_number(c)?,
                "crown/branching.split" | "branch/split" => {
                    p.split_deg = read::any_number(c)?.clamp(1.0, 89.0);
                }
                "crown/branching.eagerness" | "branch/eagerness" => {
                    p.eagerness = read::any_number(c)?.clamp(0.0, 1.0);
                }
                "crown/branching.target" | "branch/target" => {
                    p.target = read::any_number(c)?.clamp(0.0, 1.0);
                }
                "crown/branching.balance" | "branch/balance" => {
                    p.balance = read::any_number(c)?.clamp(0.0, 1.0);
                }
                "crown/branching.exponent" | "branch/exponent" => {
                    p.exponent = read::any_number(c)?.clamp(1.0, 8.0);
                }
                "crown/branching.min-radius" | "branch/min-radius" => {
                    p.min_radius = read::any_number(c)?.clamp(0.2, 5.0);
                }
                "leaves.leaves" => p.leaves = read::any_number(c)? > 0.5,
                "leaves.clumps" => p.clumps_on = read::any_number(c)? > 0.5,
                "leaves" => (p.leaves, p.clumps_on) = leaves_word(c)?,
                _ => {}
            }
        }
        Ok(Self { p })
    }
}

fn draw(v: &Value, rng: &mut Rng) -> f64 {
    match *v {
        Value::Number(x) => x,
        Value::Range(a, b) => rng.uniform(a, b),
        Value::Word(_) => f64::NAN,
    }
}

/// A leaf's size, world mm (already scaled), for the leaf placer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeafSize {
    /// The largest leaf's length.
    pub length_mm: f64,
    /// Its width.
    pub width_mm: f64,
}

/// The printer line the leaf placer floors leaf size at (four lines).
const LINE_MM: f64 = 0.45;

/// What one tree came to, for the build record and the tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grown {
    pub attractors: usize,
    pub terminals: usize,
    pub nodes: usize,
    pub moved: usize,
    pub handles_shortened: usize,
    pub min_elevation_deg: f64,
    pub min_curve_elevation_deg: f64,
    pub trunk_radius_mm: f64,
    pub clumps: usize,
    pub wood_cut_mm3: f64,
    pub slivers: usize,
    pub leaves_placed: usize,
    /// Leaves the placer could not build (a degenerate frame): a warning,
    /// noted in the build record, never swallowed.
    pub leaves_failed_error: usize,
    pub leaves: usize,
}

/// One built tree: its solid and its numbers.
pub struct Built {
    pub solid: Solid,
    pub grown: Grown,
    pub at: [f64; 3],
    pub top_mm: f64,
    pub stamp_r_mm: f64,
}

impl Tree {
    /// Build one tree standing at `at` (world mm, on the ground).
    ///
    /// # Errors
    /// [`LookError`] when the crown is empty or the kernel fails.
    #[allow(clippy::too_many_lines)]
    pub fn grow(
        &self,
        scene: &mut Scene,
        seed: Seed,
        at: [f64; 3],
        printer: &dt_kit::Printer,
    ) -> Result<Built, LookError> {
        let p = &self.p;
        let stand_rule = dt_kit::Stand::for_printer(printer);
        let s = scene.frame().scale();
        let mut rng = Rng::new(seed.key("sizes"));
        let height = draw(&p.height, &mut rng) * s;
        let trunk = draw(&p.trunk, &mut rng) * s;
        let radius = 0.5 * draw(&p.width, &mut rng) * s;
        let clump_r = draw(&p.clump_size, &mut rng) * s;
        let leaf = LeafSize {
            length_mm: draw(&p.leaf_length, &mut rng) * s,
            width_mm: draw(&p.leaf_width, &mut rng) * s,
        };
        let trunk_width = p.trunk_width.as_ref().map(|v| draw(v, &mut rng) * s);
        if trunk >= height - 1e-6 {
            return Err(LookError::control(
                "trunk/height",
                format!(
                    "the trunk ({:.1} mm) is as tall as the tree ({:.1} mm): no crown",
                    trunk / s,
                    height / s
                ),
                "make trunk height less than height".into(),
            ));
        }
        let env = crown::Envelope::new(
            at[0],
            at[1],
            at[2],
            height,
            trunk,
            radius,
            (radius - BASE_INSET_MM * s).max(0.0),
            p.shape,
        );
        let mut crng = Rng::new(seed.key("crown"));
        let pts = crown::sample(&env, &mut crng, p.clumps);
        if pts.is_empty() {
            return Err(LookError::build("the crown has no surface to grow to"));
        }
        let g_mm = GROUP_MM * s;
        let k = crown::n_groups(&env, g_mm, g_mm);
        let labels = crown::groups(&pts, k, &mut crng, 1.0);
        let pts = crown::bulge(&pts, &labels, &env, BULGE_MM * s);
        let step = SEGMENT_MM * s;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let max_steps = (libm::ceil(height / step) as usize * 4).max(60);
        let traced = graph::colonise(
            at,
            &pts,
            (k >= 2).then_some(labels.as_slice()),
            &Colonise {
                step_mm: step,
                split_cos: p.eagerness * libm::cos(p.split_deg.to_radians()),
                aim_sin: libm::sin(TARGET_FDM_DEG.to_radians()),
                floor_deg: STRICT_FDM_DEG,
                climb_deg: CLIMB_DEG,
                max_branches: MAX_BRANCHES,
                alpha: SMOOTHING,
                max_steps,
                aim: p.target,
                balance: p.balance,
            },
        );
        let mut g = graph::simplify(&traced);
        let tip = (p.min_radius * s).max(TIP_FLOOR_MM);
        graph::pipe_radii(&mut g, p.exponent, tip);
        if let Some(w) = trunk_width {
            set_trunk_width(&mut g, w);
        }
        let ends: Vec<usize> = g
            .target
            .iter()
            .enumerate()
            .filter_map(|(i, t)| t.map(|_| i))
            .collect();
        let mut grown = Grown {
            attractors: pts.len(),
            terminals: ends.len(),
            nodes: g.nodes.len(),
            moved: traced.moved,
            min_elevation_deg: g.elevations().into_iter().fold(90.0, f64::min),
            trunk_radius_mm: g.radii[0],
            ..Grown::default()
        };

        // the trunk sinks into the ground (its foot below the lowest
        // ground under its flare), so the union welds it in
        let r0 = g.radii[0];
        let sink = SINK_MM + ground_drop(scene, at, r0 * (1.0 + FLARE));
        let mut wood_g = g.clone();
        wood_g.nodes[0][2] -= sink;
        let (curves, shortened) = graph::curves(&wood_g, HANDLE, STRICT_FDM_DEG);
        grown.handles_shortened = shortened;
        grown.min_curve_elevation_deg = curves
            .iter()
            .flatten()
            .map(graph::Curve::min_elevation)
            .fold(90.0, f64::min);
        let bark = bark::Bark::new(&wood_g, seed.key("bark").value(), at[2], height);
        let ground = at[2];
        let tubes = graph::tubes(
            &wood_g,
            &curves,
            &TubeStyle {
                step_mm: bark::STEP_MM,
                min_radius_mm: MIN_TUBE_RADIUS_MM,
                dome_rings: 4,
            },
            &|i| {
                if bark.barked(i) {
                    bark::SIDES
                } else {
                    bark::BARE_SIDES
                }
            },
            &|ring, angles, radii| {
                bark.surface(ring, angles, radii);
                if wood_g.parents[ring.edge] == Some(0) {
                    let h = (ring.centre[2] - ground).max(0.0);
                    let f = 1.0 + FLARE * libm::exp(-h / r0.max(1e-6));
                    radii.iter_mut().for_each(|r| *r *= f);
                }
            },
        );
        let tubes: Vec<TriMesh> = tubes
            .into_iter()
            .filter(|m| !m.triangles.is_empty())
            .collect();
        let n_tubes = u32::try_from(tubes.len()).map_err(LookError::build)?;
        let slots = scene
            .slots(n_tubes, Material::Wood)
            .map_err(LookError::build)?;
        let tube_solids: Vec<Solid> = tubes
            .par_iter()
            .enumerate()
            .map(|(k, m)| form::solid(slots.get(u32::try_from(k).unwrap_or(0)), m))
            .collect::<Result<_, _>>()
            .map_err(LookError::build)?;
        scene.progress().fraction("branches", 0.01);
        let wood = union(&tube_solids).map_err(LookError::build)?;
        scene.progress().fraction("wood", 0.03);
        let (wood, slivers) = crumbs(scene, wood, Material::Wood)?;
        grown.slivers += slivers;
        // the landings dip to the strict floor inside their clumps; where a
        // landing's underside shows below its clump it would hang, so the
        // wood too stands on the ground (a slab under the root)
        let footing = ground_box(scene, at, r0 * 4.0, ground - 40.0, ground)?;
        let (wood, stood_wood) = dt_kit::stand(scene, &wood, &footing, Material::Wood, &stand_rule)
            .map_err(LookError::build)?;
        grown.wood_cut_mm3 = stood_wood.cut_mm3;
        let (wood, slivers) = crumbs(scene, wood, Material::Wood)?;
        grown.slivers += slivers;
        let mut parts = vec![wood.clone()];

        if p.clumps_on {
            let length = CLUSTER_LENGTH_MM * s;
            let specs: Vec<clump::Spec> = ends
                .iter()
                .filter_map(|&i| {
                    let c = curves[i]?;
                    let (start, start_t, len) = clump::start_on(&c, length);
                    let r_wood = g.radii[i].max(MIN_TUBE_RADIUS_MM);
                    Some(clump::Spec {
                        tip: g.nodes[i],
                        tip_tangent: c.tangent(1.0),
                        start,
                        start_tangent: start_t,
                        r_wood,
                        r_foliage: clump_r,
                        length: len,
                        key: seed.key("clump").index(i as u64).value(),
                    })
                })
                .collect();
            let meshes: Vec<TriMesh> = specs.par_iter().map(clump::mesh).collect();
            let n = u32::try_from(specs.len()).map_err(LookError::build)?;
            let cslots = scene.slots(n, Material::Leaf).map_err(LookError::build)?;
            let solids: Vec<Solid> = meshes
                .par_iter()
                .enumerate()
                .map(|(k, m)| form::solid(cslots.get(u32::try_from(k).unwrap_or(0)), m))
                .collect::<Result<_, _>>()
                .map_err(LookError::build)?;
            grown.clumps = solids.len();
            scene.progress().fraction("clumps", 0.26);
            let crown_solid = union(&solids).map_err(LookError::build)?;
            let (crown_solid, slivers) = crumbs(scene, crown_solid, Material::Leaf)?;
            grown.slivers += slivers;
            scene.progress().fraction("crown", LEAVES_FROM);
            if p.leaves {
                let shape = leaves::LeafShape {
                    length_mm: leaf.length_mm / s,
                    width_mm: leaf.width_mm / s,
                    overhang_deg: printer.overhang_deg,
                    ..leaves::LeafShape::default()
                }
                .scaled(s, LINE_MM);
                let (placed, lstats) = leaves::place_progress(
                    &crown_solid.mesh(),
                    &wood.mesh(),
                    &shape,
                    seed.key("leaves"),
                    &scene.progress(),
                    (LEAVES_FROM, LEAVES_TO),
                )?;
                grown.leaves_failed_error = lstats.failed_error;
                let n = u32::try_from(placed.len()).map_err(LookError::build)?;
                if n > 0 {
                    let lslots = scene.slots(n, Material::Leaf).map_err(LookError::build)?;
                    let solid_ticks =
                        scene
                            .progress()
                            .counter("leaf solids", placed.len(), LEAVES_TO, 0.75);
                    let ls: Vec<Solid> = placed
                        .par_iter()
                        .enumerate()
                        .map(|(k, m)| {
                            let s = form::solid(lslots.get(u32::try_from(k).unwrap_or(0)), m);
                            solid_ticks.tick();
                            s
                        })
                        .collect::<Result<_, _>>()
                        .map_err(LookError::build)?;
                    grown.leaves_placed = placed.len();
                    grown.leaves = ls.len();
                    // the leaves go into the tree's one union with the wood
                    // and the crown (a union of their own first costs a
                    // second pass over every leaf)
                    parts.extend(ls);
                }
            }
            parts.push(crown_solid);
        }
        let solid = union(&parts).map_err(LookError::build)?;
        let (solid, pockets) = fill_pockets(scene, solid)?;
        grown.slivers += pockets;
        let top_mm = solid.top().unwrap_or(at[2] + height);
        Ok(Built {
            solid,
            grown,
            at,
            top_mm,
            stamp_r_mm: (r0 * STAMP_RADII).max(STAMP_MIN_MM * s),
        })
    }
}

/// `trunk width`: the trunk is `w` across at the ground and tapers
/// (by height) to its pipe-model radius at the first fork; the branches
/// keep theirs. Scaling every radius instead fattened each branch by the
/// same factor, so a wide trunk hung slabs of wood off the crown.
fn set_trunk_width(g: &mut graph::Graph, w: f64) {
    let ch = g.children();
    let mut chain = vec![0];
    while let [only] = ch[*chain.last().unwrap_or(&0)].as_slice() {
        chain.push(*only);
    }
    let fork = *chain.last().unwrap_or(&0);
    let (z0, z1) = (g.nodes[0][2], g.nodes[fork][2]);
    let r_fork = g.radii[fork];
    let r0 = (0.5 * w).max(TIP_FLOOR_MM);
    for &i in &chain {
        let t = if z1 - z0 > 1e-9 {
            ((g.nodes[i][2] - z0) / (z1 - z0)).clamp(0.0, 1.0)
        } else {
            1.0
        };
        g.radii[i] = if i == 0 { r0 } else { r0 + (r_fork - r0) * t };
    }
}

impl Element for Tree {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Tree),
            reads: vec![
                Read::Solids(Kind::Ground),
                Read::IfAny(Kind::Wall),
                Read::IfAny(Kind::Floor),
            ],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let s = scene.frame().scale();
        // the crown radius as `grow` draws it (the same stream)
        let mut rng = Rng::new(ctx.seed.key("sizes"));
        let _ = draw(&self.p.height, &mut rng);
        let _ = draw(&self.p.trunk, &mut rng);
        let mut margin = 0.5 * draw(&self.p.width, &mut rng) * s;
        let _ = draw(&self.p.clump_size, &mut rng);
        let _ = draw(&self.p.leaf_length, &mut rng);
        let _ = draw(&self.p.leaf_width, &mut rng);
        if let Some(v) = &self.p.trunk_width {
            // a set trunk width: its flared foot must stand on the tile
            // too (it would hang past the edge), and must fit on it
            let foot = 0.5 * draw(v, &mut rng) * s * (1.0 + FLARE);
            let [w, h] = scene.frame().size_mm();
            if 2.0 * foot > w.min(h) {
                return Err(LookError::control(
                    "trunk/width",
                    format!(
                        "a trunk {:.1} mm wide flares to {:.1} mm at the ground, wider than the tile",
                        2.0 * foot / s / (1.0 + FLARE),
                        2.0 * foot / s
                    ),
                    format!(
                        "make trunk width at most {:.1} mm on this tile",
                        w.min(h) / s / (1.0 + FLARE)
                    ),
                ));
            }
            margin = margin.max(foot);
        }
        let at = place(scene, ctx, margin, 3.0 * s)?;
        let b = self.grow(scene, ctx.seed, at, &ctx.printer)?;
        let gr = &b.grown;
        #[allow(clippy::cast_precision_loss)]
        for (k, v) in [
            ("tree.attractors", gr.attractors as f64),
            ("tree.terminals", gr.terminals as f64),
            ("tree.nodes", gr.nodes as f64),
            ("tree.attractors_moved", gr.moved as f64),
            ("tree.handles_shortened", gr.handles_shortened as f64),
            ("tree.min_branch_elevation_deg", gr.min_elevation_deg),
            ("tree.min_curve_elevation_deg", gr.min_curve_elevation_deg),
            ("tree.trunk_radius_mm", gr.trunk_radius_mm),
            ("tree.clumps", gr.clumps as f64),
            ("tree.wood_cut_mm3", gr.wood_cut_mm3),
            ("tree.slivers_dropped", gr.slivers as f64),
            ("tree.leaves_placed", gr.leaves_placed as f64),
            ("tree.leaves", gr.leaves as f64),
            ("tree.leaves_failed_error", gr.leaves_failed_error as f64),
            ("tree.x_mm", b.at[0]),
            ("tree.y_mm", b.at[1]),
            ("tree.height_mm", b.top_mm - b.at[2]),
        ] {
            scene.note(k, v);
        }
        // the trunk's footprint: grass keeps out, things stand clear
        let frame = *scene.frame();
        let (nx, ny) = frame.nodes();
        let step = frame.step_mm();
        let mut mask = dt_kit::Mask::new(nx, ny, false);
        for (k, bit) in mask.bits.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = ((k % nx) as f64 * step, (k / nx) as f64 * step);
            *bit = libm::hypot(x - b.at[0], y - b.at[1]) <= b.stamp_r_mm;
        }
        // stamped at its foot, not its crown top: the support is what
        // later things stand on, and nothing stands on a tree (a second
        // tree there would be planted in this one's crown)
        scene.stamp(&mask, b.at[2]);
        scene.add(
            Tag {
                kind: Kind::Tree,
                owner: ctx.path.to_owned(),
            },
            b.solid,
        );
        Ok(())
    }
}
