//! Things and layers, and the parameters that hold a thing, as dt-doc reads
//! them: the schema says once which an element is, a parameter's kind is
//! known where it is written, and a value of the wrong kind is refused at its
//! line with what to write.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failing unwrap is the failure"
)]

mod common;

use dt_doc::model::{Callee, Value};
use dt_doc::schema::{Generator, element};
use dt_doc::{DocError, Known, decode};

fn known() -> Known {
    let mut k = Known::new();
    k.add_heights("library/parts/heights.dtile", common::HEIGHTS)
        .unwrap();
    k
}

fn errors(text: &str) -> Vec<DocError> {
    match decode("t.dtile", text, &known()) {
        Ok(_) => Vec::new(),
        Err(e) => e,
    }
}

fn doc(define: &str, stack: &str) -> String {
    format!(
        "dharmatiles version=\"1.0.0\"\n{define}\ntile size=\"1x1\" {{\n    stack {{\n{stack}\n    }}\n}}\n"
    )
}

const STAND: &str =
    "define stand {\n    param piece pebble\n    stack { piece at=\"(0.5sq, 0.5sq)\" }\n}";

fn ok(text: &str) {
    let e = errors(text);
    assert!(e.is_empty(), "{text}\n{e:#?}");
}

fn refused(text: &str, code: &str, want: &str) {
    let e = errors(text);
    assert!(
        e.iter().any(|e| e.code == code
            && (e.message.contains(want) || e.help.as_deref().is_some_and(|h| h.contains(want)))),
        "{text}\nwanted {code} saying `{want}`, got {e:#?}"
    );
}

#[test]
fn the_schema_says_once_whether_an_element_is_a_thing_or_a_layer() {
    for (name, want) in [
        ("rock", Generator::Thing),
        ("stone", Generator::Thing),
        ("boulder", Generator::Thing),
        ("pebble", Generator::Thing),
        ("flower", Generator::Thing),
        ("tree", Generator::Thing),
        ("dirt", Generator::Layer),
        ("water", Generator::Layer),
        ("scatter", Generator::Layer),
        ("wall", Generator::Layer),
        ("cliff", Generator::Layer),
        ("grass", Generator::Layer),
    ] {
        assert_eq!(element(name).unwrap().generator(), want, "{name}");
    }
    // a thing is what a scatter may hold: no second table
    for e in dt_doc::elements::ELEMENTS.iter().filter(|e| e.scattered) {
        assert_eq!(e.generator(), Generator::Thing, "{} is scattered", e.name);
    }
}

#[test]
fn a_param_that_names_a_thing_holds_one() {
    let doc = decode("t.dtile", &doc(STAND, "        stand"), &known()).unwrap();
    let stand = doc.definition("stand").unwrap();
    let p = &stand.params[0];
    assert!(p.thing);
    assert!(matches!(&p.default.as_ref().unwrap().value, Value::Thing(n) if n == "pebble"));
    // and its use as an entry calls the parameter
    assert!(matches!(&stand.stack[0].callee, Callee::Param(p) if p.name == "piece"));
    // a define whose stack is one such entry is a thing itself
    assert!(stand.thing);
}

#[test]
fn a_param_written_as_an_entry_holds_a_thing_with_no_default() {
    let text = doc(
        "define stand {\n    param piece\n    stack { piece at=\"(0.5sq, 0.5sq)\" }\n}",
        "        stand piece=rock",
    );
    ok(&text);
    refused(
        &doc(
            "define stand {\n    param piece\n    stack { piece at=\"(0.5sq, 0.5sq)\" }\n}",
            "        stand",
        ),
        dt_lang::codes::MISSING_CONTROL,
        "needs `piece`",
    );
}

#[test]
fn a_thing_is_passed_by_name_and_by_a_definition() {
    ok(&doc(STAND, "        stand piece=boulder"));
    let mine = format!("define my-rock {{ stack {{ rock }} }}\n{STAND}");
    ok(&doc(&mine, "        stand piece=my-rock"));
    // a thing parameter passed on to another definition's
    let on = format!(
        "{STAND}\ndefine outer {{\n    param holds boulder\n    stack {{ stand piece=holds }}\n}}"
    );
    ok(&doc(&on, "        outer holds=rock"));
}

