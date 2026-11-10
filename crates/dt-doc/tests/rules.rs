//! One test (or a few) per language rule: what decodes, and what is refused.
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use common::*;
use dt_doc::expr::Unit;
use dt_doc::model::{Form, Value};
use dt_doc::place::{Along, Corner, Edge, Point, Side};
use dt_doc::schema::{self, Axis, Kind};

fn none(_: &str) -> Option<f64> {
    None
}

/// The first stack entry of a one-tile snippet.
fn first(text: &str) -> dt_doc::model::Entry {
    snippet(text).tile().unwrap().stack[0].clone()
}

fn assert_fails(text: &str, code: &str, want: &str) {
    let (e, shown) = fails(text);
    assert!(e.iter().any(|e| e.code == code), "want {code}:\n{shown}");
    assert!(flat(&shown).contains(want), "want `{want}`:\n{shown}");
}

// ---- mm at the reference, named levels, (h) opt-in ----------

#[test]
fn sizes_are_mm_and_h_is_opt_in() {
    let e = first("tile { stack { rock size=40 } }");
    let n = e.value("size").unwrap().num().unwrap();
    assert_eq!((n.unit, n.bounds(&none).unwrap()), (None, (40.0, 40.0)));
    let e = first("tile { stack { rock size=(h)1.2 } }");
    let n = e.value("size").unwrap().num().unwrap();
    assert_eq!(n.unit, Some(Unit::H));
    assert!((n.bounds(&none).unwrap().0 - 1.2 * schema::H_MM).abs() < 1e-9);
}

#[test]
fn heights_are_named_or_mm_or_expressions() {
    for text in [
        "tile { line s wobble=groundHeight { through \"0,0.5\" \"1,0.5\" }; stack { dirt } }",
        "tile { line s wobble=wallHeight { through \"0,0.5\" \"1,0.5\" }; stack { dirt } }",
        "tile { line s wobble=4 { through \"0,0.5\" \"1,0.5\" }; stack { dirt } }",
        "tile { line s wobble=\"groundHeight - 1\" { through \"0,0.5\" \"1,0.5\" }; stack { dirt } }",
        "tile { let foot 4; line s wobble=foot { through \"0,0.5\" \"1,0.5\" }; stack { dirt } }",
        "tile { let path \"groundHeight - 1\"; line s wobble=path { through \"0,0.5\" \"1,0.5\" }; stack { dirt } }",
        "tile { override groundHeight 6; line s wobble=groundHeight { through \"0,0.5\" \"1,0.5\" }; stack { dirt } }",
    ] {
        snippet(text);
    }
    assert_fails(
        "tile { line s wobble=grundHeight { through \"0,0.5\" \"1,0.5\" }; stack { dirt } }",
        "dt::unknown-name",
        "did you mean `groundHeight`?",
    );
    // A height name is not a size.
    assert_fails(
        "tile { stack { rock size=groundHeight } }",
        "dt::unknown-name",
        "no parameter is called `groundHeight`",
    );
}

#[test]
fn the_old_levels_block_points_at_let() {
    assert_fails(
        "tile { levels foot=4; stack { dirt } }",
        "dt::unknown-node",
        "`levels` is now `let`",
    );
}

// ---- every written size scales; recorded as metadata ---------

#[test]
fn every_control_has_an_axis_that_says_how_it_scales() {
    let rock = schema::element("rock").unwrap();
    assert_eq!(rock.control("size").unwrap().value, Some(Axis::Mm));
    let tree = schema::element("tree").unwrap();
    let leaf = tree.control("leaf").unwrap();
    assert_eq!(leaf.sub("length").unwrap().value, Some(Axis::Mm));
    let crown = tree.control("crown").unwrap();
    assert_eq!(crown.sub("clumps").unwrap().value, Some(Axis::Count));
    let grass = schema::element("grass").unwrap();
    assert_eq!(grass.control("height").unwrap().value, Some(Axis::Mm));
    assert_eq!(
        grass.control("clearings").unwrap().value,
        Some(Axis::PerSquare)
    );
    let e = first("tile { stack { tree { leaf length=4.5 } } }");
    let s = e.control("leaf").unwrap();
    assert_eq!(s.subs[0].control.unwrap().value, Some(Axis::Mm));
}

