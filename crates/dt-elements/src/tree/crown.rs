//! The crown: its envelope, the attractors on its
//! side surface, their Voronoi groups
//! and the outward group bulge.

use dt_kit::Rng;
use dt_kit::graph::round_half_even;

/// The crown silhouette's four numbers (`crown shape`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub top_pointiness: f64,
    pub top_curve: f64,
    pub bottom_pointiness: f64,
    pub bottom_curve: f64,
}

/// `round`: the default envelope.
pub const ROUND: Shape = Shape {
    top_pointiness: 0.0,
    top_curve: 1.4,
    bottom_pointiness: 0.35,
    bottom_curve: 0.8,
};

/// An axisymmetric crown envelope (`CanopyEnvelope`).
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    pub cx: f64,
    pub cy: f64,
    pub ground: f64,
    pub height: f64,
    pub trunk: f64,
    pub radius: f64,
    pub base_radius: f64,
    pub shape: Shape,
    raw_base: f64,
    raw_peak: f64,
}

impl Envelope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cx: f64,
        cy: f64,
        ground: f64,
        height: f64,
        trunk: f64,
        radius: f64,
        base_radius: f64,
        shape: Shape,
    ) -> Self {
        let mut e = Self {
            cx,
            cy,
            ground,
            height,
            trunk: trunk.clamp(0.0, height),
            radius,
            base_radius,
            shape,
            raw_base: 0.0,
            raw_peak: 1.0,
        };
        e.raw_base = e.solve_base();
        e.raw_peak = e.peak_for(e.raw_base);
        e
    }

    pub fn base_z(&self) -> f64 {
        self.ground + self.trunk
    }

    pub fn crown_height(&self) -> f64 {
        (self.height - self.trunk).max(0.0)
    }

    /// Radius at normalised crown height `t`.
    pub fn radius_at_t(&self, t: f64) -> f64 {
        let c = t.clamp(0.0, 1.0);
        let raw = if c <= 0.0 {
            self.raw_base
        } else if c >= 1.0 {
            0.0
        } else {
            self.raw(c, self.raw_base)
        };
        if self.raw_peak <= 1e-12 {
            0.0
        } else {
            self.radius * raw / self.raw_peak
        }
    }

    pub fn radius_at_z(&self, z: f64) -> f64 {
        let h = self.crown_height();
        if h <= 1e-8 {
            return 0.0;
        }
        self.radius_at_t((z - self.base_z()) / h)
    }

    /// The outward unit normal of the crown surface at `p`.
    pub fn normal(&self, p: [f64; 3]) -> [f64; 3] {
        let dz = (self.crown_height() * 0.001).max(0.01);
        let drdz = (self.radius_at_z(p[2] + dz) - self.radius_at_z(p[2] - dz)) / (2.0 * dz);
        let (dx, dy) = (p[0] - self.cx, p[1] - self.cy);
        let r = libm::hypot(dx, dy);
        let (c, s) = if r > 1e-9 {
            (dx / r, dy / r)
        } else {
            (0.0, 0.0)
        };
        let n = libm::sqrt(c * c + s * s + drdz * drdz).max(1e-9);
        [c / n, s / n, -drdz / n]
    }

    fn raw(&self, t: f64, base: f64) -> f64 {
        let s = &self.shape;
        let bottom = base + (1.0 - base) * end_profile(t, s.bottom_pointiness, s.bottom_curve);
        let top = end_profile(1.0 - t, s.top_pointiness, s.top_curve);
        smooth_min(bottom, top, 0.08)
    }

    fn peak_for(&self, base: f64) -> f64 {
        (0..257)
            .map(|k| {
                let t = f64::from(k) / 256.0;
                if k == 0 {
                    base
                } else if k == 256 {
                    0.0
                } else {
                    self.raw(t, base)
                }
            })
            .fold(f64::NEG_INFINITY, f64::max)
    }

    fn solve_base(&self) -> f64 {
        if self.radius <= 1e-12 {
            return 0.0;
        }
        let target = (self.base_radius / self.radius).clamp(0.0, 1.0);
        if target <= 1e-12 {
            return 0.0;
        }
        let (mut lo, mut hi) = (0.0, (target * 1.25 + 0.25).min(1.0));
        for _ in 0..32 {
            let mid = 0.5 * (lo + hi);
            let peak = self.peak_for(mid);
            let ratio = if peak > 1e-12 { mid / peak } else { 0.0 };
            if ratio < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi
    }
}

fn end_profile(u: f64, pointiness: f64, curve: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
    let p = pointiness.clamp(0.0, 1.0);
    let c = curve.max(0.01);
    let linear = libm::pow(u, c);
    let arc = libm::sqrt((1.0 - (1.0 - u) * (1.0 - u)).clamp(0.0, 1.0));
    (1.0 - p) * arc + p * linear
}

