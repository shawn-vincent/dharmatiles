//! Square at the seam: a line is one smooth curve, square to the tile's side
//! at an end on a side and along the bisector at a corner; a point on a side
//! is on a seam; the words that name a seam come from the edge contract.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

mod common;

use dt_eval::{Diag, Library, Options, Tile, check};

fn tile(size: &str, body: &str) -> Result<Tile, Vec<Diag>> {
    let text = format!(
        "dharmatiles version=\"1.0.0\"\ntile size=\"{size}\" seed=1 {{\n{body}\n stack {{\n base\n dirt\n }}\n}}\n"
    );
    let src = common::memory(&[("t.dtile", text.as_str())]);
    let both = src;
    check(&both, "t.dtile", &Options::default()).map(|c| c.tile.unwrap())
}

fn ok(size: &str, body: &str) -> Tile {
    tile(size, body).unwrap_or_else(|e| panic!("{e:#?}"))
}

fn codes(e: &[Diag]) -> Vec<&'static str> {
    e.iter().map(|d| d.code).collect()
}

fn unit(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx.hypot(dy);
    [dx / l, dy / l]
}

#[test]
fn a_line_passes_through_every_point_and_leaves_a_side_square() {
    let t = ok(
        "1x1",
        "line a { through \"(0sq, 1sq/3)\" \"(0.5sq, 0.45sq)\" \"(1sq, 2sq/3)\" }",
    );
    let l = &t.lines["a"];
    // the points are on the curve (y north: the page's 1/3 down is 2/3 up)
    for p in [[0.0, 2.0 / 3.0], [0.5, 0.55], [1.0, 1.0 / 3.0]] {
        assert!(
            l.iter()
                .any(|q| (q[0] - p[0]).abs() < 1e-12 && (q[1] - p[1]).abs() < 1e-12),
            "{p:?}"
        );
    }
    // the first run leaves due east, the last arrives due east
    let d0 = unit(l[0], l[1]);
    assert!((d0[0] - 1.0).abs() < 1e-4 && d0[1].abs() < 1e-2, "{d0:?}");
    let n = l.len();
    let d1 = unit(l[n - 2], l[n - 1]);
    assert!((d1[0] - 1.0).abs() < 1e-4 && d1[1].abs() < 1e-2, "{d1:?}");
}

#[test]
fn an_end_on_a_corner_leaves_along_its_bisector() {
    let t = ok("1x1", "line a { through nw \"(0.5sq, 0.5sq)\" }");
    let l = &t.lines["a"];
    let d = unit(l[0], l[1]);
    // north-west corner, going in: south-east, 45 degrees to both sides
    assert!(
        (d[0] - 0.5f64.sqrt()).abs() < 1e-3 && (d[1] + 0.5f64.sqrt()).abs() < 1e-3,
        "{d:?}"
    );
}

#[test]
fn curve_straight_is_the_polyline_exactly() {
    let t = ok(
        "1x1",
        "line a curve=straight { through \"(0sq, 1sq/3)\" \"(0.5sq, 0.45sq)\" \"(1sq, 2sq/3)\" }",
    );
    assert_eq!(
        t.lines["a"],
        vec![[0.0, 1.0 - 1.0 / 3.0], [0.5, 0.55], [1.0, 1.0 - 2.0 / 3.0]]
    );
}

#[test]
fn a_knee_a_heading_and_ends_free_are_asked_for_on_a_through() {
    // corner=#true keeps a sharp knee: the curve turns by the whole angle at the point
    let t = ok(
        "1x1",
        "line a {\n through \"(0sq, 0.5sq)\"\n through \"(0.5sq, 0.2sq)\" corner=#true\n through \"(1sq, 0.5sq)\"\n }",
    );
    let l = &t.lines["a"];
    let i = l.iter().position(|p| (p[0] - 0.5).abs() < 1e-12).unwrap();
    let (a, b) = (unit(l[i - 1], l[i]), unit(l[i], l[i + 1]));
    assert!(a[0] * b[0] + a[1] * b[1] < 0.95, "{a:?} {b:?}");
    // a heading on an end leaves that way instead of square: 90deg is south
    let t = ok(
        "1x1",
        "line a {\n through \"(0sq, 0.5sq)\" heading=\"90deg\"\n through \"(1sq, 0.5sq)\"\n }",
    );
    let l = &t.lines["a"];
    let d = unit(l[0], l[1]);
    assert!(d[1] < -0.99, "{d:?}");
    // ends=free: the end takes its direction from its neighbours, not the side
    let t = ok(
        "1x1",
        "line a ends=free { through \"(0sq, 0.5sq)\" \"(0.5sq, 0.2sq)\" \"(1sq, 0.5sq)\" }",
    );
    let d = unit(t.lines["a"][0], t.lines["a"][1]);
    assert!(d[0] < 0.99, "{d:?}");
}

#[test]
fn heading_and_pull_belong_on_one_point() {
    let e = tile(
        "1x1",
        "line a { through \"(0sq, 0.5sq)\" \"(1sq, 0.5sq)\" heading=\"30deg\" }",
    )
    .unwrap_err();
    assert!(e.iter().any(|d| d.code == "dt::line"), "{e:#?}");
}

#[test]
fn two_mirrored_tiles_meet_in_the_same_direction_at_a_seam() {
    let a = ok(
        "1x1",
        "line a { through \"(1sq, 1sq/3)\" \"(0.5sq, 0.7sq)\" }",
    );
    let b = ok(
        "1x1",
        "line a { through \"(0sq, 1sq/3)\" \"(0.5sq, 0.7sq)\" }",
    );
    let l = &a.lines["a"];
    let m = &b.lines["a"];
    // the west tile's line leaves its east side due west; the east tile's leaves its west side due east
    let da = unit(l[0], l[1]);
    let db = unit(m[0], m[1]);
    assert!((da[0] + 1.0).abs() < 1e-3 && (db[0] - 1.0).abs() < 1e-3);
    assert_eq!(l[0][1], m[0][1]);
}

