//! Field filters on the grid convention of [`crate::grid`]: values on an
//! `nx` by `ny` node lattice, row-major (`j` outer), as `f64`.
//!
//! - [`blur`]: separable Gaussian, scipy's `gaussian_filter` with
//!   `mode='reflect'` and `truncate=4`, so the blurs match scipy exactly;
//! - [`blur_recursive`]: the Young–van Vliet recursive Gaussian, constant
//!   cost per node whatever the sigma (for wide blurs);
//! - [`dilate`] and [`erode`]: grey morphology over a disc or a square;
//! - [`quantile`]: numpy's default (linear) quantile.
//!
//! Every output node is computed by one fixed sequence of operations, so
//! results are identical at any thread count. Seeded maths uses `libm`.

use rayon::prelude::*;

/// scipy's `reflect` index: `d c b a | a b c d | d c b a`.
fn reflect(k: i64, n: usize) -> usize {
    #[allow(clippy::cast_possible_wrap)]
    let n = n as i64;
    let period = 2 * n;
    let mut m = k.rem_euclid(period);
    if m >= n {
        m = period - 1 - m;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let m = m as usize;
    m
}

/// The normalised Gaussian kernel of `sigma` nodes (scipy: radius
/// `int(4 sigma + 0.5)`).
fn kernel(sigma: f64) -> Vec<f64> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let r = (4.0 * sigma + 0.5) as usize;
    #[allow(clippy::cast_precision_loss)]
    let mut k: Vec<f64> = (0..=2 * r)
        .map(|i| {
            let x = i as f64 - r as f64;
            libm::exp(-0.5 * x * x / (sigma * sigma))
        })
        .collect();
    let s: f64 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= s);
    k
}

fn blur_lines(src: &[f64], dst: &mut [f64], len: usize, k: &[f64]) {
    let r = k.len() / 2;
    #[allow(clippy::cast_possible_wrap)]
    let ri = r as i64;
    dst.par_chunks_mut(len)
        .zip(src.par_chunks(len))
        .for_each(|(out, line)| {
            for (i, o) in out.iter_mut().enumerate() {
                let mut acc = 0.0;
                if i >= r && i + r < len {
                    for (w, v) in k.iter().zip(&line[i - r..=i + r]) {
                        acc += w * v;
                    }
                } else {
                    #[allow(clippy::cast_possible_wrap)]
                    let c = i as i64;
                    for (t, w) in k.iter().enumerate() {
                        #[allow(clippy::cast_possible_wrap)]
                        let at = c + t as i64 - ri;
                        acc += w * line[reflect(at, len)];
                    }
                }
                *o = acc;
            }
        });
}

fn transpose(src: &[f64], nx: usize, ny: usize) -> Vec<f64> {
    let mut t = vec![0.0; nx * ny];
    t.par_chunks_mut(ny).enumerate().for_each(|(i, col)| {
        for (j, c) in col.iter_mut().enumerate() {
            *c = src[j * nx + i];
        }
    });
    t
}

/// Separable Gaussian blur of `sigma` nodes (scipy `gaussian_filter`,
/// `mode='reflect'`, `truncate=4`). `sigma <= 0` returns the input.
#[must_use]
pub fn blur(v: &[f64], nx: usize, ny: usize, sigma: f64) -> Vec<f64> {
    if sigma <= 0.0 || nx == 0 || ny == 0 || sigma.is_nan() {
        return v.to_vec();
    }
    let k = kernel(sigma);
    let mut a = vec![0.0; nx * ny];
    blur_lines(v, &mut a, nx, &k);
    let t = transpose(&a, nx, ny);
    let mut b = vec![0.0; nx * ny];
    blur_lines(&t, &mut b, ny, &k);
    transpose(&b, ny, nx)
}

