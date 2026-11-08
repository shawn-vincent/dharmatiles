//! The adaptive lift: a height grid as a closed solid whose top keeps
//! fewer triangles where the surface is flat, within an error bound.
//!
//! The top is tiled by rectangles of nodes (leaves), split in halves
//! until each is triangulated within `tol` mm of every node it covers.
//! Leaves conform: every leaf corner on a leaf's side is a vertex of that
//! side, so neighbours share their edges exactly (no T-junctions) and the
//! mesh is watertight by construction. A leaf with only its four corners
//! is two triangles split as the uniform lift splits a cell; a leaf with
//! more is a fan from its centre; leaves stay balanced (a side holds at
//! most one vertex between its corners), so fans stay well shaped. Every
//! node is checked against the final
//! triangulation, and leaves that miss are split again, so the bound
//! holds at every node. A one-cell leaf is exactly the uniform lift.
//!
//! Walls run from the perimeter vertices down to z = 0 and the bottom is
//! a fan from its centre, as in [`crate::grid::lift`].

use rayon::prelude::*;

use crate::grid::{Grid, LiftError};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Leaf {
    pub(crate) i0: usize,
    pub(crate) j0: usize,
    pub(crate) i1: usize,
    pub(crate) j1: usize,
}

impl Leaf {
    fn split(self) -> [Self; 2] {
        let (w, h) = (self.i1 - self.i0, self.j1 - self.j0);
        if w >= h {
            let m = self.i0 + w / 2;
            [Self { i1: m, ..self }, Self { i0: m, ..self }]
        } else {
            let m = self.j0 + h / 2;
            [Self { j1: m, ..self }, Self { j0: m, ..self }]
        }
    }

    pub(crate) const fn is_cell(&self) -> bool {
        self.i1 - self.i0 == 1 && self.j1 - self.j0 == 1
    }
}

/// The leaves of the adaptive lift. `force` makes a leaf a single cell
/// (the stack mesher forces every leaf a material boundary crosses); a
/// forced leaf is split before the error bound is asked of it, and a
/// `force` that is always false changes nothing.
pub(crate) fn plan_leaves(
    g: &Grid,
    tol: f64,
    max_cells: usize,
    force: &(dyn Fn(Leaf) -> bool + Sync),
) -> Result<Vec<Leaf>, LiftError> {
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
    let z = |i: usize, j: usize| f64::from(g.z[j * nx + i]);
    let max_cells = max_cells.max(1);
    // first pass: split until each leaf is within tol of its bilinear patch
    let mut leaves = Vec::new();
    let mut todo = vec![Leaf {
        i0: 0,
        j0: 0,
        i1: nx - 1,
        j1: ny - 1,
    }];
    while let Some(l) = todo.pop() {
        let big = l.i1 - l.i0 > max_cells || l.j1 - l.j0 > max_cells;
        if !l.is_cell() && (big || force(l) || bilinear_error(&z, l) > tol) {
            todo.extend(l.split());
        } else {
            leaves.push(l);
        }
    }
    // then split what is out of balance (a side with more than one
    // vertex between its corners: fans stay well shaped) or misses the
    // bound on its real triangulation, until nothing does
    loop {
        let verts = Corners::new(nx, ny, &leaves);
        let split: Vec<bool> = leaves
            .par_iter()
            .map(|l| {
                !l.is_cell()
                    && (force(*l) || verts.crowded(*l) || fan_error(&z, g, &verts, *l) > tol)
            })
            .collect();
        if !split.contains(&true) {
            break;
        }
        leaves = leaves
            .iter()
            .zip(&split)
            .flat_map(|(l, &s)| if s { l.split().to_vec() } else { vec![*l] })
            .collect();
    }
    leaves.sort_unstable_by_key(|l| (l.j0, l.i0));
    Ok(leaves)
}

/// Max |z − bilinear patch of the corners| over the leaf's nodes.
fn bilinear_error(z: &impl Fn(usize, usize) -> f64, l: Leaf) -> f64 {
    let (a, b, c, d) = (z(l.i0, l.j0), z(l.i1, l.j0), z(l.i1, l.j1), z(l.i0, l.j1));
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = ((l.i1 - l.i0) as f64, (l.j1 - l.j0) as f64);
    let mut e: f64 = 0.0;
    for j in l.j0..=l.j1 {
        #[allow(clippy::cast_precision_loss)]
        let v = (j - l.j0) as f64 / h;
        for i in l.i0..=l.i1 {
            #[allow(clippy::cast_precision_loss)]
            let u = (i - l.i0) as f64 / w;
            let p = (a * (1.0 - u) + b * u) * (1.0 - v) + (d * (1.0 - u) + c * u) * v;
            e = e.max((z(i, j) - p).abs());
        }
    }
    e
}

