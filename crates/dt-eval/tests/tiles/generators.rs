//! Generators: a parameter that holds a thing.  What the reader makes of a
//! thing passed by name, where it is refused, and how it is seeded.  These
//! read documents (`check`); nothing here builds.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failing unwrap is the failure"
)]

use crate::common;

use dt_eval::{Diag, Options, check};

fn doc(parts: &str, stack: &str) -> String {
    format!(
        "dharmatiles version=\"1.0.0\"\n{parts}\ntile size=\"1x1\" seed=4 {{\n    stack {{\n        base\n        dirt\n{stack}\n    }}\n}}\n"
    )
}

fn read(text: &str) -> Result<dt_eval::Checked, Vec<Diag>> {
    check(
        &common::memory(&[("t.dtile", text)]),
        "t.dtile",
        &Options::default(),
    )
}

fn refused(text: &str) -> Vec<Diag> {
    match read(text) {
        Ok(_) => panic!("expected a refusal:\n{text}"),
        Err(e) => e,
    }
}

fn says(errors: &[Diag], code: &str, want: &str) {
    assert!(
        errors.iter().any(|e| e.code == code
            && (e.message.contains(want) || e.help.as_deref().is_some_and(|h| h.contains(want)))),
        "wanted {code} saying `{want}`, got {:#?}",
        errors
            .iter()
            .map(|e| (e.code, &e.message, &e.help))
            .collect::<Vec<_>>()
    );
}

const STAND: &str = "define stand {
    param piece pebble
    stack {
        piece at=\"(0.3sq, 0.5sq)\"
        scatter count=2 { piece }
    }
}";

#[test]
fn a_thing_param_stands_for_its_default_and_for_what_the_caller_passes() {
    let c = read(&doc(STAND, "        stand\n        stand piece=boulder")).unwrap();
    let t = c.tile.unwrap();
    let names: Vec<&str> = t.stack.iter().map(|e| e.what.value.as_str()).collect();
    assert!(
        names.contains(&"pebble") && names.contains(&"boulder"),
        "{names:?}"
    );
    let kids: Vec<&str> = t
        .stack
        .iter()
        .filter(|e| e.what.value == "scatter")
        .map(|e| e.children[0].what.value.as_str())
        .collect();
    assert_eq!(kids, ["pebble", "boulder"]);
}

