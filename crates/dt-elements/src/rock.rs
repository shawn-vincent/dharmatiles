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
//! into the land below it (dt-kit's `Land::seat`, on the land as laid, grain
//! and all): 0.25 mm under the lowest node its foot can touch, and at least
//! 0.55 mm under the median there. It is measured after: `seat_gap_max` is
//! the most any foot hovers over the land, `seat_gap_stones` how many do.

use dt_kit::Kind;
use dt_kit::vec3::{cross, dot, norm, sub};
use dt_kit::{
    Convex, Disc, Grain, Material, Placed, Rng, Scene, SeatRule, Seed, Spread, Tag, TriMesh, grain,
    hull, scatter, solid,
};

use crate::read::Settings;
use crate::{Control, Ctx, Draw, Element, LookError, Plan, Write};

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
// seat
const SEAT_RULE: SeatRule = SeatRule {
    below_min: 0.25,
    below_median: 0.55,
};
/// A foot more than this far over the land is hovering: the lift's error
/// bound.
const HOVER_TOL_MM: f64 = dt_kit::LIFT_TOL_MM;

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
    pub bury: Draw,
    /// The tile's formula for its `top=` (a cloud over its place), when it
    /// has one: each stone's peak stands at the cloud's roll there.
    pub top: Option<crate::Formula>,
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
            bury: Draw::flat(0.0, 0.0),
            top: None,
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
        bury: Draw::flat(0.0, 0.0),
        top: None,
    })
}

/// What one rock thing is: its kind, size range (reference mm) with its
/// odds, family mix, and its weight in a scatter.
#[derive(Clone, Debug, PartialEq)]
pub struct Thing {
    /// `rock`, `stone`, `boulder` or `pebble`.
    pub kind: String,
    /// Its size range, and the odds along it.
    pub size: Draw,
    /// Dome, slab, block, wedge weights.
    pub mix: [f64; 4],
    /// Its share of a scatter's mix.
    pub weight: f64,
    /// How much of its width sinks under the ground, a share range.
    pub buried: Draw,
    /// Its `top=`: the tile's formula for it, when written.
    pub top: Option<crate::Formula>,
}

/// The things a rock element builds, with their default size (dt-doc's
/// vocabulary: `rock` is 6..15).
#[must_use]
pub fn is_thing(name: &str) -> bool {
    matches!(name, "rock" | "stone" | "boulder" | "pebble")
}

/// A share as the fraction the checked percent form lowers to.  A bare
/// number never reaches here from a document (`dt::percent` refuses it
/// when it is read); a value outside 0% to 100% is refused rather than
/// read as a percent.
fn share(d: Draw) -> Result<Draw, LookError> {
    if d.lo() < 0.0 || d.hi() > 1.0 {
        let mut e = LookError::control(
            "buried",
            format!(
                "`buried` is a share from 0% to 100%, not {}% to {}%",
                d.lo() * 100.0,
                d.hi() * 100.0
            ),
            "write a percent such as \"30%\" or \"20%..50%\"".into(),
        );
        e.code = Some("dt::percent");
        return Err(e);
    }
    Ok(d)
}

impl Thing {
    /// From a thing's controls (defaults already filled in).
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take or a bad size.
    pub fn new(kind: &str, controls: &[Control], weight: f64) -> Result<Self, LookError> {
        let mut t = Self {
            kind: kind.to_owned(),
            size: Draw::flat(6.0, 15.0),
            mix: FAMILIES,
            weight,
            buried: Draw::flat(0.0, 0.0),
            top: None,
        };
        let s = Settings::new(controls);
        if let Some(c) = s.control("top") {
            match &c.value {
                crate::Value::Formula(f) => t.top = Some(f.clone()),
                _ => {
                    return Err(LookError::control(
                        "top",
                        format!("a {kind}'s top is a formula the tile works out where it stands"),
                        "write `top=\"$bottom + [4dmm .. 9dmm]\"`".into(),
                    ));
                }
            }
        }
        t.size = s.dist_or("size", t.size)?;
        if let Some(r) = s.dist("buried")? {
            t.buried = share(r)?;
        }
        if let Some(w) = s.word("shape")? {
            t.mix = match w {
                "rounded" => [0.85, 0.15, 0.0, 0.0],
                "angular" => [0.0, 0.0, 0.45, 0.55],
                "flat" => [0.0, 1.0, 0.0, 0.0],
                _ => FAMILIES,
            };
        }
        Ok(t)
    }

