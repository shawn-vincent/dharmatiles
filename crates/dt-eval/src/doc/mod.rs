//! Documents through dt-doc: KDL text → the few facts the evaluator needs.
//!
//! dt-doc does the reading: [`dt_doc::uses`] finds a file's `use` lines,
//! the files they name are decoded first (depth first, each once) and
//! registered in a [`Known`] under their [`dt_doc::model::Use::key`], then
//! [`dt_doc::decode`] reads the file itself.  Every dt-doc error becomes a
//! [`Diag`] at its text, hint and all.
//!
//! What this module adds is the evaluator's side of the model:
//!
//! - `use` keys resolve through the [`Source`]: a bare path or `lib:` is
//!   the bundled library, `./` and `../` are the project, relative to the
//!   using file;
//! - a tile's `let`s that are one number resolve to mm, for the element
//!   controls and line wobbles that name them;
//! - definitions are inlined, with their parameters bound to the caller's
//!   arguments or their defaults;
//! - every control a built element takes is filled in: as written, or its
//!   schema default ([`dt_doc::schema::Control::default_value`]);
//! - the edge contract (library/parts/edges.dtile) is read for every tile;
//! - anything that needs an element not built yet is set aside in
//!   [`Tile::pending`]: `check` accepts it, `build` refuses it.
//!
//! The evaluator only sees [`Tile`].

mod model;
mod reader;

pub use model::*;
pub use reader::*;
