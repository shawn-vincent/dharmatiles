//! Where blades go and how long they are: strands traced along the flow,
//! fronds rooted along them, each drawn and shortened to fit.
//!
//! Blades are printed things, so their sizes are true mm and do not scale
//! with the system's square; the strand spacing is the blade density and
//! does not either.

use dt_kit::{Draw, Field, Rng};
use rayon::prelude::*;

use super::flow::{Flow, Swaths, poisson_sites};

pub const STRAND_SPACING_MM: f64 = 2.65;
const STRAND_LEN_MM: (f64, f64) = (15.0, 30.0);
const STRAND_BACK_FRAC: (f64, f64) = (0.2, 0.6);
const STRAND_MARGIN_MM: f64 = 8.0;
const STRAND_GAP_FRAC: f64 = 0.5;
const STRAND_LATERAL_MM: f64 = 0.3;
pub const TRACE_STEP_MM: f64 = 0.25;
pub const FROND_HALF_W_MM: (f64, f64) = (0.8, 1.25);
const FROND_MIN_LEN_MM: f64 = 1.5;
/// How far along the flow a root under a rock, trunk or wall looks for
/// open ground to grow from instead.
const ROOT_EXIT_MM: f64 = 10.0;
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
    len: &Draw,
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
            let l = len.draw(rng);
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
    /// Distance to the area's edge (outside nodes: another region, water),
    /// mm; `None` when the grass's area is the whole tile.
    pub edge_dist: Option<Field>,
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
    len: &Draw,
    hgt: &Draw,
    cover: f64,
    edge_rng: &mut Rng,
) -> (Vec<Frond>, usize) {
    let (roots, l0, n_strands) = make_strand_roots(rng, flow, (w, h), len);
    let keep: Vec<usize> = (0..roots.len())
        .filter(|&i| roots[i][0] >= 0.0 && roots[i][0] < w && roots[i][1] >= 0.0 && roots[i][1] < h)
        .collect();
    let roots: Vec<[f64; 2]> = keep.iter().map(|&i| roots[i]).collect();
    let l0: Vec<f64> = keep.iter().map(|&i| l0[i]).collect();
    let n = roots.len();
    let idx = |p: [f64; 2]| {
        let (i, j) = surf.soil.node(p[0], p[1]);
        j * surf.soil.nx + i
    };
    // a root under a rock, a trunk or a wall grows where the flow leaves
    // it, so the grass keeps its density on open ground and presses
    // against what stands there instead of thinning round it
    let roots = match &surf.blk_depth {
        Some(blk) => {
            let under: Vec<usize> = (0..n).filter(|&i| blk.z[idx(roots[i])] > 0.0).collect();
            let starts: Vec<[f64; 2]> = under.iter().map(|&i| roots[i]).collect();
            let exits = trace_batch(&starts, ROOT_EXIT_MM, flow, None, 1.0);
            let mut moved = roots;
            for (&i, path) in under.iter().zip(&exits) {
                let open = path.iter().find(|p| {
                    p[0] >= 0.0 && p[0] < w && p[1] >= 0.0 && p[1] < h && blk.z[idx(**p)] <= 0.0
                });
                if let Some(p) = open {
                    moved[i] = *p;
                }
            }
            moved
        }
        None => roots,
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
    let lmax = len.hi() * 1.1 + 0.5;
    let paths = trace_batch(&roots, lmax, flow, Some(&delta), 1.0);
    let grower = Grower {
        surf,
        sw,
        hgt,
        cover,
        w,
        h,
    };
    let mut taken = Taken::default();
    let mut out = Vec::new();
    for f in 0..n {
        if let Some(fr) = grower.grow(rng, roots[f], l0[f], &paths[f], bank[f], &mut taken) {
            out.push(fr);
        }
    }
    edge_fill(edge_rng, &grower, flow, len, lmax, &mut taken, &mut out);
    (out, n_strands)
}

/// Roots already grown, binned by `MIN_ROOT_GAP_MM`.
#[derive(Default)]
struct Taken(std::collections::HashMap<(i64, i64), Vec<[f64; 2]>>);

impl Taken {
    fn bin(p: [f64; 2]) -> (i64, i64) {
        #[allow(clippy::cast_possible_truncation)]
        let b = (
            libm::floor(p[0] / MIN_ROOT_GAP_MM) as i64,
            libm::floor(p[1] / MIN_ROOT_GAP_MM) as i64,
        );
        b
    }

    fn crowded(&self, p: [f64; 2]) -> bool {
        let b = Self::bin(p);
        (-1..=1).any(|di| {
            (-1..=1).any(|dj| {
                self.0.get(&(b.0 + di, b.1 + dj)).is_some_and(|v| {
                    v.iter()
                        .any(|q| libm::hypot(q[0] - p[0], q[1] - p[1]) < MIN_ROOT_GAP_MM)
                })
            })
        })
    }

    fn add(&mut self, p: [f64; 2]) {
        self.0.entry(Self::bin(p)).or_default().push(p);
    }
}

/// What every root is grown against: one rule for the strands' roots and
/// the starts added along an edge.
struct Grower<'a> {
    surf: &'a Surface,
    sw: &'a Swaths,
    hgt: &'a Draw,
    cover: f64,
    w: f64,
    h: f64,
}

