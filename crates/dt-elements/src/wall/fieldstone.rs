//! Fieldstone (drystone): crack network first. The chassis layout gives the crack topology;
//! every bed line has one shared wobble and every head joint one shared
//! slanted, bowed curve, both keyed on the crack itself, so the two
//! stones flanking a crack evaluate the same curve and fit each other by
//! construction. A stone is the polygon its cracks bound (corners
//! rounded, inward ring noise), morphed into a pebble: latitude rings
//! from the front pole to the back, the equator the outline. Head and bed
//! contacts overlap a little so the union presses the stones together.
//! No mortar core: rough rubble hearting fills the body.
//!
//! Printability (the brief's by-construction rule): a pebble's underside
//! turns down to its front pole, a shelf that overhangs the stone below
//! wherever it stands proud of it. The underside here keeps the
//! upper profile but turns down no shallower than the overhang limit
//! allows (`UNDER_*`): the lower half of each latitude ring is pulled
//! back toward the face so the rounded bottom is a chamfer-like curve the
//! printer can lay, and proudness is held to the stone below.

use dt_kit::form::{Relief, relief_radial, subdivide};
use dt_kit::{KitError, Rng, Seed, TriMesh};

use super::layout::{self, Cell, Courses, End, Seg};
use super::{Mason, Run};

// crack network (mm at the reference)
const WOBBLE_AMP: (f64, f64) = (0.16, 0.38);
const WOBBLE_WL: (f64, f64) = (11.0, 32.0);
const WOBBLE_TAPER: f64 = 12.0;
const DRIFT: f64 = 2.2;
const BOW: f64 = 1.1;
const BED_OVERLAP: (f64, f64) = (0.35, 0.80);
const END_MARGIN: f64 = 0.7;
/// The press into the seat (drawn up to 0.22 × the height).
const SEAT_PRESS: (f64, f64) = (0.6, 1.0);
const SEAT_PRESS_CAP: f64 = 0.32;
const HEAD_OVERLAP: (f64, f64) = (0.45, 0.95);
const RING_NOISE: (f64, f64) = (0.04, 0.16);
// the pebble
const ROUND_SIZE_CAP: f64 = 0.26;
const MORPH_A: (f64, f64) = (2.0, 3.2);
const MORPH_B: (f64, f64) = (0.33, 0.48);
const POLE_DRIFT: f64 = 0.12;
const BED_FLAT_EXP: (f64, f64) = (0.40, 0.65);
const PROUD: (f64, f64) = (0.10, 0.70);
const PROUD_DEEP_PROB: f64 = 0.15;
const PROUD_DEEP: (f64, f64) = (0.70, 1.10);
const RING_STEP: f64 = 1.2;
const N_LAT: usize = 17;
// coping on edge
const COPING_H: f64 = 6.5;
const COPING_W: (f64, f64) = (2.2, 3.6);
const COPING_LEAN: f64 = 0.30;
const COPING_TOP_DROP: f64 = 1.1;
const COPING_OV: (f64, f64) = (0.15, 0.35);
const COPING_SEAT_OV: (f64, f64) = (0.30, 0.60);
const CORNER_SLAB: (f64, f64) = (0.5, 1.5);
// cell topology
const THROUGH_FRAC: f64 = 0.20;
const THROUGH_MAX: f64 = 9.5;
const SPLIT_H_PROB: f64 = 0.28;
const SPLIT_H_MIN: f64 = 5.5;
const K_THROUGH: i64 = 9;
const K_SPLIT_B: i64 = 1;
const K_SPLIT_T: i64 = 2;
const K_REST: i64 = 3;
const K_COPING: i64 = 5;
/// A resting stone's underside bevels up at 45° from this far in from
/// each face, mm at the reference.
const UNDER_FROM: f64 = 2.0;
/// Joint pins (see [`joint_pins`]), mm at the reference: the wedge's
/// half-width at the joint's mouth, how far back from the faces it sits,
/// how far it reaches up into the stone above, and the widest head joint
/// it fills.
const PIN_HALF_MM: f64 = 1.6;
const PIN_SETBACK_MM: f64 = 0.6;
const PIN_EMBED_MM: f64 = 0.3;
const PIN_GAP_MM: f64 = 2.0;

/// What the fieldstone layout leaves the chassis.
pub(crate) struct Field {
    /// Wall-local z where the top course (or coping) begins: the
    /// hearting stops beneath it.
    pub cap_z0: f64,
}

fn quant(v: f64) -> u64 {
    #[allow(clippy::cast_possible_truncation)]
    let k = libm::round(v * 100.0) as i64;
    k.cast_unsigned()
}

/// The shared wobble of the bed line at wall-local `z` on segment `seg`.
fn bed(seed: Seed, s: f64, seg: usize, len: f64, z: f64, t: f64) -> f64 {
    let mut r = Rng::new(seed.key("bed").index(seg as u64).index(quant(z)));
    let amp = [
        r.uniform(WOBBLE_AMP.0, WOBBLE_AMP.1) * s,
        r.uniform(WOBBLE_AMP.0, WOBBLE_AMP.1) * s,
    ];
    let wl = [
        r.uniform(WOBBLE_WL.0, WOBBLE_WL.1) * s,
        r.uniform(WOBBLE_WL.0, WOBBLE_WL.1) * s,
    ];
    let tau = 2.0 * core::f64::consts::PI;
    let ph = [r.uniform(0.0, tau), r.uniform(0.0, tau)];
    let taper = (t.min(len - t) / (WOBBLE_TAPER * s)).clamp(0.0, 1.0);
    (0..2)
        .map(|k| amp[k] * libm::sin(tau * t / wl[k] + ph[k]))
        .sum::<f64>()
        * taper
}

