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

/// A refusal as the user reads it: its code, words and the text it points at
/// (not where in the file: a document and its twin differ in offsets).
type Read = (&'static str, String, String, Option<String>, String);

fn reads(errors: &[Diag]) -> Vec<Read> {
    errors
        .iter()
        .map(|d| {
            let (src, span) = d.at.as_ref().unwrap();
            let text = &src.inner()[span.offset()..span.offset() + span.len()];
            (
                d.code,
                d.message.clone(),
                d.label.clone(),
                d.help.clone(),
                text.to_owned(),
            )
        })
        .collect()
}

/// `callee` written with `line` on its line and `block` as its child lines.
fn entry(callee: &str, line: &str, block: &str) -> String {
    let block = if block.is_empty() {
        String::new()
    } else {
        format!(" {{\n{block}\n        }}")
    };
    format!("{callee} {line} at=\"(0.5sq, 0.5sq)\"{block}")
}

/// A refusal of what is written on `thing`, and the same text written on a
/// thing parameter standing for it: the same code, words and text pointed
/// at, the latter inside the definition, where it is written.
fn same_refusal(thing: &str, line: &str, block: &str) -> Vec<Read> {
    let direct = refused(&doc("", &format!("        {}", entry(thing, line, block))));
    let parts = format!(
        "define stand {{\n    param piece {thing}\n    stack {{\n        {}\n    }}\n}}",
        entry("piece", line, block)
    );
    let text = doc(&parts, "        stand");
    let through = refused(&text);
    assert_eq!(
        reads(&through),
        reads(&direct),
        "`{thing} {line} {block}` through a thing parameter"
    );
    // pointing at the use site: the define's text, not the call's
    let call = text.rfind("stand").unwrap();
    for d in &through {
        let (_, span) = d.at.as_ref().unwrap();
        assert!(span.offset() < call, "{:?} points at the call", d.message);
    }
    reads(&direct)
}

#[test]
fn a_property_on_a_thing_param_is_refused_as_on_the_element() {
    use dt_lang::codes;
    let cases: [(&str, &str, &str); 8] = [
        // a length with no unit
        ("pebble", "size=5", codes::UNIT),
        // an angle where a length is
        ("pebble", "size=\"5deg\"", codes::TYPE),
        // out of its range
        ("pebble", "size=\"900dmm\"", codes::RANGE),
        // a percent past a hundred
        ("pebble", "buried=\"150%\"", codes::RANGE),
        // a word it does not have
        ("pebble", "shape=jagged", codes::CHOICE),
        // a control it does not have
        ("pebble", "wobble=3", codes::UNKNOWN_CONTROL),
        // written twice
        ("pebble", "size=\"2dmm\" size=\"3dmm\"", codes::DUPLICATE),
        // a group written as a property
        ("tree", "crown=1", codes::GROUP),
    ];
    for (thing, line, code) in cases {
        let r = same_refusal(thing, line, "");
        assert!(
            r.iter().any(|r| r.0 == code),
            "`{thing} {line}` wanted {code}, got {r:#?}"
        );
    }
}

#[test]
fn a_group_control_on_a_thing_param_is_refused_as_on_the_element() {
    use dt_lang::codes;
    let cases: [(&str, &str); 5] = [
        // a sub-control with no unit
        ("            crown width=5", codes::UNIT),
        // out of its range
        ("            crown width=\"900dmm\"", codes::RANGE),
        // a word it does not have
        ("            crown shape=pointy", codes::CHOICE),
        // a sub-control it does not have
        ("            crown wibble=3", codes::UNKNOWN_CONTROL),
        // a group given a value of its own
        ("            trunk 3", codes::GROUP),
    ];
    for (block, code) in cases {
        let r = same_refusal("tree", "", block);
        assert!(
            r.iter().any(|r| r.0 == code),
            "`{block}` wanted {code}, got {r:#?}"
        );
    }
}

#[test]
fn a_tree_passed_as_a_thing_takes_its_groups() {
    let parts = "define grove {
    param piece tree
    stack {
        piece at=\"(0.5sq, 0.5sq)\" {
            trunk height=\"10dmm\" width=\"3dmm\"
            crown width=\"20dmm\" shape=cone
        }
    }
}";
    let t = read(&doc(parts, "        grove")).unwrap().tile.unwrap();
    let tree = t.stack.iter().find(|e| e.what.value == "tree").unwrap();
    let names: Vec<&str> = tree.controls.iter().map(|c| c.name.as_str()).collect();
    for want in ["crown/width", "crown/shape", "trunk/height", "trunk/width"] {
        assert!(names.contains(&want), "{want} in {names:?}");
    }
    // the same as written on the tree itself
    let direct = read(&doc(
        "",
        "        tree at=\"(0.5sq, 0.5sq)\" {
            trunk height=\"10dmm\" width=\"3dmm\"
            crown width=\"20dmm\" shape=cone
        }",
    ))
    .unwrap()
    .tile
    .unwrap();
    let direct = direct
        .stack
        .iter()
        .find(|e| e.what.value == "tree")
        .unwrap();
    let show = |e: &dt_eval::doc::Entry| {
        e.controls
            .iter()
            .map(|c| format!("{} {:?}", c.name, c.value))
            .collect::<Vec<_>>()
    };
    assert_eq!(show(tree), show(direct));
}

