//! The grid kernel: values on a regular lattice of nodes, the uniform lift
//! of a height grid into a solid, and the exact Euclidean distance
//! transform.
//!
//! One convention: a grid of `nx` by
//! `ny` nodes, `step` mm apart, node `(i, j)` at `(i * step, j * step)` mm,
//! stored row by row (`j` outer). A tile of `cols` squares with `n` cells
//! per square has `cols * n + 1` nodes across, so its edge nodes lie exactly
//! on the tile edge.

use crate::{Footprint, TriMesh};

/// Values on a lattice of nodes, row-major (`j` outer): a painted height
/// (`Grid<f32>`, what is lifted), a field of numbers (`Grid<f64>`, the
/// kit's `Field`), flags or labels.  Positions are mm, `step` apart; a
/// grid counted in cells has `step` 1, and its mm methods work in cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid<T = f32> {
    /// Nodes across (x).
    pub nx: usize,
    /// Nodes up (y).
    pub ny: usize,
    /// Node spacing in mm.
    pub step: f64,
    /// `nx * ny` values.
    pub z: Vec<T>,
}

impl<T: Copy> Grid<T> {
    /// A grid with every node set to `value`.
    #[must_use]
    pub fn filled(nx: usize, ny: usize, step: f64, value: T) -> Self {
        Self {
            nx,
            ny,
            step,
            z: vec![value; nx * ny],
        }
    }

    /// The index of node `(i, j)`.
    #[must_use]
    pub const fn index(&self, i: usize, j: usize) -> usize {
        j * self.nx + i
    }

    /// The value at node `(i, j)`.
    #[must_use]
    #[inline]
    pub fn at(&self, i: usize, j: usize) -> T {
        self.z[j * self.nx + i]
    }

    /// Set node `(i, j)`.
    #[inline]
    pub fn set(&mut self, i: usize, j: usize, v: T) {
        self.z[j * self.nx + i] = v;
    }

    /// The same lattice with every value `f` of this one's.
    #[must_use]
    pub fn map<U: Copy>(&self, f: impl Fn(T) -> U) -> Grid<U> {
        self.with_z(self.z.iter().map(|&v| f(v)).collect())
    }

    /// Combine with another grid on the same lattice, node by node.
    #[must_use]
    pub fn zip<U: Copy, V: Copy>(&self, other: &Grid<U>, f: impl Fn(T, U) -> V) -> Grid<V> {
        self.with_z(
            self.z
                .iter()
                .zip(&other.z)
                .map(|(&a, &b)| f(a, b))
                .collect(),
        )
    }

    /// A grid on the same lattice with values `z`.
    #[must_use]
    pub fn with_z<U>(&self, z: Vec<U>) -> Grid<U> {
        debug_assert_eq!(z.len(), self.z.len());
        Grid {
            nx: self.nx,
            ny: self.ny,
            step: self.step,
            z,
        }
    }

    /// The x of column `i`, mm.
    #[must_use]
    pub fn x(&self, i: usize) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let x = i as f64 * self.step;
        x
    }

    /// The y of row `j`, mm.
    #[must_use]
    pub fn y(&self, j: usize) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let y = j as f64 * self.step;
        y
    }

    /// The node at or below `(x, y)` mm, clamped to the lattice.
    #[must_use]
    pub fn node(&self, x: f64, y: f64) -> (usize, usize) {
        let k = |v: f64, n: usize| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = libm::floor(v / self.step).max(0.0) as usize;
            k.min(n - 1)
        };
        (k(x, self.nx), k(y, self.ny))
    }

    /// The nodes within `r` mm of `(x, y)`, as index ranges `(i0..i1, j0..j1)`.
    #[must_use]
    pub fn window(
        &self,
        x: f64,
        y: f64,
        r: f64,
    ) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
        let lo = |v: f64, n: usize| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = libm::ceil((v - r) / self.step).max(0.0) as usize;
            k.min(n)
        };
        let hi = |v: f64, n: usize| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = (libm::floor((v + r) / self.step) + 1.0).max(0.0) as usize;
            k.min(n)
        };
        (
            lo(x, self.nx)..hi(x, self.nx),
            lo(y, self.ny)..hi(y, self.ny),
        )
    }
}