/// The (slant, bow) of the head joint at cut `t` in `course`.
fn drift(seed: Seed, s: f64, seg: usize, course: i64, t: f64) -> (f64, f64) {
    let mut r = Rng::new(
        seed.key("drift")
            .index(seg as u64)
            .index(course.cast_unsigned())
            .index(quant(t)),
    );
    (r.uniform(-DRIFT, DRIFT) * s, r.uniform(-BOW, BOW) * s)
}

/// The shared lean (dt per dz) of the coping joint at cut `t`.
fn lean(seed: Seed, seg: usize, t: f64) -> f64 {
    let mut r = Rng::new(seed.key("lean").index(seg as u64).index(quant(t)));
    r.uniform(-COPING_LEAN, COPING_LEAN)
}

/// `_cells`: the chassis layout with throughstones merged and tall cells
/// split, the coping course on edge on top.
pub(crate) fn cells(m: &Mason, run: &Run, courses: &Courses, rng: &mut Rng) -> (Vec<Cell>, Field) {
    let s = run.scale;
    let h = run.height;
    let hc = if m.coping_on_edge && !run.laid {
        h - COPING_H * s
    } else {
        h
    };
    let mut c = *courses;
    c.height = hc;
    let cells = layout::layout(&run.segs, &c, rng);
    let cells = merge_throughstones(cells, s, rng);
    let mut cells = split_cells(cells, s, rng);
    if m.coping_on_edge && !run.laid {
        for c in &mut cells {
            c.is_top = false;
        }
        cells.extend(coping_cells(&run.segs, run.across, hc, h, s, rng));
    }
    let cap_z0 = cells
        .iter()
        .filter(|c| c.is_top)
        .map(|c| c.z0)
        .fold(h, f64::min);
    (cells, Field { cap_z0 })
}

fn coping_cells(segs: &[Seg], t: f64, z0: f64, h: f64, s: f64, rng: &mut Rng) -> Vec<Cell> {
    let n_joints = segs.len() - 1;
    let mut out = Vec::new();
    for (k, seg) in segs.iter().enumerate() {
        let owns_start = k > 0 && (k - 1) % 2 != 0;
        let owns_end = k < n_joints && k % 2 == 0;
        let mut t0 = if k == 0 || owns_start { 0.0 } else { t };
        let mut t1 = if k == n_joints || owns_end {
            seg.len
        } else {
            seg.len - t
        };
        let mut slab0 = None;
        let mut slab1 = None;
        if k > 0 && owns_start {
            let e = t + rng.uniform(CORNER_SLAB.0, CORNER_SLAB.1) * s;
            slab0 = Some((0.0, e));
            t0 = e;
        }
        if k < n_joints && owns_end {
            let b = seg.len - t - rng.uniform(CORNER_SLAB.0, CORNER_SLAB.1) * s;
            slab1 = Some((b, seg.len));
            t1 = b;
        }
        let span = t1 - t0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (libm::round(span / (0.5 * (COPING_W.0 + COPING_W.1) * s)) as usize).max(1);
        let w: Vec<f64> = (0..n)
            .map(|_| rng.uniform(COPING_W.0, COPING_W.1))
            .collect();
        let sum: f64 = w.iter().sum();
        let mut pieces = Vec::new();
        pieces.extend(slab0);
        let mut acc = t0;
        for x in &w {
            let e = acc + x * span / sum;
            pieces.push((acc, e));
            acc = e;
        }
        pieces.extend(slab1);
        for (b, (ta, tb)) in pieces.into_iter().enumerate() {
            let first = ta <= 1e-6 && (k == 0 || owns_start);
            let last = tb >= seg.len - 1e-6 && (k == n_joints || owns_end);
            #[allow(clippy::cast_possible_wrap)]
            out.push(Cell {
                seg: k,
                t0: ta,
                t1: tb,
                end0: if first { End::Face } else { End::Joint },
                end1: if last { End::Face } else { End::Joint },
                z0,
                z1: h,
                is_top: true,
                is_bottom: false,
                is_quoin: false,
                key: vec![9000 + k as i64, k as i64, b as i64, K_COPING],
                side_bands: None,
                coping: true,
            });
        }
    }
    out
}