// ---- both forms, same model ---------------------------------------

#[test]
fn line_and_child_forms_decode_the_same() {
    let a = first("tile { stack { wall at=north thickness=7 } }");
    let b = first("tile { stack { wall at=north { thickness 7 } } }");
    assert_eq!(a.value("thickness"), b.value("thickness"));
    assert_eq!(a.control("thickness").unwrap().form, Form::Property);
    assert_eq!(b.control("thickness").unwrap().form, Form::Child);
    // Words, and place words, too.
    let a = first("tile { stack { wall at=north style=cut-stone } }");
    let b = first("tile { stack { wall { at north; style cut-stone } } }");
    assert_eq!(a.value("style"), b.value("style"));
    assert_eq!(a.place.along.unwrap().value, b.place.along.unwrap().value);
}

#[test]
fn a_control_is_written_once() {
    assert_fails(
        "tile { stack { wall at=north thickness=7 { thickness 8 } } }",
        "dt::duplicate",
        "`thickness` is set twice",
    );
}

#[test]
fn groups_are_child_lines() {
    assert_fails(
        "tile { stack { tree leaf=4 } }",
        "dt::group",
        "`leaf` is a group",
    );
    assert_fails(
        "tile { stack { tree { leaf 4.5 } } }",
        "dt::group",
        "has no value of its own",
    );
    // A control with a value and sub-controls: ledges 1 height="40%".
    let e = first("tile { stack { cliff { ledges 1 height=\"40%\" } } }");
    let l = e.control("ledges").unwrap();
    assert_eq!(
        l.value
            .as_ref()
            .unwrap()
            .value
            .num()
            .unwrap()
            .expr
            .bounds(&none)
            .unwrap(),
        (1.0, 1.0)
    );
    assert_eq!(
        l.sub("height")
            .unwrap()
            .num()
            .unwrap()
            .expr
            .bounds(&none)
            .unwrap(),
        (0.4, 0.4)
    );
    // a group: on the line, or as a child
    first("tile { stack { tree { crown width=30 } } }");
}

// ---- units are not written ----------------------------------------

#[test]
fn mm_annotation_is_an_optional_check() {
    let e = first("tile { stack { tree { leaf length=(mm)4.5 } } }");
    let n = e.sub("leaf", "length").unwrap().num().unwrap();
    assert_eq!(n.unit, Some(Unit::Mm));
    assert_eq!(n.bounds(&none).unwrap(), (4.5, 4.5));
    assert_fails(
        "tile { stack { tree { crown clumps=(mm)30 } } }",
        "dt::unit",
        "(mm) and (h) are for sizes and levels",
    );
    assert_fails(
        "tile { stack { rock size=(cm)4 } }",
        "dt::unit",
        "units are not written",
    );
}

#[test]
fn unit_suffixes_are_refused_with_a_hint() {
    assert_fails(
        "tile { stack { tree { leaf length=\"4.5mm\" } } }",
        "dt::expr",
        "`4.5mm` is just `4.5`",
    );
    assert_fails(
        "tile { stack { tree { leaf length=4.5mm } } }",
        "dt::syntax",
        "units are not written: write 4.5",
    );
}

// ---- ranges and percents quoted -----------------------------------

#[test]
fn ranges_and_percents_are_quoted() {
    let e = first("tile { stack { grass cover=\"85%\" length=\"8..14\" } }");
    assert_eq!(
        e.value("cover")
            .unwrap()
            .num()
            .unwrap()
            .bounds(&none)
            .unwrap(),
        (0.85, 0.85)
    );
    assert_eq!(
        e.value("length")
            .unwrap()
            .num()
            .unwrap()
            .bounds(&none)
            .unwrap(),
        (8.0, 14.0)
    );
    assert_fails(
        "tile { stack { grass cover=85% } }",
        "dt::syntax",
        "\"85%\"",
    );
    assert_fails(
        "tile { stack { grass length=8..14 } }",
        "dt::syntax",
        "\"8..14\"",
    );
}

