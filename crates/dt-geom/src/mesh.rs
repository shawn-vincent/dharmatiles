//! The mesh trait. Every boolean backend (manifold-csg, manifold-rust)
//! implements it, so young crates stay swappable (risk R10).

use std::collections::HashMap;

/// An indexed triangle mesh: the exchange format between backends, export
/// and audit. It mirrors Manifold's `MeshGL64`: vertices may be split where
/// properties differ, and `merge_from`/`merge_to` say which split vertices
/// are one point of the solid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriMesh {
    /// Vertex positions in mm.
    pub positions: Vec<[f64; 3]>,
    /// Counter-clockwise triangles (outward normals) indexing `positions`.
    pub triangles: Vec<[u32; 3]>,
    /// Extra per-vertex properties after x, y, z (`num_extra` per vertex).
    pub extra: Vec<f64>,
    /// Number of extra properties per vertex.
    pub num_extra: usize,
    /// Split vertex `merge_from[i]` is the same point as `merge_to[i]`.
    pub merge_from: Vec<u32>,
    /// See `merge_from`.
    pub merge_to: Vec<u32>,
    /// Triangle runs: run `r` is triangles `run_index[r]/3 .. run_index[r+1]/3`
    /// (indices into the flat vertex-index array, as in `MeshGL`).
    pub run_index: Vec<u32>,
    /// The original (source) ID of each run (R9: material tracking).
    pub run_original_id: Vec<u32>,
    /// Per-triangle face ID (coplanar faces share one). Empty if unknown.
    pub face_id: Vec<u32>,
}

impl TriMesh {
    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// The original ID of every triangle, from the runs (`None` without runs).
    #[must_use]
    pub fn triangle_original_ids(&self) -> Option<Vec<u32>> {
        if self.run_original_id.is_empty() {
            return None;
        }
        let mut ids = Vec::with_capacity(self.triangles.len());
        for (r, &id) in self.run_original_id.iter().enumerate() {
            let start = self.run_index.get(r).map_or(0, |&s| s as usize / 3);
            let end = self
                .run_index
                .get(r + 1)
                .map_or(self.triangles.len(), |&e| e as usize / 3);
            ids.extend(std::iter::repeat_n(id, end.saturating_sub(start)));
        }
        Some(ids)
    }

    /// True if the mesh is a closed, consistently oriented 2-manifold once
    /// split vertices are merged: every directed edge appears once and has
    /// exactly one twin running the other way.
    #[must_use]
    pub fn is_watertight(&self) -> bool {
        if self.triangles.is_empty() {
            return true;
        }
        let mut canon: Vec<u32> = (0..self.positions.len())
            .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
            .collect();
        for (&f, &t) in self.merge_from.iter().zip(&self.merge_to) {
            if let Some(slot) = canon.get_mut(f as usize) {
                *slot = t;
            }
        }
        let mut edges: HashMap<(u32, u32), u32> = HashMap::with_capacity(self.triangles.len() * 3);
        for t in &self.triangles {
            let v = t.map(|i| canon.get(i as usize).copied().unwrap_or(i));
            if v[0] == v[1] || v[1] == v[2] || v[0] == v[2] {
                return false;
            }
            for k in 0..3 {
                *edges.entry((v[k], v[(k + 1) % 3])).or_default() += 1;
            }
        }
        edges
            .iter()
            .all(|(&(a, b), &n)| n == 1 && edges.get(&(b, a)) == Some(&1))
    }

    /// A 64-bit FNV-1a hash of the exact bytes of every field: equal hashes
    /// mean byte-identical meshes (R2).
    #[must_use]
    pub fn content_hash(&self) -> u64 {
        let mut h = Fnv::default();
        h.u64(self.positions.len() as u64);
        for p in &self.positions {
            p.iter().for_each(|x| h.u64(x.to_bits()));
        }
        h.u64(self.triangles.len() as u64);
        for t in &self.triangles {
            t.iter().for_each(|&i| h.u64(u64::from(i)));
        }
        self.extra.iter().for_each(|x| h.u64(x.to_bits()));
        for v in [
            &self.merge_from,
            &self.merge_to,
            &self.run_index,
            &self.face_id,
        ] {
            h.u64(v.len() as u64);
            v.iter().for_each(|&i| h.u64(u64::from(i)));
        }
        h.finish()
    }
}

