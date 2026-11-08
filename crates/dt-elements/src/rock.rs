//! Rocks: chiselled convex rocks, the
//! things `rock`, `stone`, `boulder` and `pebble`, and the `scatter` that
//! places many of them.
//!
//! A rock is made at the reference square, in fixed numbers:
//!
//! 1. **body**: the convex hull of points on a non-increasing profile
//!    (dome, slab, block or wedge), with angle-only lumpiness (`make_body`);
//! 2. **facets**: blue-noise seeds of mixed radii on the body; each seed's
//!    tangent plane, tilted up to 14°, cuts a box round the body. The
//!    facet count is every cut that leaves something (`cut_facets`);
//! 3. **chips**: small planes at corners and along sharp edges (dihedral
//!    over 18°), 8 to 16 a rock, each refused if it would remove more than
//!    3% of the volume (`chip_corners_edges`);
//!
//! then scaled to the system (its shape is a photocopy at every scale)
//! and given **grain** in true mm.
//! Facets and chips are half-space cuts of one convex polytope, never
//! booleans, so every rock is closed, convex below its grain, and sits on
//! a flat base with no face pointing down past 30° (`FACET_MIN_NZ`).
//!
//! Placement: a quarter of the placements are
//! clusters of 2 to 4 stones; placements go largest first, 1.5 mm apart
//! (`gap`) and 1 mm inside the rim, 400 darts each. Each stone is seated
//! into the ground below it (dt-kit's `seat`): 0.25 mm under the lowest
//! node within 0.85 of its radius, and at least 0.55 mm under the median
//! there (0.3 + 0.25 mm below a flat 4 mm floor).

use dt_kit::Kind;
use dt_kit::vec3::{cross, dot, norm, sub};
use dt_kit::{
    Convex, Field, Grain, Material, Placed, Rng, Scene, Seed, Spread, Tag, TriMesh, grain, hull,
    scatter, seat, solid,
};

use crate::{Control, Ctx, Element, LookError, Plan, Value, Write};

// body
const HEIGHT_RATIO: (f64, f64) = (0.45, 0.85);
const HEIGHT_CAP_MM: f64 = 9.0;
const HEIGHT_MIN_MM: f64 = 2.5;
const SLAB_HEIGHT: f64 = 0.6;
const ASPECT: (f64, f64) = (1.0, 2.0);
const WEDGE_OFFSET: (f64, f64) = (0.2, 0.45);
const BASE_PTS: (i64, i64) = (14, 22);
const BODY_PTS: (i64, i64) = (60, 120);
const TOP_PTS: (i64, i64) = (1, 3);
const LUMP_AMP: f64 = 0.08;
/// The family mix: dome, slab, block, wedge.
const FAMILIES: [f64; 4] = [0.35, 0.2, 0.2, 0.25];
// facets (:52)
const FACET_SAMPLES: usize = 4000;
const FACET_R_MM: (f64, f64) = (0.9, 2.5);
const FACET_R_EXP: f64 = 1.6;
const FACET_TILT_DEG: f64 = 14.0;
const FACET_OFFSET_MM: (f64, f64) = (-0.15, 0.1);
const FACET_MIN_NZ: f64 = -0.5;
const WIDTH_REF_MM: f64 = 15.0;
// chips (:60)
const SHARP_EDGE_DEG: f64 = 18.0;
const CHIP_MIN_Z_MM: f64 = 0.6;
const CORNER_CHIP_FRAC: f64 = 0.45;
const CORNER_CHIP_MM: (f64, f64) = (0.3, 0.9);
const EDGE_CHIP_FRAC: f64 = 0.35;
const EDGE_CHIP_MM: (f64, f64) = (0.2, 0.55);
const CHIP_TILT_DEG: f64 = 25.0;
const MAX_CHIPS: (i64, i64) = (8, 16);
/// A chip may remove at most this share of the volume.
pub const CHIP_MAX_LOSS: f64 = 0.03;
// clusters and scatter (:78)
const CLUSTER_PROB: f64 = 0.25;
const CLUSTER_STONES: (i64, i64) = (2, 4);
const CLUSTER_SIZE: (f64, f64) = (0.45, 0.9);
const MIN_STONE_MM: f64 = 4.0;
const CLUSTER_DIST: (f64, f64) = (0.35, 0.6);
const RIM_MM: f64 = 1.0;
const DART_TRIES: u32 = 400;
/// The size skew when `mostly` is not written.
const WIDTH_EXP: f64 = 1.4;
// seat
const SEAT_BELOW_MIN_MM: f64 = 0.25;
const SEAT_BELOW_MEDIAN_MM: f64 = 0.55;

/// One rock before its grain: a convex polytope, base at z = 0, centred
/// near the origin, at the size it is built.
#[derive(Clone, Debug)]
pub struct Stone {
    /// The polytope.
    pub convex: Convex,
    /// Facets cut (cuts that left something).
    pub facets: u32,
    /// Chips cut.
    pub chips: u32,
    /// The largest share of volume one chip removed.
    pub chip_loss: f64,
    /// Its width (the drawn size), mm.
    pub width: f64,
}

impl Stone {
    /// Its footprint radius: the farthest vertex from the z axis.
    #[must_use]
    pub fn radius(&self) -> f64 {
        self.convex
            .used()
            .iter()
            .map(|&i| {
                let v = self.convex.vertices[i as usize];
                libm::hypot(v[0], v[1])
            })
            .fold(0.0, f64::max)
    }