fn merge_throughstones(mut cells: Vec<Cell>, s: f64, rng: &mut Rng) -> Vec<Cell> {
    let mut taken = vec![false; cells.len()];
    let mut drop = vec![false; cells.len()];
    let mut merged = Vec::new();
    let mut rest = Vec::new();
    for i in 0..cells.len() {
        {
            let c = &cells[i];
            if c.is_top || c.is_quoin || taken[i] || rng.random() > THROUGH_FRAC {
                continue;
            }
        }
        let (course, seg) = (cells[i].key[0], cells[i].seg);
        let above: Vec<usize> = (0..cells.len())
            .filter(|&a| cells[a].key[0] == course + 1 && cells[a].seg == seg)
            .collect();
        for a in above {
            let (c, o) = (&cells[i], &cells[a]);
            if o.is_quoin
                || taken[a]
                || c.t1.min(o.t1) - c.t0.max(o.t0) < 3.0 * s
                || o.z1 - c.z0 > THROUGH_MAX * s
            {
                continue;
            }
            let lo = c.t0.max(o.t0);
            let hi = c.t1.min(o.t1);
            merged.push(Cell {
                seg,
                t0: lo,
                t1: hi,
                // a wall end stays a face (making both ends joints would let
                // an end throughstone run past the end margin)
                end0: if (c.t0 >= lo - 1e-9 && c.end0 == End::Face)
                    || (o.t0 >= lo - 1e-9 && o.end0 == End::Face)
                {
                    End::Face
                } else {
                    End::Joint
                },
                end1: if (c.t1 <= hi + 1e-9 && c.end1 == End::Face)
                    || (o.t1 <= hi + 1e-9 && o.end1 == End::Face)
                {
                    End::Face
                } else {
                    End::Joint
                },
                z0: c.z0,
                z1: o.z1,
                is_top: o.is_top,
                is_bottom: c.is_bottom,
                is_quoin: false,
                key: vec![course, seg as i64, c.key[2], K_THROUGH],
                side_bands: Some(vec![(course, c.z0, c.z1), (course + 1, o.z0, o.z1)]),
                coping: false,
            });
            taken[i] = true;
            taken[a] = true;
            for d in [i, a] {
                let dn = &mut cells[d];
                let left = dn.t0 < lo - 1.0 * s;
                let right = dn.t1 > hi + 1.0 * s;
                if left && right {
                    // both remainders survive (keeping only the left
                    // would leave a hole behind the right)
                    let mut r = dn.clone();
                    r.t0 = hi;
                    r.end0 = End::Joint;
                    r.key.push(K_REST);
                    rest.push(r);
                }
                if left {
                    dn.t1 = lo;
                    dn.end1 = End::Joint;
                } else if right {
                    dn.t0 = hi;
                    dn.end0 = End::Joint;
                } else {
                    drop[d] = true;
                }
            }
            break;
        }
    }
    let mut out: Vec<Cell> = cells
        .into_iter()
        .zip(drop)
        .filter(|(_, d)| !d)
        .map(|(c, _)| c)
        .collect();
    out.extend(rest);
    out.extend(merged);
    out
}

fn split_cells(cells: Vec<Cell>, s: f64, rng: &mut Rng) -> Vec<Cell> {
    let mut out = Vec::with_capacity(cells.len());
    for c in cells {
        let h = c.z1 - c.z0;
        if c.is_top
            || c.is_quoin
            || c.key.len() > 3
            || h < SPLIT_H_MIN * s
            || rng.random() > SPLIT_H_PROB
        {
            out.push(c);
            continue;
        }
        let zm = c.z0 + h * rng.uniform(0.45, 0.65);
        for (z0, z1, tag, bot) in [
            (c.z0, zm, K_SPLIT_B, c.is_bottom),
            (zm, c.z1, K_SPLIT_T, false),
        ] {
            let mut key = c.key.clone();
            key.push(tag);
            out.push(Cell {
                z0,
                z1,
                is_top: false,
                is_bottom: bot,
                key,
                ..c.clone()
            });
        }
    }
    out
}

/// One fieldstone, drawn: its outline ring (t, z) and its pebble.
pub(crate) struct Stone {
    pub seg: usize,
    /// The outline along the wall; the cell up the wall.
    t: [f64; 2],
    z: [f64; 2],
    ring: Vec<[f64; 2]>,
    y0: f64,
    y1: f64,
    ctr: [f64; 2],
    poles: [[f64; 2]; 2],
    a: [f64; 2],
    b: [f64; 2],
    pz: f64,
    coping: bool,
    /// The bed the stone sits on (wall-local z under its bottom), for
    /// the underside rule; `None` for a bottom stone or laid flat.
    under: Option<f64>,
    under_from: f64,
    relief: Option<Relief>,
    seed: u64,
}