/// Young and van Vliet's recursive Gaussian (1995, with Young, van Vliet
/// and van Ginkel's 2002 coefficients): a forward and a backward
/// third-order pass per axis, so the cost per node is constant. Ends are
/// padded by reflection over `4 sigma`, as [`blur`] treats them.
#[must_use]
pub fn blur_recursive(v: &[f64], nx: usize, ny: usize, sigma: f64) -> Vec<f64> {
    if sigma <= 0.0 || nx == 0 || ny == 0 || sigma.is_nan() {
        return v.to_vec();
    }
    let q = if sigma >= 2.5 {
        0.98711 * sigma - 0.96330
    } else {
        3.97156 - 4.14554 * libm::sqrt(1.0 - 0.26891 * sigma)
    };
    let q2 = q * q;
    let q3 = q2 * q;
    let b0 = 1.57825 + 2.44413 * q + 1.4281 * q2 + 0.422_205 * q3;
    let b1 = 2.44413 * q + 2.85619 * q2 + 1.26661 * q3;
    let b2 = -1.4281 * q2 - 1.26661 * q3;
    let b3 = 0.422_205 * q3;
    let bb = 1.0 - (b1 + b2 + b3) / b0;
    let coef = (bb, b1 / b0, b2 / b0, b3 / b0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pad = (4.0 * sigma).ceil() as usize + 3;
    let pass = |src: &[f64], len: usize| -> Vec<f64> {
        let mut out = vec![0.0; src.len()];
        out.par_chunks_mut(len)
            .zip(src.par_chunks(len))
            .for_each(|(o, line)| recursive_line(line, o, pad, coef));
        out
    };
    let a = pass(v, nx);
    let t = transpose(&a, nx, ny);
    let b = pass(&t, ny);
    transpose(&b, ny, nx)
}

fn recursive_line(
    line: &[f64],
    out: &mut [f64],
    pad: usize,
    (b, b1, b2, b3): (f64, f64, f64, f64),
) {
    let len = line.len();
    #[allow(clippy::cast_possible_wrap)]
    let ext: Vec<f64> = (-(pad as i64)..(len + pad) as i64)
        .map(|k| line[reflect(k, len)])
        .collect();
    let n = ext.len();
    let mut w = vec![0.0; n];
    let (mut p1, mut p2, mut p3) = (ext[0], ext[0], ext[0]);
    for (wi, &x) in w.iter_mut().zip(&ext) {
        let y = b * x + b1 * p1 + b2 * p2 + b3 * p3;
        *wi = y;
        (p3, p2, p1) = (p2, p1, y);
    }
    let last = w[n - 1];
    let (mut p1, mut p2, mut p3) = (last, last, last);
    for k in (0..n).rev() {
        let y = b * w[k] + b1 * p1 + b2 * p2 + b3 * p3;
        w[k] = y;
        (p3, p2, p1) = (p2, p1, y);
    }
    out.copy_from_slice(&w[pad..pad + len]);
}

/// A structuring element for [`dilate`] and [`erode`], in nodes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Structuring {
    /// Every node within this distance.
    Disc(f64),
    /// A `2r + 1` square.
    Square(usize),
}

impl Structuring {
    /// Half-width of each row of the footprint, from `-r` to `r`.
    fn chords(self) -> Vec<usize> {
        match self {
            Self::Square(r) => vec![r; 2 * r + 1],
            Self::Disc(rad) => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let r = rad.max(0.0).floor() as usize;
                #[allow(clippy::cast_precision_loss)]
                (0..=2 * r)
                    .map(|k| {
                        let dy = k as f64 - r as f64;
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let w = libm::sqrt((rad * rad - dy * dy).max(0.0)).floor() as usize;
                        w
                    })
                    .collect()
            }
        }
    }
}

