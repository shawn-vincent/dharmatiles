//! The canonical form of what a build reads: the one way a value becomes
//! the bytes a cache key is made of.  The writer ([`Canon`], [`Canonical`])
//! is `dt-value`'s, so the things a tile makes are keyed by the same one
//! (`dt_kit::ThingId`); each crate writes its own types where they are
//! defined (the kit's frame and printer, the elements' values and formulas,
//! the document's sides).  Here is only what is the evaluator's: a printer
//! profile, and the fold into 64 bits the in-memory step cache keys on.

pub use dt_value::{Canon, Canonical};

use crate::options::Profile;

/// The form folded into 64 bits, for the in-memory cache.
#[must_use]
pub fn fnv(c: &Canon) -> u64 {
    crate::hash::bytes(crate::hash::OFFSET, c.bytes())
}

/// A printer profile is its numbers; its name is only what it is called
/// (the store adds that apart, as it names the target).
impl Canonical for Profile {
    fn canon(&self, c: &mut Canon) {
        (**self).canon(c);
    }
}
