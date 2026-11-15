//! Golden summaries per tile: stage
//! hashes, triangle count, volume, the repair counts, overhang and band mm²,
//! see-through holes and the gate's verdict,
//! as insta snapshots; `cargo insta review` shows what changed.
//!
//! Each golden tile is built once for the whole binary (see `memo`: two
//! threads, every observer on), and the tests share the build.
//! Determinism: the hash snapshots were recorded from plain builds on the
//! whole machine, so the observed build matching them is the proof that
//! threads, progress, sketching and the debug layer change nothing; the
//! hash snapshot is shared by macOS and Linux; Windows checks the metrics
//! snapshot and the render gate only.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use crate::memo;
use dt_eval::{Built, Library, Options};
use dt_print::{VIEWS, render};

use super::memo::{built, hashes, prebuild};

/// The torture set: the warm check's tiles, each built once, held to its stage hashes, repair counts and the
/// gate's verdict. Chosen (14 Nov 2026) by measured line coverage of the engine (dt-geom, dt-kit, dt-elements,
/// dt-eval, dt-doc, dt-value, dt-lang): greedily, each the tile that ran the most lines the ones before it did not
/// per second of building, to 99.5% of what every test tile together runs, all of them 1×1 (two made for it:
/// `1x1-grove-small` for the grove's scattered trees under a cloud, `1x1-reeds` for the shore's reeds); then the
/// odd shapes that line counts cannot see (a 3×1 zigzag, a 1×2 diagonal, a corner sliver), and the small tiles
/// that alone carry a feature a test reads (plain soil with clods, no base, a body in a definition, the clouds the debug layer draws). Every other test tile
/// that holds goldens is in [`THOROUGH`], run by `cargo xtask check --full`.
pub const TILES: &[&str] = &[
    "lib:tests/ground/1x1-flat.dtile",
    "lib:tests/water/1x1-bands-pond.dtile",
    "lib:tests/cliffs/1x1-cliff-needles.dtile",
    "lib:tests/walls/1x1-wall-cut-stone-corner.dtile",
    "lib:tests/ground/1x1-bands-soil.dtile",
    "lib:tests/plants/1x1-grove-small.dtile",
    "lib:tests/floors/1x1-floor-fieldstone.dtile",
    "lib:tests/ground/1x1-cloud-pinned.dtile",
    "lib:tests/rocks/1x1-rock-tops.dtile",
    "lib:tests/cliffs/1x1-cliff-plateau.dtile",
    "lib:tests/water/1x1-reeds.dtile",
    "lib:tests/plants/1x1-grass-flowers.dtile",
    "lib:tests/floors/1x1-planks.dtile",
    "lib:tests/walls/1x1-wall-fieldstone.dtile",
    "lib:tests/plants/1x1-tree.dtile",
    "lib:tests/walls/1x1-wall-brick.dtile",
    "lib:tests/walls/1x1-wall-regular-brick.dtile",
    "lib:tests/cliffs/1x1-cliff-escarpment.dtile",
    "lib:tests/ground/1x1-bands-path.dtile",
    "lib:tests/floors/1x1-floor-stone.dtile",
    "lib:tests/water/1x1-water-u+soil.dtile",
    "lib:tests/shapes/1x1-island.dtile",
    "lib:tests/walls/1x1-wall-rim.dtile",
    "lib:tests/shapes/3x1-zigzag.dtile",
    "lib:tests/shapes/1x2-diagonal.dtile",
    "lib:tests/shapes/1x1-corner-sliver.dtile",
    "lib:tests/ground/1x1-no-base.dtile",
    "lib:tests/ground/1x1-define-body.dtile",
    "lib:tests/ground/1x1-soil.dtile",
    "lib:tests/ground/1x1-cloud-dirt.dtile",
    "lib:tests/water/1x1-cloud-water.dtile",
    "lib:tests/rocks/2x2-rock-clouds.dtile",
];

