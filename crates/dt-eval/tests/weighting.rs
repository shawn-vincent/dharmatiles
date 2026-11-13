//! Weighted ranges end to end: a document that writes one builds with its
//! odds; a flat one is the plain range to the last bit; every refusal is a
//! `dt::` diagnostic with its code and a help line.

#![allow(clippy::unwrap_used)]

mod common;

use dt_eval::{Diag, Memory, Options, build, check};

/// These tests write their documents in format 1 and read them from memory.
fn legacy() -> Options {
    Options::default()
}

fn mem(text: &str) -> Memory {
    common::memory(&[("t.dtile", text)])
}

fn tile(lets: &str, stack: &str) -> String {
    format!(
        "dharmatiles version=\"1.0.0\"; tile size=\"1x1\" seed=7 {{\n{lets}\n    stack {{\n{stack}\n    }}\n}}\n"
    )
}

/// A range's ends and a list's values written with their unit (`6..10` is
/// `6dmm..10dmm`, `[6:2, 8]` is `[6dmm:2, 8dmm]`): the weighting tests below
/// write their mistakes with plain numbers and give them units here.
fn units(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut depth = 0_i32;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
        let starts_number = c.is_ascii_digit()
            && (i == 0
                || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '.')
                || i >= 2 && chars[i - 2] == '.');
        if starts_number {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
            {
                i += 1;
            }
            out += &chars[start..i].iter().collect::<String>();
            let rest: String = chars[i..].iter().collect();
            let before: String = chars[..start].iter().collect();
            let (rest, before) = (rest.trim_start(), before.trim_end());
            let range_end = rest.starts_with("..") || before.ends_with("..");
            let list_item = depth > 0 && !before.ends_with(':') && !before.ends_with(":-");
            if range_end || list_item {
                out += "dmm";
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
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

const SCATTER: &str = "        dirt\n        scatter perSquare=4 { stone size=\"1dmm .. 6dmm\" }";

#[test]
fn a_flat_weighting_is_the_plain_range_to_the_last_bit() {
    let plain = hash(&tile(
        "",
        "        dirt\n        rock size=\"6dmm .. 10dmm\"",
    ));
    for same in [
        "flat(6dmm .. 10dmm)",
        "flat(6dmm .. 10dmm)",
        "low(6dmm .. 10dmm, 1)",
        "high(6dmm .. 10dmm, 1)",
    ] {
        let text = tile("", &format!("        dirt\n        rock size=\"{same}\""));
        assert_eq!(hash(&text), plain, "{same}");
    }
    // and a let holding the plain range draws the same
    let text = tile(
        "    let fat \"[6dmm .. 10dmm]\"",
        "        dirt\n        rock size=\"fat\"",
    );
    assert_eq!(hash(&text), plain);
}

#[test]
fn a_weighting_moves_what_it_weights_and_only_that() {
    let plain = hash(&tile("", SCATTER));
    let low = hash(&tile(
        "",
        "        dirt\n        scatter perSquare=4 { stone size=\"low(1dmm .. 6dmm, 11)\" }",
    ));
    assert_ne!(low, plain);
    // the ground is the same: only the stones moved
    assert_eq!(low[0], plain[0]);
    // a let is a distribution: the same odds, so the same build
    let named = hash(&tile(
        "    let small \"low([1dmm .. 6dmm], 11)\"",
        "        dirt\n        scatter perSquare=4 { stone size=\"small\" }",
    ));
    assert_eq!(named, low);
}

#[test]
fn every_use_of_a_let_is_its_own_draw() {
    // two stones of one scatter each draw from their own key, so they differ
    // though they name one let
    let text = tile(
        "    let small \"low([1dmm .. 6dmm], 11)\"",
        "        dirt\n        scatter count=2 { stone size=\"small\" }",
    );
    assert!(check(&mem(&text), "t.dtile", &legacy()).is_ok());
}

#[test]
fn a_ratio_out_of_range_is_dt_range() {
    let d = refused(
        &tile(
            "",
            "        dirt\n        rock size=\"low(6dmm .. 10dmm, -2)\"",
        ),
        "dt::range",
        "0.25 leans high",
    );
    assert_eq!(d.message, "low's ratio must be above 0, not -2");
    refused(
        &tile(
            "",
            "        dirt\n        rock size=\"bell(6dmm .. 10dmm, 2)\"",
        ),
        "dt::range",
        "share of the width",
    );
    refused(
        &tile(
            "",
            "        dirt\n        rock size=\"peak(6dmm .. 10dmm, 0.5, 1.5)\"",
        ),
        "dt::range",
        "low end",
    );
    // out of range through a let, where it is used
    refused(
        &tile(
            "    let s \"low([6dmm .. 10dmm], 0)\"",
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
            &tile(
                "",
                &format!("        dirt\n        rock size=\"{}\"", units(size)),
            ),
            "dt::weighting",
            "",
        );
    }
}

#[test]
fn an_unknown_function_lists_the_functions() {
    refused(
        &tile("", "        dirt\n        rock size=\"bel(1dmm..6dmm)\""),
        "dt::value",
        "flat, low, high, bell, peak, ends, steps or mix, or a list like [1:4, 2:5]",
    );
}

#[test]
fn a_band_takes_a_weighting_as_a_cloud() {
    let ok = |text: String| {
        check(&mem(&text), "t.dtile", &legacy()).unwrap_or_else(|e| panic!("{text}\n{e:?}"))
    };
    let top = |v: &str| tile("", &format!("        dirt top=\"{v}\""));
    for v in [
        "low([$groundHeight .. $groundHeight + 2.5dmm])",
        "bell([$groundHeight .. $groundHeight + 2.5dmm])",
        "peak([2dmm .. 4dmm])",
        "high([2dmm .. 4dmm], 3)",
        "flat([$groundHeight .. $groundHeight + 2.5dmm])",
    ] {
        ok(top(v));
    }
    // a pin's top is a cloud too
    ok(tile(
        "",
        "        dirt { pin top=\"bell([2dmm .. 6dmm])\" at=southCenter }",
    ));
    // through a name: the cloud is solved where the let is read
    ok(tile(
        "    let hump \"low([4dmm .. 7dmm])\"",
        "        dirt top=\"hump\"",
    ));
    // a distance is a band the solver reads, not a draw: still refused
    let d = refused(
        &tile(
            "    line l { through \"(0sq, 0.5sq)\" \"(1sq, 0.5sq)\" }\n    region r { within \"low([2dmm .. 6dmm])\" of=l }",
            "        dirt at=r",
        ),
        "dt::weighting",
        "write the range alone",
    );
    assert_eq!(d.message, "a weighted range does not go on a distance");
}

#[test]
fn a_weighted_range_of_no_width_is_a_note() {
    let c = check(
        &mem(&tile(
            "",
            "        dirt\n        rock size=\"low(5dmm .. 5dmm)\"",
        )),
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
    let one = hash(&tile(
        "",
        "        dirt\n        rock size=\"6dmm .. 10dmm\"",
    ));
    // one range moved by a number is the same draw to the last bit
    let text = tile(
        "    let s \"[6dmm .. 10dmm]\"",
        "        dirt\n        rock size=\"s + 0dmm\"",
    );
    assert_eq!(hash(&text), one);
    // two ranges build (they were refused once), and are not one range
    let text = tile(
        "    let s \"[3dmm .. 5dmm]\"",
        "        dirt\n        rock size=\"s + s\"",
    );
    let two = hash(&text);
    assert_ne!(two, one);
    // editing one value moves nothing drawn after it: the ground is the same
    assert_eq!(two[0], one[0]);
    // a plain sum written out builds too, in every spelling
    for sum in [
        "flat(3dmm .. 5dmm) + flat(3dmm .. 5dmm)",
        "3dmm..5dmm + 3dmm..5dmm",
        "3dmm..5dmm + 3dmm..5dmm",
        "3dmm..5dmm + 3dmm..5dmm",
    ] {
        let text = tile("", &format!("        dirt\n        rock size=\"{sum}\""));
        assert_eq!(hash(&text), two, "{sum}");
    }
    // a bracket reads anywhere in a control value, as in a height
    assert_eq!(
        hash(&tile(
            "",
            "        dirt\n        rock size=\"6dmm .. 10dmm\""
        )),
        one
    );
}

const WHOLE_SET: [&str; 4] = [
    "ends(6dmm .. 10dmm, 2, 0.25)",
    "steps(6dmm .. 10dmm, 1, 4, 2)",
    "[6dmm, 8dmm:3, 10dmm]",
    "mix(bell(6dmm .. 10dmm), 3, flat(6dmm .. 10dmm), 1)",
];

#[test]
fn the_whole_set_builds_each_its_own_draw() {
    let plain = hash(&tile(
        "",
        "        dirt\n        rock size=\"6dmm .. 10dmm\"",
    ));
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
        "    let pick \"[6dmm, 8dmm:3, 10dmm]\"",
        "        dirt\n        rock size=\"pick\"",
    ));
    assert_eq!(named, seen[3]);
    // and they scale and shift with the range, and add as dice
    for size in [
        "2dmm * [3, 4:3, 5]",
        "[3dmm, 4dmm:3, 5dmm] * 2",
        "mix(bell(6dmm .. 10dmm), 3, flat(6dmm .. 10dmm), 1) + 0dmm",
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
            &format!(
                "        dirt\n        scatter count=\"{v}\" {{ stone size=\"2dmm .. 4dmm\" }}"
            ),
        ))
    };
    assert_ne!(count("[1, 3:2, 5]"), count("1..5"));
    count("ends(1..5, 1, 0)");
    count("steps(1..5, 1, 0, 3)");
}