#[test]
fn shares_are_percents_and_sizes_are_not() {
    assert_fails(
        "tile { stack { rock buried=0.3 } }",
        "dt::percent",
        "write `buried=\"30%\"`",
    );
    assert_fails(
        "tile { stack { rock size=\"20%\" } }",
        "dt::percent",
        "not a percent",
    );
    assert_fails(
        "tile { stack { rock size=\"8..2\" } }",
        "dt::range",
        "low end first",
    );
}

#[test]
fn a_range_is_weighted_by_a_function_not_caret_or_mostly() {
    assert_fails(
        "tile { stack { scatter count=3 { stone size=\"1..6^2\" } } }",
        "dt::expr",
        "low(1..6, 4)",
    );
    // `mostly=` is gone, on a thing and on a scatter, as a property or a line
    for text in [
        "tile { stack { scatter count=3 { stone size=\"1..6\" mostly=small } } }",
        "tile { stack { scatter count=3 mostly=large { stone size=\"1..6\" } } }",
        "tile { stack { rock mostly=even } }",
        "tile { stack { stone { mostly small } } }",
    ] {
        assert_fails(text, "dt::unknown-control", "size=\"low(1..6, 11)\"");
    }
    let e = first("tile { stack { scatter count=3 { stone size=\"low(1..6, 11)\" } } }");
    assert!(
        e.children[0]
            .value("size")
            .unwrap()
            .num()
            .unwrap()
            .expr
            .is_range()
    );
}

// ---- renames ----------------------------------------------------

#[test]
fn old_words_say_what_replaced_them() {
    for (text, want) in [
        ("tile { stack { hills } }", "`hills` is now `dirt`"),
        ("tile { stack { meadow } }", "`meadow` is now `grass`"),
        (
            "tile { stack { wall at=north family=brick } }",
            "`family` is now `style`",
        ),
        ("tile { stack { dirt self=yard } }", "`self=` is now `at=`"),
        (
            "tile { stack { dirt { bank near=cliff } } }",
            "a bank is drawn",
        ),
        (
            "tile { stack { cliff form=mound } }",
            "a cliff's form is its region",
        ),
        (
            "tile { stack { scatter near=cliff { stone } } }",
            "`near=` is now `at=\"foot(…)\"`",
        ),
        ("tile { stack { scree } }", "scatter at=\"foot(cliff)\""),
        ("tile cols=2 { }", "size=\\\"2x1\\\""),
        ("heights { edge 3.4 }", "`heights` is now `let`"),
    ] {
        let (_, shown) = fails(text);
        let want = want.replace("\\\"", "\"");
        assert!(
            flat(&shown).contains(&want),
            "{text}: want `{want}`\n{shown}"
        );
    }
}

// ---- words for character, numbers always accepted --------------

#[test]
fn words_keep_their_numbers_and_numbers_are_accepted() {
    let e = first("tile { stack { cliff { stoneSize massive } } }");
    let w = e.value("stoneSize").unwrap().word().unwrap();
    assert_eq!((w.word, w.sets), ("massive", &[("stoneSize", 1.35)][..]));
    let e = first("tile { stack { cliff { stoneSize \"135%\" } } }");
    let n = e.value("stoneSize").unwrap().num().unwrap();
    assert!((n.bounds(&none).unwrap().0 - 1.35).abs() < 1e-12);
    // Weights instead of a word.
    let e = first("tile { stack { cliff { rock columns=50 blocks=50 } } }");
    assert!(e.value("rock").is_none());
    assert_eq!(e.control("rock").unwrap().subs.len(), 2);
    // A misspelt word suggests the right one.
    assert_fails(
        "tile { stack { tree { crown shape=rund } } }",
        "dt::choice",
        "did you mean `round`?",
    );
    // Wall height: a word or mm.
    let e = first("tile { stack { wall at=south { height 20 } } }");
    assert!(e.value("height").unwrap().num().is_some());
}

// ---- tree branching ---------------------------------------------

#[test]
fn branch_knobs_are_for_parts_files_only() {
    for b in ["open", "normal", "dense"] {
        first(&format!(
            "tile {{ stack {{ tree {{ crown branching={b} }} }} }}"
        ));
    }
    assert_fails(
        "tile { stack { tree { branch split=30 } } }",
        "dt::expert",
        "crown branching=open|normal|dense",
    );
    snippet("define my-tree { stack { tree { branch split=30 eagerness=0.5 } } }");
    assert_fails(
        "tile { stack { tree { debug attractors=#true } } }",
        "dt::unknown-control",
        "--show attractors",
    );
}

