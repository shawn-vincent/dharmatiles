//! The organic leaf placer: thousands of separate leaves over the merged
//! crown. A port of `src/dharmatiles/trees/placement_organic.py`
//! (`place_leaves_organic`) and the per-leaf pipeline of
//! `placement_leaf.py` (`_attempt_leaf_gen`), with the same constants.
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
//!    surface normal: curled (32°) and tip-lifted (1.2 mm) on upward faces,
//!    a flush end-to-end arch (0.8 mm) on undersides, bare below a normal z
//!    of −0.75.
//! 6. **Skewer cull.** A leaf whose blade is inside a branch where the
//!    branch is outside the crown is dropped.
//!
//! Every ray goes through [`dt_kit::rays::RayMesh`]. Leaves are built in
//! parallel and gathered in root order, so the output is the same at any
//! thread count (R2).

use std::collections::HashMap;

use dt_kit::rays::{RayHit, RayMesh};
use dt_kit::{Rng, Seed, TriMesh};
use rayon::prelude::*;

use super::leaf::{
    self, BASE, Blade, N_LAT, RINGS, STRIDE, TIP, V, add, cross, dot, norm, rotate_about,
    safe_norm, scale, sub,
};
use crate::LookError;

// ── Constants (placement_organic.py / placement_leaf.py) ─────────────────────

/// Root spacing as a fraction of leaf width: the overlap knob.
const SPACING_FRAC: f64 = 0.45;
/// Candidates ≈ this × area / spacing².
const CANDIDATE_FACTOR: f64 = 10.0;
/// Zone blend: fully pitched blade at normal z ≥ HI, fully flush at ≤ LO.
const ZONE_HI: f64 = 0.30;
const ZONE_LO: f64 = -0.45;
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
/// Surface re-projection lift.
const PROJECT_LIFT_MM: f64 = 1.0;

/// The leaf's shape. The defaults are the approved leaf: 4.5 × 3 mm, 0.24
/// mm thick (`Tree(...)`'s `leaf_*` defaults).
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
        }
    }
}

impl LeafShape {
    /// Sized for a system (D2: `scale` = square / 35 mm). Length and width
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

/// Wall time of each stage, seconds (0 on wasm).
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
    /// Leaves dropped for skewering an exposed branch.
    pub branch_culled: usize,
    /// Coverage test points (placeable ones).
    pub test_points: usize,
    /// Test points with no root within one spacing on a like-facing surface.
    pub uncovered: usize,
    /// Each returned leaf's root and blade direction (its seated tangent,
    /// Python's `base_tangents`), in output order.
    pub roots_placed: Vec<([f64; 3], [f64; 3])>,
    /// Stage times.
    pub times: LeafTimes,
}

