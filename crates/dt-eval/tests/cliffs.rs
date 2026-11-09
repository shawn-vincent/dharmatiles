//! The cliff as a pieces body: every stone top inside its band, a film of
//! dirt only on stones that stand, the tile's outer sides carrying grain,
//! and every old cliff form migrating to a tile that checks clean.

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
/// The film's band over the stone's top, mm (the tile writes `bottom + [0.6 .. 1.2]`).
const FILM_MIN_MM: f64 = 0.6;
const FILM_MAX_MM: f64 = 1.2;
/// How far the rock under a film triangle may differ from the highest rock
/// in the 3 x 3 cells round it: the cells are 0.25 mm and a top's slope under
/// a film is under 35 degrees, so about 0.75 * 0.7; and the float32 round.
const CELL_SLOPE_MM: f64 = 0.6;
/// A vertex this near the tile's plane is on it, mm.
const FLAT_MM: f64 = 0.1;

/// A free-standing pile on the library's dirt, with the film or without it.
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
            stone-size broken
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

/// The highest rock under each 0.25 mm cell, from the no-film build.
fn rock_tops(b: &Built) -> std::collections::HashMap<(i32, i32), f64> {
    let cell = |v: f64| (v / 0.25).round() as i32;
    let mut tops = std::collections::HashMap::new();
    for (t, m) in b.export.mesh.triangles.iter().zip(&b.materials) {
        if *m != Material::Rock {
            continue;
        }
        for &i in t {
            let p = b.export.mesh.positions[i as usize];
            let e = tops.entry((cell(p[0]), cell(p[1]))).or_insert(f64::MIN);
            *e = e.max(p[2]);
        }
    }
    tops
}

/// A flat-topped block of stone with a film on its top, or bare.
fn table(film: bool) -> String {
    format!(
        r#"tile size="1x1" seed=2 {{
    region crag {{ within 13 of=center }}
    stack {{
        base
        dirt top=dirtEdgeHeight {{
            pin top=dirtEdgeHeight at=edges
        }}
        cliff top="dirtEdgeHeight + 14" at=crag {{
            rock blocks
            stone-size massive
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

#[test]
fn a_film_lies_only_on_stones_that_stand_and_is_a_layer_of_its_band() {
    let (bare, filmed) = (built(&table(false)), built(&table(true)));
    let rock = rock_tops(&bare);
    let cell = |v: f64| (v / 0.25).round() as i32;
    let (mut film, mut over, mut bare) = (0_usize, 0_usize, 0_usize);
    let mut gaps: Vec<f64> = Vec::new();
    for (t, m) in filmed.export.mesh.triangles.iter().zip(&filmed.materials) {
        if *m != Material::Earth {
            continue;
        }
        let ps = t.map(|i| filmed.export.mesh.positions[i as usize]);
        let c = [0, 1, 2].map(|k| (ps[0][k] + ps[1][k] + ps[2][k]) / 3.0);
        // the ground round the pile is under its edge height and a pebble:
        // higher earth is film
        if c[2] < EDGE_MM + PEBBLE_MAX_MM {
            continue;
        }
        film += 1;
        // a stone stands under it: the film is the band's thickness over
        // its top, grain pits cut into it, so it lies between a cell's slope
        // under the stone's top and the band's high over it
        let near = (-1..=1)
            .flat_map(|di| (-1..=1).map(move |dj| (di, dj)))
            .filter_map(|(di, dj)| rock.get(&(cell(c[0]) + di, cell(c[1]) + dj)))
            .copied()
            .fold(f64::MIN, f64::max);
        if near <= f64::MIN {
            bare += 1;
            continue;
        }
        gaps.push(c[2] - near);
        if c[2] - near >= -CELL_SLOPE_MM && c[2] - near <= FILM_MAX_MM + CELL_SLOPE_MM {
            over += 1;
        }
    }
    gaps.sort_by(f64::total_cmp);
    assert!(film > 100, "the film is laid: {film}");
    // never over the band's high (grain is held under it) ...
    let highest = gaps.last().copied().unwrap();
    assert!(
        highest <= FILM_MAX_MM + 0.05,
        "a film triangle {highest} mm over the stone's top, band high {FILM_MAX_MM}"
    );
    // ... and the film as a whole is a layer of that band: its middle lies
    // between nothing and the band's high (grain pits take it below the
    // band's low, by design; a half of it is over half the least thickness)
    let median = gaps[gaps.len() / 2];
    assert!(
        median > 0.5 * FILM_MIN_MM - CELL_SLOPE_MM && median < FILM_MAX_MM,
        "median film {median} mm over the stone"
    );
    // none lies where no stone stands, and all but the film's own edge (a
    // wall a lip's width from the stone's top) lie within its thickness of one
    assert_eq!(bare, 0, "film triangles over no stone at all");
    assert!(
        over * 100 >= film * 98,
        "film triangles not over a standing stone: {} of {film}",
        film - over
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
        "stone-size broken",
        "stone-size broken\n            ledges 2\n            shattered patches=2 width=\"5..10\"",
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
