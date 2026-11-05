//! Chips: crisp planar scars on the arrises (round 17: a few chips of
//! 3-7 mm on edges over 60 degrees, dense small ones only on underside
//! lips) and corner chips.

use std::collections::BTreeMap;

use dt_kit::{Piece, Rng, TriMesh, form, welded};

use super::util::Draw;

const CHIP_DEG: (f64, f64) = (60.0, 40.0);
pub const CHIP_MM: (f64, f64) = (0.5, 1.1);
const CHIP_LEN_MM: (f64, f64) = (3.0, 7.0);
const CHIP_GAP_MM: (f64, f64) = (1.5, 6.0);
const CHIP_LIP_NZ: f64 = -0.1;
const CHIP_LIP_MM: (f64, f64) = (0.25, 0.9);

type V = [f64; 3];

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V) -> f64 {
    libm::sqrt(dot(a, a))
}
fn unit(a: V) -> V {
    let l = norm(a);
    if l > 0.0 { a.map(|x| x / l) } else { a }
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V, k: f64) -> V {
    a.map(|x| x * k)
}

/// n tilted by U(0, max_deg) about a random tangent axis.
pub fn tilted(rng: &mut Rng, n: V, max_deg: f64) -> V {
    let mut t = [
        rng.normal(0.0, 1.0),
        rng.normal(0.0, 1.0),
        rng.normal(0.0, 1.0),
    ];
    let d = dot(t, n);
    t = sub(t, mul(n, d));
    t = unit(t);
    let tl = rng.uniform(0.0, max_deg).to_radians();
    add(mul(n, libm::cos(tl)), mul(t, libm::sin(tl)))
}

pub fn face_normals(p: &[V], t: &[[u32; 3]]) -> Vec<V> {
    t.iter()
        .map(|tr| {
            let [a, b, c] = tr.map(|i| p[i as usize]);
            unit(cross(sub(b, a), sub(c, a)))
        })
        .collect()
}

/// (face a, face b, edge vertices) for each interior edge, in vertex order.
pub fn adjacency(t: &[[u32; 3]]) -> Vec<(usize, usize, u32, u32)> {
    let mut m: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (f, tr) in t.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (tr[k], tr[(k + 1) % 3]);
            m.entry((a.min(b), a.max(b))).or_default().push(f);
        }
    }
    m.into_iter()
        .filter(|(_, v)| v.len() == 2)
        .map(|((a, b), v)| (v[0], v[1], a, b))
        .collect()
}

struct Arris {
    n1: V,
    n2: V,
    pts: Vec<V>,
}

