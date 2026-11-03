//! The grass's field: the flow the blades lean along, the swaths that set
//! their length, the clearings of bare earth the flow parts around, and the
//! rim rule (grasslab/grasslab.py `make_direction` :311, `Swaths` :362,
//! `place_lenses` :442, `finish_flow` :467, `steer_rim` :527).
//!
//! Everything works on one angle per point, never on vectors, so the flow
//! cannot form a whorl. Lengths here are mm at the reference square and
//! scale with `s`; a blade is a printed thing and does not (see `mod.rs`).

use core::f64::consts::PI;

use dt_kit::{Rng, Seed};
use rayon::prelude::*;

use super::lat::{Lat, kernel, s01, wrap};

// 2 flow field
const COHERENCE_MM: f64 = 15.0;
const THETA_STD_DEG: f64 = 60.0;
const TERRAIN_FOLLOW: f64 = 0.3;
const MEANDER_WAVELENGTH_MM: f64 = 15.0;
const MEANDER_AMP_SIGMA_MM: f64 = 12.0;
const MEANDER_PHASE_SIGMA_MM: f64 = 14.0;
const MAX_GROUND_SLOPE_DEG: f64 = 34.0;
// 3 swaths
pub const SWATH_SPACING_MM: f64 = 11.0;
const SWATH_LATERAL_ASPECT: f64 = 0.65;
const SWATH_WARP_MM: f64 = 2.5;
const SWATH_WARP_SIGMA_MM: f64 = 3.0;
const SWATH_AMP_MM: (f64, f64) = (1.2, 2.2);
// 4 lenses
const SOIL_LENS_CANDIDATES: usize = 80;
const SOIL_LENS_SCORE_NOISE: f64 = 0.8;
const SOIL_LENS_HALF_LEN_MM: (f64, f64) = (5.0, 9.0);
const SOIL_LENS_ASPECT: (f64, f64) = (0.45, 0.65);
const SOIL_LENS_SEP_MM: f64 = 13.0;
const SOIL_RIM_KEEP_MM: f64 = 7.0;
const SOIL_LENS_WARP_MM: f64 = 0.8;
const SOIL_LENS_WARP_SIGMA_MM: f64 = 1.2;
pub const SOIL_EDGE_MM: f64 = 0.5;
const DEFLECT_BAND_MM: (f64, f64) = (1.5, 4.5);
const DEFLECT_STRENGTH: (f64, f64) = (0.55, 1.0);
const PARTING_PUSH: f64 = 0.6;
const PARTING_SOFT_MM: f64 = 0.8;
// 4b rim
const RIM_NOISE_SIGMA_MM: f64 = 8.0;
const EDGE_FLOW_MM: (f64, f64) = (9.0, 15.0);
const EDGE_OK_DEG: (f64, f64) = (70.0, 88.0);
const EDGE_PULL: (f64, f64) = (0.15, 0.45);
const RIM_ALONG_MM: f64 = 3.0;
const STEER_SIDE_SIGMA_MM: f64 = 20.0;
const STEER_SIDE_HARD_MM: f64 = 2.5;
const STEER_SIDE_SOFT_MM: f64 = 8.0;
const STEER_NATURAL_DEG: f64 = 30.0;
const RIM_SQUARE_PULL: f64 = 0.85;
const RIM_SQUARE_REACH: f64 = 1.0;
const PULL_SOFT_RAD: f64 = 0.35;
const STEER_CORNER_MM: f64 = 2.0;
const STEER_SMOOTH_MM: f64 = 1.5;

/// Margin of the extended lattice past the tile, mm.
pub const MARGIN_MM: f64 = 14.0;

/// The extended lattice of a tile `w × h`.
pub fn lattice(w: f64, h: f64, res: f64) -> Lat {
    Lat::new(
        -MARGIN_MM,
        -MARGIN_MM,
        w + MARGIN_MM,
        h + MARGIN_MM,
        res,
        0.0,
    )
}

/// The direction field: unit vectors on the extended lattice.
#[derive(Clone, Debug)]
pub struct Flow {
    /// x component.
    pub ux: Lat,
    /// y component.
    pub uy: Lat,
}

