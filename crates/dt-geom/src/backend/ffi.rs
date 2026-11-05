//! Manifold's C++ kernel through `manifold-csg` (FFI).
//!
//! The C API's `MeshGL64` constructor takes runs, merge
//! vectors and tangents but not `faceID`, run transforms or `tolerance`, so
//! face IDs of an imported mesh are recomputed by Manifold rather than
//! carried in, and a tolerance is applied after import with
//! `set_tolerance`, which re-simplifies the mesh at it (a `MeshGL` import
//! with the tolerance set does not).

use manifold_csg::{CrossSection, FillRule, JoinType, Manifold, MeshGL64, MeshGL64Options, OpType};

use super::{deinterleave, interleave, narrow};
use crate::mesh::{BoolOp, Bounds, Mesh, MeshError, TriMesh};

/// A solid in Manifold's C++ kernel.
#[derive(Clone)]
pub struct FfiSolid(pub Manifold);

const fn op(op: BoolOp) -> OpType {
    match op {
        BoolOp::Union => OpType::Add,
        BoolOp::Difference => OpType::Subtract,
        BoolOp::Intersection => OpType::Intersect,
    }
}

impl Mesh for FfiSolid {
    fn from_tri_mesh(mesh: &TriMesh) -> Result<Self, MeshError> {
        let (props, np) = interleave(mesh);
        let tris: Vec<u64> = mesh
            .triangles
            .iter()
            .flatten()
            .map(|&i| u64::from(i))
            .collect();
        let wide = |v: &[u32]| v.iter().map(|&i| u64::from(i)).collect::<Vec<u64>>();
        let (mf, mt, ri) = (
            wide(&mesh.merge_from),
            wide(&mesh.merge_to),
            wide(&mesh.run_index),
        );
        let mut opts = MeshGL64Options::new();
        if !mf.is_empty() {
            opts = opts.merge_vertices(&mf, &mt);
        }
        if !mesh.run_original_id.is_empty() {
            opts = opts.runs(&ri, &mesh.run_original_id);
        }
        let gl = MeshGL64::new_with_options(&props, np, &tris, opts)
            .map_err(|e| MeshError::Backend(e.to_string()))?;
        let m = Manifold::from_meshgl64(&gl).map_err(|_| MeshError::NotManifold)?;
        if mesh.tolerance > m.get_tolerance() {
            return Ok(Self(m.set_tolerance(mesh.tolerance)));
        }
        Ok(Self(m))
    }

    fn to_tri_mesh(&self) -> TriMesh {
        let gl = self.0.to_meshgl64();
        let np = gl.num_prop();
        let (positions, extra) = deinterleave(&gl.vert_properties(), np);
        let tv = narrow(gl.tri_verts());
        TriMesh {
            positions,
            triangles: tv.as_chunks::<3>().0.to_vec(),
            extra,
            num_extra: np.saturating_sub(3),
            merge_from: narrow(gl.merge_from_vert()),
            merge_to: narrow(gl.merge_to_vert()),
            run_index: narrow(gl.run_index()),
            run_original_id: gl.run_original_id(),
            face_id: narrow(gl.face_id()),
            tolerance: gl.tolerance(),
        }
    }

    fn boolean(&self, other: &Self, o: BoolOp) -> Self {
        Self(self.0.boolean(&other.0, op(o)))
    }

    fn batch(parts: &[Self], o: BoolOp) -> Self {
        let ms: Vec<Manifold> = parts.iter().map(|p| p.0.clone()).collect();
        Self(Manifold::batch_boolean(&ms, op(o)))
    }

    fn trim_by_plane(&self, normal: [f64; 3], offset: f64) -> Self {
        Self(self.0.trim_by_plane(normal, offset))
    }

    fn split(&self, cutter: &Self) -> (Self, Self) {
        let (a, b) = self.0.split(&cutter.0);
        (Self(a), Self(b))
    }

    fn split_by_plane(&self, normal: [f64; 3], offset: f64) -> (Self, Self) {
        let (a, b) = self.0.split_by_plane(normal, offset);
        (Self(a), Self(b))
    }

    fn level_set(sdf: &(dyn Fn([f64; 3]) -> f64 + Sync), bounds: Bounds, edge_length: f64) -> Self {
        Self(Manifold::from_sdf(
            |x, y, z| sdf([x, y, z]),
            (bounds.min, bounds.max),
            edge_length,
            0.0,
            -1.0,
        ))
    }

    fn level_set_seq(
        sdf: &(dyn Fn([f64; 3]) -> f64 + Sync),
        bounds: Bounds,
        edge_length: f64,
    ) -> Self {
        Self(Manifold::from_sdf_seq(
            |x, y, z| sdf([x, y, z]),
            (bounds.min, bounds.max),
            edge_length,
            0.0,
            -1.0,
        ))
    }

    fn extrude(contours: &[Vec<[f64; 2]>], inset: f64, z0: f64, z1: f64) -> Self {
        let mut cs = CrossSection::from_polygons_with_fill_rule(contours, FillRule::EvenOdd);
        if inset > 0.0 {
            // the miter join, as `CrossSection::offset` defaults to.
            cs = cs.offset(-inset, JoinType::Miter, 2.0, 0);
        }
        Self(Manifold::extrude(&cs, z1 - z0).translate(0.0, 0.0, z0))
    }

    fn hull(points: &[[f64; 3]]) -> Self {
        Self(Manifold::hull_pts(points))
    }

    fn refine_to_length(&self, length: f64) -> Self {
        Self(self.0.refine_to_length(length))
    }

    fn transform(&self, m: &[f64; 12]) -> Self {
        Self(self.0.transform(m))
    }

    fn slice(&self, z: f64) -> Vec<Vec<[f64; 2]>> {
        self.0.slice_at_z(z)
    }

    fn as_original(&self) -> Self {
        Self(self.0.as_original())
    }

    fn original_id(&self) -> i32 {
        self.0.original_id()
    }

    fn reserve_ids(n: u32) -> u32 {
        manifold_csg::reserve_ids(n)
    }

    fn bounds(&self) -> Option<Bounds> {
        self.0.bounding_box().map(|b| Bounds {
            min: b.min(),
            max: b.max(),
        })
    }

    fn status(&self) -> Result<(), MeshError> {
        self.0
            .status()
            .map_err(|e| MeshError::Backend(e.to_string()))
    }

    fn volume(&self) -> f64 {
        self.0.volume()
    }

    fn triangle_count(&self) -> usize {
        self.0.num_tri()
    }
}
