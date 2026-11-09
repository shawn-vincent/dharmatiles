//! The cliff as a pieces body: every stone top inside its band, dirt on its
//! tops lying like dirt (on the stone, off the steep faces, as far as
//! `maxSlope` lets it), the tile's outer sides carrying grain, and every old
//! cliff form migrating to a tile that checks clean.

#![allow(clippy::unwrap_used, clippy::cast_possible_truncation)]

use dt_eval::{Built, Library, Memory, Options, build, check};
use dt_kit::Material;

fn mem(text: &str) -> Memory {
    Memory(std::iter::once(("t.dtile".to_owned(), text.to_owned())).collect())
}

/// The library's dirt edge height, mm: the ground round a pile.
const EDGE_MM: f64 = 3.4;
/// The tallest pebble bedded in that ground, mm (`pebble size` tops at 4.5).
const PEBBLE_MAX_MM: f64 = 4.5;
/// The dirt's band over the stone's top, mm (the tile writes `bottom + [0.6 .. 1.2]`).
const DIRT_MIN_MM: f64 = 0.6;
const DIRT_MAX_MM: f64 = 1.2;
/// How far the dirt's grain may stand over its band, mm (clods stand 0.75).
const GRAIN_MM: f64 = 0.8;
/// A vertex this near the tile's plane is on it, mm.
const FLAT_MM: f64 = 0.1;

/// A free-standing pile on the library's dirt, with dirt on it or without.
fn mound(film: bool) -> String {
    format!(
        r#"tile size="1x1" seed=1 {{
    region crag {{ within 13.5 of=center; within 9 of="(0.42, 0.55)"; within 8 of="(0.6, 0.45)" }}
    region lump2 {{ within 9 of="(0.47, 0.53)"; within 6.5 of="(0.58, 0.44)" }}
    region lump3 {{ within 6.5 of="(0.5, 0.5)"; within 4.5 of="(0.4, 0.57)" }}
    region lump4 {{ within 3.8 of="(0.52, 0.47)" }}
    stack {{
        base
        dirt top=dirtEdgeHeight {{
            pin top=dirtEdgeHeight at=edges
        }}
        cliff top="dirtEdgeHeight + [3 .. 9]" at=crag {{
            pin top="dirtEdgeHeight + [10 .. 20]" at=lump2
            pin top="dirtEdgeHeight + [19 .. 28]" at=lump3
            pin top="dirtEdgeHeight + [29 .. 34]" at=lump4
            rock mixed
            stoneSize broken
        }}
{}    }}
}}
"#,
        if film {
            "        dirt top=\"bottom + [0.6 .. 1.2]\" at=crag\n"
        } else {
            ""
        }
    )
}

fn built(text: &str) -> Built {
    build(&mem(text), "t.dtile", &Options::default()).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// The highest vertex of the written mesh of `material` inside `disc`
/// (centre, radius, mm), or the whole tile.
fn top_of(b: &Built, material: Material, disc: Option<([f64; 2], f64)>) -> f64 {
    let mut top = f64::MIN;
    for (t, m) in b.export.mesh.triangles.iter().zip(&b.materials) {
        if *m != material {
            continue;
        }
        for &i in t {
            let p = b.export.mesh.positions[i as usize];
            if disc.is_none_or(|(c, r)| (p[0] - c[0]).hypot(p[1] - c[1]) <= r) {
                top = top.max(p[2]);
            }
        }
    }
    top
}

#[test]
fn every_stone_top_stands_in_its_band_and_the_pile_reaches_it() {
    let b = built(&mound(false));
    assert!(b.audit.failures(&Options::default().printer).is_empty());
    let edge = EDGE_MM;
    // nothing passes the band's highest high (the build refuses a stone
    // over its own band: a bug in the body)
    assert!(
        top_of(&b, Material::Rock, None) <= edge + 34.0 + 0.01,
        "{}",
        top_of(&b, Material::Rock, None)
    );
    // and the stones reach each pin's band: the summit's, the middle's, the foot's
    for (centre, r, lo) in [
        ([0.52 * 35.0, 0.47 * 35.0], 3.8, 29.0),
        ([0.5 * 35.0, 0.5 * 35.0], 6.5, 19.0),
        ([0.47 * 35.0, 0.53 * 35.0], 9.0, 10.0),
    ] {
        let top = top_of(&b, Material::Rock, Some((centre, r)));
        assert!(
            top >= edge + lo - 0.3,
            "disc at {centre:?}: {top} under {lo}"
        );
    }
}

/// A flat-topped block of stone with dirt on its top, or bare, the dirt
/// written with `extra` after its region.
fn table(film: bool, seed: u32, extra: &str) -> String {
    format!(
        r#"tile size="1x1" seed={seed} {{
    region crag {{ within 13 of=center }}
    stack {{
        base
        dirt top=dirtEdgeHeight {{
            pin top=dirtEdgeHeight at=edges
        }}
        cliff top="dirtEdgeHeight + 14" at=crag {{
            rock blocks columns=0 slabs=0 odd=0 monoliths=0
            stoneSize massive
        }}
{}    }}
}}
"#,
        if film {
            format!("        dirt top=\"bottom + [0.6 .. 1.2]\" at=crag {extra}\n")
        } else {
            String::new()
        }
    )
}

