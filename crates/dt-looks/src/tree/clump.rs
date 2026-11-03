//! Foliage clumps (`_build_foliage_cluster_mesh`, mesh.py:713): a rounded
//! cone swept along the last stretch of a branch end, back cap → cone →
//! front dome, offset upward so the branch runs along its underside,
//! roughened by inward-only noise (fine grain and a coarse 4 mm lumpiness).
//!
//! Printable by construction: the Python clumps' undersides hang in the
//! air (about 150–220 mm² of a 1×1 tree's wood and clumps fail the
//! layer-reach audit there). Here the noise never dents an underside (a
//! dent there is a ceiling over air), and the crown is then cut with
//! [`dt_kit::stand`] (mod.rs), which bevels whatever still hangs at 37°
//! up from what holds it. Tops and sides, the silhouette from above and
//! from eye level, are the Python's.

use dt_kit::graph::{Curve, add, cross, dist, dot, norm, scale, sub, unit};
use dt_kit::{TriMesh, form};

use super::bark::hash01;

const BACK_ROUND_FRAC: f64 = 0.4;
const FINE_MM: f64 = 0.10;
const COARSE_MM: f64 = 1.0;
const COARSE_CELL_MM: f64 = 4.0;
const MAX_NOISE_MM: f64 = COARSE_MM + 2.0 * FINE_MM;
const N_THETA: usize = 28;
/// The noise is full where a vertex normal's z is above `NOISE_FULL_NZ`
/// and gone below `NOISE_NONE_NZ`.
const NOISE_FULL_NZ: f64 = -0.1;
const NOISE_NONE_NZ: f64 = -0.5;
const RING_RES_MM: f64 = 1.2;

/// One clump's inputs, in world mm.
#[derive(Clone, Debug)]
pub struct Spec {
    pub tip: [f64; 3],
    pub tip_tangent: [f64; 3],
    pub start: [f64; 3],
    pub start_tangent: [f64; 3],
    pub r_wood: f64,
    pub r_foliage: f64,
    pub length: f64,
    pub key: u64,
}

/// Where a clump of `length` starts on an edge's curve, walking back from
/// the tip (`_bezier_clump_start`); extended straight back along the
/// start tangent when the edge is shorter.
pub fn start_on(c: &Curve, length: f64) -> ([f64; 3], [f64; 3], f64) {
    let pts: Vec<[f64; 3]> = (0..64).map(|k| c.at(f64::from(k) / 63.0)).collect();
    let mut acc = 0.0;
    let mut t_start = 0.0;
    let mut found = false;
    for k in (1..64).rev() {
        let seg = dist(pts[k], pts[k - 1]);
        if acc + seg >= length {
            let f = (length - acc) / seg.max(1e-12);
            t_start = (f64::from(u32::try_from(k).unwrap_or(0)) - f) / 63.0;
            found = true;
            break;
        }
        acc += seg;
    }
    let t_start = if found { t_start.clamp(0.0, 1.0) } else { 0.0 };
    let pos = c.at(t_start);
    let tan = unit(c.tangent(t_start)).unwrap_or([0.0, 0.0, 1.0]);
    let covered = if found { length } else { acc };
    if covered < length - 1e-9 {
        (sub(pos, scale(tan, length - covered)), tan, length)
    } else {
        (pos, tan, length)
    }
}

fn up_perp(t: [f64; 3]) -> [f64; 3] {
    let p = sub([0.0, 0.0, 1.0], scale(t, t[2]));
    if norm(p) > 1e-6 {
        scale(p, 1.0 / norm(p))
    } else {
        [0.0; 3]
    }
}

