//! The organic leaf placer: thousands of separate leaves over the merged
//! crown. The placer works as a whole
//! crown pass, then a per-leaf pipeline.
//!
//! 1. **One surface.** The caller's foliage is the union of the clumps; all
//!    sampling, seating and containment run against it, so seams between
//!    clumps are just creases.
//! 2. **Coverage by construction.** Area-weighted candidates (10 × area /
//!    spacing²) are dart-thrown into a maximal Poisson-disk set at spacing
//!    0.45 × leaf width, with a normal-aware conflict test (roots on the two
//!    walls of a crease more than ~75° apart never block each other). Every
//!    placeable point is then within one spacing of a root; a fresh test
//!    sample counts what is left uncovered.
//! 3. **Direction.** Each leaf points down-slope, turned by up to ±15° by a
//!    smooth positional field (wavelength 7 mm): coherent, not combed.
//! 4. **Shingling.** Bottom-up by root height, each leaf stands off 0.3 mm
//!    (cap 1.2) above the tallest lower leaf whose tip conflicts with it.
//! 5. **Per leaf**: re-centre the blade on its root, seat the root oval at
//!    equal depth (0.75 mm), build the blade, skew it for printability,
//!    seat its belly 0.3 mm off the clump, stand it off, gate the neck,
//!    tuck base and tip, solidify. The blade blends continuously with the
//!    surface normal: curled (32°) and tip-lifted (1.2 mm) on every
//!    upward-facing part of the crown,
//!    a flush end-to-end arch (0.8 mm) on undersides, bare below a normal z
//!    of −0.75.
//! 6. **Skewer cull.** A leaf whose blade is inside a branch where the
//!    branch is outside the crown is dropped.
//!
//! Every ray goes through [`dt_kit::rays::RayMesh`]. Leaves are built in
//! parallel and gathered in root order, so the output is the same at any
//! thread count.

use std::collections::HashMap;

use dt_kit::Progress;
use dt_kit::rays::{RayHit, RayMesh};
use dt_kit::{Rng, Seed, TriMesh};
use rayon::prelude::*;

use super::leaf::{
    self, BASE, Blade, N_LAT, RINGS, STRIDE, TIP, V, add, cross, dot, norm, rotate_about,
    safe_norm, scale, sub,
};
use crate::LookError;

mod seat;
mod surface;

use seat::{Fail, Job, Outcome};
pub(super) use surface::split_bodies;
use surface::{Body, Crown};

// ── Constants ─────────────────────

