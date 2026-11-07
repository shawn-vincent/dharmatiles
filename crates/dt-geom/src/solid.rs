//! [`Solid`]: a solid mesh with exact booleans, in Manifold's C++ kernel
//! through `manifold-csg`.
//!
//! The C API's `MeshGL64` constructor takes runs, merge
//! vectors and tangents but not `faceID`, run transforms or `tolerance`, so
//! face IDs of an imported mesh are recomputed by Manifold rather than
//! carried in, and a tolerance is applied after import with
//! `set_tolerance`, which re-simplifies the mesh at it (a `MeshGL` import
//! with the tolerance set does not).

use manifold_csg::{CrossSection, FillRule, JoinType, Manifold, MeshGL64, MeshGL64Options, OpType};

use crate::mesh::{BoolOp, Bounds, MeshError, TriMesh, half_space};

/// A solid mesh with exact booleans.
///
/// Source IDs follow the rule in [`crate::source`]: a pipeline gives every
/// leaf solid and every cut an ID from an [`crate::source::IdBlock`]
/// ([`Solid::with_original_id`], [`Solid::trim_by_plane_with_id`]) instead of
/// the IDs the kernel mints from its process-wide counter, and never calls
/// [`Solid::as_original`] on a solid with more than one source.
#[derive(Clone)]
pub struct Solid(pub Manifold);

const fn op(op: BoolOp) -> OpType {
    match op {
        BoolOp::Union => OpType::Add,
        BoolOp::Difference => OpType::Subtract,
        BoolOp::Intersection => OpType::Intersect,
    }
}

