//! The grid kernel: values on a regular lattice of nodes, the uniform lift
//! of a height grid into a solid, and the exact Euclidean distance
//! transform.
//!
//! One convention: a grid of `nx` by
//! `ny` nodes, `step` mm apart, node `(i, j)` at `(i * step, j * step)` mm,
//! stored row by row (`j` outer). A tile of `cols` squares with `n` cells
//! per square has `cols * n + 1` nodes across, so its edge nodes lie exactly
//! on the tile edge.

use crate::TriMesh;

/// Values on a lattice of nodes, row-major (`j` outer).
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    /// Nodes across (x).
    pub nx: usize,
    /// Nodes up (y).
    pub ny: usize,
    /// Node spacing in mm.
    pub step: f64,
    /// `nx * ny` values.
    pub z: Vec<f32>,
}

impl Grid {
    /// A grid with every node set to `value`.
    #[must_use]
    pub fn new(nx: usize, ny: usize, step: f64, value: f32) -> Self {
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
        let mut g = Grid::new(5, 4, 0.5, 2.0);
        g.z[7] = 3.0;
        let m = lift(&g).unwrap();
        assert!(m.is_watertight());
        assert_eq!(m.positions.len(), 20 + 14 + 1);
    }

    #[test]
    fn lift_refuses_zero() {
        let mut g = Grid::new(3, 3, 1.0, 1.0);
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
