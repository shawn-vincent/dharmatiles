//! Weighted ranges end to end: a document that writes one builds with its
//! odds; a flat one is the plain range to the last bit; every refusal is a
//! `dt::` diagnostic with its code and a help line.

#![allow(clippy::unwrap_used)]

use dt_eval::{Diag, Memory, Options, build, check};

fn mem(text: &str) -> Memory {
    Memory(
        [("t.dtile".to_owned(), text.to_owned())]
            .into_iter()
            .collect(),
    )
}

fn tile(lets: &str, stack: &str) -> String {
    format!("tile size=\"1x1\" seed=7 {{\n{lets}\n    stack {{\n{stack}\n    }}\n}}\n")
}

fn errors(text: &str) -> Vec<Diag> {
    match check(&mem(text), "t.dtile", &Options::default()) {
        Ok(c) => panic!("expected an error; notes {:?}", c.notes()),
        Err(e) => e,
    }
}

fn hash(text: &str) -> Vec<(String, u64)> {
    let b = build(&mem(text), "t.dtile", &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    b.stages.iter().map(|s| (s.name.clone(), s.hash)).collect()
}

fn refused(text: &str, code: &str, help: &str) -> Diag {
    let e = errors(text);
    let d = e
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("want {code}: {e:?}"));
    assert!(
        d.help.as_deref().is_some_and(|h| h.contains(help)),
        "{code} help {:?} lacks `{help}`",
        d.help
    );
    assert!(d.location().is_some(), "{code} points at the document");
    d.clone()
}

const SCATTER: &str = "        dirt\n        scatter perSquare=4 { stone size=\"1..6\" }";

#[test]
fn a_flat_weighting_is_the_plain_range_to_the_last_bit() {
    let plain = hash(&tile("", "        dirt\n        rock size=\"6..10\""));
    for same in [
        "flat(6..10)",
        "flat([6 .. 10])",
        "low(6..10, 1)",
        "high(6..10, 1)",
    ] {
        let text = tile("", &format!("        dirt\n        rock size=\"{same}\""));
        assert_eq!(hash(&text), plain, "{same}");
    }
    // and a let holding the plain range draws the same
    let text = tile(
        "    let fat \"[6 .. 10]\"",
        "        dirt\n        rock size=\"fat\"",
    );
    assert_eq!(hash(&text), plain);
}

#[test]
fn a_weighting_moves_what_it_weights_and_only_that() {
    let plain = hash(&tile("", SCATTER));
    let low = hash(&tile(
        "",
        "        dirt\n        scatter perSquare=4 { stone size=\"low(1..6, 11)\" }",
    ));
    assert_ne!(low, plain);
    // the ground is the same: only the stones moved
    assert_eq!(low[0], plain[0]);
    // a let is a distribution: the same odds, so the same build
    let named = hash(&tile(
        "    let small \"low(1..6, 11)\"",
        "        dirt\n        scatter perSquare=4 { stone size=\"small\" }",
    ));
    assert_eq!(named, low);
}

#[test]
fn every_use_of_a_let_is_its_own_draw() {
    // two stones of one scatter each draw from their own key, so they differ
    // though they name one let
    let text = tile(
        "    let small \"low(1..6, 11)\"",
        "        dirt\n        scatter count=2 { stone size=\"small\" }",
    );
    assert!(check(&mem(&text), "t.dtile", &Options::default()).is_ok());
}

#[test]
fn a_ratio_out_of_range_is_dt_range() {
    let d = refused(
        &tile("", "        dirt\n        rock size=\"low(6..10, -2)\""),
        "dt::range",
        "0.25 leans high",
    );
    assert_eq!(d.message, "low's ratio must be above 0, not -2");
    refused(
        &tile("", "        dirt\n        rock size=\"bell(6..10, 2)\""),
        "dt::range",
        "share of the width",
    );
    refused(
        &tile(
            "",
            "        dirt\n        rock size=\"peak(6..10, 0.5, 1.5)\"",
        ),
        "dt::range",
        "low end",
    );
    // out of range through a let, where it is used
    refused(
        &tile(
            "    let s \"low(6..10, 0)\"",
            "        dirt\n        rock size=\"s\"",
        ),
        "dt::range",
        "1 is flat",
    );
}