/// The outline polygon (t, z), counter-clockwise, bounded by the cell's
/// cracks (`_outline`).
#[allow(clippy::too_many_lines)]
fn outline(m: &Mason, run: &Run, c: &Cell, seed: Seed, r: &mut Rng) -> Vec<[f64; 2]> {
    let s = run.scale;
    let g = &run.segs[c.seg];
    let bands = c
        .side_bands
        .clone()
        .unwrap_or_else(|| vec![(c.key[0], c.z0, c.z1)]);
    let w = c.t1 - c.t0;
    let (ov0, ov1) = if c.coping {
        (
            r.uniform(COPING_OV.0, COPING_OV.1) * s,
            r.uniform(COPING_OV.0, COPING_OV.1) * s,
        )
    } else if c.is_top {
        (0.0, 0.0)
    } else {
        let cap = 0.25 * w;
        (
            (r.uniform(HEAD_OVERLAP.0, HEAD_OVERLAP.1) * s).min(cap),
            (r.uniform(HEAD_OVERLAP.0, HEAD_OVERLAP.1) * s).min(cap),
        )
    };
    let top_drop = if c.coping {
        r.uniform(0.0, COPING_TOP_DROP) * s
    } else {
        0.0
    };
    let base = if run.laid { 0.0 } else { run.base };
    // a laid stone on the tile's boundary runs past the plane, so the cut
    // there is through its flat top and square
    let pad = if run.laid { m.edge_pad } else { 0.0 };
    let zeff = |z: f64, t: f64| -> f64 {
        if c.is_bottom && z <= c.z0 + 1e-9 {
            return if run.laid { z - pad } else { base };
        }
        if c.is_top && z >= c.z1 - 1e-9 {
            if run.laid {
                return z + pad;
            }
            return z - top_drop;
        }
        z + bed(seed, s, c.seg, g.len, z, t)
    };
    let side = |t_cut: f64, end: End, off: f64, inward: f64| -> Vec<[f64; 2]> {
        if end == End::Face {
            let em = if run.laid { -pad } else { END_MARGIN * s };
            let tq = t_cut + inward * em;
            return vec![[tq, zeff(c.z0, tq)], [tq, zeff(c.z1, tq)]];
        }
        if c.coping {
            let l = lean(seed, c.seg, t_cut);
            return [c.z0, c.z1]
                .iter()
                .map(|&z| [t_cut + off + l * (z - c.z0), zeff(z, t_cut)])
                .collect();
        }
        let mut pts = Vec::new();
        for &(crs, zb0, zb1) in &bands {
            let (d, bow) = drift(seed, s, c.seg, crs, t_cut);
            let zm = 0.5 * (zb0 + zb1);
            let hh = 0.5 * (zb1 - zb0);
            for k in 0..5 {
                #[allow(clippy::cast_precision_loss)]
                let z = zb0 + (zb1 - zb0) * k as f64 / 4.0;
                let q = (z - zm) / hh;
                let t = t_cut + off + d * q / 2.0 + bow * (1.0 - q * q);
                let zp = if k == 0 || k == 4 { zeff(z, t) } else { z };
                pts.push([t, zp]);
            }
        }
        pts
    };
    // at an inside corner a stone butts the other run's inner face, which
    // sits back by its proudness: run on into it, so the corner crack
    // never opens wider than the stone above can bridge
    let corner = (PROUD_DEEP.1 + 0.1) * s;
    let (mut ov0, mut ov1) = (ov0, ov1);
    if !run.laid && !c.coping {
        if c.seg > 0 && c.end0 == End::Joint && libm::fabs(c.t0 - run.across) < 1e-6 {
            ov0 += corner;
        }
        if c.seg + 1 < run.segs.len()
            && c.end1 == End::Joint
            && libm::fabs(c.t1 - (g.len - run.across)) < 1e-6
        {
            ov1 += corner;
        }
    }
    let left = side(c.t0, c.end0, -ov0, 1.0);
    let right = side(c.t1, c.end1, ov1, -1.0);
    let bedline = |z: f64, ta: f64, tb: f64| -> Vec<[f64; 2]> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((libm::fabs(tb - ta) / (4.0 * s)) as usize + 2).max(2);
        (0..n)
            .map(|k| {
                #[allow(clippy::cast_precision_loss)]
                let t = ta + (tb - ta) * k as f64 / (n - 1) as f64;
                [t, zeff(z, t)]
            })
            .collect()
    };
    let bot = bedline(c.z0, left[0][0], right[0][0]);
    let top = bedline(c.z1, right[right.len() - 1][0], left[left.len() - 1][0]);
    let mut pts = bot;
    pts.extend_from_slice(&right[1..]);
    pts.extend_from_slice(&top[1..]);
    let lrev: Vec<[f64; 2]> = left.iter().rev().copied().collect();
    pts.extend_from_slice(&lrev[1..lrev.len() - 1]);
    // the bed overlap: stretch past the bed lines, the coping pressed
    // down into the course below
    let (zlo, zhi) = pts
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
            (a.min(p[1]), b.max(p[1]))
        });
    let span = (zhi - zlo).max(1e-9);
    if c.coping {
        let ovb = r.uniform(COPING_SEAT_OV.0, COPING_SEAT_OV.1) * s;
        for p in &mut pts {
            p[1] -= ovb * (1.0 - (p[1] - zlo) / span);
        }
    } else if !run.laid {
        // the top course presses down too (exempting it would leave its
        // underside roofing an open V crack the printer cannot lay)
        // but keeps its own ragged top
        let cap = 0.22 * (c.z1 - c.z0);
        let ovt = if c.is_top {
            0.0
        } else {
            (r.uniform(BED_OVERLAP.0, BED_OVERLAP.1) * s).min(cap)
        };
        let ovb = if c.is_bottom {
            0.0
        } else {
            // a stone presses into its seat deeper than it rises into the
            // stone above: the V crack under it closes before it can roof
            (r.uniform(SEAT_PRESS.0, SEAT_PRESS.1) * s).min(SEAT_PRESS_CAP * (c.z1 - c.z0))
        };
        for p in &mut pts {
            let u = (p[1] - zlo) / span;
            p[1] += ovt * u - ovb * (1.0 - u);
        }
    }
    let _ = m;
    pts
}