/// The chips (hulls) that break the arrises as a mason's are broken.
#[allow(clippy::too_many_lines)]
pub fn chip_edges(
    rng: &mut Rng,
    rock: &Piece,
    dep_mm: (f64, f64),
    keep: &dyn Fn(V, V, V) -> bool,
) -> Vec<TriMesh> {
    let (p, t) = welded(&rock.mesh());
    if t.is_empty() {
        return Vec::new();
    }
    let fn_ = face_normals(&p, &t);
    let mut keys: Vec<[i64; 6]> = Vec::new();
    let mut arris: Vec<Arris> = Vec::new();
    let amin = CHIP_DEG.1.to_radians();
    for (fa, fb, a, b) in adjacency(&t) {
        let (n1, n2) = (fn_[fa], fn_[fb]);
        let ang = libm::acos(dot(n1, n2).clamp(-1.0, 1.0));
        if ang <= amin {
            continue;
        }
        if n1[2].min(n2[2]) < -0.5 || dot(n1, n2) < -0.98 {
            continue;
        }
        let (pa, pb) = (p[a as usize], p[b as usize]);
        if keep(n1, n2, pa) {
            continue;
        }
        let r2 = |x: f64| (x * 100.0).round() as i64;
        let r1 = |x: f64| (x * 10.0).round() as i64;
        let key = [
            r2(n1[0]) * 1_000_003 + r2(n1[1]) * 1009 + r2(n1[2]),
            r2(n2[0]) * 1_000_003 + r2(n2[1]) * 1009 + r2(n2[2]),
            r1(dot(n1, pa)),
            r1(dot(n2, pa)),
            0,
            0,
        ];
        if let Some(k) = keys.iter().position(|q| *q == key) {
            arris[k].pts.extend([pa, pb]);
        } else {
            keys.push(key);
            arris.push(Arris {
                n1,
                n2,
                pts: vec![pa, pb],
            });
        }
    }
    let ptp = |a: &Arris| {
        (0..3)
            .map(|k| {
                let lo = a.pts.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min);
                let hi = a.pts.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max);
                hi - lo
            })
            .fold(0.0, f64::max)
    };
    arris.sort_by(|a, b| ptp(b).total_cmp(&ptp(a)));
    let mut chips: Vec<TriMesh> = Vec::new();
    let mut done: Vec<(V, f64)> = Vec::new();
    for a in &arris {
        let (n1, n2) = (a.n1, a.n2);
        let e = cross(n1, n2);
        if norm(e) < 1e-6 {
            continue;
        }
        let e = unit(e);
        let sv: Vec<f64> = a.pts.iter().map(|&p| dot(p, e)).collect();
        let o = sub(a.pts[0], mul(e, sv[0]));
        let svmin = sv.iter().copied().fold(f64::INFINITY, f64::min);
        let svmax = sv.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut s = svmin + rng.uniform(0.0, CHIP_GAP_MM.1);
        let s1 = svmax;
        let nb = unit(add(n1, n2));
        let u_of = |n: V| {
            let u = cross(n, e);
            let sg = dot(u, sub(nb, n)).signum();
            mul(u, -sg)
        };
        let uu = [u_of(n1), u_of(n2)];
        let lip = n1[2].min(n2[2]) < CHIP_LIP_NZ;
        if lip {
            s = svmin - rng.uniform(0.0, 1.0);
        } else if dot(n1, n2) > libm::cos(CHIP_DEG.0.to_radians()) {
            continue;
        }
        while s < s1 {
            let (mut ln, d) = if lip {
                (
                    0.5 * rng.uniform(CHIP_LEN_MM.0, CHIP_LEN_MM.1),
                    rng.uniform(CHIP_LIP_MM.0, CHIP_LIP_MM.1),
                )
            } else {
                (
                    rng.uniform(CHIP_LEN_MM.0, CHIP_LEN_MM.1),
                    rng.uniform(dep_mm.0, dep_mm.1),
                )
            };
            ln = (ln.max(2.0 * d + 0.5)).min(s1 - s + d);
            let nc = unit(tilted(rng, nb, 12.0));
            let w = cross(e, nc);
            let tw = uu
                .iter()
                .map(|&u| d * dot(w, u).abs() / (-dot(nc, u)).max(0.05))
                .fold(0.0, f64::max)
                + 0.3;
            let c = add(o, mul(e, s + 0.5 * ln));
            let clear = done
                .iter()
                .all(|&(c2, h2)| norm(sub(c, c2)) > 0.5 * ln + h2 + 0.5);
            if ln > 2.0 * d + 0.3
                && nc[2] >= -0.3
                && nc[2] - e[2].abs() >= -1.1
                && tw < 4.0
                && (lip || clear)
            {
                let mut pts = Vec::new();
                for (r_, lo, hi) in [(-d, d, ln - d), (0.0, 0.0, ln), (1.0, -1.0, ln + 1.0)] {
                    for sa in [lo, hi] {
                        for ttw in [-tw, tw] {
                            pts.push(add(add(add(o, mul(e, s + sa)), mul(nc, r_)), mul(w, ttw)));
                        }
                    }
                }
                if let Ok(h) = form::hull(&pts) {
                    chips.push(h);
                    done.push((c, 0.5 * ln));
                }
            }
            s += if lip {
                ln * rng.uniform(0.4, 0.8)
            } else {
                ln + rng.uniform(CHIP_GAP_MM.0, CHIP_GAP_MM.1)
            };
        }
    }
    chips
}

const CORNER_CHIP_FRAC: f64 = 0.45;
const CORNER_CHIP_MM: (f64, f64) = (0.3, 0.9);
const CHIP_MIN_Z_MM: f64 = 0.6;
const CHIP_TILT_DEG: f64 = 25.0;
const FACET_MIN_NZ: f64 = -0.5;

/// Corner chips (edge chips off): small planar cuts at the
/// polytope corners. `rock` has its base at z = 0.
pub fn corner_chips(rng: &mut Rng, rock: Piece, d: f64, max_loss: f64) -> Piece {
    let (p, t) = welded(&rock.mesh());
    if t.is_empty() {
        return rock;
    }
    let fn_ = face_normals(&p, &t);
    let mut vn = vec![[0.0; 3]; p.len()];
    for (f, tr) in t.iter().enumerate() {
        for &v in tr {
            vn[v as usize] = add(vn[v as usize], fn_[f]);
        }
    }
    let scale = libm::sqrt(d / 15.0);
    let mut planes: Vec<(V, V, f64)> = Vec::new();
    for vi in rng.perm(p.len()) {
        if rng.random() > CORNER_CHIP_FRAC || p[vi][2] < CHIP_MIN_Z_MM {
            continue;
        }
        let n = unit(vn[vi]);
        planes.push((
            p[vi],
            n,
            rng.uniform(CORNER_CHIP_MM.0, CORNER_CHIP_MM.1) * scale,
        ));
    }
    let cap = rng.int(8, 16);
    let order = rng.perm(planes.len());
    let mut rock = rock;
    let mut n_chips = 0;
    for i in order {
        if n_chips >= cap {
            break;
        }
        let (p_, n, dep) = planes[i];
        let n2 = unit(tilted(rng, n, CHIP_TILT_DEG));
        if n2[2] < FACET_MIN_NZ {
            continue;
        }
        let Ok(cut) = rock.keep(n2, dot(n2, p_) - dep) else {
            continue;
        };
        if cut.is_empty() || cut.volume() < (1.0 - max_loss) * rock.volume() {
            continue;
        }
        rock = cut;
        n_chips += 1;
    }
    rock
}
