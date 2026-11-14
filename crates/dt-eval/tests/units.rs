//! Format 1's coordinates and units (step 3a): rows and columns from the
//! north-west, points in parentheses, `x y d a`, scatter regions, kinds and
//! units checked before any build, and format 1 tiles written as the mirror
//! of format 0 ones building the same mesh.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

mod common;

use dt_eval::coords::Coords;
use dt_eval::regions::{Kind, RegionTree};
use dt_eval::{Built, Checked, Diag, Memory, Options, build, check};

fn preview() -> Options {
    Options {
        ..Options::default()
    }
}

fn mem(text: &str) -> Memory {
    Memory([("t.dtile".to_owned(), text.to_owned())].into())
}

fn tile1(size: &str, body: &str) -> String {
    format!("dharmatiles version=1\ntile size=\"{size}\" seed=3 seams=free {{\n{body}\n}}\n")
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
    c.regions.as_ref().unwrap()
}

#[test]
fn a_scatter_is_a_region_of_its_own_and_its_things_sit_in_it() {
    // at tile level: a tree has the scatter as its parent, the tile above that
    let c = ok(&tile1(
        "1x1",
        "stack { base\n dirt\n scatter count=2 { tree top=\"$bottom + 35dmm\" } }",
    ));
    let t = tree(&c);
    let sc = c
        .tile
        .as_ref()
        .unwrap()
        .stack
        .iter()
        .position(|e| e.what.value == "scatter")
        .unwrap();
    let own = t.of_entry[sc];
    assert_eq!(t.nodes[own].kind, Kind::Scatter);
    assert_eq!(t.parent(own), Some(RegionTree::TILE));
    let kid = t.of_child[sc][0];
    assert_eq!(t.parent(kid), Some(own));
    assert_eq!(t.parent(t.parent(kid).unwrap()), Some(RegionTree::TILE));
    assert_eq!(t.parents(kid), 2);
    // the scatter covers the whole tile
    assert!(t.contains(own, [0.1, 0.9]) && t.contains(own, [0.9, 0.1]));
}

#[test]
fn rows_and_columns_start_at_the_north_west() {
    let c = ok(&tile1(
        "2x1",
        r#"
    line run curve=straight { through "(0.25sq, 0.25sq)" "(1.75sq, 0.75sq)" }
    region top { boundedBy run; fill "(0.5sq, 0.1sq)" }
    stack { base
 dirt }
"#,
    ));
    let t = c.tile.as_ref().unwrap();
    // geometry keeps its frame: y runs north from the south-west corner
    assert_eq!(t.lines["run"], vec![[0.25, 0.75], [1.75, 0.25]]);
    assert_eq!(t.regions[0].fills, vec![[0.5, 0.9]]);
    // named edges and corners mean what they did
    let c = ok(&tile1(
        "1x1",
        "line run curve=straight { through south \"(0.5sq, 0.5sq)\" north }\n stack { base\n dirt }",
    ));
    assert_eq!(
        c.tile.as_ref().unwrap().lines["run"],
        vec![[0.5, 0.0], [0.5, 0.5], [0.5, 1.0]]
    );
    // a point off the tile is refused where it is written
    let e = refused(&tile1(
        "1x1",
        "line run { through south \"(0.5sq, 1.5sq)\" }\n stack { base\n dirt }",
    ));
    assert!(e.iter().any(|d| d.code == dt_lang::codes::POINT), "{e:#?}");
}

fn hashes(b: &Built) -> String {
    b.report
        .stages
        .iter()
        .map(|s| format!("{:016x}  {}\n", s.hash, s.name))
        .collect()
}

fn built(text: &str, opts: &Options) -> Built {
    build(&mem(text), "t.dtile", opts).unwrap_or_else(|e| panic!("{text}\n{e:#?}"))
}

/// The same tile written two ways, the second its mirror (`y` flipped,
/// every size in `dmm`), must build the same mesh: the frame and `dmm` are
/// exact.
fn same_mesh(zero: &str, one: &str) {
    let a = build(
        &common::memory(&[("t.dtile", zero)]),
        "t.dtile",
        &Options::default(),
    )
    .unwrap_or_else(|e| panic!("{zero}\n{e:#?}"));
    let b = built(one, &preview());
    assert!(!a.report.stages.is_empty());
    assert_eq!(hashes(&a), hashes(&b), "{zero}\n{one}");
}