fn smooth_min(a: f64, b: f64, k: f64) -> f64 {
    let k = k.max(1e-9);
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// `n` attractors over the side surface, weighted by
/// its area of revolution (never uniform in height: the apex club-top
/// artefact returns), on a jittered golden-angle spiral.
pub fn sample(env: &Envelope, rng: &mut Rng, n: usize) -> Vec<[f64; 3]> {
    let h = env.crown_height();
    if h <= 1e-8 || env.radius <= 1e-8 || n == 0 {
        return Vec::new();
    }
    let m = 257.max(n * 8);
    #[allow(clippy::cast_precision_loss)]
    let ts: Vec<f64> = (0..m).map(|k| k as f64 / (m - 1) as f64).collect();
    let zs: Vec<f64> = ts.iter().map(|t| env.base_z() + t * h).collect();
    let rs: Vec<f64> = ts.iter().map(|&t| env.radius_at_t(t)).collect();
    let dr = gradient(&rs, &zs);
    let dens: Vec<f64> = (0..m)
        .map(|k| {
            let d = 2.0 * std::f64::consts::PI * rs[k] * libm::sqrt(1.0 + dr[k] * dr[k]);
            if d.is_finite() { d.max(0.0) } else { 0.0 }
        })
        .collect();
    let mut cum = vec![0.0; m];
    for k in 1..m {
        cum[k] = cum[k - 1] + 0.5 * (dens[k - 1] + dens[k]) * (zs[k] - zs[k - 1]);
    }
    let total = cum[m - 1];
    if total <= 1e-9 {
        return Vec::new();
    }
    let golden = std::f64::consts::PI * (3.0 - libm::sqrt(5.0));
    let jitter: Vec<f64> = (0..n).map(|_| rng.random()).collect();
    let phase = rng.uniform(0.0, std::f64::consts::TAU);
    let twist: Vec<f64> = (0..n).map(|_| rng.uniform(-0.5, 0.5)).collect();
    #[allow(clippy::cast_precision_loss)]
    (0..n)
        .map(|i| {
            let a = (i as f64 + jitter[i]) / n as f64 * total;
            let z = interp(a, &cum, &zs);
            let r = env.radius_at_z(z);
            let th = i as f64 * golden + phase + twist[i] * golden;
            [env.cx + r * libm::cos(th), env.cy + r * libm::sin(th), z]
        })
        .collect()
}

/// numpy's `gradient` with second-order edges.
fn gradient(y: &[f64], x: &[f64]) -> Vec<f64> {
    let n = y.len();
    let mut g = vec![0.0; n];
    for k in 1..n - 1 {
        let (h1, h2) = (x[k] - x[k - 1], x[k + 1] - x[k]);
        g[k] = (h1 * h1 * y[k + 1] - h2 * h2 * y[k - 1] + (h2 * h2 - h1 * h1) * y[k])
            / (h1 * h2 * (h1 + h2));
    }
    let (h1, h2) = (x[1] - x[0], x[2] - x[1]);
    g[0] = -(2.0 * h1 + h2) / (h1 * (h1 + h2)) * y[0] + (h1 + h2) / (h1 * h2) * y[1]
        - h1 / (h2 * (h1 + h2)) * y[2];
    let (h1, h2) = (x[n - 2] - x[n - 3], x[n - 1] - x[n - 2]);
    g[n - 1] = h2 / (h1 * (h1 + h2)) * y[n - 3] - (h1 + h2) / (h1 * h2) * y[n - 2]
        + (2.0 * h2 + h1) / (h2 * (h1 + h2)) * y[n - 1];
    g
}

/// numpy's `interp` (clamped at the ends).
fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    if x <= xp[0] {
        return fp[0];
    }
    let n = xp.len();
    if x >= xp[n - 1] {
        return fp[n - 1];
    }
    let k = xp.partition_point(|&v| v <= x).max(1) - 1;
    let d = xp[k + 1] - xp[k];
    if d <= 0.0 {
        fp[k]
    } else {
        fp[k] + (x - xp[k]) / d * (fp[k + 1] - fp[k])
    }
}

/// How many attractor groups the crown is split into.
pub fn n_groups(env: &Envelope, width: f64, height: f64) -> usize {
    if width <= 0.0 || height <= 0.0 {
        return 1;
    }
    let around = round_half_even(2.0 * std::f64::consts::PI * env.radius / width).max(1.0);
    let tall = round_half_even(env.crown_height() / height).max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (around * tall) as usize;
    n.max(2)
}

