//! dt-doc: dharmatiles documents.
//!
//! KDL v2 text is parsed by the `kdl` crate into a node tree, then decoded
//! by hand into the typed model in [`model`], keeping source spans so
//! every mistake is reported through miette at the text that caused it.
//! Parameter values go through the small expression language in [`expr`].
//! [`edit`] changes one value without disturbing anything else in the
//! file.
//!
//! This crate has no geometry dependency, so `dharmatiles check`, an
//! editor or the browser can use it without building Manifold.
//!
//! Status: Phase 0 spike R6 (plan.html).  The model and the look tables
//! in [`schema`] are drafts for the three spike tiles.

pub mod decode;
pub mod edit;
pub mod error;
pub mod expr;
pub mod model;
pub mod schema;

pub use decode::{Known, decode};
pub use error::DocError;
