//! Block faces: one crack network per open column face, planned once for
//! the whole pile so cracks run on across stones.
//! Cracks are thin, straight, block-splitting; they run from an edge, a
//! joint or another crack to one of those, never fade or dead-end.

use std::collections::HashSet;
use std::sync::Arc;

use dt_kit::{Rng, blur2};

use super::lines::Line;
use super::stack::Placed;
use super::util::{Draw, Field, G, P2, edt_out, norm2};

pub const RELIEF_GRID_MM: f64 = 0.1;
const CRACK_PER_MM2: f64 = 0.06;
const CRACK_STONE_MM: f64 = 10.0;
const CRACK_STEP_MM: (f64, f64) = (4.0, 8.0);
const CRACK_BEND_DEG: f64 = 6.0;
const CRACK_WANDER_DEG: f64 = 12.0;
const CRACK_LEVEL_P: f64 = 0.35;
const CRACK_DIAG_P: f64 = 0.05;
const CRACK_DIAG_DEG: (f64, f64) = (30.0, 50.0);
const CRACK_LEAN_DEG: f64 = 8.0;
const CRACK_GAP_MM: f64 = 2.0;
const CRACK_ALONG: (f64, f64) = (25.0, 3.0);
const CRACK_MIN_MM: f64 = 3.0;
const CRACK_MEET_DEG: f64 = 35.0;
const CRACK_CORNER_MM: f64 = 2.0;
const CRACK_WRAP_DEG: (f64, f64) = (5.0, 15.0);
const CRACK_SLOT_MM: (f64, f64) = (0.4, 0.5);
const CRACK_DEPTH_MM: (f64, f64) = (0.5, 0.65);
const CRACK_PINCH: (f64, f64) = (0.7, 1.2);
const CRACK_SHIFT_MM: (f64, f64) = (0.3, 0.7);
const CRACK_LEVEL_DEG: f64 = 30.0;
pub const CRACK_FINE_MM: f64 = 5.0;
const CRACK_FINE: f64 = 0.8;
const FACET_TILT_DEG: (f64, f64) = (3.0, 8.0);
const FACET_MM: (f64, f64, f64) = (0.0, 0.3, 0.6);
const JOINT_PROUD_MM: f64 = 0.3;
const FACET_DEEP_MM: (f64, f64) = (1.4, 0.7);
const FACET_SEAM_MM: f64 = 0.1;

/// One open side of a column.
#[derive(Clone, Debug)]
pub struct OpenFace {
    pub p0: P2,
    pub p1: P2,
    pub m: [f64; 3],
    pub rim: bool,
}

/// A plane taken from a column: `n . x >= off` removed; `d` its face direction.
#[derive(Clone, Copy, Debug)]
pub struct Cut {
    pub n: [f64; 3],
    pub off: f64,
    pub d: [f64; 3],
}

#[derive(Clone)]
pub struct Column {
    pub stones: Vec<usize>,
    pub poly: Vec<P2>,
    pub zlo: f64,
    pub zhi: f64,
    pub zone: f64,
    pub open: Vec<OpenFace>,
    pub cuts: Vec<Cut>,
    pub setback: Vec<((usize, bool), f64)>,
    pub corner_mm: f64,
    pub step: f64,
    pub back: Vec<(usize, f64)>,
    pub carries: bool,
    pub faces: Vec<Arc<Face>>,
    pub reach: Vec<[f64; 3]>,
}

/// One open face of a column in its own plane: u along it from `p0`
/// (unit `t`), z up, outward `m`. `g` is the crack slots' depth (<= 0) and
/// `f` the sub-faces' setback (<= 0) over a RELIEF_GRID raster from (0, z0)
/// (nx = along u, ny = up z).
pub struct Face {
    pub p0: P2,
    pub t: P2,
    pub m: P2,
    pub l: f64,
    pub z0: f64,
    pub g: Field,
    pub f: Field,
    pub cracks: Vec<Line>,
    pub rim: bool,
}

fn heading(th: f64) -> P2 {
    [libm::cos(th), libm::sin(th)]
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Edge,
    Joint,
    Crack,
}

struct Net {
    lines: Vec<(Kind, Line)>,
}

