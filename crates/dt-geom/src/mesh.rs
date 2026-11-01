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
    /// The solid's tolerance in mm (`MeshGL::tolerance`); 0 means the
    /// kernel's default for the mesh's size. R1 found the replay must carry
    /// it: Python's operands were float32 imports with ~1e-7 x size, and
    /// re-importing them at the default ~1e-12 x size changes some results.
    pub tolerance: f64,
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
        h.u64(self.tolerance.to_bits());
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

/// A closed box covering `b` on the side `n̂ · x ≥ offset` of the plane
/// (`n̂` = `normal` normalised, as Manifold's plane ops take it), one face on
/// the plane, every triangle carrying original ID `id`.
///
/// # Errors
/// [`MeshError::Backend`] if `normal` is zero or not finite.
pub fn half_space(b: Bounds, normal: [f64; 3], offset: f64, id: u32) -> Result<TriMesh, MeshError> {
    let len = normal.iter().map(|x| x * x).sum::<f64>().sqrt();
    if !(len.is_finite() && len > 0.0) {
        return Err(MeshError::Backend("half-space normal is zero".into()));
    }
    let n = normal.map(|x| x / len);
    let dot = |a: [f64; 3], c: [f64; 3]| a[0] * c[0] + a[1] * c[1] + a[2] * c[2];
    let cross = |a: [f64; 3], c: [f64; 3]| {
        [
            a[1] * c[2] - a[2] * c[1],
            a[2] * c[0] - a[0] * c[2],
            a[0] * c[1] - a[1] * c[0],
        ]
    };
    // u, v, n right-handed, so the unit cube's winding stays outward
    let seed = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u0 = cross(seed, n);
    let ul = dot(u0, u0).sqrt();
    let u = u0.map(|x| x / ul);
    let v = cross(n, u);
    let c = [0, 1, 2].map(|k| 0.5 * (b.min[k] + b.max[k]));
    let half = 0.5
        * (0..3)
            .map(|k| (b.max[k] - b.min[k]).powi(2))
            .sum::<f64>()
            .sqrt();
    let r = 2.0 * half + 1.0;
    let h = r + (dot(n, c) - offset).abs();
    let o = [0, 1, 2].map(|k| c[k] - (dot(n, c) - offset) * n[k]);
    let corner = |i: usize| {
        let a = if i & 1 == 0 { -r } else { r };
        let bb = if i & 2 == 0 { -r } else { r };
        let t = if i & 4 == 0 { 0.0 } else { h };
        [0, 1, 2].map(|k| o[k] + a * u[k] + bb * v[k] + t * n[k])
    };
    Ok(TriMesh {
        positions: (0..8).map(corner).collect(),
        triangles: BOX_TRIANGLES.to_vec(),
        run_index: vec![0, 36],
        run_original_id: vec![id],
        ..TriMesh::default()
    })
}

/// The 12 outward triangles of a box whose corner `i` has bit 0 = +x,
/// bit 1 = +y, bit 2 = +z.
pub const BOX_TRIANGLES: [[u32; 3]; 12] = [
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

/// Axis-aligned bounds of a level set, in mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    /// Minimum corner.
    pub min: [f64; 3],
    /// Maximum corner.
    pub max: [f64; 3],
}

/// A solid mesh with exact booleans. Implemented once per backend feature.
///
/// Source IDs follow the rule in [`crate::source`]: a pipeline gives every
/// leaf solid and every cut an ID from an [`crate::source::IdBlock`]
/// ([`Mesh::with_original_id`], [`Mesh::trim_by_plane_with_id`]) instead of
/// the IDs the kernel mints from its process-wide counter, and never calls
/// [`Mesh::as_original`] on a solid with more than one source.
pub trait Mesh: Sized + Send + Sync + Clone {
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

    /// This solid as a new original with a fresh original ID from the
    /// kernel's process-wide counter. It collapses every source ID into one
    /// and recomputes face IDs, so it cuts the link R9's material tracing
    /// needs; pipelines use [`Mesh::with_original_id`] on leaves instead.
    #[must_use]
    fn as_original(&self) -> Self;

    /// Reserve `n` consecutive original IDs from the kernel's counter and
    /// return the first. Call it once, in sequence, before parallel work.
    fn reserve_ids(n: u32) -> u32;

    /// Axis-aligned bounds, or `None` if empty.
    fn bounds(&self) -> Option<Bounds>;

    /// A leaf solid relabelled so every triangle carries original ID `id`,
    /// and triangle `t` face ID `face_base + t`.
    ///
    /// This is the one allowed round trip through [`TriMesh`]: it re-imports
    /// the mesh with a single run. Use it on a single-source leaf (a level
    /// set, an imported mesh) before any material is recorded for it, and
    /// record materials from the result. Face IDs must not repeat across
    /// the leaves of one combine: the port (manifold-rust 0.19.2) renumbers
    /// some face IDs in a large batch union when two sources share them.
    /// manifold-csg's C API cannot import face IDs, so on the ffi backend
    /// Manifold recomputes them (coplanar groups) and `face_base` is unused;
    /// the C++ kernel keeps them apart by source on its own.
    ///
    /// # Errors
    /// [`MeshError::Backend`] if `face_base + t` overflows `u32`, or if the
    /// kernel refuses its own export (it should not).
    fn with_original_id(&self, id: u32, face_base: u32) -> Result<Self, MeshError> {
        let mut m = self.to_tri_mesh();
        if m.triangles.is_empty() {
            return Ok(self.clone());
        }
        let too_big = || MeshError::Backend("leaf too large for its face-ID range".into());
        let n = u32::try_from(m.triangles.len()).map_err(|_| too_big())?;
        face_base.checked_add(n).ok_or_else(too_big)?;
        m.run_index = vec![0, n * 3];
        m.run_original_id = vec![id];
        m.face_id = (0..n).map(|t| face_base + t).collect();
        Self::from_tri_mesh(&m)
    }

    /// Keep the part where `normal · x ≥ offset` (as [`Mesh::trim_by_plane`]),
    /// with the new cut face carrying original ID `cut_id` instead of an ID
    /// minted by the kernel. The cut is an intersection with a half-space
    /// box imported under `cut_id`, so its result does not depend on which
    /// thread ran first.
    ///
    /// # Errors
    /// [`MeshError`] if the half-space box cannot be built (zero normal).
    fn trim_by_plane_with_id(
        &self,
        normal: [f64; 3],
        offset: f64,
        cut_id: u32,
    ) -> Result<Self, MeshError> {
        let Some(b) = self.bounds() else {
            return Ok(self.clone());
        };
        let hs = Self::from_tri_mesh(&half_space(b, normal, offset, cut_id)?)?;
        Ok(self.intersection(&hs))
    }

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