/// The tiles that held goldens before the torture set and are not in it: their snapshots are kept and checked by
/// the thorough tests.
const THOROUGH: &[&str] = &[
    "lib:tests/cliffs/2x1-cliff.dtile",
    "lib:tests/floors/1x1-flagstones-heaved.dtile",
    "lib:tests/floors/1x1-floor-stone-wall.dtile",
    "lib:tests/ground/2x2-dirt.dtile",
    "lib:tests/ground/2x2-hill.dtile",
    "lib:tests/plants/2x2-grass.dtile",
    "lib:tests/plants/2x2-meadow.dtile",
    "lib:tests/rocks/2x2-rocks.dtile",
    "lib:tests/water/2x2-shore.dtile",
    "lib:tests/water/2x2-water.dtile",
];

/// The shape tiles are the edge cases: each must pass the gate, watertight
/// with nothing lost.
#[test]
fn the_shape_tiles_pass_the_gate() {
    let printer = Options::default().printer;
    for path in TILES.iter().filter(|p| p.contains("/shapes/")) {
        let a = &built(path).audit;
        assert!(
            a.failures(&printer).is_empty(),
            "{path}: {:?}",
            a.failures(&printer)
        );
        assert!(a.watertight, "{path}");
        assert!(a.dropped.is_empty(), "{path}: {:?}", a.dropped);
    }
}

/// Metrics that every platform must agree on: recorded as a JSON snapshot, and read back as this struct.
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Metrics {
    /// The gate's verdict on DungeonBlocks and the numbers it judged, as the program reports them (the build
    /// record's audit, `dt_kit::AuditSummary`), every measure to 3 decimals: with the repair counts in it, what
    /// the library sweep holds every library tile to, held here for the test tiles on every run.
    audit: dt_kit::AuditSummary,
    render_red_pixels: usize,
}

/// To 3 decimals.
fn thousandths(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// `a`, every measure to 3 decimals: what every platform agrees on.
fn rounded(mut a: dt_kit::AuditSummary) -> dt_kit::AuditSummary {
    for x in [
        &mut a.volume_mm3,
        &mut a.overhang_mm2,
        &mut a.overhang_budget_mm2,
        &mut a.band_mm2,
        &mut a.band_budget_mm2,
    ] {
        *x = thousandths(*x);
    }
    for d in &mut a.dropped {
        d.volume_mm3 = thousandths(d.volume_mm3);
        for x in d.bounds.as_flattened_mut() {
            *x = thousandths(*x);
        }
    }
    a
}

fn metrics(b: &Built) -> Metrics {
    let img = render(
        &b.export.mesh,
        &b.colours(),
        &VIEWS[0],
        b.footprint,
        (600, 475),
    );
    Metrics {
        audit: rounded(dt_eval::TileRecord::of(b, &Options::default().printer, 0.0).audit),
        render_red_pixels: img.red_pixels(),
    }
}

/// The tile's metrics against its snapshot; at another seed ([`dt_testkit::test_seed`]), whether it passes the
/// gate only, against the recorded struct.
fn metrics_hold(name: &str, b: &Built) {
    if dt_testkit::at_recorded_seeds() {
        insta::assert_json_snapshot!(format!("{name}-metrics"), metrics(b));
        return;
    }
    let file = std::path::PathBuf::from(format!(
        "{}/tests/tiles/snapshots/tiles__golden__{name}-metrics.snap",
        env!("CARGO_MANIFEST_DIR")
    ));
    let snap =
        insta::Snapshot::from_file(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let text = snap
        .contents()
        .as_text()
        .unwrap_or_else(|| panic!("{}: not a text snapshot", file.display()))
        .to_string();
    let recorded: Metrics =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    assert_eq!(
        recorded.audit.passes(),
        metrics(b).audit.passes(),
        "{name}: whether the gate passes at seed {:?}, against the tile's own",
        dt_testkit::test_seed()
    );
}

#[test]
fn golden_metrics() {
    prebuild(TILES);
    for path in TILES {
        let b = built(path);
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".dtile");
        metrics_hold(name, b);
    }
}

#[test]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_golden_metrics() {
    prebuild(THOROUGH);
    for path in THOROUGH {
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".dtile");
        metrics_hold(name, built(path));
    }
}

#[test]
#[cfg(not(windows))]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_golden_hashes() {
    if !dt_testkit::at_recorded_seeds() {
        return; // recorded at the tiles' own seeds: see dt_testkit::test_seed
    }
    prebuild(THOROUGH);
    for path in THOROUGH {
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".dtile");
        insta::assert_snapshot!(format!("{name}-hashes"), hashes(built(path)));
    }
}