#[test]
fn an_old_hyphenated_control_name_gives_the_camel_case_one() {
    for (old, new) in dt_doc::elements::OLD_CONTROL_NAMES {
        assert!(!new.contains('-') && new.chars().any(char::is_uppercase));
        assert_eq!(old.replace('-', "").to_lowercase(), new.to_lowercase());
    }
    assert_fails(
        "tile { stack { scatter per-square=3 { rock } } }",
        "dt::unknown-control",
        "write `perSquare`",
    );
    assert_fails(
        "tile { stack { cliff { stone-size massive } } }",
        "dt::unknown-control",
        "write `stoneSize`",
    );
    assert_fails(
        "tile { stack { tree { crown clump-size=6 } } }",
        "dt::unknown-control",
        "write `clumpSize`",
    );
    assert_fails(
        "tile { stack { dirt bump-size=14 } }",
        "dt::unknown-control",
        "write `bumpSize`",
    );
    assert_fails(
        "tile {\n line shore { through west east }\n region pool { bounded-by shore; fill \"0.5,0.2\" }\n stack { dirt }\n}",
        "dt::unknown-node",
        "write `boundedBy`",
    );
}

// ---- the whole tile is the default area -------------------------

#[test]
fn regions_only_on_divided_tiles() {
    assert_fails(
        "tile { region all { fill \"0.5,0.5\" }; stack { dirt at=all } }",
        "dt::region",
        "the whole tile is already the default area",
    );
    assert_fails(
        "tile { stack { dirt at=verge } }",
        "dt::place",
        "no place is called `verge`",
    );
    assert_fails(
        "tile { line l { through west east }; region r { boundedBy l } }",
        "dt::region",
        "fill point",
    );
}

#[test]
fn a_tile_is_1x1_unless_it_says() {
    let doc = snippet("tile { stack { dirt } }");
    let h = doc.tile().unwrap().header().unwrap();
    assert_eq!((h.cols, h.rows, h.seed), (1, 1, None));
    let doc = snippet("tile size=\"3x2\" seed=9 { }");
    let h = doc.tile().unwrap().header().unwrap();
    assert_eq!((h.cols, h.rows, h.seed), (3, 2, Some(9)));
    assert_fails("tile size=\"3 by 2\" { }", "dt::tile", "columns by rows");
    assert_fails("tile size=2x2 { }", "dt::syntax", "\"2x2\"");
}

#[test]
fn a_tile_is_at_most_64_squares_a_side() {
    let h = snippet("tile size=\"64x64\" { }")
        .tile()
        .unwrap()
        .header()
        .unwrap();
    assert_eq!((h.cols, h.rows), (64, 64));
    assert_fails("tile size=\"65x1\" { }", "dt::tile", "at most 64 squares");
    assert_fails(
        "tile size=\"1x100000\" { }",
        "dt::tile",
        "at most 64 squares",
    );
    assert_fails("tile size=\"33554432x1\" { }", "dt::tile", "several tiles");
}

// ---- edges, edge paths, side= -----------------------------------

#[test]
fn edges_and_edge_paths() {
    let e = first("tile { stack { wall at=\"south east north\" } }");
    assert_eq!(
        e.place.along.unwrap().value,
        Along::Edges(vec![Edge::South, Edge::East, Edge::North])
    );
    assert_fails(
        "tile { stack { wall at=\"north south\" } }",
        "dt::place",
        "don't meet at a corner",
    );
    assert_fails(
        "tile { stack { wall at=nort } }",
        "dt::place",
        "did you mean `north`?",
    );
    assert_fails(
        "tile { stack { wall } }",
        "dt::missing-control",
        "`wall` needs `at=`",
    );
}

#[test]
fn side_is_for_drawn_lines() {
    let e = first("tile { line l { through west east }; stack { wall at=l side=left } }");
    assert_eq!(e.place.side.unwrap().value, Side::Left);
    assert_fails(
        "tile { stack { wall at=north side=left } }",
        "dt::place",
        "always stands inside the tile",
    );
    assert_fails(
        "tile { line l { through west east }; stack { wall at=l side=middle } }",
        "dt::choice",
        "left, right or centre",
    );
}