impl Net {
    fn dist_pt(&self, p: P2) -> f64 {
        self.lines
            .iter()
            .map(|l| l.1.dist(p))
            .fold(f64::INFINITY, f64::min)
    }
}

fn along(a: P2, b: P2, g: &Line) -> bool {
    let l = norm2([b[0] - a[0], b[1] - a[1]]);
    if l < 0.2 || g.dist_seg(a, b) > CRACK_ALONG.1 {
        return false;
    }
    let mid = [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])];
    let ch = g.chord(g.project(mid));
    let dot = ((b[0] - a[0]) * ch[0] + (b[1] - a[1]) * ch[1]).abs();
    dot > libm::cos(CRACK_ALONG.0.to_radians()) * l * norm2(ch)
}

type Walked = (Vec<P2>, Option<usize>);

#[allow(clippy::too_many_arguments)]
fn walk(rng: &mut Rng, net: &Net, p: P2, th0: f64, up: bool) -> Walked {
    let mut pts = vec![p];
    let mut th = th0;
    for _ in 0..60 {
        th = th0
            + (th - th0 + rng.normal(0.0, CRACK_BEND_DEG).to_radians()).clamp(
                -CRACK_WANDER_DEG.to_radians(),
                CRACK_WANDER_DEG.to_radians(),
            );
        let last = pts[pts.len() - 1];
        let step = rng.uniform(CRACK_STEP_MM.0, CRACK_STEP_MM.1) * if up { 1.0 } else { -1.0 };
        let h = heading(th);
        let q = [last[0] + step * h[0], last[1] + step * h[1]];
        // nearest crossing along the step
        let mut best: Option<(f64, P2, usize)> = None;
        for (k, (_, l)) in net.lines.iter().enumerate() {
            for c in l.cross(last, q) {
                let dd = norm2([c[0] - last[0], c[1] - last[1]]);
                if best.is_none_or(|b| dd < b.0) {
                    best = Some((dd, c, k));
                }
            }
        }
        if let Some((_, c, k)) = best {
            pts.push(c);
            return (pts, Some(k));
        }
        // coming closer than the gap, or along an edge or joint: on into it
        let mut near: Vec<usize> = Vec::new();
        for (k, (kind, l)) in net.lines.iter().enumerate() {
            if l.dist(last) > CRACK_GAP_MM
                && (l.dist_seg(last, q) < CRACK_GAP_MM
                    || (*kind != Kind::Crack && along(last, q, l)))
            {
                near.push(k);
            }
        }
        if near.is_empty() {
            pts.push(q);
            continue;
        }
        let k = near
            .into_iter()
            .min_by(|&a, &b| {
                net.lines[a]
                    .1
                    .dist_seg(last, q)
                    .total_cmp(&net.lines[b].1.dist_seg(last, q))
            })
            .unwrap_or(0);
        let (_, a, b) = net.lines[k].1.closest(last, q);
        pts.push(a);
        pts.push(b);
        return (pts, Some(k));
    }
    (pts, None)
}