impl Flow {
    fn new(vx: &[f64], vy: &[f64], like: &Lat) -> Self {
        let n: Vec<f64> = vx
            .iter()
            .zip(vy)
            .map(|(a, b)| libm::hypot(*a, *b).max(1e-9))
            .collect();
        Self {
            ux: like.with(vx.iter().zip(&n).map(|(a, n)| a / n).collect()),
            uy: like.with(vy.iter().zip(&n).map(|(a, n)| a / n).collect()),
        }
    }

    /// The unit direction at `(x, y)`.
    pub fn at(&self, x: f64, y: f64) -> [f64; 2] {
        [self.ux.sample(x, y), self.uy.sample(x, y)]
    }
}

/// `s` with `∇s ≈ (cos θ, sin θ)`: the least-squares fit by a cosine
/// transform (the mirror-extended FFT Poisson solve of grasslab's
/// `_along_flow_coord`). Lines of constant `s` cross the flow, so
/// `sin(2π s / λ)` makes neighbouring streamlines wave in phase.
fn along_flow_coord(th: &Lat) -> Lat {
    let (nx, ny, h) = (th.nx, th.ny, th.res);
    let cx = th.with(th.z.iter().map(|a| libm::cos(*a)).collect());
    let cy = th.with(th.z.iter().map(|a| libm::sin(*a)).collect());
    let (dx, _) = cx.gradient();
    let (_, dy) = cy.gradient();
    let div: Vec<f64> = dx.iter().zip(&dy).map(|(a, b)| a + b).collect();
    let tab = |n: usize| -> Vec<f64> {
        let mut t = vec![0.0; n * n];
        for m in 0..n {
            for k in 0..n {
                t[m * n + k] = libm::cos(PI * m as f64 * (k as f64 + 0.5) / n as f64);
            }
        }
        t
    };
    let (cxt, cyt) = (tab(nx), tab(ny));
    // forward over x (each row), then over y (each column)
    let fwd_x = |d: &[f64]| -> Vec<f64> {
        let mut out = vec![0.0; nx * ny];
        out.par_chunks_mut(nx).enumerate().for_each(|(j, row)| {
            for (m, o) in row.iter_mut().enumerate() {
                let c = &cxt[m * nx..(m + 1) * nx];
                *o = (0..nx).map(|i| d[j * nx + i] * c[i]).sum();
            }
        });
        out
    };
    let fwd_y = |d: &[f64]| -> Vec<f64> {
        let mut out = vec![0.0; nx * ny];
        out.par_chunks_mut(nx).enumerate().for_each(|(m, row)| {
            let c = &cyt[m * ny..(m + 1) * ny];
            for (i, o) in row.iter_mut().enumerate() {
                *o = (0..ny).map(|j| d[j * nx + i] * c[j]).sum();
            }
        });
        out
    };
    let mut f = fwd_y(&fwd_x(&div));
    for my in 0..ny {
        for mx in 0..nx {
            let kx = PI * mx as f64 / (nx as f64 * h);
            let ky = PI * my as f64 / (ny as f64 * h);
            let k2 = kx * kx + ky * ky;
            f[my * nx + mx] = if mx == 0 && my == 0 {
                0.0
            } else {
                // DCT-III weights (1/N for the zero mode, 2/N otherwise)
                let wx = if mx == 0 { 1.0 } else { 2.0 } / nx as f64;
                let wy = if my == 0 { 1.0 } else { 2.0 } / ny as f64;
                -f[my * nx + mx] / k2 * wx * wy
            };
        }
    }
    // inverse: x[n] = Σ_m F_m cos(...) (weights already folded in)
    let mut tmp = vec![0.0; nx * ny];
    tmp.par_chunks_mut(nx).enumerate().for_each(|(j, row)| {
        for (i, o) in row.iter_mut().enumerate() {
            *o = (0..ny).map(|m| f[m * nx + i] * cyt[m * ny + j]).sum();
        }
    });
    let mut out = vec![0.0; nx * ny];
    out.par_chunks_mut(nx).enumerate().for_each(|(j, row)| {
        for (i, o) in row.iter_mut().enumerate() {
            *o = (0..nx).map(|m| tmp[j * nx + m] * cxt[m * nx + i]).sum();
        }
    });
    th.with(out)
}