#[test]
fn a_pond_written_as_its_mirror_builds_the_same_mesh() {
    // dyadic coordinates, so `rows - y` is exact
    same_mesh(
        r#"dharmatiles version="1.0.0"; tile size="1x1" seed=50 {
    line shore wobble="5dmm" { through "(0sq, 0.5sq)" "(1sq, 0.5sq)" }
    region pool { boundedBy shore; fill "(0.5sq, 0.75sq)" }
    region shoreBand { within "5dmm" of=shore }
    stack {
        base
        dirt {
            pin top="[$waterBedHeight .. $groundHeight + 1dmm]" at=shoreBand
            pin top="$waterBedHeight" at=pool
        }
        scatter count=2 at=pool { rock size="6dmm .. 10dmm" }
        water at=shoreBand
    }
}
"#,
        r#"
dharmatiles version=1
tile size="1x1" seed=50 {
    line shore wobble="5dmm" { through "(0sq, 0.5sq)" "(1sq, 0.5sq)" }
    region pool { boundedBy shore; fill "(0.5sq, 0.75sq)" }
    region shoreBand { within "5dmm" of=shore }
    stack {
        base
        dirt {
            pin top="[$waterBedHeight .. $groundHeight + 1dmm]" at=shoreBand
            pin top=$waterBedHeight at=pool
        }
        scatter count=2 at=pool { rock size="6dmm..10dmm" }
        water at=shoreBand
    }
}
"#,
    );
}

#[test]
fn a_hill_in_dmm_and_mm_builds_the_same_mesh() {
    // a printed millimetre is a design millimetre on DungeonBlocks (35 mm)
    same_mesh(
        r#"dharmatiles version="1.0.0"; tile size="2x2" seed=11 {
    stack {
        base
        dirt bumpSize="40dmm" top="[16dmm .. 16dmm + 2dmm]" {
            scatter perSquare=6 { pebble size="1dmm .. 2.2dmm" }
        }
    }
}
"#,
        r#"
dharmatiles version=1
tile size="2x2" seed=11 {
    stack {
        base
        dirt bumpSize="40dmm" top="[16dmm .. 16dmm + 2mm]" {
            scatter perSquare=6 { pebble size="1dmm..2.2mm" }
        }
    }
}
"#,
    );
}

#[test]
fn points_pins_and_a_slope_written_as_their_mirror_build_the_same_mesh() {
    same_mesh(
        r#"dharmatiles version="1.0.0"; tile size="2x1" seed=9 {
    point knoll "(0.5sq, 0.75sq)"
    stack {
        base
        dirt {
            slope "30deg"
            pin top="8dmm" at=knoll
            pin top="9dmm" at="(1.5sq, 0.25sq)"
        }
    }
}
"#,
        r#"
dharmatiles version=1
tile size="2x1" seed=9 {
    point knoll "(0.5sq, 0.75sq)"
    stack {
        base
        dirt {
            slope "30deg"
            pin top="8dmm" at=knoll
            pin top="9dmm" at="point($tile, 75%, 0.25 * 1sq)"
        }
    }
}
"#,
    );
}

/// The node at `(x, y)` squares from the south-west corner.
fn node(c: &Checked, x: f64, y: f64) -> usize {
    let frame = tree(c).frame;
    let per = f64::from(frame.cells_per_square);
    let (nx, _) = frame.nodes();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (i, j) = ((x * per).round() as usize, (y * per).round() as usize);
    j * nx + i
}