/// A crack network over [0, L] x [z0, z1], `want` mm more of it.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn crack_net(
    rng: &mut Rng,
    l: f64,
    z0: f64,
    z1: f64,
    joints: &[Vec<P2>],
    want: f64,
    cracks: &[Line],
    starts: &[(P2, f64)],
) -> Vec<Line> {
    let mut net = Net { lines: Vec::new() };
    net.lines.push((
        Kind::Edge,
        Line::new(vec![[0.0, z0], [l, z0], [l, z1], [0.0, z1], [0.0, z0]]),
    ));
    for j in joints {
        net.lines.push((Kind::Joint, Line::new(j.clone())));
    }
    for c in cracks {
        net.lines.push((Kind::Crack, c.clone()));
    }
    let mut total = 0.0;
    // the bare spots
    let mut b: Vec<P2> = Vec::new();
    let mut x = 0.25;
    while x < l {
        let mut z = z0 + 0.25;
        while z < z1 {
            b.push([x, z]);
            z += 0.5;
        }
        x += 0.5;
    }
    for &(p, th) in starts {
        let h = heading(th);
        let (pts, hit) = walk(
            rng,
            &net,
            [p[0] + 0.05 * h[0], p[1] + 0.05 * h[1]],
            th,
            true,
        );
        if hit.is_some() {
            let mut all = vec![p];
            all.extend(pts);
            net.lines.push((Kind::Crack, Line::new(all)));
        }
    }
    let mut bare: Vec<f64> = b.iter().map(|&p| net.dist_pt(p)).collect();
    let tries = if want > 0.0 {
        3 * (want as usize) + 60
    } else {
        0
    };
    for _ in 0..tries {
        let w: Vec<f64> = bare
            .iter()
            .map(|&d| if d > CRACK_GAP_MM { d.powi(4) } else { 0.0 })
            .collect();
        if total >= want || w.iter().all(|&x| x <= 0.0) {
            break;
        }
        let p = b[rng.pick(&w)];
        let r = rng.random();
        let th = (rng.normal(0.0, CRACK_LEAN_DEG).clamp(-15.0, 15.0)
            + if r < CRACK_LEVEL_P { 0.0 } else { 90.0 }
            + if r > 1.0 - CRACK_DIAG_P {
                rng.signed(CRACK_DIAG_DEG)
            } else {
                0.0
            })
        .to_radians();
        let (dn, hit0) = walk(rng, &net, p, th, false);
        let (up, hit) = walk(rng, &net, p, th, true);
        let (Some(h0), Some(h1)) = (hit0, hit) else {
            continue;
        };
        let mut pts: Vec<P2> = dn.into_iter().rev().collect();
        pts.extend(up.into_iter().skip(1));
        if pts.len() < 2 {
            continue;
        }
        let line = Line::new(pts.clone());
        if line.length() < CRACK_MIN_MM {
            continue;
        }
        let n = pts.len();
        let mut lens = false;
        for (h, a, bb) in [(h0, pts[0], pts[1]), (h1, pts[n - 1], pts[n - 2])] {
            if net.lines[h].0 != Kind::Crack {
                continue;
            }
            let ch = net.lines[h].1.chord(net.lines[h].1.project(a));
            let v = [a[0] - bb[0], a[1] - bb[1]];
            let dot = (v[0] * ch[0] + v[1] * ch[1]).abs();
            if dot > libm::cos(CRACK_MEET_DEG.to_radians()) * norm2(v) * norm2(ch) {
                lens = true;
            }
        }
        if lens {
            continue;
        }
        total += line.length();
        for (k, &q) in b.iter().enumerate() {
            bare[k] = bare[k].min(line.dist(q));
        }
        net.lines.push((Kind::Crack, line));
    }
    net.lines
        .into_iter()
        .filter(|(k, _)| *k == Kind::Crack)
        .map(|(_, l)| l)
        .collect()
}

pub struct Spec {
    pub p0: P2,
    pub t: P2,
    pub l: f64,
    pub z0: f64,
    pub z1: f64,
    pub joints: Vec<Vec<P2>>,
    pub want: f64,
    pub rim: bool,
    pub on: Vec<usize>,
}

fn corner_starts(
    rng: &mut Rng,
    specs: &[Spec],
    nets: &[Vec<Line>],
) -> Vec<(usize, Vec<(P2, f64)>)> {
    let mut out: Vec<(usize, Vec<(P2, f64)>)> = Vec::new();
    let e: Vec<(usize, usize, P2)> = specs
        .iter()
        .enumerate()
        .flat_map(|(i, sp)| {
            (0..2).map(move |e| {
                (
                    i,
                    e,
                    [
                        sp.p0[0] + e as f64 * sp.l * sp.t[0],
                        sp.p0[1] + e as f64 * sp.l * sp.t[1],
                    ],
                )
            })
        })
        .collect();
    for &(i, ei, p) in &e {
        for &(j, ej, q) in &e {
            let sj = &specs[j];
            let ti = specs[i].t;
            let tj = [
                (1.0 - 2.0 * ej as f64) * sj.t[0],
                (1.0 - 2.0 * ej as f64) * sj.t[1],
            ];
            if j == i
                || norm2([p[0] - q[0], p[1] - q[1]]) > CRACK_CORNER_MM
                || (ti[1] * tj[0] - ti[0] * tj[1]) > -0.05
            {
                continue;
            }
            let mut have: Vec<f64> = Vec::new();
            for c in &nets[j] {
                for q in [c.first(), c.last()] {
                    if (q[0] - ej as f64 * sj.l).abs() < 0.02 {
                        have.push(q[1]);
                    }
                }
            }
            if let Some((_, v)) = out.iter().find(|(k, _)| *k == j) {
                have.extend(v.iter().map(|s| s.0[1]));
            }
            for c_ in &nets[i] {
                let n = c_.pts.len();
                for (a, b) in [(c_.pts[n - 1], c_.pts[n - 2]), (c_.pts[0], c_.pts[1])] {
                    if (a[0] - ei as f64 * specs[i].l).abs() < 0.02
                        && sj.z0 + 0.3 < a[1]
                        && a[1] < sj.z1 - 0.3
                        && !have.iter().any(|h| (a[1] - h).abs() < 0.6)
                    {
                        let dv = [a[0] - b[0], a[1] - b[1]];
                        let th = libm::atan2(dv[1], (1.0 - 2.0 * ej as f64) * dv[0].abs())
                            + rng.signed(CRACK_WRAP_DEG).to_radians();
                        let item = ([ej as f64 * sj.l, a[1]], th);
                        if let Some(slot) = out.iter_mut().find(|(k, _)| *k == j) {
                            slot.1.push(item);
                        } else {
                            out.push((j, vec![item]));
                        }
                        have.push(a[1]);
                    }
                }
            }
        }
    }
    out
}