/// The regional angle plus in-phase meanders, with a bounded downhill pull
/// (REQ-FL-2). `ground` is the ground's height on the same lattice.
pub fn make_direction(
    seed: Seed,
    rng: &mut Rng,
    ground: &Lat,
    s: f64,
    meander: (f64, f64),
) -> Flow {
    let th_r0 = rng.uniform(0.0, 2.0 * PI);
    let nz = ground.noise(seed.key("regional"), COHERENCE_MM * s);
    let th_r = nz.with(
        nz.z.iter()
            .map(|n| th_r0 + THETA_STD_DEG.to_radians() * n)
            .collect(),
    );
    let s_al = along_flow_coord(&th_r);
    let (lo, hi) = (meander.0.to_radians(), meander.1.to_radians());
    let amp_n = ground.noise(seed.key("amp"), MEANDER_AMP_SIGMA_MM * s);
    let ph_n = ground.noise(seed.key("phase"), MEANDER_PHASE_SIGMA_MM * s);
    let (gx, gy) = ground.gradient();
    let tan_max = libm::tan(MAX_GROUND_SLOPE_DEG.to_radians());
    let n = ground.z.len();
    let mut vx = vec![0.0; n];
    let mut vy = vec![0.0; n];
    for k in 0..n {
        let amp = lo + (hi - lo) * s01(0.5 + 0.35 * amp_n.z[k]);
        let phase = PI * ph_n.z[k];
        let th =
            th_r.z[k] + amp * libm::sin(2.0 * PI * s_al.z[k] / (MEANDER_WAVELENGTH_MM * s) + phase);
        let slope = libm::hypot(gx[k], gy[k]);
        let a = TERRAIN_FOLLOW * (slope / tan_max).clamp(0.0, 1.0);
        let (dx, dy) = if slope > 1e-9 {
            (-gx[k] / slope, -gy[k] / slope)
        } else {
            (0.0, 0.0)
        };
        vx[k] = (1.0 - a) * libm::cos(th) + a * dx;
        vy[k] = (1.0 - a) * libm::sin(th) + a * dy;
    }
    Flow::new(&vx, &vy, ground)
}

/// Dart-throwing Poisson-disk sites of radius `r` in `[x0, x1] × [y0, y1]`.
pub fn poisson_sites(
    rng: &mut Rng,
    (x0, y0, x1, y1): (f64, f64, f64, f64),
    r: f64,
) -> Vec<[f64; 2]> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let tries = (30.0 * (x1 - x0) * (y1 - y0) / (r * r)) as usize;
    let cell = r / core::f64::consts::SQRT_2;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (gw, gh) = (
        ((x1 - x0) / cell) as usize + 1,
        ((y1 - y0) / cell) as usize + 1,
    );
    let mut grid = vec![usize::MAX; gw * gh];
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for _ in 0..tries {
        let p = [rng.uniform(x0, x1), rng.uniform(y0, y1)];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (gi, gj) = (((p[0] - x0) / cell) as usize, ((p[1] - y0) / cell) as usize);
        let mut ok = true;
        'scan: for j in gj.saturating_sub(2)..(gj + 3).min(gh) {
            for i in gi.saturating_sub(2)..(gi + 3).min(gw) {
                let k = grid[j * gw + i];
                if k != usize::MAX {
                    let q = pts[k];
                    if (q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) < r * r {
                        ok = false;
                        break 'scan;
                    }
                }
            }
        }
        if ok {
            grid[gj * gw + gi] = pts.len();
            pts.push(p);
        }
    }
    pts
}

/// Warped anisotropic Voronoi cells, each with a wave-height envelope. Every
/// point has exactly one owner, so coverage is total. The swath groups
/// frond roots (strand lengths); frond directions come from the flow.
pub struct Swaths {
    sites: Vec<[f64; 2]>,
    axis: Vec<[f64; 2]>,
    perp: Vec<[f64; 2]>,
    amp: Vec<f64>,
    warp: Lat,
    phase: Vec<f64>,
    a_min: Vec<f64>,
    length: Vec<f64>,
    b_max: Vec<f64>,
}