    /// Its height.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.convex.bounds().1[2]
    }

    /// Scaled by `s` across and `s * z` up (the normals follow).
    #[must_use]
    pub fn scaled(&self, s: f64, z: f64) -> Self {
        let mut c = self.convex.clone();
        for v in &mut c.vertices {
            *v = [v[0] * s, v[1] * s, v[2] * s * z];
        }
        for f in &mut c.faces {
            let n = [f.normal[0] / s, f.normal[1] / s, f.normal[2] / (s * z)];
            let l = norm(n);
            f.normal = n.map(|x| x / l);
            f.offset = dot(f.normal, c.vertices[f.loop_[0] as usize]);
        }
        Self {
            convex: c,
            width: self.width * s,
            ..self.clone()
        }
    }
}

/// `a` at unit length; left alone when shorter than 1e-9 (a
/// guard, unlike `vec3::unit`).
fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = norm(a);
    if l > 1e-9 { a.map(|x| x / l) } else { a }
}

/// `n` tilted by up to `max_deg` about a random tangent axis (`_tilted`).
fn tilted(rng: &mut Rng, n: [f64; 3], max_deg: f64) -> [f64; 3] {
    let mut t = [
        rng.normal(0.0, 1.0),
        rng.normal(0.0, 1.0),
        rng.normal(0.0, 1.0),
    ];
    let k = dot(t, n);
    t = [t[0] - k * n[0], t[1] - k * n[1], t[2] - k * n[2]];
    let t = unit(t);
    let a = rng.uniform(0.0, max_deg).to_radians();
    let (s, c) = (libm::sin(a), libm::cos(a));
    unit([
        c * n[0] + s * t[0],
        c * n[1] + s * t[1],
        c * n[2] + s * t[2],
    ])
}

/// A Fisher–Yates order of `0..n`.
fn permutation(rng: &mut Rng, n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_possible_wrap
        )]
        let j = rng.int(0, i as i64) as usize;
        v.swap(i, j);
    }
    v
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Dome,
    Slab,
    Block,
    Wedge,
}

fn profile(f: Family, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match f {
        Family::Dome => libm::sqrt((1.0 - libm::pow(t, 2.2)).clamp(0.0, 1.0)),
        Family::Slab => libm::pow((1.0 - libm::pow(t, 2.8)).clamp(0.0, 1.0), 0.45),
        Family::Block => libm::pow((1.0 - libm::pow(t, 3.5)).clamp(0.0, 1.0), 0.4),
        Family::Wedge => libm::pow((1.0 - t).clamp(0.0, 1.0), 0.8),
    }
}

fn footprint_radius(ang: f64, a: f64, b: f64, f: Family) -> f64 {
    let p = if f == Family::Block { 3.0 } else { 2.0 };
    let (c, s) = (libm::fabs(libm::cos(ang)), libm::fabs(libm::sin(ang)));
    1.0 / libm::pow(libm::pow(c / a, p) + libm::pow(s / b, p), 1.0 / p)
}

/// The hull's points.
fn body_points(rng: &mut Rng, d: f64, f: Family, hmin: f64) -> Vec<[f64; 3]> {
    use std::f64::consts::TAU;
    let a = d / 2.0;
    let b = a / rng.uniform(ASPECT.0, ASPECT.1);
    let mut h = (d * rng.uniform(HEIGHT_RATIO.0, HEIGHT_RATIO.1)).min(HEIGHT_CAP_MM);
    if f == Family::Slab {
        h *= SLAB_HEIGHT;
    }
    let h = h.max(hmin);
    let yaw = rng.uniform(0.0, std::f64::consts::PI);
    let wd = rng.uniform(0.0, TAU);
    let wo = if f == Family::Wedge {
        rng.uniform(WEDGE_OFFSET.0, WEDGE_OFFSET.1)
    } else {
        0.0
    };
    let la: Vec<f64> = (0..4)
        .map(|_| rng.normal(0.0, 1.0) * LUMP_AMP / 2.0)
        .collect();
    let lp: Vec<f64> = (0..4).map(|_| rng.uniform(0.0, TAU)).collect();
    let lump = |ang: f64| {
        1.0 + [2.0, 3.0, 4.0, 5.0]
            .iter()
            .enumerate()
            .map(|(k, m)| la[k] * libm::cos(m * ang + lp[k]))
            .sum::<f64>()
    };
    let mut pts = Vec::new();
    let mut add = |t: f64, ang: f64, shrink: f64| {
        let r = footprint_radius(ang - yaw, a, b, f) * profile(f, t) * shrink * lump(ang);
        let (cx, cy) = (wo * t * a * libm::cos(wd), wo * t * b * libm::sin(wd));
        pts.push([cx + r * libm::cos(ang), cy + r * libm::sin(ang), t * h]);
    };
    let n = rng.int(BASE_PTS.0, BASE_PTS.1);
    let mut angs: Vec<f64> = (0..n).map(|_| rng.uniform(0.0, TAU)).collect();
    angs.sort_by(f64::total_cmp);
    for ang in angs {
        let s = rng.uniform(0.94, 1.0);
        add(0.0, ang, s);
    }
    for _ in 0..rng.int(BODY_PTS.0, BODY_PTS.1) {
        let (t, ang, s) = (
            rng.uniform(0.05, 0.97),
            rng.uniform(0.0, TAU),
            rng.uniform(0.93, 1.0),
        );
        add(t, ang, s);
    }
    for _ in 0..rng.int(TOP_PTS.0, TOP_PTS.1) {
        let (t, ang, s) = (
            rng.uniform(0.95, 1.0),
            rng.uniform(0.0, TAU),
            rng.uniform(0.0, 0.4),
        );
        add(t, ang, s);
    }
    pts
}

