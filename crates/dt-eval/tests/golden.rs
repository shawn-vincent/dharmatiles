//! Golden summaries per tile (plan.html, "The parity harness"): stage
//! hashes, triangle count, volume, overhang mm² and see-through holes,
//! as insta snapshots; `cargo insta review` shows what changed.
//!
//! Determinism level B (Shawn, 2 Nov 2026): every tile is built twice at
//! 1, 4 and 16 threads and every stage hash must agree on every platform;
//! the hash snapshot is shared by macOS and Linux; Windows checks the
//! metrics snapshot and the render gate only.

#![allow(clippy::unwrap_used)]

use dt_eval::{Built, Library, Options, build};
use dt_kit::{VIEWS, paint, render};

const TILES: &[&str] = &["lib:tiles/test/flat-1x1.kdl"];

fn build_at(path: &str, threads: usize) -> Built {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap();
    pool.install(|| build(&Library, path, &Options::default()))
        .unwrap_or_else(|e| panic!("{path}: {e:?}"))
}

fn hashes(b: &Built) -> String {
    b.stages
        .iter()
        .map(|s| format!("{:016x}  {}\n", s.hash, s.name))
        .collect()
}

/// Metrics that every platform must agree on.
fn metrics(b: &Built) -> String {
    let a = &b.audit;
    let img = render(
        &b.export.mesh,
        &paint(&b.materials, &a.overhang),
        &VIEWS[0],
        b.footprint,
        (600, 475),
    );
    format!(
        "triangles: {}\nvolume_mm3: {:.3}\nwatertight: {}\nbodies: {}\ndropped: {}\noverhang_mm2: {:.3}\nholes: {}\nrender_red_pixels: {}\n",
        a.triangles,
        a.volume_mm3,
        a.watertight,
        a.bodies,
        a.dropped.len(),
        a.overhang_mm2,
        a.holes,
        img.red_pixels(),
    )
}

#[test]
fn stage_hashes_agree_at_1_4_and_16_threads() {
    for path in TILES {
        let first = hashes(&build_at(path, 1));
        for threads in [1, 4, 16] {
            for run in 0..2 {
                assert_eq!(
                    hashes(&build_at(path, threads)),
                    first,
                    "{path}: {threads} threads, run {run}"
                );
            }
        }
        println!("{path}\n{first}");
    }
}

#[test]
fn golden_metrics() {
    for path in TILES {
        let b = build_at(path, 4);
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".kdl");
        insta::assert_snapshot!(format!("{name}-metrics"), metrics(&b));
    }
}

#[test]
#[cfg(not(windows))]
fn golden_hashes() {
    for path in TILES {
        let b = build_at(path, 4);
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".kdl");
        insta::assert_snapshot!(format!("{name}-hashes"), hashes(&b));
    }
}

#[test]
fn the_flat_tile_meets_the_exit_criteria() {
    let b = build_at("lib:tiles/test/flat-1x1.kdl", 4);
    let a = &b.audit;
    assert!(a.watertight, "watertight after the float32 reload");
    assert_eq!(a.overhang_mm2, 0.0);
    assert!(a.dropped.is_empty(), "{:?}", a.dropped);
    assert_eq!(a.holes, 0);
    assert!(a.failures(&Options::default().printer).is_empty());
    // the DungeonBlocks envelope: 35 × 35 mm, 5 mm ground, short peg
    let (lo, hi) = b.export.mesh.positions.iter().fold(
        ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
        |(lo, hi), p| {
            (
                [0, 1, 2].map(|k| lo[k].min(p[k])),
                [0, 1, 2].map(|k| hi[k].max(p[k])),
            )
        },
    );
    let close = |a: f64, b: f64| (a - b).abs() < 1e-5;
    assert!(close(lo[0], 0.0) && close(hi[0], 35.0) && close(lo[1], 0.0) && close(hi[1], 35.0));
    assert!(close(hi[2], 5.0), "top {}", hi[2]);
    assert!(close(lo[2], -(5.7 + 5.2) - 0.0), "peg tip {}", lo[2]);
}