/// The clump surface before the cut, outward wound.
pub fn mesh(s: &Spec) -> TriMesh {
    let r_base = s.r_wood;
    let r_tip = s.r_foliage;
    let r_back = r_base.max(BACK_ROUND_FRAC * r_tip);
    let tip_t = unit(s.tip_tangent).unwrap_or([0.0, 0.0, 1.0]);
    let start_t = unit(s.start_tangent).unwrap_or([0.0, 0.0, 1.0]);
    let (tip_p, start_p) = (s.tip, s.start);
    let d = dist(tip_p, start_p).max(1e-6);
    let spine = Curve {
        p: [
            start_p,
            add(start_p, scale(start_t, 0.45 * d)),
            sub(tip_p, scale(tip_t, 0.45 * d)),
            tip_p,
        ],
    };
    let n_sp = 64usize;
    let sp: Vec<[f64; 3]> = (0..64u32).map(|k| spine.at(f64::from(k) / 63.0)).collect();
    let st: Vec<[f64; 3]> = (0..64u32)
        .map(|k| unit(spine.tangent(f64::from(k) / 63.0)).unwrap_or(tip_t))
        .collect();
    let mut arc = vec![0.0];
    for w in sp.windows(2) {
        arc.push(arc[arc.len() - 1] + dist(w[0], w[1]));
    }
    let total = arc[n_sp - 1];
    let pi2 = std::f64::consts::FRAC_PI_2;
    let south_arc = pi2 * r_back;
    let cone_arc = libm::sqrt(total * total + (r_tip - r_back) * (r_tip - r_back));
    let north_arc = pi2 * r_tip;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = |a: f64, m: usize| (libm::ceil(a / RING_RES_MM) as usize).max(m);
    let (n_south, n_cone, n_north) = (count(south_arc, 3), count(cone_arc, 4), count(north_arc, 4));
    let factor_tip = (1.0 - (r_base + MAX_NOISE_MM) / r_tip.max(1e-6)).max(0.0);
    let dome_shift = (r_tip - r_base - MAX_NOISE_MM).max(0.0);
    let (pu_s, pu_tip) = (up_perp(start_t), up_perp(tip_t));

    let mut rings: Vec<([f64; 3], f64, [f64; 3])> = Vec::new();
    #[allow(clippy::cast_precision_loss)]
    for j in 1..=n_south {
        let phi = pi2 * j as f64 / n_south as f64;
        let rr = r_back * libm::sin(phi);
        let ax = -r_back * libm::cos(phi);
        rings.push((
            add(add(start_p, scale(start_t, ax)), scale(pu_s, 0.5 * rr)),
            rr,
            start_t,
        ));
    }
    #[allow(clippy::cast_precision_loss)]
    for i in 1..=n_cone {
        let tc = i as f64 / n_cone as f64;
        let rr = r_back + tc * (r_tip - r_back);
        let sv = tc * total;
        let k = arc.partition_point(|&a| a <= sv).clamp(1, n_sp - 1) - 1;
        let f = ((sv - arc[k]) / (arc[k + 1] - arc[k]).max(1e-12)).clamp(0.0, 1.0);
        let p = [0, 1, 2].map(|a| sp[k][a] + f * (sp[k + 1][a] - sp[k][a]));
        let t = unit([0, 1, 2].map(|a| st[k][a] + f * (st[k + 1][a] - st[k][a]))).unwrap_or(tip_t);
        let sm = 3.0 * tc * tc - 2.0 * tc * tc * tc;
        let fac = 0.5 + (factor_tip - 0.5) * sm;
        rings.push((add(p, scale(up_perp(t), rr * fac)), rr, t));
    }
    #[allow(clippy::cast_precision_loss)]
    for j in 1..n_north {
        let phi = pi2 * j as f64 / n_north as f64;
        let rr = r_tip * libm::cos(phi);
        let ax = r_tip * libm::sin(phi);
        rings.push((
            add(
                add(tip_p, scale(tip_t, ax)),
                scale(pu_tip, dome_shift * libm::cos(phi)),
            ),
            rr,
            tip_t,
        ));
    }
    let south = sub(start_p, scale(start_t, r_back));
    let north = add(tip_p, scale(tip_t, r_tip));
    let mut reference = if norm(pu_s) > 1e-6 {
        pu_s
    } else {
        [1.0, 0.0, 0.0]
    };
    let mut v: Vec<[f64; 3]> = vec![south];
    for (c, rr, ax) in &rings {
        let mut e1 = sub(reference, scale(*ax, dot(reference, *ax)));
        e1 = match unit(e1) {
            Some(e) => e,
            None => unit(if ax[2].abs() < 0.9 {
                cross(*ax, [0.0, 0.0, 1.0])
            } else {
                cross(*ax, [1.0, 0.0, 0.0])
            })
            .unwrap_or([1.0, 0.0, 0.0]),
        };
        let e2 = cross(*ax, e1);
        reference = e1;
        #[allow(clippy::cast_precision_loss)]
        for k in 0..N_THETA {
            let th = std::f64::consts::TAU * k as f64 / N_THETA as f64;
            let (ct, s) = (libm::cos(th), libm::sin(th));
            v.push([0, 1, 2].map(|a| c[a] + rr * (ct * e1[a] + s * e2[a])));
        }
    }
    v.push(north);
    let nr = rings.len();
    let u = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    let mut tris = Vec::new();
    for k in 0..N_THETA {
        tris.push([0, u(1 + k), u(1 + (k + 1) % N_THETA)]);
    }
    for r in 0..nr - 1 {
        let (a, b) = (1 + r * N_THETA, 1 + (r + 1) * N_THETA);
        for k in 0..N_THETA {
            let k1 = (k + 1) % N_THETA;
            tris.push([u(a + k), u(b + k), u(b + k1)]);
            tris.push([u(a + k), u(b + k1), u(a + k1)]);
        }
    }
    let pole = 1 + nr * N_THETA;
    let last = 1 + (nr - 1) * N_THETA;
    for k in 0..N_THETA {
        tris.push([u(pole), u(last + (k + 1) % N_THETA), u(last + k)]);
    }
    let mut m = TriMesh {
        positions: v,
        triangles: tris,
        ..TriMesh::default()
    };
    if dt_kit::signed_volume(&m) < 0.0 {
        m.triangles.iter_mut().for_each(|t| t.swap(1, 2));
    }
    // inward-only noise, faded out toward the narrow back
    let normals = form::vertex_normals(&m.positions, &m.triangles);
    let chord = unit(sub(tip_p, start_p)).unwrap_or(tip_t);
    let span = (r_tip - r_back).max(1e-6);
    let disp: Vec<f64> = m
        .positions
        .iter()
        .map(|p| {
            let rel = sub(*p, start_p);
            let tp = dot(rel, chord).clamp(0.0, total);
            let perp = dist(*p, add(start_p, scale(chord, tp)));
            let x = ((perp - r_back) / span).clamp(0.0, 1.0);
            (fine(*p, s.key) + coarse(*p, s.key)) * (3.0 * x * x - 2.0 * x * x * x)
        })
        .collect();
    let peak = disp.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // no dents in an underside: a dent there is a ceiling over air (the
    // layer-reach audit flags it), so the noise fades out on faces that
    // look down past 30° below the horizon
    for (k, p) in m.positions.iter_mut().enumerate() {
        let x = ((normals[k][2] - NOISE_NONE_NZ) / (NOISE_FULL_NZ - NOISE_NONE_NZ)).clamp(0.0, 1.0);
        let w = x * x * (3.0 - 2.0 * x);
        *p = add(*p, scale(normals[k], (disp[k] - peak) * w));
    }
    m
}