impl Grower<'_> {
    fn idx(&self, p: [f64; 2]) -> usize {
        let (i, j) = self.surf.soil.node(p[0], p[1]);
        j * self.surf.soil.nx + i
    }

    /// One frond from `root` along `path` (traced from it a step apart),
    /// drawn and shortened to fit; `None` when it is not grown (under a
    /// blocker, crowded, a soil root that is not a sprig, left out by
    /// `cover`, shortened to nothing).
    fn grow(
        &self,
        rng: &mut Rng,
        root: [f64; 2],
        l0: f64,
        path: &[[f64; 2]],
        bank: bool,
        taken: &mut Taken,
    ) -> Option<Frond> {
        let surf = self.surf;
        let (w, h) = (self.w, self.h);
        let step = TRACE_STEP_MM;
        let k = self.idx(root);
        if surf.blk_depth.as_ref().is_some_and(|b| b.z[k] > 0.0) {
            return None;
        }
        if taken.crowded(root) {
            return None;
        }
        let sprig = surf.soil.z[k] > 0.5;
        if sprig && rng.random() > SPRIG_PROB {
            return None;
        }
        let (blade_len, wmax, mut hf);
        if sprig {
            blade_len = rng.uniform(SPRIG_LEN_MM.0, SPRIG_LEN_MM.1);
            wmax = SPRIG_HALF_W_MM;
            hf = SPRIG_H_MM;
        } else {
            let (_, s, _) = self.sw.coords(root[0], root[1]);
            blade_len = l0 * (0.8 + 0.3 * s);
            wmax = rng.uniform(FROND_HALF_W_MM.0, FROND_HALF_W_MM.1);
            hf = self.hgt.draw(rng);
        }
        let is_bank = bank && !sprig;
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
        let grown = rng.random() < self.cover;
        if !grown {
            return None;
        }
        // shorten (never slice) so the tip ends inside the tile, at most
        // `spill` into bare soil, and at most `tuck` into an obstacle
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
            let kk = self.idx(*p);
            let deep = !sprig && surf.soil_depth.z[kk] > spill;
            let in_blk = surf
                .blk_depth
                .as_ref()
                .is_some_and(|b| b.z[kk] > surf.tuck_mm);
            if folds || rim || deep || in_blk || surf.outside[kk] {
                #[allow(clippy::cast_precision_loss)]
                let at = t as f64 * step;
                stop = Some(at);
                break;
            }
        }
        let mut length = blade_len;
        if let Some(l) = stop {
            if l < FROND_MIN_LEN_MM {
                return None;
            }
            hf *= libm::pow((l / blade_len).clamp(0.3, 1.0), 0.8);
            length = l;
        }
        // blade height falls beside an obstacle, where it presses against it
        if let Some(near) = &surf.near {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let tip = path[((length / step) as usize).min(path.len() - 1)];
            hf *= near.z[self.idx(tip)];
        }
        taken.add(root);
        Some(Frond {
            path: path.to_vec(),
            length,
            half_w: wmax,
            height: hf,
        })
    }
}

/// How near the area's edge (not the tile's own) bare ground is filled.
const EDGE_REACH_MM: f64 = 4.0;
/// Ground this near the tile's own edge is left to the rim rule.
const EDGE_RIM_MM: f64 = 1.6;
/// A start upstream of a gap needs this much room inside the area to lay
/// a blade's full part over it; with less, the blade is grown the other
/// way, against the edge.
const EDGE_BACK_MM: f64 = 1.0;
/// How far from its root a blade's middle lies, as a share of its length.
const EDGE_MID: f64 = 0.45;

/// A blade's half width from above `s` mm along a spine `len` long: the
/// stroke gains half a line a side every 0.45 mm from each end.
fn footprint_half(s: f64, len: f64, half_w: f64) -> f64 {
    half_w
        .min(0.3 + 0.5 * s)
        .min(0.3 + 0.5 * (len - s))
        .max(0.3)
}