#[test]
fn a_layer_is_not_a_thing() {
    refused(
        &doc(STAND, "        stand piece=dirt"),
        dt_lang::codes::TYPE,
        "`dirt` is a layer",
    );
    refused(
        &doc(STAND, "        stand piece=scatter"),
        dt_lang::codes::TYPE,
        "`scatter` is a layer",
    );
    // a definition holding more than one thing is not one
    let two = format!("define two {{ stack {{ rock; rock }} }}\n{STAND}");
    refused(
        &doc(&two, "        stand piece=two"),
        dt_lang::codes::TYPE,
        "not one thing",
    );
}

#[test]
fn a_number_is_not_a_thing_and_a_thing_is_not_a_number() {
    refused(
        &doc(STAND, "        stand piece=3"),
        dt_lang::codes::TYPE,
        "is a thing, and a number is not",
    );
    refused(
        &doc(STAND, "        stand piece=\"3dmm\""),
        dt_lang::codes::UNKNOWN_ELEMENT,
        "no element or definition is called",
    );
    // a thing where a control wants a length
    refused(
        &doc("", "        rock size=boulder"),
        dt_lang::codes::TYPE,
        "`boulder` is a thing",
    );
    let sized = "define sized {\n    param by \"8dmm\"\n    stack { rock size=by }\n}";
    refused(
        &doc(sized, "        sized by=boulder"),
        dt_lang::codes::TYPE,
        "`boulder` is a thing",
    );
}

#[test]
fn a_param_is_one_kind() {
    refused(
        &doc(
            "define both {\n    param what\n    stack { rock size=what; what at=\"(0.5sq, 0.5sq)\" }\n}",
            "        both what=rock",
        ),
        dt_lang::codes::TYPE,
        "used as a thing and as a value",
    );
}

#[test]
fn a_thing_param_is_not_named_like_an_element() {
    refused(
        "dharmatiles version=\"1.0.0\"\ndefine d {\n    param rock pebble\n    stack { dirt }\n}\n",
        dt_lang::codes::NAME,
        "is the name of an element or a definition",
    );
}

#[test]
fn a_layer_in_a_scatter_names_its_kind() {
    refused(
        &doc("", "        scatter count=2 { dirt }"),
        dt_lang::codes::SCATTER,
        "is a layer, and a scatter holds things",
    );
}

#[test]
fn a_thing_that_holds_itself_is_a_loop() {
    refused(
        "dharmatiles version=\"1.0.0\"\ndefine fern {\n    param leaf fern\n    stack { leaf at=\"(0.5sq, 0.5sq)\" }\n}\n",
        dt_lang::codes::LOOP,
        "calls itself",
    );
}

#[test]
fn what_is_written_on_a_thing_param_is_typed_when_the_thing_is_known() {
    use dt_doc::decode::{Thing, bind_thing};
    let text = doc(
        "define stand {\n    param piece pebble\n    stack { piece size=5 at=\"(0.5sq, 0.5sq)\" }\n}",
        "        stand",
    );
    // which thing it is, is not known where it is written: nothing is refused
    ok(&text);
    let k = known();
    let pebble = element("pebble").unwrap();
    let bind = |text: &str| {
        let d = decode("t.dtile", text, &k).unwrap();
        let e = d.definition("stand").unwrap().stack[0].clone();
        let Callee::Param(p) = &e.callee else {
            panic!("a thing parameter");
        };
        let src = std::sync::Arc::new(miette::NamedSource::new("t.dtile", text.to_owned()));
        bind_thing(&e, p, Thing::Element(pebble), &src, &k)
    };
    // given a thing, it is read as written on the thing: a pebble's length
    // with no unit
    let errors = bind(&text).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == dt_lang::codes::UNIT),
        "{errors:#?}"
    );
    // and with its unit it is a pebble's size, typed
    let bound = bind(&text.replace("size=5", "size=\"5dmm\"")).unwrap();
    assert!(matches!(bound.entry.callee, Callee::Element(l) if l.name == "pebble"));
    assert!(matches!(bound.entry.value("size"), Some(Value::Num(_))));
}