#[test]
fn a_line_cannot_take_an_edge_name() {
    assert_fails(
        "tile { line north { through west east } }",
        "dt::line",
        "tile edge",
    );
}

// ---- points -----------------------------------------------------

#[test]
fn points_are_pairs_corners_and_edges() {
    let e = first("tile { stack { tree at=\"0.5,0.5\" } }");
    assert!(matches!(e.place.at.unwrap().value, Point::Pair { .. }));
    let e = first("tile { stack { tree at=north-west } }");
    assert_eq!(e.place.at.unwrap().value, Point::Corner(Corner::NorthWest));
    let e = first("tile { stack { tree at=south } }");
    assert_eq!(e.place.at.unwrap().value, Point::Edge(Edge::South));
    assert_fails("tile { stack { tree at=\"1 1\" } }", "dt::point", "comma");
    assert_fails(
        "tile { stack { tree at=\"2,0.5\" } }",
        "dt::point",
        "off the tile",
    );
    assert_fails("tile { stack { tree at=1 } }", "dt::point", "quoted pair");
}

// ---- a range for at=, none for anywhere -----------------------

#[test]
fn at_may_be_a_range_or_absent() {
    let doc = snippet("tile size=\"2x2\" { stack { tree at=\"0.6..1.4,0.6..1.4\"; rock } }");
    let s = &doc.tile().unwrap().stack;
    let at = &s[0].place.at.as_ref().unwrap().value;
    assert!(at.is_range());
    assert_eq!(at.bounds(2.0, 2.0).unwrap(), ((0.6, 1.4), (0.6, 1.4)));
    assert!(s[1].place.at.is_none());
}

// ---- scatter ---------------------------------------------------

#[test]
fn a_thing_is_one() {
    assert_fails(
        "tile { stack { tree count=3 } }",
        "dt::unknown-control",
        "scatter count=… { tree }",
    );
    assert_fails(
        "tile { stack { rock perSquare=3 } }",
        "dt::unknown-control",
        "a scatter's job",
    );
    assert_fails(
        "tile { stack { tree at=\"north west\" } }",
        "dt::type",
        "scatter at=<line>",
    );
}

#[test]
fn scatter_owns_placement_and_mixes_by_weight() {
    let e =
        first("tile { stack { scatter perSquare=6 { boulder size=\"2..8\" weight=2; pebble } } }");
    assert_eq!(e.children.len(), 2);
    assert_eq!(e.children[0].place.weight.as_ref().unwrap().value, 2.0);
    assert!(e.children[1].place.weight.is_none());
    assert_eq!(e.kind, Kind::Solid);
    // A defined thing can be scattered, and placed alone.
    let doc = snippet(
        "use \"parts/trees\" oak\ntile size=\"2x2\" { stack { scatter count=2 { oak }; oak at=\"1,1\" top=\"bottom + 60\" } }",
    );
    let s = &doc.tile().unwrap().stack;
    assert_eq!(s[0].kind, Kind::Solid);
    assert_eq!(
        s[1].top.as_ref().unwrap().value.to_string(),
        "(bottom + 60)"
    );
}

#[test]
fn scatter_rules() {
    for (text, want) in [
        (
            "tile { stack { scatter { rock } } }",
            "a scatter says how many",
        ),
        (
            "tile { stack { scatter count=1 { rock } } }",
            "for more than one",
        ),
        (
            "tile { stack { scatter count=3 perSquare=2 { rock } } }",
            "both say how many",
        ),
        (
            "tile { stack { scatter count=3 reach=5 { rock } } }",
            "a line or a foot",
        ),
        (
            "tile { line road { through \"0,0.5\" \"1,0.5\" }; stack { tree at=road } }",
            "a line has no one place on it",
        ),
        (
            "tile { line road { through \"0,0.5\" \"1,0.5\" }; stack { rock at=road } }",
            "`scatter at=<line> count=3",
        ),
        ("tile { stack { scatter count=3 } }", "children say what"),
        (
            "tile { stack { scatter count=3 { dirt } } }",
            "can't be scattered",
        ),
        (
            "tile { stack { scatter count=3 { rock at=\"0.5,0.5\" } } }",
            "placed by its scatter",
        ),
        ("tile { stack { rock weight=2 } }", "share of the mix"),
        ("tile { stack { tree { rock } } }", "can't go inside `tree`"),
        (
            "tile { stack { dirt { rock } } }",
            "things on a surface go in a scatter",
        ),
        (
            "tile { stack { scatter count=2.5 { rock } } }",
            "count of whole things",
        ),
    ] {
        let (_, shown) = fails(text);
        assert!(
            flat(&shown).contains(want),
            "{text}: want `{want}`\n{shown}"
        );
    }
}