#[test]
fn a_definition_can_be_the_thing() {
    let parts = "define boulder-ish { stack { boulder } }
define grove {
    param shape boulder-ish
    stack { shape at=\"(0.5sq, 0.5sq)\" }
}";
    let t = read(&doc(parts, "        grove")).unwrap().tile.unwrap();
    assert!(t.stack.iter().any(|e| e.what.value == "boulder"));
}

#[test]
fn a_property_written_where_it_is_used_applies_over_the_things_own() {
    let parts = "define stand {
    param piece pebble
    stack { piece size=\"1dmm .. 2dmm\" at=\"(0.5sq, 0.5sq)\" }
}";
    let t = read(&doc(parts, "        stand")).unwrap().tile.unwrap();
    let e = t.stack.iter().find(|e| e.what.value == "pebble").unwrap();
    let size = e.controls.iter().find(|c| c.name == "size").unwrap();
    assert!(
        matches!(
            size.value,
            dt_elements::Value::Range(..)
                | dt_elements::Value::Dist(..)
                | dt_elements::Value::Draw(..)
        ),
        "{:?}",
        size.value
    );
    // and one the thing does not take is refused at the property
    let bad = STAND.replace("piece at=", "piece wobble=3 at=");
    says(
        &refused(&doc(&bad, "        stand")),
        dt_lang::codes::UNKNOWN_CONTROL,
        "no control `wobble`",
    );
}

#[test]
fn a_layer_where_a_thing_is_wanted_is_refused_at_the_call() {
    let e = refused(&doc(STAND, "        stand piece=dirt"));
    says(&e, dt_lang::codes::TYPE, "`dirt` is a layer");
    assert!(
        e.iter()
            .any(|d| d.help.as_deref().is_some_and(|h| h.contains("stack")))
    );
}

#[test]
fn a_number_for_a_thing_is_refused() {
    says(
        &refused(&doc(STAND, "        stand piece=3")),
        dt_lang::codes::TYPE,
        "is a thing, and a number is not",
    );
}

#[test]
fn a_thing_for_a_number_is_refused() {
    let parts = "define sized {\n    param by \"8dmm\"\n    stack { rock size=by }\n}";
    says(
        &refused(&doc(parts, "        sized by=boulder")),
        dt_lang::codes::TYPE,
        "is a thing",
    );
}

#[test]
fn a_layer_written_as_a_scatter_child_names_its_kind() {
    says(
        &refused(&doc("", "        scatter count=2 { dirt }")),
        dt_lang::codes::SCATTER,
        "is a layer, and a scatter holds things",
    );
}

#[test]
fn a_thing_param_with_no_default_must_be_given() {
    let parts = "define stand {\n    param piece\n    stack { piece at=\"(0.5sq, 0.5sq)\" }\n}";
    says(
        &refused(&doc(parts, "        stand")),
        dt_lang::codes::MISSING_CONTROL,
        "needs `piece`",
    );
}

#[test]
fn an_unknown_thing_name_is_refused_at_the_call() {
    says(
        &refused(&doc(STAND, "        stand piece=nothing")),
        dt_lang::codes::UNKNOWN_ELEMENT,
        "no element or definition is called `nothing`",
    );
}

#[test]
fn a_thing_that_holds_itself_is_a_loop() {
    let parts = "define fern {\n    param leaf fern\n    stack { leaf at=\"(0.5sq, 0.5sq)\" }\n}";
    says(
        &refused(&doc(parts, "        fern")),
        dt_lang::codes::LOOP,
        "calls itself",
    );
}

/// The seeds of the entries of a tile, by element.
fn seeds(text: &str, what: &str) -> Vec<Option<u64>> {
    read(text)
        .unwrap()
        .tile
        .unwrap()
        .stack
        .iter()
        .filter(|e| e.what.value == what)
        .map(|e| e.seed)
        .collect()
}

#[test]
fn two_definitions_sharing_one_thing_draw_differently() {
    let parts = "define fern {\n    param leaf pebble\n    stack { leaf at=\"(0.3sq, 0.5sq)\" }\n}\ndefine shrub {\n    param leaf pebble\n    stack { leaf at=\"(0.7sq, 0.5sq)\" }\n}";
    // the same pin on two calls is the same path under each: pins are for pinning
    let s = seeds(
        &doc(parts, "        fern seed=3\n        shrub seed=3"),
        "pebble",
    );
    assert_eq!(s.len(), 2);
    assert_eq!(s[0], s[1]);
    // unpinned, each entry has its own place in the tile, so its own draw
    let c = read(&doc(parts, "        fern\n        shrub"))
        .unwrap()
        .tile
        .unwrap();
    let ids: Vec<String> = c
        .stack
        .iter()
        .filter(|e| e.what.value == "pebble")
        .map(|e| format!("{:?}", e.id))
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn a_call_pins_each_thing_by_its_path_and_a_sibling_does_not_move_it() {
    let one = "define cluster {\n    param bit pebble\n    stack {\n        bit at=\"(0.3sq, 0.5sq)\"\n        bit at=\"(0.6sq, 0.5sq)\"\n    }\n}";
    let two = "define cluster {\n    param bit pebble\n    stack {\n        rock at=\"(0.9sq, 0.9sq)\"\n        bit at=\"(0.3sq, 0.5sq)\"\n        bit at=\"(0.6sq, 0.5sq)\"\n    }\n}";
    let a = seeds(&doc(one, "        cluster seed=9"), "pebble");
    let b = seeds(&doc(two, "        cluster seed=9"), "pebble");
    assert_eq!(a.len(), 2);
    assert_ne!(a[0], a[1], "two entries of one call draw differently");
    assert_eq!(a, b, "a sibling written beside them moves neither");
}

#[test]
#[ignore = "thorough: cargo xtask check --full runs it (a test tile outside the torture set)"]
fn thorough_one_definition_builds_two_things() {
    use dt_kit::Material;
    let b = crate::memo::built("lib:tests/rocks/1x1-thing-param.dtile");
    assert!(b.audit.failures(&Options::default().printer).is_empty());
    assert!(b.materials.contains(&Material::Rock));
    // a pebble stand and a boulder stand: both were placed
    let placed = b.report.placed.len();
    assert!(placed >= 8, "{placed} things placed");
}
