//! Weighted ranges end to end: a document that writes one builds with its
//! odds; a flat one is the plain range to the last bit; every refusal is a
//! `dt::` diagnostic with its code and a help line.

#![allow(clippy::unwrap_used)]

use dt_eval::{Diag, Memory, Options, build, check};

/// These tests write format 0 documents: built as written, not migrated as
/// they load (that is `migrate.rs`'s business).
fn legacy() -> Options {
    Options {
        current_format: 0,
        ..Options::default()
    }
}

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
    match check(&mem(text), "t.dtile", &legacy()) {
        Ok(c) => panic!("expected an error; notes {:?}", c.notes()),
        Err(e) => e,
    }
}

/// Each stage's hash, from a draft: a draw moves a draft as it moves a full
/// build, at a fraction of the time.
fn hash(text: &str) -> Vec<(String, u64)> {
    let draft = Options {
        draft: true,
        ..legacy()
    };
    let b = build(&mem(text), "t.dtile", &draft).unwrap_or_else(|e| panic!("{e:?}"));
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
    assert!(check(&mem(&text), "t.dtile", &legacy()).is_ok());
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
        "flat, low, high, bell, peak, ends, steps or mix, or a list like [1:4, 2:5]",
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
            &legacy()
        )
        .is_ok()
    );
    // a pin's top and a distance are bands too
    refused(
        &tile(
            "    line l { through \"0,0.5\" \"1,0.5\" }\n    region r { within \"low([2 .. 6])\" of=l }",
            "        dirt at=r",
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
fn a_weighted_range_of_no_width_is_a_note() {
    let c = check(
        &mem(&tile("", "        dirt\n        rock size=\"low(5..5)\"")),
        "t.dtile",
        &legacy(),
    )
    .unwrap();
    let n = c.notes();
    assert_eq!(n.len(), 1, "{n:?}");
    assert_eq!(n[0].code, "dt::weighting");
    assert!(n[0].message.contains("no width"), "{}", n[0].message);
}

#[test]
fn two_ranges_in_one_control_roll_apart() {
    let one = hash(&tile("", "        dirt\n        rock size=\"6..10\""));
    // one range moved by a number is the same draw to the last bit
    let text = tile(
        "    let s \"[6 .. 10]\"",
        "        dirt\n        rock size=\"s + 0\"",
    );
    assert_eq!(hash(&text), one);
    // two ranges build (they were refused once), and are not one range
    let text = tile(
        "    let s \"[3 .. 5]\"",
        "        dirt\n        rock size=\"s + s\"",
    );
    let two = hash(&text);
    assert_ne!(two, one);
    // editing one value moves nothing drawn after it: the ground is the same
    assert_eq!(two[0], one[0]);
    // a plain sum written out builds too, in every spelling
    for sum in [
        "flat(3..5) + flat(3..5)",
        "[3 .. 5] + [3 .. 5]",
        "3..5 + 3..5",
        "[3 .. 5] + 3..5",
    ] {
        let text = tile("", &format!("        dirt\n        rock size=\"{sum}\""));
        assert_eq!(hash(&text), two, "{sum}");
    }
    // a bracket reads anywhere in a control value, as in a height
    assert_eq!(
        hash(&tile("", "        dirt\n        rock size=\"[6 .. 10]\"")),
        one
    );
}

#[test]
fn the_function_lists_agree() {
    // dt-doc parses the names and the arity, dt-geom holds the odds
    let doc: Vec<&str> = dt_doc::expr::WEIGHTINGS.iter().map(|w| w.0).collect();
    let geom: Vec<&str> = dt_geom::WEIGHTINGS
        .iter()
        .map(|w| w.name)
        .filter(|n| *n != "list")
        .collect();
    assert_eq!(doc, geom);
    for (name, most) in dt_doc::expr::WEIGHTINGS {
        match name {
            // any number of weights, up to the cap, and parts with weights
            "steps" => {
                assert!(dt_geom::Weighting::from_call(name, &[1.0; 3]).is_ok());
                assert!(dt_geom::Weighting::from_call(name, &[1.0; 33]).is_err());
            }
            "mix" => {}
            _ => {
                let args = vec![0.5; most];
                assert!(dt_geom::Weighting::from_call(name, &args).is_ok(), "{name}");
                let args = vec![0.5; most + 1];
                assert!(
                    dt_geom::Weighting::from_call(name, &args).is_err(),
                    "{name}"
                );
            }
        }
    }
}

const WHOLE_SET: [&str; 4] = [
    "ends(6..10, 2, 0.25)",
    "steps(6..10, 1, 4, 2)",
    "[6, 8:3, 10]",
    "mix(bell(6..10), 3, flat(6..10), 1)",
];

#[test]
fn the_whole_set_builds_each_its_own_draw() {
    let plain = hash(&tile("", "        dirt\n        rock size=\"6..10\""));
    let mut seen = vec![plain.clone()];
    for size in WHOLE_SET {
        let h = hash(&tile(
            "",
            &format!("        dirt\n        rock size=\"{size}\""),
        ));
        // the ground is the same: only the rock moved, and it moved differently each time
        assert_eq!(h[0], plain[0], "{size}");
        assert!(!seen.contains(&h), "{size}");
        seen.push(h);
    }
    // a let holds a list or a mix like any distribution
    let named = hash(&tile(
        "    let pick \"[6, 8:3, 10]\"",
        "        dirt\n        rock size=\"pick\"",
    ));
    assert_eq!(named, seen[3]);
    // and they scale and shift with the range, and add as dice
    for size in [
        "2 * [3, 4:3, 5]",
        "[3, 4:3, 5] * 2",
        "mix(bell(6..10), 3, flat(6..10), 1) + 0",
    ] {
        hash(&tile(
            "",
            &format!("        dirt\n        rock size=\"{size}\""),
        ));
    }
    // a list on a count picks among the numbers written
    let count = |v: &str| {
        hash(&tile(
            "",
            &format!("        dirt\n        scatter count=\"{v}\" {{ stone size=\"2..4\" }}"),
        ))
    };
    assert_ne!(count("[1, 3:2, 5]"), count("1..5"));
    count("ends(1..5, 1, 0)");
    count("steps(1..5, 1, 0, 3)");
}

#[test]
fn a_list_listing_a_value_twice_is_dt_weighting() {
    let d = refused(
        &tile("", "        dirt\n        rock size=\"[6:4, 8:5, 8:1]\""),
        "dt::weighting",
        "give each value once, with its total weight: [6:4, 8:6]",
    );
    assert_eq!(d.message, "8 is listed twice in [6:4, 8:5, 8:1]");
}

#[test]
fn the_new_functions_check_their_limits() {
    let rock = |size: &str| tile("", &format!("        dirt\n        rock size=\"{size}\""));
    let many = |n: usize| vec!["1"; n].join(", ");
    for (size, message, help) in [
        (
            "ends(6..10, 9)",
            "ends' power must be from 0 to 8, not 9",
            "0 is flat",
        ),
        (
            "ends(6..10, 1, 2)",
            "ends' centre must be from 0 to 1, not 2",
            "low end",
        ),
        (
            "steps(6..10, 3)",
            "`steps` needs two weights at least",
            "one weight per bin",
        ),
        (
            "steps(6..10, 0, 0)",
            "`steps` needs a weight above 0",
            "above 0",
        ),
        (
            "steps(6..10, 1, -1)",
            "a weight in `steps` must be 0 or more, not -1",
            "weight is",
        ),
        ("[6:0, 8:0]", "a list needs a weight above 0", "above 0"),
        (
            "[6:-1, 8]",
            "a weight in a list must be 0 or more, not -1",
            "weight is",
        ),
        (
            "mix(bell(6..10), 0, flat(6..10), 0)",
            "`mix` needs a weight above 0",
            "above 0",
        ),
        (
            "mix(bell(6..10, 5), 1, flat(6..10), 1)",
            "bell's spread must be from 0.05 to 1, not 5",
            "share of the width",
        ),
    ] {
        let d = refused(&rock(size), "dt::range", help);
        assert_eq!(d.message, message, "{size}");
    }
    // the caps: 32 bins, 64 values, 16 parts
    assert!(
        check(
            &mem(&rock(&format!("steps(6..10, {})", many(32)))),
            "t.dtile",
            &legacy()
        )
        .is_ok()
    );
    refused(
        &rock(&format!("steps(6..10, {})", many(33))),
        "dt::range",
        "fewer, wider bins",
    );
    assert!(
        check(
            &mem(&rock(&format!(
                "[{}]",
                (1..=64)
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
            "t.dtile",
            &legacy()
        )
        .is_ok()
    );
    refused(
        &rock(&format!(
            "[{}]",
            (1..=65)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        "dt::range",
        "steps(",
    );
    let parts = |n: usize| vec!["flat(6..10), 1"; n].join(", ");
    assert!(
        check(
            &mem(&rock(&format!("mix({})", parts(16)))),
            "t.dtile",
            &legacy()
        )
        .is_ok()
    );
    refused(
        &rock(&format!("mix({})", parts(17))),
        "dt::range",
        "fewer parts",
    );
    // mix: the parts share one range, none nests, a part is a call
    for (size, what) in [
        (
            "mix(bell(6..10), 1, flat(6..9), 1)",
            "write 6..10 in every part",
        ),
        (
            "mix(bell(6..10), 1, mix(low(6..10), 1, flat(6..10), 1), 1)",
            "weightings do not nest",
        ),
        (
            "mix(6..10, 1, flat(6..10), 1)",
            "each part, then its weight",
        ),
        ("mix(bell(6..10), flat(6..10))", "write mix(bell(1..6), 3"),
        ("low([6, 8])", "the first thing in the call is a range"),
    ] {
        refused(&rock(size), "dt::weighting", what);
    }
}

#[test]
fn a_band_refuses_a_list_a_steps_an_ends_and_a_mix() {
    let top = |v: &str| tile("", &format!("        dirt top=\"{v}\""));
    for v in [
        "[2, 4]",
        "[2:3, 4]",
        "ends([2 .. 4])",
        "steps([2 .. 4], 1, 2)",
        "mix(low([2 .. 4]), 1, flat([2 .. 4]), 1)",
    ] {
        refused(&top(v), "dt::weighting", "write the range alone");
    }
}

/// A flat spelling of a range builds as the plain range does, the same range
/// with odds builds differently, and nothing is refused: the control draws.
fn takes_a_weighting(stack: impl Fn(&str) -> String, plain: &str, flat: &str, weighted: &str) {
    let base = hash(&tile("", &stack(plain)));
    assert_eq!(hash(&tile("", &stack(flat))), base, "{flat}");
    assert_ne!(hash(&tile("", &stack(weighted))), base, "{weighted}");
}

#[test]
fn how_deep_a_rock_is_buried_takes_a_weighting() {
    takes_a_weighting(
        |v| {
            format!(
                "        dirt\n        scatter perSquare=20 {{ pebble size=\"1..2.2\" buried=\"{v}\" }}"
            )
        },
        "20%..50%",
        "low(20%..50%, 1)",
        "low(20%..50%, 8)",
    );
}

#[test]
fn a_points_range_takes_a_weighting() {
    takes_a_weighting(
        |v| format!("        dirt\n        rock size=6 at=\"{v}\""),
        "0.3..0.7,0.5",
        "low(0.3..0.7, 1),0.5",
        "low(0.3..0.7, 9),0.5",
    );
    // the commas inside a call or a list are not the pair's
    for at in [
        "mix(bell(0.3..0.7), 1, flat(0.3..0.7), 1), 0.5",
        "[0.3, 0.6:3], high(0.4..0.6, 3)",
        "steps(0.3..0.7, 1, 3), ends(0.4..0.6)",
        "0.5, [0.4, 0.5:2, 0.6]",
    ] {
        hash(&tile(
            "",
            &format!("        dirt\n        rock size=6 at=\"{at}\""),
        ));
    }
    refused(
        &tile(
            "",
            "        dirt\n        rock size=6 at=\"low(0.3..0.7, -1),0.5\"",
        ),
        "dt::range",
        "1 is flat",
    );
}

#[test]
fn a_walls_courses_and_bays_take_a_weighting() {
    takes_a_weighting(
        |v| {
            format!(
                "        dirt\n        wall at=\"north\" style=fieldstone {{\n            stones height=\"2.2..5.2\" length=\"{v}\"\n        }}"
            )
        },
        "5..16",
        "low(5..16, 1)",
        "low(5..16, 8)",
    );
    takes_a_weighting(
        |v| {
            format!(
                "        dirt\n        wall at=\"north\" style=fieldstone {{\n            stones height=\"{v}\" length=\"5..16\"\n        }}"
            )
        },
        "2.2..5.2",
        "high(2.2..5.2, 1)",
        "high(2.2..5.2, 8)",
    );
}

#[test]
fn a_blades_length_and_height_take_a_weighting() {
    takes_a_weighting(
        |v| format!("        dirt\n        grass length=\"{v}\""),
        "3.5..6.5",
        "low(3.5..6.5, 1)",
        "low(3.5..6.5, 8)",
    );
    takes_a_weighting(
        |v| format!("        dirt\n        grass height=\"{v}\""),
        "1.5..2",
        "high(1.5..2, 1)",
        "high(1.5..2, 8)",
    );
}

#[test]
fn a_trees_heights_and_a_cliffs_patches_take_a_weighting() {
    let tree = |v: &str| format!("        dirt\n        tree {v}");
    takes_a_weighting(
        |v| tree(&format!("top=\"bottom + {v}\"")),
        "[36 .. 40]",
        "low(36..40, 1)",
        "low(36..40, 8)",
    );
    // the rest read and build too
    hash(&tile(
        "",
        &tree(
            "top=\"bottom + 38\" { trunk height=\"low(26..32)\"; crown width=24 clumpSize=\"[4, 5:3, 6]\"; leaf length=\"bell(4..5)\" width=3 }",
        ),
    ));
    let cliff = |v: &str| {
        format!(
            "tile size=\"2x1\" seed=1 {{\n    region crag {{ within \"[10 .. 20]\" of=northEdge }}\n    stack {{\n        base\n        dirt top=4\n        cliff top=\"dirtEdgeHeight + 35\" at=crag {{\n            rock mixed\n            shattered patches=3 width=\"{v}\"\n        }}\n    }}\n}}\n"
        )
    };
    let base = hash(&cliff("5..10"));
    assert_eq!(hash(&cliff("low(5..10, 1)")), base);
    assert_ne!(hash(&cliff("low(5..10, 8)")), base);
}
