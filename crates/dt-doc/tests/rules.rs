//! One test (or a few) per R6 rule: what decodes, and what is refused.
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use common::*;
use dt_doc::expr::Unit;
use dt_doc::model::{Form, Value};
use dt_doc::place::{Along, Corner, Edge, Point, Side};
use dt_doc::schema::{self, Kind, Scaling};

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

// ---- R6.1 / D1: mm at the reference, named levels, (h) opt-in ----------

#[test]
fn sizes_are_mm_and_h_is_opt_in() {
    let e = first("tile t { stack { tree height=40 } }");
    let n = e.value("height").unwrap().num().unwrap();
    assert_eq!((n.unit, n.bounds(&none).unwrap()), (None, (40.0, 40.0)));
    let e = first("tile t { stack { tree height=(h)1.2 } }");
    let n = e.value("height").unwrap().num().unwrap();
    assert_eq!(n.unit, Some(Unit::H));
    assert!((n.bounds(&none).unwrap().0 - 1.2 * schema::H_MM).abs() < 1e-9);
}

#[test]
fn levels_are_named_or_mm_or_expressions() {
    for text in [
        "tile t { stack { dirt level=ground } }",
        "tile t { stack { dirt level=wall-top } }",
        "tile t { stack { dirt level=4 } }",
        "tile t { stack { dirt level=\"ground - 1\" } }",
        "tile t { levels foot=4; stack { dirt level=foot } }",
        "tile t { levels path=\"ground - 1\"; stack { dirt level=path } }",
    ] {
        snippet(text);
    }
    assert_fails(
        "tile t { stack { dirt level=grund } }",
        "dt::unknown-name",
        "did you mean `ground`?",
    );
    // A level name is not a size.
    assert_fails(
        "tile t { stack { tree height=ground } }",
        "dt::unknown-name",
        "no parameter is called `ground`",
    );
}

// ---- R6.8: the standard levels are a prelude ----------------------------

#[test]
fn standard_levels_need_no_use_line() {
    let k = dt_doc::Known::standard();
    dt_doc::decode("t.kdl", "tile t { stack { dirt level=edge } }", &k).unwrap();
    // Without the prelude the names are unknown.
    let bare = dt_doc::Known::new();
    assert!(dt_doc::decode("t.kdl", "tile t { stack { dirt level=edge } }", &bare).is_err());
}

#[test]
fn standard_levels_are_in_mm_and_listed_for_an_unknown_name() {
    let k = dt_doc::Known::standard();
    let mm = |n: &str| k.levels().iter().find(|(k, _)| k == n).map(|(_, v)| *v);
    assert_eq!(mm("ground"), Some(5.0));
    assert_eq!(mm("wall-top"), Some(33.1));
    let e = dt_doc::decode("t.kdl", "tile t { stack { dirt level=lava } }", &k).unwrap_err();
    let help = e[0].help.as_deref().unwrap();
    assert!(help.contains("wall-top (33.1 mm)"), "{help}");
    // a near miss is suggested on its own
    let e = dt_doc::decode("t.kdl", "tile t { stack { dirt level=grund } }", &k).unwrap_err();
    assert_eq!(e[0].help.as_deref(), Some("did you mean `ground`?"));
}

#[test]
fn a_tile_may_override_a_standard_level() {
    snippet("tile t { levels ground=6; stack { dirt level=ground } }");
}

// ---- R6.5 / D2: every written size scales; recorded as metadata ---------

#[test]
fn scaling_is_recorded_per_control() {
    let tree = schema::look("tree").unwrap();
    assert_eq!(
        tree.control("height").unwrap().scaling(),
        Scaling::Photocopy
    );
    let leaf = tree.control("leaf").unwrap();
    assert_eq!(leaf.sub("length").unwrap().scaling(), Scaling::Photocopy);
    let crown = tree.control("crown").unwrap();
    assert_eq!(crown.sub("clumps").unwrap().scaling(), Scaling::Fixed);
    let grass = schema::look("grass").unwrap();
    assert_eq!(grass.control("curl").unwrap().scaling(), Scaling::Fixed);
    assert_eq!(grass.control("tufts").unwrap().scaling(), Scaling::Squares);
    let e = first("tile t { stack { tree { leaf length=4.5 } } }");
    let s = e.control("leaf").unwrap();
    assert_eq!(s.subs[0].control.unwrap().scaling(), Scaling::Photocopy);
}

