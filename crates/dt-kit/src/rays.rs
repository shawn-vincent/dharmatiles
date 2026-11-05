//! Rays (P5): first hits and point containment against a closed mesh,
//! for elements that seat things on surfaces (the tree's leaf placer).
//!
//! Every ray goes through dt-geom's [`dt_geom::Rays`] trait, on the obvhs
//! backend.
//! The queries follow trimesh's Embree wrapper's conventions:
//!
//! - the mesh is moved to its bounding-box minimum and scaled to a box of
//!   about 100 units (`100 / diagonal`) before the f32 build, and every
//!   origin goes through the same transform (`ray_pyembree._EmbreeWrap`);
//! - directions are normalised in f64, then rounded to f32;
//! - a hit's position is recomputed in f64 on the hit triangle's plane
//!   (`intersections.planes_lines`), and dropped when the ray is within
//!   `1e-5` of parallel to it;
//! - [`RayMesh::contains`] is `ray_util.contains_points`: every hit along a
//!   fixed direction and its opposite is counted (stepping past each hit by
//!   `1e-6 × diagonal`, doubling the step when a ray re-hits the same
//!   triangle), a point is inside when both counts are odd, outside when
//!   both are even or either ray escapes cleanly, and tie-broken along a
//!   third fixed direction when they disagree (fixed, so the answer is
//!   deterministic).
//!
//! Queries are batched: one call casts many rays, in parallel inside the
//! backend, and returns answers in input order, so results do not depend
//! on the thread count (R2).

use dt_geom::{ObvhsRays, Ray, Rays, TriMesh};

use crate::graph::{add, cross, dot, norm, scale, sub};

/// trimesh's `contains_points` default direction.
const CONTAINS_DIR: [f64; 3] = [0.439_506_445_5, 0.617_598_629_942, 0.652_231_566_745];
/// The tie-break direction.
const CONTAINS_TIE_DIR: [f64; 3] = [
    -0.577_350_269_189_6,
    0.288_675_134_594_8,
    0.763_762_615_825_9,
];
/// trimesh's cap on hits followed along one ray.
const MAX_HITS: usize = 100;
/// trimesh's per-hit step past a triangle, as a fraction of the diagonal.
const OFFSET_FACTOR: f64 = 1e-6;
const OFFSET_FLOOR: f64 = 1e-8;

/// Where a ray first meets the mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// The hit, in mm.
    pub point: [f64; 3],
    /// The triangle hit.
    pub triangle: u32,
}

/// A closed triangle mesh, ready for ray queries.
pub struct RayMesh<R: Rays = ObvhsRays> {
    rays: R,
    origin: [f64; 3],
    scale: f64,
    step: f64,
    lo: [f64; 3],
    hi: [f64; 3],
    positions: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
    face_normals: Vec<[f64; 3]>,
}

impl RayMesh<ObvhsRays> {
    /// Build over a mesh (positions in mm).
    #[must_use]
    pub fn new(mesh: &TriMesh) -> Self {
        Self::with_backend(&mesh.positions, &mesh.triangles)
    }
}

