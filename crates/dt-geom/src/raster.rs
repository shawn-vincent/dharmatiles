//! The 2D kernel, raster half: masks on the node lattice of
//! [`crate::grid`], line rasterising and the paint bucket.
//!
//! Lines are walls, and a region
//! is the 4-connected set of non-wall nodes reached from a fill point.
//! Lines are drawn 8-connected, so a 4-connected fill never leaks through
//! a diagonal step.

/// A boolean mask on an `nx` by `ny` node lattice, row-major.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    /// Nodes across.
    pub nx: usize,
    /// Nodes up.
    pub ny: usize,
    /// `nx * ny` flags.
    pub bits: Vec<bool>,
}

impl Mask {
    /// A mask with every node set to `value`.
    #[must_use]
    pub fn new(nx: usize, ny: usize, value: bool) -> Self {
        Self {
            nx,
            ny,
            bits: vec![value; nx * ny],
        }
    }

    /// Node `(i, j)`, `false` outside the lattice.
    #[must_use]
    pub fn get(&self, i: usize, j: usize) -> bool {
        i < self.nx && j < self.ny && self.bits[j * self.nx + i]
    }

    /// Number of set nodes.
    #[must_use]
    pub fn count(&self) -> usize {
        self.bits.iter().filter(|&&b| b).count()
    }

    /// A 64-bit FNV-1a hash of the flags (a stage hash).
    #[must_use]
    pub fn content_hash(&self) -> u64 {
        let mut h = crate::Fnv::default();
        h.u64(self.nx as u64);
        h.u64(self.ny as u64);
        for chunk in self.bits.chunks(8) {
            let byte = chunk
                .iter()
                .enumerate()
                .fold(0u8, |b, (k, &x)| b | (u8::from(x) << k));
            h.bytes(&[byte]);
        }
        h.finish()
    }
}

/// Mark every node a straight segment from `a` to `b` passes, with points
/// in lattice units (node `(i, j)` is at `(i, j)`). Samples at most half a
/// node apart and marks the nearest node of each, so the line is
/// 8-connected; points outside the lattice are clamped to its edge.
pub fn draw_line(mask: &mut Mask, a: [f64; 2], b: [f64; 2]) {
    if mask.nx == 0 || mask.ny == 0 {
        return;
    }
    let len = (b[0] - a[0]).abs().max((b[1] - a[1]).abs());
    let steps = (2.0 * len).ceil().max(1.0);
    let clamp = |v: f64, n: usize| v.round().clamp(0.0, (n - 1) as f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = steps as usize;
    for s in 0..=n {
        let t = s as f64 / steps;
        let x = a[0] + (b[0] - a[0]) * t;
        let y = a[1] + (b[1] - a[1]) * t;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (clamp(x, mask.nx) as usize, clamp(y, mask.ny) as usize);
        mask.bits[j * mask.nx + i] = true;
    }
}

/// The paint bucket: every node 4-connected to `seed` without crossing a
/// wall. Empty if `seed` is a wall or outside the lattice.
#[must_use]
pub fn fill(walls: &Mask, seed: (usize, usize)) -> Mask {
    let (nx, ny) = (walls.nx, walls.ny);
    let mut out = Mask::new(nx, ny, false);
    if seed.0 >= nx || seed.1 >= ny || walls.get(seed.0, seed.1) {
        return out;
    }
    let mut queue = std::collections::VecDeque::from([seed]);
    out.bits[seed.1 * nx + seed.0] = true;
    while let Some((i, j)) = queue.pop_front() {
        let mut visit = |i: usize, j: usize| {
            let k = j * nx + i;
            if !walls.bits[k] && !out.bits[k] {
                out.bits[k] = true;
                queue.push_back((i, j));
            }
        };
        if i > 0 {
            visit(i - 1, j);
        }
        if i + 1 < nx {
            visit(i + 1, j);
        }
        if j > 0 {
            visit(i, j - 1);
        }
        if j + 1 < ny {
            visit(i, j + 1);
        }
    }
    out
}

/// The nodes inside `polygons` (closed rings in mm, even-odd, as a slice
/// returns them) on an `nx` by `ny` lattice of `step` mm spacing, node
/// `(i, j)` at `(i·step, j·step)`: a solid's footprint. Scanline by
/// rows: a node is inside when an odd number of ring edges cross the row
/// to its left.
#[must_use]
pub fn fill_polygons(nx: usize, ny: usize, step: f64, polygons: &[Vec<[f64; 2]>]) -> Mask {
    let mut m = Mask::new(nx, ny, false);
    let mut xs: Vec<f64> = Vec::new();
    #[allow(clippy::cast_precision_loss)]
    let last = nx.saturating_sub(1) as f64;
    for j in 0..ny {
        #[allow(clippy::cast_precision_loss)]
        let y = j as f64 * step;
        xs.clear();
        for ring in polygons {
            let k = ring.len();
            for e in 0..k {
                let (a, b) = (ring[e], ring[(e + 1) % k]);
                // half-open in y, so a vertex on the row counts once
                if (a[1] <= y) != (b[1] <= y) {
                    xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            let lo = libm::ceil(pair[0] / step).max(0.0);
            let hi = libm::floor(pair[1] / step).min(last);
            if hi >= lo {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (lo, hi) = (lo as usize, hi as usize);
                m.bits[j * nx + lo..=j * nx + hi].fill(true);
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diagonal_line_splits_the_lattice() {
        let mut walls = Mask::new(20, 20, false);
        draw_line(&mut walls, [0.0, 0.0], [19.0, 13.0]);
        draw_line(&mut walls, [19.0, 13.0], [19.0, 19.0]);
        let below = fill(&walls, (15, 2));
        let above = fill(&walls, (2, 15));
        assert!(below.count() > 0 && above.count() > 0);
        assert!(!below.bits.iter().zip(&above.bits).any(|(a, b)| *a && *b));
        assert_eq!(
            below.count() + above.count() + walls.count(),
            20 * 20,
            "every node is a wall or in exactly one region"
        );
        assert_eq!(
            fill(&walls, (0, 0)).count(),
            0,
            "a seed on a wall fills nothing"
        );
    }

    #[test]
    fn no_walls_fills_everything() {
        assert_eq!(fill(&Mask::new(7, 5, false), (3, 2)).count(), 35);
    }

    #[test]
    fn polygons_fill_their_inside() {
        // a 2 mm square with a 1 mm square hole, on a 0.25 mm lattice
        let outer = vec![[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]];
        let hole = vec![[1.5, 1.5], [1.5, 2.5], [2.5, 2.5], [2.5, 1.5]];
        let m = fill_polygons(20, 20, 0.25, &[outer, hole]);
        assert!(m.get(5, 5) && m.get(11, 11) && m.get(4, 8));
        assert!(!m.get(8, 8) && !m.get(3, 3) && !m.get(13, 13));
        assert!(fill_polygons(4, 4, 1.0, &[]).count() == 0);
    }
}