#[test]
fn a_list_listing_a_value_twice_is_dt_weighting() {
    let d = refused(
        &tile(
            "",
            "        dirt\n        rock size=\"[6dmm:4, 8dmm:5, 8dmm:1]\"",
        ),
        "dt::weighting",
        "give each value once, with its total weight: [6:4, 8:6]",
    );
    assert_eq!(d.message, "8 is listed twice in [6:4, 8:5, 8:1]");
}

#[test]
fn the_new_functions_check_their_limits() {
    let rock = |size: &str| {
        tile(
            "",
            &format!("        dirt\n        rock size=\"{}\"", units(size)),
        )
    };
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
            "in every part, so the parts blend over the same values",
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
fn a_band_takes_a_list_a_steps_an_ends_and_a_mix() {
    let top = |v: &str| tile("", &format!("        dirt top=\"{v}\""));
    for v in [
        "[2dmm, 4dmm]",
        "[2dmm:3, 4dmm]",
        "ends([2dmm .. 4dmm])",
        "steps([2dmm .. 4dmm], 1, 2)",
        "mix(low([2dmm .. 4dmm]), 1, flat([2dmm .. 4dmm]), 1)",
    ] {
        let text = top(v);
        check(&mem(&text), "t.dtile", &legacy()).unwrap_or_else(|e| panic!("{text}\n{e:?}"));
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
                "        dirt\n        scatter perSquare=20 {{ pebble size=\"1dmm .. 2.2dmm\" buried=\"{v}\" }}"
            )
        },
        "20%..50%",
        "low(20%..50%, 1)",
        "low(20%..50%, 8)",
    );
}

