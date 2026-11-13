//! Format 1's region tree: every thing makes its region, named regions get
//! parents by their fill points, and `check` refuses a `parent(…)` that
//! reaches past the tile.

#![allow(clippy::unwrap_used)]

mod common;

use dt_eval::regions::{Kind, RegionTree};
use dt_eval::{Checked, Diag, Memory, Options, check};

/// These tests write their documents in format 1 and read them from memory.
fn legacy() -> Options {
    Options::default()
}

fn preview() -> Options {
    Options { ..legacy() }
}

fn mem(text: &str) -> Memory {
    common::memory(&[("t.dtile", text)])
}

fn tile1(size: &str, body: &str) -> String {
    format!("dharmatiles version=1\ntile size=\"{size}\" seed=3 {{\n{body}\n}}\n")
}

fn ok(text: &str) -> Checked {
    check(&mem(text), "t.dtile", &preview()).unwrap_or_else(|e| panic!("{text}\n{e:#?}"))
}

fn refused(text: &str) -> Vec<Diag> {
    match check(&mem(text), "t.dtile", &preview()) {
        Ok(_) => panic!("expected a refusal:\n{text}"),
        Err(e) => e,
    }
}

fn tree(c: &Checked) -> &RegionTree {
    c.regions
        .as_ref()
        .expect("a format 1 tile has a region tree")
}

/// The node called `label`.
fn node(t: &RegionTree, label: &str) -> usize {
    t.nodes
        .iter()
        .position(|n| n.label == label)
        .unwrap_or_else(|| {
            panic!(
                "no region `{label}` in {:?}",
                t.nodes.iter().map(|n| &n.label).collect::<Vec<_>>()
            )
        })
}

fn parent_label(t: &RegionTree, id: usize) -> Option<&str> {
    t.parent(id).map(|p| t.nodes[p].label.as_str())
}

const POOL_AND_BANK: &str = r#"
    line shore wobble=0 { through "(0sq, 0.5sq)" "(1sq, 0.5sq)" }
    region pool { boundedBy shore; fill "(0.5sq, 0.8sq)" }
    region bank { boundedBy shore; fill "(0.5sq, 0.2sq)" }
"#;

#[test]
fn the_tile_is_the_root_and_has_no_parent() {
    let c = ok(&tile1("1x1", "stack { base\n dirt }"));
    let t = tree(&c);
    assert_eq!(t.nodes[RegionTree::TILE].label, "$tile");
    assert_eq!(t.nodes[RegionTree::TILE].kind, Kind::Tile);
    assert_eq!(t.parent(RegionTree::TILE), None);
    assert_eq!(t.parents(RegionTree::TILE), 0);
    // a body with no at= acts in the tile
    assert_eq!(t.of_entry[0], RegionTree::TILE);
    let bb = t.bbox(RegionTree::TILE).unwrap();
    assert_eq!((bb.x0, bb.y0, bb.x1, bb.y1), (0.0, 0.0, 1.0, 1.0));
}

#[test]
fn a_pool_and_its_bank_are_siblings_and_a_body_acts_in_its_region() {
    let c = ok(&tile1(
        "1x1",
        &format!("{POOL_AND_BANK}\n stack {{ base\n dirt at=bank\n water at=pool }}"),
    ));
    let t = tree(&c);
    let (pool, bank) = (node(t, "pool"), node(t, "bank"));
    assert_eq!(t.parent(pool), Some(RegionTree::TILE));
    assert_eq!(t.parent(bank), Some(RegionTree::TILE));
    assert_eq!(t.parents(pool), 1);
    let masks: Vec<_> = [pool, bank].iter().map(|&i| t.mask(i).unwrap()).collect();
    assert!(masks[0].count() > 0 && masks[1].count() > 0);
    assert!(t.contains(pool, [0.5, 0.2]) && !t.contains(pool, [0.5, 0.8]));
    assert!(t.contains(bank, [0.5, 0.8]));
    // bounding boxes (the geometry's, from the south-west corner): the pool
    // is the south half, the bank the north
    let (bp, bb) = (t.bbox(pool).unwrap(), t.bbox(bank).unwrap());
    assert!(bp.y1 <= 0.51 && bb.y0 >= 0.49, "{bp:?} {bb:?}");
    // bodies act in their at= region (entry 0 is the base line's dirt? no:
    // the stack's entries are the built elements)
    let dirt = c
        .tile
        .as_ref()
        .unwrap()
        .stack
        .iter()
        .position(|e| e.what.value == "dirt")
        .unwrap();
    let water = c
        .tile
        .as_ref()
        .unwrap()
        .stack
        .iter()
        .position(|e| e.what.value == "water")
        .unwrap();
    assert_eq!(t.of_entry[dirt], bank);
    assert_eq!(t.of_entry[water], pool);
}