// ---- R6.2: both forms, same model ---------------------------------------

#[test]
fn line_and_child_forms_decode_the_same() {
    let a = first("tile t { stack { cliff along=north height=35 } }");
    let b = first("tile t { stack { cliff along=north { height 35 } } }");
    assert_eq!(a.value("height"), b.value("height"));
    assert_eq!(a.control("height").unwrap().form, Form::Property);
    assert_eq!(b.control("height").unwrap().form, Form::Child);
    // Words, and place words, too.
    let a = first("tile t { stack { cliff along=north rock=blocks } }");
    let b = first("tile t { stack { cliff { along north; rock blocks } } }");
    assert_eq!(a.value("rock"), b.value("rock"));
    assert_eq!(a.place.along.unwrap().value, b.place.along.unwrap().value);
}

#[test]
fn a_control_is_written_once() {
    assert_fails(
        "tile t { stack { cliff along=north height=35 { height 40 } } }",
        "dt::duplicate",
        "`height` is set twice",
    );
}

#[test]
fn groups_are_child_lines() {
    assert_fails(
        "tile t { stack { tree leaf=4 } }",
        "dt::group",
        "`leaf` is a group",
    );
    assert_fails(
        "tile t { stack { tree { leaf 4.5 } } }",
        "dt::group",
        "has no value of its own",
    );
    // A control with a value and sub-controls: ledges 1 height="40%".
    let e = first("tile t { stack { cliff along=north { ledges 1 height=\"40%\" } } }");
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
    // bumps: a value, or up/down.
    first("tile t { stack { ground bumps=3 } }");
    first("tile t { stack { ground { bumps up=3 down=1 } } }");
}

// ---- R6.3: units are not written ----------------------------------------

#[test]
fn mm_annotation_is_an_optional_check() {
    let e = first("tile t { stack { tree { leaf length=(mm)4.5 } } }");
    let n = e.sub("leaf", "length").unwrap().num().unwrap();
    assert_eq!(n.unit, Some(Unit::Mm));
    assert_eq!(n.bounds(&none).unwrap(), (4.5, 4.5));
    assert_fails(
        "tile t { stack { tree { crown clumps=(mm)30 } } }",
        "dt::unit",
        "(mm) and (h) are for sizes and levels",
    );
    assert_fails(
        "tile t { stack { tree height=(cm)4 } }",
        "dt::unit",
        "units are not written",
    );
}

#[test]
fn unit_suffixes_are_refused_with_a_hint() {
    assert_fails(
        "tile t { stack { tree { leaf length=\"4.5mm\" } } }",
        "dt::expr",
        "`4.5mm` is just `4.5`",
    );
    assert_fails(
        "tile t { stack { tree { leaf length=4.5mm } } }",
        "dt::syntax",
        "units are not written: write 4.5",
    );
}

// ---- R6.4: ranges and percents quoted -----------------------------------

#[test]
fn ranges_and_percents_are_quoted() {
    let e = first("tile t { stack { grass cover=\"85%\" length=\"8..14\" } }");
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
        "tile t { stack { grass cover=85% } }",
        "dt::syntax",
        "\"85%\"",
    );
    assert_fails(
        "tile t { stack { grass length=8..14 } }",
        "dt::syntax",
        "\"8..14\"",
    );
}

