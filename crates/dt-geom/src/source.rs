//! Source identity: one rule that gives both determinism and material
//! tracing.
//!
//! Manifold tags every triangle with an original ID and a face ID, and
//! orders a boolean's output by original ID. The IDs it mints itself come
//! from a process-wide counter, so parts built on several threads get them
//! in scheduling order, and `as_original` collapses a part's sources
//! into one and recomputes its face IDs (cutting material tracing's link back to the
//! level-set triangles a material was sampled on). The rule:
//!
//! 1. **Reserve, then assign by index.** Before parallel work, reserve one
//!    [`IdBlock`] in sequence. Part `i`'s IDs are fixed slots of it
//!    (`block.id(i * per_part + k)`), so their order is the index order
//!    whichever thread builds the part.
//! 2. **Relabel leaves, not combinations.** A leaf the kernel made (a level
//!    set, an import) takes its slot with [`Solid::with_original_id`] before
//!    anything is recorded for it, with face IDs from the slot's own range
//!    ([`IdBlock::face_base`]), so no two leaves share a face ID. Nothing that has more than one source is
//!    relabelled, and `as_original` is not used in pipelines.
//! 3. **Cuts carry an ID the caller gives them.** Plane cuts use
//!    [`Solid::trim_by_plane_with_id`], so the cut face has a slot of the
//!    block, not a minted ID.
//! 4. **Materials live in a [`Materials`] table**, keyed by original ID and
//!    face ID, filled from each leaf right after rule 2, and merged in index
//!    order. Rule for cut faces: a cut face takes the material of the solid
//!    being cut (its base material), registered under the cut's ID.
//! 5. **Combine in index order.** [`batch_in_order`] checks (in debug
//!    builds) that the parts' source IDs are disjoint and ascend with the
//!    part index, then runs [`Solid::batch`].
//!
//! `crates/dt-geom/tests/determinism.rs` runs this pipeline at 1, 4 and 16
//! threads  and checks the hashes and that every output
//! triangle traces to a material.

use std::collections::BTreeMap;

use crate::{BoolOp, Solid, TriMesh};

/// Face IDs per slot (2^20, about a million triangles per leaf).
pub const FACE_STRIDE: u32 = 1 << 20;
/// Slots with a face range (`u32` face IDs / [`FACE_STRIDE`]).
pub const MAX_SLOTS: u32 = 1 << 12;

/// A block of consecutive original IDs reserved from the kernel's counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdBlock {
    first: u32,
    len: u32,
}

impl IdBlock {
    /// Reserve `len` IDs. Call in sequence (not from parallel tasks).
    #[must_use]
    pub fn reserve(len: u32) -> Self {
        Self {
            first: Solid::reserve_ids(len),
            len,
        }
    }

    /// The `slot`-th ID of the block.
    ///
    /// # Panics
    /// If `slot` is outside the block.
    #[must_use]
    pub fn id(&self, slot: u32) -> u32 {
        assert!(
            slot < self.len,
            "ID slot {slot} outside a block of {}",
            self.len
        );
        self.first + slot
    }

    /// The first face ID of `slot`'s range: [`FACE_STRIDE`] IDs per slot,
    /// so a leaf may have up to that many triangles. Widening face IDs to u64
    /// (as `MeshGL64` has them) would lift both limits.
    ///
    /// # Panics
    /// If `slot` is outside the block or past [`MAX_SLOTS`].
    #[must_use]
    pub fn face_base(&self, slot: u32) -> u32 {
        assert!(
            slot < self.len && slot < MAX_SLOTS,
            "face range for slot {slot}"
        );
        slot * FACE_STRIDE
    }

    /// The slot of `id`, if it belongs to this block.
    #[must_use]
    pub fn slot(&self, id: u32) -> Option<u32> {
        id.checked_sub(self.first).filter(|&s| s < self.len)
    }
}