impl LeafStats {
    /// All failed builds.
    #[must_use]
    pub fn failed(&self) -> usize {
        self.failed_oval + self.failed_skew + self.failed_neck + self.failed_error
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

/// Where the placer's random draws come from.
#[derive(Clone, Debug, PartialEq)]
pub enum Draws {
    /// The engine's keyed seeds (what [`place`] uses).
    Seed(Seed),
    /// Parity with the Python library: its candidate and test samples
    /// (numpy's stream cannot be reproduced) and its `_hash01` keyed by
    /// the placer's integer seed.
    Python {
        /// The placer's `seeds[0]`.
        seed0: u64,
        /// `_surface_points_with_normals(union, n_cand, rng)`.
        candidates: Samples,
        /// The coverage test sample.
        tests: Samples,
    },
}

impl Draws {
    /// A uniform in [0, 1) keyed by a tag and an index.
    #[allow(clippy::cast_precision_loss, reason = "a 64-bit hash to [0, 1)")]
    fn h01(&self, tag: &str, i: usize) -> f64 {
        let h = match self {
            Self::Seed(s) => s.key(tag).index(i as u64).value(),
            Self::Python { seed0, .. } => py_hash(&[&seed0.to_string(), tag, &i.to_string()]),
        };
        h as f64 / 18_446_744_073_709_551_616.0
    }
}

/// Python's `_hash01` before the division: FNV-1a over each part's text,
/// a 0xFF separator, then murmur3's fmix64.
fn py_hash(parts: &[&str]) -> u64 {
    const P: u64 = 1_099_511_628_211;
    let mut h: u64 = 1_469_598_103_934_665_603;
    for part in parts {
        for &b in part.as_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(P);
        }
        h ^= 0xFF;
        h = h.wrapping_mul(P);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    h
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
    place_with(foliage, wood, shape, &Draws::Seed(seed))
}

/// [`place`] with the draws given (parity runs replay Python's).
///
/// # Errors
/// As [`place`].
pub fn place_with(
    foliage: &TriMesh,
    wood: &TriMesh,
    shape: &LeafShape,
    draws: &Draws,
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
    let cand = match draws {
        Draws::Seed(s) => {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a count"
            )]
            let n = (CANDIDATE_FACTOR * crown.area / (spacing * spacing)).max(64.0) as usize;
            crown.sample(n, &mut Rng::new(s.key("leaf-candidates")))
        }
        Draws::Python { candidates, .. } => candidates.clone(),
    };
    stats.candidates = cand.points.len();
    let mut grid = PoissonGrid::new(spacing);
    let mut roots: Vec<V> = Vec::new();
    let mut root_n: Vec<V> = Vec::new();
    for (p, n) in cand.points.iter().zip(&cand.normals) {
        if n[2] < PLACEABLE_NORMAL_Z {
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
    let tests = match draws {
        Draws::Seed(s) => {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a count"
            )]
            let n = (crown.area / (VERIFY_RES_MM * VERIFY_RES_MM)).max(64.0) as usize;
            crown.sample(n, &mut Rng::new(s.key("leaf-coverage")))
        }
        Draws::Python { tests, .. } => tests.clone(),
    };
    let checked: Vec<bool> = tests
        .points
        .par_iter()
        .zip(&tests.normals)
        .filter(|(_, n)| n[2] >= PLACEABLE_NORMAL_Z)
        .map(|(p, n)| grid.blocked(*p, safe_norm(*n)))
        .collect();
    stats.test_points = checked.len();
    stats.uncovered = checked.iter().filter(|c| !**c).count();
    stats.times.coverage = clock.lap();

    // ── directions and standoffs ──────────────────────────────────────────
    let tau = 2.0 * std::f64::consts::PI;
    let phase = [1, 2, 3].map(|k| draws.h01("org-dir", k) * tau);
    let dirs: Vec<V> = roots
        .par_iter()
        .zip(&root_n)
        .map(|(b, n)| {
            let t0 = growth_tangent(*n, *b, crown.centroid);
            safe_norm(rotate_about(t0, *n, field_angle(*b, phase)))
        })
        .collect();
    let standoffs = shingle_standoffs(&roots, &dirs, len, draws);
    stats.times.layout = clock.lap();

    // ── per-leaf build ────────────────────────────────────────────────────
    let tris = leaf::surface_triangles();
    let job = Job {
        crown: &crown,
        bodies: &bodies,
        shape,
        tris: &tris,
    };
    let outcomes: Vec<Outcome> = (0..roots.len())
        .into_par_iter()
        .map(|i| {
            let n = root_n[i];
            let tz = ((n[2] - ZONE_LO) / (ZONE_HI - ZONE_LO)).clamp(0.0, 1.0);
            let st = tz * tz * (3.0 - 2.0 * tz);
            let k = 1.0 - SIZE_JITTER * draws.h01("org-size", i);
            job.leaf(roots[i], n, dirs[i], st, standoffs[i], len * k, wid * k)
        })
        .collect();
    stats.times.build = clock.lap();

    let mut solids = Vec::new();
    for (i, o) in outcomes.into_iter().enumerate() {
        match o {
            Outcome::Placed(m, tangent) => {
                stats.roots_placed.push((roots[i], tangent));
                solids.push(m);
            }
            Outcome::Branch => stats.branch_culled += 1,
            Outcome::Failed(Fail::Oval) => stats.failed_oval += 1,
            Outcome::Failed(Fail::Skew) => stats.failed_skew += 1,
            Outcome::Failed(Fail::Neck) => stats.failed_neck += 1,
            Outcome::Failed(Fail::Error) => stats.failed_error += 1,
        }
    }
    stats.placed = solids.len();
    stats.times.total = clock.total();
    Ok((solids, stats))
}

// ── the crown surface ─────────────────────────────────────────────────────────

