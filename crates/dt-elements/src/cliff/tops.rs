//! The pile's tops, read from the finished stones, never from the plan they
//! were made from.
//!
//! A ray is dropped on every node of the lattice onto the stones as they are
//! (cut, chipped, carved and grained); the highest first hit is the top of
//! the pile there. The core stands under it, and what is laid after the
//! cliff (a dirt) lies on it as on any surface: the terrain's support.

use dt_kit::Frame;
use dt_kit::rays::RayMesh;
use rayon::prelude::*;

/// The top of the pile at every node (mm, as the stones stand, `-∞` where
/// no stone is hit): a ray down from above onto each stone.
pub fn stone_tops(frame: &Frame, stones: &[RayMesh], foot: f64) -> Vec<f64> {
    let (nx, ny) = frame.nodes();
    let step = frame.step_mm();
    let per_stone: Vec<Vec<(usize, f64)>> = stones
        .par_iter()
        .map(|m| {
            let (lo, hi) = m.bounds();
            let (is, js) = frame.node_range([lo[0], lo[1]], [hi[0], hi[1]], 0.0);
            let mut at = Vec::new();
            let mut origins = Vec::new();
            for j in js {
                for i in is.clone() {
                    at.push(j * nx + i);
                    origins.push([i as f64 * step, j as f64 * step, hi[2] + 1.0]);
                }
            }
            let dirs = vec![[0.0, 0.0, -1.0]; origins.len()];
            m.first_hits(&origins, &dirs)
                .into_iter()
                .zip(at)
                .filter_map(|(h, k)| h.map(|h| (k, h.point[2])))
                .collect()
        })
        .collect();
    let mut top = vec![f64::NEG_INFINITY; nx * ny];
    for hits in per_stone {
        for (k, z) in hits {
            top[k] = top[k].max(z + foot);
        }
    }
    top
}
