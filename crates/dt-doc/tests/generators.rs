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
use dt_doc::schema::element;
use dt_doc::units::Kind;
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
    "let stand {\n    param piece pebble\n    stack { piece at=\"(0.5sq, 0.5sq)\" }\n}";

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
        ("rock", Kind::Thing),
        ("stone", Kind::Thing),
        ("boulder", Kind::Thing),
        ("pebble", Kind::Thing),
        ("flower", Kind::Thing),
        ("tree", Kind::Thing),
        ("dirt", Kind::Layer),
        ("water", Kind::Layer),
        ("scatter", Kind::Layer),
        ("wall", Kind::Layer),
        ("cliff", Kind::Layer),
        ("grass", Kind::Layer),
    ] {
        assert_eq!(element(name).unwrap().generator(), want, "{name}");
    }
    // a thing is what a scatter may hold: no second table
    for e in dt_doc::elements::ELEMENTS.iter().filter(|e| e.scattered) {
        assert_eq!(e.generator(), Kind::Thing, "{} is scattered", e.name);
    }
}

#[test]
fn a_param_that_names_a_thing_holds_one() {
    let doc = decode("t.dtile", &doc(STAND, "        stand"), &known()).unwrap();
    let stand = doc.definition("stand").unwrap();
    let p = &stand.params[0];
    assert_eq!(p.kind, Kind::Thing);
    assert!(matches!(&p.default.as_ref().unwrap().value, Value::Generator(_, n) if n == "pebble"));
    // and its use as an entry calls the parameter
    assert!(matches!(&stand.stack[0].callee, Callee::Param(p) if p.name == "piece"));
    // placed (`at=`), the entry is a layer, so the define is one
    assert_eq!(stand.generator, Kind::Layer);
    // a body that is one unplaced thing, written bare, is that thing
    let plain = "let one {\n    param piece pebble\n    piece\n}";
    let d = decode(
        "t.dtile",
        &format!(
            "dharmatiles version=\"1.0.0\"\n{plain}\ntile size=\"1x1\" {{ stack {{ one }} }}\n"
        ),
        &known(),
    )
    .unwrap();
    assert_eq!(d.definition("one").unwrap().generator, Kind::Thing);
}

#[test]
fn a_param_that_says_it_holds_a_thing_holds_one_with_no_default() {
    let kinded =
        "let stand {\n    param piece kind=thing\n    stack { piece at=\"(0.5sq, 0.5sq)\" }\n}";
    ok(&doc(kinded, "        stand piece=rock"));
    refused(
        &doc(kinded, "        stand"),
        dt_lang::codes::MISSING_CONTROL,
        "needs `piece`",
    );
}

#[test]
fn a_thing_is_passed_by_name_and_by_a_definition() {
    ok(&doc(STAND, "        stand piece=boulder"));
    let mine = format!("let my-rock {{ rock }}\n{STAND}");
    ok(&doc(&mine, "        stand piece=my-rock"));
    // a thing parameter passed on to another definition's
    let on = format!(
        "{STAND}\nlet outer {{\n    param holds boulder\n    stack {{ stand piece=holds }}\n}}"
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
    let two = format!("let two {{ stack {{ rock; rock }} }}\n{STAND}");
    refused(
        &doc(&two, "        stand piece=two"),
        dt_lang::codes::TYPE,
        "`two` is a layer, and `stand piece` wants a thing",
    );
}

#[test]
fn a_number_is_not_a_thing_and_a_thing_is_not_a_number() {
    refused(
        &doc(STAND, "        stand piece=3"),
        dt_lang::codes::TYPE,
        "`3` is a plain number, and `stand piece` wants a thing",
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
    let sized = "let sized {\n    param by \"8dmm\"\n    stack { rock size=by }\n}";
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
            "let both {\n    param what kind=layer\n    stack { rock size=what; what }\n}",
            "        both what=dirt",
        ),
        dt_lang::codes::TYPE,
        "`what` is a layer, and `rock size` wants a length",
    );
}

#[test]
fn a_thing_param_is_not_named_like_an_element() {
    refused(
        "dharmatiles version=\"1.0.0\"\nlet d {\n    param rock pebble\n    stack { dirt }\n}\n",
        dt_lang::codes::NAME,
        "is the name of an element or a definition",
    );
}

#[test]
fn a_layer_in_a_scatter_names_its_kind() {
    refused(
        &doc("", "        scatter count=2 { dirt }"),
        dt_lang::codes::SCATTER,
        "is a layer, and a scatter wants a thing",
    );
}

