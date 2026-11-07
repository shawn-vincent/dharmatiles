//! The crown and the wood as surfaces to seat leaves on: ray queries, smooth
//! normals, and the wood split into bodies.

use super::*;

/// The foliage union, ready for queries.
pub(super) struct Crown {
    pub(super) rays: RayMesh,
    pub(super) positions: Vec<V>,
    pub(super) triangles: Vec<[u32; 3]>,
    /// Angle-weighted vertex normals (trimesh's `vertex_normals`), equal
    /// across split copies of a vertex.
    pub(super) normals: Vec<V>,
    pub(super) areas: Vec<f64>,
    pub(super) area: f64,
    /// Mean of the distinct vertices.
    pub(super) centroid: V,
}

impl Crown {
    pub(super) fn new(m: &TriMesh) -> Self {
        let rep = weld(m);
        let mut acc = vec![[0.0; 3]; m.positions.len()];
        let mut areas = Vec::with_capacity(m.triangles.len());
        for t in &m.triangles {
            let p = t.map(|i| m.positions[i as usize]);
            let fn_ = cross(sub(p[1], p[0]), sub(p[2], p[0]));
            let l = norm(fn_);
            areas.push(0.5 * l);
            if l <= 0.0 {
                continue;
            }
            let fnu = scale(fn_, 1.0 / l);
            for k in 0..3 {
                let a = sub(p[(k + 1) % 3], p[k]);
                let b = sub(p[(k + 2) % 3], p[k]);
                let ang = libm::acos((dot(a, b) / (norm(a) * norm(b))).clamp(-1.0, 1.0));
                let r = rep[t[k] as usize];
                acc[r] = add(acc[r], scale(fnu, ang));
            }
        }
        let normals = (0..m.positions.len())
            .map(|i| {
                let v = acc[rep[i]];
                let l = norm(v);
                if l > 0.0 { scale(v, 1.0 / l) } else { v }
            })
            .collect();
        let mut c = [0.0; 3];
        let mut n = 0.0;
        for (i, p) in m.positions.iter().enumerate() {
            if rep[i] == i {
                c = add(c, *p);
                n += 1.0;
            }
        }
        Self {
            rays: RayMesh::new(m),
            positions: m.positions.clone(),
            triangles: m.triangles.clone(),
            normals,
            area: areas.iter().sum(),
            areas,
            centroid: scale(c, 1.0 / n),
        }
    }

    /// Area-weighted samples with smooth normals (drawn in this order:
    /// faces, then u, then v).
    pub(super) fn sample(&self, n: usize, rng: &mut Rng) -> Samples {
        let mut cum = Vec::with_capacity(self.areas.len());
        let mut s = 0.0;
        for a in &self.areas {
            s += a;
            cum.push(s);
        }
        if s <= 0.0 {
            return Samples::default();
        }
        for c in &mut cum {
            *c /= s;
        }
        let fi: Vec<usize> = (0..n)
            .map(|_| {
                let r = rng.random();
                cum.partition_point(|c| *c < r).min(cum.len() - 1)
            })
            .collect();
        let u: Vec<f64> = (0..n).map(|_| rng.random()).collect();
        let v: Vec<f64> = (0..n).map(|_| rng.random()).collect();
        let mut out = Samples::default();
        for k in 0..n {
            let (mut a, mut b) = (u[k], v[k]);
            if a + b > 1.0 {
                a = 1.0 - a;
                b = 1.0 - b;
            }
            let t = self.triangles[fi[k]];
            let [p0, p1, p2] = t.map(|i| self.positions[i as usize]);
            let p = add(add(p0, scale(sub(p1, p0), a)), scale(sub(p2, p0), b));
            out.points.push(p);
            out.normals.push(self.smooth_normal(fi[k], p));
        }
        out
    }

