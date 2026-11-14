//! Golden summaries per tile: stage
//! hashes, triangle count, volume, overhang mm² and see-through holes,
//! as insta snapshots; `cargo insta review` shows what changed.
//!
//! Each golden tile is built once on the whole machine, all of them at
//! once, and the tests share the builds. Determinism: every tile is built
//! again on two threads and every stage hash must agree; the hash
//! snapshot is shared by macOS and Linux; Windows checks the metrics
//! snapshot and the render gate only.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]

use std::collections::HashMap;
use std::sync::LazyLock;

use dt_eval::{Built, Library, Options, build};
use dt_print::{VIEWS, render};
use rayon::prelude::*;

const TILES: &[&str] = &[
    "lib:tests/ground/1x1-flat.dtile",
    "lib:tests/ground/1x1-no-base.dtile",
    "lib:tests/ground/1x1-soil.dtile",
    "lib:tests/ground/2x2-dirt.dtile",
    "lib:tests/rocks/2x2-rocks.dtile",
    "lib:tests/water/2x2-water.dtile",
    "lib:tests/plants/2x2-grass.dtile",
    "lib:tests/plants/2x2-meadow.dtile",
    "lib:tests/water/2x2-shore.dtile",
    "lib:tests/plants/1x1-grass-flowers.dtile",
    "lib:tests/cliffs/2x1-cliff.dtile",
    "lib:tests/plants/1x1-tree.dtile",
    "lib:tests/walls/1x1-wall-cut-stone-corner.dtile",
    "lib:tests/walls/1x1-wall-fieldstone.dtile",
    "lib:tests/floors/1x1-floor-stone.dtile",
    "lib:tests/floors/1x1-flagstones-heaved.dtile",
    "lib:tests/floors/1x1-planks.dtile",
    "lib:tests/walls/1x1-wall-rim.dtile",
    "lib:tests/ground/2x2-hill.dtile",
    // a stone floor with a wall standing on it: what laying floors before
    // the lift must leave as it is
    "lib:tests/floors/1x1-floor-stone-wall.dtile",
    // a cove of water cut into dirt: a hollow in the land under a fluid
    "lib:tests/water/1x1-water-u+soil.dtile",
    // odd shapes no library would ship: a closed line, sharp teeth on a
    // long tile, a tile taller than wide, a region that is only a sliver
    "lib:tests/shapes/1x1-island.dtile",
    "lib:tests/shapes/3x1-zigzag.dtile",
    "lib:tests/shapes/1x2-diagonal.dtile",
    "lib:tests/shapes/1x1-corner-sliver.dtile",
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

fn build_at(path: &str, threads: usize) -> Built {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap();
    pool.install(|| build(&Library, path, &Options::default()))
        .unwrap_or_else(|e| panic!("{path}: {e:?}"))
}

/// Every golden tile, built once on the whole machine, all at once; every
/// test here reads these.
static BUILT: LazyLock<HashMap<&'static str, Built>> = LazyLock::new(|| {
    TILES
        .par_iter()
        .map(|&p| {
            (
                p,
                build(&Library, p, &Options::default()).unwrap_or_else(|e| panic!("{p}: {e:?}")),
            )
        })
        .collect()
});

fn built(path: &str) -> &'static Built {
    BUILT
        .get(path)
        .unwrap_or_else(|| panic!("{path} is not a golden tile"))
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
        "triangles: {}\nvolume_mm3: {:.3}\nwatertight: {}\nbodies: {}\ndropped: {}\nvoids_filled: {}\npinches_moved: {}\nslivers: {}\ncollapsed: {}\noverhang_mm2: {:.3}\nholes: {}\nrender_red_pixels: {}\n",
        a.triangles,
        a.volume_mm3,
        a.watertight,
        a.bodies,
        a.dropped.len(),
        a.voids.len(),
        a.pinches_moved,
        a.slivers.len(),
        a.collapsed,
        a.overhang_mm2,
        a.holes,
        img.red_pixels(),
    )
}

/// Every leaf solid takes a slot reserved in sequence, so a build is the
/// same whatever the threads: each golden tile on two threads, all at once,
/// against its build on the whole machine.
#[test]
fn stage_hashes_agree_whatever_the_threads() {
    // the shared builds first, on this thread: a pool worker must never
    // wait on them while they wait on the pool
    LazyLock::force(&BUILT);
    TILES.par_iter().for_each(|path| {
        assert_eq!(
            hashes(&build_at(path, 2)),
            hashes(built(path)),
            "{path}: 2 threads against all"
        );
    });
}

