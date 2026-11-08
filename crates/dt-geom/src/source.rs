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
//!    set, an import) takes its slot with [`Solid::with_original_id`].
//!    Nothing that has more than one source is relabelled, and
//!    `as_original` is not used in pipelines.
//! 3. **Cuts carry an ID the caller gives them.** Plane cuts use
//!    [`Solid::trim_by_plane_with_id`], so the cut face has a slot of the
//!    block, not a minted ID.
//! 4. **A slot has one material.** A triangle's material is its original
//!    ID's slot's. A cut face takes the slot of the solid being cut, so it
//!    has that solid's material.
//! 5. **Combine in index order.** [`batch_in_order`] checks (in debug
//!    builds) that the parts' source IDs are disjoint and ascend with the
//!    part index, then runs [`Solid::batch`].
//!
//! `crates/dt-geom/tests/determinism.rs` runs this pipeline at 1, 4 and 16
//! threads  and checks the hashes and that every output
//! triangle traces to a material.

use crate::{BoolOp, Solid};

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

    /// The slot of `id`, if it belongs to this block.
    #[must_use]
    pub fn slot(&self, id: u32) -> Option<u32> {
        id.checked_sub(self.first).filter(|&s| s < self.len)
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
