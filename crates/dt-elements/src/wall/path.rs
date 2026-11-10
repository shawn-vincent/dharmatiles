//! A drawn line as a wall's spine: the line as the tile has it (wobble
//! and all) made into a few straight runs, set on the side `side=` asks
//! for, its ends at the tile's edge cut clean.

use crate::{LineSide, LookError};

/// How far the straight runs may stray from the drawn line, mm at the
/// 35 mm square.
const TOLERANCE_MM: f64 = 0.35;
/// How close to a tile edge a line's end must lie to be at it, mm.
const AT_EDGE_MM: f64 = 1e-3;
/// A line that meets an edge within this many degrees of square to it ends
/// flush with the edge, as a wall on the edge does.
const SQUARE_DEG: f64 = 1.0;

/// The spine of a drawn wall.
pub(crate) struct Spine {
    /// Its points, mm, the outer face with the body to its left. A ring
    /// does not repeat its first point.
    pub pts: Vec<[f64; 2]>,
    /// Whether the last run returns to the first.
    pub ring: bool,
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    libm::hypot(a[0] - b[0], a[1] - b[1])
}

/// Distance of `p` from the chord `a`..`b`.
fn off_chord(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (v, w) = ([b[0] - a[0], b[1] - a[1]], [p[0] - a[0], p[1] - a[1]]);
    let len = libm::hypot(v[0], v[1]);
    if len < 1e-12 {
        return libm::hypot(w[0], w[1]);
    }
    (v[0] * w[1] - v[1] * w[0]).abs() / len
}

/// Douglas-Peucker: the indices (ascending, ends included) of `pts[lo..=hi]`
/// that keep the line within `tol`.
fn simplify(pts: &[[f64; 2]], lo: usize, hi: usize, tol: f64, keep: &mut Vec<usize>) {
    let mut far = (0.0, lo);
    for (i, p) in pts.iter().enumerate().take(hi).skip(lo + 1) {
        let d = off_chord(*p, pts[lo], pts[hi]);
        if d > far.0 {
            far = (d, i);
        }
    }
    if far.0 > tol {
        simplify(pts, lo, far.1, tol, keep);
        keep.push(far.1);
        simplify(pts, far.1, hi, tol, keep);
    }
}

/// How sharply the line turns at `pts[i]`, radians.
fn turn(pts: &[[f64; 2]], i: usize, ring: bool) -> f64 {
    let n = pts.len();
    let (p, q, r) = if ring {
        (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n])
    } else if i == 0 || i + 1 == n {
        return f64::INFINITY;
    } else {
        (pts[i - 1], pts[i], pts[i + 1])
    };
    let (a, b) = ([q[0] - p[0], q[1] - p[1]], [r[0] - q[0], r[1] - q[1]]);
    libm::atan2((a[0] * b[1] - a[1] * b[0]).abs(), a[0] * b[0] + a[1] * b[1])
}