impl Grid<f32> {
    /// A 64-bit FNV-1a hash of the exact values (a stage hash).
    #[must_use]
    pub fn content_hash(&self) -> u64 {
        let mut h = crate::Fnv::default();
        h.u64(self.nx as u64);
        h.u64(self.ny as u64);
        h.u64(self.step.to_bits());
        self.z.iter().for_each(|v| h.u64(u64::from(v.to_bits())));
        h.finish()
    }
}

impl From<&Grid<f32>> for Grid<f64> {
    /// A painted surface as a field of numbers.
    fn from(g: &Grid<f32>) -> Self {
        g.map(f64::from)
    }
}

impl Grid<f64> {
    /// The bilinear value at `(x, y)` mm (clamped to the lattice).
    #[must_use]
    pub fn sample(&self, x: f64, y: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let (mx, my) = ((self.nx - 1) as f64, (self.ny - 1) as f64);
        let (u, v) = (
            (x / self.step).clamp(0.0, mx),
            (y / self.step).clamp(0.0, my),
        );
        let (fu, fv) = (libm::floor(u), libm::floor(v));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (fu as usize, fv as usize);
        let (i1, j1) = ((i + 1).min(self.nx - 1), (j + 1).min(self.ny - 1));
        let (tu, tv) = (u - fu, v - fv);
        let a = self.at(i, j) * (1.0 - tu) + self.at(i1, j) * tu;
        let b = self.at(i, j1) * (1.0 - tu) + self.at(i1, j1) * tu;
        a * (1.0 - tv) + b * tv
    }

    /// Gaussian blur of `sigma` mm (scipy's `gaussian_filter`, reflected
    /// ends; recursive past 32 nodes).
    #[must_use]
    pub fn blur(&self, sigma_mm: f64) -> Self {
        let s = sigma_mm / self.step;
        let z = if s > 32.0 {
            crate::filter::blur_recursive(&self.z, self.nx, self.ny, s)
        } else {
            crate::filter::blur(&self.z, self.nx, self.ny, s)
        };
        self.with_z(z)
    }

    /// Grey dilation over a disc of `r` mm.
    #[must_use]
    pub fn dilate(&self, r_mm: f64) -> Self {
        self.with_z(crate::filter::dilate(
            &self.z,
            self.nx,
            self.ny,
            Footprint::Disc(r_mm / self.step),
        ))
    }

    /// Grey erosion over a disc of `r` mm.
    #[must_use]
    pub fn erode(&self, r_mm: f64) -> Self {
        self.with_z(crate::filter::erode(
            &self.z,
            self.nx,
            self.ny,
            Footprint::Disc(r_mm / self.step),
        ))
    }

    /// The relief range (max − min) over the square window of side `w` mm
    /// round each node: the printable-texture measure.
    #[must_use]
    pub fn range(&self, w_mm: f64) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let r = libm::round(0.5 * w_mm / self.step) as usize;
        let hi = crate::filter::dilate(&self.z, self.nx, self.ny, Footprint::Square(r));
        let lo = crate::filter::erode(&self.z, self.nx, self.ny, Footprint::Square(r));
        self.with_z(hi.iter().zip(&lo).map(|(a, b)| a - b).collect())
    }

    /// numpy's linear quantile of the values where `keep` holds (all
    /// values when `keep` is `None`).
    #[must_use]
    pub fn quantile(&self, q: f64, keep: Option<&[bool]>) -> f64 {
        match keep {
            None => crate::filter::quantile(&self.z, q),
            Some(k) => {
                let v: Vec<f64> = self
                    .z
                    .iter()
                    .zip(k)
                    .filter(|(_, k)| **k)
                    .map(|(v, _)| *v)
                    .collect();
                crate::filter::quantile(&v, q)
            }
        }
    }

    /// The slope along x and along y (rise per mm) at every node, with
    /// numpy's `gradient` (central differences inside, one-sided at the
    /// ends; 0 across a single node).
    #[must_use]
    pub fn gradient(&self) -> (Self, Self) {
        let (nx, ny, h) = (self.nx, self.ny, self.step);
        let mut gx = vec![0.0; nx * ny];
        let mut gy = vec![0.0; nx * ny];
        for j in 0..ny {
            let (j0, j1) = (j.saturating_sub(1), (j + 1).min(ny - 1));
            for i in 0..nx {
                let (i0, i1) = (i.saturating_sub(1), (i + 1).min(nx - 1));
                let k = j * nx + i;
                #[allow(clippy::cast_precision_loss)]
                {
                    gx[k] = (self.at(i1, j) - self.at(i0, j)) / ((i1 - i0).max(1) as f64 * h);
                    gy[k] = (self.at(i, j1) - self.at(i, j0)) / ((j1 - j0).max(1) as f64 * h);
                }
            }
        }
        (self.with_z(gx), self.with_z(gy))
    }

    /// The steepest slope (rise per mm), from [`Grid::gradient`].
    #[must_use]
    pub fn max_slope(&self) -> f64 {
        let (gx, gy) = self.gradient();
        gx.z.iter()
            .zip(&gy.z)
            .fold(0.0, |m: f64, (x, y)| m.max(libm::sqrt(x * x + y * y)))
    }
}