    /// Barycentric blend of a triangle's vertex normals at `p` (trimesh's
    /// `points_to_barycentric`, Cramer's rule), normalised.
    pub(super) fn smooth_normal(&self, tri: usize, p: V) -> V {
        let t = self.triangles[tri];
        let [a, b, c] = t.map(|i| self.positions[i as usize]);
        let (e0, e1, w) = (sub(b, a), sub(c, a), sub(p, a));
        let (d00, d01, d11) = (dot(e0, e0), dot(e0, e1), dot(e1, e1));
        let (d02, d12) = (dot(e0, w), dot(e1, w));
        let inv = 1.0 / (d00 * d11 - d01 * d01);
        let b1 = (d11 * d02 - d01 * d12) * inv;
        let b2 = (d00 * d12 - d01 * d02) * inv;
        let b0 = 1.0 - b1 - b2;
        let [n0, n1, n2] = t.map(|i| self.normals[i as usize]);
        let n = add(add(scale(n0, b0), scale(n1, b1)), scale(n2, b2));
        let l = norm(n);
        if l > 1e-9 {
            scale(n, 1.0 / l)
        } else {
            [0.0; 3]
        }
    }

    pub(super) fn inside(&self, p: V) -> bool {
        self.rays.contains(&[p])[0]
    }

    pub(super) fn hit(&self, o: V, d: V) -> Option<RayHit> {
        self.rays.first_hits(&[o], &[d])[0]
    }

    /// Signed clearance of `p` along `n`: positive outside (distance down
    /// to the surface along −n), negative inside (depth up to it along
    /// +n); `None` when the ray misses.
    pub(super) fn clearance(&self, p: V, n: V) -> Option<(f64, bool)> {
        let inside = self.inside(p);
        let d = if inside { n } else { scale(n, -1.0) };
        self.hit(p, d).map(|h| {
            let c = norm(sub(h.point, p));
            (if inside { -c } else { c }, inside)
        })
    }

    /// Drop `q` onto the surface along −n from
    /// 1 mm above; the hit and its smooth normal, unless it misses or falls
    /// further than `max_drop`.
    pub(super) fn project(&self, q: V, n: V, max_drop: f64) -> Option<(V, V)> {
        let g = add(q, scale(n, PROJECT_LIFT_MM));
        let h = self.hit(g, scale(n, -1.0))?;
        if norm(sub(h.point, g)) > max_drop {
            return None;
        }
        Some((
            h.point,
            safe_norm(self.smooth_normal(h.triangle as usize, h.point)),
        ))
    }
}

impl Crown {
    /// Drop `q` onto the crown along `−n`: from [`DRAPE_REACH_MM`] above
    /// it, or nearer when that start is inside another lobe (the hit then
    /// faces away). The hit and its smooth normal.
    pub(super) fn drop_on(&self, q: V, n: V) -> Option<(RayHit, V)> {
        for reach in [DRAPE_REACH_MM, 0.6, 0.25] {
            let start = add(q, scale(n, reach));
            let Some(hit) = self.hit(start, scale(n, -1.0)) else {
                continue;
            };
            if norm(sub(hit.point, start)) > reach + DRAPE_REACH_MM {
                continue;
            }
            let ns = self.smooth_normal(hit.triangle as usize, hit.point);
            if dot(ns, n) > DRAPE_FOLD_COS {
                return Some((hit, ns));
            }
        }
        None
    }

    /// Whether the crown, walked down its steepest slope from `p` (normal
    /// `n`) for [`BELOW_STEPS`] steps of [`BELOW_STEP_MM`], gets below `z`.
    pub(super) fn goes_below(&self, p: V, n: V, z: f64) -> bool {
        let down = sub([0.0, 0.0, -1.0], scale(n, -n[2]));
        if norm(down) < 1e-6 {
            return false;
        }
        self.walk(p, n, safe_norm(down), BELOW_STEP_MM, BELOW_STEPS)
            .is_ok_and(|f| f.iter().any(|f| f.p[2] < z))
    }