#[test]
fn shares_are_percents_and_sizes_are_not() {
    assert_fails(
        "tile t { stack { rock buried=0.3 } }",
        "dt::percent",
        "write `buried=\"30%\"`",
    );
    assert_fails(
        "tile t { stack { rock size=\"20%\" } }",
        "dt::percent",
        "not a percent",
    );
    assert_fails(
        "tile t { stack { rock size=\"8..2\" } }",
        "dt::range",
        "low end first",
    );
}

#[test]
fn skew_is_mostly_not_caret() {
    let e = first("tile t { stack { scatter count=3 mostly=large { stone size=\"1..6\" } } }");
    assert_eq!(
        e.value("mostly").unwrap().word().unwrap().sets,
        &[("exponent", 0.5)]
    );
    assert_fails(
        "tile t { stack { scatter count=3 { stone size=\"1..6^2\" } } }",
        "dt::expr",
        "mostly=small",
    );
}

// ---- D8 / V6: renames ----------------------------------------------------

#[test]
fn old_words_say_what_replaced_them() {
    for (text, want) in [
        ("tile t { stack { hills } }", "`hills` is now `ground`"),
        ("tile t { stack { meadow } }", "`meadow` is now `grass`"),
        (
            "tile t { stack { wall along=north family=brick } }",
            "`family` is now `style`",
        ),
        (
            "tile t { stack { dirt self=yard } }",
            "`self=` is now `in=`",
        ),
        (
            "tile t { stack { dirt { bank near=cliff } } }",
            "`near=` is now `against=`",
        ),
        ("tile t { stack { scree } }", "scatter against=cliff"),
        ("tile t cols=2 { }", "size=\\\"2x1\\\""),
        ("heights { edge 3.4 }", "`heights` is now `levels`"),
    ] {
        let (_, shown) = fails(text);
        let want = want.replace("\\\"", "\"");
        assert!(
            flat(&shown).contains(&want),
            "{text}: want `{want}`\n{shown}"
        );
    }
}

// ---- D4 / V2: words for character, numbers always accepted --------------

#[test]
fn words_keep_their_numbers_and_numbers_are_accepted() {
    let e = first("tile t { stack { cliff along=north { stone-size massive } } }");
    let w = e.value("stone-size").unwrap().word().unwrap();
    assert_eq!((w.word, w.sets), ("massive", &[("stone-size", 1.35)][..]));
    let e = first("tile t { stack { cliff along=north { stone-size \"135%\" } } }");
    let n = e.value("stone-size").unwrap().num().unwrap();
    assert!((n.bounds(&none).unwrap().0 - 1.35).abs() < 1e-12);
    // Weights instead of a word.
    let e = first("tile t { stack { cliff along=north { rock columns=50 blocks=50 } } }");
    assert!(e.value("rock").is_none());
    assert_eq!(e.control("rock").unwrap().subs.len(), 2);
    // A misspelt word suggests the right one.
    assert_fails(
        "tile t { stack { tree { crown shape=rund } } }",
        "dt::choice",
        "did you mean `round`?",
    );
    // Wall height: a word or mm.
    let e = first("tile t { stack { wall along=south { height 20 } } }");
    assert!(e.value("height").unwrap().num().is_some());
}

// ---- D9 / V7: tree branching ---------------------------------------------

#[test]
fn branch_knobs_are_for_parts_files_only() {
    for b in ["open", "normal", "dense"] {
        first(&format!(
            "tile t {{ stack {{ tree {{ crown branching={b} }} }} }}"
        ));
    }
    assert_fails(
        "tile t { stack { tree { branch split=30 } } }",
        "dt::expert",
        "crown branching=open|normal|dense",
    );
    snippet("define my-tree { stack { tree { branch split=30 eagerness=0.5 } } }");
    assert_fails(
        "tile t { stack { tree { debug attractors=#true } } }",
        "dt::unknown-control",
        "--show attractors",
    );
}

// ---- D5 / V3: the whole tile is the default area -------------------------

