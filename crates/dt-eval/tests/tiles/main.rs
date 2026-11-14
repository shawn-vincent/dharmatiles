//! The test tiles (`library/tests/`), each built once for the whole binary.
//!
//! These were separate test files, each building the same tiles again; they
//! are modules here so that [`memo`] can hold one build of each tile, with
//! every observer on, for all of them.  See `memo` for why one build
//! with the observers on stands for the plain one.

#[path = "../common/mod.rs"]
mod common;

mod cliffs;
mod clouds;
mod debug_clouds;
mod floors;
mod golden;
mod memo;
mod placement;
mod progress;
mod trace;
mod walls;