    /// One draw of the size: flat unless the size is weighted.
    pub(crate) fn draw(&self, rng: &mut Rng) -> f64 {
        self.size.draw(rng)
    }
}

/// What the build notes: per stone.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tally {
    placements: usize,
    clusters: usize,
    facets: Vec<u32>,
    chips: Vec<u32>,
    chip_loss: f64,
    widths: Vec<f64>,
    hover_max: f64,
    hovering: usize,
    /// Stones whose peak a `top=` cloud set.
    topped: usize,
}

/// Build placed items into the scene: grain, seat on `ground`, add as
/// rock solids owned by `owner`.
pub(crate) fn add_items(
    scene: &mut Scene,
    owner: &dt_kit::EntryId,
    seed: Seed,
    items: &[(&Item, [f64; 2], f64)],
    crowns: Option<&dyn crate::CrownTops>,
    tally: &mut Tally,
) -> Result<(), LookError> {
    let s = scene.frame().scale();
    let slack = scene.frame().step_mm();
    let n: usize = items.iter().map(|(it, _, _)| it.stones.len()).sum();
    #[allow(clippy::cast_possible_truncation)]
    let slots = scene
        .slots(n as u32, Material::Rock)
        .map_err(LookError::build)?;
    let g = Grain::default();
    let ticks = scene.progress().counter("stones", n, 0.0, 1.0);
    // seated on the land as it is laid, grain and all; a tile with no land
    // under it is the base
    let land = scene.land_view();
    // each stone's place and foot first: a `top=` rolls over all of them
    struct Foot {
        stone: Stone,
        item: usize,
        p: [f64; 2],
        yaw: f64,
        /// The land at its middle, mm.
        bottom: f64,
        /// Where its foot is set, mm.
        z0: f64,
        k: u32,
    }
    let mut feet: Vec<Foot> = Vec::with_capacity(n);
    let mut k = 0u32;
    for (ii, (item, at, yaw)) in items.iter().enumerate() {
        tally.placements += 1;
        tally.clusters += usize::from(item.stones.len() > 1);
        let (sn, cs) = (libm::sin(yaw.to_radians()), libm::cos(yaw.to_radians()));
        for (off, stone) in &item.stones {
            let st = stone.scaled(s, 1.0);
            let o = [off[0] * s, off[1] * s];
            let p = [at[0] + cs * o[0] - sn * o[1], at[1] + sn * o[0] + cs * o[1]];
            // the disc a cell wider than the stone, so the land under every
            // cell its foot touches is seen
            let disc = Disc {
                at: p,
                r: st.radius() + slack,
            };
            let mut z0 = land.as_ref().map_or(0.0, |l| l.seat(disc, SEAT_RULE).z);
            if item.bury.hi() > 0.0 {
                let mut br = Rng::new(seed.key("bury").index(u64::from(k)));
                z0 -= item.bury.draw(&mut br) * st.width;
            }
            feet.push(Foot {
                bottom: land.as_ref().map_or(0.0, |l| l.top(p[0], p[1])),
                stone: st,
                item: ii,
                p,
                yaw: *yaw,
                z0,
                k,
            });
            tally.facets.push(stone.facets);
            tally.chips.push(stone.chips);
            tally.chip_loss = tally.chip_loss.max(stone.chip_loss);
            tally.widths.push(feet[feet.len() - 1].stone.width);
            k += 1;
        }
    }
    // a `top=` stands each stone's peak at its cloud's roll
    let frame = *scene.frame();
    let mut marks = scene.collecting().clouds.then(Vec::new);
    let peaks = peaks_under_clouds(
        &feet
            .iter()
            .map(|f| (f.item, f.p, f.stone.radius(), f.bottom, f.z0, f.k))
            .collect::<Vec<_>>(),
        items,
        seed,
        &frame,
        crowns,
        owner,
        marks.as_mut(),
    )?;
    let mut built = Vec::with_capacity(n);
    let spots: Vec<([f64; 2], f64)> = feet.iter().map(|f| (f.p, f.stone.radius())).collect();
    for (f, peak) in feet.into_iter().zip(peaks) {
        let st = match peak {
            Some(h) => {
                tally.topped += 1;
                let z = h / f.stone.height();
                f.stone.scaled(1.0, z)
            }
            None => f.stone,
        };
        // a low build draws the stone as its hull: the facets, no grain
        let mesh = if scene.low() {
            st.convex.to_tri_mesh()
        } else {
            grain(&st.convex, &g, seed.index(u64::from(f.k)).value()).map_err(LookError::build)?
        };
        built.push(
            solid(slots.get(f.k), &mesh)
                .map_err(LookError::build)?
                .turn(f.yaw, 1.0)
                .translate([f.p[0], f.p[1], f.z0]),
        );
        ticks.tick();
    }
    // how each foot rests on the land: the gap under it
    if let Some(land) = &land {
        for sol in &built {
            let gap = land.hover(sol);
            tally.hover_max = tally.hover_max.max(gap);
            tally.hovering += usize::from(gap > HOVER_TOL_MM);
        }
    }
    // a rock laid before the lift lays its top: the surface laid after it (a
    // dirt) lies on it like on any surface
    if !scene.lifted() && !built.is_empty() {
        let meshes: Vec<dt_kit::rays::RayMesh> = built
            .iter()
            .map(|sol| dt_kit::rays::RayMesh::new(&sol.mesh()))
            .collect();
        let tops = crate::cliff::tops::stone_tops(scene.frame(), &meshes, 0.0);
        scene.lay_tops(&tops);
    }
    if let (Some(m), Some(d)) = (marks, scene.clouds()) {
        for c in m {
            d.column(c);
        }
    }
    // where each stood, before any detail: the same in every tier
    for (at, r) in spots {
        scene.place(Kind::Rock, at, r);
    }
    for sol in built {
        scene.add(
            Tag {
                kind: Kind::Rock,
                owner: owner.clone(),
            },
            sol,
        );
    }
    Ok(())
}

