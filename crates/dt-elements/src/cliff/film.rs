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
use dt_kit::{Field, Frame, Mask, OnFrame};
use rayon::prelude::*;

/// The steepest top a dirt film lies on, degrees (an angle: it never scales).
pub const FILM_SLOPE_DEG: f64 = 35.0;
/// How far a film stands back from a lip, a face or a slit, mm at the
/// reference square (scaled with the system like every size an author writes).
pub const FILM_INSET_MM: f64 = 1.5;
/// A step between neighbouring nodes this deep is a lip, mm at the reference
/// square (scaled).
const LIP_MM: f64 = 1.0;
/// How far the smoothing that measures the slope reaches, nodes.
const SMOOTH_NODES: usize = 2;
/// A gap this narrow between two places a film may not lie, nodes, is closed:
/// a slit finer than the lattice hits and misses from node to node, and
/// without it the inset would follow that alternation as a comb.
const CLOSE_NODES: f64 = 1.5;
/// How far the outline of a film patch is rounded, nodes (the blur's sigma).
const EDGE_SMOOTH_NODES: f64 = 2.0;

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

/// Where a film may lie: on the pile's tops (`tops`) that are flat enough,
/// clear of lips, faces and slits, inside `area` and over the core. `scale`
/// is the system's (its mm sizes shrink with it, its angles do not).
pub fn film_ground(frame: &Frame, tops: &[f64], area: &Mask, core: &Mask, scale: f64) -> Mask {
    let (nx, ny) = frame.nodes();
    let step = frame.step_mm();
    let hit = |k: usize| tops[k].is_finite();
    let smoothed = smoothed_tops(frame, tops);
    let (gx, gy) = smoothed.gradient();
    let (lip_mm, inset_mm) = (LIP_MM * scale, FILM_INSET_MM * scale);
    let tan = libm::tan(FILM_SLOPE_DEG.to_radians());
    let mut bad = vec![false; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let k = j * nx + i;
            // a lip: a neighbour with no stone, or a step to it deeper than a film lies over
            let lip = |(di, dj): (i64, i64)| {
                let (ni, nj) = (i as i64 + di, j as i64 + dj);
                if ni < 0 || nj < 0 || ni >= nx as i64 || nj >= ny as i64 {
                    return false;
                }
                let q = nj as usize * nx + ni as usize;
                !hit(q) || (tops[q] - tops[k]).abs() > lip_mm
            };
            bad[k] = !hit(k)
                || [(-1, 0), (1, 0), (0, -1), (0, 1)].into_iter().any(lip)
                || libm::hypot(gx.z[k], gy.z[k]) > tan;
        }
    }
    let bad = close(frame, &bad, CLOSE_NODES * step);
    let (to_bad, _) = dt_kit::distance(frame, &bad);
    let not_core: Vec<bool> = core.bits.iter().map(|&b| !b).collect();
    let (off_core, _) = dt_kit::distance(frame, &not_core);
    let allowed = |k: usize| hit(k) && area.bits[k] && core.bits[k];
    let mut flat = Mask::new(nx, ny, false);
    for k in 0..nx * ny {
        flat.bits[k] = allowed(k) && to_bad.z[k] >= inset_mm && off_core.z[k] >= inset_mm;
    }
    // an edge that follows the lattice is a staircase, and one that follows
    // a slit finer than the lattice a comb: round the patches' outlines
    let soft = Field::new(frame, 0.0)
        .with_z(flat.bits.iter().map(|&b| f64::from(u8::from(b))).collect())
        .blur(EDGE_SMOOTH_NODES * step);
    for k in 0..nx * ny {
        flat.bits[k] = allowed(k) && soft.z[k] >= 0.5;
    }
    flat
}

/// The pile's top smoothed over the nodes that are hit, so that its slope is
/// the slope of the stone's top, not of its grain. Nodes with no stone hold 0
/// (they are lips, and never read for a slope).
fn smoothed_tops(frame: &Frame, tops: &[f64]) -> Field {
    let (nx, ny) = frame.nodes();
    let hit = |k: usize| tops[k].is_finite();
    let z = (0..nx * ny)
        .map(|k| {
            if !hit(k) {
                return 0.0;
            }
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
        })
        .collect();
    Field::new(frame, 0.0).with_z(z)
}

