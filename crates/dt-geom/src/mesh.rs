//! The mesh trait. The boolean backend (manifold-csg) implements it, so
//! the kernel stays swappable.

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
    /// The original (source) ID of each run.
    pub run_original_id: Vec<u32>,
    /// Per-triangle face ID (coplanar faces share one). Empty if unknown.
    pub face_id: Vec<u32>,
    /// The solid's tolerance in mm (`MeshGL::tolerance`); 0 means the
    /// kernel's default for the mesh's size. A float32 import
    /// carries ~1e-7 x size, and re-importing it at the default ~1e-12 x
    /// size changes some results, so imports keep their own.
    pub tolerance: f64,
}

impl TriMesh {
    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Add `other`'s triangles after this mesh's, keeping both meshes'
    /// runs (and so every triangle's original ID).  Nothing is joined:
    /// overlaps stay.  Per-vertex properties and face IDs are kept only
    /// when both meshes have them alike.
    pub fn append(&mut self, other: &TriMesh) {
        let base_v = u32::try_from(self.positions.len()).unwrap_or(u32::MAX);
        let base_i = u32::try_from(self.triangles.len() * 3).unwrap_or(u32::MAX);
        let first = self.triangles.is_empty() && self.positions.is_empty();
        if first {
            self.num_extra = other.num_extra;
            self.tolerance = other.tolerance;
        }
        if self.num_extra == other.num_extra {
            self.extra.extend_from_slice(&other.extra);
        } else {
            self.extra.clear();
            self.num_extra = 0;
        }
        if first || (!self.face_id.is_empty() && !other.face_id.is_empty()) {
            self.face_id.extend_from_slice(&other.face_id);
        } else {
            self.face_id.clear();
        }
        self.tolerance = self.tolerance.max(other.tolerance);
        self.positions.extend_from_slice(&other.positions);
        self.triangles
            .extend(other.triangles.iter().map(|t| t.map(|i| i + base_v)));
        self.merge_from
            .extend(other.merge_from.iter().map(|i| i + base_v));
        self.merge_to
            .extend(other.merge_to.iter().map(|i| i + base_v));
        if !other.run_original_id.is_empty() {
            // runs are `run_index[r] .. run_index[r + 1]`: the last bound of
            // this mesh is the first of the next
            if !self.run_index.is_empty() {
                self.run_index.pop();
            }
            self.run_index
                .extend(other.run_index.iter().map(|i| i + base_i));
            self.run_original_id
                .extend_from_slice(&other.run_original_id);
        }
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
    /// mean byte-identical meshes.
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
