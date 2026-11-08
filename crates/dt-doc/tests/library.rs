//! The library tiles and parts decode, and every value they write
//! arrives in the model (nothing in the tile is lost).
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use common::*;
use dt_doc::model::{Callee, Definition, Entry, Form, Source, TileHeader, Value};
use dt_doc::place::{Along, Edge, Point};
use dt_doc::schema::{Axis, Kind};

fn none(_: &str) -> Option<f64> {
    None
}

fn tile(doc: &dt_doc::model::Document) -> &Definition {
    doc.tile().expect("a tile")
}

fn entry<'a>(es: &'a [Entry], what: &str) -> &'a Entry {
    es.iter()
        .find(|e| e.what.value == what)
        .unwrap_or_else(|| panic!("no `{what}`"))
}

/// (low, high) of a numeric value at the reference, levels from the prelude.
fn range(v: Option<&Value>) -> (f64, f64) {
    let env = |n: &str| match n {
        "ground" => Some(5.0),
        "edge" => Some(3.4),
        "foot" => Some(4.0),
        _ => None,
    };
    match v {
        Some(Value::Num(n)) => n.bounds(&env).unwrap(),
        other => panic!("not a number: {other:?}"),
    }
}

fn num(v: Option<&Value>) -> f64 {
    let (lo, hi) = range(v);
    assert!((lo - hi).abs() < 1e-12, "a range, not a number");
    lo
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

fn word(v: Option<&Value>) -> &'static str {
    match v {
        Some(Value::Word(w)) => w.word,
        other => panic!("not a word: {other:?}"),
    }
}

#[test]
fn every_library_file_decodes() {
    let k = known();
    let mut bad = String::new();
    for (name, text) in ALL {
        if let Err(e) = dt_doc::decode(name, text, &k) {
            bad.push_str(&format!("{name}:\n{}\n", render(&e)));
        }
    }
    assert!(bad.is_empty(), "{bad}");
}

#[test]
fn flat_tile_is_one_ground() {
    let doc = decode_ok(FLAT);
    let t = tile(&doc);
    assert_eq!(t.name.value, "tile");
    assert_eq!(
        t.header(),
        Some(TileHeader {
            cols: 1,
            rows: 1,
            seed: Some(1)
        })
    );
    assert!(t.regions.is_empty() && t.lines.is_empty());
    assert_eq!(t.stack.len(), 1);
    let g = &t.stack[0];
    assert_eq!(g.what.value, "dirt");
    assert_eq!(g.kind, Kind::Surface);
    // Flat at the edge height: its top, and the two words that keep it quiet.
    assert!(g.top.is_some(), "flat: its top is written");
    assert_eq!(g.controls.len(), 2, "flat: clods and stones");
    assert!(t.base.is_some(), "the tile writes its base");
    let words = g.element().unwrap().control("stones").unwrap();
    assert_eq!(words.value, Some(Axis::Word));
}

#[test]
fn printer_profile() {
    let doc = decode_ok(PRINTER);
    let p = doc.printer.as_ref().unwrap();
    assert_eq!(p.name.value, "default");
    assert_eq!(
        (
            p.line.value,
            p.layer.value,
            p.overhang.value,
            p.steepest.value,
            p.gap.value
        ),
        (0.45, 0.2, 55.0, 65.0, 0.2)
    );
}

#[test]
fn steepest_is_past_overhang_and_within_one_line() {
    let profile = |steepest: f64| {
        format!("printer p {{ line 0.45; layer 0.2; overhang 55; steepest {steepest}; gap 0.2 }}")
    };
    let (_, shown) = fails(&profile(50.0));
    assert!(flat(&shown).contains("must be past `overhang`"), "{shown}");
    // 0.2 × tan 70° = 0.55 mm, more than a 0.45 mm line
    let (_, shown) = fails(&profile(70.0));
    assert!(
        flat(&shown).contains("more than one 0.45 mm line"),
        "{shown}"
    );
    assert!(flat(&shown).contains("at most 66.0"), "{shown}");
}