#[test]
fn regions_only_on_divided_tiles() {
    assert_fails(
        "tile t { region all { fill \"0.5,0.5\" }; stack { dirt in=all } }",
        "dt::region",
        "the whole tile is already the default area",
    );
    assert_fails(
        "tile t { stack { dirt in=verge } }",
        "dt::unknown-name",
        "leave `in=` out",
    );
    assert_fails(
        "tile t { line l { through west east }; region r { bounded-by l } }",
        "dt::region",
        "fill point",
    );
}

#[test]
fn a_tile_is_1x1_unless_it_says() {
    let doc = snippet("tile t { stack { ground } }");
    let h = doc.tile().unwrap().header().unwrap();
    assert_eq!((h.cols, h.rows, h.seed), (1, 1, None));
    let doc = snippet("tile t size=\"3x2\" seed=9 { }");
    let h = doc.tile().unwrap().header().unwrap();
    assert_eq!((h.cols, h.rows, h.seed), (3, 2, Some(9)));
    assert_fails("tile t size=\"3 by 2\" { }", "dt::tile", "columns by rows");
    assert_fails("tile t size=2x2 { }", "dt::syntax", "\"2x2\"");
}

#[test]
fn a_tile_is_at_most_64_squares_a_side() {
    let h = snippet("tile t size=\"64x64\" { }")
        .tile()
        .unwrap()
        .header()
        .unwrap();
    assert_eq!((h.cols, h.rows), (64, 64));
    assert_fails("tile t size=\"65x1\" { }", "dt::tile", "at most 64 squares");
    assert_fails(
        "tile t size=\"1x100000\" { }",
        "dt::tile",
        "at most 64 squares",
    );
    assert_fails(
        "tile t size=\"33554432x1\" { }",
        "dt::tile",
        "several tiles",
    );
}

// ---- D6 / V4: edges, edge paths, side= -----------------------------------

#[test]
fn edges_and_edge_paths() {
    let e = first("tile t { stack { wall along=\"south east north\" } }");
    assert_eq!(
        e.place.along.unwrap().value,
        Along::Edges(vec![Edge::South, Edge::East, Edge::North])
    );
    assert_fails(
        "tile t { stack { wall along=\"north south\" } }",
        "dt::place",
        "don't meet at a corner",
    );
    assert_fails(
        "tile t { stack { wall along=nort } }",
        "dt::unknown-name",
        "did you mean `north`?",
    );
    assert_fails(
        "tile t { stack { wall } }",
        "dt::missing-control",
        "`wall` needs `along=`",
    );
}

#[test]
fn side_is_for_drawn_lines() {
    let e = first("tile t { line l { through west east }; stack { wall along=l side=left } }");
    assert_eq!(e.place.side.unwrap().value, Side::Left);
    assert_fails(
        "tile t { stack { wall along=north side=left } }",
        "dt::place",
        "always stands inside the tile",
    );
    assert_fails(
        "tile t { line l { through west east }; stack { wall along=l side=middle } }",
        "dt::choice",
        "left, right or centre",
    );
}

#[test]
fn a_line_cannot_take_an_edge_name() {
    assert_fails(
        "tile t { line north { through west east } }",
        "dt::line",
        "tile edge",
    );
}

// ---- D7 / V5: points -----------------------------------------------------

#[test]
fn points_are_pairs_corners_and_edges() {
    let e = first("tile t { stack { tree at=\"0.5,0.5\" } }");
    assert!(matches!(e.place.at.unwrap().value, Point::Pair { .. }));
    let e = first("tile t { stack { tree at=north-west } }");
    assert_eq!(e.place.at.unwrap().value, Point::Corner(Corner::NorthWest));
    let e = first("tile t { stack { tree at=south } }");
    assert_eq!(e.place.at.unwrap().value, Point::Edge(Edge::South));
    assert_fails("tile t { stack { tree at=\"1 1\" } }", "dt::point", "comma");
    assert_fails(
        "tile t { stack { tree at=\"2,0.5\" } }",
        "dt::point",
        "off the tile",
    );
    assert_fails("tile t { stack { tree at=1 } }", "dt::point", "quoted pair");
}

