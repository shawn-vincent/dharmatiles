//! Deliberate mistakes in the real tiles, and what the author sees.
//! `cargo run -p dt-doc --example errors` prints the same reports.
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use common::*;

fn report(case: &Case) -> (Vec<dt_doc::DocError>, String) {
    let (name, text) = case.tile;
    assert!(
        text.contains(case.from),
        "{}: `{}` not in {name}",
        case.title,
        case.from
    );
    let broken = text.replacen(case.from, case.to, 1);
    let errors = dt_doc::decode(name, &broken, &known()).expect_err("should fail");
    let shown = render(&errors);
    (errors, shown)
}

#[test]
fn typo_suggests_the_control() {
    let (e, shown) = report(&CASES[0]);
    assert_eq!(e.len(), 1, "{shown}");
    assert_eq!(e[0].code, "dt::unknown-control");
    assert_eq!(e[0].help.as_deref(), Some("did you mean `per-square`?"));
    assert!(shown.contains("per-sqare"), "{shown}");
}

#[test]
fn wrong_unit_explains_h() {
    let (e, shown) = report(&CASES[1]);
    assert_eq!(e.len(), 1, "{shown}");
    assert_eq!(e[0].code, "dt::wrong-unit");
    assert!(
        e[0].help
            .as_deref()
            .unwrap()
            .contains("35 mm is about 1.057"),
        "{shown}"
    );
}

#[test]
fn v1_true_says_write_hash_true() {
    let (e, shown) = report(&CASES[2]);
    assert_eq!(e[0].code, "dt::syntax");
    assert!(e[0].message.contains("KDL v1"), "{shown}");
    assert_eq!(e[0].help.as_deref(), Some("write #true"));
}

#[test]
fn missing_along_is_reported_on_the_wall() {
    let (e, shown) = report(&CASES[3]);
    assert_eq!(e.len(), 1, "{shown}");
    assert_eq!(e[0].code, "dt::missing-control");
    assert_eq!(e[0].message, "`wall` needs `along`");
    let (name, text) = CASES[3].tile;
    let broken = text.replacen(CASES[3].from, CASES[3].to, 1);
    let s = e[0].span;
    assert_eq!(&broken[s.offset()..s.offset() + s.len()], "wall", "{name}");
}

#[test]
fn bad_skew_points_inside_the_string() {
    let (e, shown) = report(&CASES[4]);
    assert_eq!(e[0].code, "dt::expr");
    let (_, text) = CASES[4].tile;
    let broken = text.replacen(CASES[4].from, CASES[4].to, 1);
    let s = e[0].span;
    assert_eq!(&broken[s.offset()..s.offset() + s.len()], "x", "{shown}");
}

#[test]
fn more_mistakes() {
    let k = known();
    let cases = [
        // an unknown look
        ("tile t { stack { medow } }", "did you mean `meadow`?"),
        // a region that doesn't exist
        (
            "tile t { region yard { fill 0.5 0.5 }; stack { dirt self=yrd } }",
            "did you mean `yard`?",
        ),
        // a control in the wrong group
        (
            "tile t { stack { tree length=4.5 } }",
            "belongs in the `leaf` group",
        ),
        // a bad choice
        (
            "tile t { line l { through 0 0 1 1 }; stack { wall along=l family=feildstone } }",
            "did you mean `fieldstone`?",
        ),
        // a print-scale size written in squares
        (
            "tile t { stack { tree { leaf length=(sq)0.1 } } }",
            "OpenLOCK build keeps",
        ),
        // a named height that doesn't exist
        (
            "tile t { stack { hills height=grund } }",
            "did you mean `ground`?",
        ),
        // an unquoted range
        ("tile t { stack { meadow bare=20% } }", "written as strings"),
        // a missing fill point
        ("tile t { region r { } }", "paint bucket"),
    ];
    for (text, want) in cases {
        let e = dt_doc::decode("t.kdl", text, &k).expect_err(text);
        let shown = render(&e);
        assert!(shown.contains(want), "{text}\n{shown}");
    }
}