/// Root spacing as a fraction of leaf width: the overlap knob.
const SPACING_FRAC: f64 = 0.45;
/// Candidates ≈ this × area / spacing².
const CANDIDATE_FACTOR: f64 = 10.0;
/// Zone blend: fully pitched blade at normal z ≥ HI, fully flush at ≤ LO.
const ZONE_HI: f64 = 0.30;
const ZONE_LO: f64 = -0.45;
/// Every leaf rooted on an upward-facing crown surface is fully pitched,
/// even when its downhill tip reaches a steeper part of the crown.
const TOP_NORMAL_Z: f64 = 0.0;
/// Below this normal z the clump stays bare.
const PLACEABLE_NORMAL_Z: f64 = -0.75;
/// Tip clearance ceiling blend: ceiling = touch + st × this.
const TIP_CEIL_RANGE_MM: f64 = 2.4;
/// Direction field: peak deviation from down-slope, and its wavelength.
const DIR_VAR_DEG: f64 = 15.0;
const DIR_WAVELEN_MM: f64 = 7.0;
/// Tip lift of a fully pitched blade.
const TIP_LIFT_MM: f64 = 1.2;
/// Per-leaf size jitter, downward only.
const SIZE_JITTER: f64 = 0.2;
/// Height-sorted shingle standoff.
const SHINGLE_STEP_MM: f64 = 0.3;
const SHINGLE_CAP_MM: f64 = 1.2;
const SHINGLE_NEIGHBOR_MM: f64 = 2.4;
const STANDOFF_JITTER_MM: f64 = 0.05;
/// The flush (underside) blade's end-to-end arch.
const FLUSH_ARCH_MM: f64 = 0.8;
/// Curl cap of a pitched blade.
const PITCH_CURL_DEG: f64 = 32.0;
/// Neck gate: in-plane slide ⊕ net normal standoff.
const MAX_NECK_MM: f64 = 1.8;
/// Skew cap, as a fraction of leaf length.
const MAX_SKEW_FRAC: f64 = 0.35;
/// A tip this near another leaf's base, centre or tip is a real overlap.
const TIP_CONFLICT_MM: f64 = 1.5;
/// Coverage test density (points ≈ area / this²).
const VERIFY_RES_MM: f64 = 0.9;
/// Normal agreement for a Poisson conflict (≈ 75°).
const ROOT_BLOCK_COS: f64 = 0.26;
/// Root oval embed depth.
const ROOT_EMBED_MM: f64 = 0.75;
/// Blade belly standoff above the clump.
const PROTRUSION_MM: f64 = 0.3;
/// Printability skew margin: blade tip over oval tip, in z.
const SKEW_TIP_MARGIN_MM: f64 = 0.05;
/// A flattened blade may not fold more than this (cosine of the angle
/// between the crown's normals a step apart): past it the crown has a
/// crease between clumps, and the blade is turned to lie elsewhere.
const DRAPE_FOLD_COS: f64 = 0.34;
/// [`Crown::goes_below`]: how far down the crown is looked for under a
/// flattened blade's lowest point.
const BELOW_STEP_MM: f64 = 0.3;
const BELOW_STEPS: usize = 3;
/// A blade's lowest point needs the crown this close under it, mm.
const HANG_REACH_MM: f64 = 0.3;
/// Headings a flattened blade tries, degrees off its own, in order.
const DRAPE_TURNS: [f64; 10] = [
    0.0, 30.0, -30.0, 60.0, -60.0, 90.0, -90.0, 135.0, -135.0, 180.0,
];
/// A flattened blade's middle lies this far off the crown (plus its
/// dome and its shingle step), mm.
const DRAPE_EMBED_MM: f64 = 0.0;
/// A flattened blade's dome fades out over this many degrees under the
/// printer's limit (by the crown's slope under it).
const DRAPE_DOME_FADE_DEG: f64 = 25.0;
/// A flattened blade's rim lies this far off the crown (inside it), mm.
const DRAPE_RIM_MM: f64 = -0.05;
/// A flattened blade is a slab down to the first of these depths inside
/// the crown, mm, that does not fold its floor.
const DRAPE_ROOT_MM: [f64; 5] = [0.4, 0.3, 0.2, 0.15, 0.1];
/// Each shingle step lifts a flattened blade this much more off the
/// crown, mm: overlapping flattened blades lie on surfaces a whole step
/// apart (near-coincident faces leave slivers in the union and break at
/// the float32 export).
const DRAPE_STEP_MM: f64 = 0.02;
/// A flattened blade rises from its rim's tuck to its full lift over this
/// height above its lowest point, mm.
const DRAPE_RAMP_MM: f64 = 0.6;

/// A flattened blade's lift off the crown for its shingle standoff.
fn drape_embed(standoff: f64) -> f64 {
    let step = libm::round(standoff / SHINGLE_STEP_MM).clamp(0.0, SHINGLE_CAP_MM / SHINGLE_STEP_MM);
    DRAPE_EMBED_MM + DRAPE_STEP_MM * step
}
/// A flattened blade finds the crown within this of its flat plane, mm.
const DRAPE_REACH_MM: f64 = 1.5;
/// Where a leaf's tip lies, as a fraction of its length down-slope of its
/// root (the blade spans ±½ L round the root).
const TIP_STEPS: usize = leaf::N_LONG / 2;
/// Margin under the printer's overhang limit for underside blades.
const HANG_MARGIN_DEG: f64 = 3.0;
/// Surface re-projection lift.
const PROJECT_LIFT_MM: f64 = 1.0;

/// The leaf's shape. The defaults: 4.5 × 3 mm, 0.24 mm
/// thick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeafShape {
    /// The largest leaf's length, mm (leaves vary down to 80%).
    pub length_mm: f64,
    /// The largest leaf's width, mm.
    pub width_mm: f64,
    /// Blade dome height, mm. A printability number: never scaled.
    pub thickness_mm: f64,
    /// Midrib crease V-angle, degrees.
    pub fold_angle_deg: f64,
    /// Crease-side Bézier shoulder of the cross-section.
    pub inner_curve: f64,
    /// Edge-side Bézier shoulder of the cross-section.
    pub outer_curve: f64,
    /// Tip curl, degrees (capped at 32° on the crown).
    pub curl_deg: f64,
    /// The printer's overhang limit, degrees from vertical: underside
    /// blades are flattened so no blade face hangs past it (see
    /// [`place`]).
    pub overhang_deg: f64,
}