/// Area-weighted points on a mesh, with their face normals.
fn sample_surface(rng: &mut Rng, m: &TriMesh, n: usize) -> Vec<([f64; 3], [f64; 3])> {
    let tris: Vec<([f64; 3], [f64; 3], [f64; 3])> = m
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| m.positions[i as usize]);
            (a, b, c)
        })
        .collect();
    if tris.is_empty() {
        return Vec::new(); // no surface to sample (a hull that came out empty)
    }
    let mut cum = Vec::with_capacity(tris.len());
    let mut total = 0.0;
    let mut normals = Vec::with_capacity(tris.len());
    for &(a, b, c) in &tris {
        let x = cross(sub(b, a), sub(c, a));
        total += 0.5 * norm(x);
        cum.push(total);
        normals.push(unit(x));
    }
    (0..n)
        .map(|_| {
            let r = rng.random() * total;
            let k = cum.partition_point(|&c| c < r).min(tris.len() - 1);
            let (a, b, c) = tris[k];
            let (mut s, mut t) = (rng.random(), rng.random());
            if s + t > 1.0 {
                (s, t) = (1.0 - s, 1.0 - t);
            }
            let p = [0, 1, 2].map(|i| a[i] + s * (b[i] - a[i]) + t * (c[i] - a[i]));
            (p, normals[k])
        })
        .collect()
}