/// `mask` closed by `r_mm`: grown by it, then shrunk by it, so a gap narrower
/// than twice that between two set places is filled and the rest is as it was.
fn close(frame: &Frame, mask: &[bool], r_mm: f64) -> Vec<bool> {
    let (d, _) = dt_kit::distance(frame, mask);
    let grown: Vec<bool> = d.z.iter().map(|&v| v <= r_mm).collect();
    let rest: Vec<bool> = grown.iter().map(|&b| !b).collect();
    let (d, _) = dt_kit::distance(frame, &rest);
    d.z.iter().map(|&v| v > r_mm).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat pile over the whole 1x1 tile, a film area whose east edge is a
    /// comb (every other row one node further out), and the core everywhere.
    fn comb() -> (Frame, Mask) {
        let frame = Frame::dungeonblocks(1, 1);
        let (nx, ny) = frame.nodes();
        let tops = vec![10.0; nx * ny];
        let mut area = Mask::new(nx, ny, false);
        for j in 0..ny {
            for i in 0..nx {
                area.bits[j * nx + i] = i < 90 + (j % 2) * 2;
            }
        }
        let core = Mask::new(nx, ny, true);
        let flat = film_ground(&frame, &tops, &area, &core, 1.0);
        (frame, flat)
    }

    /// The east-most node of the film in each row away from the tile's rim.
    fn east_edge(frame: &Frame, flat: &Mask) -> Vec<usize> {
        let (nx, ny) = frame.nodes();
        (30..ny - 30)
            .filter_map(|j| (0..nx).rev().find(|&i| flat.bits[j * nx + i]))
            .collect()
    }

    #[test]
    fn a_comb_in_the_outline_is_rounded_off() {
        let (frame, flat) = comb();
        let edge = east_edge(&frame, &flat);
        assert!(edge.len() > 50, "there is a film");
        let (lo, hi) = (edge.iter().min().unwrap(), edge.iter().max().unwrap());
        assert!(hi - lo <= 1, "the edge wanders {lo}..{hi} nodes");
    }

    #[test]
    fn a_slit_narrower_than_the_lattice_is_one_gap_not_a_comb() {
        let frame = Frame::dungeonblocks(1, 1);
        let (nx, ny) = frame.nodes();
        let mut tops = vec![10.0; nx * ny];
        // a slit that is hit on every other row only
        for j in (0..ny).step_by(2) {
            tops[j * nx + 60] = f64::NEG_INFINITY;
        }
        let area = Mask::new(nx, ny, true);
        let core = Mask::new(nx, ny, true);
        let flat = film_ground(&frame, &tops, &area, &core, 1.0);
        // the film stops the same distance from the slit in every row
        let west: Vec<usize> = (30..ny - 30)
            .filter_map(|j| (0..60).rev().find(|&i| flat.bits[j * nx + i]))
            .collect();
        let east: Vec<usize> = (30..ny - 30)
            .filter_map(|j| (60..nx).find(|&i| flat.bits[j * nx + i]))
            .collect();
        for edge in [west, east] {
            let (lo, hi) = (edge.iter().min().unwrap(), edge.iter().max().unwrap());
            assert!(hi - lo <= 1, "the edge wanders {lo}..{hi} nodes");
        }
    }

    #[test]
    fn the_inset_from_a_lip_scales_with_the_system() {
        let frame = Frame::dungeonblocks(1, 1);
        let (nx, ny) = frame.nodes();
        let step = frame.step_mm();
        let mut tops = vec![10.0; nx * ny];
        // a step down of 2 mm along x = node 60: a lip at any scale
        for j in 0..ny {
            for i in 61..nx {
                tops[j * nx + i] = 8.0;
            }
        }
        let area = Mask::new(nx, ny, true);
        let core = Mask::new(nx, ny, true);
        let reach = |scale: f64| {
            let flat = film_ground(&frame, &tops, &area, &core, scale);
            let j = ny / 2;
            let last = (0..60).rev().find(|&i| flat.bits[j * nx + i]).unwrap();
            (60 - last) as f64 * step
        };
        let (full, small) = (reach(1.0), reach(0.5));
        // the film stands the inset back from the lip
        assert!((full - FILM_INSET_MM).abs() < 3.0 * step, "{full}");
        assert!((small - 0.5 * FILM_INSET_MM).abs() < 3.0 * step, "{small}");
        assert!(small < full);
    }
}