#[test]
#[cfg(not(windows))]
fn golden_hashes() {
    if !dt_testkit::at_recorded_seeds() {
        return; // recorded at the tiles' own seeds: see dt_testkit::test_seed
    }
    prebuild(TILES);
    for path in TILES {
        let b = built(path);
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".dtile");
        insta::assert_snapshot!(format!("{name}-hashes"), hashes(b));
    }
}

/// The other tiles the test binary builds and reads, which no golden
/// snapshot holds the metrics of: only their stage hashes are kept, so that
/// their observed builds are held to a plain one the same way.
pub const OBSERVED: &[&str] = &[
    "lib:tests/walls/1x1-wall-cut-stone.dtile",
    "lib:tests/cliffs/1x1-cliff-mound.dtile",
    "lib:tests/ground/1x1-cloud-dirt.dtile",
    "lib:tests/ground/1x1-cloud-pinned.dtile",
    "lib:tests/water/1x1-cloud-water.dtile",
    "lib:tests/rocks/2x2-rock-clouds.dtile",
    "lib:tests/plants/2x2-grove.dtile",
    "lib:tests/water/1x1-water+grass.dtile",
    "lib:tests/walls/1x1-wall-stone+grass.dtile",
    "lib:tests/walls/1x1-wall-brick-corner+grass.dtile",
];

/// Threads, progress, sketching and the debug layer change no hash: the
/// observed builds of the tiles outside the golden list against hashes
/// recorded from plain builds (`golden_hashes` does it for the rest).
#[test]
#[cfg(not(windows))]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_observers_change_nothing() {
    if !dt_testkit::at_recorded_seeds() {
        return; // recorded at the tiles' own seeds: see dt_testkit::test_seed
    }
    prebuild(OBSERVED);
    for path in OBSERVED {
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".dtile");
        insta::assert_snapshot!(format!("{name}-hashes"), hashes(built(path)));
    }
}

#[test]
fn the_flat_tile_meets_the_exit_criteria() {
    let b = built("lib:tests/ground/1x1-flat.dtile");
    let a = &b.audit;
    assert!(a.watertight, "watertight after the float32 reload");
    assert_eq!(a.overhang_mm2, 0.0);
    assert!(a.dropped.is_empty(), "{:?}", a.dropped);
    assert_eq!(a.holes, 0);
    assert!(a.failures(&Options::default().printer).is_empty());
    // the DungeonBlocks envelope: 35 × 35 mm, dirt flat at the 3.4 mm edge height
    // with its grain on top (clods and pebbles), short peg
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
    assert!(hi[2] >= 3.4 && hi[2] < 5.5, "top {}", hi[2]);
    assert!(close(lo[2], -(5.7 + 5.2) - 0.0), "peg tip {}", lo[2]);
}

/// No `base` line, no base: no connector part, no stack entry for it, the
/// tile printed flat from zero up and the audit passing, on both systems;
/// with a base line the same tile has the connector under zero.
#[test]
fn no_base_line_means_no_base() {
    let min_z = |b: &Built| {
        b.export
            .mesh
            .positions
            .iter()
            .fold(f64::INFINITY, |lo, p| lo.min(p[2]))
    };
    for system in ["db", "ol"] {
        let opts = Options {
            system: system.to_owned(),
            ..Options::default()
        };
        let none = memo::build(&Library, "lib:tests/ground/1x1-no-base.dtile", &opts).unwrap();
        assert!(none.audit.failures(&opts.printer).is_empty(), "{system}");
        assert!(none.audit.watertight && none.audit.holes == 0);
        assert!(min_z(none).abs() < 1e-6, "{system}: flat from zero");
        assert!(
            none.materials
                .iter()
                .all(|m| *m != dt_kit::Material::Connector),
            "{system}: a connector part"
        );
        let some = memo::build(&Library, "lib:tests/ground/1x1-flat.dtile", &opts).unwrap();
        assert!(min_z(some) < -6.9, "{system}: the base is under zero");
        assert!(some.audit.failures(&opts.printer).is_empty(), "{system}");
    }
}

