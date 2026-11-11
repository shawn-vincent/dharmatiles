//! Format 1 as dt-doc decodes it: read only when the reader is told to
//! preview it (an explicit option, never the environment).

#![allow(clippy::unwrap_used)]

mod common;

use dt_doc::{Known, decode};

fn known() -> Known {
    let mut k = Known::new().with_format_preview(true);
    k.add_heights("library/parts/heights.dtile", common::HEIGHTS)
        .unwrap();
    k
}

fn codes(text: &str) -> Vec<&'static str> {
    match decode("t.dtile", text, &known()) {
        Ok(_) => Vec::new(),
        Err(e) => e.iter().map(|e| e.code).collect(),
    }
}

fn tile(body: &str) -> String {
    format!("dharmatiles version=1\ntile size=\"1x1\" {{\n{body}\n}}\n")
}

#[test]
fn the_gate_is_the_readers_option() {
    let text = tile("stack { base }");
    assert!(decode("t", &text, &known()).is_ok());
    let closed = Known::new();
    let e = decode("t", &text, &closed).unwrap_err();
    assert_eq!(e[0].code, "dt::format-newer");
}

#[test]
fn lets_work_in_any_block_and_in_a_define() {
    let doc = decode(
        "t.dtile",
        &format!(
            "dharmatiles version=1\n{}",
            "define lawn { let $blade \"3dmm\"\n stack { override $blade \"4dmm\"\n scatter count=2 { let $q \"1dmm\"\n rock size=\"$q\" } } }\n"
        ),
        &known(),
    )
    .unwrap();
    assert_eq!(doc.format(), 1);
    let d = doc.definition("lawn").unwrap();
    assert_eq!(d.consts.len(), 1);
    assert_eq!(d.stack_consts.len(), 1);
    assert_eq!(d.stack[0].consts.len(), 1);
}

#[test]
fn a_part_has_no_let_in_format_zero() {
    let e = decode(
        "t.dtile",
        "define lawn { let a 3\n stack { base } }\n",
        &known(),
    )
    .unwrap_err();
    assert_eq!(e[0].code, "dt::unknown-node");
}

#[test]
fn words_and_names() {
    assert_eq!(codes(&tile("let high 3\n stack { base }")), ["dt::name"]);
    assert_eq!(
        codes(&tile("let $high 3\n stack { base }")),
        Vec::<&str>::new()
    );
    assert_eq!(
        codes(&tile("override foot 3\n stack { base }")),
        ["dt::name"]
    );
    assert_eq!(codes(&tile("let $x 3\n stack { base }")), ["dt::name"]);
    assert_eq!(
        codes(&tile("let foot 3\n let foot 4\n stack { base }")),
        ["dt::name"]
    );
    assert_eq!(
        codes(&tile("stack { base\n dirt top=\"bottom + 1\" }")),
        ["dt::name"]
    );
    // `$d` is set by a body at each node, a plain fraction: not a height
    assert_eq!(
        codes(&tile("stack { base\n dirt top=\"$d\" }")),
        ["dt::type"]
    );
}

#[test]
fn a_dollar_name_is_format_one() {
    let e = decode(
        "t.dtile",
        "tile size=\"1x1\" { stack { base\n dirt top=\"$groundHeight\" } }\n",
        &known(),
    )
    .unwrap_err();
    assert_eq!(e[0].code, "dt::name");
}