fn signed_area(p: &[[f64; 2]]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn centroid(p: &[[f64; 2]]) -> [f64; 2] {
    let n = p.len();
    let a = signed_area(p);
    if a.abs() < 1e-12 {
        #[allow(clippy::cast_precision_loss)]
        let k = n as f64;
        return [
            p.iter().map(|q| q[0]).sum::<f64>() / k,
            p.iter().map(|q| q[1]).sum::<f64>() / k,
        ];
    }
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..n {
        let (q, r) = (p[i], p[(i + 1) % n]);
        let x = q[0] * r[1] - r[0] * q[1];
        cx += (q[0] + r[0]) * x;
        cy += (q[1] + r[1]) * x;
    }
    [cx / (6.0 * a), cy / (6.0 * a)]
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn len2(a: [f64; 2]) -> f64 {
    libm::sqrt(a[0] * a[0] + a[1] * a[1])
}

/// The point `d` along the polyline from `p[i]` stepping by `dir` (±1),
/// and how many vertices it passed.
fn walk(p: &[[f64; 2]], i: usize, dir: isize, d: f64) -> ([f64; 2], usize) {
    let n = p.len();
    #[allow(clippy::cast_possible_wrap)]
    let ni = n as isize;
    let mut left = d;
    let mut cur = i;
    for k in 1..n {
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        let nx = ((i as isize + dir * k as isize).rem_euclid(ni)) as usize;
        let e = len2(sub(p[nx], p[cur]));
        if e >= left {
            let f = left / e.max(1e-12);
            let q = [
                p[cur][0] + (p[nx][0] - p[cur][0]) * f,
                p[cur][1] + (p[nx][1] - p[cur][1]) * f,
            ];
            return (q, k - 1);
        }
        left -= e;
        cur = nx;
    }
    (p[cur], n - 1)
}

/// The outline's corners rounded at `r` and the crack inset by `inset`
/// (a morphological opening, as `buffer(−(j/2 + r)).buffer(r)`): every
/// vertex turning more than 30° is replaced by a tangent arc, then the
/// ring is pulled in along its normals by `inset`.
///
/// Corners below `chamfer_below` are cut straight instead (a 45° chamfer
/// on a square corner): the underside of a rounded corner turns flat and
/// roofs the notch under it; a chamfer the printer lays.
fn round_corners(
    p: &[[f64; 2]],
    r: f64,
    inset: f64,
    chamfer_below: f64,
    chamfer_r: f64,
) -> Vec<[f64; 2]> {
    let n = p.len();
    let mut corner = vec![false; n];
    for i in 0..n {
        let a = sub(p[i], p[(i + n - 1) % n]);
        let b = sub(p[(i + 1) % n], p[i]);
        let (la, lb) = (len2(a), len2(b));
        if la < 1e-9 || lb < 1e-9 {
            continue;
        }
        let cross = a[0] * b[1] - a[1] * b[0];
        let dot = a[0] * b[0] + a[1] * b[1];
        let turn = libm::atan2(cross, dot);
        // convex (counter-clockwise ring: left turns) corners only
        corner[i] = turn > 30f64.to_radians();
    }
    // each corner's tangent points and the vertices its arc replaces
    let mut arcs: Vec<Option<([f64; 2], [f64; 2])>> = vec![None; n];
    let mut consumed = vec![false; n];
    if r > 1e-6 {
        for i in 0..n {
            if !corner[i] || consumed[i] {
                continue;
            }
            // the corner angle over a stretch ~r
            let r = if p[i][1] < chamfer_below {
                chamfer_r
            } else {
                r
            };
            let (pa, _) = walk(p, i, -1, r);
            let (pb, _) = walk(p, i, 1, r);
            let (u, v) = (sub(p[i], pa), sub(pb, p[i]));
            let ang = libm::atan2(u[0] * v[1] - u[1] * v[0], u[0] * v[0] + u[1] * v[1]);
            let d = (r * libm::tan(0.5 * ang.clamp(0.0, 2.8))).max(1e-6);
            let (a, ka) = walk(p, i, -1, d);
            let (b, kb) = walk(p, i, 1, d);
            for k in 1..=ka {
                consumed[(i + n - k % n) % n] = true;
            }
            for k in 1..=kb {
                consumed[(i + k) % n] = true;
            }
            arcs[i] = Some((a, b));
        }
    }
    let mut out = Vec::new();
    for i in 0..n {
        if let Some((a, b)) = arcs[i] {
            let straight = p[i][1] < chamfer_below;
            for k in 0..=6 {
                let t = f64::from(k) / 6.0;
                let (w0, w1, w2) = if straight {
                    (1.0 - t, 0.0, t)
                } else {
                    ((1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t)
                };
                out.push([
                    w0 * a[0] + w1 * p[i][0] + w2 * b[0],
                    w0 * a[1] + w1 * p[i][1] + w2 * b[1],
                ]);
            }
        } else if !consumed[i] {
            out.push(p[i]);
        }
    }
    if inset > 0.0 {
        let normals = normals(&out);
        for (q, nn) in out.iter_mut().zip(normals) {
            q[0] -= nn[0] * inset;
            q[1] -= nn[1] * inset;
        }
    }
    out
}

/// Outward unit normals of a counter-clockwise ring, smoothed twice.
fn normals(ring: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let n = ring.len();
    let unit = |v: [f64; 2]| {
        let l = len2(v) + 1e-12;
        [v[0] / l, v[1] / l]
    };
    let mut nrm: Vec<[f64; 2]> = (0..n)
        .map(|i| {
            let t = sub(ring[(i + 1) % n], ring[(i + n - 1) % n]);
            unit([t[1], -t[0]])
        })
        .collect();
    for _ in 0..2 {
        let prev = nrm.clone();
        for i in 0..n {
            let (a, b) = (prev[(i + n - 1) % n], prev[(i + 1) % n]);
            nrm[i] = unit([
                prev[i][0] + 0.5 * (a[0] + b[0]),
                prev[i][1] + 0.5 * (a[1] + b[1]),
            ]);
        }
    }
    nrm
}

/// `shapely.segmentize`: every edge longer than `step` split evenly.
fn segmentize(p: &[[f64; 2]], step: f64) -> Vec<[f64; 2]> {
    let n = p.len();
    let mut out = Vec::with_capacity(2 * n);
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        let l = len2(sub(b, a));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = (libm::ceil(l / step) as usize).max(1);
        for j in 0..k {
            #[allow(clippy::cast_precision_loss)]
            let f = j as f64 / k as f64;
            out.push([a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]);
        }
    }
    out
}

/// Draw the stone for a cell (`_place_block`, up to the mesh), or `None`
/// for a cell the rounding swallows.
#[allow(clippy::too_many_lines)]
pub(crate) fn stone(m: &Mason, run: &Run, f: &Field, c: &Cell, seed: Seed) -> Option<Stone> {
    let _ = f;
    let s = run.scale;
    let bs = super::cell_seed(seed, "stone", &c.key);
    let mut r = Rng::new(bs.key("draw"));
    let raw = outline(m, run, c, seed, &mut r);
    let (lo, hi) = raw.iter().fold(
        ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
        |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            )
        },
    );
    let span = [hi[0] - lo[0], hi[1] - lo[1]];
    if span[0] < 0.5 * s || span[1] < 0.5 * s {
        return None;
    }
    let rr = (m.rounding.draw(&mut r) * s).min(ROUND_SIZE_CAP * span[0].min(span[1]));
    let mut poly = raw;
    if signed_area(&poly) < 0.0 {
        poly.reverse();
    }
    // a stone that rests on another chamfers its bottom corners
    let below = if c.is_bottom || run.laid {
        f64::NEG_INFINITY
    } else {
        centroid(&poly)[1]
    };
    // the chamfer at least as big as any rounding the stone below may
    // have, so the corner under it never roofs a rounded notch
    let ch = (m.rounding.hi * s)
        .min(ROUND_SIZE_CAP * span[0].min(span[1]))
        .max(rr);
    let rounded = round_corners(&poly, rr, m.joint * s / 2.0, below, ch);
    if rounded.len() < 3 || signed_area(&rounded) < 0.25 * s * s {
        return None;
    }
    let mut ring = segmentize(&rounded, RING_STEP * s);
    // inward ring noise: irregular silhouettes, cracks that open and
    // close along their run (inward only, so no overlap is added)
    let nrm = normals(&ring);
    let n = ring.len();
    let seglen: Vec<f64> = (0..n)
        .map(|i| len2(sub(ring[(i + 1) % n], ring[i])))
        .collect();
    let per: f64 = seglen.iter().sum();
    let mut arc = vec![0.0; n];
    for i in 1..n {
        arc[i] = arc[i - 1] + seglen[i - 1];
    }
    let lams = [per / r.uniform(1.5, 3.5), per / r.uniform(1.5, 3.5)];
    let tau = 2.0 * core::f64::consts::PI;
    let phs = [r.uniform(0.0, tau), r.uniform(0.0, tau)];
    for i in 0..n {
        let mut wgt = 1.0;
        for k in 0..2 {
            wgt *= 0.5 * (1.0 + libm::sin(tau * arc[i] / lams[k] + phs[k]));
        }
        let d = (RING_NOISE.0 + (RING_NOISE.1 - RING_NOISE.0) * wgt) * s;
        ring[i][0] -= nrm[i][0] * d;
        ring[i][1] -= nrm[i][1] * d;
    }
    let recess = |r: &mut Rng| {
        if r.random() < PROUD_DEEP_PROB {
            r.uniform(PROUD_DEEP.0, PROUD_DEEP.1) * s
        } else {
            r.uniform(PROUD.0, PROUD.1) * s
        }
    };
    let y0 = recess(&mut r);
    let mut y1 = run.across - recess(&mut r);
    if run.laid {
        y1 = run.across + 1.0;
    }
    let ctr = centroid(&ring);
    let a = [
        r.uniform(MORPH_A.0, MORPH_A.1),
        r.uniform(MORPH_A.0, MORPH_A.1),
    ];
    let b = [
        r.uniform(MORPH_B.0, MORPH_B.1),
        r.uniform(MORPH_B.0, MORPH_B.1),
    ];
    let pz = r.uniform(BED_FLAT_EXP.0, BED_FLAT_EXP.1);
    let mut pole = || {
        [
            ctr[0] + r.uniform(-POLE_DRIFT, POLE_DRIFT) * span[0],
            ctr[1] + r.uniform(-POLE_DRIFT, POLE_DRIFT) * span[1],
        ]
    };
    let poles = [pole(), pole()];
    let relief = match m.relief {
        Some(v) if v <= 0.0 => None,
        rel => {
            let foot = span[0].max(span[1]).max(y1 - y0);
            let amp = rel.map_or((0.05 * foot).clamp(0.15 * s, 0.5 * s), |v| v * s);
            Some(Relief {
                scale_mm: (foot / 2.4).max(2.2 * s),
                carve_mm: amp,
                band: 0.45,
                dish_mm: 0.5 * amp,
                ..Relief::default()
            })
        }
    };
    // a face on the tile boundary sits back past the relief's outward
    // swell, so the boundary cut never planes a stone into a shelf
    let (mut y0, mut y1) = (y0, y1);
    if !run.laid {
        let swell = relief.as_ref().map_or(0.0, |r| r.dish_mm + r.grain_mm) + 0.05;
        if run.on_edge(c.seg, [c.t0, 0.0], [c.t1, 0.0]) {
            y0 = y0.max(swell);
        }
        if run.on_edge(c.seg, [c.t0, run.across], [c.t1, run.across]) {
            y1 = y1.min(run.across - swell);
        }
    }
    let under = if c.is_bottom || run.laid || c.coping {
        None
    } else {
        Some(c.z0)
    };
    Some(Stone {
        seg: c.seg,
        // the outline's own reach along the wall (drift and head overlap
        // carry it past the cell)
        t: [lo[0], hi[0]],
        z: [c.z0, c.z1],
        ring,
        y0,
        y1,
        ctr,
        poles,
        a,
        b,
        pz,
        coping: c.coping,
        under,
        under_from: UNDER_FROM * s,
        relief,
        seed: bs.key("relief").value(),
    })
}

