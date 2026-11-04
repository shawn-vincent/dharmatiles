//! One stone, finished (rockpile.py :1437-1597, 1674-1797): cut to its
//! column's planes, grooved under the stone it carries, seated on its bed,
//! chipped, carved on its faces, cracked across its top, fitted to the
//! tile, grained.

use std::collections::HashSet;

use dt_kit::rays::RayMesh;
use dt_kit::{KitError, Piece, Rng, Slot, Support, TriMesh, form, hung, welded};

use super::chips::{CHIP_MM, adjacency, chip_edges, corner_chips, face_normals};
use super::faces::{Column, Face};
use super::finish::{
    COLUMN_STEP_MAX, Cutter, SEAT_SLOPE_DEG, occupied, offset_poly, open_faces, prism, top_z_poly,
};
use super::form::{FOOT, Form, GRID_MM};
use super::relief::relief;
use super::stack::{Placed, SINK_MM};
use super::util::{Field, P2, area, bounds, norm2, point_in, ring_dist, smooth01, trace_outline};

const JOINT_MM: (f64, f64) = (0.8, 1.2);
const JOINT_MIN_MM: f64 = 4.0;
const SHOULDER_P: f64 = 0.6;
const SHOULDER_MM: (f64, f64) = (0.8, 2.5);
const JOINT_OVERLAP_MM: f64 = 0.03;
const CHIP_MAX_MM3: f64 = 3.0;
pub const BEVEL_MM: f64 = 0.8;
const SEAT_GRID_MM: f64 = 0.2;
const SEAT_SPAN_MM: f64 = 1.0;
const SEAT_KEEP_MM: (f64, f64) = (4.5, 1.0);
const FACE_GRAIN_MM: f64 = 0.25;
const FACE_GRAIN_LEN_MM: f64 = 1.0;
const FIT_GAP_MM: f64 = 0.02;
const CRACK_DEPTH_MM: (f64, f64) = (0.5, 0.65);
const CRACK_SLOT_MM: (f64, f64) = (0.4, 0.5);
const TEX_EDGE_MM: f64 = 0.25;
const SHARP_EDGE_DEG: f64 = 18.0;
const EDGE_KEEP_MM: f64 = 0.5;
const TEX_BASE_FADE_MM: f64 = 0.8;
const Z_UP: [f64; 3] = [0.0, 0.0, 1.0];

type V = [f64; 3];

pub struct Env<'a> {
    pub form: &'a Form,
    pub s: &'a Field,
    pub placed: &'a [Placed],
    pub seed: u64,
}

fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Each convex vertical corner cut off by a vertical plane across its
/// bisector, unless a stone rests over it (`loaded`).
fn chamfer_corners(
    mut rock: Piece,
    poly: &[P2],
    d: f64,
    loaded: &dyn Fn(P2) -> bool,
) -> Result<Piece, KitError> {
    let n = poly.len();
    let Some((lo, hi)) = rock.bounds() else {
        return Ok(rock);
    };
    for k in 0..n {
        let (a, b, c) = (poly[(k + n - 1) % n], poly[k], poly[(k + 1) % n]);
        let e1 = [b[0] - a[0], b[1] - a[1]];
        let e2 = [c[0] - b[0], c[1] - b[1]];
        let (l1, l2) = (norm2(e1), norm2(e2));
        if e1[0] * e2[1] - e1[1] * e2[0] <= 0.0 || l1.min(l2) < 2.0 {
            continue;
        }
        let (u1, u2) = ([e1[0] / l1, e1[1] / l1], [e2[0] / l2, e2[1] / l2]);
        let mut m = [u1[1] + u2[1], -u1[0] - u2[0]];
        let ml = norm2(m);
        m = [m[0] / ml, m[1] / ml];
        if loaded([b[0] - 0.6 * m[0], b[1] - 0.6 * m[1]]) {
            continue;
        }
        let k1 = d / (m[0] * u1[0] + m[1] * u1[1]).max(1e-6);
        let k2 = d / (-(m[0] * u2[0] + m[1] * u2[1])).max(1e-6);
        let p1 = [b[0] - u1[0] * k1.min(l1), b[1] - u1[1] * k1.min(l1)];
        let p2 = [b[0] + u2[0] * k2.min(l2), b[1] + u2[1] * k2.min(l2)];
        let p3 = [b[0] + 2.0 * m[0], b[1] + 2.0 * m[1]];
        let mut pts = Vec::new();
        for q in [p1, p2, p3] {
            for z in [lo[2] - 1.0, hi[2] + 1.0] {
                pts.push([q[0], q[1], z]);
            }
        }
        if let Ok(h) = form::hull(&pts) {
            let cut = rock.minus(&[h])?;
            if !cut.is_empty() {
                rock = cut;
            }
        }
    }
    Ok(rock)
}