#[test]
fn golden_metrics() {
    for path in TILES {
        let b = built(path);
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".dtile");
        insta::assert_snapshot!(format!("{name}-metrics"), metrics(b));
    }
}

#[test]
#[cfg(not(windows))]
fn golden_hashes() {
    for path in TILES {
        let b = built(path);
        let name = path.rsplit('/').next().unwrap().trim_end_matches(".dtile");
        insta::assert_snapshot!(format!("{name}-hashes"), hashes(b));
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
        let none = build(&Library, "lib:tests/ground/1x1-no-base.dtile", &opts).unwrap();
        assert!(none.audit.failures(&opts.printer).is_empty(), "{system}");
        assert!(none.audit.watertight && none.audit.holes == 0);
        assert!(min_z(&none).abs() < 1e-6, "{system}: flat from zero");
        assert!(
            none.materials
                .iter()
                .all(|m| *m != dt_kit::Material::Connector),
            "{system}: a connector part"
        );
        let some = build(&Library, "lib:tests/ground/1x1-flat.dtile", &opts).unwrap();
        assert!(min_z(&some) < -6.9, "{system}: the base is under zero");
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
    const EDGE: f64 = 3.4;
    const ROUND: f64 = 0.2;
    for path in [
        "lib:tests/ground/2x2-dirt.dtile",
        "lib:tests/ground/1x1-soil.dtile",
    ] {
        let b = built(path);
        let (surface, _) = b.surface.as_ref().unwrap();
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
    for path in [
        "lib:tests/ground/1x1-soil.dtile",
        "lib:tests/ground/2x2-dirt.dtile",
        "lib:tests/ground/2x2-hill.dtile",
    ] {
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
    for path in [
        "lib:tests/ground/1x1-soil.dtile",
        "lib:tests/ground/2x2-dirt.dtile",
        "lib:tests/ground/2x2-hill.dtile",
    ] {
        let b = built(path);
        let a = &b.audit;
        assert!(a.watertight, "{path}: watertight after the float32 reload");
        assert_eq!(a.overhang_mm2, 0.0, "{path}");
        assert!(a.dropped.is_empty(), "{path}: {:?}", a.dropped);
        assert_eq!(a.holes, 0, "{path}");
        let note = |k: &str| b.notes.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
        let tex = note("texture_ok").unwrap();
        assert!(tex >= 0.97, "{path}: printable texture {tex}");
        assert!(note("clods").unwrap() > 0.0 && note("pebbles_small").unwrap() > 0.0);
        // the edge contract: the top of every column on the tile edge is
        // the edge level less the dirt's round (0.2 mm), and no vertex of
        // the ground on the edge is above the edge level
        let edge = b.edge_mm.unwrap();
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
fn the_rocks_and_water_tiles_meet_the_exit_criteria() {
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

#[test]
fn the_wall_and_floor_tiles_meet_the_exit_criteria() {
    for path in [
        "lib:tests/walls/1x1-wall-cut-stone-corner.dtile",
        "lib:tests/walls/1x1-wall-fieldstone.dtile",
        "lib:tests/walls/1x1-wall-rim.dtile",
        "lib:tests/floors/1x1-floor-stone.dtile",
        "lib:tests/floors/1x1-flagstones-heaved.dtile",
        "lib:tests/floors/1x1-planks.dtile",
    ] {
        let b = built(path);
        let a = &b.audit;
        assert!(a.watertight, "{path}: watertight after the float32 reload");
        assert_eq!(a.overhang_mm2, 0.0, "{path}");
        assert_eq!(a.holes, 0, "{path}");
        let note = |k: &str| {
            b.notes
                .iter()
                .find(|(n, _)| n.ends_with(k))
                .map(|(_, v)| *v)
        };
        assert!(note("units").unwrap_or(0.0) > 0.0, "{path}: {:?}", b.notes);
    }
}

#[test]
fn the_grass_tiles_meet_the_exit_criteria() {
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
        let note = |k: &str| b.notes.iter().find(|(n, _)| n == k).map(|(_, v)| *v);
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
                b.notes
            );
        }
    }
}
