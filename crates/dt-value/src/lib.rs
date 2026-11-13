//! dharmatiles values: what a document's numbers draw from, with no
//! geometry in it.
//!
//! - [`seed::Seed`], the deterministic path-keyed seed derivation, and the
//!   seeded stream [`Rng`].
//! - [`weighting`]: a range's odds ([`Weighting`], [`Dist`]): one draw, any
//!   width.

pub mod seed;
pub mod weighting;

pub use seed::{Rng, Seed};
pub use weighting::{
    Dist, Draw, ENDS_POWER, MAX_BINS, MAX_LIST, MAX_MIX, Op, WEIGHTINGS, Weighting, WeightingArg,
    WeightingError, WeightingFn, WeightingRest, weighting_names,
};