/// The bed a rock is seated on, every low strip narrower than SEAT_SPAN
/// bridged (a grey closing).
fn seat_bed(r: &Placed) -> Field {
    let nmax = r
        .bed
        .v
        .iter()
        .copied()
        .filter(|v| !v.is_nan())
        .fold(f64::NEG_INFINITY, f64::max);
    let mut filled = r.bed.clone();
    for (q, v) in filled.v.iter_mut().enumerate() {
        if !r.cells.v[q] {
            *v = nmax;
        }
    }
    let rad = 0.5 * SEAT_SPAN_MM / GRID_MM;
    let closed = super::util::erode_disc(&super::util::dilate_disc(&filled, rad), rad);
    let mut out = r.bed.clone();
    for q in 0..out.v.len() {
        out.v[q] = if r.cells.v[q] {
            r.bed.v[q].max(closed.v[q])
        } else {
            f64::NAN
        };
    }
    out
}

/// Seat a resting rock on the stones under it (its underside one surface
/// at the overhang limit starting BEVEL up its side at the outline).
#[allow(clippy::too_many_lines)]
fn seat(rock: Piece, r: &Placed, opens: &[(P2, P2)], own: &[bool]) -> Result<Piece, KitError> {
    let Some((lo3, hi3)) = rock.bounds() else {
        return Ok(rock);
    };
    let (lo, hi) = (lo3[2], hi3[2]);
    let bed = seat_bed(r);
    let mut bv: Vec<f64> = bed.v.iter().copied().filter(|v| !v.is_nan()).collect();
    bv.sort_by(f64::total_cmp);
    if bv.is_empty() {
        return Ok(rock);
    }
    let seat_mid = bv[bv.len() / 2];
    let slope = SEAT_SLOPE_DEG.to_radians().tan();
    // the outline the bevel starts from: the rock at the bevel's top, and
    // wherever its sides flare out below that
    let zs = [seat_mid + BEVEL_MM, lo + 0.05].map(|z| z.clamp(lo + 0.05, hi - 0.05));
    let mut polys: Vec<Vec<[f64; 2]>> = Vec::new();
    for z in zs {
        polys.extend(rock.slice(z));
    }
    if polys.is_empty() {
        return Ok(rock);
    }
    // union by raster, then trace
    let (mut b0, mut b1) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in polys.iter().flatten() {
        b0 = [b0[0].min(p[0]), b0[1].min(p[1])];
        b1 = [b1[0].max(p[0]), b1[1].max(p[1])];
    }
    let res = 0.05;
    let (nx, ny) = (
        ((b1[0] - b0[0]) / res).ceil() as usize + 2,
        ((b1[1] - b0[1]) / res).ceil() as usize + 2,
    );
    let mut m = super::util::Mask::new(nx, ny, false);
    for j in 0..ny {
        for i in 0..nx {
            let p = [
                b0[0] + (i as f64 + 0.5) * res,
                b0[1] + (j as f64 + 0.5) * res,
            ];
            if polys.iter().any(|q| point_in(q, p)) {
                m.set(i, j, true);
            }
        }
    }
    let ring0 = trace_outline(&m, res);
    if ring0.len() < 3 {
        return Ok(rock);
    }
    let ring: Vec<P2> = super::util::simplify_ring(&ring0, 0.03)
        .into_iter()
        .map(|p| [p[0] + b0[0], p[1] + b0[1]])
        .collect();
    // bed under every point (nearest footprint cell)
    let valid = bed.map(|v| !v.is_nan());
    let (_, near) = dt_kit::distance_nearest(&valid.v, bed.nx, bed.ny);
    let bed_at = |p: P2| -> f64 {
        let i = ((p[0] / GRID_MM) as i64).clamp(0, bed.nx as i64 - 1) as usize;
        let j = ((p[1] / GRID_MM) as i64).clamp(0, bed.ny as i64 - 1) as usize;
        near[j * bed.nx + i].map_or(seat_mid, |k| bed.v[k])
    };
    let bb = bounds(&ring);
    let (x0, y0, x1, y1) = (bb[0] - 0.6, bb[1] - 0.6, bb[2] + 0.6, bb[3] + 0.6);
    let gx = ((x1 - x0) / SEAT_GRID_MM).floor() as usize + 2;
    let gy = ((y1 - y0) / SEAT_GRID_MM).floor() as usize + 2;
    // points along the ring
    let mut bpts: Vec<P2> = Vec::new();
    {
        let n = ring.len();
        let mut acc = 0.0;
        let mut next = 0.0;
        for k in 0..n {
            let (a, b) = (ring[k], ring[(k + 1) % n]);
            let l = norm2([b[0] - a[0], b[1] - a[1]]);
            while next <= acc + l {
                let t = (next - acc) / l.max(1e-12);
                bpts.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
                next += SEAT_GRID_MM;
            }
            acc += l;
        }
    }
    let mut q = Vec::with_capacity(gx * gy);
    for j in 0..gy {
        for i in 0..gx {
            q.push([x0 + i as f64 * SEAT_GRID_MM, y0 + j as f64 * SEAT_GRID_MM]);
        }
    }
    let inside: Vec<bool> = q.iter().map(|&p| point_in(&ring, p)).collect();
    let d: Vec<f64> = q.iter().map(|&p| slope * ring_dist(&ring, p)).collect();
    let dmax = (0..q.len())
        .filter(|&k| inside[k])
        .map(|k| d[k])
        .fold(0.0, f64::max);
    let bev = (dmax - SINK_MM).clamp(0.2, BEVEL_MM);
    let hb: Vec<f64> = bpts.iter().map(|&p| bed_at(p) + bev).collect();
    let local: Vec<f64> = q.iter().map(|&p| bed_at(p) + bev).collect();
    let mut under = vec![0.0; q.len()];
    for k in 0..q.len() {
        under[k] = if inside[k] {
            let mut best = f64::NEG_INFINITY;
            for (b, &h) in bpts.iter().zip(&hb) {
                let dd = slope * norm2([q[k][0] - b[0], q[k][1] - b[1]]);
                best = best.max(h - dd);
            }
            best.min(local[k] - d[k])
        } else {
            local[k] + d[k]
        };
    }
    if !opens.is_empty() && own.iter().any(|&b| b) {
        // eroded own cells (disc of 0.5 mm = 2 cells)
        let ownm = super::util::Mask {
            nx: bed.nx,
            ny: bed.ny,
            v: own.to_vec(),
        };
        let owf = ownm.map(|b| if b { 1.0 } else { 0.0 });
        let er = super::util::erode_disc(&owf, 0.5 / GRID_MM);
        for k in 0..q.len() {
            if !inside[k] {
                continue;
            }
            let dmin = opens
                .iter()
                .map(|&(a, b)| super::util::seg_dist(q[k], a, b))
                .fold(f64::INFINITY, f64::min);
            if dmin < SEAT_KEEP_MM.0 {
                let i = ((q[k][0] / GRID_MM) as i64).clamp(0, bed.nx as i64 - 1) as usize;
                let j = ((q[k][1] / GRID_MM) as i64).clamp(0, bed.ny as i64 - 1) as usize;
                if er.at(i, j) > 0.5 {
                    under[k] = under[k].max(local[k] - bev - SEAT_KEEP_MM.1);
                }
            }
        }
    }
    let zh = hi + 1.0;
    let depth: Vec<f64> = under.iter().map(|&u| zh - u).collect();
    let mut keep = hung(&depth, gx, gy, SEAT_GRID_MM, 0.01)?;
    for p in &mut keep.positions {
        *p = [x0 + p[0], y0 + p[1], zh - p[2]];
    }
    keep.triangles.iter_mut().for_each(|t| t.swap(1, 2)); // mirrored in z
    let umin = under.iter().copied().fold(f64::INFINITY, f64::min);
    let deep = umin - 0.1;
    let rock2 = if deep < lo {
        rock.pad(lo + 0.05, deep, lo + 0.06)?
    } else {
        rock.clone()
    };
    let out = rock2.within(&keep)?;
    Ok(if out.is_empty() { rock } else { out })
}