impl Default for LeafShape {
    fn default() -> Self {
        Self {
            length_mm: 4.5,
            width_mm: 3.0,
            thickness_mm: 0.24,
            fold_angle_deg: 6.0,
            inner_curve: 1.5,
            outer_curve: 0.72,
            curl_deg: 40.0,
            overhang_deg: 55.0,
        }
    }
}

impl LeafShape {
    /// Sized for a system (`scale` = square / 35 mm). Length and width
    /// scale, but never below four printed lines; the thickness never
    /// scales.
    #[must_use]
    pub fn scaled(self, scale: f64, line_mm: f64) -> Self {
        let floor = 4.0 * line_mm;
        Self {
            length_mm: (self.length_mm * scale).max(floor),
            width_mm: (self.width_mm * scale).max(floor),
            ..self
        }
    }
}

/// One placed leaf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Its root on the crown, mm.
    pub root: [f64; 3],
    /// The crown's smooth normal there.
    pub normal: [f64; 3],
    /// The blade's seated direction.
    pub tangent: [f64; 3],
    /// The zone blend: 1 = pitched (curled, lifted) on upward faces, 0 =
    /// flush arch on undersides.
    pub zone: f64,
}

/// Wall time of each stage, seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LeafTimes {
    /// Ray structures, normals, wood bodies.
    pub prepare: f64,
    /// Candidates and the Poisson-disk throw.
    pub roots: f64,
    /// The coverage test.
    pub coverage: f64,
    /// Directions and standoffs.
    pub layout: f64,
    /// Every leaf's seat, build and cull.
    pub build: f64,
    /// All of it.
    pub total: f64,
}

/// What the placer did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeafStats {
    /// Leaf spacing used, mm.
    pub spacing_mm: f64,
    /// Candidates thrown.
    pub candidates: usize,
    /// Roots the Poisson-disk throw kept.
    pub roots: usize,
    /// Leaves returned.
    pub placed: usize,
    /// Builds that failed: the root oval would not seat inside the clump.
    pub failed_oval: usize,
    /// Builds that failed: the printability skew needed too long a slide.
    pub failed_skew: usize,
    /// Builds that failed: the stitch neck would be too long.
    pub failed_neck: usize,
    /// Builds that failed: degenerate frame.
    pub failed_error: usize,
    /// Builds that failed: a flattened blade could not find the crown under
    /// part of it.
    pub failed_drape: usize,
    /// Leaves dropped for skewering an exposed branch.
    pub branch_culled: usize,
    /// Coverage test points (placeable ones).
    pub test_points: usize,
    /// Test points with no root within one spacing on a like-facing surface.
    pub uncovered: usize,
    /// Each returned leaf, in output order.
    pub leaves: Vec<Placed>,
    /// Stage times.
    pub times: LeafTimes,
}

impl LeafStats {
    /// All failed builds.
    #[must_use]
    pub fn failed(&self) -> usize {
        self.failed_oval
            + self.failed_skew
            + self.failed_neck
            + self.failed_error
            + self.failed_drape
    }
}

/// Points on the surface and their smooth normals.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Samples {
    /// Positions, mm.
    pub points: Vec<[f64; 3]>,
    /// Unit normals.
    pub normals: Vec<[f64; 3]>,
}

/// A uniform in [0, 1) keyed by a tag and an index.
#[allow(clippy::cast_precision_loss, reason = "a 64-bit hash to [0, 1)")]
fn h01(seed: Seed, tag: &str, i: usize) -> f64 {
    seed.key(tag).index(i as u64).value() as f64 / 18_446_744_073_709_551_616.0
}

/// **leaves**: place separate leaves over a tree's crown.
///
/// `foliage` is the union of the foliage clumps as one closed mesh;
/// `wood` the trunk and branches (closed bodies: one union, or separate
/// closed tubes concatenated; each connected body is tested on its own),
/// used to drop leaves that skewer an exposed branch. Returns one closed,
/// outward-wound leaf solid per placed leaf, in world mm, in root order.
///
/// # Errors
/// [`LookError`] if the leaf shape is not positive and finite.
pub fn place(
    foliage: &TriMesh,
    wood: &TriMesh,
    shape: &LeafShape,
    seed: Seed,
) -> Result<(Vec<TriMesh>, LeafStats), LookError> {
    place_inner(foliage, wood, shape, seed, &Progress::none(), (0.0, 1.0))
}