#[test]
fn a_region_inside_another_has_it_as_its_parent() {
    let c = ok(&tile1(
        "2x2",
        r#"
    line outer closed=#true { through "(0.2sq, 1.8sq)" "(1.8sq, 1.8sq)" "(1.8sq, 0.2sq)" "(0.2sq, 0.2sq)" }
    line inner closed=#true { through "(0.8sq, 1.2sq)" "(1.2sq, 1.2sq)" "(1.2sq, 0.8sq)" "(0.8sq, 0.8sq)" }
    region meadow { boundedBy outer; fill "(0.5sq, 1.5sq)" }
    region pond { boundedBy inner; fill "(1sq, 1sq)" }
    stack { base
 dirt at=meadow
 water at=pond }
"#,
    ));
    let t = tree(&c);
    let (meadow, pond) = (node(t, "meadow"), node(t, "pond"));
    assert_eq!(parent_label(t, pond), Some("meadow"), "{:?}", c.explain());
    assert_eq!(parent_label(t, meadow), Some("$tile"));
    assert_eq!(t.parents(pond), 2);
    let ex = c.explain().join("\n");
    assert!(ex.contains("pond: region, parent meadow"), "{ex}");
}

#[test]
fn a_scatter_of_trees_in_a_region_gives_each_tree_a_region_under_the_scatters() {
    let c = ok(&tile1(
        "2x2",
        &format!(
            "{POOL_AND_BANK}\n stack {{ base\n dirt at=bank\n scatter at=bank count=3 {{ tree top=\"$bottom + 35dmm\"\n rock size=\"6dmm..10dmm\" }} }}"
        ),
    ));
    let t = tree(&c);
    let bank = node(t, "bank");
    let tile = c.tile.as_ref().unwrap();
    let sc = tile
        .stack
        .iter()
        .position(|e| e.what.value == "scatter")
        .unwrap();
    // the scatter has a region of its own: its area, under the region it is
    // written in
    let own = t.of_entry[sc];
    assert_eq!(t.nodes[own].kind, Kind::Scatter);
    assert_eq!(t.parent(own), Some(bank));
    assert_eq!(t.nodes[own].mask, t.nodes[bank].mask);
    let kids = &t.of_child[sc];
    assert_eq!(kids.len(), 2);
    for &k in kids {
        assert_eq!(t.parent(k), Some(own));
        assert_eq!(t.nodes[k].kind, Kind::Placed);
        // the scatter, the region, the tile
        assert_eq!(t.parents(k), 3);
    }
    // a tree's disc is its crown diameter (30 mm by default) round it
    let r = t.nodes[kids[0]].radius.unwrap();
    assert!((r - 15.0 / 35.0).abs() < 1e-9, "{r}");
    // a rock's, half its size (the middle of 6..10 is 8)
    let r = t.nodes[kids[1]].radius.unwrap();
    assert!((r - 4.0 / 35.0).abs() < 1e-9, "{r}");
    let ex = c.explain().join("\n");
    assert!(ex.contains("tree (entry"), "{ex}");
    assert!(ex.contains("placed disc"), "{ex}");
}

#[test]
fn a_tree_at_a_point_makes_its_crown_disc_in_the_tile() {
    let c = ok(&tile1(
        "2x2",
        "stack { base\n dirt\n tree at=\"(1sq, 1sq)\" top=\"$bottom + 35dmm\" { crown width=\"20dmm\" } }",
    ));
    let t = tree(&c);
    let tile = c.tile.as_ref().unwrap();
    let i = tile
        .stack
        .iter()
        .position(|e| e.what.value == "tree")
        .unwrap();
    let d = t.of_entry[i];
    assert_eq!(t.nodes[d].kind, Kind::Disc);
    assert_eq!(t.parent(d), Some(RegionTree::TILE));
    assert_eq!(t.parents(d), 1);
    assert!(t.contains(d, [1.0, 1.0]) && !t.contains(d, [1.0, 1.4]));
    let bb = t.bbox(d).unwrap();
    assert!((bb.x0 - (1.0 - 10.0 / 35.0)).abs() < 0.02, "{bb:?}");
    assert!((bb.x1 - (1.0 + 10.0 / 35.0)).abs() < 0.02, "{bb:?}");
}