impl Solid {
    /// Build a solid from an indexed triangle mesh.
    ///
    /// # Errors
    /// [`MeshError::NotManifold`] if the input is not a closed 2-manifold.
    pub fn from_tri_mesh(mesh: &TriMesh) -> Result<Self, MeshError> {
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

    /// Export back to an indexed triangle mesh.
    pub fn to_tri_mesh(&self) -> TriMesh {
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

    /// `self ∪ other`, `self − other` or `self ∩ other`.
    #[must_use]
    pub fn boolean(&self, other: &Self, o: BoolOp) -> Self {
        Self(self.0.boolean(&other.0, op(o)))
    }

    /// `self ∪ other`.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        self.boolean(other, BoolOp::Union)
    }

    /// `self − other`.
    #[must_use]
    pub fn difference(&self, other: &Self) -> Self {
        self.boolean(other, BoolOp::Difference)
    }

    /// `self ∩ other`.
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Self {
        self.boolean(other, BoolOp::Intersection)
    }

    /// `op` over all of `parts` in one batch (`parts[0] op parts[1] op …`).
    #[must_use]
    pub fn batch(parts: &[Self], o: BoolOp) -> Self {
        let ms: Vec<Manifold> = parts.iter().map(|p| p.0.clone()).collect();
        Self(Manifold::batch_boolean(&ms, op(o)))
    }

    /// Keep the part on the side of the plane where `normal · x ≥ offset`.
    #[must_use]
    pub fn trim_by_plane(&self, normal: [f64; 3], offset: f64) -> Self {
        Self(self.0.trim_by_plane(normal, offset))
    }

    /// `(self ∩ cutter, self − cutter)`.
    #[must_use]
    pub fn split(&self, cutter: &Self) -> (Self, Self) {
        let (a, b) = self.0.split(&cutter.0);
        (Self(a), Self(b))
    }

    /// `(above, below)` the plane `normal · x = offset`.
    #[must_use]
    pub fn split_by_plane(&self, normal: [f64; 3], offset: f64) -> (Self, Self) {
        let (a, b) = self.0.split_by_plane(normal, offset);
        (Self(a), Self(b))
    }

    /// The solid where `sdf(p) > 0` (positive inside), meshed on a grid of
    /// `edge_length` within `bounds`.
    #[must_use]
    pub fn level_set(
        sdf: &(dyn Fn([f64; 3]) -> f64 + Sync),
        bounds: Bounds,
        edge_length: f64,
    ) -> Self {
        Self(Manifold::from_sdf(
            |x, y, z| sdf([x, y, z]),
            (bounds.min, bounds.max),
            edge_length,
            0.0,
            -1.0,
        ))
    }

    /// [`Solid::level_set`] meshed in one thread. The kernel's parallel
    /// level set is not reproducible: its vertex count can change from run
    /// to run on one input (a 2 mm brick: 1109 or 1110 vertices), which
    /// breaks determinism level B.
    #[must_use]
    pub fn level_set_seq(
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

    /// The even-odd region of `contours` (closed polygons in mm, x and y),
    /// shrunk by `inset` mm with mitred corners when `inset > 0`, as a prism
    /// from `z0` up to `z1`.
    #[must_use]
    pub fn extrude(contours: &[Vec<[f64; 2]>], inset: f64, z0: f64, z1: f64) -> Self {
        let mut cs = CrossSection::from_polygons_with_fill_rule(contours, FillRule::EvenOdd);
        if inset > 0.0 {
            // the miter join, as `CrossSection::offset` defaults to.
            cs = cs.offset(-inset, JoinType::Miter, 2.0, 0);
        }
        Self(Manifold::extrude(&cs, z1 - z0).translate(0.0, 0.0, z0))
    }

    /// The convex hull of `points`.
    #[must_use]
    pub fn hull(points: &[[f64; 3]]) -> Self {
        Self(Manifold::hull_pts(points))
    }

    /// Split edges until none is longer than `length` mm (the grain
    /// pass refines before it moves vertices).
    #[must_use]
    pub fn refine_to_length(&self, length: f64) -> Self {
        Self(self.0.refine_to_length(length))
    }

    /// An affine map: `m` is column-major 3 × 4 (the x, y and z columns of
    /// the linear part, then the translation), as Manifold takes it.
    #[must_use]
    pub fn transform(&self, m: &[f64; 12]) -> Self {
        Self(self.0.transform(m))
    }

    /// The cross-section at height `z`: closed polygons in x and y, holes
    /// wound the other way.
    #[must_use]
    pub fn slice(&self, z: f64) -> Vec<Vec<[f64; 2]>> {
        self.0.slice_at_z(z)
    }

    /// This solid as a new original with a fresh original ID from the
    /// kernel's process-wide counter. It collapses every source ID into one
    /// and recomputes face IDs, so it cuts the link material tracing
    /// needs; pipelines use [`Solid::with_original_id`] on leaves instead.
    #[must_use]
    pub fn as_original(&self) -> Self {
        Self(self.0.as_original())
    }

    /// Reserve `n` consecutive original IDs from the kernel's counter and
    /// return the first. Call it once, in sequence, before parallel work.
    pub fn reserve_ids(n: u32) -> u32 {
        manifold_csg::reserve_ids(n)
    }

    /// Axis-aligned bounds, or `None` if empty.
    pub fn bounds(&self) -> Option<Bounds> {
        self.0.bounding_box().map(|b| Bounds {
            min: b.min(),
            max: b.max(),
        })
    }

    /// A leaf solid relabelled so every triangle carries original ID `id`,
    /// and triangle `t` face ID `face_base + t`.
    ///
    /// This is the one allowed round trip through [`TriMesh`]: it re-imports
    /// the mesh with a single run. Use it on a single-source leaf (a level
    /// set, an imported mesh) before any material is recorded for it, and
    /// record materials from the result. Face IDs must not repeat across
    /// the leaves of one combine: a backend may renumber
    /// some face IDs in a large batch union when two sources share them.
    /// manifold-csg's C API cannot import face IDs, so Manifold recomputes them (coplanar groups) and `face_base` is unused;
    /// the C++ kernel keeps them apart by source on its own.
    ///
    /// # Errors
    /// [`MeshError::Backend`] if `face_base + t` overflows `u32`, or if the
    /// kernel refuses its own export (it should not).
    pub fn with_original_id(&self, id: u32, face_base: u32) -> Result<Self, MeshError> {
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

    /// Keep the part where `normal · x ≥ offset` (as [`Solid::trim_by_plane`]),
    /// with the new cut face carrying original ID `cut_id` instead of an ID
    /// minted by the kernel. The cut is an intersection with a half-space
    /// box imported under `cut_id`, so its result does not depend on which
    /// thread ran first.
    ///
    /// # Errors
    /// [`MeshError`] if the half-space box cannot be built (zero normal).
    pub fn trim_by_plane_with_id(
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
    pub fn original_id(&self) -> i32 {
        self.0.original_id()
    }

    /// `Ok` unless the backend reports an error status.
    ///
    /// # Errors
    /// The backend's status, as text.
    pub fn status(&self) -> Result<(), MeshError> {
        self.0
            .status()
            .map_err(|e| MeshError::Backend(e.to_string()))
    }

    /// Enclosed volume in mm³.
    pub fn volume(&self) -> f64 {
        self.0.volume()
    }

    /// Number of triangles (forces evaluation).
    pub fn triangle_count(&self) -> usize {
        self.0.num_tri()
    }
}

/// Interleave positions and extra properties into `MeshGL` vertex rows.
fn interleave(m: &TriMesh) -> (Vec<f64>, usize) {
    let np = 3 + m.num_extra;
    let mut props = Vec::with_capacity(m.positions.len() * np);
    for (i, p) in m.positions.iter().enumerate() {
        props.extend_from_slice(p);
        if m.num_extra > 0 {
            props.extend_from_slice(&m.extra[i * m.num_extra..(i + 1) * m.num_extra]);
        }
    }
    (props, np)
}

/// Split `MeshGL` vertex rows back into positions and extra properties.
fn deinterleave(props: &[f64], np: usize) -> (Vec<[f64; 3]>, Vec<f64>) {
    let np = np.max(3);
    let n = props.len() / np;
    let mut pos = Vec::with_capacity(n);
    let mut extra = Vec::with_capacity(n * (np - 3));
    for row in props.chunks_exact(np) {
        pos.push([row[0], row[1], row[2]]);
        extra.extend_from_slice(&row[3..]);
    }
    (pos, extra)
}

/// Narrow an index buffer that the kernel already limits to 32 bits.
fn narrow(v: impl IntoIterator<Item = u64>) -> Vec<u32> {
    v.into_iter()
        .map(|x| u32::try_from(x).unwrap_or(u32::MAX))
        .collect()
}