/// One rock at the reference square, `d` mm wide, family drawn from
/// `mix` (dome, slab, block, wedge weights), up to its grain. A `low`
/// stone is no taller than 0.55 of its width, not at least 2.5 mm (scree
/// and buried grit, not spikes).
///
/// # Errors
/// [`LookError`] if the hull fails (it cannot for these points).
pub fn make_stone(rng: &mut Rng, d: f64, mix: [f64; 4], low: bool) -> Result<Stone, LookError> {
    let total: f64 = mix.iter().sum();
    let mut u = rng.random() * total;
    let mut family = Family::Wedge;
    for (f, w) in [Family::Dome, Family::Slab, Family::Block, Family::Wedge]
        .into_iter()
        .zip(mix)
    {
        if u < w {
            family = f;
            break;
        }
        u -= w;
    }
    let body = hull(&body_points(
        rng,
        d,
        family,
        // scree is small, and a small fallen stone is not a spike (the floor
        // drops to 0.55 of its width)
        if low {
            HEIGHT_MIN_MM.min(0.55 * d)
        } else {
            HEIGHT_MIN_MM
        },
    ))
    .map_err(LookError::build)?;
    // facets
    let scale = libm::sqrt(d / WIDTH_REF_MM);
    let (rmin, rmax) = (FACET_R_MM.0 * scale, FACET_R_MM.1 * scale);
    let mut seeds: Vec<([f64; 3], [f64; 3], f64)> = Vec::new();
    for (p, n) in sample_surface(rng, &body, FACET_SAMPLES) {
        if n[2] < -0.3 {
            continue;
        }
        let r = rmin + (rmax - rmin) * libm::pow(rng.random(), FACET_R_EXP);
        let far = seeds
            .iter()
            .all(|(q, _, rq)| norm(sub(p, *q)) >= 0.5 * (r + rq));
        if far {
            seeds.push((p, n, r));
        }
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in &body.positions {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let pad = 1.0;
    let mut rock = Convex::cuboid(
        [lo[0] - pad, lo[1] - pad, 0.0],
        [hi[0] + pad, hi[1] + pad, hi[2] + pad],
    );
    let mut facets = 0;
    for (p, n, _) in &seeds {
        let n2 = tilted(rng, *n, FACET_TILT_DEG);
        let off = rng.uniform(FACET_OFFSET_MM.0, FACET_OFFSET_MM.1);
        if n2[2] < FACET_MIN_NZ {
            continue;
        }
        if let Some(c) = rock.cut(n2, dot(n2, *p) + off) {
            rock = c;
            facets += 1;
        }
    }
    let rock = rock.compact();
    // chips
    let mut planes: Vec<([f64; 3], [f64; 3], f64)> = Vec::new();
    let corners = rock.corners();
    for vi in permutation(rng, corners.len()) {
        let (v, n) = corners[vi];
        if rng.random() > CORNER_CHIP_FRAC || v[2] < CHIP_MIN_Z_MM {
            continue;
        }
        let depth = rng.uniform(CORNER_CHIP_MM.0, CORNER_CHIP_MM.1) * scale;
        planes.push((v, unit(n), depth));
    }
    for e in rock.edges() {
        if e.angle <= SHARP_EDGE_DEG.to_radians() || rng.random() > EDGE_CHIP_FRAC {
            continue;
        }
        let [pa, pb] = e.ends;
        if pa[2].min(pb[2]) < CHIP_MIN_Z_MM {
            continue;
        }
        let t = rng.uniform(0.2, 0.8);
        let n = unit([0, 1, 2].map(|k| e.normals[0][k] + e.normals[1][k]));
        let p = [0, 1, 2].map(|k| pa[k] + t * (pb[k] - pa[k]));
        let depth = rng.uniform(EDGE_CHIP_MM.0, EDGE_CHIP_MM.1) * scale;
        planes.push((p, n, depth));
    }
    let cap = rng.int(MAX_CHIPS.0, MAX_CHIPS.1);
    let mut rock = rock;
    let (mut chips, mut chip_loss) = (0, 0.0f64);
    for i in permutation(rng, planes.len()) {
        if chips >= cap {
            break;
        }
        let (p, n, depth) = planes[i];
        let n2 = tilted(rng, n, CHIP_TILT_DEG);
        if n2[2] < FACET_MIN_NZ {
            continue;
        }
        let Some(c) = rock.cut(n2, dot(n2, p) - depth) else {
            continue;
        };
        let (v0, v1) = (rock.volume(), c.volume());
        if v1 < (1.0 - CHIP_MAX_LOSS) * v0 {
            continue;
        }
        chip_loss = chip_loss.max(1.0 - v1 / v0);
        rock = c;
        chips += 1;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(Stone {
        convex: rock.compact(),
        facets,
        chips: chips as u32,
        chip_loss,
        width: d,
    })
}

/// A placement: one stone, or a cluster round a main stone (offsets from
/// the placement's centre, reference mm), and its clearance radius.
#[derive(Clone, Debug)]
pub struct Item {
    /// Its stones and their offsets.
    pub stones: Vec<([f64; 2], Stone)>,
    /// Its radius for spacing.
    pub r: f64,
    /// How much of its width sinks under the ground, a share range.
    pub bury: (f64, f64),
}

/// One stone, or (a quarter of the time when `clusters`) a cluster of 2
/// to 4; `low` as for [`make_stone`].
///
/// # Errors
/// As [`make_stone`].
pub fn make_item(
    rng: &mut Rng,
    d: f64,
    mix: [f64; 4],
    clusters: bool,
    low: bool,
) -> Result<Item, LookError> {
    use std::f64::consts::TAU;
    let main = make_stone(rng, d, mix, low)?;
    if !clusters || rng.random() >= CLUSTER_PROB {
        return Ok(Item {
            stones: vec![([0.0, 0.0], main)],
            r: d / 2.0,
            bury: (0.0, 0.0),
        });
    }
    let k = rng.int(CLUSTER_STONES.0, CLUSTER_STONES.1);
    let mut stones = vec![([0.0, 0.0], main)];
    let mut r = d / 2.0;
    let ang0 = rng.uniform(0.0, TAU);
    for j in 1..k {
        let dj = (d * rng.uniform(CLUSTER_SIZE.0, CLUSTER_SIZE.1)).max(MIN_STONE_MM);
        let s = make_stone(rng, dj, mix, low)?;
        #[allow(clippy::cast_precision_loss)]
        let ang = ang0 + j as f64 * TAU / k as f64 + rng.uniform(-0.5, 0.5);
        let dist = (d / 2.0 + dj / 2.0) * rng.uniform(CLUSTER_DIST.0, CLUSTER_DIST.1);
        stones.push(([dist * libm::cos(ang), dist * libm::sin(ang)], s));
        r = r.max(dist + dj / 2.0);
    }
    Ok(Item {
        stones,
        r,
        bury: (0.0, 0.0),
    })
}

/// What one rock thing is: its kind, size range (reference mm), skew and
/// family mix, and its weight in a scatter.
#[derive(Clone, Debug, PartialEq)]
pub struct Thing {
    /// `rock`, `stone`, `boulder` or `pebble`.
    pub kind: String,
    /// Its size range.
    pub size: (f64, f64),
    /// `lo + (hi − lo)·u^exponent`.
    pub exponent: f64,
    /// Dome, slab, block, wedge weights.
    pub mix: [f64; 4],
    /// Its share of a scatter's mix.
    pub weight: f64,
    /// How much of its width sinks under the ground, a share range.
    pub buried: (f64, f64),
}

/// The things a rock element builds, with their default size (dt-doc's
/// vocabulary: `rock` is 6..15).
#[must_use]
pub fn is_thing(name: &str) -> bool {
    matches!(name, "rock" | "stone" | "boulder" | "pebble")
}

impl Thing {
    /// From a thing's controls (defaults already filled in).
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take or a bad size.
    pub fn new(kind: &str, controls: &[Control], weight: f64) -> Result<Self, LookError> {
        let mut t = Self {
            kind: kind.to_owned(),
            size: (6.0, 15.0),
            exponent: WIDTH_EXP,
            mix: FAMILIES,
            weight,
            buried: (0.0, 0.0),
        };
        for c in controls {
            match (c.name.as_str(), &c.value) {
                ("size", Value::Range(a, b)) => t.size = (*a, *b),
                ("size", Value::Number(a)) => t.size = (*a, *a),
                ("buried", v) => {
                    let share = |x: f64| if x > 1.0 { x / 100.0 } else { x };
                    t.buried = match v {
                        Value::Number(a) => (share(*a), share(*a)),
                        Value::Range(a, b) => (share(*a), share(*b)),
                        Value::Word(_) => (0.0, 0.0),
                    };
                }
                ("mostly.exponent", Value::Number(e)) => t.exponent = *e,
                ("shape", Value::Word(w)) => {
                    t.mix = match w.as_str() {
                        "rounded" => [0.85, 0.15, 0.0, 0.0],
                        "angular" => [0.0, 0.0, 0.45, 0.55],
                        "flat" => [0.0, 1.0, 0.0, 0.0],
                        _ => FAMILIES,
                    }
                }
                _ => {}
            }
        }
        Ok(t)
    }

    fn draw(&self, rng: &mut Rng) -> f64 {
        let (lo, hi) = self.size;
        lo + (hi - lo) * libm::pow(rng.random(), self.exponent)
    }
}

/// What the build notes: per stone.
#[derive(Clone, Debug, Default)]
struct Tally {
    placements: usize,
    clusters: usize,
    facets: Vec<u32>,
    chips: Vec<u32>,
    chip_loss: f64,
    widths: Vec<f64>,
}

/// Build placed items into the scene: grain, seat on `ground`, add as
/// rock solids owned by `owner`.
fn add_items(
    scene: &mut Scene,
    owner: &str,
    seed: Seed,
    items: &[(&Item, [f64; 2], f64)],
    tally: &mut Tally,
) -> Result<(), LookError> {
    let s = scene.frame().scale();
    let ground = scene.land();
    let n: usize = items.iter().map(|(it, _, _)| it.stones.len()).sum();
    #[allow(clippy::cast_possible_truncation)]
    let slots = scene
        .slots(n as u32, Material::Rock)
        .map_err(LookError::build)?;
    let g = Grain::default();
    let mut k = 0u32;
    let ticks = scene.progress().counter("stones", n, 0.0, 1.0);
    for (item, at, yaw) in items {
        tally.placements += 1;
        tally.clusters += usize::from(item.stones.len() > 1);
        let (sn, cs) = (libm::sin(yaw.to_radians()), libm::cos(yaw.to_radians()));
        for (off, stone) in &item.stones {
            let st = stone.scaled(s, 1.0);
            let o = [off[0] * s, off[1] * s];
            let p = [at[0] + cs * o[0] - sn * o[1], at[1] + sn * o[0] + cs * o[1]];
            let r = st.radius();
            let mut z0 = ground.as_ref().map_or(0.0, |gr| {
                seat(gr, p, r, SEAT_BELOW_MIN_MM, SEAT_BELOW_MEDIAN_MM)
            });
            if item.bury.1 > 0.0 {
                let mut br = Rng::new(seed.key("bury").index(u64::from(k)));
                z0 -= br.uniform(item.bury.0, item.bury.1) * st.width;
            }
            let mesh = grain(&st.convex, &g, seed.index(u64::from(k)).value())
                .map_err(LookError::build)?;
            let sol = solid(slots.get(k), &mesh)
                .map_err(LookError::build)?
                .turn(*yaw, 1.0)
                .translate([p[0], p[1], z0]);
            scene.add(
                Tag {
                    kind: Kind::Rock,
                    owner: owner.to_owned(),
                },
                sol,
            );
            tally.facets.push(stone.facets);
            tally.chips.push(stone.chips);
            tally.chip_loss = tally.chip_loss.max(stone.chip_loss);
            tally.widths.push(st.width);
            k += 1;
            ticks.tick();
        }
    }
    Ok(())
}

fn note(scene: &mut Scene, t: &Tally) {
    #[allow(clippy::cast_precision_loss)]
    let mean = |v: &[u32]| v.iter().map(|&x| f64::from(x)).sum::<f64>() / v.len().max(1) as f64;
    let min = |v: &[u32]| f64::from(v.iter().copied().min().unwrap_or(0));
    let max = |v: &[u32]| f64::from(v.iter().copied().max().unwrap_or(0));
    #[allow(clippy::cast_precision_loss)]
    for (k, v) in [
        ("placements", t.placements as f64),
        ("clusters", t.clusters as f64),
        ("stones", t.facets.len() as f64),
        ("facets_min", min(&t.facets)),
        ("facets_max", max(&t.facets)),
        ("facets_mean", mean(&t.facets)),
        ("chips_min", min(&t.chips)),
        ("chips_max", max(&t.chips)),
        ("chip_loss_max", t.chip_loss),
        (
            "width_min",
            t.widths.iter().copied().fold(f64::INFINITY, f64::min),
        ),
        ("width_max", t.widths.iter().copied().fold(0.0, f64::max)),
    ] {
        scene.note(k, v);
    }
}

/// Discs (x, y, r) round every rock already in the scene.
fn rocks_in(scene: &Scene) -> Vec<[f64; 3]> {
    scene
        .stands(&[Kind::Rock])
        .iter()
        .map(|s| [s.at[0], s.at[1], s.r])
        .collect()
}

/// A rock thing on its own: one rock, at `at=` or somewhere in its area.
pub struct Single {
    thing: Thing,
}

impl Single {
    /// From its controls.
    ///
    /// # Errors
    /// As [`Thing::new`].
    pub fn new(kind: &str, controls: &[Control]) -> Result<Self, LookError> {
        Ok(Self {
            thing: Thing::new(kind, controls, 1.0)?,
        })
    }
}

impl Element for Single {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Rock),
            reads: Vec::new(),
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let mut rng = Rng::new(ctx.seed.key("rock"));
        let d = self.thing.draw(&mut rng);
        let item = make_item(&mut rng, d, self.thing.mix, false, false)?;
        let yaw = rng.uniform(0.0, 360.0);
        let s = scene.frame().scale();
        let sq = scene.frame().square_mm;
        let at = match ctx.at.map(|[[x0, x1], [y0, y1]]| {
            // a range is drawn from the entry's seed
            let x = if x1 > x0 { rng.uniform(x0, x1) } else { x0 };
            let y = if y1 > y0 { rng.uniform(y0, y1) } else { y0 };
            [x * sq, y * sq]
        }) {
            Some(a) => {
                // a stone never stands past the tile's edge: it is
                // clipped by no one, so it would print past its neighbour
                let [w, h] = scene.frame().size_mm();
                let m = item.r * s + RIM_MM * s;
                if a[0] < m || a[1] < m || a[0] > w - m || a[1] > h - m {
                    return Err(LookError::general(
                        format!(
                            "a {} {d:.1} mm wide at this point stands past the tile's edge",
                            self.thing.kind
                        ),
                        format!(
                            "keep it {:.2} squares from every edge",
                            m / scene.frame().square_mm
                        ),
                    ));
                }
                a
            }
            None => {
                let placed = place(scene, ctx, &mut rng, &[item.r * s], 1.5 * s);
                let Some(p) = placed.first() else {
                    return Err(LookError::build(format!(
                        "no room for a {} {d:.1} mm wide here",
                        self.thing.kind
                    )));
                };
                p.at
            }
        };
        let mut t = Tally::default();
        add_items(scene, ctx.path, ctx.seed, &[(&item, at, yaw)], &mut t)?;
        note(scene, &t);
        Ok(())
    }
}

/// Dart-throw radii (world mm) into the entry's area, largest first.
fn place(scene: &Scene, ctx: &Ctx<'_>, rng: &mut Rng, radii: &[f64], gap: f64) -> Vec<Placed> {
    let frame = *scene.frame();
    let area = ctx.area;
    let step = frame.step_mm();
    let inside = |x: f64, y: f64| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (
            (libm::round(x / step).max(0.0) as usize).min(area.nx - 1),
            (libm::round(y / step).max(0.0) as usize).min(area.ny - 1),
        );
        if area.bits[j * area.nx + i] { 1.0 } else { 0.0 }
    };
    let spread = Spread {
        tries: DART_TRIES,
        inset: [RIM_MM * frame.scale(), 1.0],
        confine: true,
        gap,
        factor: 1.0,
    };
    scatter(&frame, rng, radii, &spread, &inside, &rocks_in(scene))
}