/// A rock's peak stands at least this far over its foot, mm at the 35 mm
/// square, and at most [`MAX_PEAK_MM`].
const MIN_PEAK_MM: f64 = 1.0;
const MAX_PEAK_MM: f64 = 120.0;

/// One stone as a `top=` reads it: its item, its middle (mm), its footprint
/// radius, the land at its middle, the height its foot is set at, its
/// number.
type Stood = (usize, [f64; 2], f64, f64, f64, u32);

/// The height over its foot each stone with a `top=` is stood at (`None` for
/// a stone without one), mm.  The formula is worked out at the stone's
/// middle (so `$d` is 0% there, `$region` the disc of the stone) and gives a
/// cloud; the stone rolls one number in it.  The rolls come from one smooth
/// field over the tile (stones a square apart are alike), ranked over the
/// topped stones so a weighting's odds hold; a lone stone draws its own.
fn peaks_under_clouds(
    stood: &[Stood],
    items: &[(&Item, [f64; 2], f64)],
    seed: Seed,
    frame: &dt_kit::Frame,
    crowns: Option<&dyn crate::CrownTops>,
    owner: &dt_kit::EntryId,
    mut marks: Option<&mut Vec<dt_kit::debug::Column>>,
) -> Result<Vec<Option<f64>>, LookError> {
    let topped: Vec<(usize, &crate::Formula)> = stood
        .iter()
        .enumerate()
        .filter_map(|(i, f)| items[f.0].0.top.as_ref().map(|t| (i, t)))
        .collect();
    let mut out = vec![None; stood.len()];
    if topped.is_empty() {
        return Ok(out);
    }
    let Some(crowns) = crowns else {
        return Err(LookError::build(
            "a rock's top is worked out by the tile it is built in",
        ));
    };
    let (s, sq) = (frame.scale(), frame.square_mm);
    let mut clouds = Vec::with_capacity(topped.len());
    for &(i, formula) in &topped {
        let (_, p, r, bottom, _, _) = stood[i];
        clouds.push(crowns.cloud(
            formula,
            &crate::Tip {
                at: [p[0] / sq, p[1] / sq],
                centre: [p[0] / sq, p[1] / sq],
                radius: r / sq,
                bottom_dmm: bottom / s,
            },
        )?);
    }
    // the rolls: one smooth field, ranked
    let key = seed.key("roll");
    let m = topped.len();
    let mut rank = vec![0.5; m];
    if m == 1 {
        rank[0] = Rng::new(key).random();
    } else {
        let rolls: Vec<f64> = topped
            .iter()
            .map(|&(i, _)| dt_kit::noise_at(key, sq, &[stood[i].1[0]], &[stood[i].1[1]])[0])
            .collect();
        let mut order: Vec<usize> = (0..m).collect();
        order.sort_by(|&a, &b| rolls[a].total_cmp(&rolls[b]).then(a.cmp(&b)));
        #[allow(clippy::cast_precision_loss)]
        for (r, &j) in order.iter().enumerate() {
            rank[j] = (r as f64 + 0.5) / m as f64;
        }
    }
    for (j, &(i, _)) in topped.iter().enumerate() {
        let c = &clouds[j];
        let t = c.odds.as_ref().map_or(rank[j], |w| w.unit(rank[j]));
        let top = (c.lo + (c.hi - c.lo) * t) * s;
        let (_, p, _, _, z0, _) = stood[i];
        let height = top - z0;
        if !(MIN_PEAK_MM * s..=MAX_PEAK_MM * s).contains(&height) {
            return Err(LookError::control(
                "top",
                format!(
                    "this top is {:.1} mm over the foot of the rock at ({:.1}, {:.1}) mm (the land there is {:.1} mm up); a rock's peak stands {MIN_PEAK_MM} to {MAX_PEAK_MM} mm over its foot",
                    height / s,
                    p[0] / s,
                    p[1] / s,
                    stood[i].3 / s
                ),
                "write the top over the ground at its foot: `$bottom + [4dmm .. 9dmm]`".into(),
            ));
        }
        if let Some(m) = marks.as_deref_mut() {
            let shade = |u| dt_kit::debug::shade_of(c.odds.as_ref(), u);
            m.push(dt_kit::debug::Column {
                owner: owner.clone(),
                class: "stone-cloud",
                at: p,
                span: [c.lo * s, c.hi * s],
                rolled: top,
                shade: [shade(0.0), shade(1.0), shade(t)],
            });
        }
        out[i] = Some(height);
    }
    Ok(out)
}

