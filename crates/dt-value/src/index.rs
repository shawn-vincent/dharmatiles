//! Indices that fit, by construction: the one place a `usize` count or index becomes a `u32` or `u8` id.
//!
//! A mesh holds under 2^32 vertices and a stack under 2^8 layers long before memory runs out, so a conversion that
//! does not fit is a broken invariant, never an input: it stops the build with the value, in this one place, instead
//! of saturating at `u32::MAX` at a hundred call sites (see docs/standards/errors.html).

/// A `usize` index or count as a `u32` id.
///
/// # Panics
/// When `i` does not fit in 32 bits: a mesh that large cannot exist in memory, so this is an invariant, not an input.
#[must_use]
pub fn idx(i: usize) -> u32 {
    match u32::try_from(i) {
        Ok(v) => v,
        Err(_) => panic!("index {i} does not fit a 32-bit id (a mesh never holds 2^32 items)"),
    }
}

/// A `usize` index or count as a `u8` id (a layer number).
///
/// # Panics
/// When `i` does not fit in 8 bits.
#[must_use]
pub fn idx8(i: usize) -> u8 {
    match u8::try_from(i) {
        Ok(v) => v,
        Err(_) => panic!("index {i} does not fit an 8-bit id"),
    }
}