impl Swaths {
    /// Swath cells over the tile `w × h`.
    pub fn new(seed: Seed, rng: &mut Rng, w: f64, h: f64, flow: &Flow, s: f64) -> Self {
        let m = MARGIN_MM;
        let sites = poisson_sites(rng, (-m, -m, w + m, h + m), SWATH_SPACING_MM * s);
        let n = sites.len();
        let mut axis = Vec::new();
        let mut perp = Vec::new();
        for p in &sites {
            let d = flow.at(p[0], p[1]);
            let nd = libm::hypot(d[0], d[1]).max(1e-12);
            let a = [d[0] / nd, d[1] / nd];
            axis.push(a);
            perp.push([-a[1], a[0]]);
        }
        let amp: Vec<f64> = (0..n)
            .map(|_| rng.uniform(SWATH_AMP_MM.0, SWATH_AMP_MM.1))
            .collect();
        let grid = Lat::new(-m, -m, w + m, h + m, 0.25, 0.0);
        let warp_n = grid.noise(seed.key("warp"), SWATH_WARP_SIGMA_MM * s);
        let warp = warp_n.with(warp_n.z.iter().map(|v| SWATH_WARP_MM * s * v).collect());
        let phase: Vec<f64> = (0..n).map(|_| rng.uniform(-1.0, 1.0)).collect();
        let mut sw = Self {
            sites,
            axis,
            perp,
            amp,
            warp,
            phase,
            a_min: vec![0.0; n],
            length: vec![1.0; n],
            b_max: vec![1.0; n],
        };
        // per-cell normalisation from the extended grid
        let assigned: Vec<(usize, f64, f64)> = (0..grid.ny)
            .into_par_iter()
            .flat_map_iter(|j| {
                let (g, sw) = (&grid, &sw);
                let y = g.y(j);
                (0..g.nx).map(move |i| sw.assign(g.x(i), y))
            })
            .collect();
        let mut a_lo = vec![f64::INFINITY; n];
        let mut a_hi = vec![f64::NEG_INFINITY; n];
        let mut b_hi = vec![0.5f64; n];
        for &(o, a, b) in &assigned {
            a_lo[o] = a_lo[o].min(a);
            a_hi[o] = a_hi[o].max(a);
            b_hi[o] = b_hi[o].max(b.abs());
        }
        for i in 0..n {
            if a_lo[i].is_finite() {
                sw.a_min[i] = a_lo[i];
                sw.length[i] = (a_hi[i] - a_lo[i]).max(1.0);
                sw.b_max[i] = b_hi[i];
            }
        }
        sw
    }

    /// The owner of a point and its along / across coordinates.
    fn assign(&self, x: f64, y: f64) -> (usize, f64, f64) {
        let wp = self.warp.sample(x, y);
        let mut best = (0, f64::INFINITY, 0.0, 0.0);
        for (k, p) in self.sites.iter().enumerate() {
            let (dx, dy) = (x - p[0], y - p[1]);
            let a = dx * self.axis[k][0] + dy * self.axis[k][1];
            let b = dx * self.perp[k][0] + dy * self.perp[k][1];
            let d = libm::hypot(a, b / SWATH_LATERAL_ASPECT) + wp * self.phase[k];
            if d < best.1 {
                best = (k, d, a, b);
            }
        }
        (best.0, best.2, best.3)
    }

    /// The point's cell, and its normalised coordinates: `s` along
    /// (0 to 1), `l` across (−1 to 1).
    pub fn coords(&self, x: f64, y: f64) -> (usize, f64, f64) {
        let (o, a, b) = self.assign(x, y);
        let s = ((a - self.a_min[o]) / self.length[o]).clamp(0.0, 1.0);
        let l = (b / self.b_max[o]).clamp(-1.0, 1.0);
        (o, s, l)
    }

    /// The wave envelope: low upstream and at the sides, crest downstream
    /// centre. Kept for the swath's amplitude; blade length uses `s` only.
    #[allow(dead_code)]
    pub fn env(&self, o: usize, s: f64, l: f64) -> f64 {
        self.amp[o] * (0.25 + 0.75 * libm::pow(s, 1.6)) * libm::pow(1.0 - l * l, 0.6)
    }
}

/// Eye-shaped bare-earth openings aligned with the local flow
/// (REQ-FL-4): `|a| / half_len + (b / half_w)² ≤ 1` in each lens's
/// `(axis, perp)` frame, with a shared border warp.
#[derive(Clone)]
pub struct Lenses {
    /// Centres.
    pub centre: Vec<[f64; 2]>,
    /// Unit axes.
    pub axis: Vec<[f64; 2]>,
    /// Half lengths, mm.
    pub half_len: Vec<f64>,
    /// Half widths, mm.
    pub half_w: Vec<f64>,
    /// The shared border warp.
    pub warp: Lat,
}