/// `v` squares as the lattice counts them: to the nearest node.
fn snap(c: &Checked, v: f64) -> f64 {
    let per = f64::from(tree(c).frame.cells_per_square);
    (v * per).round() / per
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn coordinates_count_from_the_north_west_in_any_unit() {
    use dt_doc::expr::UnitWord::{Dmm, In, Mm, Pct, Sq};
    let c = ok(&tile1("2x1", "stack { base\n dirt }"));
    let co = Coords::new(tree(&c), 35.0, 0.2);
    let t = RegionTree::TILE;
    let at = node(&c, 1.5, 1.0 / 6.0);
    let south = snap(&c, 1.0 / 6.0);
    assert!(close(co.x(t, at, Sq), 1.5));
    assert!(close(co.x(t, at, Mm), 52.5));
    assert!(close(co.x(t, at, Dmm), 52.5));
    assert!(close(co.x(t, at, In), 52.5 / 25.4));
    assert!(close(co.x(t, at, Pct), 0.75));
    // y runs south from the north edge: a sixth up from the south is five sixths down
    assert!(close(co.y(t, at, Sq), 1.0 - south));
    assert!(close(co.y(t, at, Pct), 1.0 - south));
    // OpenLOCK's square: mm does not scale, sq and dmm do
    let ol = Coords::new(tree(&c), 25.4, 0.2);
    assert!(close(ol.x(t, at, Mm), 1.5 * 25.4));
    assert!(close(ol.x(t, at, Dmm), 52.5));
    assert!(close(ol.x(t, at, Sq), 1.5));
}

#[test]
fn depth_is_zero_at_the_deepest_and_one_at_the_edge_and_a_turn_goes_round_it() {
    use dt_doc::expr::UnitWord::{Deg, Mm, Pct, Rad};
    let c = ok(&tile1("2x1", "stack { base\n dirt }"));
    let co = Coords::new(tree(&c), 35.0, 0.2);
    let t = RegionTree::TILE;
    // 2x1: the deepest part is a ridge along the middle; its middle is the centre
    assert!(close(co.d(t, node(&c, 1.0, 0.5), Pct), 0.0));
    assert!(close(co.d(t, node(&c, 0.5, 0.5), Pct), 0.0));
    assert!(close(co.d(t, node(&c, 1.0, 0.0), Pct), 1.0));
    assert!(close(co.d(t, node(&c, 0.0, 0.5), Pct), 1.0));
    // a third of the way in from the south edge: a third of the way to the ridge's 0.5
    assert!(close(
        co.d(t, node(&c, 1.0, 1.0 / 6.0), Pct),
        1.0 - 2.0 * snap(&c, 1.0 / 6.0)
    ));
    // in millimetres from the edge: 0 at the edge, 17.5 mm at the middle
    assert!(close(co.d(t, node(&c, 1.0, 0.0), Mm), 0.0));
    assert!(close(co.d(t, node(&c, 1.0, 0.5), Mm), 17.5));
    // round the middle: east 0, south a quarter, west a half, north three quarters
    let a = |x, y, u| co.a(t, node(&c, x, y), u);
    assert!(close(a(1.5, 0.5, Pct), 0.0));
    assert!(close(a(1.0, 1.0 / 6.0, Pct), 0.25));
    assert!(close(a(0.5, 0.5, Pct), 0.5));
    assert!(close(a(1.0, 5.0 / 6.0, Pct), 0.75));
    assert!(close(a(1.0, 1.0 / 6.0, Deg), 90.0));
    assert!(close(a(1.0, 1.0 / 6.0, Rad), std::f64::consts::FRAC_PI_2));
}

#[test]
fn a_piece_has_a_centre_of_its_own_and_a_thin_one_is_a_cell_deep() {
    use dt_doc::expr::UnitWord::Pct;
    // two separate pieces: a big lake and a puddle beside it
    let c = ok(&tile1(
        "3x1",
        r#"
    line a closed=#true { through "(0.2sq, 0.2sq)" "(1.6sq, 0.2sq)" "(1.6sq, 0.8sq)" "(0.2sq, 0.8sq)" }
    line b closed=#true { through "(2.2sq, 0.4sq)" "(2.6sq, 0.4sq)" "(2.6sq, 0.6sq)" "(2.2sq, 0.6sq)" }
    region lake { boundedBy a; fill "(0.9sq, 0.5sq)" }
    region puddle { boundedBy b; fill "(2.4sq, 0.5sq)" }
    region both { within "0.01dmm" of=lake
                  except lake }
    stack { base
 dirt at=lake
 water at=puddle }
"#,
    ));
    let t = tree(&c);
    let id = |l: &str| t.nodes.iter().position(|n| n.label == l).unwrap();
    let co = Coords::new(t, 35.0, 0.2);
    // each is 0% at its own deepest part
    assert!(co.d(id("lake"), node(&c, 0.9, 0.5), Pct) < 0.1);
    assert!(co.d(id("puddle"), node(&c, 2.4, 0.5), Pct) < 0.1);
    // and 100% at its own edge
    assert!(co.d(id("puddle"), node(&c, 2.22, 0.5), Pct) > 0.5);
}

#[test]
fn a_region_with_a_hole_refuses_a_turn_and_nothing_else() {
    let ring = r#"
    line outer closed=#true { through "(0.2sq, 0.2sq)" "(1.8sq, 0.2sq)" "(1.8sq, 1.8sq)" "(0.2sq, 1.8sq)" }
    line inner closed=#true { through "(0.8sq, 0.8sq)" "(1.2sq, 0.8sq)" "(1.2sq, 1.2sq)" "(0.8sq, 1.2sq)" }
    region ring { boundedBy outer inner; fill "(0.5sq, 0.5sq)" }
"#;
    let e = refused(&tile1(
        "2x2",
        &format!("{ring}\n stack {{ base\n dirt at=ring top=\"$groundHeight + 4dmm * $a\" }}"),
    ));
    assert_eq!(e[0].code, dt_lang::codes::REGION, "{e:#?}");
    assert!(e[0].message.contains("has a hole"), "{e:#?}");
    // pointed at the formula's own `$a`
    let (src, span) = e[0].at.as_ref().unwrap();
    assert_eq!(
        &src.inner()[span.offset()..span.offset() + span.len()],
        "$a"
    );
    // through a call, too, and through a name the formula reads
    let e = refused(&tile1(
        "2x2",
        &format!(
            "{ring}\n let $turn \"a($region, %)\"\n stack {{ base\n dirt at=ring top=\"$groundHeight + 4dmm * $turn\" }}"
        ),
    ));
    assert_eq!(e[0].code, dt_lang::codes::REGION, "{e:#?}");
    // the depth, the position, and the tile's own turn are fine
    ok(&tile1(
        "2x2",
        &format!(
            "{ring}\n stack {{ base\n dirt at=ring top=\"$groundHeight + 4dmm * ($d + x($tile, %) + a($tile, %))\" }}"
        ),
    ));
}

#[test]
fn a_body_works_its_top_out_in_the_new_coordinates() {
    // `$y` counts south: the ground is higher in the south, where `$y` is large
    let c = ok(&tile1(
        "2x2",
        "stack { base\n dirt top=\"$groundHeight + 1dmm * $y\" }",
    ));
    let b = &c.solved.as_ref().unwrap().bodies[0];
    let (south, north) = (b.at(1.0, 0.5), b.at(1.0, 1.5));
    // y = 2 - 0.5 = 1.5 against y = 2 - 1.5 = 0.5: 1 mm higher
    assert!(close(south.lo - north.lo, 1.0), "{south:?} {north:?}");
    // a dome: `$d` is 0 at the deepest and 1 at the edge
    let c = ok(&tile1(
        "2x2",
        "stack { base\n dirt top=\"$groundHeight + 8dmm * (1 - $d)\" }",
    ));
    let b = &c.solved.as_ref().unwrap().bodies[0];
    assert!(close(b.at(1.0, 1.0).lo, 13.0), "{:?}", b.at(1.0, 1.0));
    assert!(b.at(1.0, 1.0).lo > b.at(0.5, 1.0).lo);
    // an angle in radians or degrees makes a wave round the middle
    ok(&tile1(
        "2x2",
        "stack { base\n dirt top=\"$groundHeight + 2dmm * sin(2 pi rad * $a) + 1dmm * cos(360deg * $a)\" }",
    ));
}

#[test]
fn an_element_reads_only_the_names_it_sets() {
    // a tree sets the point names too, at each of its branch tips, and a
    // rock at its middle
    for top in [
        "$bottom + 35dmm * $x",
        "$bottom + 35dmm * x($tile, %)",
        "$bottom + 35dmm * (1 - $d * 0.2) + 1dmm * $a",
    ] {
        ok(&tile1(
            "1x1",
            &format!("stack {{ base\n dirt\n tree at=\"(0.5sq, 0.5sq)\" top=\"{top}\" }}"),
        ));
    }
    ok(&tile1(
        "1x1",
        "stack { base\n dirt\n scatter count=2 { rock top=\"$bottom + 3dmm * $x\" } }",
    ));
    // a rock sets no more than that
    let e = refused(&tile1(
        "1x1",
        "stack { base\n dirt\n scatter count=2 { rock top=\"$bottom + 3dmm * $nope\" } }",
    ));
    assert!(!e.is_empty(), "{e:#?}");
    // a body sets them all
    ok(&tile1(
        "1x1",
        "stack { base\n dirt top=\"$groundHeight + 1dmm * ($x + $y + $d + $a)\" }",
    ));
    // and neither is a size or a count worked out at each point
    let e = refused(&tile1(
        "1x1",
        "stack { base\n scatter count=2 { rock size=\"$d * 1dmm\" } }",
    ));
    assert_eq!(e[0].code, dt_lang::codes::TYPE, "{e:#?}");
}

// ---- points in a named region's frame ----

/// A tile cut across the middle by `shore` (y = 0.5 sq from the north); `bank`
/// is the north half, and `nw` its west half, so `parent($region)` of a body
/// at `nw` is `bank`.
fn banks(pin_at: &str, body_at: &str) -> String {
    tile1(
        "2x2",
        &format!(
            "line shore {{ through \"(0, 1sq)\" \"(2sq, 1sq)\" }}\n line cut {{ through \"(1sq, 0)\" \"(1sq, 1sq)\" }}\n region bank {{ boundedBy shore; fill \"(1sq, 0.5sq)\" }}\n region nw {{ boundedBy shore cut; fill \"(0.5sq, 0.5sq)\" }}\n stack {{ base\n dirt at={body_at} {{\n pin top=\"$groundHeight\" at={pin_at} }} }}"
        ),
    )
}

/// The place the first body's first pin holds once `check` has worked it out.
fn pin_place(text: &str) -> [f64; 2] {
    let c = ok(text);
    let tile = c.tile.as_ref().unwrap();
    match &tile.stack.iter().find(|e| e.is_body()).unwrap().pins[0].at {
        dt_doc::model::PlaceRef::Point(x, y) => [*x, *y],
        other => panic!("a pin still holds `{other}`"),
    }
}

#[test]
fn a_point_in_a_named_regions_frame_is_a_share_of_its_box_from_its_north_west() {
    let c = ok(&banks("\"(50%, 50%)\"", "bank"));
    let t = tree(&c);
    let id = t.nodes.iter().position(|n| n.label == "bank").unwrap();
    let b = t.bbox(id).unwrap();
    let at = pin_place(&banks("\"(50%, 50%)\"", "bank"));
    assert!(close(at[0], b.x0 + 0.5 * (b.x1 - b.x0)), "{at:?} {b:?}");
    assert!(close(at[1], b.y1 - 0.5 * (b.y1 - b.y0)), "{at:?} {b:?}");
    // a length counts from the box's north-west corner, down
    let at = pin_place(&banks("\"(0.25sq, 0.25sq)\"", "bank"));
    assert!(
        close(at[0], b.x0 + 0.25) && close(at[1], b.y1 - 0.25),
        "{at:?} {b:?}"
    );
    // the frame by name: `$region` is the same, `$tile` is the whole tile
    assert_eq!(
        pin_place(&banks("\"point($region, 25%, 50%)\"", "bank")),
        pin_place(&banks("\"(25%, 50%)\"", "bank")),
    );
    let tiled = pin_place(&banks("\"point($tile, 50%, 25%)\"", "bank"));
    assert!(close(tiled[0], 1.0) && close(tiled[1], 1.5), "{tiled:?}");
}

#[test]
fn parent_and_polar_name_other_frames() {
    // `bank` is the north half: x 0 to 2, y 1 to 2 (from the south)
    let up = pin_place(&banks("\"point(parent($region), 25%, 25%)\"", "nw"));
    assert!(
        (up[0] - 0.5).abs() < 0.02 && (up[1] - 1.75).abs() < 0.02,
        "{up:?}"
    );
    // 0% out is the deepest point: the middle of the region
    let mid = pin_place(&banks("\"polar($region, 0%, 0deg)\"", "bank"));
    assert!(
        (mid[0] - 1.0).abs() < 0.1 && (mid[1] - 1.5).abs() < 0.1,
        "{mid:?}"
    );
    // a quarter turn is straight down the page, a share of the depth out
    let down = pin_place(&banks("\"polar($region, 50%, 90deg)\"", "bank"));
    assert!(
        (down[0] - mid[0]).abs() < 0.05 && down[1] < mid[1],
        "{down:?} {mid:?}"
    );
    // round the parent's centre (1, 1.5), half its depth (0.5) out to the west
    let own = pin_place(&banks("\"polar(parent($region), 50%, 180deg)\"", "nw"));
    assert!(
        (own[0] - 0.75).abs() < 0.05 && (own[1] - 1.5).abs() < 0.05,
        "{own:?}"
    );
}

#[test]
fn a_point_outside_the_region_it_sits_in_is_refused_at_check() {
    for at in [
        // the south half is not `bank`
        "\"(50%, 150%)\"",
        "\"point($tile, 50%, 75%)\"",
        "\"point(parent($region), 75%, 25%)\"",
    ] {
        let body = if at.contains("parent") { "nw" } else { "bank" };
        let e = refused(&banks(at, body));
        assert_eq!(e[0].code, dt_lang::codes::POINT, "{at}: {e:#?}");
        assert!(e[0].message.contains("outside the region"), "{at}: {e:#?}");
        assert!(e[0].message.contains("`"), "{at}: {e:#?}");
    }
    // it points at the point, not the body
    let text = banks("\"(50%, 150%)\"", "bank");
    let e = refused(&text);
    let at = e[0].location().unwrap();
    assert_eq!(at.1, 7 + 2, "line of the pin");
    // a point the region covers is fine
    ok(&banks("\"(50%, 50%)\"", "bank"));
    // tile level: the tile itself is the region, and off it is the old error
    let e = refused(&tile1(
        "1x1",
        "stack { base\n dirt {\n pin top=\"$groundHeight\" at=\"(1.5sq, 0)\" } }",
    ));
    assert!(e[0].message.contains("off the tile"), "{e:#?}");
}

#[test]
fn the_tile_has_no_parent_for_a_point_either() {
    let e = refused(&banks(
        "\"point(parent(parent(parent($region))), 25%, 25%)\"",
        "nw",
    ));
    assert!(
        e.iter()
            .any(|d| d.code == dt_lang::codes::POINT && d.message.contains("no parent")),
        "{e:#?}"
    );
    let e = refused(&tile1(
        "1x1",
        "stack { base\n dirt {\n pin top=\"$groundHeight\" at=\"point(parent($region), 0, 0)\" } }",
    ));
    assert!(e.iter().any(|d| d.message.contains("no parent")), "{e:#?}");
}

#[test]
fn a_points_in_a_part_are_worked_out_at_each_call_site() {
    // the same part, called on two tile sizes, puts its rock at the middle of
    // each: a part has no tile of its own
    let at = |size: &str| {
        let text = format!(
            "dharmatiles version=1\ndefine knoll {{\n stack {{ rock at=\"(50%, 25%)\" size=\"8dmm\" }}\n}}\ntile size=\"{size}\" seed=3 seams=free {{\n stack {{ base\n dirt\n knoll }}\n}}\n"
        );
        let c = ok(&text);
        let tile = c.tile.as_ref().unwrap();
        let rock = tile.stack.iter().find(|e| e.what.value == "rock").unwrap();
        let d = rock.at.as_ref().unwrap();
        [d[0].lo(), d[1].lo()]
    };
    assert_eq!(at("2x1"), [1.0, 0.75]);
    assert_eq!(at("1x3"), [0.5, 2.25]);
}
