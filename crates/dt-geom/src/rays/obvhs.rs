//! [`Rays`] over obvhs: a PLOC-built compressed wide BVH (CWBVH, 8-way),
//! traversed on the CPU, with Embree's triangle test.

use std::time::Duration;

use glam::Vec3A;
use obvhs::{
    BvhBuildParams,
    cwbvh::{CwBvh, builder::build_cwbvh_from_tris},
    ray::{Ray as ORay, RayHit},
    triangle::Triangle,
};
use rayon::prelude::*;

use super::{FNV_OFFSET, Hit, RAYS_PER_TASK, Ray, Rays, fnv1a};

/// obvhs CWBVH plus its triangles, stored in BVH leaf order.
pub struct ObvhsRays {
    bvh: CwBvh,
    /// `tris[k]` is input triangle `bvh.primitive_indices[k]`, precomputed
    /// in Embree's form: v0, e1 = v0 - v1, e2 = v2 - v0, Ng = e2 x e1.
    tris: Vec<[Vec3A; 4]>,
}

impl ObvhsRays {
    /// The build preset R3 chose: `medium_build` queried fastest on the
    /// cliff batches (2.3x Embree single-threaded) and still builds faster
    /// than Embree on both captures.
    pub const PRESET: BvhBuildParams = BvhBuildParams::medium_build();

    /// Build with an explicit obvhs preset (the R3 sweep uses this).
    pub fn build_with(
        positions: &[[f32; 3]],
        triangles: &[[u32; 3]],
        params: BvhBuildParams,
    ) -> Self {
        let otris: Vec<Triangle> = triangles
            .iter()
            .map(|t| {
                let [v0, v1, v2] = t.map(|i| Vec3A::from_array(positions[i as usize]));
                Triangle { v0, v1, v2 }
            })
            .collect();
        let bvh = if otris.is_empty() {
            CwBvh::default()
        } else {
            build_cwbvh_from_tris(&otris, params, &mut Duration::default())
        };
        let tris = bvh
            .primitive_indices
            .iter()
            .map(|&i| {
                let t = &otris[i as usize];
                let (e1, e2) = (t.v0 - t.v1, t.v2 - t.v0);
                [t.v0, e1, e2, e2.cross(e1)]
            })
            .collect();
        Self { bvh, tris }
    }

    fn first_hit(&self, ray: &Ray) -> Option<Hit> {
        if self.tris.is_empty() {
            return None;
        }
        // obvhs 0.4 bug: safe_inverse(-0.0) is -1/eps but the octant test
        // reads -0.0 as positive, so the slab swaps and the ray misses every
        // node (horizontal rays were lost).
        // Adding +0.0 turns -0.0 into +0.0 and changes nothing else.
        let oray = ORay::new(
            Vec3A::from_array(ray.origin),
            Vec3A::from_array(ray.dir.map(|c| c + 0.0)),
            ray.t_min,
            ray.t_max,
        );
        let mut hit = RayHit::none();
        let found = self.bvh.ray_traverse(oray, &mut hit, |r, id| {
            intersect_embree(&self.tris[id], r).unwrap_or(f32::INFINITY)
        });
        found.then(|| Hit {
            t: hit.t,
            triangle: self.bvh.primitive_indices[hit.primitive_id as usize],
        })
    }
}

impl Rays for ObvhsRays {
    fn build(positions: &[[f32; 3]], triangles: &[[u32; 3]]) -> Self {
        Self::build_with(positions, triangles, Self::PRESET)
    }

    fn intersect(&self, rays: &[Ray]) -> Vec<Option<Hit>> {
        rays.par_iter()
            .with_min_len(RAYS_PER_TASK)
            .map(|r| self.first_hit(r))
            .collect()
    }

    fn structure_hash(&self) -> u64 {
        let h = fnv1a(bytemuck::cast_slice(&self.bvh.nodes), FNV_OFFSET);
        fnv1a(bytemuck::cast_slice(&self.bvh.primitive_indices), h)
    }
}

/// [`super::intersect_triangle`] on precomputed `[v0, e1, e2, Ng]`, in glam
/// SIMD vectors. Same operations, same order.
#[inline(always)]
fn intersect_embree(tri: &[Vec3A; 4], ray: &ORay) -> Option<f32> {
    let [v0, e1, e2, ng] = *tri;
    let c = v0 - ray.origin;
    let r = c.cross(ray.direction);
    let den = ng.dot(ray.direction);
    let abs_den = den.abs();
    let sgn = den.to_bits() & 0x8000_0000;
    let u = f32::from_bits(r.dot(e2).to_bits() ^ sgn);
    let v = f32::from_bits(r.dot(e1).to_bits() ^ sgn);
    if abs_den <= 0.0 || u < 0.0 || v < 0.0 || u + v > abs_den {
        return None;
    }
    let t = f32::from_bits(ng.dot(c).to_bits() ^ sgn);
    if !(abs_den * ray.tmin < t && t <= abs_den * ray.tmax) {
        return None;
    }
    Some(t / abs_den)
}
