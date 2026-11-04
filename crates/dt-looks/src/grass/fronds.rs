//! Where blades go and how long they are: strands traced along the flow,
//! fronds rooted along them, each drawn and shortened to fit
//! (grasslab/grasslab.py `trace_batch` :578, `make_strand_roots` :603,
//! `plan_fronds` :735, `draw_frond` :765).
//!
//! Blades are printed things, so their sizes are true mm and do not scale
//! with the system's square; the strand spacing is the blade density and
//! does not either.

use dt_kit::{Field, Rng};
use rayon::prelude::*;

use super::flow::{Flow, MARGIN_MM, Swaths, poisson_sites};

pub const STRAND_SPACING_MM: f64 = 2.65;
const STRAND_LEN_MM: (f64, f64) = (15.0, 30.0);
const STRAND_BACK_FRAC: (f64, f64) = (0.2, 0.6);
const STRAND_MARGIN_MM: f64 = 8.0;
const STRAND_GAP_FRAC: f64 = 0.5;
const STRAND_LATERAL_MM: f64 = 0.3;
pub const TRACE_STEP_MM: f64 = 0.25;
pub const FROND_HALF_W_MM: (f64, f64) = (0.8, 1.25);
const FROND_MIN_LEN_MM: f64 = 1.5;
const FROND_DELTA_DEG: f64 = 4.0;
const STRAY_FRAC: f64 = 0.05;
const STRAY_DELTA_DEG: (f64, f64) = (20.0, 35.0);
const SPRIG_PROB: f64 = 0.10;
const SPRIG_LEN_MM: (f64, f64) = (1.8, 3.0);
const SPRIG_HALF_W_MM: f64 = 0.5;
const SPRIG_H_MM: f64 = 0.6;
const RIM_TIP_INSET_MM: (f64, f64) = (0.2, 1.5);
const SOIL_SPILL_MM: (f64, f64) = (0.8, 3.2);
const WISP_FRAC: f64 = 0.04;
const WISP_SPILL_MM: f64 = 4.0;
const BANK_REACH_MM: f64 = 4.0;
const BANK_WANDER_DEG: f64 = 15.0;
const BANK_STRAY_FRAC: f64 = 0.12;
const BANK_H_SCALE: (f64, f64) = (0.65, 1.15);
/// Two blades rooted closer than this are one blade to the eye, and their
/// near-coincident surfaces are what a union cannot cleanly resolve: the
/// later is not grown.
const MIN_ROOT_GAP_MM: f64 = 0.1;

/// `lo + (hi − lo)·u²`: a draw skewed toward `lo`.
fn skewed(rng: &mut Rng, (lo, hi): (f64, f64)) -> f64 {
    let u = rng.random();
    lo + (hi - lo) * u * u
}

/// RK2 streamlines of the flow from each start: `n + 1` points, a step
/// `step` apart in arc length. `delta` rotates each heading by a constant
/// (radians); `sign` −1 runs upstream.
pub fn trace_batch(
    starts: &[[f64; 2]],
    length: f64,
    flow: &Flow,
    delta: Option<&[f64]>,
    sign: f64,
) -> Vec<Vec<[f64; 2]>> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((length / TRACE_STEP_MM).ceil() as usize).max(1);
    starts
        .par_iter()
        .enumerate()
        .map(|(i, p0)| {
            let (c, sn) = delta.map_or((1.0, 0.0), |d| (libm::cos(d[i]), libm::sin(d[i])));
            let dir = |q: [f64; 2]| {
                let v = flow.at(q[0], q[1]);
                [sign * (c * v[0] - sn * v[1]), sign * (sn * v[0] + c * v[1])]
            };
            let mut pts = Vec::with_capacity(n + 1);
            let mut q = *p0;
            pts.push(q);
            for _ in 0..n {
                let d1 = dir(q);
                let mid = [
                    q[0] + 0.5 * TRACE_STEP_MM * d1[0],
                    q[1] + 0.5 * TRACE_STEP_MM * d1[1],
                ];
                let d2 = dir(mid);
                q = [q[0] + TRACE_STEP_MM * d2[0], q[1] + TRACE_STEP_MM * d2[1]];
                pts.push(q);
            }
            pts
        })
        .collect()
}