    /// Walk `steps` steps of `ds` along the crown from `p` heading `t`:
    /// each step goes `ds` along the local tangent and drops back onto the
    /// crown along the local normal.
    pub(super) fn walk(&self, p: V, n: V, t: V, ds: f64, steps: usize) -> Result<Vec<Frame>, Fail> {
        let (mut p, mut n, mut t) = (p, n, t);
        let mut out = Vec::with_capacity(steps);
        for _ in 0..steps {
            let guess = add(p, scale(t, ds));
            let (hit, nn) = self.drop_on(guess, n).ok_or(Fail::Drape)?;
            let step = sub(hit.point, p);
            let tt = safe_norm(sub(step, scale(nn, dot(step, nn))));
            p = hit.point;
            n = nn;
            t = tt;
            out.push(Frame { p, n, t });
        }
        Ok(out)
    }
}

/// `rep[i]`: the vertex `i` is welded to (`merge_from` → `merge_to`).
pub(super) fn weld(m: &TriMesh) -> Vec<usize> {
    let mut rep: Vec<usize> = (0..m.positions.len()).collect();
    for (f, t) in m.merge_from.iter().zip(&m.merge_to) {
        rep[*f as usize] = *t as usize;
    }
    // Chains (a → b → c) settle on their end.
    for i in 0..rep.len() {
        let mut r = rep[i];
        while rep[r] != r {
            r = rep[r];
        }
        rep[i] = r;
    }
    rep
}

/// One closed wood body (a branch tube, or the whole wood when unioned).
pub(super) struct Body {
    pub(super) rays: RayMesh,
    pub(super) lo: V,
    pub(super) hi: V,
}

impl Body {
    pub(super) fn new(m: &TriMesh) -> Self {
        let rays = RayMesh::new(m);
        let (lo, hi) = rays.bounds();
        Self { rays, lo, hi }
    }
}

/// The connected bodies of a mesh (by shared, welded vertices), in order
/// of their first triangle.
pub(in crate::tree) fn split_bodies(m: &TriMesh) -> Vec<TriMesh> {
    let rep = weld(m);
    let mut parent: Vec<usize> = (0..m.positions.len()).collect();
    for t in &m.triangles {
        let a = find(&mut parent, rep[t[0] as usize]);
        for &v in &t[1..] {
            let b = find(&mut parent, rep[v as usize]);
            if a != b {
                parent[b] = a;
            }
        }
    }
    let groups: Vec<usize> = m
        .triangles
        .iter()
        .map(|t| find(&mut parent, rep[t[0] as usize]))
        .collect();
    gather(m, &groups)
}

/// Union-find root of `i`, halving the path.
pub(super) fn find(p: &mut [usize], mut i: usize) -> usize {
    while p[i] != i {
        p[i] = p[p[i]];
        i = p[i];
    }
    i
}

/// The triangles of `m` grouped by `groups[t]` (first-seen order), each
/// group its own compact mesh.
#[allow(clippy::cast_possible_truncation, reason = "vertex indices fit u32")]
pub(super) fn gather(m: &TriMesh, groups: &[usize]) -> Vec<TriMesh> {
    let mut of: HashMap<usize, usize> = HashMap::new();
    let mut tris: Vec<Vec<[u32; 3]>> = Vec::new();
    for (t, g) in m.triangles.iter().zip(groups) {
        let k = *of.entry(*g).or_insert_with(|| {
            tris.push(Vec::new());
            tris.len() - 1
        });
        tris[k].push(*t);
    }
    tris.into_iter()
        .map(|ts| {
            let mut map: HashMap<u32, u32> = HashMap::new();
            let mut positions = Vec::new();
            let triangles = ts
                .iter()
                .map(|t| {
                    t.map(|i| {
                        *map.entry(i).or_insert_with(|| {
                            positions.push(m.positions[i as usize]);
                            (positions.len() - 1) as u32
                        })
                    })
                })
                .collect();
            TriMesh {
                positions,
                triangles,
                ..TriMesh::default()
            }
        })
        .collect()
}