/// The foliage union, ready for queries.
struct Crown {
    rays: RayMesh,
    positions: Vec<V>,
    triangles: Vec<[u32; 3]>,
    /// Angle-weighted vertex normals (trimesh's `vertex_normals`), equal
    /// across split copies of a vertex.
    normals: Vec<V>,
    areas: Vec<f64>,
    area: f64,
    /// Mean of the distinct vertices.
    centroid: V,
}

impl Crown {
    fn new(m: &TriMesh) -> Self {
        let rep = weld(m);
        let mut acc = vec![[0.0; 3]; m.positions.len()];
        let mut areas = Vec::with_capacity(m.triangles.len());
        for t in &m.triangles {
            let p = t.map(|i| m.positions[i as usize]);
            let fn_ = cross(sub(p[1], p[0]), sub(p[2], p[0]));
            let l = norm(fn_);
            areas.push(0.5 * l);
            if l <= 0.0 {
                continue;
            }
            let fnu = scale(fn_, 1.0 / l);
            for k in 0..3 {
                let a = sub(p[(k + 1) % 3], p[k]);
                let b = sub(p[(k + 2) % 3], p[k]);
                let ang = libm::acos((dot(a, b) / (norm(a) * norm(b))).clamp(-1.0, 1.0));
                let r = rep[t[k] as usize];
                acc[r] = add(acc[r], scale(fnu, ang));
            }
        }
        let normals = (0..m.positions.len())
            .map(|i| {
                let v = acc[rep[i]];
                let l = norm(v);
                if l > 0.0 { scale(v, 1.0 / l) } else { v }
            })
            .collect();
        let mut c = [0.0; 3];
        let mut n = 0.0;
        for (i, p) in m.positions.iter().enumerate() {
            if rep[i] == i {
                c = add(c, *p);
                n += 1.0;
            }
        }
        Self {
            rays: RayMesh::new(m),
            positions: m.positions.clone(),
            triangles: m.triangles.clone(),
            normals,
            area: areas.iter().sum(),
            areas,
            centroid: scale(c, 1.0 / n),
        }
    }

    /// Area-weighted samples with smooth normals (`_sample_surface` +
    /// `_surface_points_with_normals`; same draw order: faces, u, v).
    fn sample(&self, n: usize, rng: &mut Rng) -> Samples {
        let mut cum = Vec::with_capacity(self.areas.len());
        let mut s = 0.0;
        for a in &self.areas {
            s += a;
            cum.push(s);
        }
        if s <= 0.0 {
            return Samples::default();
        }
        for c in &mut cum {
            *c /= s;
        }
        let fi: Vec<usize> = (0..n)
            .map(|_| {
                let r = rng.random();
                cum.partition_point(|c| *c < r).min(cum.len() - 1)
            })
            .collect();
        let u: Vec<f64> = (0..n).map(|_| rng.random()).collect();
        let v: Vec<f64> = (0..n).map(|_| rng.random()).collect();
        let mut out = Samples::default();
        for k in 0..n {
            let (mut a, mut b) = (u[k], v[k]);
            if a + b > 1.0 {
                a = 1.0 - a;
                b = 1.0 - b;
            }
            let t = self.triangles[fi[k]];
            let [p0, p1, p2] = t.map(|i| self.positions[i as usize]);
            let p = add(add(p0, scale(sub(p1, p0), a)), scale(sub(p2, p0), b));
            out.points.push(p);
            out.normals.push(self.smooth_normal(fi[k], p));
        }
        out
    }

    /// Barycentric blend of a triangle's vertex normals at `p` (trimesh's
    /// `points_to_barycentric`, Cramer's rule), normalised.
    fn smooth_normal(&self, tri: usize, p: V) -> V {
        let t = self.triangles[tri];
        let [a, b, c] = t.map(|i| self.positions[i as usize]);
        let (e0, e1, w) = (sub(b, a), sub(c, a), sub(p, a));
        let (d00, d01, d11) = (dot(e0, e0), dot(e0, e1), dot(e1, e1));
        let (d02, d12) = (dot(e0, w), dot(e1, w));
        let inv = 1.0 / (d00 * d11 - d01 * d01);
        let b1 = (d11 * d02 - d01 * d12) * inv;
        let b2 = (d00 * d12 - d01 * d02) * inv;
        let b0 = 1.0 - b1 - b2;
        let [n0, n1, n2] = t.map(|i| self.normals[i as usize]);
        let n = add(add(scale(n0, b0), scale(n1, b1)), scale(n2, b2));
        let l = norm(n);
        if l > 1e-9 {
            scale(n, 1.0 / l)
        } else {
            [0.0; 3]
        }
    }