/// Running max (`max = true`) or min of every window `[i - w, i + w]`
/// (clipped to the line), by van Herk and Gil–Werman: O(1) per node.
fn running(line: &[f64], w: usize, max: bool, out: &mut [f64]) {
    let n = line.len();
    let pick = |a: f64, b: f64| if max { a.max(b) } else { a.min(b) };
    if w == 0 {
        out.copy_from_slice(line);
        return;
    }
    let size = 2 * w + 1;
    let fill = if max {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    };
    // pad w on each side with the neutral value
    let padded: Vec<f64> = std::iter::repeat_n(fill, w)
        .chain(line.iter().copied())
        .chain(std::iter::repeat_n(fill, w + size))
        .collect();
    let m = padded.len();
    let mut g = vec![fill; m];
    let mut h = vec![fill; m];
    for (k, &v) in padded.iter().enumerate() {
        g[k] = if k % size == 0 { v } else { pick(g[k - 1], v) };
    }
    for k in (0..m).rev() {
        h[k] = if k % size == size - 1 || k == m - 1 {
            padded[k]
        } else {
            pick(h[k + 1], padded[k])
        };
    }
    for (i, o) in out.iter_mut().enumerate().take(n) {
        // window [i, i + size - 1] in padded coordinates
        *o = pick(h[i], g[i + size - 1]);
    }
}