/// The up-facing triangles of `b` of `material`, as three vertices each.
fn up_triangles(b: &Built, material: Material) -> Vec<[[f64; 3]; 3]> {
    let mut out = Vec::new();
    for (t, m) in b.export.mesh.triangles.iter().zip(&b.materials) {
        if *m != material {
            continue;
        }
        let ps = t.map(|i| b.export.mesh.positions[i as usize].map(f64::from));
        let (u, v) = (
            [0, 1, 2].map(|k| ps[1][k] - ps[0][k]),
            [0, 1, 2].map(|k| ps[2][k] - ps[0][k]),
        );
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        if n[2] > 0.9 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() {
            out.push(ps);
        }
    }
    out
}

/// The film's thickness over the stone under it, exactly: at every vertex of a
/// flat-facing earth triangle higher than the ground round the pile, its
/// height less the height of the bare build's stone straight under it (found
/// by the triangle the point falls in), sorted.
fn film_thickness(bare: &Built, filmed: &Built) -> Vec<f64> {
    let mut grid: std::collections::HashMap<(i32, i32), Vec<[[f64; 3]; 3]>> = Default::default();
    for ps in up_triangles(bare, Material::Rock) {
        let span = |k: usize| {
            let (lo, hi) = ps
                .iter()
                .fold((f64::MAX, f64::MIN), |(l, h), p| (l.min(p[k]), h.max(p[k])));
            (lo.floor() as i32)..=(hi.floor() as i32)
        };
        for i in span(0) {
            for j in span(1) {
                grid.entry((i, j)).or_default().push(ps);
            }
        }
    }
    let stone_at = |x: f64, y: f64| -> Option<f64> {
        let mut best: Option<f64> = None;
        for ps in grid.get(&(x.floor() as i32, y.floor() as i32))? {
            let d = (ps[1][1] - ps[2][1]) * (ps[0][0] - ps[2][0])
                + (ps[2][0] - ps[1][0]) * (ps[0][1] - ps[2][1]);
            let a = ((ps[1][1] - ps[2][1]) * (x - ps[2][0])
                + (ps[2][0] - ps[1][0]) * (y - ps[2][1]))
                / d;
            let b = ((ps[2][1] - ps[0][1]) * (x - ps[2][0])
                + (ps[0][0] - ps[2][0]) * (y - ps[2][1]))
                / d;
            let c = 1.0 - a - b;
            if a >= -1e-9 && b >= -1e-9 && c >= -1e-9 {
                let z = a * ps[0][2] + b * ps[1][2] + c * ps[2][2];
                best = Some(best.map_or(z, |v| v.max(z)));
            }
        }
        best
    };
    let mut thick: Vec<f64> = up_triangles(filmed, Material::Earth)
        .iter()
        .flatten()
        .filter(|p| p[2] >= EDGE_MM + PEBBLE_MAX_MM)
        .filter_map(|p| stone_at(p[0], p[1]).map(|z| p[2] - z))
        .collect();
    thick.sort_by(f64::total_cmp);
    thick
}

/// The mound with its foot stones standing clear of the ground's pebbles.
fn tall(film: bool) -> String {
    mound(film).replace("[3 .. 9]", "[8 .. 12]")
}

/// The up-facing earth vertices higher than the ground round the pile.
fn dirt_vertices(b: &Built) -> Vec<[f64; 3]> {
    up_triangles(b, Material::Earth)
        .into_iter()
        .flatten()
        .filter(|p| p[2] >= EDGE_MM + PEBBLE_MAX_MM)
        .collect()
}

