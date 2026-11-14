//! **spine**: the smooth curve a drawn line is, through the points written.
//!
//! A line is one curve of cubic Bézier segments, one per pair of points. A
//! segment's handles are 0.4 of its chord long (times the point's `pull`), and
//! lie along the direction the line has at that point:
//!
//! * inside the line, the chord of the point's two neighbours (Catmull-Rom);
//! * at a `corner`, the chord itself on each side, so the knee stays sharp;
//! * at an end on a tile side, the side's normal; at an end on a tile corner,
//!   the corner's bisector ([`edge_heading`], the one place that decides it);
//! * where a point is given a `heading`, that.
//!
//! The curve passes through every point. Two tiles whose lines meet at the
//! same place on a side meet in the same direction. All lengths share one
//! unit (squares, in the evaluator), with y running north.

/// How long a Bézier handle is, as a share of its chord.
const HANDLE: f64 = 0.4;

/// A point of a line and what is written of its direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knot {
    /// Where it is.
    pub at: [f64; 2],
    /// A sharp knee: the line arrives and leaves along its chords.
    pub corner: bool,
    /// The direction (unit) the line goes through here, when written.
    pub heading: Option<[f64; 2]>,
    /// How far the handles reach, as a share of the usual (1 by default).
    pub pull: f64,
}

impl Knot {
    /// A plain point.
    #[must_use]
    pub fn at(at: [f64; 2]) -> Self {
        Self {
            at,
            corner: false,
            heading: None,
            pull: 1.0,
        }
    }
}

/// Where a point must lie to count as on a tile's side or corner: exactly,
/// to this much (the units of the tile).
const ON_EDGE: f64 = 1e-9;

/// The direction a line goes into a `w` x `h` tile from `p`, when `p` is on
/// its outline: the inward normal on a side, the inward bisector (45 degrees to
/// both sides) on a corner. `None` for any other point. A line leaves by the
/// opposite of this at its last point.
#[must_use]
pub fn edge_heading(p: [f64; 2], w: f64, h: f64) -> Option<[f64; 2]> {
    let axis = |v: f64, size: f64| -> f64 {
        if v.abs() <= ON_EDGE {
            1.0
        } else if (v - size).abs() <= ON_EDGE {
            -1.0
        } else {
            0.0
        }
    };
    let (dx, dy) = (axis(p[0], w), axis(p[1], h));
    if dx == 0.0 && dy == 0.0 {
        return None;
    }
    let n = libm::hypot(dx, dy);
    Some([dx / n, dy / n])
}

