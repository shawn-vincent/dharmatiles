//! The OpenLOCK build of every library tile that builds (plan.html, P5:
//! "an OpenLOCK build of every tile converted so far"; Practices: "The
//! OpenLOCK build has been checked, from P5 on").
//!
//! Each tile is built for `--system ol`: 25.4 mm squares, everything the
//! author wrote scaled by 25.4 / 35 (D2), the clip-socket base under
//! z = 0. It must pass the gate, be a photocopy of the DungeonBlocks
//! build in plan, and be the same at 1 and 4 threads.

#![allow(clippy::unwrap_used)]

use dt_eval::{Built, Library, Options, build, check};

fn ol() -> Options {
    Options {
        system: "ol".to_owned(),
        ..Options::default()
    }
}

fn build_at(path: &str, opts: &Options, threads: usize) -> Built {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap();
    pool.install(|| build(&Library, path, opts))
        .unwrap_or_else(|e| panic!("{path}: {e:?}"))
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
fn every_buildable_tile_builds_for_openlock() {
    let tiles = buildable();
    assert!(tiles.len() >= 3, "{tiles:?}");
    for path in &tiles {
        let b = build_at(path, &ol(), 4);
        let a = &b.audit;
        let fails = a.failures(&ol().printer);
        assert!(fails.is_empty(), "{path}: {fails:?}");
        assert!(a.watertight, "{path}");
        assert!(a.dropped.is_empty(), "{path}: {:?}", a.dropped);
        assert!(b.out.starts_with("ol"), "{path}: {:?}", b.out);
        let db = build_at(path, &Options::default(), 4);
        for k in 0..2 {
            let ratio = b.footprint[k] / db.footprint[k];
            assert!((ratio - 25.4 / 35.0).abs() < 1e-12, "{path}: {ratio}");
        }
        // the edge level is a photocopy too
        if let (Some(e), Some(d)) = (b.edge_mm, db.edge_mm) {
            assert!((e / d - 25.4 / 35.0).abs() < 1e-12, "{path}");
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
}

#[test]
fn openlock_builds_are_thread_identical() {
    for path in buildable() {
        let a = build_at(&path, &ol(), 1);
        let b = build_at(&path, &ol(), 4);
        assert_eq!(a.stages, b.stages, "{path}");
    }
}