/// 64-bit FNV-1a, for stage hashes.
#[derive(Clone, Copy, Debug)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    /// Feed bytes.
    pub fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= u64::from(x);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Feed one little-endian u64.
    pub fn u64(&mut self, x: u64) {
        self.bytes(&x.to_le_bytes());
    }

    /// The hash so far.
    #[must_use]
    pub const fn finish(self) -> u64 {
        self.0
    }
}

/// Why a backend refused or failed an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshError {
    /// The input is not a closed, consistently oriented 2-manifold.
    NotManifold,
    /// Any other backend-reported failure.
    Backend(String),
}

impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotManifold => f.write_str("mesh is not a closed 2-manifold"),
            Self::Backend(msg) => write!(f, "mesh backend error: {msg}"),
        }
    }
}

impl std::error::Error for MeshError {}

/// A boolean operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoolOp {
    /// `a ∪ b`.
    Union,
    /// `a − b`.
    Difference,
    /// `a ∩ b`.
    Intersection,
}

/// Axis-aligned bounds of a level set, in mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// Minimum corner.
    pub min: [f64; 3],
    /// Maximum corner.
    pub max: [f64; 3],
}

/// A solid mesh with exact booleans. Implemented once per backend feature.
pub trait Mesh: Sized + Send + Sync {
    /// Build a solid from an indexed triangle mesh.
    ///
    /// # Errors
    /// [`MeshError::NotManifold`] if the input is not a closed 2-manifold.
    fn from_tri_mesh(mesh: &TriMesh) -> Result<Self, MeshError>;

    /// Export back to an indexed triangle mesh.
    fn to_tri_mesh(&self) -> TriMesh;

    /// `self ∪ other`, `self − other` or `self ∩ other`.
    #[must_use]
    fn boolean(&self, other: &Self, op: BoolOp) -> Self;

    /// `self ∪ other`.
    #[must_use]
    fn union(&self, other: &Self) -> Self {
        self.boolean(other, BoolOp::Union)
    }

    /// `self − other`.
    #[must_use]
    fn difference(&self, other: &Self) -> Self {
        self.boolean(other, BoolOp::Difference)
    }

    /// `self ∩ other`.
    #[must_use]
    fn intersection(&self, other: &Self) -> Self {
        self.boolean(other, BoolOp::Intersection)
    }

    /// `op` over all of `parts` in one batch (`parts[0] op parts[1] op …`).
    #[must_use]
    fn batch(parts: &[Self], op: BoolOp) -> Self;

    /// Keep the part on the side of the plane where `normal · x ≥ offset`.
    #[must_use]
    fn trim_by_plane(&self, normal: [f64; 3], offset: f64) -> Self;

    /// `(self ∩ cutter, self − cutter)`.
    #[must_use]
    fn split(&self, cutter: &Self) -> (Self, Self);

    /// `(above, below)` the plane `normal · x = offset`.
    #[must_use]
    fn split_by_plane(&self, normal: [f64; 3], offset: f64) -> (Self, Self);

    /// The solid where `sdf(p) > 0` (positive inside), meshed on a grid of
    /// `edge_length` within `bounds`.
    #[must_use]
    fn level_set(sdf: &(dyn Fn([f64; 3]) -> f64 + Sync), bounds: Bounds, edge_length: f64) -> Self;

    /// This solid as a new original with a fresh original ID (R9).
    #[must_use]
    fn as_original(&self) -> Self;

    /// The original ID, or -1 if this solid is the result of an operation.
    fn original_id(&self) -> i32;

    /// `Ok` unless the backend reports an error status.
    ///
    /// # Errors
    /// The backend's status, as text.
    fn status(&self) -> Result<(), MeshError>;

    /// Enclosed volume in mm³.
    fn volume(&self) -> f64;

    /// Number of triangles (forces evaluation).
    fn triangle_count(&self) -> usize;
}