fn unit(v: [f64; 2]) -> Option<[f64; 2]> {
    let l = libm::hypot(v[0], v[1]);
    (l > 1e-12 && l.is_finite()).then(|| [v[0] / l, v[1] / l])
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn length(v: [f64; 2]) -> f64 {
    libm::hypot(v[0], v[1])
}

/// The curve through `knots`, sampled every `step` or less along it, the knots
/// themselves among the samples. `ends` is the direction of travel written for
/// the first and the last point (`None`: from the neighbours); a `closed` line
/// has no ends and its last sample is its first point again.
#[must_use]
pub fn spine(
    knots: &[Knot],
    closed: bool,
    ends: [Option<[f64; 2]>; 2],
    step: f64,
) -> Vec<[f64; 2]> {
    let n = knots.len();
    if n < 2 || !step.is_finite() || step <= 0.0 {
        return knots.iter().map(|k| k.at).collect();
    }
    let segs = if closed { n } else { n - 1 };
    let at = |i: usize| knots[i % n].at;
    let chord = |i: usize| unit(sub(at(i + 1), at(i)));
    // the direction through each point
    let mut dir: Vec<Option<[f64; 2]>> = (0..n)
        .map(|i| {
            if closed || (i > 0 && i + 1 < n) {
                unit(sub(at(i + 1), at(i + n - 1))).or_else(|| chord(i))
            } else {
                None
            }
        })
        .collect();
    if !closed {
        // a free end: the neighbour's direction mirrored in the chord
        let mirrored = |c: Option<[f64; 2]>, d: Option<[f64; 2]>| -> Option<[f64; 2]> {
            let c = c?;
            match d {
                Some(d) => {
                    let k = 2.0 * (c[0] * d[0] + c[1] * d[1]);
                    unit([k * c[0] - d[0], k * c[1] - d[1]])
                }
                None => Some(c),
            }
        };
        let first = mirrored(chord(0), dir.get(1).copied().flatten());
        let last = mirrored(
            unit(sub(at(n - 1), at(n - 2))),
            dir.get(n - 2).copied().flatten(),
        );
        dir[0] = first;
        dir[n - 1] = last;
        if let Some(e) = ends[0] {
            dir[0] = Some(e);
        }
        if let Some(e) = ends[1] {
            dir[n - 1] = Some(e);
        }
    }
    for (d, k) in dir.iter_mut().zip(knots) {
        if let Some(h) = k.heading {
            *d = Some(h);
        }
    }
    let mut out = vec![at(0)];
    for s in 0..segs {
        let (a, b) = (s, s + 1);
        let (p0, p3) = (at(a), at(b));
        let l = length(sub(p3, p0));
        if l <= 1e-12 {
            continue;
        }
        let c = chord(a).unwrap_or([1.0, 0.0]);
        let leave = if knots[a % n].corner {
            c
        } else {
            dir[a % n].unwrap_or(c)
        };
        let arrive = if knots[b % n].corner {
            c
        } else {
            dir[b % n].unwrap_or(c)
        };
        let (ha, hb) = (
            HANDLE * l * knots[a % n].pull,
            HANDLE * l * knots[b % n].pull,
        );
        let p1 = [p0[0] + ha * leave[0], p0[1] + ha * leave[1]];
        let p2 = [p3[0] - hb * arrive[0], p3[1] - hb * arrive[1]];
        let poly = length(sub(p1, p0)) + length(sub(p2, p1)) + length(sub(p3, p2));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = (libm::ceil(poly / step) as usize).max(1);
        for i in 1..=k {
            if i == k {
                out.push(p3);
                break;
            }
            #[allow(clippy::cast_precision_loss)]
            let t = i as f64 / k as f64;
            let u = 1.0 - t;
            let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            out.push([
                w0 * p0[0] + w1 * p1[0] + w2 * p2[0] + w3 * p3[0],
                w0 * p0[1] + w1 * p1[1] + w2 * p2[1] + w3 * p3[1],
            ]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knots(p: &[[f64; 2]]) -> Vec<Knot> {
        p.iter().map(|&q| Knot::at(q)).collect()
    }

    fn near(a: [f64; 2], b: [f64; 2], e: f64) -> bool {
        length(sub(a, b)) <= e
    }

    #[test]
    fn edge_headings_are_normals_and_bisectors() {
        assert_eq!(edge_heading([0.0, 0.4], 1.0, 1.0), Some([1.0, 0.0]));
        assert_eq!(edge_heading([1.0, 0.4], 3.0, 2.0), None);
        assert_eq!(edge_heading([3.0, 0.4], 3.0, 2.0), Some([-1.0, 0.0]));
        assert_eq!(edge_heading([0.6, 2.0], 3.0, 2.0), Some([0.0, -1.0]));
        let c = edge_heading([0.0, 0.0], 1.0, 1.0).unwrap();
        assert!((c[0] - c[1]).abs() < 1e-15 && (length(c) - 1.0).abs() < 1e-15);
        // a square corner partway along a bigger tile's side is a side
        assert_eq!(edge_heading([1.0, 0.0], 2.0, 2.0), Some([0.0, 1.0]));
        // a mirrored tile's point has the mirrored heading
        let (a, b) = (
            edge_heading([0.0, 0.43], 1.0, 1.0).unwrap(),
            edge_heading([1.0, 0.43], 1.0, 1.0).unwrap(),
        );
        assert_eq!([a[0], a[1]], [-b[0], b[1]]);
    }

    #[test]
    fn it_passes_through_every_point() {
        let pts = [[0.0, 0.3], [0.4, 0.5], [0.7, 0.2], [1.0, 0.6]];
        let out = spine(
            &knots(&pts),
            false,
            [Some([1.0, 0.0]), Some([1.0, 0.0])],
            0.01,
        );
        for p in pts {
            assert!(out.iter().any(|q| near(*q, p, 1e-12)), "{p:?}");
        }
        for w in out.windows(2) {
            assert!(length(sub(w[1], w[0])) <= 0.0101);
        }
    }

    #[test]
    fn an_end_leaves_in_the_direction_given() {
        let pts = [[0.0, 0.4], [0.6, 1.0]];
        let i = edge_heading(pts[0], 1.0, 1.0);
        let o = edge_heading(pts[1], 1.0, 1.0).map(|d| [-d[0], -d[1]]);
        let out = spine(&knots(&pts), false, [i, o], 1e-6);
        // the tangent of a cubic is its handle: the first samples lie on the normal
        let d0 = unit(sub(out[1], out[0])).unwrap();
        assert!(length(sub(d0, [1.0, 0.0])) < 1e-6, "{d0:?}");
        let m = out.len();
        let d1 = unit(sub(out[m - 1], out[m - 2])).unwrap();
        assert!(length(sub(d1, [0.0, 1.0])) < 1e-6, "{d1:?}");
    }

    #[test]
    fn a_corner_keeps_its_knee() {
        let pts = [[0.0, 0.0], [0.5, 0.5], [1.0, 0.0]];
        let mut k = knots(&pts);
        k[1].corner = true;
        let out = spine(&k, false, [None, None], 0.002);
        let i = out.iter().position(|q| near(*q, pts[1], 1e-12)).unwrap();
        let before = unit(sub(out[i], out[i - 1])).unwrap();
        let after = unit(sub(out[i + 1], out[i])).unwrap();
        // the segments come in and go out along their straight chords
        assert!(length(sub(before, unit([1.0, 1.0]).unwrap())) < 1e-2);
        assert!(length(sub(after, unit([1.0, -1.0]).unwrap())) < 1e-2);
        // and without the corner it is smooth there
        let smooth = spine(&knots(&pts), false, [None, None], 0.002);
        let i = smooth.iter().position(|q| near(*q, pts[1], 1e-12)).unwrap();
        let (b, a) = (
            unit(sub(smooth[i], smooth[i - 1])).unwrap(),
            unit(sub(smooth[i + 1], smooth[i])).unwrap(),
        );
        assert!(length(sub(b, a)) < 1e-2);
    }

    #[test]
    fn a_heading_and_a_pull_bend_the_curve() {
        let pts = [[0.0, 0.0], [1.0, 0.0]];
        let mut k = knots(&pts);
        k[0].heading = Some([0.0, 1.0]);
        let bent = spine(&k, false, [None, None], 0.01);
        assert!(bent[1][1] > bent[1][0]);
        k[0].pull = 0.2;
        let slack = spine(&k, false, [None, None], 0.01);
        let rise = |c: &[[f64; 2]]| c.iter().map(|p| p[1]).fold(0.0, f64::max);
        assert!(rise(&slack) < rise(&bent));
    }

    #[test]
    fn a_closed_line_closes() {
        let pts = [[0.5, 0.7], [0.72, 0.5], [0.5, 0.28], [0.28, 0.5]];
        let out = spine(&knots(&pts), true, [None, None], 0.01);
        assert!(near(out[0], *out.last().unwrap(), 1e-12));
        assert!(out.len() > 40);
    }
}