#[test]
fn a_point_on_a_side_is_on_a_seam_or_it_is_dt_seam_with_a_fix() {
    let e = tile(
        "1x1",
        "line a { through \"(0sq, 0.45sq)\" \"(1sq, 0.5sq)\" }",
    )
    .unwrap_err();
    assert_eq!(codes(&e), ["dt::seam"]);
    assert_eq!(
        e[0].message,
        "column 1, row 1, west side, at 0.45 is not a seam"
    );
    let fix = e[0].fix.as_ref().unwrap();
    assert_eq!(fix.replacement, "\"w\"");
    // a second column of a 2x1: the fix names the square
    let e = tile(
        "2x1",
        "line a { through \"(1.45sq, 0sq)\" \"(1.5sq, 1sq)\" }",
    )
    .unwrap_err();
    assert_eq!(
        e[0].message,
        "column 2, row 1, north side, at 0.45 is not a seam"
    );
    assert_eq!(e[0].fix.as_ref().unwrap().replacement, "\"n(2)\"");
    // the point where two squares meet is not a seam
    let e = tile("2x1", "line a { through \"(1sq, 0sq)\" \"(1.5sq, 1sq)\" }").unwrap_err();
    assert_eq!(codes(&e), ["dt::seam"]);
    // thirds are exact; 0.33 is not a third
    ok(
        "1x1",
        "line a { through \"(0sq, 1sq/3)\" \"(1sq, 2sq/3)\" }",
    );
    let e = tile(
        "1x1",
        "line a { through \"(0sq, 0.33sq)\" \"(1sq, 2sq/3)\" }",
    )
    .unwrap_err();
    assert_eq!(codes(&e), ["dt::seam"]);
    // the tile's corners are seams; a tile may opt out
    ok("1x1", "line a { through \"(0sq, 0sq)\" \"(1sq, 1sq)\" }");
    let text = "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" seed=1 seams=free {\n line a { through \"(0sq, 0.45sq)\" \"(1sq, 0.5sq)\" }\n stack {\n base\n dirt\n }\n}\n";
    let src = common::memory(&[("t.dtile", text)]);
    check(&src, "t.dtile", &Options::default()).unwrap();
}

#[test]
fn a_point_near_a_side_and_a_range_on_one_are_refused() {
    let e = tile(
        "1x1",
        "line a { through \"(0.02sq, 0.5sq)\" \"(1sq, 0.5sq)\" }",
    )
    .unwrap_err();
    assert_eq!(codes(&e), ["dt::near-edge"]);
    assert_eq!(e[0].fix.as_ref().unwrap().replacement, "\"(0sq, 0.5sq)\"");
    let e = tile(
        "1x1",
        "line a { through \"(0sq, 0.4sq .. 0.6sq)\" \"(1sq, 0.5sq)\" }",
    )
    .unwrap_err();
    assert_eq!(codes(&e), ["dt::edge-range"]);
}

#[test]
fn seam_words_name_the_places_and_say_what_is_wrong() {
    // a side one square long: the word stands alone, with or without its number
    let a = ok("1x1", "line a { through nnw \"ssw(1)\" }");
    let b = ok("1x1", "line a { through \"nnw(1)\" ssw }");
    assert_eq!(a.lines["a"], b.lines["a"]);
    // nnw is a third along the north side from the west; the page's y is down
    assert!((a.lines["a"][0][0] - 1.0 / 3.0).abs() < 1e-12 && a.lines["a"][0][1] == 1.0);
    // the corner words are the tile's corners and take no number
    ok("2x2", "line a { through nw se }");
    let e = tile("2x2", "line a { through \"nw(1)\" se }").unwrap_err();
    assert!(e.iter().any(|d| d.code == "dt::seam"), "{e:#?}");
    // a longer side needs its square: the bare word is refused and the help lists the forms
    let e = tile("2x1", "line a { through n s }").unwrap_err();
    assert!(
        e[0].help.as_deref().unwrap().contains("\"n(1)\", \"n(2)\""),
        "{e:#?}"
    );
    let t = ok("2x1", "line a { through \"n(2)\" \"s(1)\" }");
    assert!((t.lines["a"][0][0] - 1.5).abs() < 1e-12);
    // an unknown word with a near one is offered as a fix
    let e = tile("1x1", "line a { through nnww s }").unwrap_err();
    assert_eq!(e[0].fix.as_ref().unwrap().replacement, "nnw");
}

#[test]
fn the_seam_table_is_the_contracts_and_closed_under_turning() {
    let r = dt_eval::doc::read(&Library, dt_eval::doc::EDGES).unwrap();
    let table = r.file.doc.seams.as_ref().unwrap();
    assert!(table.closed().is_ok());
    assert_eq!(table.positions().len(), 3);
    // an open table is refused where it is written
    let text = "dharmatiles version=\"1.0.0\"\nseams {\n seam \"1/3\" north=a south=b west=c east=d\n corner north-west=nw north-east=ne south-east=se south-west=sw\n}\n";
    let e = dt_eval::doc::read(&common::memory(&[("e.dtile", text)]), "e.dtile").unwrap_err();
    assert!(e.iter().any(|d| d.code == "dt::seams"), "{e:#?}");
}

#[test]
fn the_edge_words_still_mean_the_middle_of_their_side() {
    let t = ok("1x1", "line a { through west east }");
    let l = &t.lines["a"];
    assert_eq!((l[0], l[l.len() - 1]), ([0.0, 0.5], [1.0, 0.5]));
    ok("1x1", "line a { through north south }");
}
