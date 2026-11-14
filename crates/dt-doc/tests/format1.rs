//! Format 1 as dt-doc decodes it: `let` in any block and in a define,
//! reserved words, `$` names.

#![allow(clippy::unwrap_used)]

mod common;

use dt_doc::{Known, decode};

fn known() -> Known {
    let mut k = Known::new();
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
fn words_and_names() {
    assert_eq!(
        codes(&tile("let high 3\n stack { base }")),
        [dt_lang::codes::NAME]
    );
    assert_eq!(
        codes(&tile("let $high 3\n stack { base }")),
        Vec::<&str>::new()
    );
    assert_eq!(
        codes(&tile("override foot 3\n stack { base }")),
        [dt_lang::codes::NAME]
    );
    assert_eq!(
        codes(&tile("let $x 3\n stack { base }")),
        [dt_lang::codes::NAME]
    );
    assert_eq!(
        codes(&tile("let foot 3\n let foot 4\n stack { base }")),
        [dt_lang::codes::NAME]
    );
    assert_eq!(
        codes(&tile("stack { base\n dirt top=\"bottom + 1\" }")),
        [dt_lang::codes::NAME]
    );
    // `$d` is set by a body at each node, a plain fraction: not a height
    assert_eq!(
        codes(&tile("stack { base\n dirt top=\"$d\" }")),
        [dt_lang::codes::TYPE]
    );
}
