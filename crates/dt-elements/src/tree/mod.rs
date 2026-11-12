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

use crate::read::Settings;
use crate::{Control, Ctx, Degrees, Draw, Element, LookError, Plan, Read, Write};
use site::{crumbs, fill_pockets, ground_at, ground_drop, place, scatter_sites};
pub(crate) use site::{drop_crumbs, ground_box};

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
/// How far a clump and its leaves stand above the branch tip it sits on, as
/// the clump's radius times this and a few mm besides: the crown's tips are
/// held this far under the cloud.
const CAP_CLUMPS: f64 = 1.6;
const CAP_MM: f64 = 1.0;
/// How wide the smooth roll of the tips is, in crown radii: tips nearer
/// than this roll alike.
const ROLL_RADII: f64 = 1.0;
/// `crown base radius` is the crown radius less this (both library tiles).
const BASE_INSET_MM: f64 = 1.0;
/// Grass keeps out of a disc this many trunk radii across.
const STAMP_RADII: f64 = 1.8;
const STAMP_MIN_MM: f64 = 1.6;

/// What the tree's controls set (mm at the 35 mm square; ranges drawn at
/// build).
#[derive(Clone, Debug, PartialEq)]
struct Params {
    /// Its height over its foot (`top="bottom + 40"`, and the default).
    height: Draw,
    /// Its `top=`: the formula (a cloud over its tips) the tile worked out,
    /// by the entry that owns it; `None` stands the tree `height` over its
    /// foot.
    crown: Option<crate::Formula>,
    trunk: Draw,
    trunk_width: Option<Draw>,
    width: Draw,
    shape: crown::Shape,
    clumps: usize,
    clump_size: Draw,
    split: Degrees,
    eagerness: f64,
    target: f64,
    balance: f64,
    exponent: f64,
    min_radius: f64,
    leaves: bool,
    clumps_on: bool,
    leaf_length: Draw,
    leaf_width: Draw,
}

/// A tree's crown diameter when none is written, mm at the 35 mm square:
/// the crown's radius is half of it.  `check` makes the tree's region, a
/// disc, from the crown width a tree will have.
pub const DEFAULT_CROWN_WIDTH_MM: f64 = 30.0;

/// The heights a tree may stand over its foot, mm at the 35 mm square.
const MIN_HEIGHT_MM: f64 = 20.0;
const MAX_HEIGHT_MM: f64 = 120.0;

/// Where in the tree's stage the leaves' build lies (progress fractions).
const LEAVES_FROM: f64 = 0.5;
const LEAVES_TO: f64 = 0.61;

impl Default for Params {
    fn default() -> Self {
        Self {
            height: Draw::flat(40.0, 40.0),
            crown: None,
            trunk: Draw::flat(30.0, 30.0),
            trunk_width: None,
            width: Draw::flat(DEFAULT_CROWN_WIDTH_MM, DEFAULT_CROWN_WIDTH_MM),
            shape: crown::ROUND,
            clumps: 32,
            clump_size: Draw::flat(5.5, 5.5),
            split: Degrees(30.0),
            eagerness: 0.8,
            target: 0.33,
            balance: 1.0,
            exponent: 3.0,
            min_radius: 1.0,
            // `leaves on` (dt-doc's default)
            leaves: true,
            clumps_on: true,
            leaf_length: Draw::flat(4.5, 4.5),
            leaf_width: Draw::flat(3.0, 3.0),
        }
    }
}

/// The `tree` element.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    p: Params,
}

/// One of the branch knobs: the expert control (a parts file's) over the
/// number the `branching` word stands for.  Both names are the schema's;
/// a test checks they are.
pub struct Knob {
    /// `branch/<knob>`.
    pub expert: &'static str,
    /// `crown/branching.<knob>`.
    pub word: &'static str,
}

const SPLIT: Knob = Knob {
    expert: "branch/split",
    word: "crown/branching.split",
};
const EAGERNESS: Knob = Knob {
    expert: "branch/eagerness",
    word: "crown/branching.eagerness",
};
const TARGET: Knob = Knob {
    expert: "branch/target",
    word: "crown/branching.target",
};
const BALANCE: Knob = Knob {
    expert: "branch/balance",
    word: "crown/branching.balance",
};
const EXPONENT: Knob = Knob {
    expert: "branch/exponent",
    word: "crown/branching.exponent",
};
const MIN_RADIUS: Knob = Knob {
    expert: "branch/minRadius",
    word: "crown/branching.minRadius",
};