pub(crate) fn note(scene: &mut Scene, t: &Tally) {
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
        ("seat_gap_max", t.hover_max),
        ("seat_gap_stones", t.hovering as f64),
    ] {
        scene.note(k, v);
    }
    if t.topped > 0 {
        #[allow(clippy::cast_precision_loss)]
        scene.note("topped_stones", t.topped as f64);
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
        let mut item = make_item(&mut rng, d, self.thing.mix, false, false)?;
        item.top.clone_from(&self.thing.top);
        let yaw = rng.uniform(0.0, 360.0);
        let s = scene.frame().scale();
        let sq = scene.frame().square_mm;
        let at = match ctx.at.map(|[dx, dy]| {
            // a range is drawn from the entry's seed, with its odds; a number never draws
            let x = if dx.is_point() {
                dx.lo()
            } else {
                dx.draw(&mut rng)
            };
            let y = if dy.is_point() {
                dy.lo()
            } else {
                dy.draw(&mut rng)
            };
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
                let placed = place(
                    scene,
                    ctx,
                    &mut rng,
                    &[item.r * s],
                    1.5 * s,
                    crate::LINE_REACH_MM,
                    &[],
                );
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
        add_items(
            scene,
            ctx.path,
            ctx.seed,
            &[(&item, at, yaw)],
            ctx.crowns,
            &mut t,
        )?;
        note(scene, &t);
        Ok(())
    }
}

/// The entry's area less the core of a cliff's stones: nothing is set down
/// on it (it lies behind the stones, and what stood on it would be loose).
fn open_area(scene: &Scene, ctx: &Ctx<'_>, reach: f64) -> dt_kit::Mask {
    // at= a line: the band either side of it
    let mut area = ctx.spread_area(scene.frame(), reach);
    if let Some(rest) = scene.rest() {
        for (a, c) in area.bits.iter_mut().zip(&rest.core.bits) {
            *a &= !*c;
        }
    }
    area
}

/// Dart-throw radii (world mm) into the entry's area, largest first.
///
/// `also` are discs (x, y, r) of other things already placed, kept clear
/// of like the rocks that stand.
pub(crate) fn place(
    scene: &Scene,
    ctx: &Ctx<'_>,
    rng: &mut Rng,
    radii: &[f64],
    gap: f64,
    reach: f64,
    also: &[[f64; 3]],
) -> Vec<Placed> {
    let frame = *scene.frame();
    let area = &open_area(scene, ctx, reach);
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
    let mut avoid = rocks_in(scene);
    avoid.extend_from_slice(also);
    scatter(&frame, rng, radii, &spread, &inside, &avoid)
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
    let area = &open_area(scene, ctx, reach);
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
    count: Option<Draw>,
    per_square: Draw,
    gap: f64,
    /// `at=foot(…)`: how far from the foot stones lie, mean mm; `at=` a
    /// line: how far either side of it.  `None` is each one's default.
    reach: Option<f64>,
    things: Vec<Thing>,
}

/// Placements: 9 to 12 per 2×2, so 2.25 to 3 a square.
pub(crate) const PER_SQUARE: Draw = Draw::flat(2.25, 3.0);
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
        let set = Settings::new(controls);
        let mut s = Self {
            count: None,
            per_square: PER_SQUARE,
            gap: GAP_MM,
            reach: None,
            things: Vec::new(),
        };
        s.count = set.dist("count")?;
        s.per_square = set.dist_or("perSquare", PER_SQUARE)?;
        s.reach = set.number("reach")?;
        s.gap = set.number_or("gap", s.gap)?;
        for ch in children {
            if !is_thing(&ch.name) {
                return Err(LookError::general(
                    format!("`{}` is not a rock thing", ch.name),
                    "a scatter holds rocks (rock, stone, boulder, pebble), flowers and trees",
                ));
            }
            s.things
                .push(Thing::new(&ch.name, &ch.controls, ch.weight)?);
        }
        if s.things.is_empty() {
            return Err(LookError::general(
                "a scatter needs a child to say what it scatters",
                "write `scatter perSquare=2 { rock }`",
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
                    format!("add a `{kind}` to the stack, or remove `at=foot(…)`"),
                )
            })?),
            None => None,
        };
        // squares the area covers
        #[allow(clippy::cast_precision_loss)]
        let line_reach = self.reach.unwrap_or(crate::LINE_REACH_MM);
        let squares = foot.map_or_else(
            || ctx.spread_squares(&frame, &ctx.spread_area(&frame, line_reach)),
            |f| f.squares,
        );
        // a whole number: the ends round and every one is as likely as any
        let n = match &self.count {
            Some(r) => r.int_by(1.0, &mut rng),
            None => self.per_square.int_by(squares, &mut rng),
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = n.clamp(0, 10_000) as usize;
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
                None => make_item(&mut r, d, thing.mix, true, thing.buried.hi() > 0.0)?,
            };
            item.bury = thing.buried.clone();
            item.top.clone_from(&thing.top);
            items.push(item);
            kinds.push(thing.kind.as_str());
        }
        let radii: Vec<f64> = items.iter().map(|it| it.r * s).collect();
        let placed = match foot {
            Some(foot) => place_at_foot(
                scene,
                ctx,
                &mut rng,
                foot,
                &items,
                &kinds,
                self.reach.unwrap_or(SCREE_REACH_MM),
            ),
            None => place(scene, ctx, &mut rng, &radii, self.gap * s, line_reach, &[]),
        };
        let posed: Vec<(&Item, [f64; 2], f64)> = placed
            .iter()
            .map(|p| (&items[p.index], p.at, rng.uniform(0.0, 360.0)))
            .collect();
        let mut t = Tally::default();
        add_items(scene, ctx.path, ctx.seed, &posed, ctx.crowns, &mut t)?;
        note(scene, &t);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buried_is_a_share_never_a_loose_percent() {
        let c = |v: f64| Control {
            name: "buried".into(),
            value: crate::Value::Number(v),
        };
        let ok = Thing::new("rock", &[c(0.3)], 1.0).unwrap();
        assert_eq!(ok.buried, Draw::flat(0.3, 0.3));
        // 1 is 100%; 40 is not read as 40%
        assert!(Thing::new("rock", &[c(1.0)], 1.0).is_ok());
        let e = Thing::new("rock", &[c(40.0)], 1.0).unwrap_err();
        assert_eq!(e.code, Some("dt::percent"));
        assert!(e.help.unwrap().contains("percent"));
    }

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
