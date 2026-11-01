//! [`Rays`] over parry3d: its `TriMesh` (binned-SAH BVH) and parry's own
//! triangle test.

use std::fmt::Write;

use parry3d::math::Vector;
use parry3d::query::Ray as PRay;
use parry3d::shape::{CompositeShapeRef, TriMesh};
use rayon::prelude::*;

use super::{FNV_OFFSET, Hit, RAYS_PER_TASK, Ray, Rays, fnv1a};

/// A parry3d triangle mesh (which owns its BVH). `None` for an empty mesh,
/// which parry refuses to build.
pub struct ParryRays {
    mesh: Option<TriMesh>,
}

impl ParryRays {
    fn first_hit(&self, ray: &Ray) -> Option<Hit> {
        let mesh = self.mesh.as_ref()?;
        // parry rays start at t = 0: move the origin to t_min.
        let o = Vector::from_array(ray.origin);
        let d = Vector::from_array(ray.dir);
        let pray = PRay::new(o + d * ray.t_min, d);
        let (tri, t) =
            CompositeShapeRef(mesh).cast_local_ray(&pray, ray.t_max - ray.t_min, false)?;
        let t = t + ray.t_min;
        (t > ray.t_min).then_some(Hit { t, triangle: tri })
    }
}

impl Rays for ParryRays {
    fn build(positions: &[[f32; 3]], triangles: &[[u32; 3]]) -> Self {
        let verts = positions.iter().map(|p| Vector::from_array(*p)).collect();
        Self {
            mesh: TriMesh::new(verts, triangles.to_vec()).ok(),
        }
    }

    fn intersect(&self, rays: &[Ray]) -> Vec<Option<Hit>> {
        rays.par_iter()
            .with_min_len(RAYS_PER_TASK)
            .map(|r| self.first_hit(r))
            .collect()
    }

    fn structure_hash(&self) -> u64 {
        // parry exposes no node slice; its Debug output covers the nodes.
        // Stream it through the hash rather than building the string.
        let mut w = FnvWriter(FNV_OFFSET);
        if let Some(m) = &self.mesh {
            // Writing to FnvWriter cannot fail.
            let _ = write!(w, "{:?}", m.bvh());
        }
        w.0
    }
}

struct FnvWriter(u64);

impl Write for FnvWriter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0 = fnv1a(s.as_bytes(), self.0);
        Ok(())
    }
}