#[test]
fn dirt_follows_a_stones_top_as_a_layer_of_its_band() {
    // the dirt is its band over the stone under it; the grain's pits may take
    // it under the band's low, never under the stone's top by more than a
    // pit's depth, and its grain never stands more than a clod over the high
    for seed in [2, 5, 9] {
        let name = format!("table seed {seed}");
        let (bare, dirt) = (
            built(&table(false, seed, "")),
            built(&table(true, seed, "")),
        );
        let t = film_thickness(&bare, &dirt);
        assert!(
            t.len() > 500,
            "{name}: the dirt is laid: {} triangles",
            t.len()
        );
        assert!(
            t[0] >= DIRT_MIN_MM - GRAIN_MM,
            "{name}: the dirt is {} mm over the stone at its thinnest",
            t[0]
        );
        assert!(
            t[t.len() - 1] <= DIRT_MAX_MM + GRAIN_MM,
            "{name}: the dirt is {} mm over the stone at its thickest",
            t[t.len() - 1]
        );
        // it lies on the stone, not a different height from it: the middle
        // of it is within its band
        let median = t[t.len() / 2];
        assert!(
            median > 0.5 * DIRT_MIN_MM && median < DIRT_MAX_MM + 0.5 * GRAIN_MM,
            "{name}: median dirt {median} mm over the stone"
        );
    }
}

#[test]
fn dirt_is_absent_where_the_surface_under_it_is_steeper_than_max_slope() {
    // a pile of broken stones: their grain and tilts are steeper than 35
    // degrees at the scale dirt reads the slope, so there is no dirt on it
    // at the default and it stays bare; the same dirt lies on the flat tops
    // of the table (`dirt_follows_a_stones_top`), and the unit tests in
    // `dirt.rs` hold the angle itself
    let tile = |extra: &str| tall(true).replace("at=crag\n", &format!("at=crag {extra}\n"));
    assert_eq!(
        dirt_vertices(&built(&tile(""))).len(),
        0,
        "dirt on broken stone at 35 degrees"
    );
    assert!(
        dirt_vertices(&built(&tile("maxSlope=60"))).len() > 100,
        "a steeper limit lets it on"
    );
}

#[test]
fn max_slope_changes_how_much_stone_holds_dirt() {
    let tile = |extra: &str| tall(true).replace("at=crag\n", &format!("at=crag {extra}\n"));
    let cover = |extra: &str| {
        let b = built(&tile(extra));
        dirt_vertices(&b).len()
    };
    let (low, default, high) = (
        cover("maxSlope=45"),
        cover("maxSlope=60"),
        cover("maxSlope=85"),
    );
    assert!(
        low < default && default < high,
        "dirt over steeper stone: {low} < {default} < {high}"
    );
}

/// The share of the side's vertices (above the base, under `z_max`) within
/// 0.1 mm of the plane, and the number of 3 mm windows of the side that are
/// flat: every vertex in the window on the plane.
fn side_grain(b: &Built, axis: usize, high: bool, z_max: f64) -> (f64, usize, usize, f64) {
    let [w, h] = b.footprint;
    let size = [w, h];
    let (mut near, mut on) = (0_u32, 0_u32);
    let mut depths: Vec<f64> = Vec::new();
    let mut windows: std::collections::HashMap<(i32, i32), (bool, u32)> = Default::default();
    for p in &b.export.mesh.positions {
        if p[2] < 0.5 || p[2] > z_max {
            continue;
        }
        let d = if high { size[axis] - p[axis] } else { p[axis] };
        if !(0.0..1.0).contains(&d) {
            continue;
        }
        near += 1;
        depths.push(d);
        let flat = d < FLAT_MM;
        if flat {
            on += 1;
        }
        let along = p[1 - axis];
        let e = windows
            .entry(((along / 3.0) as i32, (p[2] / 3.0) as i32))
            .or_insert((true, 0));
        e.0 &= flat;
        e.1 += 1;
    }
    let flat_windows = windows.values().filter(|(f, n)| *f && *n > 20).count();
    let mean = depths.iter().sum::<f64>() / depths.len().max(1) as f64;
    let var = depths.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / depths.len().max(1) as f64;
    (
        f64::from(on) / f64::from(near.max(1)),
        flat_windows,
        windows.len(),
        var.sqrt(),
    )
}

