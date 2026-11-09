//! The film's ground: where a dirt laid on the cliff lies, read from the
//! finished stones, never from the plan they were made from.
//!
//! A ray is dropped on every node of the lattice onto the stones as they are
//! (cut, chipped, carved and grained); the highest first hit is the top of
//! the pile there. A film lies where that top is a top: it is hit, it is not
//! steeper than a film lies, and no lip, face or slit of the stones is
//! within [`FILM_INSET_MM`] of it. So it never runs down a face or into the
//! gap between two stones, and it is never laid where no stone stands.

use dt_kit::rays::RayMesh;
use dt_kit::{Frame, Mask};
use rayon::prelude::*;

/// The steepest top a dirt film lies on, degrees.
pub const FILM_SLOPE_DEG: f64 = 35.0;
/// How far a film stands back from a lip, a face or a slit, mm.
pub const FILM_INSET_MM: f64 = 1.5;
/// A step between neighbouring nodes this deep is a lip, mm.
const LIP_MM: f64 = 1.0;
/// How far the smoothing that measures the slope reaches, nodes.
const SMOOTH_NODES: usize = 2;

/// The top of the pile at every node (mm, as the stones stand, `-∞` where
/// no stone is hit): a ray down from above onto each stone.
pub fn stone_tops(frame: &Frame, stones: &[RayMesh], foot: f64) -> Vec<f64> {
    let (nx, ny) = frame.nodes();
    let step = frame.step_mm();
    let per_stone: Vec<Vec<(usize, f64)>> = stones
        .par_iter()
        .map(|m| {
            let (lo, hi) = m.bounds();
            let range = |a: f64, b: f64, n: usize| {
                let first = (a / step).ceil().max(0.0) as usize;
                let last = ((b / step).floor().max(-1.0) + 1.0).min(n as f64) as usize;
                first..last
            };
            let mut at = Vec::new();
            let mut origins = Vec::new();
            for j in range(lo[1], hi[1], ny) {
                for i in range(lo[0], hi[0], nx) {
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

/// Where a film may lie: on the pile's tops (`tops`) that are flat enough,
/// clear of lips, faces and slits, inside `area` and over the core.
pub fn film_ground(frame: &Frame, tops: &[f64], area: &Mask, core: &Mask) -> Mask {
    let (nx, ny) = frame.nodes();
    let step = frame.step_mm();
    let hit = |k: usize| tops[k].is_finite();
    // the pile's top smoothed over the nodes that are hit: its slope is the
    // slope of the stone's top, not of its grain
    let smooth = |k: usize| -> f64 {
        let (i, j) = (k % nx, k / nx);
        let (mut sum, mut n) = (0.0, 0.0);
        for jj in j.saturating_sub(SMOOTH_NODES)..=(j + SMOOTH_NODES).min(ny - 1) {
            for ii in i.saturating_sub(SMOOTH_NODES)..=(i + SMOOTH_NODES).min(nx - 1) {
                let q = jj * nx + ii;
                if hit(q) {
                    sum += tops[q];
                    n += 1.0;
                }
            }
        }
        sum / n
    };
    let smoothed: Vec<f64> = (0..nx * ny)
        .map(|k| if hit(k) { smooth(k) } else { f64::NEG_INFINITY })
        .collect();
    let tan = libm::tan(FILM_SLOPE_DEG.to_radians());
    let mut bad = vec![false; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let k = j * nx + i;
            if !hit(k) {
                bad[k] = true;
                continue;
            }
            let mut lip = false;
            let mut gx = 0.0;
            let mut gy = 0.0;
            for (di, dj) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (ni, nj) = (i as i64 + di, j as i64 + dj);
                if ni < 0 || nj < 0 || ni >= nx as i64 || nj >= ny as i64 {
                    continue;
                }
                let q = nj as usize * nx + ni as usize;
                if !hit(q) || (tops[q] - tops[k]).abs() > LIP_MM {
                    lip = true;
                    break;
                }
                let d = (smoothed[q] - smoothed[k]) / step;
                if di != 0 {
                    gx += d * di as f64 * 0.5;
                } else {
                    gy += d * dj as f64 * 0.5;
                }
            }
            bad[k] = lip || libm::hypot(gx, gy) > tan;
        }
    }
    let (to_bad, _) = dt_kit::distance(frame, &bad);
    let not_core: Vec<bool> = core.bits.iter().map(|&b| !b).collect();
    let (off_core, _) = dt_kit::distance(frame, &not_core);
    let mut flat = Mask::new(nx, ny, false);
    for k in 0..nx * ny {
        flat.bits[k] = hit(k)
            && area.bits[k]
            && core.bits[k]
            && to_bad.z[k] >= FILM_INSET_MM
            && off_core.z[k] >= FILM_INSET_MM;
    }
    flat
}