/// The line's points `pts` as a spine for a wall `across` mm thick on a
/// tile of `tile` mm, at `scale` to the 35 mm square. `min_run` is the
/// shortest straight run worth a stone.
///
/// # Errors
/// A line too short, or turning too tightly for the wall's thickness.
pub(crate) fn drawn(
    line: &[[f64; 2]],
    side: LineSide,
    across: f64,
    scale: f64,
    tile: [f64; 2],
    min_run: f64,
) -> Result<Spine, LookError> {
    let mut pts: Vec<[f64; 2]> = Vec::with_capacity(line.len());
    for &p in line {
        if pts.last().is_none_or(|&l| dist(l, p) > 1e-4) {
            pts.push(p);
        }
    }
    let ring = pts.len() >= 4 && dist(pts[0], pts[pts.len() - 1]) < 1e-3;
    if ring {
        pts.pop();
    }
    if pts.len() < 2 || (ring && pts.len() < 3) {
        return Err(LookError::general(
            "this line is too short for a wall to follow",
            "draw the line through two or more points",
        ));
    }
    // which ends lie on a tile edge (before anything moves them)
    let edge = |p: [f64; 2]| -> Option<(usize, f64)> {
        (0..2).find_map(|k| {
            if p[k].abs() < AT_EDGE_MM {
                Some((k, -1.0))
            } else if (p[k] - tile[k]).abs() < AT_EDGE_MM {
                Some((k, 1.0))
            } else {
                None
            }
        })
    };
    let mut ends = (edge(pts[0]), edge(pts[pts.len() - 1]));
    if ring {
        ends = (None, None);
    }

    // a few straight runs
    let tol = TOLERANCE_MM * scale;
    let mut keep = vec![0];
    if ring {
        let far = (1..pts.len())
            .max_by(|&a, &b| dist(pts[0], pts[a]).total_cmp(&dist(pts[0], pts[b])))
            .unwrap_or(1);
        simplify(&pts, 0, far, tol, &mut keep);
        keep.push(far);
        let mut closed = pts.clone();
        closed.push(pts[0]);
        let mut back = vec![far];
        simplify(&closed, far, pts.len(), tol, &mut back);
        keep.extend(back.into_iter().skip(1));
    } else {
        simplify(&pts, 0, pts.len() - 1, tol, &mut keep);
        keep.push(pts.len() - 1);
    }
    let mut pts: Vec<[f64; 2]> = keep.into_iter().map(|i| pts[i]).collect();
    // a run shorter than a stone goes: the vertex it ends at that turns
    // least is dropped
    let least = if ring { 3 } else { 2 };
    while pts.len() > least {
        let n = pts.len();
        let runs = if ring { n } else { n - 1 };
        let Some(k) = (0..runs)
            .filter(|&k| dist(pts[k], pts[(k + 1) % n]) < min_run)
            .min_by(|&a, &b| {
                dist(pts[a], pts[(a + 1) % n]).total_cmp(&dist(pts[b], pts[(b + 1) % n]))
            })
        else {
            break;
        };
        let ends_of = [k, (k + 1) % n];
        let drop = ends_of
            .into_iter()
            .filter(|&i| ring || (i != 0 && i + 1 != n))
            .min_by(|&a, &b| turn(&pts, a, ring).total_cmp(&turn(&pts, b, ring)));
        match drop {
            Some(i) => {
                pts.remove(i);
            }
            None => break,
        }
    }

    // the side: the spine is the outer face, the body to its left
    let reversed = side == LineSide::Right;
    if reversed {
        pts.reverse();
        ends = (ends.1, ends.0);
    }
    if side == LineSide::Centre {
        pts = offset(&pts, ring, -0.5 * across)?;
    }

    // an end on a tile edge that the line meets slantwise runs on past it,
    // so the tile's cut is the wall's end
    if !ring {
        let n = pts.len();
        let outward = |p: [f64; 2], q: [f64; 2]| {
            let l = dist(p, q).max(1e-12);
            [(p[0] - q[0]) / l, (p[1] - q[1]) / l]
        };
        let ends = [
            (0, ends.0, outward(pts[0], pts[1])),
            (n - 1, ends.1, outward(pts[n - 1], pts[n - 2])),
        ];
        for (i, edge, u) in ends {
            let Some((k, sign)) = edge else { continue };
            let cos = sign * u[k];
            if cos >= libm::cos(SQUARE_DEG.to_radians()) || cos < 0.2 {
                continue;
            }
            // the body is to the left of the way the spine is walked
            let walk = if i == 0 { [-u[0], -u[1]] } else { u };
            let left_k = if k == 0 { -walk[1] } else { walk[0] };
            let plane = if sign > 0.0 { tile[k] } else { 0.0 };
            let outside = sign * (pts[i][k] - plane);
            let lowest = outside + (across * sign * left_k).min(0.0);
            let need = (0.5 * scale - lowest) / cos;
            if need > 0.0 {
                pts[i] = [pts[i][0] + u[0] * need, pts[i][1] + u[1] * need];
            }
        }
    }
    Ok(Spine { pts, ring })
}