impl Grid<bool> {
    /// The distance in nodes from every node to the nearest set one
    /// (infinite with none set), and that node's index.
    #[must_use]
    pub fn distance(&self) -> (Grid<f64>, Vec<Option<usize>>) {
        let (d, near) = distance_nearest(&self.z, self.nx, self.ny);
        (self.with_z(d), near)
    }

    /// The set nodes grown by the 4-neighbour cross, `n` times.
    #[must_use]
    pub fn grow4(&self, n: usize) -> Self {
        let mut cur = self.clone();
        for _ in 0..n {
            let mut next = cur.clone();
            for j in 0..self.ny {
                for i in 0..self.nx {
                    if cur.at(i, j) {
                        continue;
                    }
                    let hit = (i > 0 && cur.at(i - 1, j))
                        || (i + 1 < self.nx && cur.at(i + 1, j))
                        || (j > 0 && cur.at(i, j - 1))
                        || (j + 1 < self.ny && cur.at(i, j + 1));
                    if hit {
                        next.set(i, j, true);
                    }
                }
            }
            cur = next;
        }
        cur
    }

    /// The 4-connected pieces of the set nodes, labelled 1, 2, … in
    /// row-major order of their first node (0 elsewhere), and how many.
    #[must_use]
    pub fn label(&self) -> (Grid<u32>, u32) {
        let mut labels = self.map(|_| 0u32);
        let mut n = 0;
        let mut stack = Vec::new();
        for j in 0..self.ny {
            for i in 0..self.nx {
                if !self.at(i, j) || labels.at(i, j) != 0 {
                    continue;
                }
                n += 1;
                labels.set(i, j, n);
                stack.push((i, j));
                while let Some((a, b)) = stack.pop() {
                    let mut go = |x: usize, y: usize| {
                        if self.at(x, y) && labels.at(x, y) == 0 {
                            labels.set(x, y, n);
                            stack.push((x, y));
                        }
                    };
                    if a > 0 {
                        go(a - 1, b);
                    }
                    if a + 1 < self.nx {
                        go(a + 1, b);
                    }
                    if b > 0 {
                        go(a, b - 1);
                    }
                    if b + 1 < self.ny {
                        go(a, b + 1);
                    }
                }
            }
        }
        (labels, n)
    }
}

/// Why a grid could not be lifted.
#[derive(Clone, Debug, PartialEq)]
pub enum LiftError {
    /// Fewer than 2 × 2 nodes.
    TooSmall,
    /// A node at or below the base (z = 0), or not finite.
    BelowBase {
        /// The node.
        at: (usize, usize),
        /// Its value.
        z: f32,
    },
}

impl std::fmt::Display for LiftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooSmall => f.write_str("a surface needs at least 2 × 2 nodes"),
            Self::BelowBase { at, z } => {
                write!(
                    f,
                    "the surface is at {z} mm at node {at:?}: it must be above 0"
                )
            }
        }
    }
}

impl std::error::Error for LiftError {}

