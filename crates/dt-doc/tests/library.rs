//! The three R6 tiles decode, and every value the Python sources set
//! arrives in the model ("nothing in the tile is lost", plan.html R6).
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use common::*;
use dt_doc::expr::Expr;
use dt_doc::model::{Callee, DefKind, Definition, Entry, Value};

fn tile(doc: &dt_doc::model::Document) -> &Definition {
    doc.definitions
        .iter()
        .find(|d| matches!(d.kind, DefKind::Tile { .. }))
        .expect("a tile")
}

fn entry<'a>(def: &'a Definition, what: &str) -> &'a Entry {
    def.stack
        .iter()
        .find(|e| e.what.value == what)
        .unwrap_or_else(|| panic!("no `{what}` in the stack"))
}

fn none(_: &str) -> Option<f64> {
    None
}

/// (low, high) of a numeric value; names resolve through `env`.
fn range_env(v: Option<&Value>, env: &dyn Fn(&str) -> Option<f64>) -> (f64, f64) {
    match v {
        Some(Value::Expr(x)) => (x.sample(env, 0.0).unwrap(), x.sample(env, 1.0).unwrap()),
        other => panic!("not a number: {other:?}"),
    }
}

fn range(v: Option<&Value>) -> (f64, f64) {
    range_env(v, &none)
}