// ---- stack order is free for solids -----------------------------

#[test]
fn kinds_let_solids_be_planned_first() {
    let doc = snippet(
        "use \"parts/grass\" lawn\ntile { stack { lawn; scatter count=3 { rock }; dirt; wall at=north } }",
    );
    let kinds: Vec<_> = doc.tile().unwrap().stack.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [Kind::Surface, Kind::Solid, Kind::Surface, Kind::Solid]
    );
}

// ---- use -------------------------------------------------------------------

#[test]
fn use_names_where_every_name_came_from() {
    snippet("use \"lib:parts/grass\" lawn\ntile { stack { lawn } }");
    assert_fails(
        "tile { stack { lawn } }",
        "dt::unknown-element",
        "write `use \"parts/grass\" lawn`",
    );
    assert_fails(
        "use \"parts/grass\" lawn\ntile { stack { long-grass } }",
        "dt::unknown-element",
        "add it to the use line",
    );
    assert_fails(
        "use \"parts/grass\" lwn\ntile { }",
        "dt::use",
        "did you mean `lawn`?",
    );
    assert_fails(
        "use \"parts/gras\" lawn\ntile { }",
        "dt::use",
        "did you mean `parts/grass`?",
    );
    assert_fails(
        "use \"parts/grass\"\ntile { }",
        "dt::use",
        "use \"parts/grass\" lawn long-grass",
    );
    let u = dt_doc::uses("t.dtile", "use \"./mine\" a b\ntile { }").unwrap();
    assert_eq!(u[0].key(), "./mine");
    assert_eq!(u[0].names.len(), 2);
}

#[test]
fn definitions_are_typed_by_what_their_parameters_fill() {
    assert_fails(
        "use \"parts/grass\" lawn\ntile { stack { lawn cover=0.5 } }",
        "dt::percent",
        "cover=\"50%\"",
    );
    assert_fails(
        "use \"parts/grass\" lawn\ntile { stack { lawn lenght=5 } }",
        "dt::unknown-control",
        "did you mean `length`?",
    );
    // A required parameter.
    assert_fails(
        "define d { param h; stack { rock size=h } }\ntile { stack { d } }",
        "dt::missing-control",
        "`d` needs `h`",
    );
    // Calls in any order within a file; loops refused.
    snippet("tile { stack { b } }\ndefine b { stack { a } }\ndefine a { stack { dirt } }");
    assert_fails(
        "define a { stack { b } }\ndefine b { stack { a } }",
        "dt::loop",
        "calls itself",
    );
}

// ---- errors keep going ---------------------------------------------------

#[test]
fn every_mistake_is_reported() {
    let (e, shown) =
        fails("tile { stack { hills; tree hieght=40; wall at=north style=feildstone } }");
    assert_eq!(e.len(), 3, "{shown}");
    assert!(e.iter().all(|e| e.help.is_some()), "{shown}");
}

#[test]
fn value_is_kept_with_its_span() {
    let text = "tile { stack { rock size=40 } }";
    let doc = snippet(text);
    let e = &doc.tile().unwrap().stack[0];
    let s = e.control("size").unwrap().value.as_ref().unwrap().span;
    assert_eq!(&text[s.offset()..s.offset() + s.len()], "40");
    assert!(matches!(e.value("size"), Some(Value::Num(_))));
}

// ---- two whole tiles ------------------------------------------------------

