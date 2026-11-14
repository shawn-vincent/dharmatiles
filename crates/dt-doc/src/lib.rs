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
//! - [`ops`] makes every edit of a `.dtile`, a print list and an inventory,
//!   by the id each node or piece carries ([`nodeid`] names a tile's nodes),
//!   without disturbing anything else in the text, with one error type.
//!
//! - [`lower`] turns a written value into the numbers and odds a build draws from.
//!
//! This crate has no geometry dependency, so `dharmatiles check`, an
//! editor or the browser can use it without building Manifold.

pub mod consts;
pub mod decode;
pub mod dim;
pub mod elements;
pub mod error;
pub mod expr;
pub mod framed;
pub mod header;
pub mod inventory;
pub mod known;
pub mod lower;
pub mod map;
pub mod model;
pub mod nodeid;
pub mod ops;
pub mod paths;
pub mod place;
pub mod printed;
pub mod schema;
pub mod seams;
pub mod units;
pub mod words;

pub use decode::{decode, uses};
pub use error::DocError;
pub use known::Known;
