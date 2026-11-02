//! A spatial hash: discs on a bounded plane, binned by a fixed cell, so
//! "is anything near here?" visits only nearby cells. Answers depend only
//! on what was inserted, never on order or threads.

/// Discs `(x, y, r)` binned into square cells over `[0, w] × [0, h]`
/// (points outside are clamped into the edge cells).
#[derive(Clone, Debug)]
pub struct SpatialHash {
    cell: f64,
    nx: usize,
    ny: usize,
    bins: Vec<Vec<u32>>,
    discs: Vec<[f64; 3]>,
}

impl SpatialHash {
    /// An empty hash over a `w` by `h` plane with `cell`-sized bins.
    #[must_use]
    pub fn new(w: f64, h: f64, cell: f64) -> Self {
        let cell = if cell > 0.0 { cell } else { 1.0 };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = |v: f64| ((v.max(0.0) / cell).ceil() as usize).max(1);
        let (nx, ny) = (n(w), n(h));
        Self {
            cell,
            nx,
            ny,
            bins: vec![Vec::new(); nx * ny],
            discs: Vec::new(),
        }
    }

    fn bin(&self, v: f64, n: usize) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = (v / self.cell).floor().max(0.0) as usize;
        k.min(n - 1)
    }

    /// Add a disc.
    pub fn insert(&mut self, x: f64, y: f64, r: f64) {
        let k = self.bin(y, self.ny) * self.nx + self.bin(x, self.nx);
        let id = u32::try_from(self.discs.len()).unwrap_or(u32::MAX);
        self.discs.push([x, y, r]);
        self.bins[k].push(id);
    }

    /// The discs inserted so far, in order.
    #[must_use]
    pub fn discs(&self) -> &[[f64; 3]] {
        &self.discs
    }

    /// True if `test` holds for any disc whose centre is within `reach`
    /// of `(x, y)` (others are not offered).
    pub fn any_near(&self, x: f64, y: f64, reach: f64, test: impl Fn(&[f64; 3]) -> bool) -> bool {
        let (i0, i1) = (self.bin(x - reach, self.nx), self.bin(x + reach, self.nx));
        let (j0, j1) = (self.bin(y - reach, self.ny), self.bin(y + reach, self.ny));
        for j in j0..=j1 {
            for i in i0..=i1 {
                for &id in &self.bins[j * self.nx + i] {
                    let d = &self.discs[id as usize];
                    if libm::hypot(d[0] - x, d[1] - y) <= reach && test(d) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::cast_precision_loss)]
    use super::*;

    #[test]
    fn finds_exactly_the_near_discs() {
        let mut h = SpatialHash::new(50.0, 40.0, 3.0);
        let pts: Vec<[f64; 3]> = (0..400)
            .map(|k| {
                let k = k as f64;
                [(k * 7.3) % 50.0, (k * 3.1) % 40.0, 0.1 * (k % 7.0)]
            })
            .collect();
        for p in &pts {
            h.insert(p[0], p[1], p[2]);
        }
        for q in 0..200 {
            let (x, y) = ((q as f64 * 1.7) % 52.0 - 1.0, (q as f64 * 2.9) % 42.0 - 1.0);
            let near = |d: &[f64; 3]| (d[0] - x).hypot(d[1] - y) < 2.0 + d[2];
            let brute = pts.iter().any(near);
            assert_eq!(h.any_near(x, y, 2.7, near), brute, "({x},{y})");
        }
    }
}