/// A crack reaching the top of a stone whose top is bare runs on across
/// it: from where it crosses the top's front edge straight back until the
/// top ends, a slot whose floor is CRACK_DEPTH under the top, open above.
fn top_cracks(
    rng: &mut Rng,
    rock: &Piece,
    r: &Placed,
    faces: &[std::sync::Arc<Face>],
    env: &Env<'_>,
) -> Result<(Piece, usize), KitError> {
    let t = rock.mesh();
    if t.positions.is_empty() {
        return Ok((rock.clone(), 0));
    }
    let zmax = t
        .positions
        .iter()
        .map(|p| p[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let rm = RayMesh::new(&t);
    let mut cuts: Vec<TriMesh> = Vec::new();
    for fc in faces {
        let u: Vec<f64> = t
            .positions
            .iter()
            .map(|p| (p[0] - fc.p0[0]) * fc.t[0] + (p[1] - fc.p0[1]) * fc.t[1])
            .collect();
        let sm_: Vec<f64> = t
            .positions
            .iter()
            .map(|p| (p[0] - fc.p0[0]) * fc.m[0] + (p[1] - fc.p0[1]) * fc.m[1])
            .collect();
        let smax = sm_.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let (umin, umax) = (
            u.iter().copied().fold(f64::INFINITY, f64::min),
            u.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        );
        if smax < -3.0 || umax.min(fc.l) - umin.max(0.0) < 1.0 {
            continue;
        }
        let (ua, ub) = (umin.max(0.0), umax.min(fc.l));
        let us: Vec<f64> = (0..24).map(|k| ua + (ub - ua) * k as f64 / 23.0).collect();
        let edge: Vec<P2> = us
            .iter()
            .map(|&uu| {
                [
                    uu,
                    r.top_z(fc.p0[0] + fc.t[0] * uu, fc.p0[1] + fc.t[1] * uu),
                ]
            })
            .collect();
        let eline = super::lines::Line::new(edge);
        for cs in &fc.cracks {
            for q in cross_lines(cs, &eline) {
                let th = rng.normal(0.0, 6.0).to_radians();
                let d2 = [
                    -fc.m[0] * libm::cos(th) + fc.t[0] * libm::sin(th),
                    -fc.m[1] * libm::cos(th) + fc.t[1] * libm::sin(th),
                ];
                let d3 = [d2[0], d2[1], 0.0];
                let o = [
                    fc.p0[0] + fc.t[0] * q[0] + fc.m[0] * (smax + 0.5),
                    fc.p0[1] + fc.t[1] * q[0] + fc.m[1] * (smax + 0.5),
                    0.0,
                ];
                let ns = 400;
                let ss: Vec<f64> = (0..ns).map(|k| k as f64 * 0.1).collect();
                let org: Vec<V> = ss
                    .iter()
                    .map(|&s| [o[0] + s * d3[0], o[1] + s * d3[1], zmax + 2.0])
                    .collect();
                let hits = rm.first_hits(&org, &vec![[0.0, 0.0, -1.0]; ns]);
                let h: Vec<f64> = hits
                    .iter()
                    .map(|h| h.map_or(f64::NAN, |h| h.point[2]))
                    .collect();
                let ok: Vec<bool> = (0..ns)
                    .map(|k| {
                        !h[k].is_nan()
                            && h[k] > r.top_z(org[k][0], org[k][1]) - 2.5
                            && !occupied(env.form, env.s, [org[k][0], org[k][1], h[k] + 0.4])
                    })
                    .collect();
                let Some(i0) = ok.iter().position(|&b| b) else {
                    continue;
                };
                let i1 = match ok[i0..].iter().position(|&b| !b) {
                    Some(k) => i0 + k - 1,
                    None => ns - 1,
                };
                if i0 > 40 || i1 < i0 + 15 {
                    continue;
                }
                let dd = rng.uniform(CRACK_DEPTH_MM.0, CRACK_DEPTH_MM.1);
                // minimum filter (window 7) of the top
                let seg: Vec<f64> = (i0..=i1).map(|k| h[k]).collect();
                let fl: Vec<f64> = (0..seg.len())
                    .map(|k| {
                        let a = k.saturating_sub(3);
                        let b = (k + 3).min(seg.len() - 1);
                        seg[a..=b].iter().copied().fold(f64::INFINITY, f64::min) - dd
                    })
                    .collect();
                let flmax = fl.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let mut prof: Vec<[f64; 2]> = vec![[0.0, fl[0]]];
                prof.extend((i0..=i1).map(|k| [ss[k], fl[k - i0]]));
                let xe = ss[i1] + 0.3;
                prof.push([xe, fl[fl.len() - 1]]);
                prof.push([xe, flmax + 3.0]);
                prof.push([0.0, flmax + 3.0]);
                let w = rng.uniform(CRACK_SLOT_MM.0, CRACK_SLOT_MM.1);
                let side = [
                    d3[1] * Z_UP[2] - d3[2] * Z_UP[1],
                    d3[2] * Z_UP[0] - d3[0] * Z_UP[2],
                    d3[0] * Z_UP[1] - d3[1] * Z_UP[0],
                ];
                let mut m = dt_kit::prism_mesh(&[prof], 0.0, w);
                for p in &mut m.positions {
                    let (x, y, z) = (p[0], p[1], p[2]);
                    *p = [
                        d3[0] * x + Z_UP[0] * y + side[0] * z + o[0] - 0.5 * w * side[0],
                        d3[1] * x + Z_UP[1] * y + side[1] * z + o[1] - 0.5 * w * side[1],
                        d3[2] * x + Z_UP[2] * y + side[2] * z + o[2] - 0.5 * w * side[2],
                    ];
                }
                // columns d3, Z, side: right-handed, no flip
                cuts.push(m);
            }
        }
    }
    if cuts.is_empty() {
        return Ok((rock.clone(), 0));
    }
    let n = cuts.len();
    let out = rock.minus(&cuts)?;
    Ok(if out.is_empty() {
        (rock.clone(), 0)
    } else {
        (out, n)
    })
}

fn cross_lines(a: &super::lines::Line, b: &super::lines::Line) -> Vec<P2> {
    let mut out = Vec::new();
    for k in 1..b.pts.len() {
        out.extend(a.cross(b.pts[k - 1], b.pts[k]));
    }
    out
}

/// Never cut a finished stone at the tile boundary: one that reaches past
/// it is shrunk along that axis until its outermost point just touches it.
fn fit_tile(rock: Piece, form: &Form, setback: &[((usize, bool), f64)]) -> Piece {
    let Some((lo, hi)) = rock.bounds() else {
        return rock;
    };
    let mut m = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
    let mut any = false;
    for (ax, l) in [(0, form.w), (1, form.h)] {
        let sb = |hi_side: bool| {
            setback
                .iter()
                .find(|s| s.0 == (ax, hi_side))
                .map_or(0.0, |s| s.1)
        };
        let a = FIT_GAP_MM + sb(false);
        let b = l - FIT_GAP_MM - sb(true);
        let (l0, h0) = (lo[ax], hi[ax]);
        let (nl, nh) = (l0.max(a), h0.min(b));
        if ((nl - l0).abs() < 1e-12 && (nh - h0).abs() < 1e-12) || nh - nl < 0.5 * (h0 - l0) {
            continue;
        }
        let k = (nh - nl) / (h0 - l0);
        m[ax * 4] = k;
        m[9 + ax] = nl - k * l0;
        any = true;
    }
    if any { rock.transformed(&m) } else { rock }
}

/// The face grain: value-noise fBm FACE_GRAIN_MM deep, one field over the
/// whole tile so it runs on across the joints.
pub fn grain(rock: &Piece, seed: u64, z0: f64) -> Result<Piece, KitError> {
    let base = rock.mesh();
    if base.positions.is_empty() {
        return Ok(rock.clone());
    }
    let (bp, bt) = welded(&base);
    let bn = face_normals(&bp, &bt);
    // sharp edges
    let mut segs: Vec<(V, V)> = Vec::new();
    for (fa, fb, a, b) in adjacency(&bt) {
        if libm::acos(dot(bn[fa], bn[fb]).clamp(-1.0, 1.0)) > SHARP_EDGE_DEG.to_radians() {
            segs.push((bp[a as usize], bp[b as usize]));
        }
    }
    let (p, t) = rock.refined(TEX_EDGE_MM);
    // angle-weighted vertex normals, smoothed 4x
    let mut n = vec![[0.0; 3]; p.len()];
    for tr in &t {
        let v = tr.map(|i| p[i as usize]);
        let e1 = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
        let e2 = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
        let fnm = unitv(crossv(e1, e2));
        for k in 0..3 {
            let a = unitv(subv(v[(k + 1) % 3], v[k]));
            let b = unitv(subv(v[(k + 2) % 3], v[k]));
            let w = libm::acos(dot(a, b).clamp(-1.0, 1.0));
            for c in 0..3 {
                n[tr[k] as usize][c] += w * fnm[c];
            }
        }
    }
    let mut nb: Vec<Vec<u32>> = vec![Vec::new(); p.len()];
    for tr in &t {
        for k in 0..3 {
            let (a, b) = (tr[k], tr[(k + 1) % 3]);
            nb[a as usize].push(b);
            nb[b as usize].push(a);
        }
    }
    for l in &mut nb {
        l.sort_unstable();
        l.dedup();
    }
    for _ in 0..4 {
        let prev = n.clone();
        for (i, l) in nb.iter().enumerate() {
            if l.is_empty() {
                continue;
            }
            let mut s = [0.0; 3];
            for &j in l {
                for c in 0..3 {
                    s[c] += prev[j as usize][c];
                }
            }
            n[i] = s.map(|x| x / l.len() as f64);
        }
    }
    let n: Vec<V> = n.into_iter().map(unitv).collect();
    // edge distance by cell hashing of the sharp segments
    let cell = 0.5;
    let key = |x: f64, y: f64, z: f64| {
        (
            (x / cell).floor() as i64,
            (y / cell).floor() as i64,
            (z / cell).floor() as i64,
        )
    };
    let mut hash: std::collections::HashMap<(i64, i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    for (si, (a, b)) in segs.iter().enumerate() {
        let lo = key(
            a[0].min(b[0]) - EDGE_KEEP_MM,
            a[1].min(b[1]) - EDGE_KEEP_MM,
            a[2].min(b[2]) - EDGE_KEEP_MM,
        );
        let hi = key(
            a[0].max(b[0]) + EDGE_KEEP_MM,
            a[1].max(b[1]) + EDGE_KEEP_MM,
            a[2].max(b[2]) + EDGE_KEEP_MM,
        );
        for i in lo.0..=hi.0 {
            for j in lo.1..=hi.1 {
                for k in lo.2..=hi.2 {
                    hash.entry((i, j, k)).or_default().push(si);
                }
            }
        }
    }
    let rm = RayMesh::new(&base);
    let org: Vec<V> = p
        .iter()
        .zip(&n)
        .map(|(q, nv)| {
            [
                q[0] + 0.02 * nv[0],
                q[1] + 0.02 * nv[1],
                q[2] + 0.02 * nv[2],
            ]
        })
        .collect();
    let hits = rm.first_hits(&org, &n);
    let mut out = p.clone();
    for (i, q) in p.iter().enumerate() {
        let gr = form::fbm3(*q, seed ^ 0x5A5A, FACE_GRAIN_LEN_MM, 3);
        let mut dmin = f64::INFINITY;
        if let Some(l) = hash.get(&key(q[0], q[1], q[2])) {
            for &si in l {
                let (a, b) = segs[si];
                let ab = subv(b, a);
                let tt = (dot(subv(*q, a), ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
                let c = [a[0] + tt * ab[0], a[1] + tt * ab[1], a[2] + tt * ab[2]];
                dmin = dmin.min(norm3(subv(*q, c)));
            }
        }
        let we = smooth01(if dmin.is_finite() {
            dmin / EDGE_KEEP_MM
        } else {
            1e9
        });
        let mut w =
            smooth01((q[2] - z0 - 0.15) / TEX_BASE_FADE_MM) * smooth01((n[i][2] + 0.5) / 0.4);
        let gap = hits[i].map_or(f64::INFINITY, |h| norm3(subv(h.point, *q)));
        w *= smooth01((gap - 0.8) / 0.8);
        let amp = FACE_GRAIN_MM * gr * (0.5 + 0.5 * we) * w;
        out[i] = [
            q[0] + amp * n[i][0],
            q[1] + amp * n[i][1],
            q[2] + amp * n[i][2],
        ];
    }
    let g = rock.rebuilt(out, t)?;
    Ok(if g.is_empty() { rock.clone() } else { g })
}

fn subv(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn crossv(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm3(a: V) -> f64 {
    libm::sqrt(dot(a, a))
}
fn unitv(a: V) -> V {
    let l = norm3(a);
    if l > 0.0 { a.map(|x| x / l) } else { a }
}

/// Per-column mutable state while its stones are built (top down).
#[derive(Default)]
pub struct ColState {
    pub refused: HashSet<usize>,
    pub tops: usize,
}

/// Build stone `ri` of column `c`. `support` is what a cut must not undercut.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn rock_piece(
    rng: &mut Rng,
    crng: &mut Rng,
    ri: usize,
    c: &Column,
    st: &mut ColState,
    env: &Env<'_>,
    support: &[&Support],
    slot: Slot,
) -> Result<Option<Piece>, KitError> {
    let form = env.form;
    let r = &env.placed[ri];
    let rpoly = r.poly();
    if rpoly.len() < 3 {
        return Ok(None);
    }
    let poly = offset_poly(&rpoly, JOINT_OVERLAP_MM);
    let bb = bounds(&poly);
    let zhi = [
        [bb[0], bb[1]],
        [bb[2], bb[1]],
        [bb[2], bb[3]],
        [bb[0], bb[3]],
    ]
    .iter()
    .map(|p| r.top_z(p[0], p[1]))
    .fold(f64::NEG_INFINITY, f64::max)
        + 1.0;
    // the outline offset a hair can self-cross on a tiny feature: then the
    // outline itself, then its convex hull
    let hull = super::util::hull2(&rpoly);
    let mut tried = [poly.clone(), rpoly.clone(), hull].into_iter();
    let rock = loop {
        let Some(p) = tried.next() else {
            return Ok(None);
        };
        match prism(slot, &p, r.z0, zhi - r.z0, r.top, form) {
            Ok(rock) => break rock,
            Err(_) => continue,
        }
    };
    if rock.is_empty() {
        return Ok(None);
    }
    // a column on a tile-boundary wall stands back from it a little
    let mut setback: Vec<((usize, bool), f64)> = Vec::new();
    for &((ax, hi), sb) in &c.setback {
        let (nx, ny) = (r.cells.nx, r.cells.ny);
        let row: Vec<bool> = if ax == 0 {
            let i = if hi { nx - 1 } else { 0 };
            (0..ny).map(|j| r.cells.at(i, j)).collect()
        } else {
            let j = if hi { ny - 1 } else { 0 };
            (0..nx).map(|i| r.cells.at(i, j)).collect()
        };
        let any_n = if ax == 0 {
            (0..ny)
                .filter(|&j| (0..nx).any(|i| r.cells.at(i, j)))
                .count()
        } else {
            (0..nx)
                .filter(|&i| (0..ny).any(|j| r.cells.at(i, j)))
                .count()
        };
        let cnt = row.iter().filter(|&&b| b).count();
        if any_n > 0 && cnt as f64 >= 0.95 * any_n as f64 {
            setback.push(((ax, hi), sb));
        }
    }
    let loaded_at = |q: P2| occupied(form, env.s, [q[0], q[1], r.top_z(q[0], q[1]) + 0.4]);
    let rock = chamfer_corners(rock, &rpoly_offset_unused(&poly), c.corner_mm, &loaded_at)?;
    let Some((lo3, hi3)) = rock.bounds() else {
        return Ok(None);
    };
    let mesh_v = rock.mesh().positions;
    let (zb, ztop_r) = (lo3[2], hi3[2]);
    let faces = open_faces(&rpoly, form, env.s, zb, ztop_r);
    let opens: Vec<V> = faces.iter().map(|f| f.m).collect();
    let mut cutter = Cutter::new(rock, Some(support));
    let reach_dot = |d: V| {
        c.reach
            .iter()
            .map(|p| dot(*p, d))
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let reach_min = |d: V| {
        c.reach
            .iter()
            .map(|p| dot(*p, d))
            .fold(f64::INFINITY, f64::min)
    };
    for (k, cut) in c.cuts.iter().enumerate() {
        if st.refused.contains(&k) {
            continue;
        }
        if opens.iter().map(|&m| dot(cut.d, m)).fold(-1.0, f64::max) > 0.9 {
            let vmax = mesh_v
                .iter()
                .map(|p| dot(*p, cut.d))
                .fold(f64::NEG_INFINITY, f64::max);
            let mut sh = cut.d.map(|x| x * (vmax - reach_dot(cut.d)).min(0.0));
            if norm3(sh) <= 0.3 {
                sh = [0.0; 3];
            }
            let backv = c.back.iter().find(|b| b.0 == ri).map_or(0.0, |b| b.1);
            let ptp = reach_dot(cut.d) - reach_min(cut.d);
            let bk = backv.min((COLUMN_STEP_MAX * (ptp - 4.0)).max(0.0));
            let off = cut.off + dot(cut.n, subv(sh, cut.d.map(|x| x * bk)));
            if !cutter.take(cut.n, off, None, false) {
                st.refused.insert(k);
            }
        }
    }
    // its own top front edge: a 45 degree V under the stone it carries, or
    // on an open ledge top a broken shoulder
    let cen = super::util::centroid(&rpoly);
    let loaded = occupied(form, env.s, [cen[0], cen[1], ztop_r + 0.4]);
    if form.ztop.is_none_or(|zt| ztop_r < zt - 0.5) {
        for f in &faces {
            if f.rim {
                continue;
            }
            let qs: Vec<P2> = (0..7)
                .map(|k| {
                    let t = 0.05 + 0.9 * k as f64 / 6.0;
                    [
                        f.p0[0] + t * (f.p1[0] - f.p0[0]) - f.m[0],
                        f.p0[1] + t * (f.p1[1] - f.p0[1]) - f.m[1],
                    ]
                })
                .collect();
            if qs.iter().any(|q| {
                occupied(
                    form,
                    env.s,
                    [
                        q[0] + 2.0 * f.m[0],
                        q[1] + 2.0 * f.m[1],
                        r.top_z(q[0], q[1]) + 0.4,
                    ],
                )
            }) {
                continue;
            }
            let any_here = qs
                .iter()
                .any(|q| occupied(form, env.s, [q[0], q[1], r.top_z(q[0], q[1]) + 0.4]));
            let (n, dep, extra);
            if loaded || any_here {
                if norm2([f.p1[0] - f.p0[0], f.p1[1] - f.p0[1]]) < JOINT_MIN_MM {
                    continue;
                }
                dep = rng.uniform(JOINT_MM.0, JOINT_MM.1);
                n = unitv([f.m[0] + Z_UP[0], f.m[1] + Z_UP[1], f.m[2] + Z_UP[2]]);
                extra = 0.5;
            } else if rng.random() > SHOULDER_P * c.zone {
                continue;
            } else {
                let k = rng.uniform(0.6, 1.6);
                n = unitv([f.m[0], f.m[1], k]);
                dep = rng.uniform(SHOULDER_MM.0, SHOULDER_MM.1);
                extra = 1.0;
            }
            let q = [0.5 * (f.p0[0] + f.p1[0]), 0.5 * (f.p0[1] + f.p1[1])];
            let p = [
                q[0] - n[0] * dep,
                q[1] - n[1] * dep,
                r.top_z(q[0], q[1]) - n[2] * dep,
            ];
            cutter.take_back(n, p, dep, extra, None);
        }
    }
    let mut rock = cutter.solid;
    let mut zbase = zb;
    if zb > FOOT {
        // sits on rock: seated on the bed (not under its open faces where
        // its own column's stone carries it)
        let (nx, ny) = (r.cells.nx, r.cells.ny);
        let mut own = vec![false; nx * ny];
        for &s2 in &c.stones {
            if s2 != ri && env.placed[s2].z0 < r.z0 {
                let o = &env.placed[s2];
                for j in 0..ny {
                    for i in 0..nx {
                        let q = j * nx + i;
                        if o.cells.v[q] && r.cells.v[q] && !r.bed.v[q].is_nan() {
                            let zt =
                                o.top_z((i as f64 + 0.5) * GRID_MM, (j as f64 + 0.5) * GRID_MM);
                            if (zt - r.bed.v[q]).abs() < 0.5 {
                                own[q] = true;
                            }
                        }
                    }
                }
            }
        }
        let opens2: Vec<(P2, P2)> = faces.iter().map(|f| (f.p0, f.p1)).collect();
        rock = seat(rock, r, &opens2, &own)
            .map_err(|e| KitError::Shape(format!("stone {ri} seat: {e}")))?;
        zbase = r
            .bed
            .v
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .fold(f64::INFINITY, f64::min);
    }
    // broken corners and arrises
    let vol = rock.volume().max(1e-6);
    let down = rock.transformed(&[
        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -zbase,
    ]);
    let down = corner_chips(rng, down, r.d.max(4.0), 0.03_f64.min(CHIP_MAX_MM3 / vol));
    let rock = down.transformed(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, zbase]);
    let keep = |n1: V, n2: V, p: V| -> bool {
        let h = [n1[0] + n2[0], n1[1] + n2[1]];
        let hl = norm2(h).max(1e-6);
        let qs = [0.0, 1.0].map(|k| [p[0] - k * h[0] / hl, p[1] - k * h[1] / hl]);
        let wh = [form.w, form.h];
        for n in [n1, n2] {
            for ax in 0..2 {
                if n[ax].abs() > 0.9 && p[ax].min(wh[ax] - p[ax]) < 1.5 {
                    return true;
                }
            }
        }
        let cr = crossv(n1, n2);
        cr[2].abs() < 0.6 * norm3(cr)
            && p[2] > r.top_z(p[0], p[1]) - 1.6
            && qs
                .iter()
                .any(|q| occupied(form, env.s, [q[0], q[1], r.top_z(q[0], q[1]) + 0.4]))
    };
    let chips = chip_edges(crng, &rock, CHIP_MM, &keep);
    // each finishing step may fail on a degenerate cut (the kernel refuses a
    // mesh that is not a closed manifold): that step is then skipped, and
    // the stone keeps what it had
    let rock = if chips.is_empty() {
        rock
    } else {
        match rock.minus(&chips) {
            Ok(o) if !o.is_empty() => o,
            _ => rock,
        }
    };
    let rock = relief(rock.clone(), &c.faces, None, None)
        .and_then(|p| p.parts_over(1e9))
        .unwrap_or(rock);
    let rock = match top_cracks(rng, &rock, r, &c.faces, env) {
        Ok((p, k)) => match p.parts_over(1e9) {
            Ok(p) => {
                st.tops += k;
                p
            }
            Err(_) => rock,
        },
        Err(_) => rock,
    };
    let rock = fit_tile(rock, form, &setback);
    let grained = grain(&rock, env.seed, zbase).and_then(|p| p.parts_over(1e9));
    Ok(Some(grained.unwrap_or(rock)))
}

fn rpoly_offset_unused(p: &[P2]) -> Vec<P2> {
    let _ = area;
    p.to_vec()
}

#[allow(dead_code)]
fn _t(_: &Field) {
    let _ = top_z_poly;
}

#[cfg(test)]
mod tests {
    use super::*;
    use dt_kit::Convex;

    fn run(beds: [f64; 3]) -> (f64, f64) {
        use super::super::util::Mask;
        let (nx, ny) = (200, 200);
        let mut cells = Mask::new(nx, ny, false);
        let mut bed = Field::new(nx, ny, f64::NAN);
        for j in 40..64 {
            for i in 40..88 {
                cells.set(i, j, true);
                bed.set(i, j, beds[(i - 40) / 16]);
            }
        }
        let lo = beds.iter().copied().fold(f64::INFINITY, f64::min);
        let r = Placed {
            cells,
            z0: lo - 0.8,
            top: (24.0, 0.0, 0.0),
            d: 6.0,
            bed,
        };
        let rock = Convex::cuboid([10.0, 10.0, lo - 0.8], [22.0, 16.0, 24.0]).to_tri_mesh();
        let slot = {
            let mut sc =
                dt_kit::Scene::new(dt_kit::Frame::dungeonblocks(1, 1), dt_kit::Seed::new(1));
            sc.slots(1, dt_kit::Material::Rock).unwrap().get(0)
        };
        let piece = Piece::new(slot, &rock).unwrap();
        let seated = seat(piece, &r, &[], &vec![false; nx * ny]).unwrap();
        let mut u = seated;
        for (k, b) in beds.iter().enumerate() {
            let x0 = 10.0 + 4.0 * k as f64;
            let m = Convex::cuboid([x0 - 0.05, 9.0, 0.0], [x0 + 4.05, 17.0, *b]).to_tri_mesh();
            u = u.plus(&m).unwrap();
        }
        let (_, a) = dt_kit::audit(
            &u.mesh(),
            [35.0, 35.0],
            &dt_kit::Printer::new(0.45, 0.2, 55.0, 0.2),
        );
        eprintln!(
            "beds {beds:?}: overhang {:.3} band {:.3}",
            a.overhang_mm2, a.band_mm2
        );
        (a.overhang_mm2, a.band_mm2)
    }

    /// A narrow stone sinks into its bed (its underside is the seat's V).
    #[test]
    fn a_narrow_stone_sinks_into_its_bed() {
        use super::super::util::Mask;
        let (nx, ny) = (200, 200);
        let mut cells = Mask::new(nx, ny, false);
        let mut bed = Field::new(nx, ny, f64::NAN);
        for j in 40..49 {
            for i in 40..50 {
                cells.set(i, j, true);
                bed.set(i, j, 5.0);
            }
        }
        let r = Placed {
            cells,
            z0: 4.2,
            top: (12.0, 0.0, 0.0),
            d: 3.0,
            bed,
        };
        let rock = Convex::cuboid([10.0, 10.0, 4.2], [12.5, 12.25, 12.0]).to_tri_mesh();
        let slot = {
            let mut sc =
                dt_kit::Scene::new(dt_kit::Frame::dungeonblocks(1, 1), dt_kit::Seed::new(1));
            sc.slots(1, dt_kit::Material::Rock).unwrap().get(0)
        };
        let piece = Piece::new(slot, &rock).unwrap();
        let seated = seat(piece, &r, &[], &vec![false; nx * ny]).unwrap();
        let zmin = seated.bounds().unwrap().0[2];
        eprintln!("narrow stone bottom {zmin}");
        assert!(zmin < 4.6, "bottom {zmin}");
    }

    /// A box stone seated on stepped supports: the union must print.
    #[test]
    fn a_seat_has_no_overhang() {
        for beds in [
            [5.0, 5.0, 3.0],
            [5.0, 12.0, 5.0],
            [12.0, 5.0, 12.0],
            [8.0, 8.0, 8.0],
            [14.0, 8.0, 3.0],
        ] {
            let (o, _) = run(beds);
            assert!(o < 0.05, "{beds:?}: {o}");
        }
    }
}
