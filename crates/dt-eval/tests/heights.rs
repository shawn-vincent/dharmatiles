//! The heights language through `check`: tops, pins, defaults and the Solve
//! phase, on the hand-migrated library tiles and on small documents.  No
//! meshes: this is the check level.  (`build` does not build tops and pins
//! yet; the library tiles here stay in tests/ until it does.)

#![allow(clippy::unwrap_used)]

use std::fmt::Write as _;

use dt_eval::{Checked, Diag, Library, Memory, Options, build, check};

fn mem(text: &str) -> Memory {
    Memory(std::iter::once(("t.dtile".to_owned(), text.to_owned())).collect())
}

fn tile(body: &str) -> String {
    format!("tile size=\"1x1\" seed=3 {{\n{body}\n}}\n")
}

fn solved(text: &str) -> Checked {
    check(&mem(text), "t.dtile", &Options::default()).unwrap_or_else(|e| panic!("{text}\n{e:#?}"))
}

fn refused(text: &str) -> Vec<Diag> {
    match check(&mem(text), "t.dtile", &Options::default()) {
        Ok(_) => panic!("expected a refusal:\n{text}"),
        Err(e) => e,
    }
}

fn library(name: &str) -> Checked {
    let group = if name.contains("pond") || name.contains("stream") {
        "water"
    } else {
        "ground"
    };
    let path = format!("lib:tests/{group}/{name}.dtile");
    check(&Library, &path, &Options::default()).unwrap_or_else(|e| panic!("{path}: {e:#?}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn codes(c: &Checked) -> Vec<&'static str> {
    c.notes().iter().map(|d| d.code).collect()
}

#[test]
fn plain_dirt_takes_the_library_top_and_rim() {
    let c = solved(&tile("stack { base\n dirt }"));
    let s = c.solved.as_ref().unwrap();
    let d = &s.bodies[0];
    // exactly dirtEdgeHeight at every rim, and the default top away from it
    for (x, y) in [(0.0, 0.5), (1.0, 0.2), (0.4, 0.0), (0.9, 1.0), (0.0, 0.0)] {
        let b = d.at(x, y);
        assert!(close(b.lo, 3.4) && close(b.hi, 3.4), "({x}, {y}): {b:?}");
    }
    let mid = d.at(0.5, 0.5);
    assert!(close(mid.lo, 5.0) && close(mid.hi, 6.0), "{mid:?}");
    assert!(c.notes().is_empty(), "{:?}", c.notes());
    // the rim pin is the library's, and wins on all of the rim
    let rim = &d.pins[0];
    assert!(rim.default && rim.wins == rim.place_nodes && rim.place_nodes > 0);
}

#[test]
fn a_tiles_own_top_replaces_the_default_and_its_pins_win() {
    let c = solved(&tile(
        "let hump \"groundHeight + [1 .. 2]\"\n point mound \"(0.5, 0.5)\"\n stack { base\n dirt top=\"[groundHeight .. groundHeight + 2.5]\" { pin top=hump at=mound } }",
    ));
    let d = &c.solved.as_ref().unwrap().bodies[0];
    let m = d.at(0.5, 0.5);
    assert!(close(m.lo, 6.0) && close(m.hi, 7.0), "{m:?}");
    // the rim is still the library's pin
    let r = d.at(0.0, 0.3);
    assert!(close(r.lo, 3.4) && close(r.hi, 3.4));
    // the pin is exact at its point and the gap eases back: not at 5..7.5 right beside it
    assert!(d.pins.iter().any(|p| !p.default && p.wins == 1));
}

#[test]
fn bottom_is_the_surface_under_the_body() {
    // a film on the dirt: 0.6 to 1.2 mm over wherever the dirt is
    let c = solved(&tile(
        "region crag { within 6 of=center }\n stack { base\n dirt top=\"groundHeight + [0 .. 1]\"\n dirt top=\"bottom + [0.6 .. 1.2]\" at=crag }",
    ));
    let film = &c.solved.as_ref().unwrap().bodies[1];
    let at = film.at(0.5, 0.5);
    let under = c.solved.as_ref().unwrap().bodies[0].at(0.5, 0.5);
    assert!(
        close(at.lo, under.lo + 0.6) && close(at.hi, under.hi + 1.2),
        "{at:?} on {under:?}"
    );
    // only the first dirt is the first land layer: the film is not pulled to the rim
    assert!(film.pins.is_empty());
}

#[test]
fn a_steep_pin_is_noted_with_the_pins_that_cause_it() {
    let c = solved(&tile(
        "point summit \"(0.5, 0.5)\"\n stack { base\n dirt { pin top=\"groundHeight + 9\" at=summit } }",
    ));
    assert!(codes(&c).contains(&"dt::steep"), "{:?}", c.notes());
    let n = c.notes().iter().find(|d| d.code == "dt::steep").unwrap();
    assert!(n.message.contains("steeper than its 24"), "{}", n.message);
    // the same summit, allowed by a steeper slope on the body, is quiet
    let c = solved(&tile(
        "point summit \"(0.5, 0.5)\"\n stack { base\n dirt { slope 60\n pin top=\"groundHeight + 9\" at=summit } }",
    ));
    assert!(!codes(&c).contains(&"dt::steep"), "{:?}", c.notes());
}

#[test]
fn a_pin_covered_by_a_later_one_is_shadowed() {
    let c = solved(&tile(
        "point spot \"(0.5, 0.5)\"\n stack { base\n dirt {\n pin a top=7 at=spot\n pin b top=8 at=spot } }",
    ));
    let n = c
        .notes()
        .iter()
        .find(|d| d.code == "dt::shadowed")
        .expect("a shadowed note");
    assert!(n.message.contains("wins nowhere"), "{}", n.message);
    assert!(n.message.contains("pin `b`"), "{}", n.message);
}

#[test]
fn a_pin_that_covers_a_library_default_pin_says_nothing() {
    // the default edge pin is covered whole by the tile's own: that is how a
    // tile overrides it, so there is no `dt::shadowed` note; the tile's own
    // covered pin still gets one (the test above)
    let c = solved(&tile("stack { base\n dirt { pin top=7 at=edges } }"));
    assert!(!codes(&c).contains(&"dt::shadowed"), "{:?}", c.notes());
}

#[test]
fn a_pin_with_no_ground_is_refused() {
    // a region the tile's lines never enclose
    let e = refused(&tile(
        "line wall { through \"(0, 0.5)\" \"(1, 0.5)\" }\n region nowhere { boundedBy wall; fill \"(0.5, 0.5)\" }\n stack { base\n dirt { pin top=7 at=nowhere } }",
    ));
    assert_eq!(e[0].code, "dt::empty");
    assert!(e[0].message.contains("covers nothing"), "{}", e[0].message);
}

#[test]
fn a_layer_that_can_never_be_thick_enough_is_refused() {
    let e = refused(&tile(
        "region r { within 5 of=center }\n stack { base\n dirt top=5\n dirt top=\"bottom + [0 .. 0.3]\" at=r }",
    ));
    assert_eq!(e[0].code, "dt::thin");
    assert!(e[0].message.contains("skin"), "{}", e[0].message);
}

#[test]
fn a_cliff_written_at_a_tile_edge_is_refused_with_the_region_to_write() {
    // `at=north` is the old form's word: an edge has no width, so the cliff
    // has no region of its own (`dharmatiles migrate` writes it as a band)
    let e = refused(&tile("stack { base\n dirt\n cliff at=north }"));
    assert_eq!(e[0].code, "dt::type", "{e:?}");
    assert!(e[0].help.as_deref().unwrap().contains("within"), "{e:?}");
}

#[test]
fn override_moves_the_tide_with_the_surface() {
    // waterEdgeHeight reads waterSurfaceHeight: overriding it moves the edge
    let base = solved(&tile("stack { base\n dirt\n water }"));
    let over = solved(&tile(
        "override waterSurfaceHeight 3\n stack { base\n dirt\n water }",
    ));
    let at = |c: &Checked| c.solved.as_ref().unwrap().bodies[1].at(0.5, 0.0);
    // cos(720 * 0.5) = 1: the crest
    assert!(close(at(&base).lo, 3.2), "{:?}", at(&base));
    assert!(close(at(&over).lo, 3.6), "{:?}", at(&over));
    assert_eq!(over.explain()[0], "override waterSurfaceHeight 2.6 → 3");
}

#[test]
fn heights_scale_with_the_system() {
    let c = check(
        &mem(&tile("stack { base\n dirt }")),
        "t.dtile",
        &Options {
            system: "ol".into(),
            ..Options::default()
        },
    )
    .unwrap();
    let d = &c.solved.as_ref().unwrap().bodies[0];
    let rim = d.at(0.0, 0.5);
    assert!(close(rim.lo, 3.4 * 25.4 / 35.0), "{rim:?}");
}

#[test]
fn a_tile_with_a_dirt_is_solved_without_writing_any_heights() {
    let c = solved(&tile("stack { dirt }"));
    assert!(c.solved.is_some());
    assert!(c.pending().is_empty());
}

#[test]
fn a_tile_that_only_names_its_base_builds_as_today() {
    // `base`, a name for a region and `at=`: all read, nothing pending
    let c = solved(&tile(
        "region r { within 6 of=center }\n stack { base\n dirt\n grass at=r }",
    ));
    assert!(c.pending().is_empty(), "{:?}", c.pending());
    assert!(c.solved.is_some());
}

#[test]
fn a_cliffs_top_builds() {
    let text = tile(
        "region r { within 12 of=northEdge }\n stack { base\n dirt\n cliff top=\"dirtEdgeHeight + 35\" at=r }",
    );
    let c = solved(&text);
    assert!(c.pending().is_empty(), "{:?}", c.pending());
    assert!(c.solved.is_some());
}

#[test]
fn a_dirts_top_and_pins_build() {
    let text = tile("stack { base\n dirt top=6 { pin top=4 at=center } }");
    let c = solved(&text);
    assert!(c.pending().is_empty(), "{:?}", c.pending());
    assert!(build(&mem(&text), "t.dtile", &Options::default()).is_ok());
}

#[test]
fn a_trees_top_is_its_height_a_rocks_waits_and_a_line_is_refused() {
    // a tree's top is its height: read, nothing pending
    let c = solved(&tile(
        "region canopy { within 15 of=center }\n stack { base\n dirt\n tree top=\"bottom + [40 .. 45]\" at=canopy }",
    ));
    assert!(c.pending().is_empty(), "{:?}", c.pending());
    // a rock's top is not built yet
    let c = solved(&tile(
        "region canopy { within 15 of=center }\n stack { base\n dirt\n rock top=\"bottom + 8\" at=canopy }",
    ));
    assert_eq!(c.pending().len(), 1, "{:?}", c.pending());
    // a line has no one place on it: refused, not pending
    let e = refused(&tile(
        "line trail { through southCenter northCenter }\n stack { base\n dirt\n tree top=\"bottom + [40 .. 45]\" at=trail }",
    ));
    assert_eq!(e[0].code, "dt::type", "{e:?}");
}

#[test]
fn regions_that_within_and_except() {
    let c = solved(&tile(
        "line trail { through southCenter northCenter }\n region path { within 4.5 of=trail }\n region verge { except path }\n stack { base\n dirt { pin top=4 at=path } }",
    ));
    let d = &c.solved.as_ref().unwrap().bodies[0];
    // 4.5 mm either side of x = 0.5 on a 35 mm square: 9 mm of 35
    let path = &d.pins[1];
    let nodes = f64::from(c.solved.as_ref().unwrap().cells_per_square) + 1.0;
    let across = path.place_nodes as f64 / nodes / nodes;
    assert!((across - 9.0 / 35.0).abs() < 0.02, "{across}");
    assert!(close(d.at(0.5, 0.5).lo, 4.0));
}

// ---- the migrated tiles -------------------------------------------------

/// A few bands of a solved tile, to pin its numbers.
fn summary(c: &Checked) -> String {
    let mut out = String::new();
    let s = c.solved.as_ref().unwrap();
    writeln!(out, "cells per square: {}", s.cells_per_square).unwrap();
    writeln!(out, "notes: {:?}", codes(c)).unwrap();
    for b in &s.bodies {
        writeln!(out, "{} ({} nodes)", b.element, b.region_nodes).unwrap();
        for p in &b.pins {
            writeln!(
                out,
                "  {} wins {}/{}",
                p.text.replace(" in the pool", ""),
                p.wins,
                p.place_nodes
            )
            .unwrap();
        }
        for y in [1.0, 0.75, 0.5, 0.25, 0.0] {
            let row: Vec<String> = [0.0, 0.25, 0.5, 0.75, 1.0]
                .iter()
                .map(|&x| {
                    let v = b.at(x, y);
                    if close(v.lo, v.hi) {
                        format!("{:.2}", v.lo)
                    } else {
                        format!("{:.2}..{:.2}", v.lo, v.hi)
                    }
                })
                .collect();
            writeln!(out, "  y={y:.2}  {}", row.join("  ")).unwrap();
        }
    }
    out
}

#[test]
fn the_migrated_tiles_check_and_solve() {
    for name in ["soil", "pond", "stream", "path"] {
        let c = library(&format!("1x1-bands-{name}"));
        assert!(c.solved.is_some(), "{name}");
        // pending: only what build does not do yet
        assert!(
            c.pending().iter().all(|d| d.code == "dt::not-yet"),
            "{name}: {:?}",
            c.pending()
        );
        insta::assert_snapshot!(format!("bands-{name}"), summary(&c));
    }
}

#[test]
fn the_stream_banks_fall_from_the_land_to_the_bed() {
    let c = library("1x1-bands-stream");
    let dirt = &c.solved.as_ref().unwrap().bodies[0];
    // the bed, exactly the bed's band
    let bed = dirt.at(0.5, 0.5);
    assert!(close(bed.lo, 0.4) && close(bed.hi, 0.7), "{bed:?}");
    // beyond the banks, the land
    let land = dirt.at(0.5, 0.95);
    assert!(land.lo >= 3.4 - 1e-9 && land.hi <= 6.0 + 1e-9, "{land:?}");
    // the water fills the banks at its top, narrowing nowhere it was not written
    let water = &c.solved.as_ref().unwrap().bodies[1];
    assert_eq!(water.element, "water");
    assert!(water.region_nodes > 0);
}

/// The height a built tile's one tree came to, mm.
fn tree_height(text: &str) -> f64 {
    let b = build(&mem(text), "t.dtile", &Options::default())
        .unwrap_or_else(|e| panic!("{text}\n{e:#?}"));
    b.notes
        .iter()
        .find(|(k, _)| k == "tree.height_mm")
        .map(|(_, v)| *v)
        .expect("a tree")
}

#[test]
fn a_trees_top_over_its_foot_or_above_zero_is_its_height() {
    // flat ground at groundHeight (4 mm): `bottom + 30` and `groundHeight + 30`
    // stand the same tree
    let tree = |top: &str| {
        tile(&format!(
            "stack {{ base\n dirt top=groundHeight clods=none\n tree at=\"0.5,0.5\" top=\"{top}\" {{ trunk height=12; crown width=14 clumps=12 }} }}"
        ))
    };
    let over = tree_height(&tree("bottom + 30"));
    let above = tree_height(&tree("groundHeight + 30"));
    let taller = tree_height(&tree("bottom + 45"));
    assert!((above - over).abs() < 0.3, "{above} vs {over}");
    assert!(taller > over + 10.0, "{taller} vs {over}");
    // a top that leaves the tree too short where it stands is refused there
    let e = match build(
        &mem(&tree("groundHeight + 10")),
        "t.dtile",
        &Options::default(),
    ) {
        Ok(_) => panic!("a 10 mm tree built"),
        Err(e) => e,
    };
    assert!(
        format!("{e:?}").contains("a tree is 20 to 120 mm tall"),
        "{e:?}"
    );
    // `bottom` is added once
    let e = refused(&tree("bottom * 2"));
    assert!(e[0].message.contains("added once"), "{e:?}");
}