#[test]
fn a_walls_courses_and_bays_take_a_weighting() {
    takes_a_weighting(
        |v| {
            format!(
                "        dirt\n        wall at=\"north\" style=fieldstone {{\n            stones height=\"2.2dmm .. 5.2dmm\" length=\"{v}\"\n        }}"
            )
        },
        "5dmm .. 16dmm",
        "low(5dmm .. 16dmm, 1)",
        "low(5dmm .. 16dmm, 8)",
    );
    takes_a_weighting(
        |v| {
            format!(
                "        dirt\n        wall at=\"north\" style=fieldstone {{\n            stones height=\"{v}\" length=\"5dmm .. 16dmm\"\n        }}"
            )
        },
        "2.2dmm .. 5.2dmm",
        "high(2.2dmm .. 5.2dmm, 1)",
        "high(2.2dmm .. 5.2dmm, 8)",
    );
}

#[test]
fn a_blades_length_and_height_take_a_weighting() {
    takes_a_weighting(
        |v| format!("        dirt\n        grass length=\"{v}\""),
        "3.5dmm .. 6.5dmm",
        "low(3.5dmm .. 6.5dmm, 1)",
        "low(3.5dmm .. 6.5dmm, 8)",
    );
    takes_a_weighting(
        |v| format!("        dirt\n        grass height=\"{v}\""),
        "1.5dmm .. 2dmm",
        "high(1.5dmm .. 2dmm, 1)",
        "high(1.5dmm .. 2dmm, 8)",
    );
}

#[test]
fn a_trees_heights_and_a_cliffs_patches_take_a_weighting() {
    let tree = |v: &str| format!("        dirt\n        tree {v}");
    takes_a_weighting(
        |v| tree(&format!("top=\"$bottom + {v}\"")),
        "[46dmm .. 50dmm]",
        "low([46dmm .. 50dmm], 1)",
        "low([46dmm .. 50dmm], 8)",
    );
    // the rest read and build too
    hash(&tile(
        "",
        &tree(
            "top=\"$bottom + 48dmm\" { trunk height=\"low(26dmm .. 32dmm)\"; crown width=\"24dmm\" clumpSize=\"[4dmm, 5dmm:3, 6dmm]\"; leaf length=\"bell(4dmm .. 5dmm)\" width=\"3dmm\" }",
        ),
    ));
    let cliff = |v: &str| {
        format!(
            "dharmatiles version=\"1.0.0\"; tile size=\"2x1\" seed=1 {{\n    region crag {{ within \"[10dmm .. 20dmm]\" of=northEdge }}\n    stack {{\n        base\n        dirt top=\"4dmm\"\n        cliff top=\"$dirtEdgeHeight + 35dmm\" at=crag {{\n            rock mixed\n            shattered patches=3 width=\"{v}\"\n        }}\n    }}\n}}\n"
        )
    };
    let base = hash(&cliff("5dmm .. 10dmm"));
    assert_eq!(hash(&cliff("low(5dmm .. 10dmm, 1)")), base);
    assert_ne!(hash(&cliff("low(5dmm .. 10dmm, 8)")), base);
}