/// Plan every open face of the pile. `columns[c].faces` are filled in.
pub fn plan_faces(rng: &mut Rng, cols: &mut [Column], placed: &[Placed]) {
    // (column, p0, p1, rim)
    let f: Vec<(usize, P2, P2, bool)> = cols
        .iter()
        .enumerate()
        .flat_map(|(ci, c)| c.open.iter().map(move |o| (ci, o.p0, o.p1, o.rim)))
        .collect();
    let t_: Vec<P2> = f
        .iter()
        .map(|&(_, a, b, _)| {
            let l = norm2([b[0] - a[0], b[1] - a[1]]);
            [(b[0] - a[0]) / l, (b[1] - a[1]) / l]
        })
        .collect();
    let m_: Vec<P2> = t_.iter().map(|t| [t[1], -t[0]]).collect();
    let dot = |a: P2, b: P2| a[0] * b[0] + a[1] * b[1];
    let n = f.len();
    let mut par: Vec<usize> = (0..n).collect();
    fn find(par: &mut [usize], mut i: usize) -> usize {
        while par[i] != i {
            par[i] = par[par[i]];
            i = par[i];
        }
        i
    }
    for i in 0..n {
        for j in i + 1..n {
            let dp = [f[j].1[0] - f[i].1[0], f[j].1[1] - f[i].1[1]];
            let lo = dot(f[i].2, t_[i]).min(dot(f[j].2, t_[i]));
            let hi = dot(f[i].1, t_[i]).max(dot(f[j].1, t_[i]));
            if dot(m_[i], m_[j]) > libm::cos(25f64.to_radians())
                && dot(dp, m_[i]).abs() < 3.0
                && hi - lo < 1.0
            {
                let (a, b) = (find(&mut par, i), find(&mut par, j));
                par[a] = b;
            }
        }
    }
    let labels: Vec<usize> = (0..n).map(|i| find(&mut par, i)).collect();
    let mut groups: Vec<usize> = Vec::new();
    for &l in &labels {
        if !groups.contains(&l) {
            groups.push(l);
        }
    }
    groups.sort_unstable();
    let mut specs: Vec<Spec> = Vec::new();
    for g in groups {
        let ix: Vec<usize> = (0..n).filter(|&i| labels[i] == g).collect();
        let mut m = [0.0, 0.0];
        for &i in &ix {
            let len = norm2([f[i].2[0] - f[i].1[0], f[i].2[1] - f[i].1[1]]);
            m[0] += len * m_[i][0];
            m[1] += len * m_[i][1];
        }
        let ml = norm2(m);
        let m = [m[0] / ml, m[1] / ml];
        let t = [-m[1], m[0]];
        let ends: Vec<P2> = ix.iter().flat_map(|&i| [f[i].1, f[i].2]).collect();
        let u0 = ends
            .iter()
            .map(|&p| dot(p, t))
            .fold(f64::INFINITY, f64::min);
        let u1 = ends
            .iter()
            .map(|&p| dot(p, t))
            .fold(f64::NEG_INFINITY, f64::max);
        let mx = ends
            .iter()
            .map(|&p| dot(p, m))
            .fold(f64::NEG_INFINITY, f64::max);
        let p0 = [t[0] * u0 + m[0] * mx, t[1] * u0 + m[1] * mx];
        let on: Vec<usize> = {
            let mut v = Vec::new();
            for &i in &ix {
                if !v.contains(&f[i].0) {
                    v.push(f[i].0);
                }
            }
            v
        };
        let z0 = on
            .iter()
            .map(|&c| cols[c].zlo)
            .fold(f64::INFINITY, f64::min);
        let z1 = on
            .iter()
            .map(|&c| cols[c].zhi)
            .fold(f64::NEG_INFINITY, f64::max);
        let mut joints: Vec<Vec<P2>> = Vec::new();
        let mut area = 0.0;
        for &c in &on {
            let col = &cols[c];
            let us: Vec<f64> = ix
                .iter()
                .filter(|&&i| f[i].0 == c)
                .flat_map(|&i| [dot(f[i].1, t) - u0, dot(f[i].2, t) - u0])
                .collect();
            let ua = us.iter().copied().fold(f64::INFINITY, f64::min);
            let ub = us.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            area += (ub - ua) * (col.zhi - col.zlo) * (0.6 + 0.4 * col.zone);
            let grid: Vec<f64> = (0..12).map(|k| ua + (ub - ua) * k as f64 / 11.0).collect();
            for &r in &col.stones {
                let tz = super::finish::top_z_poly(&placed[r], &placed[r].poly());
                if tz < col.zhi - 0.5 {
                    let j: Vec<P2> = grid
                        .iter()
                        .map(|&u| {
                            let x = p0[0] + t[0] * u;
                            let y = p0[1] + t[1] * u;
                            [u, placed[r].top_z(x, y)]
                        })
                        .collect();
                    let jmin = j.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
                    let jmax = j.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
                    if z0 + 1.0 < jmin && jmax < z1 - 1.0 {
                        joints.push(j);
                    }
                }
            }
            for u in [ua, ub] {
                if 0.5 < u && u < (u1 - u0) - 0.5 {
                    joints.push(vec![[u, col.zlo], [u, col.zhi]]);
                }
            }
        }
        let nst: usize = on.iter().map(|&c| cols[c].stones.len()).sum();
        let want = (CRACK_PER_MM2 * area).min(CRACK_STONE_MM * nst as f64);
        specs.push(Spec {
            p0,
            t,
            l: u1 - u0,
            z0,
            z1,
            joints,
            want,
            rim: ix.iter().any(|&i| f[i].3),
            on,
        });
    }
    let mut nets: Vec<Vec<Line>> = specs
        .iter()
        .map(|sp| crack_net(rng, sp.l, sp.z0, sp.z1, &sp.joints, sp.want, &[], &[]))
        .collect();
    for _ in 0..4 {
        let starts = corner_starts(rng, &specs, &nets);
        if starts.is_empty() {
            break;
        }
        for (i, st) in starts {
            let sp = &specs[i];
            nets[i] = crack_net(rng, sp.l, sp.z0, sp.z1, &sp.joints, 0.0, &nets[i], &st);
        }
    }
    for (sp, net) in specs.iter().zip(nets) {
        let fc = Arc::new(plan_face(rng, sp, net));
        for &c in &sp.on {
            cols[c].faces.push(Arc::clone(&fc));
        }
    }
}