// ---- D12 / V10: a range for at=, none for anywhere -----------------------

#[test]
fn at_may_be_a_range_or_absent() {
    let doc = snippet("tile t size=\"2x2\" { stack { tree at=\"0.6..1.4,0.6..1.4\"; rock } }");
    let s = &doc.tile().unwrap().stack;
    let at = &s[0].place.at.as_ref().unwrap().value;
    assert!(at.is_range());
    assert_eq!(at.bounds(2.0, 2.0).unwrap(), ((0.6, 1.4), (0.6, 1.4)));
    assert!(s[1].place.at.is_none());
}

// ---- D11 / V9: scatter ---------------------------------------------------

#[test]
fn a_thing_is_one() {
    assert_fails(
        "tile t { stack { tree count=3 } }",
        "dt::unknown-control",
        "scatter count=… { tree }",
    );
    assert_fails(
        "tile t { stack { rock per-square=3 } }",
        "dt::unknown-control",
        "a scatter's job",
    );
    assert_fails(
        "tile t { stack { tree along=north } }",
        "dt::place",
        "scatter along=",
    );
}

#[test]
fn scatter_owns_placement_and_mixes_by_weight() {
    let e = first(
        "tile t { stack { scatter per-square=6 mostly=small { boulder size=\"2..8\" weight=2; pebble } } }",
    );
    assert_eq!(e.children.len(), 2);
    assert_eq!(e.children[0].place.weight.as_ref().unwrap().value, 2.0);
    assert!(e.children[1].place.weight.is_none());
    assert_eq!(e.kind, Kind::Solid);
    // A defined thing can be scattered, and placed alone.
    let doc = snippet(
        "use \"parts/trees\" oak\ntile t size=\"2x2\" { stack { scatter count=2 { oak }; oak at=\"1,1\" height=60 } }",
    );
    let s = &doc.tile().unwrap().stack;
    assert_eq!(s[0].kind, Kind::Solid);
    assert_eq!(
        s[1].value("height")
            .unwrap()
            .num()
            .unwrap()
            .bounds(&none)
            .unwrap(),
        (60.0, 60.0)
    );
}