/// K-means (Lloyd, 20 rounds) in a z-scaled
/// space, seeded from `k` distinct attractors.
pub fn groups(pts: &[[f64; 3]], k: usize, rng: &mut Rng, z_scale: f64) -> Vec<usize> {
    let n = pts.len();
    if n == 0 || k <= 1 {
        return vec![0; n];
    }
    let k = k.min(n);
    let scaled: Vec<[f64; 3]> = pts.iter().map(|p| [p[0], p[1], p[2] * z_scale]).collect();
    // k distinct seeds (a partial Fisher–Yates shuffle)
    let mut idx: Vec<usize> = (0..n).collect();
    for i in 0..k {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_possible_wrap
        )]
        let j = rng.int(i as i64, (n - 1) as i64) as usize;
        idx.swap(i, j);
    }
    let mut seeds: Vec<[f64; 3]> = idx[..k].iter().map(|&i| scaled[i]).collect();
    let mut labels = vec![0; n];
    for _ in 0..20 {
        for (l, p) in labels.iter_mut().zip(&scaled) {
            let d2 = |s: &[f64; 3]| (0..3).map(|a| (p[a] - s[a]) * (p[a] - s[a])).sum::<f64>();
            *l = (1..k).fold(0, |b, j| if d2(&seeds[j]) < d2(&seeds[b]) { j } else { b });
        }
        let mut sum = vec![[0.0; 3]; k];
        let mut cnt = vec![0usize; k];
        for (l, p) in labels.iter().zip(&scaled) {
            cnt[*l] += 1;
            for a in 0..3 {
                sum[*l][a] += p[a];
            }
        }
        let mut moved = false;
        for j in 0..k {
            if cnt[j] > 0 {
                #[allow(clippy::cast_precision_loss)]
                let c = sum[j].map(|v| v / cnt[j] as f64);
                if (0..3).any(|a| (c[a] - seeds[j][a]).abs() > 1e-6) {
                    moved = true;
                }
                seeds[j] = c;
            }
        }
        if !moved {
            break;
        }
    }
    labels
}

/// Each group pushed out along the crown normal in a
/// dome, `bulge` at its middle and nothing at its edge.
pub fn bulge(pts: &[[f64; 3]], labels: &[usize], env: &Envelope, bulge: f64) -> Vec<[f64; 3]> {
    let mut gids = labels.to_vec();
    gids.sort_unstable();
    gids.dedup();
    if gids.len() <= 1 {
        return pts.to_vec();
    }
    let mut out = pts.to_vec();
    for g in gids {
        let inside: Vec<usize> = (0..pts.len()).filter(|&i| labels[i] == g).collect();
        let other: Vec<usize> = (0..pts.len()).filter(|&i| labels[i] != g).collect();
        if inside.is_empty() || other.is_empty() {
            continue;
        }
        let edge: Vec<f64> = inside
            .iter()
            .map(|&i| {
                other
                    .iter()
                    .map(|&j| dt_kit::graph::dist(pts[i], pts[j]))
                    .fold(f64::INFINITY, f64::min)
            })
            .collect();
        let dmax = edge.iter().copied().fold(0.0, f64::max);
        if dmax < 1e-9 {
            continue;
        }
        for (k, &i) in inside.iter().enumerate() {
            let t = edge[k] / dmax;
            let dome = libm::sqrt((2.0 * t - t * t).clamp(0.0, 1.0));
            let n = env.normal(pts[i]);
            for a in 0..3 {
                out[i][a] += n[a] * dome * bulge;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_envelope_meets_its_radii() {
        let e = Envelope::new(0.0, 0.0, 5.0, 40.0, 30.0, 15.0, 14.0, ROUND);
        assert!((e.radius_at_t(0.0) - 14.0).abs() < 0.01);
        let peak = (0..=100)
            .map(|k| e.radius_at_t(f64::from(k) / 100.0))
            .fold(0.0, f64::max);
        assert!((peak - 15.0).abs() < 0.05, "{peak}");
        assert!(e.radius_at_t(1.0).abs() < 1e-12);
    }

    #[test]
    fn attractors_lie_on_the_side_surface() {
        let e = Envelope::new(10.0, 10.0, 5.0, 35.0, 30.0, 12.0, 11.0, ROUND);
        let mut rng = Rng::new(dt_kit::Seed::new(4));
        let pts = sample(&e, &mut rng, 32);
        assert_eq!(pts.len(), 32);
        for p in pts {
            let r = libm::hypot(p[0] - 10.0, p[1] - 10.0);
            assert!((r - e.radius_at_z(p[2])).abs() < 1e-9);
            assert!(p[2] >= 35.0 - 1e-9 && p[2] <= 40.0 + 1e-9);
        }
    }
}
