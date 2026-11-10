//! Every library tile's build order, pinned: the order decides each
//! solid's slot and so every hash, so a change to the planner that moves
//! any tile's order shows here first.

#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;

use dt_eval::{Library, Options, check};

#[test]
fn every_library_tile_builds_in_the_order_it_did() {
    let mut text = String::new();
    for path in Library::paths()
        .expect("the library is listed")
        .into_iter()
        .filter(|p| p.starts_with("lib:tiles/") || p.starts_with("lib:tests/"))
    {
        match check(&Library, &path, &Options::default()) {
            Ok(c) => writeln!(text, "{path}: {:?}", c.steps).unwrap(),
            Err(_) => writeln!(text, "{path}: refused").unwrap(),
        }
    }
    insta::assert_snapshot!(text);
}
