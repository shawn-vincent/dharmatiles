//! dt-eval's integration tests, one binary: every test file is a module here,
//! so that [`memo`] can hold one build of each tile (and of each document a
//! test writes) for all of them, and no tile is built twice with the same
//! inputs in a run.  `memo_only` fails a test that builds without the memo
//! and does not say why.  See `memo` for why one build with the observers
//! on stands for the plain one.

#[path = "../common/mod.rs"]
mod common;

mod baseline;
mod cache;
mod cliffs;
mod clouds;
mod code_examples;
mod debug;
mod debug_clouds;
mod dirt_after_things;
mod dirt_on_rocks;
mod docs_examples;
mod documents;
mod edges;
mod floors;
mod format1;
mod golden;
mod heights;
mod memo;
mod memo_only;
mod openlock;
mod order;
mod pipeline;
mod placement;
mod printers;
mod progress;
mod regions;
mod rock_tops;
mod schema_agreement;
mod seams;
mod trace;
mod tree_crowns;
mod units;
mod unwritten;
mod walls;
mod weighting;