#[test]
fn the_tiles_outer_sides_carry_grain_all_the_way_up() {
    for path in [
        "lib:tiles/cliffs/1x1-cliff.dtile",
        "lib:tiles/cliffs/1x1-cliff-corner.dtile",
    ] {
        let b = build(&Library, path, &Options::default()).unwrap();
        // the crag stands on the north (and west) sides; every side, the
        // foot of the stones included, is grained like the top
        for (axis, high) in [(0, false), (0, true), (1, false), (1, true)] {
            let (share, flat, total, spread) = side_grain(&b, axis, high, 9.0);
            // a face on the plane has every vertex within FLAT_MM of it, so
            // its depths spread by at most FLAT_MM / 2; grain spreads them by
            // more than twice that (0.2 to 0.3 mm in the library's cliffs)
            assert!(
                spread > FLAT_MM,
                "{path} side {axis}{high}: depths spread {spread} mm, a flat face's is at most {}",
                FLAT_MM / 2.0
            );
            assert!(
                share <= 0.6,
                "{path} side {axis}{high}: a flat face, {share}"
            );
            assert_eq!(
                flat, 0,
                "{path} side {axis}{high}: {flat} of {total} windows are flat"
            );
        }
    }
}

#[test]
fn every_old_cliff_form_migrates_to_a_tile_that_checks_clean() {
    for (form, size) in [
        ("run", "2x1"),
        ("plateau", "2x2"),
        ("massif", "2x2"),
        ("mound", "1x1"),
        ("spire", "1x1"),
        ("needles", "2x1"),
        ("escarpment", "2x1"),
        ("canyon", "2x2"),
        ("corner", "2x2"),
    ] {
        let along = if matches!(form, "run" | "escarpment") {
            " along=north"
        } else if form == "corner" {
            " along=\"north west\""
        } else {
            ""
        };
        let old = format!(
            "tile size=\"{size}\" seed=1 {{\n    stack {{\n        base\n        dirt top=dirtEdgeHeight {{\n            bank against=cliff height=\"1.5..3.5\" reach=\"3..6.5\"\n        }}\n        cliff form={form}{along} {{\n            rock mixed\n            top dirt\n        }}\n    }}\n}}\n"
        );
        let new = dt_doc::migrate::heights(&old).unwrap().unwrap();
        assert!(!new.contains("form="), "{form}\n{new}");
        // migrating again is a no-op
        assert!(
            dt_doc::migrate::heights(&new).unwrap().is_none(),
            "{form}\n{new}"
        );
        let c = check(&mem(&new), "t.dtile", &Options::default())
            .unwrap_or_else(|e| panic!("{form}\n{new}\n{e:#?}"));
        assert!(
            c.notes().iter().all(|n| n.code != "dt::edge"),
            "{form}: {:?}\n{new}",
            c.notes()
        );
        // and it builds, on the quick setting (the gate is for full builds):
        // there are stones, and they are rock
        let draft = Options {
            draft: true,
            ..Options::default()
        };
        let b = build(&mem(&new), "t.dtile", &draft)
            .unwrap_or_else(|e| panic!("{form} does not build\n{new}\n{e:#?}"));
        assert!(note(&b, "cliff_stones") > 0.0, "{form}: no stones\n{new}");
        assert!(
            top_of(&b, Material::Rock, None) > EDGE_MM + 3.0,
            "{form}: no pile\n{new}"
        );
    }
}

#[test]
fn the_ledges_and_the_shattered_controls_still_parse() {
    let text = mound(false).replace(
        "stoneSize broken",
        "stoneSize broken\n            ledges 2\n            shattered patches=2 width=\"5..10\"",
    );
    check(&mem(&text), "t.dtile", &Options::default()).unwrap();
}

fn note(b: &Built, name: &str) -> f64 {
    b.notes
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| *v)
        .unwrap()
}

#[test]
fn a_band_that_holds_a_stone_down_does_not_stack_it_on_itself() {
    // the spire's lumps have wide bands one over the other: a stone standing
    // over two of them is held to the lower one's high, and the stack once
    // placed that same stone again and again at its site (dozens of
    // coincident solids, a union that never finished)
    let b = build(
        &Library,
        "lib:tiles/cliffs/1x1-cliff-spire.dtile",
        &Options::default(),
    )
    .unwrap();
    assert!(b.audit.failures(&Options::default().printer).is_empty());
    // the property: no stone is the same solid as another (the build drops
    // repeats as a guard and counts them)
    assert_eq!(note(&b, "cliff_repeated_stones"), 0.0);
    assert!(note(&b, "cliff_stones") > 0.0);
}