/// The edge contract: the edge height, 3.4 mm, is the most a dirt rim
/// reaches. The grain runs on to the rim and over the edge-round (0.2 mm,
/// cut from the sharp corner at the edge height), set into the surface, so
/// the rim is that height less the round at its high points and the grain's
/// depth below it elsewhere, and never above. Neighbouring tiles touch at
/// their high points.
#[test]
fn the_edge_height_is_the_most_the_rim_reaches() {
    the_edge_height_is_the_most_the_rim_reaches_on(&[
        "lib:tests/ground/1x1-flat.dtile",
        "lib:tests/ground/1x1-soil.dtile",
    ]);
}

#[test]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_the_edge_height_is_the_most_the_rim_reaches() {
    the_edge_height_is_the_most_the_rim_reaches_on(&["lib:tests/ground/2x2-dirt.dtile"]);
}

fn the_edge_height_is_the_most_the_rim_reaches_on(paths: &[&str]) {
    const EDGE: f64 = 3.4;
    const ROUND: f64 = 0.2;
    for &path in paths {
        let b = built(path);
        let (surface, _) = b.report.surface.as_ref().unwrap();
        let (nx, ny) = (surface.nx, surface.ny);
        let mut rim: Vec<f64> = Vec::new();
        for j in 0..ny {
            for i in 0..nx {
                if i == 0 || j == 0 || i == nx - 1 || j == ny - 1 {
                    rim.push(f64::from(surface.z[j * nx + i]));
                }
            }
        }
        assert!(rim.len() > 100);
        let top = rim.iter().copied().fold(f64::MIN, f64::max);
        // nothing above the edge height, less the round, to float32
        assert!(top <= EDGE - ROUND + 1e-5, "{path}: the rim tops at {top}");
        // and the rim does reach up near it: the grain is set into the
        // surface, so its high points meet the ceiling
        let near = rim.iter().filter(|&&z| z > EDGE - ROUND - 1.5).count();
        println!(
            "{path}: rim top {top:.3}, within 1.5 mm {}/{}",
            near,
            rim.len()
        );
        assert!(
            top > EDGE - ROUND - 0.5,
            "{path}: the rim only reaches {top}"
        );
        // most of the rim is within the grain's depth (1.5 mm) of the
        // ceiling, not far below it
        assert!(
            near * 100 >= rim.len() * 80,
            "{path}: only {near} of {} rim nodes are within the grain's depth",
            rim.len()
        );
        // and in the mesh nothing on the rim stands above the ceiling
        let [w, h] = b.footprint;
        let mut mesh_top = f64::MIN;
        for p in &b.export.mesh.positions {
            let on_rim = p[0].abs() < 1e-6
                || (p[0] - w).abs() < 1e-6
                || p[1].abs() < 1e-6
                || (p[1] - h).abs() < 1e-6;
            if on_rim && p[2] > 0.01 {
                mesh_top = mesh_top.max(p[2]);
            }
        }
        assert!(
            mesh_top <= EDGE + 1e-5,
            "{path}: the edge tops at {mesh_top} mm"
        );
    }
}

#[test]
fn the_dirt_sides_are_relief_behind_the_plane() {
    the_dirt_sides_are_relief_behind_the_plane_on(&["lib:tests/ground/1x1-soil.dtile"]);
}

#[test]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_the_dirt_sides_are_relief_behind_the_plane() {
    the_dirt_sides_are_relief_behind_the_plane_on(&[
        "lib:tests/ground/2x2-dirt.dtile",
        "lib:tests/ground/2x2-hill.dtile",
    ]);
}