impl Lenses {
    fn perp(&self, i: usize) -> [f64; 2] {
        [-self.axis[i][1], self.axis[i][0]]
    }

    /// Approximate distance `d` (mm, < 0 inside), across coordinate `b`,
    /// and the lens-metric gradient `∇f`.
    fn edge(&self, i: usize, x: f64, y: f64) -> (f64, f64, f64, f64) {
        let (dx, dy) = (x - self.centre[i][0], y - self.centre[i][1]);
        let (ax, pp) = (self.axis[i], self.perp(i));
        let a = dx * ax[0] + dy * ax[1];
        let b = dx * pp[0] + dy * pp[1];
        let f = a.abs() / self.half_len[i] + (b / self.half_w[i]).powi(2);
        let ga = if a == 0.0 { 0.0 } else { a.signum() } / self.half_len[i];
        let gb = 2.0 * b / (self.half_w[i] * self.half_w[i]);
        (
            (f - 1.0) * self.half_w[i] / 2.0,
            b,
            ga * ax[0] + gb * pp[0],
            ga * ax[1] + gb * pp[1],
        )
    }

    /// Approximate signed distance to the nearest lens (< 0 inside).
    pub fn dist(&self, x: f64, y: f64) -> f64 {
        let d = (0..self.centre.len())
            .map(|i| self.edge(i, x, y).0)
            .fold(f64::INFINITY, f64::min);
        d + self.warp.sample(x, y)
    }

    /// 1 inside a lens, 0 outside, smooth over the border.
    pub fn mask(&self, x: f64, y: f64) -> f64 {
        s01(0.5 - self.dist(x, y) / SOIL_EDGE_MM)
    }

    /// Lens shapes added (the first `k` of `self`, then `shapes`), sharing
    /// the warp (meadowlab's `combine`).
    pub fn combine(&self, k: usize, shapes: &Shapes) -> Self {
        let mut c = self.centre[..k.min(self.centre.len())].to_vec();
        let mut ax = self.axis[..k.min(self.centre.len())].to_vec();
        let mut l = self.half_len[..k.min(self.centre.len())].to_vec();
        let mut b = self.half_w[..k.min(self.centre.len())].to_vec();
        c.extend(&shapes.centre);
        ax.extend(&shapes.axis);
        l.extend(&shapes.half_len);
        b.extend(&shapes.half_w);
        Self {
            centre: c,
            axis: ax,
            half_len: l,
            half_w: b,
            warp: self.warp.clone(),
        }
    }
}

/// Extra lens shapes: aprons round rocks, circles for the flow.
#[derive(Clone, Default)]
pub struct Shapes {
    /// Centres.
    pub centre: Vec<[f64; 2]>,
    /// Axes.
    pub axis: Vec<[f64; 2]>,
    /// Half lengths.
    pub half_len: Vec<f64>,
    /// Half widths.
    pub half_w: Vec<f64>,
}

