//! A lattice of values that reaches past the tile.
//! the flow, the swaths and the clearings live on it, so streamlines can
//! start outside the tile and run in.

use dt_kit::{Seed, noise_at};
use rayon::prelude::*;

/// Values on a regular lattice with origin `(x0, y0)` and spacing `res`
/// (mm), row-major (`j` outer).
#[derive(Clone, Debug)]
pub struct Lat {
    /// Nodes across.
    pub nx: usize,
    /// Nodes up.
    pub ny: usize,
    /// World x of node 0.
    pub x0: f64,
    /// World y of node 0.
    pub y0: f64,
    /// Spacing, mm.
    pub res: f64,
    /// `nx * ny` values.
    pub z: Vec<f64>,
}

#[allow(clippy::cast_precision_loss)]
impl Lat {
    /// A lattice covering `[x0, x1] × [y0, y1]`, every value `v`.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64, res: f64, v: f64) -> Self {
        let nx = libm::ceil((x1 - x0) / res) as usize + 1;
        let ny = libm::ceil((y1 - y0) / res) as usize + 1;
        Self {
            nx,
            ny,
            x0,
            y0,
            res,
            z: vec![v; nx * ny],
        }
    }

    /// The same lattice, filled by `f(x, y)`.
    pub fn like(&self, f: impl Fn(f64, f64) -> f64 + Sync) -> Self {
        let mut out = self.clone();
        let nx = self.nx;
        out.z.par_chunks_mut(nx).enumerate().for_each(|(j, row)| {
            let y = self.y(j);
            for (i, o) in row.iter_mut().enumerate() {
                *o = f(self.x(i), y);
            }
        });
        out
    }

    /// The same lattice with values `z`.
    pub fn with(&self, z: Vec<f64>) -> Self {
        debug_assert_eq!(z.len(), self.z.len());
        Self { z, ..self.clone() }
    }

    /// World x of column `i`.
    pub fn x(&self, i: usize) -> f64 {
        self.x0 + i as f64 * self.res
    }

    /// World y of row `j`.
    pub fn y(&self, j: usize) -> f64 {
        self.y0 + j as f64 * self.res
    }

    /// The value at node `(i, j)`.
    pub fn at(&self, i: usize, j: usize) -> f64 {
        self.z[j * self.nx + i]
    }

    /// Bilinear value at `(x, y)`, the edge value beyond the lattice.
    pub fn sample(&self, x: f64, y: f64) -> f64 {
        let fx = ((x - self.x0) / self.res).clamp(0.0, (self.nx - 1) as f64);
        let fy = ((y - self.y0) / self.res).clamp(0.0, (self.ny - 1) as f64);
        let (i0, j0) = (fx as usize, fy as usize);
        let (i1, j1) = ((i0 + 1).min(self.nx - 1), (j0 + 1).min(self.ny - 1));
        let (tx, ty) = (fx - i0 as f64, fy - j0 as f64);
        let a = self.at(i0, j0) * (1.0 - tx) + self.at(i1, j0) * tx;
        let b = self.at(i0, j1) * (1.0 - tx) + self.at(i1, j1) * tx;
        a * (1.0 - ty) + b * ty
    }

    /// Central differences (one-sided at the edges): `(∂x, ∂y)` per mm.
    pub fn gradient(&self) -> (Vec<f64>, Vec<f64>) {
        let (nx, ny, h) = (self.nx, self.ny, self.res);
        let mut gx = vec![0.0; nx * ny];
        let mut gy = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let (a, b) = (i.saturating_sub(1), (i + 1).min(nx - 1));
                gx[j * nx + i] = (self.at(b, j) - self.at(a, j)) / ((b - a).max(1) as f64 * h);
                let (c, d) = (j.saturating_sub(1), (j + 1).min(ny - 1));
                gy[j * nx + i] = (self.at(i, d) - self.at(i, c)) / ((d - c).max(1) as f64 * h);
            }
        }
        (gx, gy)
    }

    /// Gaussian blur of `sigma_mm` (edges reflected).
    pub fn blur(&self, sigma_mm: f64) -> Self {
        let s = sigma_mm / self.res;
        if s < 0.05 {
            return self.clone();
        }
        let k = kernel(s);
        let r = k.len() / 2;
        let (nx, ny) = (self.nx, self.ny);
        let refl = |i: i64, n: usize| -> usize {
            let n = n as i64;
            let mut i = i;
            if n == 1 {
                return 0;
            }
            let p = 2 * n;
            i = i.rem_euclid(p);
            (if i >= n { p - 1 - i } else { i }) as usize
        };
        let mut tmp = vec![0.0; nx * ny];
        tmp.par_chunks_mut(nx).enumerate().for_each(|(j, row)| {
            for (i, o) in row.iter_mut().enumerate() {
                let mut a = 0.0;
                for (t, w) in k.iter().enumerate() {
                    a += w * self.z[j * nx + refl(i as i64 + t as i64 - r as i64, nx)];
                }
                *o = a;
            }
        });
        let mut out = vec![0.0; nx * ny];
        out.par_chunks_mut(nx).enumerate().for_each(|(j, row)| {
            for (i, o) in row.iter_mut().enumerate() {
                let mut a = 0.0;
                for (t, w) in k.iter().enumerate() {
                    a += w * tmp[refl(j as i64 + t as i64 - r as i64, ny) * nx + i];
                }
                *o = a;
            }
        });
        self.with(out)
    }

    /// Unit-variance, zero-mean smooth noise of `sigma_mm` keyed by position so it does not depend on the
    /// lattice.
    pub fn noise(&self, seed: Seed, sigma_mm: f64) -> Self {
        let xs: Vec<f64> = (0..self.nx).map(|i| self.x(i)).collect();
        let ys: Vec<f64> = (0..self.ny).map(|j| self.y(j)).collect();
        let n = noise_at(seed, sigma_mm, &xs, &ys);
        let m = n.iter().sum::<f64>() / n.len() as f64;
        let sd = libm::sqrt(n.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / n.len() as f64);
        self.with(n.iter().map(|v| (v - m) / (sd + 1e-12)).collect())
    }
}

/// A normalised Gaussian kernel for `sigma` samples, reaching 4 sigma.
pub fn kernel(sigma: f64) -> Vec<f64> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let r = (4.0 * sigma + 0.5) as usize;
    let mut k: Vec<f64> = (0..=2 * r)
        .map(|t| {
            let d = t as f64 - r as f64;
            libm::exp(-0.5 * d * d / (sigma * sigma))
        })
        .collect();
    let s: f64 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= s);
    k
}

/// Smooth step 0 → 1 over `[0, 1]`.
pub fn s01(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// An angle wrapped to `[-π, π)`.
pub fn wrap(a: f64) -> f64 {
    let pi = core::f64::consts::PI;
    (a + pi).rem_euclid(2.0 * pi) - pi
}