/// The most a stone's face may stand proud of the face of a stone it
/// rests on, mm (a printability floor: it does not scale). Tighter than
/// the blocks' 0.45: a pebble's underside curls back from its face, so
/// the crack under it is deeper than the step.
const HOLD_PROUD_MM: f64 = 0.2;

/// Hold every stone's faces within [`HOLD_PROUD_MM`] of the faces of the
/// stones it rests on (bottom up, so a held stone holds the next): its
/// underside then never roofs the crack under a stone sunk deeper.
pub(crate) fn hold_proud(stones: &mut [Stone]) {
    let mut order: Vec<usize> = (0..stones.len()).collect();
    order.sort_by(|&a, &b| stones[a].z[0].total_cmp(&stones[b].z[0]).then(a.cmp(&b)));
    for &i in &order {
        if stones[i].under.is_none() {
            continue;
        }
        let (seg, t, z0) = (stones[i].seg, stones[i].t, stones[i].z[0]);
        let mut lo = f64::NEG_INFINITY;
        let mut hi = f64::INFINITY;
        for b in stones.iter() {
            if b.seg == seg && libm::fabs(b.z[1] - z0) < 0.6 && b.t[0].max(t[0]) < b.t[1].min(t[1])
            {
                lo = lo.max(b.y0 - HOLD_PROUD_MM);
                hi = hi.min(b.y1 + HOLD_PROUD_MM);
            }
        }
        let st = &mut stones[i];
        st.y0 = st.y0.max(lo);
        st.y1 = st.y1.min(hi);
    }
}