/// Lens centres on low ground, at least `SOIL_LENS_SEP_MM` apart, axis =
/// the local flow. `ground` reads the tile's ground height.
#[allow(clippy::too_many_arguments)]
pub fn place_lenses(
    seed: Seed,
    rng: &mut Rng,
    ground: &dyn Fn(f64, f64) -> f64,
    allow: &dyn Fn(f64, f64) -> bool,
    flow: &Flow,
    w: f64,
    h: f64,
    count: usize,
    s: f64,
) -> Lenses {
    let k = SOIL_RIM_KEEP_MM * s;
    let cand: Vec<[f64; 2]> = (0..SOIL_LENS_CANDIDATES)
        .map(|_| {
            [
                rng.uniform(k, (w - k).max(k + 1e-6)),
                rng.uniform(k, (h - k).max(k + 1e-6)),
            ]
        })
        .filter(|c| allow(c[0], c[1]))
        .collect();
    if cand.is_empty() || count == 0 {
        let nz = flow
            .ux
            .noise(seed.key("lens-warp"), SOIL_LENS_WARP_SIGMA_MM * s);
        return Lenses {
            centre: Vec::new(),
            axis: Vec::new(),
            half_len: Vec::new(),
            half_w: Vec::new(),
            warp: nz.with(vec![0.0; nz.z.len()]),
        };
    }
    let hs: Vec<f64> = cand.iter().map(|c| ground(c[0], c[1])).collect();
    #[allow(clippy::cast_precision_loss)]
    let mean = hs.iter().sum::<f64>() / hs.len() as f64;
    #[allow(clippy::cast_precision_loss)]
    let sd = libm::sqrt(hs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / hs.len() as f64);
    let score: Vec<f64> = hs
        .iter()
        .map(|v| -(v - mean) / (sd + 1e-9) + rng.normal(0.0, SOIL_LENS_SCORE_NOISE))
        .collect();
    let mut order: Vec<usize> = (0..cand.len()).collect();
    order.sort_by(|&a, &b| score[b].total_cmp(&score[a]));
    let mut cs: Vec<[f64; 2]> = Vec::new();
    for i in order {
        if cs
            .iter()
            .all(|c| libm::hypot(cand[i][0] - c[0], cand[i][1] - c[1]) >= SOIL_LENS_SEP_MM * s)
        {
            cs.push(cand[i]);
        }
        if cs.len() == count {
            break;
        }
    }
    let axis: Vec<[f64; 2]> = cs
        .iter()
        .map(|c| {
            let d = flow.at(c[0], c[1]);
            let n = libm::hypot(d[0], d[1]).max(1e-12);
            [d[0] / n, d[1] / n]
        })
        .collect();
    let half_len: Vec<f64> = cs
        .iter()
        .map(|_| rng.uniform(SOIL_LENS_HALF_LEN_MM.0, SOIL_LENS_HALF_LEN_MM.1) * s)
        .collect();
    let half_w: Vec<f64> = half_len
        .iter()
        .map(|l| l * rng.uniform(SOIL_LENS_ASPECT.0, SOIL_LENS_ASPECT.1))
        .collect();
    let nz = flow
        .ux
        .noise(seed.key("lens-warp"), SOIL_LENS_WARP_SIGMA_MM * s);
    let warp = nz.with(nz.z.iter().map(|v| SOIL_LENS_WARP_MM * s * v).collect());
    Lenses {
        centre: cs,
        axis,
        half_len,
        half_w,
        warp,
    }
}

/// The flow wraps the lenses, then the rim rule (REQ-FL-4/5).
pub fn finish_flow(
    seed: Seed,
    rng: &mut Rng,
    flow: &Flow,
    lenses: &Lenses,
    w: f64,
    h: f64,
    s: f64,
) -> Flow {
    let like = &flow.ux;
    let (nx, ny) = (like.nx, like.ny);
    let mut vx = flow.ux.z.clone();
    let mut vy = flow.uy.z.clone();
    for i in 0..lenses.centre.len() {
        let band = rng.uniform(DEFLECT_BAND_MM.0, DEFLECT_BAND_MM.1) * s;
        let strength = rng.uniform(DEFLECT_STRENGTH.0, DEFLECT_STRENGTH.1);
        let pp = lenses.perp(i);
        vx.par_chunks_mut(nx)
            .zip(vy.par_chunks_mut(nx))
            .enumerate()
            .for_each(|(j, (rx, ry))| {
                let y = like.y(j);
                for (ii, (ax, ay)) in rx.iter_mut().zip(ry.iter_mut()).enumerate() {
                    let (d, b, gx, gy) = lenses.edge(i, like.x(ii), y);
                    let nn = libm::hypot(gx, gy).max(1e-9);
                    let (gx, gy) = (gx / nn, gy / nn);
                    let wgt = strength * (1.0 - s01(d / band));
                    let inward = (*ax * gx + *ay * gy).min(0.0);
                    *ax -= wgt * inward * gx;
                    *ay -= wgt * inward * gy;
                    // parting at the upstream tip: sideways by the side we are on
                    let side = libm::tanh(b / (PARTING_SOFT_MM * s));
                    *ax -= PARTING_PUSH * wgt * inward * side * pp[0];
                    *ay -= PARTING_PUSH * wgt * inward * side * pp[1];
                    let n = libm::hypot(*ax, *ay).max(1e-9);
                    *ax /= n;
                    *ay /= n;
                }
            });
    }
    let th = like.with(
        vy.iter()
            .zip(&vx)
            .map(|(y, x)| libm::atan2(*y, *x))
            .collect(),
    );
    let th = steer_rim(seed, &th, w, h, s);
    let _ = ny;
    Flow::new(
        &th.z.iter().map(|a| libm::cos(*a)).collect::<Vec<_>>(),
        &th.z.iter().map(|a| libm::sin(*a)).collect::<Vec<_>>(),
        like,
    )
}

