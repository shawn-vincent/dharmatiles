//! Small helpers the cliff shares: draws, 2D grids, polygons, segments.
//! Grids are row-major, `j * nx + i`, i along x (dt-geom's convention).

use dt_kit::{Footprint, Rng, distance_nearest};

/// Draws numpy's generator has and ours lacks.
pub trait Draw {
    fn pick(&mut self, p: &[f64]) -> usize;
    fn perm(&mut self, n: usize) -> Vec<usize>;
    fn signed(&mut self, lo_hi: (f64, f64)) -> f64;
    fn count(&mut self, lo_hi: (i64, i64)) -> i64;
}

impl Draw for Rng {
    fn pick(&mut self, p: &[f64]) -> usize {
        let t: f64 = p.iter().sum();
        let mut x = self.random() * t;
        for (i, v) in p.iter().enumerate() {
            if x < *v {
                return i;
            }
            x -= v;
        }
        p.len() - 1
    }
    fn perm(&mut self, n: usize) -> Vec<usize> {
        let mut v: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            let j = usize::try_from(self.int(0, i as i64)).unwrap_or(0);
            v.swap(i, j);
        }
        v
    }
    fn signed(&mut self, lo_hi: (f64, f64)) -> f64 {
        let s = if self.random() < 0.5 { -1.0 } else { 1.0 };
        s * self.uniform(lo_hi.0, lo_hi.1)
    }
    fn count(&mut self, lo_hi: (i64, i64)) -> i64 {
        self.int(lo_hi.0, lo_hi.1)
    }
}

pub fn lerp(lo_hi: (f64, f64), u: f64) -> f64 {
    lo_hi.0 + (lo_hi.1 - lo_hi.0) * u
}

/// A scalar grid, `nx` by `ny`.
#[derive(Clone, Debug)]
pub struct G<T> {
    pub nx: usize,
    pub ny: usize,
    pub v: Vec<T>,
}

impl<T: Copy> G<T> {
    pub fn new(nx: usize, ny: usize, x: T) -> Self {
        Self {
            nx,
            ny,
            v: vec![x; nx * ny],
        }
    }
    #[inline]
    pub fn at(&self, i: usize, j: usize) -> T {
        self.v[j * self.nx + i]
    }
    #[inline]
    pub fn set(&mut self, i: usize, j: usize, x: T) {
        self.v[j * self.nx + i] = x;
    }
    pub fn map<U: Copy>(&self, f: impl Fn(T) -> U) -> G<U> {
        G {
            nx: self.nx,
            ny: self.ny,
            v: self.v.iter().map(|&x| f(x)).collect(),
        }
    }
}

/// Cell-centre coordinates of cell (i, j).
#[inline]
pub fn cx(i: usize, grid_mm: f64) -> f64 {
    (i as f64 + 0.5) * grid_mm
}

pub type Mask = G<bool>;
pub type Field = G<f64>;

/// Distance (in cells) from every cell to the nearest `false` cell, as
/// scipy's `distance_transform_edt` of the mask.
pub fn edt_in(m: &Mask) -> Field {
    let inv: Vec<bool> = m.v.iter().map(|&b| !b).collect();
    let (d, _) = distance_nearest(&inv, m.nx, m.ny);
    Field {
        nx: m.nx,
        ny: m.ny,
        v: d.into_iter()
            .map(|x| if x.is_finite() { x } else { 1e9 })
            .collect(),
    }
}

/// Distance (cells) from every cell to the nearest `true` cell.
pub fn edt_out(m: &Mask) -> Field {
    let (d, _) = distance_nearest(&m.v, m.nx, m.ny);
    Field {
        nx: m.nx,
        ny: m.ny,
        v: d.into_iter()
            .map(|x| if x.is_finite() { x } else { 1e9 })
            .collect(),
    }
}

/// Grey dilation / erosion over a disc of radius `r` cells.
pub fn dilate_disc(f: &Field, r: f64) -> Field {
    Field {
        nx: f.nx,
        ny: f.ny,
        v: dt_kit::dilate2(&f.v, f.nx, f.ny, Footprint::Disc(r)),
    }
}
pub fn erode_disc(f: &Field, r: f64) -> Field {
    Field {
        nx: f.nx,
        ny: f.ny,
        v: dt_kit::erode2(&f.v, f.nx, f.ny, Footprint::Disc(r)),
    }
}
pub fn blur(f: &Field, sigma_cells: f64) -> Field {
    Field {
        nx: f.nx,
        ny: f.ny,
        v: dt_kit::blur2(&f.v, f.nx, f.ny, sigma_cells),
    }
}