fn num(v: Option<&Value>) -> f64 {
    let (lo, hi) = range(v);
    assert!((lo - hi).abs() < 1e-12, "a range, not a number");
    lo
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

fn word(v: Option<&Value>) -> &str {
    match v {
        Some(Value::Word(w)) => w,
        Some(Value::Expr(Expr::Name(w))) => w,
        other => panic!("not a word: {other:?}"),
    }
}

#[test]
fn every_library_file_decodes() {
    let k = known();
    for (name, text) in ALL {
        if let Err(e) = dt_doc::decode(name, text, &k) {
            panic!("{name}:\n{}", render(&e));
        }
    }
}

#[test]
fn standard_heights() {
    let doc = decode_ok(HEIGHTS);
    let names: Vec<_> = doc.heights.iter().map(|h| h.name.value.as_str()).collect();
    assert_eq!(names, ["edge", "ground", "water", "wall"]);
}

#[test]
fn grass_parts_nest() {
    let doc = decode_ok(GRASS);
    let lawn = doc.definition("lawn").unwrap();
    let params: Vec<_> = lawn.params.iter().map(|p| p.name.value.as_str()).collect();
    assert_eq!(params, ["length", "curl", "clearance", "clumps"]);
    let meadow = entry(lawn, "meadow");
    // `blade length=length` refers to the parameter.
    assert_eq!(word(meadow.group_setting("blade", "length")), "length");

    // long-grass calls lawn: a definition inside a definition.
    let long = doc.definition("long-grass").unwrap();
    let call = entry(long, "lawn");
    assert!(matches!(&call.callee, Callee::Definition(d) if d == "lawn"));
    // Python: SpeciesConfig(blade_length=D[8:14], blade_curl=D[0.35:0.65], blade_clearance=0.2)
    assert!(close(range(call.setting("length")), (8.0, 14.0)));
    assert!(close(range(call.setting("curl")), (63.0, 117.0))); // 0.35π..0.65π in degrees
    assert!(close(range(call.setting("clearance")), (0.2, 0.2)));
}

#[test]
fn grass_tree_keeps_every_control() {
    let doc = decode_ok(GRASS_TREE);
    assert_eq!(doc.uses[0].value, "lib:parts/grass");
    let t = tile(&doc);
    // SurfaceConfig(cols=2, rows=2, seed=84)
    assert_eq!(
        t.kind,
        DefKind::Tile {
            cols: 2,
            rows: 2,
            seed: Some(84)
        }
    );
    // Region(id='meadow', selector=FloodFill(0.5, 0.5))
    assert_eq!(t.regions[0].name.value, "meadow");
    assert!(t.regions[0].lines.is_empty());
    assert_eq!(t.regions[0].fills.len(), 1);

    let order: Vec<_> = t.stack.iter().map(|e| e.what.value.as_str()).collect();
    assert_eq!(order, ["hills", "tree", "long-grass"]);
    assert!(
        t.stack
            .iter()
            .all(|e| e.region.as_ref().unwrap().value == "meadow")
    );

    // Tree(placement=Uniform(count_per_square=0.25), n_attractors=32,
    //      foliage_clusters=True, leaf_length_mm=4.5, leaf_width_mm=3.0,
    //      debug_attractors=False)
    let tree = entry(t, "tree");
    assert!((num(tree.setting("per-square")) - 0.25).abs() < 1e-12);
    assert!((num(tree.setting("tips")) - 32.0).abs() < 1e-12);
    assert_eq!(tree.setting("foliage"), Some(&Value::Bool(true)));
    assert!((num(tree.group_setting("leaf", "length")) - 4.5).abs() < 1e-12);
    assert!((num(tree.group_setting("leaf", "width")) - 3.0).abs() < 1e-12);
    // `/- debug …` is slashdashed: commented out, so absent.
    assert!(tree.groups.iter().all(|g| g.name.value != "debug"));
}

#[test]
fn wall_corner_keeps_every_control() {
    let doc = decode_ok(WALL_CORNER);
    let t = tile(&doc);
    // SurfaceConfig(seed=64): one square by default.
    assert_eq!(
        t.kind,
        DefKind::Tile {
            cols: 1,
            rows: 1,
            seed: Some(64)
        }
    );
    // FloodFill(0.5, 0.35)
    let fill = &t.regions[0].fills[0];
    assert!(close(
        (
            fill.0.sample(&none, 0.0).unwrap(),
            fill.1.sample(&none, 0.0).unwrap()
        ),
        (0.5, 0.35)
    ));
    // spine=[(1.0, 1.0), (0.0, 1.0), (0.0, 0.0)]
    let spine: Vec<(f64, f64)> = t.lines[0]
        .through
        .iter()
        .map(|(x, y)| (x.sample(&none, 0.0).unwrap(), y.sample(&none, 0.0).unwrap()))
        .collect();
    assert_eq!(spine, [(1.0, 1.0), (0.0, 1.0), (0.0, 0.0)]);

    // SoilCarpet, GrassCarpet, FieldstoneWall, Grass
    let order: Vec<_> = t.stack.iter().map(|e| e.what.value.as_str()).collect();
    assert_eq!(order, ["dirt", "wall", "lawn"]);

    // FieldstoneWall(spine=…, seed=14) and its defaults
    let wall = entry(t, "wall");
    assert_eq!(word(wall.setting("along")), "spine");
    assert_eq!(word(wall.setting("family")), "fieldstone");
    assert_eq!(wall.seed, Some(14));
    let heights = |n: &str| (n == "wall").then_some(1.0);
    assert!(close(range_env(wall.setting("top"), &heights), (1.0, 1.0))); // top_mm 33.1
    assert!((num(wall.setting("thickness")) * 35.0 - 7.0).abs() < 1e-9); // thickness_mm 7.0
    assert_eq!(word(wall.setting("coping")), "perched"); // coping=None
    assert!(close(
        range(wall.group_setting("stones", "course")),
        (2.2, 5.2)
    ));
    assert!(close(
        range(wall.group_setting("stones", "bay")),
        (5.0, 16.0)
    ));
    assert!(close(
        range(wall.group_setting("stones", "joint")),
        (0.0, 0.0)
    ));
    assert!(close(
        range(wall.group_setting("stones", "reveal")),
        (2.8, 2.8)
    ));
    assert!(close(
        range(wall.group_setting("stones", "roundover")),
        (1.3, 2.6)
    ));
}

#[test]
fn cliff_run_keeps_every_control() {
    let doc = decode_ok(CLIFF_RUN);
    let t = tile(&doc);
    assert_eq!(
        t.kind,
        DefKind::Tile {
            cols: 2,
            rows: 1,
            seed: Some(1)
        }
    );
    assert!(
        t.about
            .as_deref()
            .unwrap()
            .starts_with("Rock band against the north edge.")
    );
    // FOOT_Z_MM = 4.0
    assert_eq!(t.heights[0].name.value, "foot");
    let foot_h = num(Some(&t.heights[0].value.value));
    assert!((foot_h * 33.1 - 4.0).abs() < 0.01);

    let order: Vec<_> = t.stack.iter().map(|e| e.what.value.as_str()).collect();
    assert_eq!(order, ["dirt", "cliff", "scree"]);

    // APRON_H_MM (1.5, 3.5), APRON_R_MM (3.0, 6.5)
    let dirt = entry(t, "dirt");
    assert_eq!(word(dirt.group_setting("bank", "near")), "cliff");
    assert!(close(
        range(dirt.group_setting("bank", "height")),
        (1.5, 3.5)
    ));
    assert!(close(
        range(dirt.group_setting("bank", "reach")),
        (3.0, 6.5)
    ));

    // FORMS['run'] = dict(kind='band', walls=('back',), height=35.0, cap=True, mass=1.35)
    let cliff = entry(t, "cliff");
    assert_eq!(word(cliff.setting("along")), "back");
    assert_eq!(word(cliff.setting("form")), "run");
    // 1.057 h is 34.99 mm: written to 3 places, within a tenth of a layer.
    assert!((num(cliff.setting("height")) * 33.1 - 35.0).abs() < 0.02);
    assert!((num(cliff.setting("mass")) - 1.35).abs() < 1e-12);
    assert_eq!(cliff.setting("dirt-on-tops"), Some(&Value::Bool(true)));
    // KINDS probabilities
    for (k, p) in [
        ("column", 0.38),
        ("block", 0.38),
        ("slab", 0.12),
        ("odd", 0.06),
        ("monolith", 0.06),
    ] {
        assert!(
            (num(cliff.group_setting("stones", k)) - p).abs() < 1e-12,
            "{k}"
        );
    }
    // ZONES_PER_SQUARE 1.5, ZONE_SIGMA_MM (5, 10)
    assert!((num(cliff.group_setting("shatter", "zones")) - 1.5).abs() < 1e-12);
    assert!(close(
        range(cliff.group_setting("shatter", "width")),
        (5.0, 10.0)
    ));

    let scree = entry(t, "scree");
    assert_eq!(word(scree.setting("near")), "cliff");
    // SCREE_PER_SQUARE 55, SCREE_D_MM (1, 6) ** SCREE_D_EXP 2, SCREE_REACH_MM 4
    assert!((num(scree.group_setting("stones", "per-square")) - 55.0).abs() < 1e-12);
    let size = match scree.group_setting("stones", "size") {
        Some(Value::Expr(x)) => x.clone(),
        other => panic!("{other:?}"),
    };
    assert!((size.sample(&none, 0.5).unwrap() - (1.0 + 5.0 * 0.25)).abs() < 1e-12);
    assert!((num(scree.group_setting("stones", "reach")) - 4.0).abs() < 1e-12);
    // SCREE_BLOCKS 2.5, SCREE_BLOCK_MM (4.5, 8)
    assert!((num(scree.group_setting("blocks", "per-square")) - 2.5).abs() < 1e-12);
    assert!(close(
        range(scree.group_setting("blocks", "size")),
        (4.5, 8.0)
    ));
    // GRIT_PER_SQUARE 45, GRIT_D_MM (1, 2.2), GRIT_BURY (0.2, 0.5)
    assert!((num(scree.group_setting("grit", "per-square")) - 45.0).abs() < 1e-12);
    assert!(close(
        range(scree.group_setting("grit", "size")),
        (1.0, 2.2)
    ));
    assert!(close(
        range(scree.group_setting("grit", "sink")),
        (0.2, 0.5)
    ));
    // SCREE_FAMILIES
    for (k, p) in [("block", 0.45), ("wedge", 0.35), ("slab", 0.2)] {
        assert!(
            (num(scree.group_setting("kinds", k)) - p).abs() < 1e-12,
            "{k}"
        );
    }
}

#[test]
fn spans_point_at_the_text() {
    let doc = decode_ok(CLIFF_RUN);
    let t = tile(&doc);
    let cliff = entry(t, "cliff");
    let text = CLIFF_RUN.1;
    let s = cliff.what.span;
    assert_eq!(&text[s.offset()..s.offset() + s.len()], "cliff");
    let h = cliff
        .settings
        .iter()
        .find(|s| s.name.value == "height")
        .unwrap();
    let v = h.value.span;
    assert_eq!(&text[v.offset()..v.offset() + v.len()], "1.057");
}