/// The leaf corners, by row and by column, so a leaf can list every
/// vertex on its sides.
pub(crate) struct Corners {
    rows: Vec<Vec<usize>>,
    cols: Vec<Vec<usize>>,
}

impl Corners {
    pub(crate) fn new(nx: usize, ny: usize, leaves: &[Leaf]) -> Self {
        let mut rows = vec![Vec::new(); ny];
        let mut cols = vec![Vec::new(); nx];
        for l in leaves {
            for (i, j) in [(l.i0, l.j0), (l.i1, l.j0), (l.i1, l.j1), (l.i0, l.j1)] {
                rows[j].push(i);
                cols[i].push(j);
            }
        }
        for v in rows.iter_mut().chain(cols.iter_mut()) {
            v.sort_unstable();
            v.dedup();
        }
        Self { rows, cols }
    }

    /// True when a side of the leaf has more than one vertex between its
    /// corners (a neighbour more than twice as fine).
    fn crowded(&self, l: Leaf) -> bool {
        let inner = |v: &[usize], a: usize, b: usize| {
            v.partition_point(|&x| x < b) - v.partition_point(|&x| x <= a)
        };
        inner(&self.rows[l.j0], l.i0, l.i1) > 1
            || inner(&self.rows[l.j1], l.i0, l.i1) > 1
            || inner(&self.cols[l.i0], l.j0, l.j1) > 1
            || inner(&self.cols[l.i1], l.j0, l.j1) > 1
    }

    /// The leaf's boundary nodes, counter-clockwise from (i0, j0), each once.
    pub(crate) fn ring(&self, l: Leaf) -> Vec<(usize, usize)> {
        let span = |v: &[usize], a: usize, b: usize| -> Vec<usize> {
            let s = v.partition_point(|&x| x < a);
            let e = v.partition_point(|&x| x <= b);
            v[s..e].to_vec()
        };
        let mut ring = Vec::new();
        for i in span(&self.rows[l.j0], l.i0, l.i1) {
            if i < l.i1 {
                ring.push((i, l.j0));
            }
        }
        for j in span(&self.cols[l.i1], l.j0, l.j1) {
            if j < l.j1 {
                ring.push((l.i1, j));
            }
        }
        for i in span(&self.rows[l.j1], l.i0, l.i1).into_iter().rev() {
            if i > l.i0 {
                ring.push((i, l.j1));
            }
        }
        for j in span(&self.cols[l.i0], l.j0, l.j1).into_iter().rev() {
            if j > l.j0 {
                ring.push((l.i0, j));
            }
        }
        ring
    }
}

