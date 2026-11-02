//! Position-keyed noise on one lattice.
//!
//! Every noise here is a function of position and seed only: the value at
//! a point does not depend on the grid it is sampled on, its resolution
//! or the order of evaluation. A 35 mm build and a 25.4 mm build (sampled
//! at positions divided by the scale), or a preview and a final, see the
//! same pattern (plan.html, P2; architecture.html, "noise").
//!
//! One lattice: [`lattice`] gives a value in [-1, 1) to each integer
//! point `(ix, iy)` for a seed. Two noises are built on it:
//!
//! - [`smooth`]: the lattice's values, `h` mm apart, as white noise
//!   convolved with a Gaussian of `sigma` mm and normalised to unit
//!   variance. This is the position-keyed form of dirtlab's
//!   `noise(rng, shape, sigma_px)` (a blurred normal field divided by its
//!   standard deviation). Separable, so a grid costs two passes.
//! - [`fbm`]: smoothstep-interpolated value noise summed over octaves, a
//!   port of `dharmatiles.stone.noise.fbm` in 2D (z = 0).
//!
//! Seeded maths uses `libm` (R2: platform `exp` differs by OS).

/// The lattice value at integer point `(ix, iy)`, in [-1, 1).
#[must_use]
pub fn lattice(seed: u64, ix: i64, iy: i64) -> f64 {
    #[allow(clippy::cast_sign_loss)]
    let (a, b) = (ix as u64, iy as u64);
    let h = mix(seed ^ mix(a ^ mix(b ^ 0x6c61_7474_6963_6521))); // "lattice!"
    // 53 random bits → [0, 1) → [-1, 1)
    #[allow(clippy::cast_precision_loss)]
    let u = (h >> 11) as f64 * (1.0 / (1u64 << 53) as f64);
    2.0 * u - 1.0
}

/// SplitMix64's finaliser.
const fn mix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Lattice spacing of [`smooth`], as a share of `sigma`.  Half a sigma
/// keeps the variance flat to about 0.1% between lattice points.
const SMOOTH_SPACING: f64 = 0.5;
/// Gaussian taps reach this many sigmas.
const SMOOTH_REACH: f64 = 4.0;

/// Smooth noise: Gaussian-filtered lattice white noise with standard
/// deviation `sigma`, normalised to unit variance, sampled at the points
/// `xs × ys` (keyed positions, row-major with `y` outer).
///
/// `sigma` is in the same units as the positions. The result depends
/// only on the seed and the positions.
#[must_use]
pub fn smooth(seed: u64, sigma: f64, xs: &[f64], ys: &[f64]) -> Vec<f64> {
    let (nx, ny) = (xs.len(), ys.len());
    if nx == 0 || ny == 0 || sigma.is_nan() || sigma <= 0.0 {
        return vec![0.0; nx * ny];
    }
    let h = SMOOTH_SPACING * sigma;
    let taps = |ps: &[f64]| -> (i64, i64, Vec<(i64, Vec<f64>)>) {
        // for each position: the first lattice index and its weights
        let mut lo = i64::MAX;
        let mut hi = i64::MIN;
        let w: Vec<(i64, Vec<f64>)> = ps
            .iter()
            .map(|&p| {
                let a = floor_i64((p - SMOOTH_REACH * sigma) / h);
                let b = ceil_i64((p + SMOOTH_REACH * sigma) / h);
                lo = lo.min(a);
                hi = hi.max(b);
                let ws = (a..=b)
                    .map(|k| {
                        #[allow(clippy::cast_precision_loss)]
                        let d = (p - k as f64 * h) / sigma;
                        libm::exp(-0.5 * d * d)
                    })
                    .collect();
                (a, ws)
            })
            .collect();
        (lo, hi, w)
    };
    let (_, _, wx) = taps(xs);
    let (ylo, yhi, wy) = taps(ys);
    let (xlo, xhi) = wx.iter().fold((i64::MAX, i64::MIN), |(lo, hi), (a, ws)| {
        #[allow(clippy::cast_possible_wrap)]
        let b = *a + ws.len() as i64 - 1;
        (lo.min(*a), hi.max(b))
    });
    // row pass: for each lattice row, the x-filtered values at every xs
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let rows = (yhi - ylo + 1) as usize;
    let mut a = vec![0.0; rows * nx];
    let mut white = Vec::new();
    for r in 0..rows {
        #[allow(clippy::cast_possible_wrap)]
        let iy = ylo + r as i64;
        white.clear();
        white.extend((xlo..=xhi).map(|ix| lattice(seed, ix, iy)));
        for (i, (first, ws)) in wx.iter().enumerate() {
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let off = (first - xlo) as usize;
            a[r * nx + i] = ws.iter().zip(&white[off..]).map(|(w, v)| w * v).sum();
        }
    }
    // column pass; normalise: the variance of a lattice-weighted sum of
    // uniform [-1, 1) values (variance 1/3) is (1/3) (sigma sqrt(pi) / h)^2
    let norm = 1.0 / ((1.0f64 / 3.0).sqrt() * sigma * core::f64::consts::PI.sqrt() / h);
    let mut out = vec![0.0; nx * ny];
    for (j, (first, ws)) in wy.iter().enumerate() {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let off = (first - ylo) as usize;
        let row = &mut out[j * nx..(j + 1) * nx];
        for (k, w) in ws.iter().enumerate() {
            let src = &a[(off + k) * nx..(off + k + 1) * nx];
            for (o, v) in row.iter_mut().zip(src) {
                *o += w * v;
            }
        }
        row.iter_mut().for_each(|o| *o *= norm);
    }
    out
}