#[test]
fn a_wall_makes_a_strip_along_its_line() {
    let c = ok(&tile1(
        "2x1",
        "line run { through \"(0.2sq, 0.5sq)\" \"(1.8sq, 0.5sq)\" }\n stack { base\n dirt\n wall at=run thickness=\"7dmm\" }",
    ));
    let t = tree(&c);
    let tile = c.tile.as_ref().unwrap();
    let i = tile
        .stack
        .iter()
        .position(|e| e.what.value == "wall")
        .unwrap();
    let w = t.of_entry[i];
    assert_eq!(t.nodes[w].kind, Kind::Run);
    assert_eq!(t.parent(w), Some(RegionTree::TILE));
    assert!(t.contains(w, [1.0, 0.5]) && !t.contains(w, [1.0, 0.8]));
}

#[test]
fn a_region_used_as_a_number_is_a_type_error() {
    for top in [
        "$region",
        "$tile",
        "parent($region)",
        "$bottom + $region",
        "2 * parent($region)",
    ] {
        let e = refused(&tile1(
            "1x1",
            &format!("stack {{ base\n dirt top=\"{top}\" }}"),
        ));
        assert_eq!(e[0].code, "dt::type", "{top}: {e:#?}");
        assert!(e[0].message.contains("region"), "{top}: {e:#?}");
    }
}

#[test]
fn the_tile_has_no_parent() {
    let e = refused(&tile1("1x1", "stack { base\n dirt top=\"parent($tile)\" }"));
    assert!(
        e.iter().any(|d| d.message.contains("tile has no parent")),
        "{e:#?}"
    );
}

/// The text under a diagnostic's span.
fn pointed(d: &Diag) -> String {
    let (src, span) = d.at.as_ref().unwrap();
    src.inner()[span.offset()..span.offset() + span.len()].to_owned()
}

/// Reads of `formula` (written inside quotes in `text`) for the first entry
/// that is `what`, as the reader would record them, run through the tree's
/// reach check.  A document cannot write a region into a number yet, so the
/// reach is tried on the plumbing directly: step 3's `d(parent($region))`
/// is what makes a document do it.
fn reach(c: &Checked, what: &str, child: Option<usize>, formula: &str) -> Vec<Diag> {
    use std::sync::Arc;
    let text = format!("top=\"{formula}\"");
    let src: dt_eval::diag::Src = Arc::new(miette::NamedSource::new("r.dtile", text));
    let expr = dt_doc::expr::parse_v2(formula).unwrap();
    let tile = c.tile.as_ref().unwrap();
    let entry = tile
        .stack
        .iter()
        .position(|e| e.what.value == what)
        .unwrap();
    let table = Arc::new(dt_doc::consts::ConstTable::root(&[]));
    let read = dt_eval::doc::RegionRead {
        expr: &expr,
        span: miette::SourceSpan::new(5.into(), formula.len() + 1),
        src: &src,
        table: &table,
        entry,
        child,
    };
    c.regions.as_ref().unwrap().parent_flaws_of(&[read])
}