impl<R: Rays> RayMesh<R> {
    /// Build over positions and triangles with any [`Rays`] backend.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the f32 build frame is the point: Embree is f32"
    )]
    pub fn with_backend(positions: &[[f64; 3]], triangles: &[[u32; 3]]) -> Self {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in positions {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        if positions.is_empty() {
            lo = [0.0; 3];
            hi = [0.0; 3];
        }
        let diag = norm(sub(hi, lo));
        let scale = if diag > 0.0 { 100.0 / diag } else { 1.0 };
        let scaled: Vec<[f32; 3]> = positions
            .iter()
            .map(|p| [0, 1, 2].map(|k| ((p[k] - lo[k]) * scale) as f32))
            .collect();
        let face_normals = triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| positions[i as usize]);
                let n = cross(sub(b, a), sub(c, a));
                let l = norm(n);
                if l > 0.0 { n.map(|x| x / l) } else { [0.0; 3] }
            })
            .collect();
        Self {
            rays: R::build(&scaled, triangles),
            origin: lo,
            scale,
            step: (diag * OFFSET_FACTOR).max(OFFSET_FLOOR),
            lo,
            hi,
            positions: positions.to_vec(),
            triangles: triangles.to_vec(),
            face_normals,
        }
    }

    /// The mesh's bounding box, `(lo, hi)` in mm.
    #[must_use]
    pub fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        (self.lo, self.hi)
    }

    /// The first hit of each ray (`dirs` need not be unit), in input order.
    #[must_use]
    pub fn first_hits(&self, origins: &[[f64; 3]], dirs: &[[f64; 3]]) -> Vec<Option<RayHit>> {
        let dirs: Vec<[f64; 3]> = dirs.iter().map(|d| unit(*d)).collect();
        let rays: Vec<Ray> = origins
            .iter()
            .zip(&dirs)
            .map(|(o, d)| self.ray(*o, *d))
            .collect();
        self.rays
            .intersect(&rays)
            .into_iter()
            .zip(origins.iter().zip(&dirs))
            .map(|(h, (o, d))| {
                let t = h?.triangle;
                self.on_plane(t, *o, *d)
                    .map(|point| RayHit { point, triangle: t })
            })
            .collect()
    }

    /// Whether each point is inside the mesh, in input order. A point on
    /// the surface may go either way (as in trimesh).
    #[must_use]
    pub fn contains(&self, points: &[[f64; 3]]) -> Vec<bool> {
        let mut out = vec![false; points.len()];
        let idx: Vec<usize> = (0..points.len())
            .filter(|&i| (0..3).all(|k| points[i][k] > self.lo[k] && points[i][k] < self.hi[k]))
            .collect();
        if idx.is_empty() {
            return out;
        }
        let pts: Vec<[f64; 3]> = idx.iter().map(|&i| points[i]).collect();
        let (fwd, back) = self.crossing_counts(&pts, CONTAINS_DIR);
        let mut broken = Vec::new();
        for (k, &i) in idx.iter().enumerate() {
            let (a, b) = (fwd[k] % 2 == 1, back[k] % 2 == 1);
            if a == b {
                out[i] = a;
            } else if fwd[k] != 0 && back[k] != 0 {
                broken.push(i);
            }
        }
        if !broken.is_empty() {
            let pts: Vec<[f64; 3]> = broken.iter().map(|&i| points[i]).collect();
            let (fwd, back) = self.crossing_counts(&pts, unit(CONTAINS_TIE_DIR));
            for (k, &i) in broken.iter().enumerate() {
                out[i] = fwd[k] % 2 == 1 && back[k] % 2 == 1;
            }
        }
        out
    }

    /// Hits along `dir` and `-dir` from each point, every hit counted.
    fn crossing_counts(&self, pts: &[[f64; 3]], dir: [f64; 3]) -> (Vec<u32>, Vec<u32>) {
        let n = pts.len();
        let d = unit(dir);
        let neg = d.map(|x| -x);
        let mut origin: Vec<[f64; 3]> = pts.iter().chain(pts).copied().collect();
        let dirs: Vec<[f64; 3]> = (0..2 * n).map(|i| if i < n { d } else { neg }).collect();
        let mut offset: Vec<f64> = vec![self.step; 2 * n];
        let mut last: Vec<i64> = vec![-1; 2 * n];
        let mut count = vec![0_u32; 2 * n];
        let mut live: Vec<usize> = (0..2 * n).collect();
        for _ in 0..MAX_HITS {
            if live.is_empty() {
                break;
            }
            let rays: Vec<Ray> = live.iter().map(|&i| self.ray(origin[i], dirs[i])).collect();
            let hits = self.rays.intersect(&rays);
            let mut ok = Vec::with_capacity(live.len());
            let mut dupe = Vec::new();
            for (&i, h) in live.iter().zip(hits) {
                let Some(h) = h else { continue };
                let t = i64::from(h.triangle);
                if last[i] == t {
                    dupe.push(i);
                    continue;
                }
                last[i] = t;
                if let Some(p) = self.on_plane(h.triangle, origin[i], dirs[i]) {
                    count[i] += 1;
                    origin[i] = add(p, scale(dirs[i], offset[i]));
                    offset[i] = self.step;
                    ok.push(i);
                }
            }
            for &i in &dupe {
                offset[i] *= 2.0;
                origin[i] = add(origin[i], scale(dirs[i], offset[i]));
            }
            ok.extend(dupe);
            live = ok;
        }
        let back = count.split_off(n);
        (count, back)
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "the f32 query frame is the point: Embree is f32"
    )]
    fn ray(&self, o: [f64; 3], d: [f64; 3]) -> Ray {
        Ray::new(
            [0, 1, 2].map(|k| ((o[k] - self.origin[k]) * self.scale) as f32),
            d.map(|x| x as f32),
        )
    }

    /// `planes_lines`: where the line meets the triangle's plane, in f64.
    fn on_plane(&self, t: u32, o: [f64; 3], d: [f64; 3]) -> Option<[f64; 3]> {
        let n = self.face_normals[t as usize];
        let v0 = self.positions[self.triangles[t as usize][0] as usize];
        let pd = dot(d, n);
        if pd.abs() <= 1e-5 {
            return None;
        }
        let dist = dot(sub(v0, o), n) / pd;
        Some(add(scale(d, dist), o))
    }
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let l = norm(v);
    if l > 0.0 { v.map(|x| x / l) } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A closed unit cube, outward wound.
    fn cube() -> TriMesh {
        let positions = (0..8)
            .map(|i| {
                [
                    f64::from(i & 1),
                    f64::from((i >> 1) & 1),
                    f64::from((i >> 2) & 1),
                ]
            })
            .collect();
        let triangles = vec![
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ];
        TriMesh {
            positions,
            triangles,
            ..TriMesh::default()
        }
    }

    #[test]
    fn hits_and_containment() {
        let m = RayMesh::new(&cube());
        let h = m.first_hits(
            &[[0.3, 0.4, 2.0], [0.3, 0.4, 0.5], [3.0, 3.0, 3.0]],
            &[[0.0, 0.0, -2.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        );
        let p0 = h[0].map(|h| h.point);
        assert!(p0.is_some_and(|p| (p[2] - 1.0).abs() < 1e-12 && (p[0] - 0.3).abs() < 1e-12));
        assert!(h[1].is_some_and(|h| (h.point[0] - 1.0).abs() < 1e-12));
        assert_eq!(h[2], None);
        assert_eq!(
            m.contains(&[
                [0.5, 0.5, 0.5],
                [0.2, 0.9, 0.1],
                [1.5, 0.5, 0.5],
                [0.5, 0.5, -0.1]
            ]),
            vec![true, true, false, false]
        );
    }
}