/// All six, for the schema test.
pub const KNOBS: [&Knob; 6] = [
    &SPLIT,
    &EAGERNESS,
    &TARGET,
    &BALANCE,
    &EXPONENT,
    &MIN_RADIUS,
];

/// `leaves`: whether there are leaves, and clumps.
fn leaves_word(w: &str) -> (bool, bool) {
    match w {
        "on" => (true, true),
        "clumps-only" => (false, true),
        _ => (false, false),
    }
}

impl Tree {
    /// From its controls. A `branching` word sets the six branch knobs;
    /// an expert `branch/…` knob (parts files) overrides its word's number.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take or a value out of
    /// range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        let mut p = Params::default();
        let s = Settings::new(controls);
        // the stack writes a tree's `top=` as one of these two
        p.height = s.dist_or("height", p.height)?;
        if let Some(c) = s.control("top") {
            match &c.value {
                crate::Value::Formula(f) => p.crown = Some(f.clone()),
                _ => {
                    return Err(LookError::control(
                        "top",
                        "a tree's top is a formula the tile works out at each branch tip".into(),
                        "write `top=\"$bottom + 40dmm\"`".into(),
                    ));
                }
            }
        }
        p.trunk = s.dist_or("trunk/height", p.trunk)?;
        p.trunk_width = s.dist("trunk/width")?.or(p.trunk_width);
        p.width = s.dist_or("crown/width", p.width)?;
        p.clump_size = s.dist_or("crown/clumpSize", p.clump_size)?;
        p.leaf_length = s.dist_or("leaf/length", p.leaf_length)?;
        p.leaf_width = s.dist_or("leaf/width", p.leaf_width)?;
        if let Some(v) = s.number("crown/clumps")? {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                p.clumps = v.round() as usize;
            }
        }
        let sh = &mut p.shape;
        sh.top_pointiness = s.number_or("crown/shape.topPointiness", sh.top_pointiness)?;
        sh.top_curve = s.number_or("crown/shape.topCurve", sh.top_curve)?;
        sh.bottom_pointiness = s.number_or("crown/shape.bottomPointiness", sh.bottom_pointiness)?;
        sh.bottom_curve = s.number_or("crown/shape.bottomCurve", sh.bottom_curve)?;
        // the branching word's numbers, and the expert `branch/…` knobs
        // over them (a parts file's)
        let knob = |k: &Knob, now: f64| -> Result<f64, LookError> {
            Ok(s.number(k.expert)?.or(s.number(k.word)?).unwrap_or(now))
        };
        p.split = s
            .degrees(SPLIT.expert)?
            .or(s.degrees(SPLIT.word)?)
            .unwrap_or(p.split);
        p.eagerness = knob(&EAGERNESS, p.eagerness)?;
        p.target = knob(&TARGET, p.target)?;
        p.balance = knob(&BALANCE, p.balance)?;
        p.exponent = knob(&EXPONENT, p.exponent)?;
        p.min_radius = knob(&MIN_RADIUS, p.min_radius)?;
        if let Some(w) = s.word("leaves")? {
            (p.leaves, p.clumps_on) = leaves_word(w);
        }
        Ok(Self { p })
    }
}

/// A weighted range's value: a range is drawn (one draw, with its odds), a
/// number never draws.
fn draw_dist(v: &Draw, rng: &mut Rng) -> f64 {
    if v.is_point() { v.lo() } else { v.draw(rng) }
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
    /// Leaves whose lying normal could not be walked on the crown: built
    /// flush instead, noted in the build record.
    pub leaves_lying_undraped: usize,
    pub leaves: usize,
}