#[test]
fn grass_parts_nest() {
    let doc = decode_ok(GRASS);
    let lawn = doc.definition("lawn").unwrap();
    let params: Vec<_> = lawn.params.iter().map(|p| p.name.value.as_str()).collect();
    assert_eq!(params, ["length", "height", "clearings", "cover"]);
    // Each parameter is typed by the grass control it fills.
    for p in &lawn.params {
        assert_eq!(p.control.unwrap().name, p.name.value);
    }
    assert!(close(
        range(lawn.params[1].default.as_ref().map(|d| &d.value)),
        (1.5, 2.0)
    ));
    assert!(close(
        range(lawn.params[3].default.as_ref().map(|d| &d.value)),
        (1.0, 1.0)
    ));
    let grass = entry(&lawn.stack, "grass");
    assert_eq!(grass.value("length"), Some(&Value::Param("length".into())));
    assert_eq!(lawn.summary, Kind::Surface);
    assert!(!lawn.thing);

    // long-grass calls lawn: a definition inside a definition.
    let long = doc.definition("long-grass").unwrap();
    let call = entry(&long.stack, "lawn");
    assert!(matches!(&call.callee, Callee::Definition(d) if d == "lawn"));
    // longer, taller blades than the lawn's
    assert!(close(range(call.value("length")), (5.0, 9.0)));
    assert!(close(range(call.value("height")), (2.0, 3.0)));
    assert_eq!(
        call.control("height").unwrap().control.unwrap().name,
        "height"
    );
}

#[test]
fn trees_part_keeps_expert_branching() {
    let doc = decode_ok(TREES);
    let oak = doc.definition("oak").unwrap();
    assert!(oak.thing, "an oak is one thing");
    assert_eq!(oak.summary, Kind::Solid);
    let tree = entry(&oak.stack, "tree");
    assert_eq!(tree.value("height"), Some(&Value::Param("height".into())));
    assert!(close(
        range(oak.params[0].default.as_ref().map(|d| &d.value)),
        (50.0, 50.0)
    ));
    assert_eq!(word(tree.sub("crown", "shape")), "wide");
    for (k, v) in [
        ("split", 34.0),
        ("eagerness", 0.6),
        ("target", 0.25),
        ("balance", 0.8),
        ("exponent", 3.0),
        ("min-radius", 0.9),
    ] {
        assert!((num(tree.sub("branch", k)) - v).abs() < 1e-12, "{k}");
    }
}

#[test]
fn grass_tree_keeps_every_control() {
    let doc = decode_ok(GRASS_TREE);
    assert_eq!(doc.uses[0].path.value, "parts/grass");
    assert_eq!(doc.uses[0].source, Source::Library("parts/grass".into()));
    assert_eq!(doc.uses[0].names[0].value, "long-grass");
    let t = tile(&doc);
    // SurfaceConfig(cols=2, rows=2, seed=84)
    assert_eq!(
        t.header(),
        Some(TileHeader {
            cols: 2,
            rows: 2,
            seed: Some(84)
        })
    );
    // No region: the whole tile is the default area.
    assert!(t.regions.is_empty());
    let order: Vec<_> = t.stack.iter().map(|e| e.what.value.as_str()).collect();
    assert_eq!(order, ["dirt", "long-grass", "tree"]);
    let kinds: Vec<_> = t.stack.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [Kind::Surface, Kind::Surface, Kind::Solid]);

    let tree = entry(&t.stack, "tree");
    assert!(tree.place.at.is_none(), "no at=: anywhere on the tile");
    assert_eq!(num(tree.value("height")), 40.0);
    assert_eq!(tree.control("height").unwrap().form, Form::Property);
    assert_eq!(num(tree.sub("trunk", "height")), 30.0);
    assert_eq!(num(tree.sub("crown", "width")), 30.0);
    // one clump per branch end
    assert_eq!(num(tree.sub("crown", "clumps")), 32.0);
    // The words keep the numbers they stand for.
    let shape = tree.sub("crown", "shape").unwrap().word().unwrap();
    assert_eq!(shape.word, "round");
    assert_eq!(shape.sets[1], ("top-curve", 1.4));
    let branching = tree.sub("crown", "branching").unwrap().word().unwrap();
    assert_eq!(branching.word, "normal");
    assert_eq!(branching.sets[0], ("split", 30.0));
    // leaf_length_mm=4.5, leaf_width_mm=3.0
    assert_eq!(num(tree.sub("leaf", "length")), 4.5);
    assert_eq!(num(tree.sub("leaf", "width")), 3.0);
    assert_eq!(tree.path, ["tile", "stack", "tree"]);
}