/// What a triangle's material is, by (original ID, face ID) or by original
/// ID alone. A face entry wins over an ID entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Materials {
    faces: BTreeMap<(u32, u32), u32>,
    ids: BTreeMap<u32, u32>,
    /// Faces whose triangles were sampled with more than one material; the
    /// lowest-index triangle's material is kept.
    pub conflicts: usize,
}

impl Materials {
    /// Every triangle carrying original ID `id` has `material` (a solid of
    /// one material, or a cut face).
    pub fn set_id(&mut self, id: u32, material: u32) {
        self.ids.insert(id, material);
    }

    /// Record a leaf's materials face by face: `material(t)` for each
    /// triangle `t` of `leaf` (the leaf's own export, after
    /// [`Solid::with_original_id`]). Coplanar triangles share a face ID; the
    /// first triangle of a face decides it.
    pub fn set_faces(&mut self, leaf: &TriMesh, material: impl Fn(usize) -> u32) {
        let Some(ids) = leaf.triangle_original_ids() else {
            return;
        };
        let mut seen = BTreeMap::new();
        for (t, &id) in ids.iter().enumerate() {
            let face = leaf.face_id.get(t).copied().unwrap_or(u32::MAX);
            let m = material(t);
            match seen.get(&(id, face)) {
                None => {
                    seen.insert((id, face), m);
                }
                Some(&first) if first != m => self.conflicts += 1,
                Some(_) => {}
            }
        }
        self.faces.extend(seen);
    }

    /// Add another table (built in parallel); call in index order.
    pub fn merge(&mut self, other: Self) {
        self.faces.extend(other.faces);
        self.ids.extend(other.ids);
        self.conflicts += other.conflicts;
    }

    /// The material of (original ID, face ID).
    #[must_use]
    pub fn get(&self, id: u32, face: u32) -> Option<u32> {
        self.faces
            .get(&(id, face))
            .or_else(|| self.ids.get(&id))
            .copied()
    }

    /// The material of every triangle of `mesh` (`None` = untraced).
    #[must_use]
    pub fn trace(&self, mesh: &TriMesh) -> Vec<Option<u32>> {
        let Some(ids) = mesh.triangle_original_ids() else {
            return vec![None; mesh.triangle_count()];
        };
        ids.iter()
            .enumerate()
            .map(|(t, &id)| self.get(id, mesh.face_id.get(t).copied().unwrap_or(u32::MAX)))
            .collect()
    }
}

/// The distinct original IDs a solid carries, ascending.
#[must_use]
pub fn source_ids(s: &Solid) -> Vec<u32> {
    let mut v = s.to_tri_mesh().run_original_id;
    v.sort_unstable();
    v.dedup();
    v
}

/// `Err` unless the parts' source IDs are pairwise disjoint and every ID of
/// part `i` is below every ID of part `i + 1` (rule 5).
///
/// # Errors
/// A message naming the first part out of order.
pub fn check_source_order(parts: &[Solid]) -> Result<(), String> {
    let mut prev_max: Option<u32> = None;
    for (i, p) in parts.iter().enumerate() {
        let ids = source_ids(p);
        let (Some(&lo), Some(&hi)) = (ids.first(), ids.last()) else {
            continue;
        };
        if let Some(m) = prev_max
            && lo <= m
        {
            return Err(format!(
                "part {i}: source IDs {ids:?} not above the previous parts' (max {m})"
            ));
        }
        prev_max = Some(hi);
    }
    Ok(())
}

/// [`Solid::batch`] over parts whose source IDs follow the rule; debug
/// builds check it with [`check_source_order`].
///
/// # Panics
/// In debug builds, if the parts break rule 5.
#[must_use]
pub fn batch_in_order(parts: &[Solid], op: BoolOp) -> Solid {
    #[cfg(debug_assertions)]
    if let Err(e) = check_source_order(parts) {
        panic!("batch_in_order: {e}");
    }
    Solid::batch(parts, op)
}