/// The polyline `pts` pushed `d` mm to its left (a negative `d`: to its
/// right), the corners mitred.
fn offset(pts: &[[f64; 2]], ring: bool, d: f64) -> Result<Vec<[f64; 2]>, LookError> {
    let n = pts.len();
    let runs = if ring { n } else { n - 1 };
    let dir = |k: usize| {
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        let l = dist(a, b).max(1e-12);
        [(b[0] - a[0]) / l, (b[1] - a[1]) / l]
    };
    let mut out = Vec::with_capacity(n);
    for (i, p) in pts.iter().enumerate() {
        let (before, after) = if ring {
            (Some(dir((i + n - 1) % n)), Some(dir(i)))
        } else {
            ((i > 0).then(|| dir(i - 1)), (i < runs).then(|| dir(i)))
        };
        let normal = |u: [f64; 2]| [-u[1], u[0]];
        let shift = match (before, after) {
            (Some(a), Some(b)) => {
                let (na, nb) = (normal(a), normal(b));
                let sum = [na[0] + nb[0], na[1] + nb[1]];
                let k = 1.0 + na[0] * nb[0] + na[1] * nb[1];
                if k < 1e-6 {
                    return Err(too_tight());
                }
                [sum[0] / k * d, sum[1] / k * d]
            }
            (Some(u), None) | (None, Some(u)) => {
                let nu = normal(u);
                [nu[0] * d, nu[1] * d]
            }
            (None, None) => [0.0, 0.0],
        };
        out.push([p[0] + shift[0], p[1] + shift[1]]);
    }
    // a run turned over by the offset means the line turns inside the
    // wall's thickness
    for k in 0..runs {
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        let (c, e) = (out[k], out[(k + 1) % n]);
        if (b[0] - a[0]) * (e[0] - c[0]) + (b[1] - a[1]) * (e[1] - c[1]) <= 0.0 {
            return Err(too_tight());
        }
    }
    Ok(out)
}

fn too_tight() -> LookError {
    LookError::general(
        "the line turns too tightly for a wall this thick to stand on its centre",
        "ease the bend, make the wall thinner, or write side=left or side=right",
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_curve_becomes_a_few_runs_and_a_side_moves_it() {
        let arc: Vec<[f64; 2]> = (0..=40)
            .map(|i| {
                let a = f64::from(i) * 0.04;
                [
                    10.0 + 30.0 * libm::sin(a),
                    10.0 + 30.0 * (1.0 - libm::cos(a)),
                ]
            })
            .collect();
        let left = drawn(&arc, LineSide::Left, 7.0, 1.0, [70.0, 70.0], 6.0).unwrap();
        assert!(left.pts.len() > 3 && left.pts.len() < 12);
        let right = drawn(&arc, LineSide::Right, 7.0, 1.0, [70.0, 70.0], 6.0).unwrap();
        assert!(dist(right.pts[0], left.pts[left.pts.len() - 1]) < 1e-9);
        let mid = drawn(&arc, LineSide::Centre, 7.0, 1.0, [70.0, 70.0], 6.0).unwrap();
        assert!((dist(mid.pts[0], left.pts[0]) - 3.5).abs() < 1e-9);
    }

    #[test]
    fn a_closed_line_is_a_ring() {
        let sq = [
            [10.0, 10.0],
            [40.0, 10.0],
            [40.0, 40.0],
            [10.0, 40.0],
            [10.0, 10.0],
        ];
        let r = drawn(&sq, LineSide::Left, 7.0, 1.0, [70.0, 70.0], 6.0).unwrap();
        assert!(r.ring && r.pts.len() == 4);
    }

    #[test]
    fn a_slant_end_on_an_edge_runs_past_it() {
        let line = [[0.0, 10.0], [30.0, 20.0]];
        let s = drawn(&line, LineSide::Left, 7.0, 1.0, [70.0, 70.0], 6.0).unwrap();
        assert!(s.pts[0][0] < -0.4);
        let square = [[0.0, 10.0], [30.0, 10.0]];
        let s = drawn(&square, LineSide::Left, 7.0, 1.0, [70.0, 70.0], 6.0).unwrap();
        assert!(s.pts[0][0].abs() < 1e-9);
    }
}
