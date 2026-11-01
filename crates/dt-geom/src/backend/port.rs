//! The pure-Rust Manifold port (`manifold-rust`).
//!
//! Note from R1: its `batch_boolean` folds left to right, where the C++
//! kernel unions by size with a priority queue. `batch` here does the
//! size-ordered union itself, so large batches stay tractable.

use manifold_rust::linalg::Vec3;
use manifold_rust::manifold::Manifold;
use manifold_rust::types::{Box as BBox, Error, MeshGLP, OpType};

use super::{deinterleave, interleave};
use crate::mesh::{BoolOp, Bounds, Mesh, MeshError, TriMesh};

/// A solid in the pure-Rust port.
#[derive(Clone)]
pub struct PortSolid(pub Manifold);

const fn op(op: BoolOp) -> OpType {
    match op {
        BoolOp::Union => OpType::Add,
        BoolOp::Difference => OpType::Subtract,
        BoolOp::Intersection => OpType::Intersect,
    }
}

const fn v3(a: [f64; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

impl Mesh for PortSolid {
    fn from_tri_mesh(mesh: &TriMesh) -> Result<Self, MeshError> {
        let (props, np) = interleave(mesh);
        let gl = MeshGLP::<f64, u32> {
            num_prop: u32::try_from(np).unwrap_or(3),
            vert_properties: props,
            tri_verts: mesh.triangles.iter().flatten().copied().collect(),
            merge_from_vert: mesh.merge_from.clone(),
            merge_to_vert: mesh.merge_to.clone(),
            run_index: mesh.run_index.clone(),
            run_original_id: mesh.run_original_id.clone(),
            face_id: mesh.face_id.clone(),
            tolerance: mesh.tolerance,
            ..Default::default()
        };
        let m = Manifold::from_mesh_glp(&gl);
        match m.status() {
            Error::NoError => Ok(Self(m)),
            Error::NotManifold => Err(MeshError::NotManifold),
            e => Err(MeshError::Backend(e.to_string())),
        }
    }

    fn to_tri_mesh(&self) -> TriMesh {
        let gl = self.0.get_mesh_glp::<f64, u32>(-1);
        let np = gl.num_prop as usize;
        let (positions, extra) = deinterleave(&gl.vert_properties, np);
        TriMesh {
            positions,
            triangles: gl.tri_verts.as_chunks::<3>().0.to_vec(),
            extra,
            num_extra: np.saturating_sub(3),
            merge_from: gl.merge_from_vert,
            merge_to: gl.merge_to_vert,
            run_index: gl.run_index,
            run_original_id: gl.run_original_id,
            face_id: gl.face_id,
            tolerance: gl.tolerance,
        }
    }

    fn boolean(&self, other: &Self, o: BoolOp) -> Self {
        Self(self.0.boolean(&other.0, op(o)))
    }

    fn batch(parts: &[Self], o: BoolOp) -> Self {
        if o != BoolOp::Union || parts.len() < 3 {
            let ms: Vec<Manifold> = parts.iter().map(|p| p.0.clone()).collect();
            return Self(Manifold::batch_boolean(&ms, op(o)));
        }
        // The port's batch_boolean folds left to right, so every step
        // re-processes the growing result. Union the two smallest parts
        // first instead, as the C++ kernel's batch union does (R1).
        let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<(usize, usize)>> =
            std::collections::BinaryHeap::new();
        let mut slots: Vec<Option<Manifold>> = Vec::with_capacity(parts.len() * 2);
        for p in parts {
            heap.push(std::cmp::Reverse((p.0.num_tri(), slots.len())));
            slots.push(Some(p.0.clone()));
        }
        while heap.len() > 1 {
            let (Some(std::cmp::Reverse((_, a))), Some(std::cmp::Reverse((_, b)))) =
                (heap.pop(), heap.pop())
            else {
                break;
            };
            let (Some(ma), Some(mb)) = (slots[a].take(), slots[b].take()) else {
                break;
            };
            let u = ma.union(&mb);
            heap.push(std::cmp::Reverse((u.num_tri(), slots.len())));
            slots.push(Some(u));
        }
        Self(
            slots
                .into_iter()
                .flatten()
                .next()
                .unwrap_or_else(Manifold::empty),
        )
    }

    fn trim_by_plane(&self, normal: [f64; 3], offset: f64) -> Self {
        Self(self.0.trim_by_plane(v3(normal), offset))
    }

    fn split(&self, cutter: &Self) -> (Self, Self) {
        let (a, b) = self.0.split(&cutter.0);
        (Self(a), Self(b))
    }

    fn split_by_plane(&self, normal: [f64; 3], offset: f64) -> (Self, Self) {
        let (a, b) = self.0.split_by_plane(v3(normal), offset);
        (Self(a), Self(b))
    }

    fn level_set(sdf: &(dyn Fn([f64; 3]) -> f64 + Sync), bounds: Bounds, edge_length: f64) -> Self {
        let b = BBox {
            min: v3(bounds.min),
            max: v3(bounds.max),
        };
        Self(Manifold::level_set(
            |p: Vec3| sdf([p.x, p.y, p.z]),
            b,
            edge_length,
        ))
    }

    fn as_original(&self) -> Self {
        Self(self.0.as_original())
    }

    fn original_id(&self) -> i32 {
        self.0.original_id()
    }

    fn reserve_ids(n: u32) -> u32 {
        Manifold::reserve_ids(n)
    }

    fn bounds(&self) -> Option<Bounds> {
        if self.0.is_empty() {
            return None;
        }
        let b = self.0.bounding_box();
        Some(Bounds {
            min: [b.min.x, b.min.y, b.min.z],
            max: [b.max.x, b.max.y, b.max.z],
        })
    }

    fn status(&self) -> Result<(), MeshError> {
        match self.0.status() {
            Error::NoError => Ok(()),
            e => Err(MeshError::Backend(e.to_string())),
        }
    }

    fn volume(&self) -> f64 {
        self.0.volume()
    }

    fn triangle_count(&self) -> usize {
        self.0.num_tri()
    }
}