/// Scree: each item (largest first) lands near the foot of `foot`, in a
/// ground cell chosen by weight (shattered patches get more), its centre
/// `d (½ − lean) + exponential(reach)` out from the face, clear of every
/// other item and rock.
fn place_at_foot(
    scene: &Scene,
    ctx: &Ctx<'_>,
    rng: &mut Rng,
    foot: &crate::Foot,
    items: &[Item],
    kinds: &[&str],
    reach: f64,
) -> Vec<Placed> {
    let frame = *scene.frame();
    let s = frame.scale();
    let step = frame.step_mm();
    let size = frame.size_mm();
    let area = ctx.area;
    let total: f64 = foot.cells.iter().map(|c| c.2).sum();
    if foot.cells.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| items[b].r.total_cmp(&items[a].r));
    let mut out: Vec<Placed> = Vec::new();
    let existing = rocks_in(scene);
    for i in order {
        let it = &items[i];
        let (d, r) = (2.0 * it.r * s, it.r * s);
        let mut u = rng.random() * total;
        let mut cell = &foot.cells[foot.cells.len() - 1];
        for c in &foot.cells {
            if u < c.2 {
                cell = c;
                break;
            }
            u -= c.2;
        }
        let rch = -(if kinds[i] == "boulder" { 0.3 } else { 1.0 })
            * reach
            * s
            * libm::log(1.0 - rng.random());
        let off = d * (0.5 - SCREE_LEAN) + rch;
        let at = [cell.0[0] + cell.1[0] * off, cell.0[1] + cell.1[1] * off];
        if !(r + 0.3 < at[0]
            && at[0] < size[0] - r - 0.3
            && r + 0.3 < at[1]
            && at[1] < size[1] - r - 0.3)
        {
            continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (gi, gj) = (
            (libm::round(at[0] / step).max(0.0) as usize).min(area.nx - 1),
            (libm::round(at[1] / step).max(0.0) as usize).min(area.ny - 1),
        );
        if !area.bits[gj * area.nx + gi] {
            continue;
        }
        if out
            .iter()
            .any(|p| libm::hypot(at[0] - p.at[0], at[1] - p.at[1]) < SCREE_GAP * (d + 2.0 * p.r))
        {
            continue;
        }
        if existing
            .iter()
            .any(|e| libm::hypot(at[0] - e[0], at[1] - e[1]) < e[2] + r)
        {
            continue;
        }
        out.push(Placed { index: i, at, r });
    }
    out
}