/// Pinning stones: a wedge driven into the top of each head joint that a
/// stone above bridges. Two pebbles' rounded top corners open a V there,
/// and the stone above roofs it wider than a layer can reach; the wedge
/// fills the V's mouth and points down at 45° (its flanks print), set
/// back [`PIN_SETBACK_MM`] from the faces. Returns (segment, wall-local
/// mesh) pairs.
pub(crate) fn joint_pins(stones: &[Stone], s: f64) -> Result<Vec<(usize, TriMesh)>, KitError> {
    let mut out = Vec::new();
    for a in stones.iter().filter(|a| !a.coping) {
        for b in stones.iter().filter(|b| !b.coping && b.seg == a.seg) {
            // b the right-hand neighbour of a in the same course
            if b.t[0] <= a.t[0]
                || b.t[0] > a.t[1] + PIN_GAP_MM * s
                || libm::fabs(a.z[1] - b.z[1]) > 0.6 * s
            {
                continue;
            }
            let top = a.z[1].min(b.z[1]);
            // a stone above spanning the joint
            let tm = 0.5 * (a.t[1] + b.t[0]);
            let bridged = stones.iter().any(|u| {
                u.seg == a.seg
                    && libm::fabs(u.z[0] - top) < 0.6 * s
                    && u.t[0] < tm - 0.5 * s
                    && u.t[1] > tm + 0.5 * s
            });
            if !bridged {
                continue;
            }
            let w = PIN_HALF_MM * s;
            let (y0, y1) = (
                a.y0.max(b.y0) + PIN_SETBACK_MM * s,
                a.y1.min(b.y1) - PIN_SETBACK_MM * s,
            );
            if y1 - y0 < s {
                continue;
            }
            let zt = top + PIN_EMBED_MM * s;
            let mut pts = Vec::with_capacity(6);
            for y in [y0, y1] {
                pts.push([tm - w, y, zt]);
                pts.push([tm + w, y, zt]);
                pts.push([tm, y, top - w]);
            }
            out.push((a.seg, dt_kit::form::hull(&pts)?));
        }
    }
    Ok(out)
}