    fn inside(&self, p: V) -> bool {
        self.rays.contains(&[p])[0]
    }

    fn hit(&self, o: V, d: V) -> Option<RayHit> {
        self.rays.first_hits(&[o], &[d])[0]
    }

    /// Signed clearance of `p` along `n`: positive outside (distance down
    /// to the surface along −n), negative inside (depth up to it along
    /// +n); `None` when the ray misses.
    fn clearance(&self, p: V, n: V) -> Option<(f64, bool)> {
        let inside = self.inside(p);
        let d = if inside { n } else { scale(n, -1.0) };
        self.hit(p, d).map(|h| {
            let c = norm(sub(h.point, p));
            (if inside { -c } else { c }, inside)
        })
    }

    /// `_project_to_surface_gen`: drop `q` onto the surface along −n from
    /// 1 mm above; the hit and its smooth normal, unless it misses or falls
    /// further than `max_drop`.
    fn project(&self, q: V, n: V, max_drop: f64) -> Option<(V, V)> {
        let g = add(q, scale(n, PROJECT_LIFT_MM));
        let h = self.hit(g, scale(n, -1.0))?;
        if norm(sub(h.point, g)) > max_drop {
            return None;
        }
        Some((
            h.point,
            safe_norm(self.smooth_normal(h.triangle as usize, h.point)),
        ))
    }
}

/// `rep[i]`: the vertex `i` is welded to (`merge_from` → `merge_to`).
fn weld(m: &TriMesh) -> Vec<usize> {
    let mut rep: Vec<usize> = (0..m.positions.len()).collect();
    for (f, t) in m.merge_from.iter().zip(&m.merge_to) {
        rep[*f as usize] = *t as usize;
    }
    // Chains (a → b → c) settle on their end.
    for i in 0..rep.len() {
        let mut r = rep[i];
        while rep[r] != r {
            r = rep[r];
        }
        rep[i] = r;
    }
    rep
}

/// One closed wood body (a branch tube, or the whole wood when unioned).
struct Body {
    rays: RayMesh,
    lo: V,
    hi: V,
}

impl Body {
    fn new(m: &TriMesh) -> Self {
        let rays = RayMesh::new(m);
        let (lo, hi) = rays.bounds();
        Self { rays, lo, hi }
    }
}

