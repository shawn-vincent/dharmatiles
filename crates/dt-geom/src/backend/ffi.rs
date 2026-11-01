//! Manifold's C++ kernel through `manifold-csg` (FFI).
//!
//! Gap found in R1: the C API's `MeshGL64` constructor takes runs, merge
//! vectors and tangents but not `faceID` or run transforms, so face IDs of
//! an imported mesh are recomputed by Manifold rather than carried in.

use manifold_csg::{Manifold, MeshGL64, MeshGL64Options, OpType};

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

    fn as_original(&self) -> Self {
        Self(self.0.as_original())
    }

    fn original_id(&self) -> i32 {
        self.0.original_id()
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