/// Per-vertex grain: a normal draw per 0.05 mm cell of position.
fn fine(p: [f64; 3], key: u64) -> f64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let c = p.map(|x| libm::floor(x / 0.05) as i64 as u64);
    let u1 = hash01(&[key, 11, c[0], c[1], c[2]]).max(1e-10);
    let u2 = hash01(&[key, 12, c[0], c[1], c[2]]);
    libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(std::f64::consts::TAU * u2) * FINE_MM
}

/// Single-octave smooth value noise at 4 mm, ±1 mm.
fn coarse(p: [f64; 3], key: u64) -> f64 {
    let q = p.map(|x| x / COARSE_CELL_MM);
    let fl = q.map(libm::floor);
    let f = [0, 1, 2].map(|a| q[a] - fl[a]);
    let s = f.map(|t| t * t * (3.0 - 2.0 * t));
    #[allow(clippy::cast_possible_truncation)]
    let i = fl.map(|x| x as i64);
    let mut val = 0.0;
    for dz in 0..2i64 {
        let wz = if dz == 1 { s[2] } else { 1.0 - s[2] };
        for dy in 0..2i64 {
            let wy = if dy == 1 { s[1] } else { 1.0 - s[1] };
            for dx in 0..2i64 {
                let wx = if dx == 1 { s[0] } else { 1.0 - s[0] };
                #[allow(clippy::cast_sign_loss)]
                let h = hash01(&[
                    key,
                    13,
                    (i[0] + dx) as u64,
                    (i[1] + dy) as u64,
                    (i[2] + dz) as u64,
                ]);
                val += (h * 2.0 - 1.0) * wx * wy * wz;
            }
        }
    }
    COARSE_MM * val
}