/// One built tree: its solid and its numbers.
pub struct Built {
    pub solid: Solid,
    pub grown: Grown,
    pub at: [f64; 3],
    pub top_mm: f64,
    pub stamp_r_mm: f64,
    /// The cloud at each branch tip, for the debug layer (empty with it off).
    pub clouds: Vec<dt_kit::debug::Column>,
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
        crowns: Option<&dyn crate::CrownTops>,
    ) -> Result<Built, LookError> {
        let p = &self.p;
        let stand_rule = dt_kit::Stand::for_printer(printer);
        let s = scene.frame().scale();
        let mut rng = Rng::new(seed.key("sizes"));
        // no `top=`: the tree stands `height` over its foot
        let flat_top = p
            .crown
            .is_none()
            .then(|| at[2] / s + draw_dist(&p.height, &mut rng));
        let trunk = draw_dist(&p.trunk, &mut rng) * s;
        let radius = 0.5 * draw_dist(&p.width, &mut rng) * s;
        let clump_r = draw_dist(&p.clump_size, &mut rng) * s;
        let leaf = LeafSize {
            length_mm: draw_dist(&p.leaf_length, &mut rng) * s,
            width_mm: draw_dist(&p.leaf_width, &mut rng) * s,
        };
        let trunk_width = p.trunk_width.as_ref().map(|v| draw_dist(v, &mut rng) * s);
        let frame = *scene.frame();
        let tip_at = |x: f64, y: f64| dt_kit_tip(&frame, at, [x, y], radius);
        // the crown's cloud at each tip, one formula for the tree
        let cloud_at = |x: f64, y: f64| -> Result<crate::TopCloud, LookError> {
            match (&p.crown, crowns, flat_top) {
                (Some(f), Some(c), _) => c.cloud(f, &tip_at(x, y)),
                (Some(_), None, _) => Err(LookError::build(
                    "a tree's top is worked out by the tile it is built in",
                )),
                (None, _, Some(t)) => Ok(crate::TopCloud {
                    lo: t,
                    hi: t,
                    odds: None,
                }),
                (None, _, None) => Err(LookError::build("a tree has neither a top nor a height")),
            }
        };
        // the middle of the cloud at the trunk is the tree's height: what
        // the crown's envelope is measured on and what `top` must allow
        let c0 = cloud_at(at[0], at[1])?;
        let top_here = c0.lo + (c0.hi - c0.lo) * c0.odds.as_ref().map_or(0.5, |w| w.unit(0.5));
        let height = top_here * s - at[2];
        let (min_h, max_h) = (MIN_HEIGHT_MM * s, MAX_HEIGHT_MM * s);
        if !(min_h..=max_h).contains(&height) {
            return Err(LookError::control(
                "top",
                format!(
                    "this top is {:.1} mm over the ground at the tree's foot ({:.1} mm up); a tree is {MIN_HEIGHT_MM} to {MAX_HEIGHT_MM} mm tall",
                    height / s,
                    at[2] / s
                ),
                "write the height over its foot: `$bottom + 40dmm`".into(),
            ));
        }
        // the clumps and leaves stand above the tips: the tips are held
        // that far under the cloud so the crown stays under its high
        let cap = (CAP_CLUMPS * clump_r + CAP_MM * s).max(0.0);
        let env_height = height - cap;
        if trunk >= env_height - 1e-6 {
            return Err(LookError::control(
                "trunk/height",
                format!(
                    "the trunk ({:.1} mm) leaves no room for a crown under the top ({:.1} mm over the foot): the crown's tips stand {:.1} mm under it so its clumps stay below",
                    trunk / s,
                    height / s,
                    cap / s
                ),
                "make trunk height less, or the top higher".into(),
            ));
        }
        let env = crown::Envelope::new(
            at[0],
            at[1],
            at[2],
            env_height,
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
        // each tip rolls smoothly inside the cloud over it and the crown is
        // stretched or squeezed to meet it
        let mut clouds = scene.collecting().clouds.then(Vec::new);
        let pts = under_the_cloud(&pts, &env, cap, seed, radius, s, &cloud_at, clouds.as_mut())?;
        let step = SEGMENT_MM * s;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let max_steps = (libm::ceil(height / step) as usize * 4).max(60);
        let traced = graph::colonise(
            at,
            &pts,
            (k >= 2).then_some(labels.as_slice()),
            &Colonise {
                step_mm: step,
                split_cos: p.eagerness * libm::cos(p.split.radians()),
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
                grown.leaves_lying_undraped = lstats.lying_undraped;
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
            clouds: clouds.unwrap_or_default(),
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

/// What one built tree noted about itself.
fn note_built(scene: &mut Scene, b: &Built) {
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
        (
            "tree.leaves_lying_undraped",
            gr.leaves_lying_undraped as f64,
        ),
        ("tree.x_mm", b.at[0]),
        ("tree.y_mm", b.at[1]),
        ("tree.height_mm", b.top_mm - b.at[2]),
    ] {
        scene.note(k, v);
    }
}

/// Stamp a built tree's trunk on the ground and add it to the scene.
fn plant(scene: &mut Scene, path: &dt_kit::EntryId, b: Built) {
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
    if let Some(d) = scene.clouds() {
        for mut c in b.clouds {
            c.owner = path.clone();
            d.column(c);
        }
    }
    scene.add(
        Tag {
            kind: Kind::Tree,
            owner: path.clone(),
        },
        b.solid,
    );
}

impl Tree {
    /// How far a tree's trunk keeps from the tile's edge, mm: its crown
    /// radius as `grow` draws it (the same stream), or its flared foot when
    /// that is wider.  Draws from `seed` the way `grow` does.
    ///
    /// # Errors
    /// [`LookError`] for a trunk wider than the tile.
    pub(crate) fn footprint(&self, scene: &Scene, seed: Seed) -> Result<f64, LookError> {
        let s = scene.frame().scale();
        // the crown radius as `grow` draws it (the same stream)
        let mut rng = Rng::new(seed.key("sizes"));
        // The `let _ =` draws are not swallowed errors (a draw cannot fail):
        // each value is thrown away on purpose and the draw is made only to
        // advance the stream, in `grow`'s order, so the widths drawn here
        // are the ones `grow` will draw.
        if self.p.crown.is_none() {
            let _ = draw_dist(&self.p.height, &mut rng);
        }
        let _ = draw_dist(&self.p.trunk, &mut rng);
        let mut margin = 0.5 * draw_dist(&self.p.width, &mut rng) * s;
        let _ = draw_dist(&self.p.clump_size, &mut rng);
        let _ = draw_dist(&self.p.leaf_length, &mut rng);
        let _ = draw_dist(&self.p.leaf_width, &mut rng);
        if let Some(v) = &self.p.trunk_width {
            // a set trunk width: its flared foot must stand on the tile
            // too (it would hang past the edge), and must fit on it
            let foot = 0.5 * draw_dist(v, &mut rng) * s * (1.0 + FLARE);
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
        Ok(margin)
    }
}

impl Element for Tree {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Tree),
            reads: vec![Read::Solids(Kind::Terrain)],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let s = scene.frame().scale();
        let margin = self.footprint(scene, ctx.seed)?;
        let at = place(scene, ctx, margin, 3.0 * s)?;
        let b = self.grow(scene, ctx.seed, at, &ctx.printer, ctx.crowns)?;
        note_built(scene, &b);
        plant(scene, ctx.path, b);
        Ok(())
    }
}

/// How far crowns keep from one another when a scatter writes no `gap`,
/// mm at the 35 mm square.
const SCATTER_GAP_MM: f64 = 1.0;
/// How many layouts a scatter of trees tries before it says they do not fit.
const LAYOUTS: usize = 64;

/// `scatter { tree … }`: many trees, by count or per square, each a tree
/// of its own seed, crowns clear of one another.  Placed as a whole
/// first (largest crown first), then grown one after another: every
/// tree's slots are reserved in sequence, so the source ids do not depend
/// on the threads.
pub struct Scatter {
    count: Option<Draw>,
    per_square: Draw,
    gap: f64,
    reach: f64,
    /// Each tree and its share of the mix.
    trees: Vec<(Tree, f64)>,
}

impl Scatter {
    /// From its controls and its children (each a tree and its weight).
    ///
    /// # Errors
    /// [`LookError`] for a child that is not a tree, or bad controls.
    pub fn new(controls: &[Control], children: &[crate::Child]) -> Result<Self, LookError> {
        let set = Settings::new(controls);
        let mut trees = Vec::new();
        for ch in children {
            if ch.name != "tree" {
                return Err(LookError::general(
                    format!("`{}` is not a tree", ch.name),
                    "a scatter of trees alone holds trees; a mix goes through the mixed scatter",
                ));
            }
            trees.push((Tree::new(&ch.controls)?, ch.weight));
        }
        if trees.is_empty() {
            return Err(LookError::general(
                "a scatter needs a child to say what it scatters",
                "write `scatter count=3 { tree }`",
            ));
        }
        Ok(Self {
            count: set.dist("count")?,
            per_square: set.dist_or("perSquare", Draw::flat(1.0, 1.0))?,
            gap: set.number_or("gap", SCATTER_GAP_MM)?,
            reach: set.number_or("reach", crate::LINE_REACH_MM)?,
            trees,
        })
    }
}

impl Element for Scatter {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Tree),
            reads: vec![Read::Solids(Kind::Terrain)],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        if ctx.against.is_some() {
            return Err(LookError::general(
                "trees are not gathered at a foot",
                "scatter trees `at=` a region or a line",
            ));
        }
        let frame = *scene.frame();
        let s = frame.scale();
        let mut rng = Rng::new(ctx.seed.key("scatter"));
        let area = ctx.spread_area(&frame, self.reach);
        let squares = ctx.spread_squares(&frame, &area);
        // a whole number: the ends round and every one is as likely as any
        let n = match &self.count {
            Some(r) => r.int_by(1.0, &mut rng),
            None => self.per_square.int_by(squares, &mut rng),
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = n.clamp(0, 10_000) as usize;
        let total: f64 = self.trees.iter().map(|t| t.1).sum();
        // each tree: which, its own seed, and the room its crown needs
        let mut items: Vec<(&Tree, Seed)> = Vec::with_capacity(n);
        let mut radii = Vec::with_capacity(n);
        for i in 0..n {
            let item = ctx.seed.key("item").index(i as u64);
            let mut r = Rng::new(item);
            let mut u = r.random() * total;
            let (tree, _) = self
                .trees
                .iter()
                .find(|t| {
                    let hit = u < t.1;
                    u -= t.1;
                    hit
                })
                .unwrap_or(&self.trees[self.trees.len() - 1]);
            radii.push(tree.footprint(scene, item)?);
            items.push((tree, item));
        }
        let placed = lay_out(scene, &area, &mut rng, &radii, self.gap * s)?;
        grow_all(scene, ctx, &items, &placed)
    }
}

/// Dart-throw crowns of the given radii (world mm) into `area`, `gap`
/// clear of one another (world mm), largest first.  Darts land where they
/// land, and a tight row can fill up with a gap no crown fits: it is laid
/// out again (the next draws of the same stream) until every tree has
/// room, up to [`LAYOUTS`] times.
///
/// # Errors
/// [`LookError`] naming how many fit, when not all do.
pub(crate) fn lay_out(
    scene: &Scene,
    area: &dt_kit::Mask,
    rng: &mut Rng,
    radii: &[f64],
    gap: f64,
) -> Result<Vec<dt_kit::Placed>, LookError> {
    let s = scene.frame().scale();
    let n = radii.len();
    let mut placed = Vec::new();
    for _ in 0..LAYOUTS {
        let layout = scatter_sites(scene, area, rng, radii, gap, 3.0 * s);
        if layout.len() > placed.len() {
            placed = layout;
        }
        if placed.len() == n {
            break;
        }
    }
    if placed.len() < n {
        return Err(LookError::general(
            format!(
                "room for {} of the {n} trees: crowns keep clear of one another and the tile's edge",
                placed.len()
            ),
            "ask for fewer, make `gap` or the crown `width` smaller, or give a line more `reach`",
        ));
    }
    Ok(placed)
}

/// Grow each placed tree in turn (`items[p.index]`: its tree and its own
/// seed) on the ground and add it to the scene; every tree's slots are
/// reserved in sequence, so the source ids do not depend on the threads.
///
/// # Errors
/// [`LookError`] when a tree fails to grow.
pub(crate) fn grow_all(
    scene: &mut Scene,
    ctx: &Ctx<'_>,
    items: &[(&Tree, Seed)],
    placed: &[dt_kit::Placed],
) -> Result<(), LookError> {
    let mut leaves = 0.0;
    let (mut failed, mut undraped) = (0.0, 0.0);
    for p in placed {
        let (tree, seed) = items[p.index];
        let at = [p.at[0], p.at[1], ground_at(scene, p.at[0], p.at[1])];
        let b = tree.grow(scene, seed, at, &ctx.printer, ctx.crowns)?;
        leaves += b.grown.leaves as f64;
        failed += b.grown.leaves_failed_error as f64;
        undraped += b.grown.leaves_lying_undraped as f64;
        plant(scene, ctx.path, b);
    }
    scene.note("trees", placed.len() as f64);
    scene.note("trees.leaves", leaves);
    // what a lone tree notes of itself, summed over these trees
    scene.note("trees.leaves_failed_error", failed);
    scene.note("trees.leaves_lying_undraped", undraped);
    Ok(())
}

/// A tip's place for the tile's formulas: squares from the south-west
/// corner, the tree's disc and the ground at its foot.
fn dt_kit_tip(frame: &dt_kit::Frame, foot: [f64; 3], at: [f64; 2], radius: f64) -> crate::Tip {
    let sq = frame.square_mm;
    crate::Tip {
        at: [at[0] / sq, at[1] / sq],
        centre: [foot[0] / sq, foot[1] / sq],
        radius: radius / sq,
        bottom_dmm: foot[2] / frame.scale(),
    }
}

/// The crown's tips held under the cloud over them. Each tip gets a smooth
/// roll (neighbouring tips alike), ranked over the tips so the weighting's
/// odds hold, and the height the cloud gives it there; its height over the
/// crown's base is then the envelope's height share of it, so the crown
/// keeps its shape stretched or squeezed to the cloud, and no tip, with
/// the clump that stands on it, passes the cloud's high.
///
/// # Errors
/// [`LookError`] when the cloud leaves a tip no room above the crown's base
/// or the formula cannot be worked out there.
#[allow(clippy::too_many_arguments)] // the crown's inputs, and the debug layer's list
fn under_the_cloud(
    pts: &[[f64; 3]],
    env: &crown::Envelope,
    cap: f64,
    seed: Seed,
    radius: f64,
    s: f64,
    cloud_at: &dyn Fn(f64, f64) -> Result<crate::TopCloud, LookError>,
    mut marks: Option<&mut Vec<dt_kit::debug::Column>>,
) -> Result<Vec<[f64; 3]>, LookError> {
    let clouds: Vec<crate::TopCloud> = pts
        .iter()
        .map(|p| cloud_at(p[0], p[1]))
        .collect::<Result<_, _>>()?;
    // one smooth field of rolls over the tile; its rank is the roll
    let key = seed.key("roll");
    let rolls: Vec<f64> = pts
        .iter()
        .map(|p| {
            dt_kit::noise_at(
                key,
                (ROLL_RADII * radius / s).max(1.0) * s,
                &[p[0]],
                &[p[1]],
            )[0]
        })
        .collect();
    let mut order: Vec<usize> = (0..pts.len()).collect();
    order.sort_by(|&a, &b| rolls[a].total_cmp(&rolls[b]).then(a.cmp(&b)));
    let mut rank = vec![0.0; pts.len()];
    #[allow(clippy::cast_precision_loss)]
    for (r, &i) in order.iter().enumerate() {
        rank[i] = (r as f64 + 0.5) / pts.len() as f64;
    }
    let base = env.base_z();
    let h0 = env.crown_height();
    let mut out = Vec::with_capacity(pts.len());
    for ((p, c), u) in pts.iter().zip(&clouds).zip(&rank) {
        let t = c.odds.as_ref().map_or(*u, |w| w.unit(*u));
        let top = (c.lo + (c.hi - c.lo) * t) * s;
        let room = top - cap - base;
        if room < MIN_CROWN_MM * s {
            return Err(LookError::control(
                "top",
                format!(
                    "the top leaves the crown {:.1} mm to stand in over its base at ({:.1}, {:.1}) mm; its clumps need at least {:.1} mm",
                    room / s,
                    p[0] / s,
                    p[1] / s,
                    MIN_CROWN_MM + 0.0
                ),
                "raise the top where it is low, or lower the trunk".into(),
            ));
        }
        if let Some(m) = marks.as_deref_mut() {
            m.push(dt_kit::debug::Column {
                owner: dt_kit::EntryId::new(""),
                class: "tip-cloud",
                at: [p[0], p[1]],
                span: [c.lo * s, c.hi * s],
                rolled: top,
                shade: [
                    dt_kit::debug::shade_of(c.odds.as_ref(), 0.0),
                    dt_kit::debug::shade_of(c.odds.as_ref(), 1.0),
                    dt_kit::debug::shade_of(c.odds.as_ref(), t),
                ],
            });
        }
        let share = ((p[2] - base) / h0).max(0.0);
        out.push([p[0], p[1], (base + share * room).min(base + room)]);
    }
    Ok(out)
}

/// The least the crown's tips stand over its base, mm at the 35 mm square.
const MIN_CROWN_MM: f64 = 2.0;