/// [`place`], reporting the per-leaf build (the long part) to `progress`
/// as the part `span` (fractions of the stage) of the current stage.
///
/// # Errors
/// As [`place`].
pub fn place_progress(
    foliage: &TriMesh,
    wood: &TriMesh,
    shape: &LeafShape,
    seed: Seed,
    progress: &Progress,
    span: (f64, f64),
) -> Result<(Vec<TriMesh>, LeafStats), LookError> {
    place_inner(foliage, wood, shape, seed, progress, span)
}

fn place_inner(
    foliage: &TriMesh,
    wood: &TriMesh,
    shape: &LeafShape,
    seed: Seed,
    progress: &Progress,
    span: (f64, f64),
) -> Result<(Vec<TriMesh>, LeafStats), LookError> {
    let ok = |x: f64| x.is_finite() && x > 0.0;
    if !(ok(shape.length_mm) && ok(shape.width_mm) && ok(shape.thickness_mm))
        || !(shape.fold_angle_deg.is_finite()
            && shape.inner_curve.is_finite()
            && shape.outer_curve.is_finite()
            && shape.curl_deg.is_finite())
    {
        return Err(LookError::build(format!(
            "leaf shape must be positive and finite: {shape:?}"
        )));
    }
    let mut clock = Clock::start();
    let mut stats = LeafStats::default();
    if foliage.triangles.is_empty() {
        return Ok((Vec::new(), stats));
    }

    // ── prepare ──────────────────────────────────────────────────────────
    let crown = Crown::new(foliage);
    let bodies: Vec<Body> = split_bodies(wood).par_iter().map(Body::new).collect();
    stats.times.prepare = clock.lap();

    let len = shape.length_mm;
    let wid = shape.width_mm;
    let spacing = (SPACING_FRAC * wid).max(0.3);
    stats.spacing_mm = spacing;

    // ── roots: maximal normal-aware Poisson disk ──────────────────────────
    let cand = {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a count"
        )]
        let n = (CANDIDATE_FACTOR * crown.area / (spacing * spacing)).max(64.0) as usize;
        crown.sample(n, &mut Rng::new(seed.key("leaf-candidates")))
    };
    stats.candidates = cand.points.len();
    let tau = 2.0 * std::f64::consts::PI;
    let phase = [1, 2, 3].map(|k| h01(seed, "org-dir", k) * tau);
    let reach = Reach {
        crown: &crown,
        phase,
        len,
    };
    let ok_cand = reach.placeable(&cand);
    let mut grid = PoissonGrid::new(spacing);
    let mut roots: Vec<V> = Vec::new();
    let mut root_n: Vec<V> = Vec::new();
    for ((p, n), ok) in cand.points.iter().zip(&cand.normals).zip(ok_cand) {
        if !ok {
            continue;
        }
        let n = safe_norm(*n);
        if grid.blocked(*p, n) {
            continue;
        }
        grid.mark(*p, n);
        roots.push(*p);
        root_n.push(n);
    }
    stats.roots = roots.len();
    stats.times.roots = clock.lap();

    // ── coverage verification ──────────────────────────────────────────────
    let tests = {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a count"
        )]
        let n = (crown.area / (VERIFY_RES_MM * VERIFY_RES_MM)).max(64.0) as usize;
        crown.sample(n, &mut Rng::new(seed.key("leaf-coverage")))
    };
    let ok_test = reach.placeable(&tests);
    let checked: Vec<bool> = tests
        .points
        .par_iter()
        .zip(&tests.normals)
        .zip(&ok_test)
        .filter(|(_, ok)| **ok)
        .map(|((p, n), _)| grid.blocked(*p, safe_norm(*n)))
        .collect();
    stats.test_points = checked.len();
    stats.uncovered = checked.iter().filter(|c| !**c).count();
    stats.times.coverage = clock.lap();

    // ── directions and standoffs ──────────────────────────────────────────
    let dirs: Vec<V> = roots
        .par_iter()
        .zip(&root_n)
        .map(|(b, n)| reach.direction(*b, *n))
        .collect();
    let standoffs = shingle_standoffs(&roots, &dirs, len, seed);
    stats.times.layout = clock.lap();

    // ── per-leaf build ────────────────────────────────────────────────────
    let tris = leaf::surface_triangles();
    let job = Job {
        crown: &crown,
        bodies: &bodies,
        shape,
        tris: &tris,
    };
    let counter = progress.counter("leaves", roots.len(), span.0, span.1);
    let outcomes: Vec<(f64, Outcome)> = (0..roots.len())
        .into_par_iter()
        .map(|i| {
            counter.tick();
            let n = root_n[i];
            let lying = reach.lying_nz(roots[i], n, dirs[i]);
            let st = leaf_zone(n[2], lying);
            let k = 1.0 - SIZE_JITTER * h01(seed, "org-size", i);
            let o = job.leaf(roots[i], n, dirs[i], lying, standoffs[i], len * k, wid * k);
            (st, o)
        })
        .collect();
    stats.times.build = clock.lap();

    let mut solids = Vec::new();
    for (i, (st, o)) in outcomes.into_iter().enumerate() {
        match o {
            Outcome::Placed(m, tangent) => {
                stats.leaves.push(Placed {
                    root: roots[i],
                    normal: root_n[i],
                    tangent,
                    zone: st,
                });
                solids.push(m);
            }
            Outcome::Branch => stats.branch_culled += 1,
            Outcome::Failed(Fail::Oval) => stats.failed_oval += 1,
            Outcome::Failed(Fail::Skew) => stats.failed_skew += 1,
            Outcome::Failed(Fail::Neck) => stats.failed_neck += 1,
            Outcome::Failed(Fail::Error) => stats.failed_error += 1,
            Outcome::Failed(Fail::Drape) => stats.failed_drape += 1,
        }
    }
    stats.placed = solids.len();
    stats.times.total = clock.total();
    Ok((solids, stats))
}