/// `scatter { rock … }`: many rock things, by count or per square.
pub struct Scatter {
    count: Option<(f64, f64)>,
    per_square: (f64, f64),
    gap: f64,
    /// `against=`: how far from the foot stones lie, mean mm.
    reach: f64,
    things: Vec<Thing>,
}

/// Placements: 9 to 12 per 2×2, so 2.25 to 3 a square.
const PER_SQUARE: (f64, f64) = (2.25, 3.0);
const GAP_MM: f64 = 1.5;
// scree: fallen stones at a cliff's foot
const SCREE_REACH_MM: f64 = 4.0;
const SCREE_LEAN: f64 = 0.35;
const SCREE_GAP: f64 = 0.38;
/// Scree takes the cliff's fragments: angular, no domes.
const SCREE_MIX: [f64; 4] = [0.0, 0.2, 0.45, 0.35];

impl Scatter {
    /// From its controls and its children (each a thing and its weight).
    ///
    /// # Errors
    /// [`LookError`] for a child that is not a rock thing, or bad controls.
    pub fn new(controls: &[Control], children: &[crate::Child]) -> Result<Self, LookError> {
        let range = |v: &Value| match v {
            Value::Number(a) => Some((*a, *a)),
            Value::Range(a, b) => Some((*a, *b)),
            Value::Word(_) => None,
        };
        let mut s = Self {
            count: None,
            per_square: PER_SQUARE,
            gap: GAP_MM,
            reach: SCREE_REACH_MM,
            things: Vec::new(),
        };
        for c in controls {
            match c.name.as_str() {
                "count" => s.count = range(&c.value),
                "per-square" => s.per_square = range(&c.value).unwrap_or(PER_SQUARE),
                "reach" => {
                    if let Value::Number(r) = c.value {
                        s.reach = r;
                    }
                }
                "gap" => {
                    if let Value::Number(g) = c.value {
                        s.gap = g;
                    }
                }
                _ => {}
            }
        }
        for ch in children {
            if !is_thing(&ch.name) {
                return Err(LookError::general(
                    format!("a scatter of `{}` is not built yet", ch.name),
                    "a scatter takes rock, stone, boulder and pebble",
                ));
            }
            s.things
                .push(Thing::new(&ch.name, &ch.controls, ch.weight)?);
        }
        if s.things.is_empty() {
            return Err(LookError::general(
                "a scatter needs a child to say what it scatters",
                "write `scatter per-square=2 { rock }`",
            ));
        }
        Ok(s)
    }
}