fn floor_i64(v: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let f = libm::floor(v) as i64;
    f
}

fn ceil_i64(v: f64) -> i64 {
    #[allow(clippy::cast_possible_truncation)]
    let f = libm::ceil(v) as i64;
    f
}

/// Smoothstep-interpolated value noise at `scale`, in about [-1, 1].
#[must_use]
pub fn value(seed: u64, scale: f64, x: f64, y: f64) -> f64 {
    let (qx, qy) = (x / scale, y / scale);
    let (fx, fy) = (libm::floor(qx), libm::floor(qy));
    let (ix, iy) = (floor_i64(qx), floor_i64(qy));
    let (tx, ty) = (qx - fx, qy - fy);
    let (ux, uy) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let c00 = lattice(seed, ix, iy);
    let c10 = lattice(seed, ix + 1, iy);
    let c01 = lattice(seed, ix, iy + 1);
    let c11 = lattice(seed, ix + 1, iy + 1);
    let x0 = c00 + (c10 - c00) * ux;
    let x1 = c01 + (c11 - c01) * ux;
    x0 + (x1 - x0) * uy
}

/// Fractal sum of [`value`] noise: `octaves` octaves from `scale` down,
/// each half the size and half the weight, normalised to about [-1, 1]
/// (`dharmatiles.stone.noise.fbm` with gain 0.5 and lacunarity 2).
#[must_use]
pub fn fbm(seed: u64, scale: f64, octaves: u32, x: f64, y: f64) -> f64 {
    let (mut total, mut amp, mut sc, mut norm) = (0.0, 1.0, scale, 0.0);
    for o in 0..octaves {
        total += amp * value(mix(seed ^ u64::from(o)), sc, x, y);
        norm += amp;
        amp *= 0.5;
        sc *= 0.5;
    }
    if norm > 0.0 { total / norm } else { 0.0 }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::cast_precision_loss)]
    use super::*;

    fn stats(v: &[f64]) -> (f64, f64) {
        let n = v.len() as f64;
        let m = v.iter().sum::<f64>() / n;
        let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / n;
        (m, var.sqrt())
    }

    #[test]
    fn lattice_is_keyed_and_spread() {
        assert_eq!(lattice(7, 3, -4), lattice(7, 3, -4));
        assert_ne!(lattice(7, 3, -4), lattice(8, 3, -4));
        assert_ne!(lattice(7, 3, -4), lattice(7, -4, 3));
        let v: Vec<f64> = (0..10_000).map(|k| lattice(1, k % 100, k / 100)).collect();
        let (m, sd) = stats(&v);
        assert!(m.abs() < 0.02 && (sd - (1.0f64 / 3.0).sqrt()).abs() < 0.01);
        assert!(v.iter().all(|x| (-1.0..1.0).contains(x)));
    }

    #[test]
    fn smooth_is_unit_variance_and_resolution_independent() {
        let xs: Vec<f64> = (0..300).map(|i| f64::from(i) * 0.5).collect();
        let v = smooth(3, 4.0, &xs, &xs);
        let (m, sd) = stats(&v);
        assert!(m.abs() < 0.3 && (sd - 1.0).abs() < 0.2, "mean {m} sd {sd}");
        // the same points sampled within a coarser grid: same values
        let coarse: Vec<f64> = (0..150).map(f64::from).collect();
        let c = smooth(3, 4.0, &coarse, &coarse);
        for j in 0..150 {
            for i in 0..150 {
                let a = c[j * 150 + i];
                let b = v[2 * j * 300 + 2 * i];
                assert!((a - b).abs() < 1e-12, "({i},{j}) {a} {b}");
            }
        }
        // and on a window that starts elsewhere
        let w = smooth(3, 4.0, &xs[100..120], &xs[50..60]);
        for j in 0..10 {
            for i in 0..20 {
                assert!((w[j * 20 + i] - v[(50 + j) * 300 + 100 + i]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn fbm_is_bounded_and_continuous() {
        let mut prev = fbm(5, 1.6, 3, 0.0, 0.3);
        for k in 1..2000 {
            let x = f64::from(k) * 0.01;
            let v = fbm(5, 1.6, 3, x, 0.3);
            assert!(v.abs() <= 1.0);
            assert!((v - prev).abs() < 0.1, "jump at {x}");
            prev = v;
        }
        assert_ne!(fbm(5, 1.6, 3, 1.0, 2.0), fbm(6, 1.6, 3, 1.0, 2.0));
    }
}
