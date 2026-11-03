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
use dt_kit::{VIEWS, render};

const TILES: &[&str] = &[
    "lib:tiles/test/flat-1x1.kdl",
    "lib:tiles/ground/soil-1x1.kdl",
    "lib:tiles/test/dirt-2x2.kdl",
    "lib:tiles/test/rocks-2x2.kdl",
    "lib:tiles/test/water-2x2.kdl",
    "lib:tiles/test/dirt-stones-2x2.kdl",
];

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
        &b.colours(),
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
    // the DungeonBlocks envelope: 35 × 35 mm, ground at the 3.4 mm edge level, short peg
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
    assert!(close(hi[2], 3.4), "top {}", hi[2]);
    assert!(close(lo[2], -(5.7 + 5.2) - 0.0), "peg tip {}", lo[2]);
}

#[test]
fn the_dirt_tiles_meet_the_exit_criteria() {
    for path in [
        "lib:tiles/ground/soil-1x1.kdl",
        "lib:tiles/test/dirt-2x2.kdl",
    ] {
        let b = build_at(path, 4);
        let a = &b.audit;
        assert!(a.watertight, "{path}: watertight after the float32 reload");
        assert_eq!(a.overhang_mm2, 0.0, "{path}");
        assert!(a.dropped.is_empty(), "{path}: {:?}", a.dropped);
        assert_eq!(a.holes, 0, "{path}");
        let note = |k: &str| b.notes.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        let tex = note("texture_ok").unwrap();
        assert!(tex >= 0.97, "{path}: printable texture {tex}");
        assert!(note("clods").unwrap() > 0.0 && note("pebbles_small").unwrap() > 0.0);
        // the edge contract: every top vertex on the tile edge at the
        // edge level, and nothing of the ground below it
        let edge = b.edge_mm.unwrap();
        let [w, h] = b.footprint;
        for p in &b.export.mesh.positions {
            let on_edge = p[0].abs() < 1e-6
                || p[1].abs() < 1e-6
                || (p[0] - w).abs() < 1e-4
                || (p[1] - h).abs() < 1e-4;
            if on_edge && p[2] > 0.0 {
                assert!((p[2] - edge).abs() < 1e-5, "{path}: edge vertex {p:?}");
            }
        }
    }
}

#[test]
fn the_rocks_and_water_tiles_meet_the_exit_criteria() {
    use dt_kit::Material;
    for path in [
        "lib:tiles/test/rocks-2x2.kdl",
        "lib:tiles/test/water-2x2.kdl",
        "lib:tiles/test/dirt-stones-2x2.kdl",
    ] {
        let b = build_at(path, 4);
        let a = &b.audit;
        assert!(a.watertight, "{path}: watertight after the float32 reload");
        assert_eq!(a.overhang_mm2, 0.0, "{path}");
        assert!(a.failures(&Options::default().printer).is_empty(), "{path}");
        // R9: every triangle traced to a material, and rock present
        assert_eq!(b.materials.len(), b.export.mesh.triangles.len(), "{path}");
        assert!(b.materials.contains(&Material::Rock), "{path}: no rock");
        let note = |k: &str| b.notes.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        if path.contains("rocks") {
            assert!(note("chip_loss_max").unwrap() <= 0.03, "{path}: chip loss");
            assert!(note("facets_min").unwrap() >= 25.0, "{path}");
            assert!(note("facets_max").unwrap() <= 170.0, "{path}");
            assert!(note("width_min").unwrap() >= 4.0 - 1e-9, "{path}");
            assert!(note("width_max").unwrap() <= 15.0 + 1e-9, "{path}");
        }
        if path.contains("water") {
            assert!(b.materials.contains(&Material::Water), "{path}: no water");
            assert!(note("edge_err_mm").unwrap() <= 0.05, "{path}: edge");
            let lo = note("relief_min").unwrap();
            assert!(
                (-1.1 - 1e-9..-0.8).contains(&lo),
                "{path}: trough floor {lo}"
            );
            let rms = note("relief_rms").unwrap();
            assert!((0.55..0.85).contains(&rms), "{path}: rms {rms}");
        }
    }
}