impl Stone {
    /// The stone's box in plan (its outline's reach along the run, and its
    /// cell across), world mm for a run laid along the south edge.
    pub fn plan_box(&self) -> ([f64; 2], [f64; 2]) {
        (self.t, self.z)
    }

    /// The pebble in wall-local (t, q, z): latitude rings from the front
    /// pole (q = y0) to the back (q = y1), each the outline scaled about
    /// its drifting pole, the equator the outline; then the relief.
    pub fn mesh(&self) -> Result<TriMesh, KitError> {
        let n = self.ring.len();
        let k_lat = N_LAT;
        let h = 0.5 * (self.y1 - self.y0);
        let ym = 0.5 * (self.y0 + self.y1);
        let zlo = self.ring.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let mut pos: Vec<[f64; 3]> = Vec::with_capacity(k_lat * n + 2);
        for j in 0..k_lat {
            #[allow(clippy::cast_precision_loss)]
            let lat = -core::f64::consts::FRAC_PI_2
                + core::f64::consts::PI * (j + 1) as f64 / (k_lat + 1) as f64;
            let u = libm::fabs(libm::sin(lat));
            let side = usize::from(lat >= 0.0);
            let (ae, be, pole) = (self.a[side], self.b[side], self.poles[side]);
            let sk = libm::pow(1.0 - libm::pow(u, ae), be);
            let szk = libm::pow(sk, self.pz);
            let (st, sz) = if self.coping { (szk, sk) } else { (sk, szk) };
            let y = ym + h * libm::sin(lat);
            for p in &self.ring {
                let t =
                    self.ctr[0] + (pole[0] - self.ctr[0]) * (1.0 - st) + (p[0] - self.ctr[0]) * st;
                let mut z =
                    self.ctr[1] + (pole[1] - self.ctr[1]) * (1.0 - sz) + (p[1] - self.ctr[1]) * sz;
                if self.under.is_some() && !self.coping {
                    // a resting stone's underside leaves its bed no
                    // shallower than 45°: a floor rising from the bed at
                    // 45° toward each face from `under_from` in lifts
                    // every point under it (one plane for the whole ring,
                    // so nothing folds); where the dome is steeper the
                    // dome stays (a convex dome, past the crossing, only
                    // steepens). The floor stays under the middle.
                    let d = if lat < 0.0 { y - self.y0 } else { self.y1 - y };
                    let from = self.under_from.min(0.9 * (self.ctr[1] - zlo));
                    z = z.max(zlo + (from - d).max(0.0));
                }
                pos.push([t, y, z.max(zlo - 1.0)]);
            }
        }
        pos.push([self.poles[0][0], self.y0, self.poles[0][1]]);
        pos.push([self.poles[1][0], self.y1, self.poles[1][1]]);
        let mut tris: Vec<[u32; 3]> = Vec::with_capacity(2 * k_lat * n);
        let id = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        for k in 0..k_lat - 1 {
            let (a0, b0) = (k * n, (k + 1) * n);
            for i in 0..n {
                let j = (i + 1) % n;
                tris.push([id(a0 + i), id(a0 + j), id(b0 + j)]);
                tris.push([id(a0 + i), id(b0 + j), id(b0 + i)]);
            }
        }
        for i in 0..n {
            let j = (i + 1) % n;
            tris.push([id(k_lat * n), id(j), id(i)]);
            tris.push([
                id(k_lat * n + 1),
                id((k_lat - 1) * n + i),
                id((k_lat - 1) * n + j),
            ]);
        }
        let mut mesh = TriMesh {
            positions: pos,
            triangles: tris,
            ..TriMesh::default()
        };
        if dt_kit::signed_volume(&mesh) < 0.0 {
            for t in &mut mesh.triangles {
                t.swap(1, 2);
            }
        }
        if let Some(r) = &self.relief {
            mesh = subdivide(&mesh);
            // the underside keeps the relief off its downward faces (a
            // carved pocket there is a roof the printer cannot lay)
            let under = self.under.is_some();
            // along rays from the centre: the pole caps pack their rings
            // closer than the relief is deep, and normal relief folds
            // there into fins
            let centre = [self.ctr[0], 0.5 * (self.y0 + self.y1), self.ctr[1]];
            relief_radial(&mut mesh, self.seed, r, centre, &|_, nv| {
                if under && nv[2] < -0.05 {
                    ((nv[2] + 0.45) / 0.4).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            });
        }
        Ok(mesh)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn corners_round_and_rings_close() {
        let sq = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 6.0], [0.0, 6.0]];
        let r = round_corners(&sq, 1.5, 0.0, f64::NEG_INFINITY, 1.5);
        assert!(r.len() >= 4 * 7);
        let a = signed_area(&r);
        assert!(a < 60.0 && a > 55.0, "{a}");
        let seg = segmentize(&r, 1.2);
        assert!(seg.len() > r.len());
    }
}