/// A 1-D Gaussian filter of `sigma` samples with wrap-around edges.
fn gauss1d_wrap(v: &[f64], sigma: f64) -> Vec<f64> {
    let k = kernel(sigma);
    let r = k.len() / 2;
    let n = v.len() as i64;
    (0..v.len())
        .map(|i| {
            k.iter()
                .enumerate()
                .map(|(t, w)| w * v[(i as i64 + t as i64 - r as i64).rem_euclid(n) as usize])
                .sum()
        })
        .collect()
}

/// Rotation compressing and pulling `phi`, inward headings turned to side
/// `sg`. Continuous in `phi` except at the unwrap cut, which the caller
/// masks (its weight is 0 there).
fn turn(phi: f64, sg: f64, w: f64, wp: f64, ok: f64, pull: f64) -> f64 {
    let p = if phi.abs() > PI / 2.0 && phi.signum() != sg {
        phi + 2.0 * PI * sg
    } else {
        phi
    };
    let a0 = p.abs();
    let a = if a0 > ok {
        ok + (a0 - ok) * (1.0 - w)
    } else {
        a0
    };
    let outward = s01((PI / 2.0 + PULL_SOFT_RAD - a0) / (2.0 * PULL_SOFT_RAD));
    let a = a * (1.0 - pull * wp * outward);
    (if p >= 0.0 { 1.0 } else { -1.0 }) * a - p
}