// ── roots ─────────────────────────────────────────────────────────────────────

/// The normal-aware Poisson grid: a root blocks a point within `gap`
/// whose normal agrees (dot > `ROOT_BLOCK_COS`).
struct PoissonGrid {
    gap: f64,
    cells: HashMap<[i64; 3], Vec<(V, V)>>,
}

impl PoissonGrid {
    fn new(gap: f64) -> Self {
        Self {
            gap,
            cells: HashMap::new(),
        }
    }

    #[allow(clippy::cast_possible_truncation, reason = "grid cells of a crown")]
    fn cell(&self, p: V) -> [i64; 3] {
        p.map(|x| libm::floor(x / self.gap) as i64)
    }

    fn blocked(&self, p: V, n: V) -> bool {
        let c = self.cell(p);
        let g2 = self.gap * self.gap;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = self.cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for (q, qn) in v {
                            let d = sub(p, *q);
                            if dot(d, d) < g2 && dot(n, *qn) > ROOT_BLOCK_COS {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    fn mark(&mut self, p: V, n: V) {
        let c = self.cell(p);
        self.cells.entry(c).or_default().push((p, n));
    }
}

// ── direction and standoff ────────────────────────────────────────────────────

/// Steepest descent in the tangent plane; radially out
/// at the apex.
fn growth_tangent(n: V, base: V, centroid: V) -> V {
    let down = [0.0, 0.0, -1.0];
    let d = sub(down, scale(n, dot(down, n)));
    let dl = norm(d);
    if dl > 1e-6 {
        return scale(d, 1.0 / dl);
    }
    let radial = [base[0] - centroid[0], base[1] - centroid[1], 0.0];
    let r = sub(radial, scale(n, dot(radial, n)));
    let rl = norm(r);
    if rl > 1e-6 {
        return scale(r, 1.0 / rl);
    }
    safe_norm(sub([1.0, 0.0, 0.0], scale(n, n[0])))
}

/// Three positional sines, radians.
fn field_angle(p: V, phase: [f64; 3]) -> f64 {
    let f = 2.0 * std::f64::consts::PI / DIR_WAVELEN_MM;
    let s = (libm::sin(f * p[0] + phase[0])
        + libm::sin(f * 0.83 * p[1] + phase[1])
        + libm::sin(f * 1.19 * p[2] + phase[2]))
        / 3.0;
    DIR_VAR_DEG.to_radians() * s
}

/// Bottom-up, each leaf stands `STEP` above the
/// tallest lower leaf it has a tip conflict with (capped), plus jitter.
#[allow(clippy::cast_possible_truncation, reason = "grid cells of a crown")]
fn shingle_standoffs(bases: &[V], dirs: &[V], len: f64, seed: Seed) -> Vec<f64> {
    let n = bases.len();
    let mut out = vec![0.0; n];
    let tips: Vec<V> = (0..n)
        .map(|i| add(bases[i], scale(dirs[i], 0.45 * len)))
        .collect();
    let tails: Vec<V> = (0..n)
        .map(|i| sub(bases[i], scale(dirs[i], 0.45 * len)))
        .collect();
    let r2 = TIP_CONFLICT_MM * TIP_CONFLICT_MM;
    let conflict = |i: usize, j: usize| {
        [(i, j), (j, i)].iter().any(|&(a, b)| {
            [tails[b], bases[b], tips[b]].iter().any(|pt| {
                let d = sub(tips[a], *pt);
                dot(d, d) < r2
            })
        })
    };
    let jitter: Vec<f64> = (0..n)
        .into_par_iter()
        .map(|i| h01(seed, "org-so", i))
        .collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| bases[a][2].total_cmp(&bases[b][2]));
    let d = SHINGLE_NEIGHBOR_MM;
    let mut cells: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
    for i in order {
        let c = bases[i].map(|x| libm::floor(x / d) as i64);
        let mut tallest = -1.0_f64;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = cells.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) {
                        for &j in v {
                            if conflict(i, j) {
                                tallest = tallest.max(out[j]);
                            }
                        }
                    }
                }
            }
        }
        let base_so = if tallest < 0.0 {
            0.0
        } else {
            tallest + SHINGLE_STEP_MM
        };
        out[i] = SHINGLE_CAP_MM.min(base_so) + STANDOFF_JITTER_MM * jitter[i];
        cells.entry(c).or_default().push(i);
    }
    out
}