#[test]
fn a_thing_that_holds_itself_is_a_loop() {
    refused(
        "dharmatiles version=\"1.0.0\"\nlet fern {\n    param leaf fern\n    stack { leaf at=\"(0.5sq, 0.5sq)\" }\n}\n",
        dt_lang::codes::LOOP,
        "calls itself",
    );
}

#[test]
fn what_is_written_on_a_thing_param_is_typed_when_the_thing_is_known() {
    use dt_doc::decode::{Callable, bind_generator};
    let text = doc(
        "let stand {\n    param piece pebble\n    stack { piece size=5 at=\"(0.5sq, 0.5sq)\" }\n}",
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
        bind_generator(&e, p, Callable::Element(pebble), &src, &k)
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
    assert!(matches!(bound.callee, Callee::Element(l) if l.name == "pebble"));
    assert!(matches!(bound.value("size"), Some(Value::Num(_))));
}

// ---- one kind system, one fit function ----------------------------------------------------------

#[test]
fn thing_and_layer_are_kinds_beside_length_and_angle() {
    use dt_doc::units::fits;
    for (word, kind) in [
        ("length", Kind::Length),
        ("angle", Kind::Angle),
        ("plain", Kind::Plain),
        ("thing", Kind::Thing),
        ("layer", Kind::Layer),
    ] {
        assert_eq!(Kind::from_word(word), Some(kind));
        assert_eq!(kind.word(), word);
    }
    // each kind fits itself; a thing fits where a layer is wanted (placing
    // lifts it), a layer never fits where a thing is wanted
    assert!(fits(Kind::Thing, Kind::Thing));
    assert!(fits(Kind::Layer, Kind::Layer));
    assert!(fits(Kind::Thing, Kind::Layer));
    assert!(!fits(Kind::Layer, Kind::Thing));
    // and neither fits a number, nor a number either
    for n in [Kind::Length, Kind::Angle, Kind::Plain] {
        assert!(!fits(Kind::Thing, n) && !fits(Kind::Layer, n));
        assert!(!fits(n, Kind::Thing) && !fits(n, Kind::Layer));
    }
    // what is not known until the call fits anything
    assert!(fits(Kind::Any, Kind::Thing) && fits(Kind::Layer, Kind::Any));
    assert!(fits(Kind::Zero, Kind::Length) && !fits(Kind::Zero, Kind::Thing));
}

const GROUND: &str = "let bank {\n    param ground dirt\n    stack { ground }\n}";

#[test]
fn a_param_can_hold_a_layer() {
    let d = decode("t.dtile", &doc(GROUND, "        bank"), &known()).unwrap();
    let bank = d.definition("bank").unwrap();
    assert_eq!(bank.params[0].kind, Kind::Layer);
    assert!(
        matches!(&bank.params[0].default.as_ref().unwrap().value, Value::Generator(Kind::Layer, n) if n == "dirt")
    );
    // a layer entry is no thing: the definition is a layer
    assert_eq!(bank.generator, Kind::Layer);
    // a built-in layer and a part are passed by name, a thing too (placing lifts it)
    ok(&doc(GROUND, "        bank ground=water"));
    let part = format!("let lawn {{ stack {{ dirt; grass }} }}\n{GROUND}");
    ok(&doc(&part, "        bank ground=lawn"));
    ok(&doc(GROUND, "        bank ground=rock"));
    // a number is not a layer
    refused(
        &doc(GROUND, "        bank ground=3"),
        dt_lang::codes::TYPE,
        "`3` is a plain number, and `bank ground` wants a layer",
    );
    // and a place word passes through a layer param as on a part call
    let placed = "let bank {\n    param ground dirt\n    stack { ground at=north }\n}";
    ok(&doc(placed, "        bank"));
}

#[test]
fn a_layer_never_fits_where_a_thing_is_wanted() {
    let leaf = "let pile {\n    param leaf kind=thing\n    stack { scatter count=2 { leaf } }\n}";
    ok(&doc(leaf, "        pile leaf=rock"));
    refused(
        &doc(leaf, "        pile leaf=dirt"),
        dt_lang::codes::TYPE,
        "`dirt` is a layer, and `pile leaf` wants a thing",
    );
    // a layer param used as a scatter's child
    let both = "let pile {\n    param ground dirt\n    stack { scatter count=2 { ground } }\n}";
    refused(
        &doc(both, "        pile"),
        dt_lang::codes::TYPE,
        "`ground` is a layer, and a scatter wants a thing",
    );
    // a layer param passed on to a thing param
    let on = format!(
        "{STAND}\nlet outer {{\n    param holds kind=layer\n    stack {{ stand piece=holds }}\n}}"
    );
    refused(
        &doc(&on, "        outer holds=dirt"),
        dt_lang::codes::TYPE,
        "`holds` is a layer, and `stand piece` wants a thing",
    );
    // a part whose one entry is placed is a layer, not a thing
    let placed = "let corner-rock { stack { rock at=\"(0.1sq, 0.1sq)\" } }";
    refused(
        &doc(placed, "        scatter count=4 { corner-rock }"),
        dt_lang::codes::SCATTER,
        "`corner-rock` is a layer, and a scatter wants a thing",
    );
}

#[test]
fn a_written_kind_agrees_with_a_default_a_use_and_a_caller() {
    // agreeing
    ok(&doc(
        "let d {\n    param reach kind=length \"3dmm\"\n    stack { rock size=reach }\n}",
        "        d reach=\"5dmm\"",
    ));
    ok(&doc(
        "let d {\n    param leaf kind=thing pebble\n    stack { leaf at=\"(0.5sq, 0.5sq)\" }\n}",
        "        d leaf=rock",
    ));
    // a default of another kind
    refused(
        &doc(
            "let d {\n    param reach kind=length 3\n    stack { rock size=reach }\n}",
            "        d",
        ),
        dt_lang::codes::UNIT,
        "a length is written with one",
    );
    refused(
        &doc(
            "let d {\n    param ground kind=layer 3\n    stack { ground }\n}",
            "        d",
        ),
        dt_lang::codes::TYPE,
        "`3` is a plain number, and `param ground` wants a layer",
    );
    refused(
        &doc(
            "let d {\n    param leaf kind=thing dirt\n    stack { leaf at=\"(0.5sq, 0.5sq)\" }\n}",
            "        d",
        ),
        dt_lang::codes::TYPE,
        "`dirt` is a layer, and `param leaf` wants a thing",
    );
    // a use that disagrees
    refused(
        &doc(
            "let d {\n    param reach kind=angle\n    stack { rock size=reach }\n}",
            "        d reach=\"5deg\"",
        ),
        dt_lang::codes::TYPE,
        "`reach` is an angle, and `rock size` wants a length",
    );
    // a caller's argument that disagrees
    refused(
        &doc(
            "let d {\n    param reach kind=length\n    stack { rock size=reach }\n}",
            "        d reach=\"5deg\"",
        ),
        dt_lang::codes::TYPE,
        "is an angle",
    );
    // a kind that is no parameter's
    refused(
        &doc(
            "let d {\n    param p kind=region 1\n    stack { dirt }\n}",
            "        d",
        ),
        dt_lang::codes::KIND,
        "one of length, angle, plain, thing or layer",
    );
}

#[test]
fn a_param_that_nothing_decides_says_to_write_its_kind() {
    refused(
        &doc(
            "let d {\n    param p\n    stack { dirt }\n}",
            "        d p=1",
        ),
        dt_lang::codes::KIND,
        "kind=",
    );
    // an entry does not decide thing or layer: it is said
    refused(
        &doc(
            "let d {\n    param ground\n    stack { ground }\n}",
            "        d ground=dirt",
        ),
        dt_lang::codes::KIND,
        "`kind=layer`",
    );
}

/// The kind words and the wording of a kind mistake are made once, in
/// `units`: no other source file writes a kind mistake out, and the old
/// two-case helpers are gone.
#[test]
fn no_check_site_words_a_kind_mistake_itself() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let banned = [
        "is a layer, and",
        "is a thing, and",
        "wants a thing",
        "wants a layer",
        "holds things",
        "fn thing_value",
        "fn is_thing",
        "fn thing_kind",
        "Generator::",
    ];
    let mut found = Vec::new();
    let mut stack = vec![crates];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let path = e.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                // sources only: not a build folder, nor a test (which says what it holds)
                if !matches!(name.as_str(), "target" | "tests" | "node_modules") {
                    stack.push(path);
                }
            } else if name.ends_with(".rs") && name != "units.rs" {
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in text.lines().enumerate() {
                    if line.trim_start().starts_with("//") {
                        continue;
                    }
                    if let Some(b) = banned.iter().find(|b| line.contains(**b)) {
                        found.push(format!("{}:{}: {b}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "a kind mistake is worded by `dt_doc::units::misfit`, and a fit asked of `units::fits`:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_kind_word_layer_is_read_as_a_kind_not_as_the_unit() {
    // `layer` is also a unit word, so it cannot name a param ...
    refused(
        &doc(
            "let d {\n    param layer dirt\n    stack { dirt }\n}",
            "        d",
        ),
        dt_lang::codes::NAME,
        "is a word of the language",
    );
    // ... but written as `kind=layer` it is the kind
    let kinded = GROUND.replace("param ground dirt", "param ground kind=layer");
    ok(&doc(&kinded, "        bank ground=dirt"));
}

// ---- the body decides the kind (U2b) --------------------------------------------------------------------------

fn kind_of(part: &str, name: &str) -> Kind {
    match decode("t.dtile", &doc(part, "        dirt"), &known()) {
        Ok(d) => d.definition(name).unwrap().generator,
        Err(e) => panic!("{part}\n{e:#?}"),
    }
}

#[test]
fn only_a_bare_unplaced_thing_is_a_thing_and_a_stack_is_always_a_layer() {
    assert_eq!(
        kind_of("let bluff {\n    rock size=\"6dmm\"\n}", "bluff"),
        Kind::Thing
    );
    // the same entry in a stack is a layer
    assert_eq!(
        kind_of("let bluff {\n    stack { rock size=\"6dmm\" }\n}", "bluff"),
        Kind::Layer
    );
    // a bare layer is a layer, and a placed thing is one
    assert_eq!(kind_of("let bank {\n    dirt\n}", "bank"), Kind::Layer);
    assert_eq!(
        kind_of("let spot {\n    rock at=\"(0.5sq, 0.5sq)\"\n}", "spot"),
        Kind::Layer
    );
    // a bare call of a param has the param's kind: a layer param is a layer, a thing param a thing
    assert_eq!(
        kind_of("let a {\n    param p kind=layer\n    p\n}", "a"),
        Kind::Layer
    );
    assert_eq!(
        kind_of("let b {\n    param p kind=thing\n    p\n}", "b"),
        Kind::Thing
    );
    // a bare call of a definition that is a thing
    assert_eq!(
        kind_of(
            "let bluff {\n    rock\n}\nlet again {\n    bluff\n}",
            "again"
        ),
        Kind::Thing
    );
}

#[test]
fn a_param_and_a_let_of_a_value_are_not_entries() {
    // the one entry is the grass: the param is the body's own
    let part = "let lawn {\n    param length \"3dmm\"\n    grass length=length\n}";
    assert_eq!(kind_of(part, "lawn"), Kind::Layer);
    let with_let =
        "let lawn {\n    let cover \"50%\"\n    param length \"3dmm\"\n    grass length=length\n}";
    ok(&doc(with_let, "        lawn"));
}

#[test]
fn a_body_is_one_bare_entry_or_a_stack_never_several_and_never_both() {
    refused(
        &doc("let two {\n    dirt\n    grass\n}", "        two"),
        dt_lang::codes::STACK,
        "several entries are a stack",
    );
    refused(
        &doc(
            "let both {\n    dirt\n    stack { grass }\n}",
            "        both",
        ),
        dt_lang::codes::STACK,
        "not both",
    );
    refused(
        &doc("let nothing { }", "        dirt"),
        dt_lang::codes::STACK,
        "empty",
    );
}

#[test]
fn a_let_is_a_value_or_a_body_not_both() {
    refused(
        &doc("let reach \"5dmm\" {\n    dirt\n}", "        dirt"),
        dt_lang::codes::VALUE,
        "a value or a body",
    );
    ok(&doc("let reach \"5dmm\"", "        dirt"));
}

#[test]
fn define_is_retired_whatever_its_shape_and_the_error_carries_the_fix() {
    for old in [
        "define lawn {\n    grass\n}",
        "define lawn \"5\"",
        "define lawn",
    ] {
        let e = errors(&doc(old, "        dirt"));
        let hit = e
            .iter()
            .find(|e| e.code == dt_lang::codes::RETIRED)
            .unwrap_or_else(|| panic!("{old}\n{e:#?}"));
        assert!(hit.message.contains("`define` is written `let`"), "{old}");
        assert!(
            hit.fix.is_some(),
            "{old}: the fix writes `let` over the word"
        );
    }
    // with a body it is read as what it was, so the one mistake is the word
    let e = errors(&doc("define lawn {\n    grass\n}", "        lawn"));
    assert_eq!(e.len(), 1, "{e:#?}");
}

#[test]
fn a_let_with_a_body_inside_a_block_is_refused_in_words() {
    let text = "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    let bluff {\n        rock\n    }\n    stack { dirt }\n}\n";
    refused(
        text,
        dt_lang::codes::VALUE,
        "written at the top level of a file",
    );
}