/// Rim rule on the field (REQ-EDGE-7/8): at or along the edge, never away.
fn steer_rim(seed: Seed, th: &Lat, w: f64, h: f64, s: f64) -> Lat {
    let (nx, ny, res) = (th.nx, th.ny, th.res);
    let nmap = |name: &str, lo: f64, hi: f64| -> Vec<f64> {
        th.noise(seed.key(name), RIM_NOISE_SIGMA_MM * s)
            .z
            .iter()
            .map(|n| lo + (hi - lo) * s01(0.5 + 0.35 * n))
            .collect()
    };
    let flow_mm: Vec<f64> = nmap("rim-flow", EDGE_FLOW_MM.0 * s, EDGE_FLOW_MM.1 * s);
    let ok: Vec<f64> = nmap("rim-ok", EDGE_OK_DEG.0, EDGE_OK_DEG.1)
        .iter()
        .map(|d| d.to_radians())
        .collect();
    let pull: Vec<f64> = nmap("rim-pull", EDGE_PULL.0, EDGE_PULL.1);
    let in_tile = |i: usize, j: usize| {
        let (x, y) = (th.x(i), th.y(j));
        (0.0..=w).contains(&x) && (0.0..=h).contains(&y)
    };
    let ix: Vec<usize> = (0..nx).filter(|&i| (0.0..=w).contains(&th.x(i))).collect();
    let iy: Vec<usize> = (0..ny).filter(|&j| (0.0..=h).contains(&th.y(j))).collect();
    // edges CCW: distance, outward normal, along axis (0 = x, 1 = y), in-tile
    // along indices in the CCW direction
    let rev = |v: &[usize]| v.iter().rev().copied().collect::<Vec<_>>();
    let edges: [(Box<dyn Fn(f64, f64) -> f64>, f64, usize, Vec<usize>); 4] = [
        (Box::new(|_x, y| y), -PI / 2.0, 0, ix.clone()),
        (Box::new(move |x, _y| w - x), 0.0, 1, iy.clone()),
        (Box::new(move |_x, y| h - y), PI / 2.0, 0, rev(&ix)),
        (Box::new(|x, _y| x), PI, 1, rev(&iy)),
    ];
    let mut prof: Vec<Vec<f64>> = Vec::new();
    let mut cache: Vec<(Vec<f64>, Vec<f64>)> = Vec::new(); // (d, phi) per node
    for (dist, out, along, idx) in &edges {
        let mut d = vec![0.0; nx * ny];
        let mut phi = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let k = j * nx + i;
                d[k] = dist(th.x(i), th.y(j)).max(0.0);
                phi[k] = wrap(th.z[k] - out);
            }
        }
        // band-weighted mean sin φ along the edge
        let n_al = if *along == 0 { nx } else { ny };
        let mut num = vec![0.0; n_al];
        let mut den = vec![0.0; n_al];
        for j in 0..ny {
            for i in 0..nx {
                if !in_tile(i, j) {
                    continue;
                }
                let k = j * nx + i;
                let wt = s01(1.0 - d[k] / flow_mm[k]);
                let a = if *along == 0 { i } else { j };
                num[a] += wt * libm::sin(phi[k]);
                den[a] += wt;
            }
        }
        prof.push(idx.iter().map(|&a| num[a] / den[a].max(1e-9)).collect());
        cache.push((d, phi));
    }
    let all: Vec<f64> = prof.concat();
    let m = gauss1d_wrap(&all, STEER_SIDE_SIGMA_MM * s / res);
    let sg: Vec<f64> = m
        .iter()
        .map(|v| if *v >= 0.0 { 1.0 } else { -1.0 })
        .collect();
    let hard = gauss1d_wrap(&sg, STEER_SIDE_HARD_MM * s / res);
    let soft = gauss1d_wrap(&sg, STEER_SIDE_SOFT_MM * s / res);
    let mut deltas: Vec<Vec<f64>> = Vec::new();
    let mut dists: Vec<Vec<f64>> = Vec::new();
    let mut k0 = 0;
    for ((_, _, along, idx), (d, phi)) in edges.iter().zip(&cache) {
        let k1 = k0 + idx.len();
        let n_al = if *along == 0 { nx } else { ny };
        let mut full_h = vec![0.0; n_al];
        let mut full_s = vec![0.0; n_al];
        for (t, &a) in idx.iter().enumerate() {
            full_h[a] = hard[k0 + t];
            full_s[a] = soft[k0 + t];
        }
        let (lo, hi) = (
            *idx.iter().min().unwrap_or(&0),
            *idx.iter().max().unwrap_or(&0),
        );
        for a in 0..lo {
            full_h[a] = full_h[lo];
            full_s[a] = full_s[lo];
        }
        for a in hi + 1..n_al {
            full_h[a] = full_h[hi];
            full_s[a] = full_s[hi];
        }
        k0 = k1;
        let mut r = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let k = j * nx + i;
                let a = if *along == 0 { i } else { j };
                let (s_hard, sv) = (full_h[a], full_s[a]);
                let (dd, ph) = (d[k], phi[k]);
                let wp = s01(1.0 - dd / (flow_mm[k] * (1.0 + RIM_SQUARE_REACH * (1.0 - sv.abs()))));
                let wgt = s01(1.0 - dd / flow_mm[k]);
                let nat_r = STEER_NATURAL_DEG.to_radians();
                let nat = s01((PI / 2.0 + nat_r - ph.abs()) / nat_r);
                let lam =
                    0.5 * (1.0 + (1.0 - nat) * s_hard + nat * (if ph >= 0.0 { 1.0 } else { -1.0 }));
                let pl = pull[k].max(RIM_SQUARE_PULL * (1.0 - sv.abs()));
                let mut rr = lam * turn(ph, 1.0, wgt, wp, ok[k], pl)
                    + (1.0 - lam) * turn(ph, -1.0, wgt, wp, ok[k], pl);
                let q = wrap(ph + rr);
                let wa = s01(1.0 - dd / (RIM_ALONG_MM * s)) * sv.abs();
                let sgn = if sv == 0.0 { 0.0 } else { sv.signum() };
                rr += (sgn * ok[k] - q) * wa;
                r[k] = rr;
            }
        }
        deltas.push(r);
        dists.push(d.clone());
    }
    // corners: a smooth nearest-edge partition of unity
    let mut delta = vec![0.0; nx * ny];
    for k in 0..nx * ny {
        let dmin = dists.iter().map(|d| d[k]).fold(f64::INFINITY, f64::min);
        let (mut num, mut den) = (0.0, 0.0);
        for (r, d) in deltas.iter().zip(&dists) {
            let p = libm::exp(-(d[k] - dmin) / (STEER_CORNER_MM * s));
            num += r[k] * p;
            den += p;
        }
        delta[k] = num / den;
    }
    let delta = th.with(delta).blur(STEER_SMOOTH_MM * s);
    th.with(th.z.iter().zip(&delta.z).map(|(a, b)| a + b).collect())
}