/// The fan centre of a leaf: its middle, on the grid's bilinear surface.
pub(crate) fn centre(g: &Grid, l: Leaf) -> [f64; 3] {
    #[allow(clippy::cast_precision_loss)]
    let (ci, cj) = (0.5 * (l.i0 + l.i1) as f64, 0.5 * (l.j0 + l.j1) as f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (fi, fj) = (ci.floor() as usize, cj.floor() as usize);
    let (ti, tj) = (ci - libm::floor(ci), cj - libm::floor(cj));
    let (i1, j1) = ((fi + 1).min(g.nx - 1), (fj + 1).min(g.ny - 1));
    let z = |i: usize, j: usize| f64::from(g.z[j * g.nx + i]);
    let zc = (z(fi, fj) * (1.0 - ti) + z(i1, fj) * ti) * (1.0 - tj)
        + (z(fi, j1) * (1.0 - ti) + z(i1, j1) * ti) * tj;
    [ci * g.step, cj * g.step, zc]
}

/// The triangles of a leaf, in node coordinates (x, y in nodes, z in mm).
fn triangles(g: &Grid, verts: &Corners, l: Leaf) -> Vec<[[f64; 3]; 3]> {
    let ring = verts.ring(l);
    #[allow(clippy::cast_precision_loss)]
    let p = |(i, j): (usize, usize)| [i as f64, j as f64, f64::from(g.z[j * g.nx + i])];
    if ring.len() == 4 {
        let [a, b, c, d] = [p(ring[0]), p(ring[1]), p(ring[2]), p(ring[3])];
        return vec![[a, b, c], [a, c, d]];
    }
    let c = centre(g, l);
    let c = [c[0] / g.step, c[1] / g.step, c[2]];
    (0..ring.len())
        .map(|k| [c, p(ring[k]), p(ring[(k + 1) % ring.len()])])
        .collect()
}

/// Max |z − triangulation| over the leaf's nodes.
fn fan_error(z: &impl Fn(usize, usize) -> f64, g: &Grid, verts: &Corners, l: Leaf) -> f64 {
    let mut e: f64 = 0.0;
    for t in triangles(g, verts, l) {
        let [a, b, c] = t;
        let det = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if det.abs() < 1e-12 {
            continue;
        }
        let lo = |k: usize| a[k].min(b[k]).min(c[k]);
        let hi = |k: usize| a[k].max(b[k]).max(c[k]);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i0, i1, j0, j1) = (
            lo(0).ceil() as usize,
            hi(0).floor() as usize,
            lo(1).ceil() as usize,
            hi(1).floor() as usize,
        );
        for j in j0..=j1 {
            for i in i0..=i1 {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (i as f64, j as f64);
                let w0 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / det;
                let w1 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / det;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                let zt = w0 * a[2] + w1 * b[2] + w2 * c[2];
                e = e.max((z(i, j) - zt).abs());
            }
        }
    }
    e
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::cast_precision_loss)]
    use super::*;
    use crate::TriMesh;
    use crate::stack::{Lattice, Layer, Round, Sides, lift_stack};

    /// The adaptive lift of one layer with flat faces: no node more than
    /// `tol` mm from the surface, no leaf wider than `max_cells` cells.
    fn lift_adaptive(g: &Grid, tol: f64, max_cells: usize) -> Result<TriMesh, LiftError> {
        let lat = Lattice {
            nx: g.nx,
            ny: g.ny,
            step: g.step,
            tol,
            max_cells,
            round: Round::None,
        };
        lift_stack(&lat, &[Layer::new(&g.z, 0)], &Sides::flat())
    }

    pub(super) fn bumpy(nx: usize, ny: usize) -> Grid {
        let mut g = Grid::<f32>::filled(nx, ny, 0.25, 3.0);
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (i as f64 * 0.25, j as f64 * 0.25);
                // a smooth hill, a sharp ridge, and a flat plain
                let hill = 2.0 * (-((x - 6.0).powi(2) + (y - 5.0).powi(2)) / 8.0).exp();
                let ridge = (0.6 - (x - 12.0).abs()).max(0.0);
                #[allow(clippy::cast_possible_truncation)]
                let v = (3.0 + hill + ridge) as f32;
                g.z[j * nx + i] = v;
            }
        }
        g
    }

    #[test]
    fn adaptive_is_watertight_and_within_tolerance() {
        let g = bumpy(61, 47);
        let uniform = crate::grid::lift(&g).unwrap();
        for tol in [0.0, 0.01, 0.05] {
            let m = lift_adaptive(&g, tol, 16).unwrap();
            assert!(m.is_watertight(), "tol {tol}");
            assert!(m.triangles.len() <= uniform.triangles.len(), "tol {tol}");
            if tol > 0.0 {
                assert!(m.triangles.len() < uniform.triangles.len() / 2, "tol {tol}");
            }
            // same volume as the uniform lift, within tol × area
            let vol = |m: &TriMesh| -> f64 {
                m.triangles
                    .iter()
                    .map(|t| {
                        let [a, b, c] = t.map(|i| m.positions[i as usize]);
                        (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                            + a[2] * (b[0] * c[1] - b[1] * c[0]))
                            / 6.0
                    })
                    .sum()
            };
            let area = 15.0 * 11.5;
            assert!(
                (vol(&m) - vol(&uniform)).abs() <= tol * area + 1e-6,
                "tol {tol}"
            );
        }
    }

    #[test]
    fn one_cell_leaves_are_the_uniform_lift() {
        let g = bumpy(9, 7);
        let m = lift_adaptive(&g, -1.0, 1).unwrap();
        let u = crate::grid::lift(&g).unwrap();
        assert_eq!(m.triangles.len(), u.triangles.len());
        assert!(m.is_watertight());
    }
}
