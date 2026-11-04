//! Polylines in a face's (u, z) plane: the few shapely calls the crack
//! network needs.

use super::util::{P2, seg_dist};

#[derive(Clone, Debug)]
pub struct Line {
    pub pts: Vec<P2>,
    cum: Vec<f64>,
}

fn d(a: P2, b: P2) -> f64 {
    libm::hypot(a[0] - b[0], a[1] - b[1])
}

impl Line {
    pub fn new(pts: Vec<P2>) -> Self {
        let mut cum = vec![0.0];
        for k in 1..pts.len() {
            cum.push(cum[k - 1] + d(pts[k - 1], pts[k]));
        }
        Self { pts, cum }
    }
    pub fn length(&self) -> f64 {
        *self.cum.last().unwrap_or(&0.0)
    }
    pub fn first(&self) -> P2 {
        self.pts[0]
    }
    pub fn last(&self) -> P2 {
        self.pts[self.pts.len() - 1]
    }
    /// The point `s` along it (clamped).
    pub fn at(&self, s: f64) -> P2 {
        let s = s.clamp(0.0, self.length());
        let k = self
            .cum
            .partition_point(|&c| c < s)
            .clamp(1, self.pts.len() - 1);
        let (a, b) = (self.pts[k - 1], self.pts[k]);
        let l = self.cum[k] - self.cum[k - 1];
        let t = if l > 0.0 {
            (s - self.cum[k - 1]) / l
        } else {
            0.0
        };
        [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
    }
    /// Arc length of the point on it nearest `p`.
    pub fn project(&self, p: P2) -> f64 {
        let mut best = (f64::INFINITY, 0.0);
        for k in 1..self.pts.len() {
            let (a, b) = (self.pts[k - 1], self.pts[k]);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let l2 = ab[0] * ab[0] + ab[1] * ab[1];
            let t = if l2 > 0.0 {
                (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = [a[0] + t * ab[0], a[1] + t * ab[1]];
            let dd = d(p, q);
            if dd < best.0 {
                best = (dd, self.cum[k - 1] + t * l2.sqrt());
            }
        }
        best.1
    }
    pub fn dist(&self, p: P2) -> f64 {
        (1..self.pts.len())
            .map(|k| seg_dist(p, self.pts[k - 1], self.pts[k]))
            .fold(f64::INFINITY, f64::min)
    }
    /// Direction there: the chord 0.3 either side of `s`.
    pub fn chord(&self, s: f64) -> P2 {
        let (a, b) = (self.at(s - 0.3), self.at(s + 0.3));
        [b[0] - a[0], b[1] - a[1]]
    }
    /// Where the segment a-b crosses it (all crossings).
    pub fn cross(&self, a: P2, b: P2) -> Vec<P2> {
        let mut out = Vec::new();
        for k in 1..self.pts.len() {
            if let Some(p) = seg_cross(a, b, self.pts[k - 1], self.pts[k]) {
                out.push(p);
            }
        }
        out
    }
    /// Distance from the segment a-b.
    pub fn dist_seg(&self, a: P2, b: P2) -> f64 {
        self.closest(a, b).0
    }
    /// (distance, point on the segment, point on the line).
    pub fn closest(&self, a: P2, b: P2) -> (f64, P2, P2) {
        let mut best = (f64::INFINITY, a, self.pts[0]);
        for k in 1..self.pts.len() {
            let r = seg_seg(a, b, self.pts[k - 1], self.pts[k]);
            if r.0 < best.0 {
                best = r;
            }
        }
        best
    }
}

fn seg_cross(a: P2, b: P2, c: P2, e: P2) -> Option<P2> {
    let r = [b[0] - a[0], b[1] - a[1]];
    let s = [e[0] - c[0], e[1] - c[1]];
    let den = r[0] * s[1] - r[1] * s[0];
    if den.abs() < 1e-14 {
        return None;
    }
    let t = ((c[0] - a[0]) * s[1] - (c[1] - a[1]) * s[0]) / den;
    let u = ((c[0] - a[0]) * r[1] - (c[1] - a[1]) * r[0]) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u))
        .then(|| [a[0] + t * r[0], a[1] + t * r[1]])
}

/// Closest points of two segments: (distance, on 1, on 2).
fn seg_seg(a: P2, b: P2, c: P2, e: P2) -> (f64, P2, P2) {
    if let Some(p) = seg_cross(a, b, c, e) {
        return (0.0, p, p);
    }
    let mut best = (f64::INFINITY, a, c);
    for (p, q, r) in [(a, c, e), (b, c, e)] {
        let (dd, x) = pt_seg(p, q, r);
        if dd < best.0 {
            best = (dd, p, x);
        }
    }
    for (p, q, r) in [(c, a, b), (e, a, b)] {
        let (dd, x) = pt_seg(p, q, r);
        if dd < best.0 {
            best = (dd, x, p);
        }
    }
    best
}

fn pt_seg(p: P2, a: P2, b: P2) -> (f64, P2) {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 {
        (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = [a[0] + t * ab[0], a[1] + t * ab[1]];
    (d(p, q), q)
}