#[test]
fn the_r6_snippets_decode() {
    let e = first(
        "tile { stack { wall at=south style=cut-stone {\n    height full\n    thickness 7\n} } }",
    );
    assert_eq!(e.value("style").unwrap().word().unwrap().word, "cut-stone");
    let e = first(
        "tile { stack { cliff {\n    rock blocks\n    ledges 1 height=\"40%\"           // a shelf 40% of the way up\n} } }",
    );
    assert_eq!(e.value("rock").unwrap().word().unwrap().word, "blocks");
}

#[test]
fn ids_are_plain_names_and_unique_among_same_named_siblings() {
    let e = first("tile { stack { dirt id=low } }");
    assert_eq!(e.place.id.unwrap().value, "low");
    snippet("tile { stack { dirt id=a; tree id=a } }");
    assert_fails(
        "tile { stack { dirt id=x; dirt id=x } }",
        "dt::id",
        "already has `id=x`",
    );
    assert_fails("tile { stack { dirt id=\"a/b\" } }", "dt::id", "plain name");
    assert_fails("tile { stack { dirt id=\"0\" } }", "dt::id", "plain name");
    assert_fails("tile { stack { dirt id=\"\" } }", "dt::id", "plain name");
}

#[test]
fn seeds_run_from_0_to_i64_max() {
    let h = snippet("tile seed=9223372036854775807 { }")
        .tile()
        .unwrap()
        .header()
        .unwrap();
    assert_eq!(h.seed, Some(i64::MAX));
    // was "`seed` is a whole number" for a whole number too big
    assert_fails(
        "tile seed=18446744073709551615 { }",
        "dt::seed",
        "from 0 to 9223372036854775807, not 18446744073709551615",
    );
    // was reported at the tile's name, by dt-eval
    let (e, _) = fails("tile seed=-5 { }");
    assert_eq!(e[0].code, "dt::seed");
    assert_eq!(e[0].span.offset(), "tile seed=".len());
    assert_fails("tile { stack { dirt seed=-1 } }", "dt::seed", "not -1");
    assert_fails("tile seed=1.5 { }", "dt::seed", "is a whole number");
}

#[test]
fn an_unclosed_brace_is_one_error() {
    let (e, shown) = fails("tile size=\"1x1\" {\n stack {\n  dirt\n }\n");
    assert_eq!(e.len(), 1, "{shown}");
    assert!(shown.contains("No closing '}'"), "{shown}");
}

#[test]
fn a_tile_is_named_by_its_file() {
    assert_fails(
        "tile meadow size=\"1x1\" { stack { dirt } }",
        "dt::tile-name",
        "its file name is its name",
    );
    assert_fails(
        "tile \"My Pond\" { stack { dirt } }",
        "dt::tile-name",
        "rename the file",
    );
}

#[test]
fn an_old_flowers_and_tree_height_name_the_new_forms() {
    assert_fails(
        "tile { stack { flowers perSquare=4 } }",
        "dt::unknown-element",
        "scatter perSquare=4 { flower }",
    );
    assert_fails(
        "tile { stack { tree height=40 } }",
        "dt::unknown-control",
        "`tree top=\"bottom + 40\"`",
    );
    assert_fails(
        "tile { stack { tree size=40 } }",
        "dt::unknown-control",
        "`tree top=\"bottom + 40\"`",
    );
    // a flower is a thing: placed alone, or scattered
    snippet("tile { stack { dirt; flower at=\"0.5,0.5\" size=\"4..6\" } }");
    snippet("tile { stack { dirt; scatter perSquare=4 { flower size=5 petals=6 } } }");
    assert_fails(
        "tile { stack { dirt; flower radius=2 } }",
        "dt::unknown-control",
        "twice the old radius",
    );
}

#[test]
fn a_declared_place_wins_over_an_edge_word() {
    // a region named `west` is that region, not the west edge
    let e = first(
        "tile { line a { through \"0.33,0\" \"0.33,1\" }; region west { boundedBy a; fill \"0.1,0.5\" }; stack { grass at=west } }",
    );
    assert_eq!(e.place.region.as_ref().unwrap().value, "west");
    assert!(e.place.along.is_none());
    // with no such place, the word is still the edge
    let e = first("tile { stack { scatter at=west count=2 { rock } } }");
    assert!(matches!(
        e.place.along.as_ref().map(|a| &a.value),
        Some(Along::Edges(v)) if v == &vec![Edge::West]
    ));
}