fn level(c: &Line) -> bool {
    let (a, b) = (c.first(), c.last());
    (b[1] - a[1]).abs() < CRACK_LEVEL_DEG.to_radians().tan() * (b[0] - a[0]).abs()
}

#[allow(clippy::too_many_lines)]
fn plan_face(rng: &mut Rng, sp: &Spec, cracks: Vec<Line>) -> Face {
    let g = RELIEF_GRID_MM;
    let nu = (sp.l / g) as usize + 2;
    let nz = ((sp.z1 - sp.z0) / g) as usize + 2;
    let at = |i: usize, j: usize| [i as f64 * g, sp.z0 + j as f64 * g];
    let mut gg = Field::new(nu, nz, 0.0);
    let mut cut = G::new(nu, nz, false);
    for cs in &cracks {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in &cs.pts {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        let m = CRACK_SLOT_MM.1 + 2.0 * g;
        let i0 = (((lo[0] - m) / g).floor().max(0.0)) as usize;
        let i1 = (((hi[0] + m) / g).ceil().max(0.0) as usize).min(nu - 1);
        let j0 = (((lo[1] - m - sp.z0) / g).floor().max(0.0)) as usize;
        let j1 = (((hi[1] + m - sp.z0) / g).ceil().max(0.0) as usize).min(nz - 1);
        let freq: Vec<(f64, f64)> = (0..3)
            .map(|_| {
                (
                    rng.uniform(1.0, 3.0),
                    rng.uniform(0.0, std::f64::consts::TAU),
                )
            })
            .collect();
        let wscale = rng.uniform(CRACK_SLOT_MM.0, CRACK_SLOT_MM.1);
        let dep = rng.uniform(CRACK_DEPTH_MM.0, CRACK_DEPTH_MM.1)
            * if cs.length() < CRACK_FINE_MM {
                CRACK_FINE
            } else {
                1.0
            };
        for j in j0..=j1 {
            for i in i0..=i1 {
                let p = at(i, j);
                let dc = cs.dist(p);
                if dc < g {
                    cut.set(i, j, true);
                }
                if dc < CRACK_SLOT_MM.1 {
                    let s = cs.project(p);
                    let f = (0.75
                        + 0.2
                            * freq
                                .iter()
                                .map(|&(fr, ph)| {
                                    libm::sin(std::f64::consts::TAU * fr * s / 6.0 + ph)
                                })
                                .sum::<f64>())
                    .clamp(CRACK_PINCH.0, CRACK_PINCH.1);
                    let w = 0.5 * wscale * f;
                    let v = -dep * ((w - dc) / g + 0.5).clamp(0.0, 1.0);
                    let q = j * nu + i;
                    gg.v[q] = gg.v[q].min(v);
                }
            }
        }
    }
    for jn in &sp.joints {
        let l = Line::new(jn.clone());
        for j in 0..nz {
            for i in 0..nu {
                if !cut.at(i, j) && l.dist(at(i, j)) < g {
                    cut.set(i, j, true);
                }
            }
        }
    }
    // the sub-faces
    let free = G {
        nx: nu,
        ny: nz,
        v: cut.v.iter().map(|&c| !c).collect(),
    };
    let (labels, n_labels) = super::util::label(&free);
    let near = {
        let bg = labels.map(|x| x == 0);
        let (_, idx) = dt_kit::distance_nearest(
            &labels.v.iter().map(|&x| x != 0).collect::<Vec<_>>(),
            nu,
            nz,
        );
        let _ = bg;
        idx
    };
    let mut labels_near = G::new(nu, nz, 0usize);
    for q in 0..nu * nz {
        labels_near.v[q] = if labels.v[q] != 0 {
            labels.v[q] as usize
        } else {
            near[q].map_or(0, |k| labels.v[k] as usize)
        };
    }
    let nl = n_labels as usize + 1;
    let th: Vec<f64> = (0..nl)
        .map(|_| rng.uniform(0.0, std::f64::consts::TAU))
        .collect();
    let mut pgrid = Field::new(nu, nz, 0.0);
    let (mut lo, mut hi) = (vec![f64::INFINITY; nl], vec![f64::NEG_INFINITY; nl]);
    for j in 0..nz {
        for i in 0..nu {
            let q = j * nu + i;
            let l = labels_near.v[q];
            let p = libm::cos(th[l]) * (i as f64 * g) + libm::sin(th[l]) * (sp.z0 + j as f64 * g);
            pgrid.v[q] = p;
            lo[l] = lo[l].min(p);
            hi[l] = hi[l].max(p);
        }
    }
    let tilt: Vec<f64> = (0..nl)
        .map(|_| rng.uniform(FACET_TILT_DEG.0, FACET_TILT_DEG.1))
        .collect();
    let k: Vec<f64> = (0..nl)
        .map(|l| {
            tilt[l]
                .to_radians()
                .tan()
                .min(FACET_MM.2 / (hi[l] - lo[l]).max(1e-3))
        })
        .collect();
    let mut tt = Field::new(nu, nz, 0.0);
    for q in 0..nu * nz {
        let l = labels_near.v[q];
        tt.v[q] = k[l] * (pgrid.v[q] - lo[l]);
    }
    // set back FACET_MM[0:2], further across each split
    let mut lines: Vec<(Line, f64)> = cracks
        .iter()
        .filter(|c| c.length() >= CRACK_FINE_MM)
        .map(|c| (c.clone(), rng.uniform(CRACK_SHIFT_MM.0, CRACK_SHIFT_MM.1)))
        .collect();
    for j in &sp.joints {
        let l = Line::new(j.clone());
        if level(&l) {
            lines.push((l, -JOINT_PROUD_MM));
        }
    }
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(level(&lines[i].0)));
    let mut e: Vec<((usize, usize), f64)> = Vec::new();
    let idx = |p: P2| -> (usize, usize) {
        (
            ((p[0] / g).round().max(0.0) as usize).min(nu - 1),
            (((p[1] - sp.z0) / g).round().max(0.0) as usize).min(nz - 1),
        )
    };
    for oi in order {
        let (cs, shift) = (&lines[oi].0, lines[oi].1);
        let mut s = 0.6;
        let mut samples: Vec<(P2, P2)> = Vec::new(); // (x, nrm)
        while s < cs.length() - 0.5 {
            let x = cs.at(s);
            let a = cs.at(s - 0.2);
            let b = cs.at(s + 0.2);
            let nrm = [a[1] - b[1], b[0] - a[0]];
            let nl_ = norm2(nrm).max(1e-12);
            samples.push((x, [nrm[0] / nl_, nrm[1] / nl_]));
            s += 0.5;
        }
        let lv = level(cs);
        let flip_all = if lv { false } else { rng.random() < 0.5 };
        let mut dmax: Vec<((usize, usize), f64)> = Vec::new();
        for (x, nrm) in samples {
            let left = if lv { nrm[1] > 0.0 } else { flip_all };
            let s0 = idx([x[0] - 0.35 * nrm[0], x[1] - 0.35 * nrm[1]]);
            let s1 = idx([x[0] + 0.35 * nrm[0], x[1] + 0.35 * nrm[1]]);
            let (fr, bk) = if left {
                (labels_near.at(s0.0, s0.1), labels_near.at(s1.0, s1.1))
            } else {
                (labels_near.at(s1.0, s1.1), labels_near.at(s0.0, s0.1))
            };
            let dt = if left {
                tt.at(s0.0, s0.1) - tt.at(s1.0, s1.1)
            } else {
                tt.at(s1.0, s1.1) - tt.at(s0.0, s0.1)
            };
            if let Some(m) = dmax.iter_mut().find(|m| m.0 == (fr, bk)) {
                m.1 = m.1.max(dt);
            } else {
                dmax.push(((fr, bk), dt));
            }
        }
        dmax.sort_by_key(|m| m.0);
        for ((p, q), dt) in dmax {
            if p != q && !e.iter().any(|x| x.0 == (q, p)) {
                if let Some(x) = e.iter_mut().find(|x| x.0 == (p, q)) {
                    x.1 = x.1.max(dt + shift);
                } else {
                    e.push(((p, q), dt + shift));
                }
            }
        }
    }
    let deep = if sp.rim {
        FACET_DEEP_MM.1
    } else {
        FACET_DEEP_MM.0
    };
    let mut bset: Vec<f64> = (0..nl)
        .map(|_| rng.uniform(FACET_MM.0, FACET_MM.1))
        .collect();
    for _ in 0..3 {
        for &((p, q), d) in &e {
            bset[q] = bset[q].max(bset[p] + d).min(deep);
        }
    }
    let mut f = Field::new(nu, nz, 0.0);
    for q in 0..nu * nz {
        f.v[q] = -(bset[labels_near.v[q]] + tt.v[q]);
    }
    f.v = blur2(&f.v, nu, nz, FACET_SEAM_MM / g);
    let _ = (edt_out, HashSet::<u8>::new);
    Face {
        p0: sp.p0,
        t: sp.t,
        m: [sp.t[1], -sp.t[0]],
        l: sp.l,
        z0: sp.z0,
        g: gg,
        f,
        cracks,
        rim: sp.rim,
    }
}