#[test]
fn wall_corner_keeps_every_control() {
    let doc = decode_ok(WALL_CORNER);
    let t = tile(&doc);
    assert_eq!(t.header().unwrap().seed, Some(64));
    assert_eq!((t.header().unwrap().cols, t.header().unwrap().rows), (1, 1));
    let order: Vec<_> = t.stack.iter().map(|e| e.what.value.as_str()).collect();
    assert_eq!(order, ["dirt", "wall", "lawn"]);

    let wall = entry(&t.stack, "wall");
    assert_eq!(
        wall.place.along.as_ref().unwrap().value,
        Along::Edges(vec![Edge::North, Edge::West])
    );
    assert!(wall.place.side.is_none());
    assert_eq!(word(wall.value("style")), "fieldstone");
    assert_eq!(wall.place.seed.as_ref().unwrap().value, 14);
    let full = wall.value("height").unwrap().word().unwrap();
    assert_eq!((full.word, full.level), ("full", Some("wall-top")));
    assert_eq!(wall.control("height").unwrap().form, Form::Child);
    assert_eq!(num(wall.value("thickness")), 7.0); // thickness_mm 7.0
    assert_eq!(word(wall.value("coping")), "loose");
    assert!(close(range(wall.sub("stones", "height")), (2.2, 5.2)));
    assert!(close(range(wall.sub("stones", "length")), (5.0, 16.0)));
    assert_eq!(num(wall.sub("joints", "width")), 0.0);
    assert_eq!(num(wall.sub("joints", "depth")), 2.8);
    assert!(close(range(wall.sub("edges", "rounding")), (1.3, 2.6)));
    assert_eq!(wall.kind, Kind::Solid);
    assert_eq!(entry(&t.stack, "lawn").kind, Kind::Surface);
}

#[test]
fn cliff_run_keeps_every_control() {
    let doc = decode_ok(CLIFF_RUN);
    let t = tile(&doc);
    assert_eq!(
        t.header(),
        Some(TileHeader {
            cols: 2,
            rows: 1,
            seed: Some(1)
        })
    );
    assert!(
        t.about
            .as_deref()
            .unwrap()
            .starts_with("Rock band against the north edge.")
    );
    // FOOT_Z_MM = 4.0
    assert_eq!(t.consts[0].name.value, "footHeight");

    let order: Vec<_> = t.stack.iter().map(|e| e.what.value.as_str()).collect();
    assert_eq!(order, ["dirt", "cliff", "scatter", "scatter"]);

    // APRON_H_MM (1.5, 3.5), APRON_R_MM (3.0, 6.5)
    let dirt = entry(&t.stack, "dirt");
    assert_eq!(
        dirt.top.as_ref().map(|t| &t.value),
        Some(&dt_doc::expr::Expr::Name("footHeight".into()))
    );
    assert_eq!(
        dirt.sub("bank", "against"),
        Some(&Value::Tag("cliff".into()))
    );
    assert!(close(range(dirt.sub("bank", "height")), (1.5, 3.5)));
    assert!(close(range(dirt.sub("bank", "reach")), (3.0, 6.5)));
    // GRIT_PER_SQUARE 45, GRIT_D_MM (1, 2.2), GRIT_BURY (0.2, 0.5): on the dirt.
    let grit = &dirt.children[0];
    assert!(grit.is_scatter());
    assert_eq!(num(grit.value("per-square")), 45.0);
    let pebble = &grit.children[0];
    assert_eq!(pebble.what.value, "pebble");
    assert!(close(range(pebble.value("size")), (1.0, 2.2)));
    assert!(close(range(pebble.value("buried")), (0.2, 0.5)));
    assert_eq!(dirt.kind, Kind::Mixed, "dirt holding pebbles");
    assert_eq!(pebble.path, ["tile", "stack", "dirt", "scatter", "pebble"]);

    // FORMS['run']: height 35, mass 1.35, cap; KINDS shares.
    let cliff = entry(&t.stack, "cliff");
    assert_eq!(
        cliff.place.along.as_ref().unwrap().value,
        Along::Edges(vec![Edge::North])
    );
    assert_eq!(num(cliff.value("height")), 35.0);
    let mixed = cliff.value("rock").unwrap().word().unwrap();
    assert_eq!(
        mixed.sets,
        &[
            ("columns", 38.0),
            ("blocks", 38.0),
            ("slabs", 12.0),
            ("odd", 6.0),
            ("monoliths", 6.0)
        ]
    );
    assert_eq!(
        cliff.value("stone-size").unwrap().word().unwrap().sets,
        &[("stone-size", 1.35)]
    );
    assert_eq!(word(cliff.value("top")), "dirt");
    assert_eq!(num(cliff.sub("shattered", "patches")), 3.0);
    assert!(close(range(cliff.sub("shattered", "width")), (5.0, 10.0)));

    // SCREE_PER_SQUARE 55, SCREE_D_MM (1, 6) ** 2, SCREE_REACH_MM 4
    let stones = &t.stack[2];
    assert_eq!(stones.place.against.as_ref().unwrap().value, "cliff");
    assert_eq!(num(stones.value("per-square")), 55.0);
    assert_eq!(num(stones.value("reach")), 4.0);
    let stone = &stones.children[0];
    assert!(close(range(stone.value("size")), (1.0, 6.0)));
    assert_eq!(
        stone.value("mostly").unwrap().word().unwrap().sets,
        &[("exponent", 2.0)]
    );
    assert_eq!(stones.kind, Kind::Solid);
    assert_eq!(stones.path, ["tile", "stack", "scatter"]);
    // SCREE_BLOCKS 2.5, SCREE_BLOCK_MM (4.5, 8)
    let boulders = &t.stack[3];
    assert_eq!(boulders.path, ["tile", "stack", "scatter#2"]);
    assert_eq!(num(boulders.value("per-square")), 2.5);
    assert!(close(range(boulders.children[0].value("size")), (4.5, 8.0)));
}