#[test]
fn a_definition_passed_as_a_thing_types_its_arguments_as_a_call_does() {
    let parts = "define bit {
    param width \"3dmm\"
    stack { pebble size=width at=\"(0.5sq, 0.5sq)\" }
}
define stand {
    param piece bit
    stack { piece width=5 }
}
define direct { stack { bit width=5 } }";
    let through = refused(&doc(parts, "        stand"));
    let call = refused(&doc(parts, "        direct"));
    let one = |e: &[Diag]| {
        reads(e)
            .into_iter()
            .filter(|r| r.0 == dt_lang::codes::UNIT)
            .map(|r| (r.1, r.2, r.4))
            .collect::<Vec<_>>()
    };
    assert!(!one(&through).is_empty(), "{through:#?}");
    assert_eq!(one(&through), one(&call));
}

/// A definition `stand` with a thing param `piece` and a param `s` that
/// fills `piece`'s `control`, called with `s=arg` (or, with no `arg`, its
/// default `default`), through `levels` definitions that pass `s` on.
fn forwarded(control: &str, default: &str, arg: Option<&str>, levels: usize) -> String {
    let mut parts = format!(
        "define stand {{\n    param piece pebble\n    param s {default}\n    stack {{\n        piece {control}=s at=\"(0.5sq, 0.5sq)\"\n    }}\n}}\n"
    );
    let mut callee = "stand".to_owned();
    for k in 0..levels {
        let name = format!("pass{k}");
        parts += &format!(
            "define {name} {{\n    param piece pebble\n    param s {default}\n    stack {{ {callee} piece=piece s=s }}\n}}\n"
        );
        callee = name;
    }
    let call = arg.map_or(callee.clone(), |a| format!("{callee} s={a}"));
    doc(&parts, &format!("        {call}"))
}

/// The refusal of `control=value` written on a pebble, as the user reads it.
fn direct_refusal(control: &str, value: &str) -> Vec<Read> {
    reads(&refused(&doc(
        "",
        &format!("        pebble {control}={value} at=\"(0.5sq, 0.5sq)\""),
    )))
}

#[test]
fn a_param_forwarded_into_a_thing_param_is_typed_by_the_control_it_fills() {
    use dt_lang::codes;
    let cases: [(&str, &str, &str); 4] = [
        // a length with no unit
        ("size", "5", codes::UNIT),
        // an angle where a length is
        ("size", "\"5deg\"", codes::TYPE),
        // out of its range
        ("size", "\"900dmm\"", codes::RANGE),
        // a word it does not have
        ("shape", "jagged", codes::CHOICE),
    ];
    for (control, value, code) in cases {
        let direct = direct_refusal(control, value);
        assert!(direct.iter().any(|r| r.0 == code), "{direct:#?}");
        for levels in 0..3 {
            // the call's argument, through `levels` definitions
            let text = forwarded(control, "\"3dmm\"", Some(value), levels);
            let through = reads(&refused(&text));
            assert_eq!(
                through, direct,
                "`{control}={value}` through {levels} levels of forwarding"
            );
            // pointing at the call's argument, the line the user wrote
            let call = text.find("tile size").unwrap();
            for d in refused(&text) {
                let (_, span) = d.at.as_ref().unwrap();
                assert!(span.offset() > call, "{:?} points at the define", d.message);
            }
        }
    }
}

