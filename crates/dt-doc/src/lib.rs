//! dt-doc: dharmatiles documents.
//!
//! KDL v2 text is parsed by the `kdl` crate into a node tree, then decoded
//! by hand into the typed model in [`model`], keeping source spans so
//! every mistake is reported through miette at the text that caused it,
//! with a hint.  Authors write mm at the DungeonBlocks
//! reference and named heights (`let`), name tile edges, give points as quoted
//! pairs, use words for character, and say how many with a `scatter`.
//!
//! - [`decode`] reads one file; [`uses`] reads only its `use` lines.
//! - [`Known`] holds what a file can see: the library's heights
//!   and the files it `use`s.
//! - [`schema`] and [`elements`] hold every element's controls and words.
//! - [`header`] reads the `dharmatiles version=…` first line, which names a
//!   document's format.
//! - [`edit`] changes, inserts and removes controls without disturbing
//!   anything else in the file.
//!
//! This crate has no geometry dependency, so `dharmatiles check`, an
//! editor or the browser can use it without building Manifold.

pub mod consts;
pub mod decode;
pub mod dim;
pub mod edit;
pub mod elements;
pub mod error;
pub mod expr;
pub mod header;
pub mod known;
pub mod migrate;
pub mod model;
pub mod place;
pub mod schema;

pub use decode::{decode, uses};
pub use error::DocError;
pub use known::Known;