#[test]
fn path_rocks_is_a_divided_tile() {
    let doc = decode_ok(PATH_ROCKS);
    let t = tile(&doc);
    assert_eq!((t.header().unwrap().cols, t.header().unwrap().rows), (1, 2));
    let trail = &t.lines[0];
    assert_eq!(trail.name.value, "trail");
    let pts: Vec<_> = trail
        .through
        .iter()
        .map(|p| p.value.bounds(1.0, 2.0).unwrap())
        .collect();
    assert_eq!(
        pts,
        [
            ((0.5, 0.5), (0.0, 0.0)),
            ((0.65, 0.65), (1.0, 1.0)),
            ((0.5, 0.5), (2.0, 2.0))
        ]
    );
    assert_eq!(
        trail.width.as_ref().unwrap().value.bounds(&none).unwrap().0,
        9.0
    );
    assert_eq!(
        trail
            .wobble
            .as_ref()
            .unwrap()
            .value
            .bounds(&none)
            .unwrap()
            .0,
        3.0
    );
    let verge = &t.regions[0];
    assert_eq!(verge.bounded_by[0].value, "trail");
    assert_eq!(verge.fills.len(), 2);

    let path = &t.stack[1];
    assert_eq!(
        path.place.along.as_ref().unwrap().value,
        Along::Line("trail".into())
    );
    assert!(
        path.value("level").is_none(),
        "the path is a second dirt, with no level of its own"
    );
    assert_eq!(path.path, ["tile", "stack", "dirt#2"]);

    let cluster = &t.stack[2];
    assert_eq!(cluster.place.region.as_ref().unwrap().value, "verge");
    assert!(matches!(
        cluster.place.at.as_ref().unwrap().value,
        Point::Pair { .. }
    ));
    assert_eq!(num(cluster.value("count")), 5.0);
    assert_eq!(num(cluster.value("spread")), 6.0);
    let rock = &cluster.children[0];
    assert_eq!(word(rock.value("shape")), "rounded");
    assert!(close(range(rock.value("buried")), (0.3, 0.3)));
    let lawn = &t.stack[3];
    assert_eq!(lawn.place.region.as_ref().unwrap().value, "verge");
    assert!(close(range(lawn.value("cover")), (0.85, 0.85)));
}

#[test]
fn spans_point_at_the_text() {
    let doc = decode_ok(CLIFF_RUN);
    let t = tile(&doc);
    let cliff = entry(&t.stack, "cliff");
    let text = CLIFF_RUN.1;
    let at = |s: miette::SourceSpan| &text[s.offset()..s.offset() + s.len()];
    assert_eq!(at(cliff.what.span), "cliff");
    assert_eq!(
        at(cliff
            .control("height")
            .unwrap()
            .value
            .as_ref()
            .unwrap()
            .span),
        "35"
    );
    assert_eq!(
        at(cliff.control("rock").unwrap().value.as_ref().unwrap().span),
        "mixed"
    );
    assert_eq!(at(cliff.place.along.as_ref().unwrap().span), "north");
}