impl Element for Scatter {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Rock),
            reads: Vec::new(),
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let frame = *scene.frame();
        let s = frame.scale();
        let mut rng = Rng::new(ctx.seed.key("scatter"));
        // gathered against a foot (scree), or spread over the area
        let foot = match ctx.against {
            Some(kind) => Some(ctx.foot(kind).ok_or_else(|| {
                LookError::general(
                    format!("nothing in this tile to gather against: no `{kind}`"),
                    format!("add a `{kind}` to the stack, or remove `against=`"),
                )
            })?),
            None => None,
        };
        // squares the area covers
        #[allow(clippy::cast_precision_loss)]
        let squares = foot.map_or_else(
            || {
                ctx.area.count() as f64
                    / (frame.cells_per_square as f64 * frame.cells_per_square as f64)
            },
            |f| f.squares,
        );
        let n = match self.count {
            Some((a, b)) => rng.uniform(a, b.max(a + f64::EPSILON)),
            None => {
                rng.uniform(
                    self.per_square.0,
                    self.per_square.1.max(self.per_square.0 + f64::EPSILON),
                ) * squares
            }
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = libm::round(n).clamp(0.0, 10_000.0) as usize;
        let total: f64 = self.things.iter().map(|t| t.weight).sum();
        let mut items = Vec::with_capacity(n);
        let mut kinds: Vec<&str> = Vec::with_capacity(n);
        for i in 0..n {
            let mut r = Rng::new(ctx.seed.key("item").index(i as u64));
            let mut u = r.random() * total;
            let thing = self
                .things
                .iter()
                .find(|t| {
                    let hit = u < t.weight;
                    u -= t.weight;
                    hit
                })
                .unwrap_or(&self.things[self.things.len() - 1]);
            let d = thing.draw(&mut r);
            let mut item = match foot {
                Some(_) => make_item(&mut r, d, SCREE_MIX, false, true)?,
                None => make_item(&mut r, d, thing.mix, true, thing.buried.1 > 0.0)?,
            };
            item.bury = thing.buried;
            items.push(item);
            kinds.push(thing.kind.as_str());
        }
        let radii: Vec<f64> = items.iter().map(|it| it.r * s).collect();
        let placed = match foot {
            Some(foot) => place_at_foot(scene, ctx, &mut rng, foot, &items, &kinds, self.reach),
            None => place(scene, ctx, &mut rng, &radii, self.gap * s),
        };
        let posed: Vec<(&Item, [f64; 2], f64)> = placed
            .iter()
            .map(|p| (&items[p.index], p.at, rng.uniform(0.0, 360.0)))
            .collect();
        let mut t = Tally::default();
        add_items(scene, ctx.path, ctx.seed, &posed, &mut t)?;
        note(scene, &t);
        Ok(())
    }
}

/// The dirt's stones: a few
/// rocks flattened to 0.65 of their height, bedded in a soil mound into
/// the dirt's own field. Returns their discs (x, y, r) for the pebbles to
/// avoid; adds the solids to the scene.
///
/// # Errors
/// As [`make_stone`].
/// How far a dirt's bedded stones keep from the border of its area, mm at
/// the reference square.
const AREA_CLEAR_MM: f64 = 7.0;

