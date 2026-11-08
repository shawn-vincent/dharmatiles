//! The heights language: tops, pins, places by name, `let` and `override`,
//! and the static refusals (names, types, order).
#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use dt_doc::model::{ConstKind, PlaceRef};

const STREAM: &str = r#"
tile size="1x1" seed=62 {
    let bankBand "[waterBedHeight .. groundHeight + 1]"
    point crossing "(0.65, 0.5)"
    line stream wobble=0.5 { through westCenter eastCenter }
    region banks { within 11.5 of=stream }
    region bed   { within 1.5 of=stream }
    stack {
        base
        dirt {
            slope 30
            pin top=bankBand at=banks
            pin top="[waterBedHeight .. waterBedHeight + 0.3]" at=bed
        }
        rock size="6..10" at=crossing
        grass
        water top="[waterSurfaceHeight - 1.9 .. waterSurfaceHeight + 3]" at=banks
    }
}
"#;

fn assert_fails(text: &str, code: &str, want: &str) {
    let (e, shown) = fails(text);
    assert!(e.iter().any(|e| e.code == code), "want {code}:\n{shown}");
    assert!(flat(&shown).contains(want), "want `{want}`:\n{shown}");
}

fn tile(stack: &str) -> String {
    format!("tile size=\"1x1\" {{\n{stack}\n}}")
}

#[test]
fn the_library_heights_are_constants() {
    let k = common::known();
    let names: Vec<&str> = k.consts().iter().map(|c| c.name.value.as_str()).collect();
    for want in [
        "waterBedHeight",
        "waterSurfaceHeight",
        "waterEdgeHeight",
        "dirtEdgeHeight",
        "groundHeight",
        "floorHeight",
        "floorEdgeHeight",
        "shortWallHeight",
        "wallHeight",
        "tallWallHeight",
    ] {
        assert!(names.contains(&want), "{want} in {names:?}");
    }
}

#[test]
fn the_edge_defaults_are_read_with_their_pins() {
    let doc = decode_ok(EDGES);
    let names: Vec<&str> = doc.defaults.iter().map(|e| e.what.value.as_str()).collect();
    assert_eq!(names, ["dirt", "floor", "water"]);
    let dirt = &doc.defaults[0];
    assert!(dirt.top.is_some());
    assert_eq!(dirt.pins.len(), 1);
    assert_eq!(dirt.pins[0].at.value, PlaceRef::Name("edges".to_owned()));
}

#[test]
fn a_stream_with_its_banks_drawn_decodes() {
    let doc = snippet(STREAM);
    let t = doc.tile().unwrap();
    assert!(t.base.is_some_and(|b| b.index == 0));
    assert_eq!(t.consts.len(), 1);
    assert_eq!(t.consts[0].kind, ConstKind::Let);
    assert_eq!(t.points[0].name.value, "crossing");
    assert_eq!((t.points[0].x, t.points[0].y), (0.65, 0.5));
    assert_eq!(t.regions.len(), 2);
    assert_eq!(t.regions[0].within.len(), 1);
    let dirt = &t.stack[0];
    assert_eq!(dirt.pins.len(), 2);
    assert_eq!(dirt.slope.as_ref().unwrap().value, 30.0);
    // `at=` of an area-filler also reads as today's `in=`
    let water = t.stack.last().unwrap();
    assert!(water.top.is_some());
    assert_eq!(water.place.region.as_ref().unwrap().value, "banks");
    // a thing at a named point
    let rock = &t.stack[1];
    assert!(matches!(
        rock.place.at.as_ref().unwrap().value,
        dt_doc::place::Point::Named(_)
    ));
}

#[test]
fn closed_lines_and_regions_that_only_except() {
    let doc = snippet(&tile(
        r#"
    line ring closed=#true wobble=2 { through "(0.3, 0.3)" "(0.7, 0.3)" "(0.5, 0.7)" }
    region inside { bounded-by ring; fill center }
    region outside { except inside }
    region verge { except inside; within 3 of=northEdge }
    stack { base; dirt }"#,
    ));
    let t = doc.tile().unwrap();
    assert!(t.lines[0].closed);
    assert_eq!(t.regions[1].except[0].value, "inside");
    assert!(t.regions[1].fills.is_empty());
}

#[test]
fn override_replaces_a_library_height_and_let_adds_a_name() {
    let doc = snippet(&tile(
        r#"
    let tarnHeight 9
    override waterBedHeight 0.8
    stack { base; dirt top="tarnHeight + [0 .. 1]" }"#,
    ));
    let t = doc.tile().unwrap();
    let kinds: Vec<_> = t.consts.iter().map(|c| c.kind).collect();
    assert_eq!(kinds, [ConstKind::Let, ConstKind::Override]);
}

// ---- refusals ----------------------------------------------------

#[test]
fn let_on_an_existing_name_is_refused_with_the_other_keyword() {
    assert_fails(
        &tile("let groundHeight 6\nstack { dirt }"),
        "dt::name",
        "`override groundHeight …` replaces it for this tile",
    );
    assert_fails(
        &tile("let a 1\nlet a 2\nstack { dirt }"),
        "dt::name",
        "already declared",
    );
    // one namespace: a point cannot reuse a constant's name
    assert_fails(
        &tile("point groundHeight \"(0.5, 0.5)\"\nstack { dirt }"),
        "dt::name",
        "already declared",
    );
}