/// The connected bodies of a mesh (by shared, welded vertices), in order
/// of their first triangle.
#[allow(clippy::cast_possible_truncation, reason = "vertex indices fit u32")]
fn split_bodies(m: &TriMesh) -> Vec<TriMesh> {
    let rep = weld(m);
    let mut parent: Vec<usize> = (0..m.positions.len()).collect();
    fn find(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    for t in &m.triangles {
        let a = find(&mut parent, rep[t[0] as usize]);
        for &v in &t[1..] {
            let b = find(&mut parent, rep[v as usize]);
            if a != b {
                parent[b] = a;
            }
        }
    }
    let mut order: Vec<usize> = Vec::new();
    let mut of: HashMap<usize, usize> = HashMap::new();
    let mut tris: Vec<Vec<[u32; 3]>> = Vec::new();
    for t in &m.triangles {
        let r = find(&mut parent, rep[t[0] as usize]);
        let k = *of.entry(r).or_insert_with(|| {
            order.push(r);
            tris.push(Vec::new());
            tris.len() - 1
        });
        tris[k].push(*t);
    }
    tris.into_iter()
        .map(|ts| {
            let mut map: HashMap<u32, u32> = HashMap::new();
            let mut positions = Vec::new();
            let triangles = ts
                .iter()
                .map(|t| {
                    t.map(|i| {
                        *map.entry(i).or_insert_with(|| {
                            positions.push(m.positions[i as usize]);
                            (positions.len() - 1) as u32
                        })
                    })
                })
                .collect();
            TriMesh {
                positions,
                triangles,
                ..TriMesh::default()
            }
        })
        .collect()
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

/// `_growth_tangent`: steepest descent in the tangent plane; radially out
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

/// `_direction_field_angle`: three positional sines, radians.
fn field_angle(p: V, phase: [f64; 3]) -> f64 {
    let f = 2.0 * std::f64::consts::PI / DIR_WAVELEN_MM;
    let s = (libm::sin(f * p[0] + phase[0])
        + libm::sin(f * 0.83 * p[1] + phase[1])
        + libm::sin(f * 1.19 * p[2] + phase[2]))
        / 3.0;
    DIR_VAR_DEG.to_radians() * s
}

/// `_shingle_standoffs`: bottom-up, each leaf stands `STEP` above the
/// tallest lower leaf it has a tip conflict with (capped), plus jitter.
#[allow(clippy::cast_possible_truncation, reason = "grid cells of a crown")]
fn shingle_standoffs(bases: &[V], dirs: &[V], len: f64, draws: &Draws) -> Vec<f64> {
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
        .map(|i| draws.h01("org-so", i))
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

// ── one leaf ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fail {
    Oval,
    Skew,
    Neck,
    Error,
}

#[allow(clippy::large_enum_variant, reason = "short-lived, one per root")]
enum Outcome {
    Placed(TriMesh, V),
    Branch,
    Failed(Fail),
}

struct Job<'a> {
    crown: &'a Crown,
    bodies: &'a [Body],
    shape: &'a LeafShape,
    tris: &'a [[u32; 3]],
}

impl Job<'_> {
    /// `_leaf_job`: re-centre on the root, attempt, cull skewers.
    #[allow(clippy::too_many_arguments, reason = "the per-root inputs")]
    fn leaf(
        &self,
        base: V,
        nrm: V,
        t_leaf: V,
        st: f64,
        standoff: f64,
        ls: f64,
        ws: f64,
    ) -> Outcome {
        let flush = st < 0.5;
        let blade = Blade {
            length_mm: ls,
            width_mm: ws,
            thickness_mm: self.shape.thickness_mm,
            fold_angle_deg: self.shape.fold_angle_deg,
            inner_curve: self.shape.inner_curve,
            outer_curve: self.shape.outer_curve,
            curl_deg: st * self.shape.curl_deg.min(PITCH_CURL_DEG),
            lift_mm: st * TIP_LIFT_MM,
        };
        // Centre the blade on its root: the build anchors the oval's centre,
        // which sits a quarter length down-slope of the blade's middle.
        let (anchor, anchor_n) = self
            .crown
            .project(
                add(base, scale(t_leaf, 0.25 * ls)),
                nrm,
                1.0 + 0.25 * ls + 1.0,
            )
            .unwrap_or((base, nrm));
        let built = self.attempt(
            anchor,
            anchor_n,
            t_leaf,
            &blade,
            st * standoff,
            flush,
            0.02 + st * TIP_CEIL_RANGE_MM,
            (1.0 - st) * FLUSH_ARCH_MM,
        );
        let (surf, inner, tangent) = match built {
            Ok(x) => x,
            Err(f) => return Outcome::Failed(f),
        };
        // Skewer cull: a blade vertex inside a branch that is outside the
        // crown pierces visibly; inside both, it is hidden and harmless.
        let mut in_branch = vec![false; surf.len()];
        for b in self.bodies {
            if (0..3).any(|k| base[k] < b.lo[k] - ls || base[k] > b.hi[k] + ls) {
                continue;
            }
            for (f, x) in in_branch.iter_mut().zip(b.rays.contains(&surf)) {
                *f |= x;
            }
        }
        let hits: Vec<V> = surf
            .iter()
            .zip(&in_branch)
            .filter(|(_, f)| **f)
            .map(|(p, _)| *p)
            .collect();
        if !hits.is_empty() && self.crown.rays.contains(&hits).iter().any(|x| !x) {
            return Outcome::Branch;
        }
        Outcome::Placed(leaf::solidify(&surf, &inner, self.tris), tangent)
    }

    /// `_attempt_leaf_gen` as the organic placer calls it (flat-seat
    /// fallback, base and tip tucks; its burial probe is off under
    /// `tuck_tip`, so it is not ported). Returns the blade surface, the
    /// root oval (world mm) and the seated tangent.
    #[allow(clippy::too_many_arguments, reason = "the placer's per-leaf knobs")]
    #[allow(
        clippy::too_many_lines,
        reason = "one pipeline, kept in Python's order"
    )]
    fn attempt(
        &self,
        base: V,
        normal: V,
        t0: V,
        blade: &Blade,
        standoff: f64,
        skip_skew: bool,
        tuck_tip_max: f64,
        arch_mm: f64,
    ) -> Result<(Vec<V>, Vec<V>, V), Fail> {
        let len = blade.length_mm;
        let c = &self.crown;
        let tilt = self.seat_tilt(base, normal, t0, len).unwrap_or(0.0);

        // Rigid frame: the oval centred under the anchor, the blade's base
        // 0.75·L up-slope of it, PROTRUSION above.
        let (ca, sa) = (libm::cos(tilt), libm::sin(tilt));
        let tangent = safe_norm(sub(scale(t0, ca), scale(normal, sa)));
        let up = safe_norm(add(scale(normal, ca), scale(t0, sa)));
        let lat = cross(normal, tangent);
        if norm(lat) < 1e-6 {
            return Err(Fail::Error);
        }
        let lat = scale(lat, 1.0 / norm(lat));
        let origin = sub(base, scale(tangent, 0.75 * len));
        let inner: Vec<V> = leaf::oval(normal, tangent, lat, len, blade.width_mm, ROOT_EMBED_MM)
            .into_iter()
            .map(|p| add(p, origin))
            .collect();
        let surf_base = add(origin, scale(normal, PROTRUSION_MM));

        if !c
            .rays
            .contains(&[inner[BASE], inner[TIP]])
            .iter()
            .all(|x| *x)
        {
            return Err(Fail::Oval);
        }

        let mut surf = leaf::surface(surf_base, tangent, up, blade);
        let moved = |surf: &mut Vec<V>, d: V| {
            for p in surf.iter_mut() {
                *p = add(*p, d);
            }
        };

        // Underside arch: 4·s·(1−s)·arch along the seat normal.
        if arch_mm > 0.0 {
            for r in 0..RINGS {
                let s = leaf::station(r);
                let bulge = scale(normal, 4.0 * s * (1.0 - s) * arch_mm);
                for j in 0..STRIDE {
                    surf[r * STRIDE + j] = add(surf[r * STRIDE + j], bulge);
                }
            }
        }

        // Printability skew: slide toward the base until the blade tip
        // clears the oval tip in z.
        let mut skew = 0.0;
        let z_need = if skip_skew {
            0.0
        } else {
            inner[TIP][2] + SKEW_TIP_MARGIN_MM - surf[TIP][2]
        };
        if z_need > 0.0 {
            let tz = tangent[2];
            skew = if tz < -1e-6 {
                z_need / -tz
            } else {
                f64::INFINITY
            };
            if skew > MAX_SKEW_FRAC * len {
                return Err(Fail::Skew);
            }
            moved(&mut surf, scale(tangent, -skew));
        }

        // Belly-dip seat: the tip-half midrib vertex (or the tip) lowest
        // along the leaf normal sits PROTRUSION off the clump.
        let mid = N_LAT / 2;
        let cands = (0..RINGS)
            .filter(|&r| leaf::station(r) > 0.5)
            .map(|r| r * STRIDE + mid)
            .chain([TIP]);
        let mut dip = TIP;
        let mut best = f64::INFINITY;
        for k in cands {
            let d = dot(sub(surf[k], surf[BASE]), up);
            if d < best {
                best = d;
                dip = k;
            }
        }
        let mut drop = 0.0;
        if let Some((cl, _)) = c.clearance(surf[dip], normal) {
            drop = cl - PROTRUSION_MM;
            let nz = normal[2];
            if drop > 0.0 && nz > 1e-6 {
                let slack = surf[TIP][2] - inner[TIP][2] - SKEW_TIP_MARGIN_MM;
                drop = drop.min(slack.max(0.0) / nz);
            }
            moved(&mut surf, scale(normal, -drop));
        }

        // Shingle standoff, after the seat.
        if standoff > 0.0 {
            moved(&mut surf, scale(normal, standoff));
        }

        // Neck gate.
        let net = (standoff - drop).max(0.0);
        if skew.hypot(net) > MAX_NECK_MM {
            return Err(Fail::Neck);
        }

        // Tuck the base slightly in (about the belly dip, ≤ 30°), then cap
        // the tip's clearance (about the base, ≤ 35°).
        let pivot = surf[dip];
        self.tuck(&mut surf, BASE, pivot, normal, tangent, -0.2, 30.0);
        let pivot = surf[BASE];
        self.tuck(&mut surf, TIP, pivot, normal, tangent, tuck_tip_max, 35.0);

        Ok((surf, inner, tangent))
    }

    /// Pitch the blade about `pivot` (axis normal × tangent) so vertex
    /// `idx` comes down toward `target` clearance along −normal.
    #[allow(clippy::too_many_arguments, reason = "the two tucks share it")]
    fn tuck(
        &self,
        surf: &mut [V],
        idx: usize,
        pivot: V,
        normal: V,
        tangent: V,
        target: f64,
        max_deg: f64,
    ) {
        let v = surf[idx];
        let Some((cl, _)) = self.crown.clearance(v, normal) else {
            return;
        };
        let r = norm(sub(v, pivot));
        if !(cl > target && r > 1e-6) {
            return;
        }
        let theta = libm::asin(((cl - target) / r).min(1.0)).min(max_deg.to_radians());
        let lat = safe_norm(cross(normal, tangent));
        let v0 = sub(v, pivot);
        let mut sgn = 1.0;
        for s in [1.0, -1.0] {
            sgn = s;
            if dot(
                sub(rotate_about(v0, lat, s * theta), v0),
                scale(normal, -1.0),
            ) > 0.0
            {
                break;
            }
        }
        for p in surf.iter_mut() {
            *p = add(pivot, rotate_about(sub(*p, pivot), lat, sgn * theta));
        }
    }

    /// `_seat_oval_tilt_gen`: pitch the oval about its centre until both
    /// ends sit equally deep (≤ 3 Newton steps, ≤ 60°); `None` to fall back
    /// to a flat seat.
    fn seat_tilt(&self, p0: V, n0: V, t0: V, len: f64) -> Option<f64> {
        let c0 = sub(p0, scale(n0, ROOT_EMBED_MM));
        let h = 0.25 * len;
        let span = 0.5 * len;
        let mut theta = 0.0_f64;
        for _ in 0..3 {
            let t = sub(scale(t0, libm::cos(theta)), scale(n0, libm::sin(theta)));
            let ends = [sub(c0, scale(t, h)), add(c0, scale(t, h))];
            let inside = self.crown.rays.contains(&ends);
            let dirs = [0, 1].map(|k| if inside[k] { n0 } else { scale(n0, -1.0) });
            let hits = self.crown.rays.first_hits(&ends, &dirs);
            let mut d = [0.0; 2];
            for k in 0..2 {
                let hp = hits[k]?.point;
                let dist = norm(sub(hp, ends[k]));
                d[k] = if inside[k] { dist } else { -dist };
            }
            let imbalance = d[0] - d[1];
            if imbalance.abs() <= 0.05 {
                break;
            }
            theta += libm::asin((imbalance / span).clamp(-1.0, 1.0));
            if theta.abs() > 60.0_f64.to_radians() {
                return None;
            }
        }
        Some(theta)
    }
}

/// Stage timing (a no-op on wasm, which has no clock).
struct Clock {
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
    #[cfg(not(target_arch = "wasm32"))]
    last: std::time::Instant,
}

impl Clock {
    #[cfg(not(target_arch = "wasm32"))]
    fn start() -> Self {
        let t = std::time::Instant::now();
        Self { start: t, last: t }
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn lap(&mut self) -> f64 {
        let t = std::time::Instant::now();
        let s = (t - self.last).as_secs_f64();
        self.last = t;
        s
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn total(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
    #[cfg(target_arch = "wasm32")]
    fn start() -> Self {
        Self {}
    }
    #[cfg(target_arch = "wasm32")]
    fn lap(&mut self) -> f64 {
        0.0
    }
    #[cfg(target_arch = "wasm32")]
    fn total(&self) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_hash_matches() {
        // _hash01(12345, "org-dir", 1) * 2**64, from Python.
        assert_eq!(py_hash(&["12345", "org-dir", "1"]), PY_HASH_12345_ORG_DIR_1);
    }

    const PY_HASH_12345_ORG_DIR_1: u64 = 17_969_432_841_416_999_660;
}
