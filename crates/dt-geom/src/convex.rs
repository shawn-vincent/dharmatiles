//! Convex polytopes cut by half-spaces: rocklab's facets and chips
//! (`cut_facets` :187, `chip_corners_edges` :231) without a boolean.
//!
//! A [`Convex`] is a closed convex solid stored as shared vertices and
//! planar faces (each face a convex polygon of vertex indices, wound
//! counter-clockwise seen from outside, with its outward plane). A cut
//! keeps the side `n · x ≤ d` of a plane: every face is clipped, each
//! crossing edge gets one new vertex (shared by both faces that hold the
//! edge, so the result stays closed), and the new face is the convex
//! polygon of the points on the plane. Every step is plain arithmetic in a
//! fixed order, so the result depends only on the inputs (R2), and a
//! convex solid cut by a plane is convex, so it stays printable wherever
//! the planes are.

use std::collections::BTreeMap;

use crate::TriMesh;
use crate::vec3::{cross, dot, norm, sub};

/// Points this close to a plane (mm) count as on it.
const EPS: f64 = 1e-9;

type V3 = [f64; 3];

/// One planar face: its vertex loop and outward plane `normal · x = offset`.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// Vertex indices, counter-clockwise seen from outside.
    pub loop_: Vec<u32>,
    /// Outward unit normal.
    pub normal: V3,
    /// Plane offset.
    pub offset: f64,
}

/// A closed convex polytope.
#[derive(Clone, Debug, PartialEq)]
pub struct Convex {
    /// Vertex positions, mm (some may be unused after cuts).
    pub vertices: Vec<V3>,
    /// The faces.
    pub faces: Vec<Face>,
}

/// An edge between two faces, with its dihedral angle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    /// Its two vertices.
    pub ends: [V3; 2],
    /// The two faces' outward normals.
    pub normals: [V3; 2],
    /// The angle between those normals, radians (0: flat).
    pub angle: f64,
}

impl Convex {
    /// The box `min..max`.
    #[must_use]
    pub fn cuboid(min: V3, max: V3) -> Self {
        let vertices = (0..8)
            .map(|i| {
                [
                    if i & 1 == 0 { min[0] } else { max[0] },
                    if i & 2 == 0 { min[1] } else { max[1] },
                    if i & 4 == 0 { min[2] } else { max[2] },
                ]
            })
            .collect();
        let f = |loop_: [u32; 4], normal: V3, offset: f64| Face {
            loop_: loop_.to_vec(),
            normal,
            offset,
        };
        Self {
            vertices,
            faces: vec![
                f([0, 4, 6, 2], [-1.0, 0.0, 0.0], -min[0]),
                f([1, 3, 7, 5], [1.0, 0.0, 0.0], max[0]),
                f([0, 1, 5, 4], [0.0, -1.0, 0.0], -min[1]),
                f([2, 6, 7, 3], [0.0, 1.0, 0.0], max[1]),
                f([0, 2, 3, 1], [0.0, 0.0, -1.0], -min[2]),
                f([4, 5, 7, 6], [0.0, 0.0, 1.0], max[2]),
            ],
        }
    }