pub fn dirt_stones(
    scene: &mut Scene,
    owner: &str,
    seed: Seed,
    z: &mut Field,
    rim: &Field,
    per_2x2: (i64, i64),
    area: &dt_kit::Mask,
) -> Result<Vec<[f64; 3]>, LookError> {
    const D_MM: (f64, f64) = (4.5, 7.5);
    const SINK: f64 = 0.4;
    const Z_SCALE: f64 = 0.65;
    const MOUND_MM: f64 = 0.3;
    const RIM: f64 = 2.0;
    const GAP: f64 = 4.0;
    const TRIES: u32 = 300;
    const RIM_TEX_MM: f64 = 1.0;
    let frame = *scene.frame();
    let [w, h] = frame.size_mm();
    let n_sq = w * h / (frame.square_mm * frame.square_mm);
    let mut rng = Rng::new(seed.key("stones"));
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let n = libm::round(rng.int(per_2x2.0, per_2x2.1) as f64 * n_sq / 4.0) as usize;
    let mut stones = Vec::with_capacity(n);
    for i in 0..n {
        let mut r = Rng::new(seed.key("stone").index(i as u64));
        let d = D_MM.0 + (D_MM.1 - D_MM.0) * libm::pow(r.random(), 1.5);
        let st = make_stone(&mut r, d, FAMILIES, false)?.scaled(1.0, Z_SCALE);
        stones.push((st, r.uniform(0.0, 360.0)));
    }
    let radii: Vec<f64> = stones.iter().map(|(s, _)| s.radius()).collect();
    let spread = Spread {
        tries: TRIES,
        inset: [RIM, 1.0],
        confine: true,
        gap: GAP,
        factor: 1.0,
    };
    // clear of the area's border by a bank's width and a stone: the ground
    // beside the area may fall away from it
    let outside: Vec<bool> = area.bits.iter().map(|b| !b).collect();
    let clear = if outside.iter().any(|&b| b) {
        Some(dt_kit::distance(&frame, &outside).0)
    } else {
        None
    };
    let keep = AREA_CLEAR_MM * frame.scale() + D_MM.1 / 2.0;
    let inside = |x: f64, y: f64| {
        let (i, j) = z.node(x, y);
        let k = j * area.nx + i;
        let ok = area.bits[k] && clear.as_ref().is_none_or(|c| c.z[k] >= keep);
        if ok { 1.0 } else { 0.0 }
    };
    let placed = scatter(&frame, &mut rng, &radii, &spread, &inside, &[]);
    #[allow(clippy::cast_possible_truncation)]
    let slots = scene
        .slots(placed.len() as u32, Material::Rock)
        .map_err(LookError::build)?;
    let g = Grain::default();
    let mut discs = Vec::new();
    for (k, p) in placed.iter().enumerate() {
        let (st, yaw) = &stones[p.index];
        let r = p.r;
        let [x, y] = p.at;
        let (is, js) = z.window(x, y, 0.7 * r + 2.5 * r);
        for j in js {
            for i in is.clone() {
                let d = libm::hypot(z.x(i) - x, z.y(j) - y);
                let e = (d - 0.7 * r).max(0.0);
                let kk = j * z.nx + i;
                z.z[kk] += MOUND_MM
                    * libm::exp(-e * e / (2.0 * (0.5 * r) * (0.5 * r)))
                    * dt_kit::smooth01((rim.z[kk] - RIM_TEX_MM) / 1.0);
            }
        }
        let z0 = seat(z, p.at, r, 0.25, SINK * st.height());
        #[allow(clippy::cast_possible_truncation)]
        let k32 = k as u32;
        let mesh = grain(
            &st.convex,
            &g,
            seed.key("grain").index(u64::from(k32)).value(),
        )
        .map_err(LookError::build)?;
        let sol = solid(slots.get(k32), &mesh)
            .map_err(LookError::build)?
            .turn(*yaw, 1.0)
            .translate([x, y, z0]);
        scene.add(
            Tag {
                kind: Kind::Rock,
                owner: owner.to_owned(),
            },
            sol,
        );
        discs.push([x, y, r]);
    }
    Ok(discs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stone_is_faceted_chipped_and_grounded() {
        for i in 0..6 {
            let mut rng = Rng::new(Seed::new(9).index(i));
            let s = make_stone(&mut rng, 6.0 + 1.8 * i as f64, FAMILIES, false).unwrap();
            assert!(s.convex.is_closed());
            assert!((10..=200).contains(&s.facets), "facets {}", s.facets);
            assert!(s.chips <= 16);
            assert!(s.chip_loss <= CHIP_MAX_LOSS + 1e-12);
            let (lo, _) = s.convex.bounds();
            assert!(lo[2].abs() < 1e-9);
            for f in &s.convex.faces {
                assert!(f.normal[2] >= FACET_MIN_NZ - 1e-9 || f.normal[2] < -0.999);
            }
        }
    }

    #[test]
    fn scaling_keeps_the_planes() {
        let mut rng = Rng::new(Seed::new(3));
        let s = make_stone(&mut rng, 10.0, FAMILIES, false)
            .unwrap()
            .scaled(0.7, 0.65);
        assert!(s.convex.is_closed());
        for f in &s.convex.faces {
            for &v in &f.loop_ {
                let p = s.convex.vertices[v as usize];
                assert!((dot(f.normal, p) - f.offset).abs() < 1e-9);
            }
        }
    }
}