/// Printable undersides: on a face whose normal points down (overhang
/// `α = asin(−nz)` from vertical), the blade may tilt at most
/// `limit − α − HANG_MARGIN_DEG` further. The steepest tilt of a blade
/// against its seat is at its tip, where both arches (the full-length
/// one and the flush one) slope, `atan(tan(arch) + 4·arch_mm / L)`, and
/// the curl and the lift turn it further. Returns the factor (0..1) all
/// four are scaled by to fit.
fn hang_scale(nz: f64, limit_deg: f64, b: &Blade, arch_mm: f64) -> f64 {
    if nz >= 0.0 {
        return 1.0;
    }
    let alpha = libm::asin((-nz).min(1.0)).to_degrees();
    let budget = (limit_deg - alpha - HANG_MARGIN_DEG).max(0.0).to_radians();
    let tilt = |k: f64| {
        libm::atan(libm::tan((k * b.arch_deg).to_radians()) + 4.0 * k * arch_mm / b.length_mm)
            + (k * b.curl_deg).to_radians()
            + libm::atan(k * b.lift_mm / b.length_mm)
    };
    if tilt(1.0) <= budget {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let m = 0.5 * (lo + hi);
        if tilt(m) <= budget { lo = m } else { hi = m }
    }
    lo
}

/// Where a leaf may root, and which way it points.
struct Reach<'a> {
    crown: &'a Crown,
    phase: [f64; 3],
    len: f64,
}

impl Reach<'_> {
    /// Down-slope, turned by the smooth field.
    fn direction(&self, b: V, n: V) -> V {
        let t0 = growth_tangent(n, b, self.crown.centroid);
        safe_norm(rotate_about(t0, n, field_angle(b, self.phase)))
    }

    /// Placeable: normal z ≥ −0.75. With a printer limit the
    /// steep band under the crown is placeable too: its blades are laid
    /// flat on the crown ([`Job::drape`]).
    fn placeable(&self, s: &Samples) -> Vec<bool> {
        s.normals
            .iter()
            .map(|n| n[2] >= PLACEABLE_NORMAL_Z)
            .collect()
    }

    /// The normal z a leaf's blade lies on: its root's, or, with a printer
    /// limit, the lower of its root's and the surface's under its tip
    /// (half a leaf down-slope, walked in [`TIP_STEPS`] steps). A blade rooted on an upward face
    /// whose tip reaches over the crown's underside is built for the
    /// underside.
    fn lying_nz(&self, root: V, n: V, dir: V) -> f64 {
        match self
            .crown
            .walk(root, n, dir, self.len / leaf::N_LONG as f64, TIP_STEPS)
        {
            Ok(frames) => frames.iter().fold(n[2], |m, f| m.min(f.n[2])),
            Err(_) => -1.0,
        }
    }
}