    /// Keep the part where `normal · x ≤ offset` (`normal` need not be
    /// unit). `None` when nothing is left or the cut would not close; the
    /// solid itself when the plane misses it.
    #[must_use]
    pub fn cut(&self, normal: V3, offset: f64) -> Option<Self> {
        let len = norm(normal);
        if !(len.is_finite() && len > 0.0) {
            return Some(self.clone());
        }
        let n = normal.map(|x| x / len);
        let d = offset / len;
        let dist: Vec<f64> = self.vertices.iter().map(|&v| dot(n, v) - d).collect();
        let used = self.used();
        if used.iter().all(|&i| dist[i as usize] > EPS) {
            return None;
        }
        if used.iter().all(|&i| dist[i as usize] <= EPS) {
            return Some(self.clone());
        }
        let mut vertices = self.vertices.clone();
        let mut split: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        let mut on_plane: Vec<u32> = Vec::new();
        let mut faces = Vec::with_capacity(self.faces.len() + 1);
        for f in &self.faces {
            let mut out = Vec::with_capacity(f.loop_.len() + 1);
            let m = f.loop_.len();
            for k in 0..m {
                let (a, b) = (f.loop_[k], f.loop_[(k + 1) % m]);
                let (da, db) = (dist[a as usize], dist[b as usize]);
                if da <= EPS {
                    out.push(a);
                    if da >= -EPS {
                        on_plane.push(a);
                    }
                }
                if (da < -EPS && db > EPS) || (da > EPS && db < -EPS) {
                    // one shared vertex per crossing edge, computed from
                    // the lower index so both faces get the same point
                    let key = (a.min(b), a.max(b));
                    let v = *split.entry(key).or_insert_with(|| {
                        let (p, q) = (key.0 as usize, key.1 as usize);
                        let t = dist[p] / (dist[p] - dist[q]);
                        let (vp, vq) = (vertices[p], vertices[q]);
                        vertices.push([0, 1, 2].map(|c| vp[c] + t * (vq[c] - vp[c])));
                        u32::try_from(vertices.len() - 1).unwrap_or(u32::MAX)
                    });
                    out.push(v);
                    on_plane.push(v);
                }
            }
            out.dedup();
            if out.len() > 1 && out.first() == out.last() {
                out.pop();
            }
            if out.len() >= 3 {
                faces.push(Face {
                    loop_: out,
                    normal: f.normal,
                    offset: f.offset,
                });
            }
        }
        on_plane.sort_unstable();
        on_plane.dedup();
        if on_plane.len() >= 3 {
            // the cap: the points on the plane, in order round their centre
            let u = {
                let seed = if n[0].abs() < 0.9 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 1.0, 0.0]
                };
                let c = cross(n, seed);
                let l = norm(c);
                c.map(|x| x / l)
            };
            let w = cross(n, u);
            #[allow(clippy::cast_precision_loss)]
            let k = on_plane.len() as f64;
            let c = on_plane.iter().fold([0.0; 3], |s, &i| {
                let p = vertices[i as usize];
                [s[0] + p[0] / k, s[1] + p[1] / k, s[2] + p[2] / k]
            });
            let mut ring: Vec<(f64, u32)> = on_plane
                .iter()
                .map(|&i| {
                    let r = sub(vertices[i as usize], c);
                    (libm::atan2(dot(r, w), dot(r, u)), i)
                })
                .collect();
            ring.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            faces.push(Face {
                loop_: ring.into_iter().map(|(_, i)| i).collect(),
                normal: n,
                offset: d,
            });
        }
        let out = Self { vertices, faces };
        // a near-degenerate cut (a plane grazing a vertex) can leave a
        // sliver face that does not close: refuse it
        (out.faces.len() >= 4 && out.is_closed()).then(|| out.compact())
    }

    /// Every directed face edge has its twin running the other way.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        let mut edges = std::collections::BTreeSet::new();
        for f in &self.faces {
            let m = f.loop_.len();
            for k in 0..m {
                if !edges.insert((f.loop_[k], f.loop_[(k + 1) % m])) {
                    return false;
                }
            }
        }
        edges.iter().all(|&(a, b)| edges.contains(&(b, a)))
    }

    /// The vertex indices any face uses, ascending.
    #[must_use]
    pub fn used(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self
            .faces
            .iter()
            .flat_map(|f| f.loop_.iter().copied())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Drop unused vertices (keeps the order of the rest).
    #[must_use]
    pub fn compact(mut self) -> Self {
        let used = self.used();
        if used.len() == self.vertices.len() {
            return self;
        }
        let mut map = vec![u32::MAX; self.vertices.len()];
        let mut vertices = Vec::with_capacity(used.len());
        for &i in &used {
            map[i as usize] = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
            vertices.push(self.vertices[i as usize]);
        }
        for f in &mut self.faces {
            f.loop_.iter_mut().for_each(|i| *i = map[*i as usize]);
        }
        self.vertices = vertices;
        self
    }

    /// Enclosed volume, mm³.
    #[must_use]
    pub fn volume(&self) -> f64 {
        self.faces
            .iter()
            .map(|f| {
                let p0 = self.vertices[f.loop_[0] as usize];
                let mut a = 0.0;
                for k in 1..f.loop_.len() - 1 {
                    let (p1, p2) = (
                        self.vertices[f.loop_[k] as usize],
                        self.vertices[f.loop_[k + 1] as usize],
                    );
                    a += dot(cross(sub(p1, p0), sub(p2, p0)), f.normal) / 2.0;
                }
                a * f.offset / 3.0
            })
            .sum()
    }

    /// The bounding box `(min, max)`.
    #[must_use]
    pub fn bounds(&self) -> (V3, V3) {
        self.used().iter().fold(
            ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
            |(lo, hi), &i| {
                let p = self.vertices[i as usize];
                (
                    [0, 1, 2].map(|c| lo[c].min(p[c])),
                    [0, 1, 2].map(|c| hi[c].max(p[c])),
                )
            },
        )
    }

    /// Each used vertex with the sum of the normals of the faces round it
    /// (unnormalised), in index order: rocklab's corner normals.
    #[must_use]
    pub fn corners(&self) -> Vec<(V3, V3)> {
        let mut sum = vec![[0.0; 3]; self.vertices.len()];
        for f in &self.faces {
            for &i in &f.loop_ {
                let s = &mut sum[i as usize];
                *s = [0, 1, 2].map(|c| s[c] + f.normal[c]);
            }
        }
        self.used()
            .into_iter()
            .map(|i| (self.vertices[i as usize], sum[i as usize]))
            .collect()
    }

    /// Every edge between two faces, once, in face order.
    #[must_use]
    pub fn edges(&self) -> Vec<Edge> {
        let mut owner: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for (fi, f) in self.faces.iter().enumerate() {
            let m = f.loop_.len();
            for k in 0..m {
                owner.insert((f.loop_[k], f.loop_[(k + 1) % m]), fi);
            }
        }
        let mut out = Vec::new();
        for f in &self.faces {
            let m = f.loop_.len();
            for k in 0..m {
                let (a, b) = (f.loop_[k], f.loop_[(k + 1) % m]);
                if a > b {
                    continue;
                }
                let Some(&gi) = owner.get(&(b, a)) else {
                    continue;
                };
                let (na, nb) = (f.normal, self.faces[gi].normal);
                out.push(Edge {
                    ends: [self.vertices[a as usize], self.vertices[b as usize]],
                    normals: [na, nb],
                    angle: libm::acos(dot(na, nb).clamp(-1.0, 1.0)),
                });
            }
        }
        out
    }

    /// A closed triangle mesh (each face fanned from its first vertex).
    #[must_use]
    pub fn to_tri_mesh(&self) -> TriMesh {
        let c = self.clone().compact();
        let mut triangles = Vec::new();
        for f in &c.faces {
            for k in 1..f.loop_.len() - 1 {
                triangles.push([f.loop_[0], f.loop_[k], f.loop_[k + 1]]);
            }
        }
        TriMesh {
            positions: c.vertices,
            triangles,
            ..TriMesh::default()
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]
    use super::*;

    fn unit() -> Convex {
        Convex::cuboid([0.0; 3], [1.0; 3])
    }

    #[test]
    fn a_box_is_closed_and_has_its_volume() {
        let b = unit();
        assert!((b.volume() - 1.0).abs() < 1e-12);
        assert!(b.to_tri_mesh().is_watertight());
        assert_eq!(b.edges().len(), 12);
        assert!(
            b.edges()
                .iter()
                .all(|e| (e.angle - core::f64::consts::FRAC_PI_2).abs() < 1e-12)
        );
    }

    #[test]
    fn cuts_keep_the_inner_side_and_stay_closed() {
        // the corner x + y + z ≤ 2.5 off a unit box: a tetrahedron of 1/48
        let c = unit().cut([1.0, 1.0, 1.0], 2.5).unwrap();
        assert!((c.volume() - (1.0 - 0.125 / 6.0)).abs() < 1e-12);
        assert_eq!(c.faces.len(), 7);
        let m = c.to_tri_mesh();
        assert!(m.is_watertight());
        // a plane through a vertex and two edge midpoints, then many more
        let mut s = unit();
        for k in 0..60 {
            let t = f64::from(k) * 0.37;
            let n = [libm::cos(t), libm::sin(t), libm::sin(1.7 * t)];
            let d = 0.48 + 0.5 * dot(n, [0.5; 3]);
            if let Some(next) = s.cut(n, d) {
                s = next;
            }
            assert!(s.to_tri_mesh().is_watertight(), "cut {k}");
            assert!(s.volume() > 0.0);
        }
    }

    #[test]
    fn a_missing_plane_changes_nothing_and_a_covering_one_empties() {
        assert_eq!(unit().cut([0.0, 0.0, 1.0], 5.0), Some(unit()));
        assert!(unit().cut([0.0, 0.0, 1.0], -1.0).is_none());
        // through a face exactly: nothing removed
        assert!((unit().cut([0.0, 0.0, 1.0], 1.0).unwrap().volume() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn corners_sum_their_face_normals() {
        let c = unit().corners();
        assert_eq!(c.len(), 8);
        for (p, n) in c {
            for k in 0..3 {
                assert_eq!(n[k], if p[k] > 0.5 { 1.0 } else { -1.0 });
            }
        }
    }
}