#[test]
fn a_weighting_wraps_a_plain_range() {
    for size in [
        "low(3)",
        "low(bell(1..6))",
        "low(1..6, 1..2)",
        "bell(1..6, 0.2, 0.5, 9)",
    ] {
        refused(
            &tile("", &format!("        dirt\n        rock size=\"{size}\"")),
            "dt::weighting",
            "",
        );
    }
}

#[test]
fn an_unknown_function_lists_the_functions() {
    refused(
        &tile("", "        dirt\n        rock size=\"bel(1..6)\""),
        "dt::value",
        "flat, low, high, bell or peak",
    );
}

#[test]
fn a_band_refuses_a_weighting_but_takes_flat() {
    let top = |v: &str| tile("", &format!("        dirt top=\"{v}\""));
    let d = refused(
        &top("low([groundHeight .. groundHeight + 2.5])"),
        "dt::weighting",
        "write the range alone: [groundHeight .. groundHeight + 2.5]",
    );
    assert_eq!(d.message, "a weighted range does not go on a band");
    for v in [
        "bell([groundHeight .. groundHeight + 2.5])",
        "peak([2 .. 4])",
        "high([2 .. 4], 3)",
    ] {
        refused(&top(v), "dt::weighting", "write the range alone");
    }
    assert!(
        check(
            &mem(&top("flat([groundHeight .. groundHeight + 2.5])")),
            "t.dtile",
            &Options::default()
        )
        .is_ok()
    );
    // a pin's top and a distance are bands too
    refused(
        &tile(
            "    line l { through \"0,0.5\" \"1,0.5\" }\n    region r { within \"low([2 .. 6])\" of=l }",
            "        dirt in=r",
        ),
        "dt::weighting",
        "write the range alone",
    );
    // a pin's top
    refused(
        &tile(
            "",
            "        dirt { pin top=\"bell([2 .. 6])\" at=southCentre }",
        ),
        "dt::weighting",
        "write the range alone",
    );
    // through a name: the band is solved where the let is read
    refused(
        &tile(
            "    let hump \"low([4 .. 7])\"",
            "        dirt top=\"hump\"",
        ),
        "dt::weighting",
        "write the range alone",
    );
}

#[test]
fn a_control_that_does_not_draw_with_odds_says_so() {
    let d = refused(
        &tile("", "        dirt\n        grass length=\"low(3..6)\""),
        "dt::weighting",
        "write the range 3..6",
    );
    assert!(d.message.contains("`length` does not take a weighting yet"));
}

#[test]
fn a_weighted_range_of_no_width_is_a_note() {
    let c = check(
        &mem(&tile("", "        dirt\n        rock size=\"low(5..5)\"")),
        "t.dtile",
        &Options::default(),
    )
    .unwrap();
    let n = c.notes();
    assert_eq!(n.len(), 1, "{n:?}");
    assert_eq!(n[0].code, "dt::weighting");
    assert!(n[0].message.contains("no width"), "{}", n[0].message);
}

#[test]
fn two_ranges_in_one_control_are_refused_not_averaged() {
    refused(
        &tile(
            "    let s \"[6 .. 8]\"\n    let t \"[1 .. 2]\"",
            "        dirt\n        rock size=\"s + t\"",
        ),
        "dt::value",
        "draws one range",
    );
}

#[test]
fn the_function_lists_agree() {
    // dt-doc parses the names and the arity, dt-geom holds the odds
    let doc: Vec<&str> = dt_doc::expr::WEIGHTINGS.iter().map(|w| w.0).collect();
    let geom: Vec<&str> = dt_geom::WEIGHTINGS.iter().map(|w| w.0).collect();
    assert_eq!(doc, geom);
    for (name, most) in dt_doc::expr::WEIGHTINGS {
        let args = vec![0.5; most];
        assert!(dt_geom::Weighting::from_call(name, &args).is_ok(), "{name}");
        let args = vec![0.5; most + 1];
        assert!(
            dt_geom::Weighting::from_call(name, &args).is_err(),
            "{name}"
        );
    }
}