#[test]
fn a_default_forwarded_into_a_thing_param_is_typed_by_the_control_it_fills() {
    for levels in 0..3 {
        // the default is the mistake, and is where it is written
        let text = forwarded("size", "5", None, levels);
        let through = reads(&refused(&text));
        assert_eq!(through, direct_refusal("size", "5"), "{levels} levels");
        let call = text.find("tile size").unwrap();
        for d in refused(&text) {
            let (_, span) = d.at.as_ref().unwrap();
            assert!(span.offset() < call, "{:?} points at the call", d.message);
        }
    }
}

/// What a pebble's controls are, by name and value.
fn controls(text: &str) -> Vec<String> {
    let t = read(text).unwrap().tile.unwrap();
    let e = t.stack.iter().find(|e| e.what.value == "pebble").unwrap();
    e.controls
        .iter()
        .map(|c| format!("{} {:?}", c.name, c.value))
        .collect()
}

#[test]
fn a_valid_forwarded_value_reads_as_written_on_the_thing() {
    let cases: [(&str, &str); 3] = [
        ("size", "\"1.5dmm\""),
        ("size", "\"1dmm .. 2dmm\""),
        ("shape", "angular"),
    ];
    for (control, value) in cases {
        let direct = controls(&doc(
            "",
            &format!("        pebble {control}={value} at=\"(0.5sq, 0.5sq)\""),
        ));
        for levels in 0..3 {
            let through = controls(&forwarded(control, "\"1dmm\"", Some(value), levels));
            assert_eq!(
                through, direct,
                "`{control}={value}` through {levels} levels"
            );
        }
    }
    // the default, with no argument
    let direct = controls(&doc(
        "",
        "        pebble size=\"1dmm\" at=\"(0.5sq, 0.5sq)\"",
    ));
    assert_eq!(controls(&forwarded("size", "\"1dmm\"", None, 2)), direct);
}

/// The one refusal a param that fills two controls of different kinds gets:
/// at the param, `dt::type`, saying both.
fn two_kinds_refused(parts: &str, call: &str, both: [&str; 2]) {
    let text = doc(parts, call);
    let errors = refused(&text);
    let kinds: Vec<&Diag> = errors
        .iter()
        .filter(|e| e.code == dt_lang::codes::TYPE)
        .collect();
    assert!(!kinds.is_empty(), "{errors:#?}");
    for d in &kinds {
        for name in both {
            assert!(d.message.contains(name), "{name} in {:?}", d.message);
        }
        // at the param, in the definition's text
        let (src, span) = d.at.as_ref().unwrap();
        let at = &src.inner()[span.offset()..span.offset() + span.len()];
        assert_eq!(at, "s", "{:?}", d.message);
        assert!(span.offset() < text.find("tile size").unwrap());
    }
}

#[test]
fn a_param_that_fills_two_controls_of_different_kinds_is_refused_at_the_param() {
    let at = "at=\"(0.5sq, 0.5sq)\"";
    // both written in the definition: a length and a share
    two_kinds_refused(
        &format!(
            "define both {{\n    param s \"1dmm\"\n    stack {{\n        pebble size=s {at}\n        pebble buried=s {at}\n    }}\n}}"
        ),
        "        both",
        ["pebble size", "pebble buried"],
    );
    // both on a thing param's entries: known when the call says the thing
    for call in ["        both", "        both s=\"2dmm\""] {
        two_kinds_refused(
            &format!(
                "define both {{\n    param piece pebble\n    param s \"1dmm\"\n    stack {{\n        piece size=s {at}\n        piece buried=s {at}\n    }}\n}}"
            ),
            call,
            ["pebble size", "pebble buried"],
        );
    }
    // one written on an element, one on the thing param's entry
    two_kinds_refused(
        &format!(
            "define both {{\n    param piece pebble\n    param s \"1dmm\"\n    stack {{\n        rock size=s {at}\n        piece buried=s {at}\n    }}\n}}"
        ),
        "        both",
        ["rock size", "pebble buried"],
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