fn the_dirt_sides_are_relief_behind_the_plane_on(paths: &[&str]) {
    for &path in paths {
        let b = built(path);
        let [w, h] = b.footprint;
        let m = &b.export.mesh;
        // nothing of the tile (the base's own parts sit under z = 0) is past
        // a plane
        for p in &m.positions {
            if p[2] >= 0.0 {
                assert!(
                    p[0] >= -1e-6 && p[0] <= w + 1e-6 && p[1] >= -1e-6 && p[1] <= h + 1e-6,
                    "{path}: {p:?} is past a plane"
                );
            }
        }
        // each face: the projected area of the side's triangles that lies
        // within 0.1 mm of the plane, away from the corners
        for side in 0..4 {
            let (mut near, mut all) = (0.0, 0.0);
            for t in &m.triangles {
                let v = t.map(|i| m.positions[i as usize]);
                if v.iter().any(|p| p[2] < 0.0) {
                    continue;
                }
                let dist = |p: &[f64; 3]| match side {
                    0 => p[1],
                    1 => w - p[0],
                    2 => h - p[1],
                    _ => p[0],
                };
                let along = |p: &[f64; 3]| if side % 2 == 0 { p[0] } else { p[1] };
                let len = if side % 2 == 0 { w } else { h };
                let c = v.iter().map(dist).sum::<f64>() / 3.0;
                let a = v.iter().map(along).sum::<f64>() / 3.0;
                if c > 1.2 || a < 0.7 || a > len - 0.7 {
                    continue;
                }
                let u = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
                let q = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
                let n = [
                    u[1] * q[2] - u[2] * q[1],
                    u[2] * q[0] - u[0] * q[2],
                    u[0] * q[1] - u[1] * q[0],
                ];
                // the part of its area facing out through the plane
                let out = match side {
                    0 => -n[1],
                    1 => n[0],
                    2 => n[1],
                    _ => -n[0],
                } / 2.0;
                if out > 0.0 {
                    all += out;
                    if c < 0.1 {
                        near += out;
                    }
                }
            }
            assert!(all > 0.0, "{path}: side {side} has no face");
            assert!(
                near / all >= 0.15,
                "{path}: only {:.0}% of side {side} is within 0.1 mm of the plane",
                100.0 * near / all
            );
        }
        // no face above the floor leans out past the overhang (55 degrees)
        let limit = -55.0_f64.to_radians().sin();
        for t in &m.triangles {
            let v = t.map(|i| m.positions[i as usize]);
            let near = v
                .iter()
                .all(|p| p[2] > 0.01 && p[0].min(w - p[0]).min(p[1]).min(h - p[1]) < 0.7);
            if !near {
                continue;
            }
            let u = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
            let q = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
            let n = [
                u[1] * q[2] - u[2] * q[1],
                u[2] * q[0] - u[0] * q[2],
                u[0] * q[1] - u[1] * q[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-9 {
                // the stones are bedded and not part of a face; a face is the
                // earth's
                assert!(
                    n[2] / len >= limit - 1e-6,
                    "{path}: {v:?} faces down past 55 degrees"
                );
            }
        }
    }
}

#[test]
fn the_dirt_tiles_meet_the_exit_criteria() {
    the_dirt_tiles_meet_the_exit_criteria_on(&["lib:tests/ground/1x1-soil.dtile"]);
}

#[test]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_the_dirt_tiles_meet_the_exit_criteria() {
    the_dirt_tiles_meet_the_exit_criteria_on(&[
        "lib:tests/ground/2x2-dirt.dtile",
        "lib:tests/ground/2x2-hill.dtile",
    ]);
}

fn the_dirt_tiles_meet_the_exit_criteria_on(paths: &[&str]) {
    for &path in paths {
        let b = built(path);
        let a = &b.audit;
        assert!(a.watertight, "{path}: watertight after the float32 reload");
        assert_eq!(a.overhang_mm2, 0.0, "{path}");
        assert!(a.dropped.is_empty(), "{path}: {:?}", a.dropped);
        assert_eq!(a.holes, 0, "{path}");
        let note = |k: &str| b.report.notes.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        let tex = note("texture_ok").unwrap();
        assert!(tex >= 0.97, "{path}: printable texture {tex}");
        assert!(note("clods").unwrap() > 0.0 && note("pebbles_small").unwrap() > 0.0);
        // the edge contract: the top of every column on the tile edge is
        // the edge level less the dirt's round (0.2 mm), and no vertex of
        // the ground on the edge is above the edge level
        let edge = b.report.edge_mm.unwrap();
        let [w, h] = b.footprint;
        let mut top = f64::MIN;
        for p in &b.export.mesh.positions {
            let on_edge = p[0].abs() < 1e-6
                || p[1].abs() < 1e-6
                || (p[0] - w).abs() < 1e-4
                || (p[1] - h).abs() < 1e-4;
            if on_edge && p[2] > 0.0 {
                top = top.max(p[2]);
            }
        }
        assert!(
            top <= edge + 1e-5 && top > edge - 0.2 - 0.6,
            "{path}: the edge tops at {top}"
        );
    }
}

#[test]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_the_rocks_and_water_tiles_meet_the_exit_criteria() {
    use dt_kit::Material;
    for path in [
        "lib:tests/rocks/2x2-rocks.dtile",
        "lib:tests/water/2x2-water.dtile",
    ] {
        let b = built(path);
        let a = &b.audit;
        assert!(a.watertight, "{path}: watertight after the float32 reload");
        assert_eq!(a.overhang_mm2, 0.0, "{path}");
        assert!(a.failures(&Options::default().printer).is_empty(), "{path}");
        // every triangle traced to a material, and rock present
        assert_eq!(b.materials.len(), b.export.mesh.triangles.len(), "{path}");
        assert!(b.materials.contains(&Material::Rock), "{path}: no rock");
        let note = |k: &str| b.report.notes.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
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

#[test]
fn the_wall_and_floor_tiles_meet_the_exit_criteria() {
    the_wall_and_floor_tiles_meet_the_exit_criteria_on(&[
        "lib:tests/walls/1x1-wall-cut-stone-corner.dtile",
        "lib:tests/walls/1x1-wall-fieldstone.dtile",
        "lib:tests/walls/1x1-wall-rim.dtile",
        "lib:tests/floors/1x1-floor-stone.dtile",
        "lib:tests/floors/1x1-planks.dtile",
    ]);
}

#[test]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_the_wall_and_floor_tiles_meet_the_exit_criteria() {
    the_wall_and_floor_tiles_meet_the_exit_criteria_on(&[
        "lib:tests/floors/1x1-flagstones-heaved.dtile",
    ]);
}

fn the_wall_and_floor_tiles_meet_the_exit_criteria_on(paths: &[&str]) {
    for &path in paths {
        let b = built(path);
        let a = &b.audit;
        assert!(a.watertight, "{path}: watertight after the float32 reload");
        assert_eq!(a.overhang_mm2, 0.0, "{path}");
        assert_eq!(a.holes, 0, "{path}");
        let note = |k: &str| {
            b.report
                .notes
                .iter()
                .find(|(n, _)| n.ends_with(k))
                .map(|(_, v)| *v)
        };
        assert!(
            note("units").unwrap_or(0.0) > 0.0,
            "{path}: {:?}",
            b.report.notes
        );
    }
}

#[test]
#[ignore = "thorough: cargo xtask check --full runs it"]
fn thorough_the_grass_tiles_meet_the_exit_criteria() {
    use dt_kit::Material;
    for path in [
        "lib:tests/plants/2x2-grass.dtile",
        "lib:tests/plants/2x2-meadow.dtile",
        "lib:tests/water/2x2-shore.dtile",
    ] {
        let b = built(path);
        let a = &b.audit;
        assert!(a.watertight, "{path}: watertight after the float32 reload");
        assert_eq!(a.overhang_mm2, 0.0, "{path}");
        assert!(a.dropped.is_empty(), "{path}: {:?}", a.dropped);
        assert_eq!(a.holes, 0, "{path}");
        assert!(b.materials.contains(&Material::Grass), "{path}: no grass");
        let note = |k: &str| b.report.notes.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        let blades = note("blades").unwrap_or(0.0);
        let least = if path.contains("shore") {
            500.0
        } else {
            1500.0
        };
        assert!(blades > least, "{path}: {blades} blades");
        // a blade is a few hundred faces, never past 1,500
        let grass_tris = b
            .materials
            .iter()
            .filter(|m| **m == Material::Grass)
            .count();
        assert!(
            (grass_tris as f64) / blades < 1500.0,
            "{path}: {} faces a blade",
            (grass_tris as f64) / blades
        );
        if path.contains("meadow") {
            // bare earth, ground under the rocks included: 20% ± 3 points
            let bare = note("bare_share").unwrap_or(0.0);
            assert!((0.17..=0.23).contains(&bare), "{path}: bare earth {bare}");
        }
        if path.contains("shore") {
            assert!(
                note("reed_stems").unwrap_or(0.0) >= 5.0,
                "{path}: {:?}",
                b.report.notes
            );
        }
    }
}