/// Binary dilation by the 4-neighbour cross, `n` times.
pub fn bin_dilate(m: &Mask, n: usize) -> Mask {
    let mut cur = m.clone();
    for _ in 0..n {
        let mut next = cur.clone();
        for j in 0..m.ny {
            for i in 0..m.nx {
                if cur.at(i, j) {
                    continue;
                }
                let hit = (i > 0 && cur.at(i - 1, j))
                    || (i + 1 < m.nx && cur.at(i + 1, j))
                    || (j > 0 && cur.at(i, j - 1))
                    || (j + 1 < m.ny && cur.at(i, j + 1));
                if hit {
                    next.set(i, j, true);
                }
            }
        }
        cur = next;
    }
    cur
}

/// 4-connected labels (0 = background).
pub fn label(m: &Mask) -> (G<u32>, u32) {
    let mut labels = G::new(m.nx, m.ny, 0u32);
    let mut n = 0;
    let mut stack = Vec::new();
    for j in 0..m.ny {
        for i in 0..m.nx {
            if !m.at(i, j) || labels.at(i, j) != 0 {
                continue;
            }
            n += 1;
            labels.set(i, j, n);
            stack.push((i, j));
            while let Some((a, b)) = stack.pop() {
                let mut go = |x: usize, y: usize| {
                    if m.at(x, y) && labels.at(x, y) == 0 {
                        labels.set(x, y, n);
                        stack.push((x, y));
                    }
                };
                if a > 0 {
                    go(a - 1, b);
                }
                if a + 1 < m.nx {
                    go(a + 1, b);
                }
                if b > 0 {
                    go(a, b - 1);
                }
                if b + 1 < m.ny {
                    go(a, b + 1);
                }
            }
        }
    }
    (labels, n)
}

pub fn smooth01(t: f64) -> f64 {
    dt_kit::smooth01(t)
}

// ---- polygons ------------------------------------------------------------

pub type P2 = [f64; 2];