/// The uniform lift: a closed solid from z = 0 up to the grid's surface.
/// Every node becomes a top vertex (two triangles per cell, split along
/// the same diagonal); the side walls run from the perimeter down to z = 0;
/// the bottom is a fan from its centre to the perimeter at z = 0, so the
/// mesh is watertight by construction. Triangles wind outward.
///
/// # Errors
/// [`LiftError`] if the grid is smaller than 2 × 2 or a node is not above 0.
pub fn lift(g: &Grid) -> Result<TriMesh, LiftError> {
    let (nx, ny) = (g.nx, g.ny);
    if nx < 2 || ny < 2 || g.z.len() != nx * ny {
        return Err(LiftError::TooSmall);
    }
    for j in 0..ny {
        for i in 0..nx {
            let z = g.z[g.index(i, j)];
            if !(z.is_finite() && z > 0.0) {
                return Err(LiftError::BelowBase { at: (i, j), z });
            }
        }
    }
    let idx = |i: usize, j: usize| u32::try_from(j * nx + i).unwrap_or(u32::MAX);
    let mut positions = Vec::with_capacity(nx * ny + 2 * (nx + ny) + 1);
    for j in 0..ny {
        for i in 0..nx {
            positions.push(node(g, i, j, f64::from(g.z[g.index(i, j)])));
        }
    }
    let mut triangles = Vec::with_capacity(2 * (nx - 1) * (ny - 1) + 6 * (nx + ny));
    for j in 0..ny - 1 {
        for i in 0..nx - 1 {
            let (a, b, c, d) = (idx(i, j), idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
            triangles.push([a, b, c]);
            triangles.push([a, c, d]);
        }
    }
    // The perimeter, counter-clockwise seen from above, each corner once.
    let mut ring: Vec<(usize, usize)> = Vec::with_capacity(2 * (nx + ny));
    ring.extend((0..nx - 1).map(|i| (i, 0)));
    ring.extend((0..ny - 1).map(|j| (nx - 1, j)));
    ring.extend((1..nx).rev().map(|i| (i, ny - 1)));
    ring.extend((1..ny).rev().map(|j| (0, j)));
    let base = u32::try_from(positions.len()).unwrap_or(u32::MAX);
    positions.extend(ring.iter().map(|&(i, j)| node(g, i, j, 0.0)));
    let centre = u32::try_from(positions.len()).unwrap_or(u32::MAX);
    let half = |n: usize| 0.5 * (n - 1) as f64 * g.step;
    positions.push([half(nx), half(ny), 0.0]);
    let n = u32::try_from(ring.len()).unwrap_or(u32::MAX);
    for k in 0..n {
        let k1 = (k + 1) % n;
        let (t0, t1) = (
            idx(ring[k as usize].0, ring[k as usize].1),
            idx(ring[k1 as usize].0, ring[k1 as usize].1),
        );
        let (b0, b1) = (base + k, base + k1);
        triangles.push([t0, b0, b1]);
        triangles.push([t0, b1, t1]);
        triangles.push([centre, b1, b0]);
    }
    Ok(TriMesh {
        positions,
        triangles,
        ..TriMesh::default()
    })
}

fn node(g: &Grid, i: usize, j: usize, z: f64) -> [f64; 3] {
    [i as f64 * g.step, j as f64 * g.step, z]
}

/// The exact Euclidean distance, in nodes, from every node to the nearest
/// `true` node of an `nx` by `ny` mask (row-major); infinity where the mask
/// has no `true` node. Felzenszwalb and Huttenlocher's separable
/// algorithm, so the result is exact and independent of thread count.
#[must_use]
pub fn distance(mask: &[bool], nx: usize, ny: usize) -> Vec<f64> {
    distance_nearest(mask, nx, ny).0
}

/// The exact Euclidean distance, in nodes, from every node to the nearest
/// `true` node, and that node's index (row-major), as scipy's
/// `distance_transform_edt(..., return_indices=True)` on the inverted
/// mask. Ties go to the lowest row, then the lowest column. `None` where
/// the mask has no `true` node.
#[must_use]
pub fn distance_nearest(mask: &[bool], nx: usize, ny: usize) -> (Vec<f64>, Vec<Option<usize>>) {
    let mut d2: Vec<f64> = vec![f64::INFINITY; nx * ny];
    let mut col: Vec<usize> = vec![0; nx * ny];
    if !mask.iter().any(|&m| m) {
        return (d2, vec![None; nx * ny]);
    }
    let mut f = vec![0.0; nx.max(ny)];
    let mut out = vec![0.0; nx.max(ny)];
    let mut arg = vec![0usize; nx.max(ny)];
    for j in 0..ny {
        for i in 0..nx {
            f[i] = if mask[j * nx + i] { 0.0 } else { f64::INFINITY };
        }
        edt_1d_arg(&f[..nx], &mut out[..nx], &mut arg[..nx]);
        d2[j * nx..(j + 1) * nx].copy_from_slice(&out[..nx]);
        col[j * nx..(j + 1) * nx].copy_from_slice(&arg[..nx]);
    }
    let mut near = vec![None; nx * ny];
    let mut dist = vec![f64::INFINITY; nx * ny];
    for i in 0..nx {
        for j in 0..ny {
            f[j] = d2[j * nx + i];
        }
        edt_1d_arg(&f[..ny], &mut out[..ny], &mut arg[..ny]);
        for j in 0..ny {
            if out[j].is_finite() {
                let jj = arg[j];
                near[j * nx + i] = Some(jj * nx + col[jj * nx + i]);
                dist[j * nx + i] = out[j].sqrt();
            }
        }
    }
    (dist, near)
}

/// 1D squared distance transform of a sampled function `f` (0 or ∞ on
/// the first pass, squared distances on the second), recording for each
/// sample the sample whose parabola is lowest there.
fn edt_1d_arg(f: &[f64], out: &mut [f64], arg: &mut [usize]) {
    let n = f.len();
    let Some(first) = f.iter().position(|x| x.is_finite()) else {
        out.iter_mut().for_each(|x| *x = f64::INFINITY);
        arg.iter_mut().for_each(|a| *a = 0);
        return;
    };
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    v[0] = first;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in first + 1..n {
        if f[q].is_infinite() {
            continue;
        }
        let qf = q as f64;
        let mut s;
        loop {
            let p = v[k] as f64;
            s = ((f[q] + qf * qf) - (f[v[k]] + p * p)) / (2.0 * qf - 2.0 * p);
            if s <= z[k] {
                k -= 1; // z[0] is -inf, so this stops at k = 0
            } else {
                break;
            }
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for q in 0..n {
        let qf = q as f64;
        while z[k + 1] < qf {
            k += 1;
        }
        let p = v[k] as f64;
        out[q] = (qf - p) * (qf - p) + f[v[k]];
        arg[q] = v[k];
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]
    use super::*;

    #[test]
    fn lift_is_watertight() {
        let mut g = Grid::<f32>::filled(5, 4, 0.5, 2.0);
        g.z[7] = 3.0;
        let m = lift(&g).unwrap();
        assert!(m.is_watertight());
        assert_eq!(m.positions.len(), 20 + 14 + 1);
    }

    #[test]
    fn lift_refuses_zero() {
        let mut g = Grid::<f32>::filled(3, 3, 1.0, 1.0);
        g.z[4] = 0.0;
        assert_eq!(lift(&g), Err(LiftError::BelowBase { at: (1, 1), z: 0.0 }));
    }

    #[test]
    fn distance_matches_brute_force() {
        let (nx, ny) = (13, 9);
        let mask: Vec<bool> = (0..nx * ny).map(|k| k % 17 == 3 || k == 50).collect();
        let d = distance(&mask, nx, ny);
        for j in 0..ny {
            for i in 0..nx {
                let mut best = f64::INFINITY;
                for (k, &m) in mask.iter().enumerate() {
                    if m {
                        let (a, b) = ((k % nx) as f64, (k / nx) as f64);
                        best = best.min(((a - i as f64).powi(2) + (b - j as f64).powi(2)).sqrt());
                    }
                }
                assert!((d[j * nx + i] - best).abs() < 1e-12, "({i},{j})");
            }
        }
        assert!(distance(&[false; 4], 2, 2).iter().all(|x| x.is_infinite()));
    }

    #[test]
    fn nearest_is_a_nearest_true_node() {
        let (nx, ny) = (13, 9);
        let mask: Vec<bool> = (0..nx * ny).map(|k| k % 17 == 3 || k == 50).collect();
        let d = distance(&mask, nx, ny);
        let (dn, near) = distance_nearest(&mask, nx, ny);
        for k in 0..nx * ny {
            let n = near[k].unwrap();
            assert!(mask[n]);
            let (a, b) = (
                (k % nx) as f64 - (n % nx) as f64,
                (k / nx) as f64 - (n / nx) as f64,
            );
            assert!(((a * a + b * b).sqrt() - d[k]).abs() < 1e-12);
            assert!((dn[k] - d[k]).abs() < 1e-12);
        }
        assert!(
            distance_nearest(&[false; 4], 2, 2)
                .1
                .iter()
                .all(Option::is_none)
        );
    }
}