/// Frond roots along strands (REQ-FL-3): long streamlines, fronds
/// shingled. Returns the roots, each root's frond length before swath
/// scaling, and the strand count.
fn make_strand_roots(
    rng: &mut Rng,
    flow: &Flow,
    (w, h): (f64, f64),
    len: (f64, f64),
) -> (Vec<[f64; 2]>, Vec<f64>, usize) {
    let m = STRAND_MARGIN_MM;
    let seeds = poisson_sites(rng, (-m, -m, w + m, h + m), STRAND_SPACING_MM);
    let n = seeds.len();
    let slen: Vec<f64> = (0..n)
        .map(|_| rng.uniform(STRAND_LEN_MM.0, STRAND_LEN_MM.1))
        .collect();
    let back: Vec<f64> = slen
        .iter()
        .map(|l| l * rng.uniform(STRAND_BACK_FRAC.0, STRAND_BACK_FRAC.1))
        .collect();
    let back_max = back.iter().copied().fold(0.0, f64::max);
    let bp = trace_batch(&seeds, back_max, flow, None, -1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let start: Vec<[f64; 2]> = (0..n)
        .map(|i| bp[i][((back[i] / TRACE_STEP_MM) as usize).min(bp[i].len() - 1)])
        .collect();
    let slen_max = slen.iter().copied().fold(0.0, f64::max);
    let fp = trace_batch(&start, slen_max, flow, None, 1.0);
    let mut roots = Vec::new();
    let mut lens = Vec::new();
    for i in 0..n {
        let mut u = 0.0;
        while u < slen[i] {
            let l = rng.uniform(len.0, len.1);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = ((u / TRACE_STEP_MM) as usize).min(fp[i].len() - 2);
            let (p, q) = (fp[i][k], fp[i][k + 1]);
            let t = [q[0] - p[0], q[1] - p[1]];
            let tn = libm::hypot(t[0], t[1]).max(1e-9);
            let lat = rng.uniform(-1.0, 1.0) * STRAND_LATERAL_MM;
            roots.push([p[0] + lat * (-t[1] / tn), p[1] + lat * (t[0] / tn)]);
            lens.push(l);
            u += STRAND_GAP_FRAC * l * rng.uniform(0.8, 1.2);
        }
    }
    (roots, lens, n)
}

/// What the fronds read per node of the tile's lattice.
pub struct Surface {
    /// Bare-earth mask (clearings), 0 to 1.
    pub soil: Field,
    /// Depth inside bare earth, mm.
    pub soil_depth: Field,
    /// Distance from grass to bare earth, mm.
    pub soil_dist: Field,
    /// Depth inside a blocker (a rock, a trunk), mm; `None` without one.
    pub blk_depth: Option<Field>,
    /// Blade height factor beside a blocker (1 = none).
    pub near: Option<Field>,
    /// Nodes where nothing grows and blades stop: outside the area, under
    /// water.
    pub outside: Vec<bool>,
    /// How far into a blocker a spine may run before it stops, mm.
    pub tuck_mm: f64,
}

/// One frond's drawn dimensions after shortening.
#[derive(Clone, Debug)]
pub struct Frond {
    /// The traced spine, a step apart from the root.
    pub path: Vec<[f64; 2]>,
    /// Blade length along its spine.
    pub length: f64,
    /// Nominal half width.
    pub half_w: f64,
    /// Crest height above the ground.
    pub height: f64,
}

/// The fronds of a tile `w × h`: roots along strands, then each drawn and
/// shortened (never sliced) to fit; `None` for those not built (under a
/// blocker, soil roots that did not become sprigs, shortened to nothing).
/// Returns the fronds and the strand count.
#[allow(clippy::too_many_arguments)]
pub fn plan(
    rng: &mut Rng,
    surf: &Surface,
    sw: &Swaths,
    flow: &Flow,
    (w, h): (f64, f64),
    len: (f64, f64),
    hgt: (f64, f64),
    cover: f64,
) -> (Vec<Frond>, usize) {
    let (roots, l0, n_strands) = make_strand_roots(rng, flow, (w, h), len);
    let keep: Vec<usize> = (0..roots.len())
        .filter(|&i| roots[i][0] >= 0.0 && roots[i][0] < w && roots[i][1] >= 0.0 && roots[i][1] < h)
        .collect();
    let _ = MARGIN_MM;
    let roots: Vec<[f64; 2]> = keep.iter().map(|&i| roots[i]).collect();
    let l0: Vec<f64> = keep.iter().map(|&i| l0[i]).collect();
    let n = roots.len();
    let cell = |p: [f64; 2]| surf.soil.node(p[0], p[1]);
    let idx = |p: [f64; 2]| {
        let (i, j) = cell(p);
        j * surf.soil.nx + i
    };
    let bank: Vec<bool> = roots
        .iter()
        .map(|&p| {
            let k = idx(p);
            surf.soil_dist.z[k] < BANK_REACH_MM && surf.soil.z[k] <= 0.5
        })
        .collect();
    let delta: Vec<f64> = (0..n)
        .map(|i| {
            let stray = rng.random() < if bank[i] { BANK_STRAY_FRAC } else { STRAY_FRAC };
            let d = if stray {
                let sgn = if rng.random() < 0.5 { -1.0 } else { 1.0 };
                sgn * rng.uniform(STRAY_DELTA_DEG.0, STRAY_DELTA_DEG.1)
            } else {
                rng.uniform(-FROND_DELTA_DEG, FROND_DELTA_DEG)
            };
            let wander = if bank[i] {
                rng.uniform(-BANK_WANDER_DEG, BANK_WANDER_DEG)
            } else {
                0.0
            };
            (d + wander).to_radians()
        })
        .collect();
    let lmax = len.1 * 1.1 + 0.5;
    let paths = trace_batch(&roots, lmax, flow, Some(&delta), 1.0);
    let step = TRACE_STEP_MM;
    let mut out = Vec::new();
    let mut taken: std::collections::HashMap<(i64, i64), Vec<[f64; 2]>> =
        std::collections::HashMap::new();
    let bin = |p: [f64; 2]| {
        #[allow(clippy::cast_possible_truncation)]
        let b = (
            libm::floor(p[0] / MIN_ROOT_GAP_MM) as i64,
            libm::floor(p[1] / MIN_ROOT_GAP_MM) as i64,
        );
        b
    };
    for f in 0..n {
        let k = idx(roots[f]);
        if surf.blk_depth.as_ref().is_some_and(|b| b.z[k] > 0.0) {
            continue;
        }
        let b = bin(roots[f]);
        let crowded = (-1..=1).any(|di| {
            (-1..=1).any(|dj| {
                taken.get(&(b.0 + di, b.1 + dj)).is_some_and(|v| {
                    v.iter().any(|q| {
                        libm::hypot(q[0] - roots[f][0], q[1] - roots[f][1]) < MIN_ROOT_GAP_MM
                    })
                })
            })
        });
        if crowded {
            continue;
        }
        let sprig = surf.soil.z[k] > 0.5;
        if sprig && rng.random() > SPRIG_PROB {
            continue;
        }
        let (blade_len, wmax, mut hf);
        if sprig {
            blade_len = rng.uniform(SPRIG_LEN_MM.0, SPRIG_LEN_MM.1);
            wmax = SPRIG_HALF_W_MM;
            hf = SPRIG_H_MM;
        } else {
            let (_, s, _) = sw.coords(roots[f][0], roots[f][1]);
            blade_len = l0[f] * (0.8 + 0.3 * s);
            wmax = rng.uniform(FROND_HALF_W_MM.0, FROND_HALF_W_MM.1);
            hf = rng.uniform(hgt.0, hgt.1);
        }
        let is_bank = bank[f] && !sprig;
        if is_bank {
            hf *= rng.uniform(BANK_H_SCALE.0, BANK_H_SCALE.1);
        }
        let inset = skewed(rng, RIM_TIP_INSET_MM);
        let spill = if rng.random() < WISP_FRAC {
            WISP_SPILL_MM
        } else {
            skewed(rng, SOIL_SPILL_MM)
        };
        // cover: a share of the roots grows nothing (drawn after the
        // others so the stream does not shift with it)
        let grown = rng.random() < cover;
        if !grown {
            continue;
        }
        // shorten (never slice) so the tip ends inside the tile, at most
        // `spill` into bare soil, and at most `tuck` into an obstacle
        let path = &paths[f];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let nl = (((blade_len / step).ceil() as usize) + 1).min(path.len());
        let mut stop = None;
        for (t, p) in path.iter().take(nl).enumerate() {
            // a streamline that runs into a sink (where the rim rule turns
            // the flow back on itself) stalls and folds: the blade ends
            // where it still runs straight on
            let folds = t >= 2 && {
                let (a, b, c) = (path[t - 2], path[t - 1], *p);
                let (u, v) = ([b[0] - a[0], b[1] - a[1]], [c[0] - b[0], c[1] - b[1]]);
                u[0] * v[0] + u[1] * v[1] < 0.5 * step * step
            };
            let rim = p[0] < inset || p[0] > w - inset || p[1] < inset || p[1] > h - inset;
            let kk = idx(*p);
            let deep = !sprig && surf.soil_depth.z[kk] > spill;
            let in_blk = surf
                .blk_depth
                .as_ref()
                .is_some_and(|b| b.z[kk] > surf.tuck_mm);
            if folds || rim || deep || in_blk || surf.outside[kk] {
                stop = Some(t as f64 * step);
                break;
            }
        }
        let mut length = blade_len;
        if let Some(l) = stop {
            if l < FROND_MIN_LEN_MM {
                continue;
            }
            hf *= libm::pow((l / blade_len).clamp(0.3, 1.0), 0.8);
            length = l;
        }
        // blade height falls beside an obstacle, where it presses against it
        if let Some(near) = &surf.near {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let tip = path[((length / step) as usize).min(path.len() - 1)];
            hf *= near.z[idx(tip)];
        }
        taken.entry(b).or_default().push(roots[f]);
        out.push(Frond {
            path: path.clone(),
            length,
            half_w: wmax,
            height: hf,
        });
    }
    (out, n_strands)
}
