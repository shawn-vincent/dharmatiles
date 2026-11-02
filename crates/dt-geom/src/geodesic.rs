//! Geodesic distance on the node lattice: multi-source shortest paths
//! through a mask (waterlab `_geodesic` :660, which uses scipy's
//! Dijkstra on the same 8-connected graph).
//!
//! Waves from land and rocks bend round a rock because their distance is
//! measured along paths that stay in the water.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Ordered f64 for the heap (distances are finite and non-negative).
#[derive(Clone, Copy, PartialEq)]
struct D(f64);
impl Eq for D {}
impl PartialOrd for D {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for D {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&o.0)
    }
}

/// Distance (in units of `step`) from the nearest `source` node to every
/// node, along 8-connected paths whose nodes are all `source` or
/// `passable`, edges weighing `step` (straight) or `step·√2` (diagonal).
/// Unreached nodes get `f64::INFINITY`. Also returns each node's nearest
/// source node index (`None` where unreached). Ties keep the lower node
/// index, so the result does not depend on anything but the inputs.
#[must_use]
pub fn geodesic(
    source: &[bool],
    passable: &[bool],
    nx: usize,
    ny: usize,
    step: f64,
) -> (Vec<f64>, Vec<Option<usize>>) {
    let n = nx * ny;
    let mut dist = vec![f64::INFINITY; n];
    let mut near: Vec<Option<usize>> = vec![None; n];
    let mut heap = BinaryHeap::new();
    for k in 0..n {
        if source[k] {
            dist[k] = 0.0;
            near[k] = Some(k);
            heap.push(Reverse((D(0.0), k)));
        }
    }
    let diag = step * core::f64::consts::SQRT_2;
    let ok = |k: usize| source[k] || passable[k];
    while let Some(Reverse((D(d), k))) = heap.pop() {
        if d > dist[k] {
            continue;
        }
        let (i, j) = (k % nx, k / nx);
        for (di, dj, w) in [
            (-1i64, 0i64, step),
            (1, 0, step),
            (0, -1, step),
            (0, 1, step),
            (-1, -1, diag),
            (1, -1, diag),
            (-1, 1, diag),
            (1, 1, diag),
        ] {
            #[allow(clippy::cast_possible_wrap)]
            let (a, b) = (i as i64 + di, j as i64 + dj);
            #[allow(clippy::cast_possible_wrap)]
            if a < 0 || b < 0 || a >= nx as i64 || b >= ny as i64 {
                continue;
            }
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let m = b as usize * nx + a as usize;
            if !ok(m) {
                continue;
            }
            let nd = d + w;
            if nd.total_cmp(&dist[m]).then(near[k].cmp(&near[m])).is_lt() {
                let better = nd < dist[m];
                dist[m] = nd;
                near[m] = near[k];
                if better {
                    heap.push(Reverse((D(nd), m)));
                }
            }
        }
    }
    (dist, near)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]
    use super::*;

    #[test]
    fn paths_go_round_a_wall() {
        // 9 × 9, source at (0, 4), a wall at x = 4 from y = 0 to 7
        let (nx, ny) = (9, 9);
        let mut src = vec![false; nx * ny];
        src[4 * nx] = true;
        let mut pass = vec![true; nx * ny];
        for j in 0..8 {
            pass[j * nx + 4] = false;
        }
        let (d, near) = geodesic(&src, &pass, nx, ny, 1.0);
        // straight along the row it is 8; round the wall's end it is longer
        assert!(d[4 * nx + 8] > 8.0 + 1.0);
        assert!(d[4 * nx + 3] == 3.0);
        assert_eq!(near[4 * nx + 8], Some(4 * nx));
        assert!(d[3 * nx + 4].is_infinite());
    }
}
