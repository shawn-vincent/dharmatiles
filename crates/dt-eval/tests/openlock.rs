//! OpenLOCK: 25.4 mm squares, everything the author wrote scaled by
//! 25.4 / 35, the clip-socket base under z = 0.
//!
//! One tile for each element family, all at once: each must pass the gate
//! and be a photocopy of its DungeonBlocks plan. Threads are the golden
//! tests' business (the system changes the scale and the base, not the
//! order solids are built in).
//!
//! Every library tile on both systems is a release check, not a test run:
//! `cargo test --release -p dt-eval --test openlock -- --ignored`
//! (or `dharmatiles build --all --system ol`).

#![allow(clippy::unwrap_used)]

use dt_eval::{Library, Options, build, check};
use dt_kit::Frame;
use rayon::prelude::*;

const RATIO: f64 = 25.4 / 35.0;

/// One tile for each element family.
const TILES: &[&str] = &[
    "lib:tiles/test/1x1-flat.dtile",
    "lib:tiles/ground/1x1-soil+grass.dtile",
    "lib:tiles/ground/1x1-grass-flowers.dtile",
    "lib:tiles/water/1x1-water+grass.dtile",
    "lib:tiles/walls/1x1-wall-brick-corner.dtile",
    "lib:tiles/floors/1x1-floor-stone-wall.dtile",
    "lib:tiles/cliffs/1x1-cliff.dtile",
    "lib:tiles/test/1x1-tree.dtile",
];

fn on(system: &str) -> Options {
    Options {
        system: system.to_owned(),
        ..Options::default()
    }
}

/// Build `path` for `system` and hold it to the gate; for OpenLOCK, also to
/// the photocopy of its DungeonBlocks plan.
fn passes(path: &str, system: &str) {
    let opts = on(system);
    let b = build(&Library, path, &opts).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    let a = &b.audit;
    let fails = a.failures(&opts.printer);
    assert!(fails.is_empty(), "{path} ({system}): {fails:?}");
    assert!(a.watertight, "{path} ({system})");
    assert!(a.dropped.is_empty(), "{path} ({system}): {:?}", a.dropped);
    assert!(b.out.starts_with(system), "{path}: {:?}", b.out);
    if system != "ol" {
        return;
    }
    let tile = check(&Library, path, &Options::default())
        .unwrap()
        .tile
        .unwrap();
    let db = Frame::for_system("db", tile.cols, tile.rows).size_mm();
    for (built, plan) in b.footprint.iter().zip(db) {
        let ratio = built / plan;
        assert!((ratio - RATIO).abs() < 1e-12, "{path}: {ratio}");
    }
    // the edge level is a photocopy too
    if let (Some(e), Some((_, d))) = (b.edge_mm, tile.levels.iter().find(|(n, _)| n == "edge")) {
        assert!((e / d - RATIO).abs() < 1e-9, "{path}: edge {e} against {d}");
    }
    // the base is 8 mm under the whole tile
    let lo = b
        .export
        .mesh
        .positions
        .iter()
        .map(|p| p[2])
        .fold(f64::INFINITY, f64::min);
    assert!((lo + 8.0).abs() < 1e-3, "{path}: bottom at {lo}");
}

#[test]
fn each_element_family_builds_for_openlock() {
    TILES.par_iter().for_each(|p| passes(p, "ol"));
}

/// Every library tile whose elements are all built.
fn buildable() -> Vec<String> {
    Library::paths()
        .into_iter()
        .filter(|p| p.contains("tiles/"))
        .filter(|p| {
            check(&Library, p, &Options::default())
                .is_ok_and(|c| c.tile.is_some() && c.pending().is_empty())
        })
        .collect()
}

#[test]
#[ignore = "every library tile on both systems: a release check (--ignored)"]
fn every_library_tile_passes_the_gate_on_both_systems() {
    let tiles = buildable();
    assert!(tiles.len() >= 3, "{tiles:?}");
    tiles
        .par_iter()
        .flat_map(|p| [(p, "db"), (p, "ol")])
        .for_each(|(p, s)| passes(p, s));
}