#[test]
fn how_far_parent_reaches_depends_on_where_the_formula_is_called() {
    let c = ok(&tile1(
        "1x1",
        &format!(
            "{POOL_AND_BANK}\n stack {{ base\n dirt\n tree at=\"(0.5sq, 0.5sq)\" top=\"$bottom + 35dmm\"\n scatter at=bank count=2 {{ tree top=\"$bottom + 35dmm\" }} }}"
        ),
    ));
    // a body at tile level: `$region` is the tile
    let e = reach(&c, "dirt", None, "parent($region)");
    assert_eq!(e.len(), 1);
    assert!(e[0].message.contains("tile has no parent"), "{e:#?}");
    assert_eq!(pointed(&e[0]), "parent($region)");
    // a tree at tile level: parent is the tile, the next reaches past it
    assert!(reach(&c, "tree", None, "parent($region)").is_empty());
    let e = reach(&c, "tree", None, "1 + parent(parent($region))");
    assert_eq!(e.len(), 1, "{e:#?}");
    assert_eq!(pointed(&e[0]), "parent(parent($region))");
    // a tree in a scatter in a region has three (the scatter, the region, the tile)
    let e = reach(&c, "scatter", Some(0), "parent(parent(parent($region)))");
    assert!(e.is_empty(), "{e:#?}");
    let e = reach(
        &c,
        "scatter",
        Some(0),
        "parent(parent(parent(parent($region))))",
    );
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(e[0].message.contains("reaches past the tile"), "{e:#?}");
    assert!(e[0].message.contains("4 up, and there are 3"), "{e:#?}");
    // the tile has no parent, wherever it is asked
    let e = reach(&c, "scatter", Some(0), "parent($tile)");
    assert!(
        e.iter().any(|d| d.message.contains("tile has no parent")),
        "{e:#?}"
    );
}

#[test]
fn the_reader_records_each_formula_with_the_thing_that_calls_it() {
    let c = ok(&tile1(
        "1x1",
        &format!(
            "{POOL_AND_BANK}\n stack {{ base\n dirt at=bank top=\"$groundHeight\"\n tree at=\"(0.5sq, 0.5sq)\" top=\"$bottom + 35dmm\"\n scatter at=bank count=2 {{ rock\n tree top=\"$bottom + 35dmm\" }} }}"
        ),
    ));
    let tile = c.tile.as_ref().unwrap();
    let reads = tile.region_reads();
    let owners: Vec<(&str, Option<usize>)> = reads
        .iter()
        .map(|r| (tile.stack[r.entry].what.value.as_str(), r.child))
        .collect();
    assert!(owners.contains(&("dirt", None)), "{owners:?}");
    assert!(owners.contains(&("tree", None)), "{owners:?}");
    // the scatter's tree is its second thing
    assert!(owners.contains(&("scatter", Some(1))), "{owners:?}");
}

#[test]
fn a_region_name_cannot_be_declared_and_d_and_a_are_fractions() {
    for line in ["let $tile 5", "override $region 5"] {
        let e = refused(&tile1("1x1", &format!("{line}\n stack {{ base\n dirt }}")));
        assert_eq!(e[0].code, "dt::name", "{line}: {e:#?}");
        assert!(
            e[0].message.contains("set by the element"),
            "{line}: {e:#?}"
        );
    }
    // `$d` and `$a` are set by a body now: a fraction, so not a height
    let e = refused(&tile1("1x1", "stack { base\n dirt top=\"$d\" }"));
    assert_eq!(e[0].code, "dt::type", "{e:#?}");
    ok(&tile1(
        "1x1",
        "stack { base\n dirt top=\"$groundHeight + 4dmm * $d\" }",
    ));
}

#[test]
fn a_let_may_hold_a_region_and_is_a_type_error_where_a_number_is_read() {
    ok(&tile1(
        "1x1",
        "let $up \"parent($region)\"\n stack { base\n dirt }",
    ));
    let e = refused(&tile1(
        "1x1",
        "let $up \"parent($region)\"\n stack { base\n dirt top=\"$bottom + $up\" }",
    ));
    assert_eq!(e[0].code, "dt::type", "{e:#?}");
    let e = refused(&tile1(
        "1x1",
        "stack { base\n scatter count=2 { rock size=\"$region\" } }",
    ));
    assert_eq!(e[0].code, "dt::type", "{e:#?}");
}

#[test]
fn regions_that_name_each_other_are_an_error_at_the_region_whether_or_not_anything_uses_them() {
    // nothing uses `a` or `b`: the loop is still said, where each is written
    let text = tile1(
        "1x1",
        "region a { except b }\n region b { except a }\n stack { base\n dirt }",
    );
    let e = refused(&text);
    let d = e
        .iter()
        .find(|d| d.code == "dt::region")
        .unwrap_or_else(|| panic!("no dt::region in {e:#?}"));
    assert!(d.at.is_some(), "{d:?}");
    assert!(d.message.contains("names itself"), "{d:?}");
}