fn morph(v: &[f64], nx: usize, ny: usize, fp: Structuring, max: bool) -> Vec<f64> {
    let chords = fp.chords();
    let r = chords.len() / 2;
    let mut widths: Vec<usize> = chords.clone();
    widths.sort_unstable();
    widths.dedup();
    // each distinct chord width: the running extreme of every row
    let runs: Vec<Vec<f64>> = widths
        .iter()
        .map(|&w| {
            let mut out = vec![0.0; nx * ny];
            out.par_chunks_mut(nx)
                .zip(v.par_chunks(nx))
                .for_each(|(o, line)| running(line, w, max, o));
            out
        })
        .collect();
    let which: Vec<usize> = chords
        .iter()
        .map(|w| widths.iter().position(|x| x == w).unwrap_or(0))
        .collect();
    let mut out = vec![0.0; nx * ny];
    out.par_chunks_mut(nx).enumerate().for_each(|(j, row)| {
        let pick = |a: f64, b: f64| if max { a.max(b) } else { a.min(b) };
        row.iter_mut().for_each(|o| {
            *o = if max {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        });
        for (k, &wk) in which.iter().enumerate() {
            #[allow(clippy::cast_possible_wrap)]
            let jj = j as i64 + k as i64 - r as i64;
            if jj < 0 || jj >= ny as i64 {
                continue;
            }
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let src = &runs[wk][jj as usize * nx..(jj as usize + 1) * nx];
            for (o, s) in row.iter_mut().zip(src) {
                *o = pick(*o, *s);
            }
        }
    });
    out
}

/// Grey dilation: every node takes the largest value under the footprint
/// (nodes outside the lattice are ignored).
#[must_use]
pub fn dilate(v: &[f64], nx: usize, ny: usize, fp: Structuring) -> Vec<f64> {
    morph(v, nx, ny, fp, true)
}

/// Grey erosion: every node takes the smallest value under the footprint.
#[must_use]
pub fn erode(v: &[f64], nx: usize, ny: usize, fp: Structuring) -> Vec<f64> {
    morph(v, nx, ny, fp, false)
}

/// numpy's default quantile (`method='linear'`) of `v`, `q` in [0, 1];
/// NaN for an empty slice.
#[must_use]
pub fn quantile(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_unstable_by(f64::total_cmp);
    #[allow(clippy::cast_precision_loss)]
    let pos = q.clamp(0.0, 1.0) * (s.len() - 1) as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(s.len() - 1);
    #[allow(clippy::cast_precision_loss)]
    let t = pos - lo as f64;
    s[lo] + (s[hi] - s[lo]) * t
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp, clippy::cast_precision_loss)]
    use super::*;

    #[test]
    fn reflect_matches_scipy() {
        let got: Vec<usize> = (-4..8).map(|k| reflect(k, 4)).collect();
        assert_eq!(got, vec![3, 2, 1, 0, 0, 1, 2, 3, 3, 2, 1, 0]);
    }

    #[test]
    fn blur_keeps_constants_and_mass() {
        let v = vec![2.5; 40 * 30];
        assert!(
            blur(&v, 40, 30, 3.0)
                .iter()
                .all(|x| (x - 2.5).abs() < 1e-12)
        );
        let mut d = vec![0.0; 41 * 41];
        d[20 * 41 + 20] = 1.0;
        let b = blur(&d, 41, 41, 2.0);
        assert!((b.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        // separable Gaussian: value at (dx, dy) ∝ exp(-(dx²+dy²)/2σ²)
        let ratio = b[20 * 41 + 22] / b[20 * 41 + 20];
        assert!((ratio - (-0.5f64).exp()).abs() < 1e-3, "{ratio}");
    }

    #[test]
    fn scipy_values() {
        // scipy.ndimage.gaussian_filter1d([0,0,1,0,0,0,0,0], 1.0, mode='reflect')
        let v = [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let b = blur(&v, 8, 1, 1.0);
        let want = [
            0.058_422_989_040_7,
            0.242_105_276_281_2,
            0.398_943_469_356_1,
            0.241_971_445_656_6,
            0.053_991_127_420_7,
            0.004_431_861_620_0,
            0.000_133_830_624_6,
            0.0,
        ];
        // a 1-row grid is reflected in y too, which keeps it unchanged
        for (a, w) in b.iter().zip(want) {
            assert!((a - w).abs() < 1e-12, "{b:?}");
        }
    }

    #[test]
    fn recursive_is_close_to_the_kernel() {
        let (nx, ny) = (120, 90);
        let v: Vec<f64> = (0..nx * ny)
            .map(|k| ((k * 7919) % 101) as f64 / 101.0)
            .collect();
        for sigma in [1.5, 4.0, 12.0] {
            let a = blur(&v, nx, ny, sigma);
            let b = blur_recursive(&v, nx, ny, sigma);
            let err = a
                .iter()
                .zip(&b)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0, f64::max);
            assert!(err < 0.02, "sigma {sigma}: {err}");
        }
    }

    #[test]
    fn morphology_matches_brute_force() {
        let (nx, ny) = (23, 17);
        let v: Vec<f64> = (0..nx * ny).map(|k| ((k * 37) % 29) as f64).collect();
        for fp in [
            Structuring::Disc(3.2),
            Structuring::Square(2),
            Structuring::Disc(0.0),
        ] {
            let d = dilate(&v, nx, ny, fp);
            let e = erode(&v, nx, ny, fp);
            for j in 0..ny {
                for i in 0..nx {
                    let (mut mx, mut mn) = (f64::NEG_INFINITY, f64::INFINITY);
                    for jj in 0..ny {
                        for ii in 0..nx {
                            let (dx, dy) = (ii as f64 - i as f64, jj as f64 - j as f64);
                            let inside = match fp {
                                Structuring::Disc(r) => {
                                    dy.abs() <= r.floor()
                                        && dx.abs() <= (r * r - dy * dy).max(0.0).sqrt().floor()
                                }
                                Structuring::Square(r) => {
                                    dx.abs() <= r as f64 && dy.abs() <= r as f64
                                }
                            };
                            if inside {
                                mx = mx.max(v[jj * nx + ii]);
                                mn = mn.min(v[jj * nx + ii]);
                            }
                        }
                    }
                    assert_eq!(d[j * nx + i], mx, "{fp:?} ({i},{j})");
                    assert_eq!(e[j * nx + i], mn, "{fp:?} ({i},{j})");
                }
            }
        }
    }

    #[test]
    fn quantile_is_numpys() {
        let v = [5.0, 1.0, 3.0, 2.0, 4.0];
        assert_eq!(quantile(&v, 0.0), 1.0);
        assert_eq!(quantile(&v, 1.0), 5.0);
        assert_eq!(quantile(&v, 0.35), 2.4);
        assert!(quantile(&[], 0.5).is_nan());
    }
}