pub fn area(poly: &[P2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|k| {
            let (a, b) = (poly[k], poly[(k + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        * 0.5
}

pub fn centroid(poly: &[P2]) -> P2 {
    let n = poly.len();
    let (mut cx, mut cy, mut a2) = (0.0, 0.0, 0.0);
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        let c = a[0] * b[1] - b[0] * a[1];
        cx += (a[0] + b[0]) * c;
        cy += (a[1] + b[1]) * c;
        a2 += c;
    }
    if a2.abs() < 1e-12 {
        return poly[0];
    }
    [cx / (3.0 * a2), cy / (3.0 * a2)]
}

pub fn bounds(poly: &[P2]) -> [f64; 4] {
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in poly {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].min(p[1]);
        b[2] = b[2].max(p[0]);
        b[3] = b[3].max(p[1]);
    }
    b
}

/// Counter-clockwise (reversed if clockwise).
pub fn ccw(mut poly: Vec<P2>) -> Vec<P2> {
    if area(&poly) < 0.0 {
        poly.reverse();
    }
    poly
}

/// Andrew's monotone chain, counter-clockwise.
pub fn hull2(pts: &[P2]) -> Vec<P2> {
    let mut p: Vec<P2> = pts.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let cr = |o: P2, a: P2, b: P2| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut h: Vec<P2> = Vec::new();
    for &q in &p {
        while h.len() >= 2 && cr(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
            h.pop();
        }
        h.push(q);
    }
    let lo = h.len() + 1;
    for &q in p.iter().rev().skip(1) {
        while h.len() >= lo && cr(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
            h.pop();
        }
        h.push(q);
    }
    h.pop();
    h
}

/// The short side of the smallest rotated rectangle round the points.
pub fn min_side(pts: &[P2]) -> f64 {
    let h = hull2(pts);
    if h.len() < 3 {
        return 0.0;
    }
    let n = h.len();
    let mut best = (f64::INFINITY, 0.0);
    for k in 0..n {
        let (a, b) = (h[k], h[(k + 1) % n]);
        let l = libm::hypot(b[0] - a[0], b[1] - a[1]);
        if l < 1e-9 {
            continue;
        }
        let (u, v) = (
            [(b[0] - a[0]) / l, (b[1] - a[1]) / l],
            [-(b[1] - a[1]) / l, (b[0] - a[0]) / l],
        );
        let (mut u0, mut u1, mut v0, mut v1) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for p in &h {
            let (pu, pv) = (p[0] * u[0] + p[1] * u[1], p[0] * v[0] + p[1] * v[1]);
            u0 = u0.min(pu);
            u1 = u1.max(pu);
            v0 = v0.min(pv);
            v1 = v1.max(pv);
        }
        let ar = (u1 - u0) * (v1 - v0);
        if ar < best.0 {
            best = (ar, (u1 - u0).min(v1 - v0));
        }
    }
    best.1
}

/// Douglas-Peucker on a closed ring.
pub fn simplify_ring(ring: &[P2], tol: f64) -> Vec<P2> {
    if ring.len() < 4 {
        return ring.to_vec();
    }
    // split at the point farthest from ring[0]
    let far = (1..ring.len())
        .max_by(|&a, &b| dist2(ring[0], ring[a]).total_cmp(&dist2(ring[0], ring[b])))
        .unwrap_or(1);
    let mut a = dp(&ring[..=far], tol);
    let mut tail: Vec<P2> = ring[far..].to_vec();
    tail.push(ring[0]);
    let b = dp(&tail, tol);
    a.pop();
    a.extend(b);
    a.pop();
    a
}

fn dist2(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

fn dp(pts: &[P2], tol: f64) -> Vec<P2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let (mut mi, mut md) = (0, 0.0);
    for (k, &p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = seg_dist(p, a, b);
        if d > md {
            md = d;
            mi = k;
        }
    }
    if md <= tol {
        return vec![a, b];
    }
    let mut l = dp(&pts[..=mi], tol);
    let r = dp(&pts[mi..], tol);
    l.pop();
    l.extend(r);
    l
}

pub fn seg_dist(p: P2, a: P2, b: P2) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 {
        (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    libm::hypot(p[0] - a[0] - t * ab[0], p[1] - a[1] - t * ab[1])
}

/// The outer boundary of a cell set (cells of `grid_mm`), traced along cell
/// edges, as a counter-clockwise ring in mm. The largest loop.
pub fn trace_outline(m: &Mask, grid_mm: f64) -> Vec<P2> {
    use std::collections::HashMap;
    // directed boundary edges, interior on the left (ccw)
    let mut next: HashMap<(i64, i64), Vec<(i64, i64)>> = HashMap::new();
    let on = |i: i64, j: i64| {
        i >= 0
            && j >= 0
            && (i as usize) < m.nx
            && (j as usize) < m.ny
            && m.at(i as usize, j as usize)
    };
    for j in 0..m.ny as i64 {
        for i in 0..m.nx as i64 {
            if !on(i, j) {
                continue;
            }
            if !on(i, j - 1) {
                next.entry((i, j)).or_default().push((i + 1, j));
            }
            if !on(i + 1, j) {
                next.entry((i + 1, j)).or_default().push((i + 1, j + 1));
            }
            if !on(i, j + 1) {
                next.entry((i + 1, j + 1)).or_default().push((i, j + 1));
            }
            if !on(i - 1, j) {
                next.entry((i, j + 1)).or_default().push((i, j));
            }
        }
    }
    let mut best: Vec<P2> = Vec::new();
    let mut best_a = 0.0;
    while let Some((&start, _)) = next.iter().min_by_key(|(k, _)| **k) {
        let mut ring = vec![start];
        let mut cur = start;
        while let Some(outs) = next.get_mut(&cur) {
            let Some(n) = outs.pop() else { break };
            if outs.is_empty() {
                next.remove(&cur);
            }
            if n == start {
                break;
            }
            ring.push(n);
            cur = n;
        }
        let pts: Vec<P2> = ring
            .iter()
            .map(|&(a, b)| [a as f64 * grid_mm, b as f64 * grid_mm])
            .collect();
        let a = area(&pts);
        if a > best_a {
            best_a = a;
            best = pts;
        }
    }
    // drop collinear vertices
    let n = best.len();
    let mut out = Vec::new();
    for k in 0..n {
        let (a, b, c) = (best[(k + n - 1) % n], best[k], best[(k + 1) % n]);
        let cr = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if cr.abs() > 1e-12 {
            out.push(b);
        }
    }
    out
}

pub fn norm2(a: P2) -> f64 {
    libm::hypot(a[0], a[1])
}

pub fn point_in(poly: &[P2], p: P2) -> bool {
    let mut c = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            c = !c;
        }
        j = i;
    }
    c
}

/// Distance from a point to a closed ring's boundary.
pub fn ring_dist(poly: &[P2], p: P2) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|k| seg_dist(p, poly[k], poly[(k + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}