/// A point on the crown with its normal and heading.
#[derive(Clone, Copy)]
struct Frame {
    p: V,
    n: V,
    t: V,
}

/// The zone blend: 1 on upward faces (pitched blade), 0 on undersides
/// (flush), smoothstep between.
fn zone(nz: f64) -> f64 {
    let tz = ((nz - ZONE_LO) / (ZONE_HI - ZONE_LO)).clamp(0.0, 1.0);
    tz * tz * (3.0 - 2.0 * tz)
}

/// Upward-facing roots own the top silhouette: keep them fully pitched even
/// when their downhill tips cross onto a steeper surface. Elsewhere the whole
/// blade's lowest crown normal still controls the pitch for printability.
fn leaf_zone(root_nz: f64, lying_nz: f64) -> f64 {
    if root_nz >= TOP_NORMAL_Z {
        1.0
    } else {
        zone(lying_nz)
    }
}

/// Stage timing.
struct Clock {
    start: std::time::Instant,
    last: std::time::Instant,
}

impl Clock {
    fn start() -> Self {
        let t = std::time::Instant::now();
        Self { start: t, last: t }
    }
    fn lap(&mut self) -> f64 {
        let t = std::time::Instant::now();
        let s = (t - self.last).as_secs_f64();
        self.last = t;
        s
    }
    fn total(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed UV sphere (outward winding), radius `r` mm, centred at
    /// `(0, 0, r + 1)`.
    #[allow(clippy::cast_possible_truncation, reason = "tiny indices")]
    fn sphere(r: f64) -> TriMesh {
        let (nu, nv) = (28_usize, 18_usize);
        let mut positions = vec![[0.0, 0.0, r + 1.0 + r], [0.0, 0.0, 1.0]];
        for j in 1..nv {
            let phi = std::f64::consts::PI * j as f64 / nv as f64;
            for i in 0..nu {
                let th = 2.0 * std::f64::consts::PI * i as f64 / nu as f64;
                positions.push([
                    r * libm::sin(phi) * libm::cos(th),
                    r * libm::sin(phi) * libm::sin(th),
                    r + 1.0 + r * libm::cos(phi),
                ]);
            }
        }
        let ring = |j: usize, i: usize| (2 + (j - 1) * nu + i % nu) as u32;
        let mut triangles = Vec::new();
        for i in 0..nu {
            triangles.push([0, ring(1, i), ring(1, i + 1)]);
            triangles.push([1, ring(nv - 1, i + 1), ring(nv - 1, i)]);
        }
        for j in 1..nv - 1 {
            for i in 0..nu {
                let (a, b, c, d) = (
                    ring(j, i),
                    ring(j, i + 1),
                    ring(j + 1, i + 1),
                    ring(j + 1, i),
                );
                triangles.push([a, d, c]);
                triangles.push([a, c, b]);
            }
        }
        TriMesh {
            positions,
            triangles,
            ..TriMesh::default()
        }
    }

    #[test]
    fn a_crown_gets_closed_leaves_and_full_coverage() {
        let crown = sphere(9.0);
        assert!(crown.is_watertight());
        let (leaves, stats) = place(
            &crown,
            &TriMesh::default(),
            &LeafShape::default(),
            Seed::new(7),
        )
        .expect("places");
        assert!(stats.placed > 100, "{stats:?}");
        assert_eq!(leaves.len(), stats.placed);
        for m in &leaves {
            assert!(m.is_watertight());
        }
        assert!(
            stats.uncovered * 100 <= stats.test_points.max(1),
            "{} of {} uncovered",
            stats.uncovered,
            stats.test_points
        );
        // The printer limit lays underside blades flat: no root faces
        // straight down.
        assert!(stats.leaves.iter().all(|l| l.normal[2] >= -0.75));
        // Every upward-facing root contributes a proud leaf to the top
        // silhouette, even if its downhill tip crosses onto the side.
        assert!(
            stats
                .leaves
                .iter()
                .filter(|l| l.normal[2] >= TOP_NORMAL_Z)
                .all(|l| (l.zone - 1.0).abs() < f64::EPSILON)
        );
    }
}