#[test]
fn override_of_a_missing_name_is_refused() {
    assert_fails(
        &tile("override waterBedHieght 0.8\nstack { dirt }"),
        "dt::name",
        "did you mean `override waterBedHeight …`",
    );
    assert_fails(
        &tile("override center 0.8\nstack { dirt }"),
        "dt::name",
        "already a word of the language",
    );
    assert_fails(
        &tile("override dirt 0.8\nstack { dirt }"),
        "dt::name",
        "already a word of the language",
    );
}

#[test]
fn names_are_camel_case() {
    assert_fails(
        &tile("let knoll-top 6\nstack { dirt }"),
        "dt::name",
        "`knollTop`",
    );
}

#[test]
fn types_are_checked_where_they_are_written() {
    assert_fails(
        &tile("stack { dirt top=\"groundHeight + x\" }"),
        "dt::type",
        "mixes",
    );
    assert_fails(&tile("stack { dirt top=x }"), "dt::type", "squares");
    // a body, grass or scatter takes a region
    assert_fails(
        &tile("stack { dirt at=northEdge }"),
        "dt::type",
        "`dirt` fills a region, and `northEdge` is a line",
    );
    assert_fails(&tile("stack { dirt at=center }"), "dt::type", "is a point");
    assert_fails(
        &tile("stack { dirt\n grass at=\"(0.5, 0.5)\" }"),
        "dt::type",
        "`grass` fills a region",
    );
    assert_fails(
        &tile(
            "region r { within 2 of=center }\nstack { dirt\n scatter count=2 at=center { rock } }",
        ),
        "dt::type",
        "`scatter` fills a region",
    );
    // a length is a length
    assert_fails(
        &tile("region r { within \"groundHeight + x\" of=center }\nstack { dirt }"),
        "dt::type",
        "not known here",
    );
}

#[test]
fn unknown_names_and_places_are_refused_with_a_suggestion() {
    assert_fails(
        &tile("stack { dirt top=groundHieght }"),
        "dt::unknown-name",
        "did you mean `groundHeight`",
    );
    assert_fails(
        &tile("stack { dirt\n rock at=nowhere }"),
        "dt::place",
        "no place is called `nowhere`",
    );
    assert_fails(
        &tile("stack { dirt { pin top=3 at=southCentre } }"),
        "dt::place",
        "did you mean `southCenter`",
    );
}

#[test]
fn a_pin_needs_a_top_and_a_place() {
    assert_fails(
        &tile("stack { dirt { pin at=center } }"),
        "dt::pin",
        "a pin needs top=",
    );
    assert_fails(
        &tile("stack { dirt { pin top=3 } }"),
        "dt::pin",
        "a pin needs at=",
    );
    assert_fails(
        &tile("stack { dirt\n grass { pin top=3 at=center } }"),
        "dt::pin",
        "has no pins",
    );
}

#[test]
fn the_base_is_first_and_fluids_are_last() {
    assert_fails(
        &tile("stack { dirt\n base }"),
        "dt::base-order",
        "the base is the first line of the stack",
    );
    assert_fails(
        &tile("stack { base\n base\n dirt }"),
        "dt::base-order",
        "one base",
    );
    assert_fails(
        &tile("stack { base\n dirt\n water\n rock size=3 }"),
        "dt::fluid-order",
        "`rock` is written after the water",
    );
    // today's tiles, with no heights, keep their order
    snippet(&tile("stack { dirt\n water\n rock size=3 }"));
}

#[test]
fn a_thing_stands_at_a_literal_point_or_a_name_and_a_body_has_a_name() {
    let doc = snippet(&tile(
        "point spot \"(0.25, 0.75)\"\n stack { base\n dirt hillDirt\n rock size=4 at=\"(0.4, 0.6)\"\n rock size=4 at=spot }",
    ));
    let t = doc.tile().unwrap();
    assert_eq!(t.stack[0].label.as_ref().unwrap().value, "hillDirt");
    let (a, b) = (&t.stack[1], &t.stack[2]);
    assert!(matches!(
        a.place.at.as_ref().unwrap().value,
        dt_doc::place::Point::Pair { .. }
    ));
    assert_eq!(
        a.at_place.as_ref().unwrap().value,
        PlaceRef::Point(0.4, 0.6)
    );
    assert_eq!(
        b.at_place.as_ref().unwrap().value,
        PlaceRef::Name("spot".to_owned())
    );
    // today's quoted pair still reads
    snippet(&tile("stack { rock size=4 at=\"0.4,0.6\" }"));
}

#[test]
fn a_cliffs_own_top_words_stay_its_own() {
    let doc = snippet(&tile(
        "stack { cliff along=north top=dirt\n cliff along=south top=\"dirtEdgeHeight + 35\" }",
    ));
    let t = doc.tile().unwrap();
    assert!(t.stack[0].top.is_none() && t.stack[0].control("top").is_some());
    assert!(t.stack[1].top.is_some());
}