/// Mark the lattice nodes under frond `f` in `cov`.
fn cover_frond(cov: &mut [bool], soil: &Field, f: &Frond) {
    let step = TRACE_STEP_MM;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((f.length / step) as usize).min(f.path.len() - 1);
    for (t, p) in f.path.iter().take(n + 1).enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let r = footprint_half(t as f64 * step, f.length, f.half_w);
        let (i0, j0) = soil.node(p[0] - r, p[1] - r);
        let (i1, j1) = soil.node(p[0] + r, p[1] + r);
        for j in j0..=j1 {
            for i in i0..=i1 {
                if libm::hypot(soil.x(i) - p[0], soil.y(j) - p[1]) <= r {
                    cov[j * soil.nx + i] = true;
                }
            }
        }
    }
}

/// Blade starts added along the area's edge (a line, a region's border,
/// the waterline; not the tile's own edge): wherever ground the grass may
/// grow on lies within `EDGE_REACH_MM` of the edge and no blade covers it,
/// a blade is grown over it through the same rule as every other root.
/// Its start is upstream along the flow so the blade's full part lies over
/// the gap; where the edge is right behind the gap, the blade is grown the
/// other way, against the edge, and shortened at it.
fn edge_fill(
    rng: &mut Rng,
    g: &Grower,
    flow: &Flow,
    len: &Draw,
    lmax: f64,
    taken: &mut Taken,
    out: &mut Vec<Frond>,
) {
    let surf = g.surf;
    let Some(edge) = &surf.edge_dist else {
        return;
    };
    let soil = &surf.soil;
    let (nx, ny) = (soil.nx, soil.ny);
    let mut cov = vec![false; nx * ny];
    for f in out.iter() {
        cover_frond(&mut cov, soil, f);
    }
    let open = |p: [f64; 2]| {
        p[0] >= 0.0 && p[0] < g.w && p[1] >= 0.0 && p[1] < g.h && {
            let k = g.idx(p);
            !surf.outside[k]
                && surf.soil.z[k] <= 0.5
                && !surf.blk_depth.as_ref().is_some_and(|b| b.z[k] > 0.0)
        }
    };
    for j in 0..ny {
        for i in 0..nx {
            let k = j * nx + i;
            let p = [soil.x(i), soil.y(j)];
            if cov[k]
                || edge.z[k] > EDGE_REACH_MM
                || p[0] < EDGE_RIM_MM
                || p[0] > g.w - EDGE_RIM_MM
                || p[1] < EDGE_RIM_MM
                || p[1] > g.h - EDGE_RIM_MM
                || !open(p)
            {
                continue;
            }
            // tried once, grown or not
            cov[k] = true;
            let l0 = len.draw(rng);
            let wander = rng.uniform(-BANK_WANDER_DEG, BANK_WANDER_DEG)
                + rng.uniform(-FROND_DELTA_DEG, FROND_DELTA_DEG);
            let d = [wander.to_radians()];
            let reach = EDGE_MID * l0;
            // room upstream of the gap, inside the area
            let back = &trace_batch(&[p], reach, flow, Some(&d), -1.0)[0];
            #[allow(clippy::cast_precision_loss)]
            let room = back
                .iter()
                .take_while(|q| open(**q))
                .count()
                .saturating_sub(1) as f64
                * TRACE_STEP_MM;
            let (root, sign) = if room >= EDGE_BACK_MM {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let t = ((room.min(reach) / TRACE_STEP_MM) as usize).min(back.len() - 1);
                (back[t], 1.0)
            } else {
                // the edge is right behind the gap: start downstream and
                // grow back toward the edge
                let fwd = &trace_batch(&[p], reach, flow, Some(&d), 1.0)[0];
                #[allow(clippy::cast_precision_loss)]
                let ahead = fwd
                    .iter()
                    .take_while(|q| open(**q))
                    .count()
                    .saturating_sub(1) as f64
                    * TRACE_STEP_MM;
                if ahead < EDGE_BACK_MM {
                    continue;
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let t = ((ahead.min(reach) / TRACE_STEP_MM) as usize).min(fwd.len() - 1);
                (fwd[t], -1.0)
            };
            let path = &trace_batch(&[root], lmax, flow, Some(&d), sign)[0];
            if let Some(f) = g.grow(rng, root, l0, path, false, taken) {
                cover_frond(&mut cov, soil, &f);
                out.push(f);
            }
        }
    }
}