#[test]
fn scatter_rules() {
    for (text, want) in [
        (
            "tile t { stack { scatter { rock } } }",
            "a scatter says how many",
        ),
        (
            "tile t { stack { scatter count=1 { rock } } }",
            "for more than one",
        ),
        (
            "tile t { stack { scatter count=3 per-square=2 { rock } } }",
            "both say how many",
        ),
        (
            "tile t { stack { scatter count=3 spread=5 { rock } } }",
            "clusters round an `at=`",
        ),
        (
            "tile t { stack { scatter count=3 reach=5 { rock } } }",
            "`against=`",
        ),
        ("tile t { stack { scatter count=3 } }", "children say what"),
        (
            "tile t { stack { scatter count=3 { dirt } } }",
            "can't be scattered",
        ),
        (
            "tile t { stack { scatter count=3 { rock at=\"0.5,0.5\" } } }",
            "placed by its scatter",
        ),
        ("tile t { stack { rock weight=2 } }", "share of the mix"),
        (
            "tile t { stack { tree { rock } } }",
            "can't go inside `tree`",
        ),
        (
            "tile t { stack { dirt { rock } } }",
            "things on a surface go in a scatter",
        ),
        (
            "tile t { stack { scatter count=2.5 { rock } } }",
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

// ---- D3 / V1: stack order is free for solids -----------------------------

#[test]
fn kinds_let_solids_be_planned_first() {
    let doc = snippet(
        "use \"parts/grass\" lawn\ntile t { stack { lawn; scatter count=3 { rock }; dirt; wall along=north } }",
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
    snippet("use \"lib:parts/grass\" lawn\ntile t { stack { lawn } }");
    assert_fails(
        "tile t { stack { lawn } }",
        "dt::unknown-look",
        "write `use \"parts/grass\" lawn`",
    );
    assert_fails(
        "use \"parts/grass\" lawn\ntile t { stack { long-grass } }",
        "dt::unknown-look",
        "add it to the use line",
    );
    assert_fails(
        "use \"parts/grass\" lwn\ntile t { }",
        "dt::use",
        "did you mean `lawn`?",
    );
    assert_fails(
        "use \"parts/gras\" lawn\ntile t { }",
        "dt::use",
        "did you mean `parts/grass`?",
    );
    assert_fails(
        "use \"parts/grass\"\ntile t { }",
        "dt::use",
        "use \"parts/grass\" lawn long-grass",
    );
    let u = dt_doc::uses("t.kdl", "use \"./mine\" a b\ntile t { }").unwrap();
    assert_eq!(u[0].key(), "./mine");
    assert_eq!(u[0].names.len(), 2);
}

#[test]
fn definitions_are_typed_by_what_their_parameters_fill() {
    assert_fails(
        "use \"parts/grass\" lawn\ntile t { stack { lawn cover=0.5 } }",
        "dt::percent",
        "cover=\"50%\"",
    );
    assert_fails(
        "use \"parts/grass\" lawn\ntile t { stack { lawn lenght=5 } }",
        "dt::unknown-control",
        "did you mean `length`?",
    );
    // A required parameter.
    assert_fails(
        "define d { param h; stack { tree height=h } }\ntile t { stack { d } }",
        "dt::missing-control",
        "`d` needs `h`",
    );
    // Calls in any order within a file; loops refused.
    snippet("tile t { stack { b } }\ndefine b { stack { a } }\ndefine a { stack { ground } }");
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
        fails("tile t { stack { hills; tree hieght=40; wall along=north style=feildstone } }");
    assert_eq!(e.len(), 3, "{shown}");
    assert!(e.iter().all(|e| e.help.is_some()), "{shown}");
}

#[test]
fn value_is_kept_with_its_span() {
    let text = "tile t { stack { tree height=40 } }";
    let doc = snippet(text);
    let e = &doc.tile().unwrap().stack[0];
    let s = e.control("height").unwrap().value.as_ref().unwrap().span;
    assert_eq!(&text[s.offset()..s.offset() + s.len()], "40");
    assert!(matches!(e.value("height"), Some(Value::Num(_))));
}

// ---- the R6 page's two snippets -------------------------------------------

#[test]
fn the_r6_snippets_decode() {
    let e = first(
        "tile t { stack { wall along=south style=cut-stone {\n    height full\n    thickness 7\n} } }",
    );
    assert_eq!(e.value("style").unwrap().word().unwrap().word, "cut-stone");
    let e = first(
        "tile t { stack { cliff along=north height=55 {\n    depth \"12..24\"                  // how far the rock reaches into the tile\n    rock blocks\n    ledges 1 height=\"40%\"           // a shelf 40% of the way up: 22 mm\n} } }",
    );
    assert_eq!(
        e.value("depth")
            .unwrap()
            .num()
            .unwrap()
            .bounds(&none)
            .unwrap(),
        (12.0, 24.0)
    );
    assert_eq!(e.value("rock").unwrap().word().unwrap().word, "blocks");
}

#[test]
fn ids_are_plain_names_and_unique_among_same_named_siblings() {
    let e = first("tile t { stack { ground id=low } }");
    assert_eq!(e.place.id.unwrap().value, "low");
    snippet("tile t { stack { ground id=a; tree id=a } }");
    assert_fails(
        "tile t { stack { ground id=x; ground id=x } }",
        "dt::id",
        "already has `id=x`",
    );
    assert_fails(
        "tile t { stack { ground id=\"a/b\" } }",
        "dt::id",
        "plain name",
    );
    assert_fails(
        "tile t { stack { ground id=\"0\" } }",
        "dt::id",
        "plain name",
    );
    assert_fails(
        "tile t { stack { ground id=\"\" } }",
        "dt::id",
        "plain name",
    );
}
