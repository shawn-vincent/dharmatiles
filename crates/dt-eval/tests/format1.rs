//! Format 1, in development: `$` names read from the context, `let` and
//! `override` in any block, reserved words, and `check` refusing a read
//! nothing installed.  Format 1 is read only with the explicit
//! `format_preview` option (the command line sets it from
//! `DT_FORMAT_PREVIEW=1`), so these tests never touch the environment.

#![allow(clippy::unwrap_used)]

use dt_eval::{Checked, Diag, Memory, Options, check};

fn preview() -> Options {
    Options {
        format_preview: true,
        ..Options::default()
    }
}

fn mem(files: &[(&str, &str)]) -> Memory {
    Memory(
        files
            .iter()
            .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
            .collect(),
    )
}

fn tile1(body: &str) -> String {
    format!("dharmatiles version=1\ntile size=\"1x1\" seed=3 {{\n{body}\n}}\n")
}

fn ok(text: &str) -> Checked {
    check(&mem(&[("t.dtile", text)]), "t.dtile", &preview())
        .unwrap_or_else(|e| panic!("{text}\n{e:#?}"))
}

fn refused(text: &str) -> Vec<Diag> {
    match check(&mem(&[("t.dtile", text)]), "t.dtile", &preview()) {
        Ok(_) => panic!("expected a refusal:\n{text}"),
        Err(e) => e,
    }
}

fn codes(d: &[Diag]) -> Vec<&'static str> {
    d.iter().map(|d| d.code).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The sizes of the rocks in the first scatter of the tile.
fn rock_sizes(c: &Checked) -> Vec<f64> {
    let t = c.tile.as_ref().unwrap();
    t.stack
        .iter()
        .filter(|e| e.what.value == "scatter")
        .flat_map(|e| e.children.iter())
        .filter(|ch| ch.what.value == "rock")
        .map(|ch| {
            let s = ch.controls.iter().find(|s| s.name == "size").unwrap();
            match &s.value {
                dt_elements::Value::Number(n) => *n,
                other => panic!("size is {other:?}"),
            }
        })
        .collect()
}

#[test]
fn format_one_needs_the_preview_and_says_so() {
    let text = tile1("stack { base\n dirt }");
    let e = check(&mem(&[("t.dtile", &text)]), "t.dtile", &Options::default())
        .err()
        .unwrap();
    assert_eq!(codes(&e), ["dt::format-newer"]);
    assert!(e[0].help.as_deref().unwrap().contains("in development"));
    ok(&text);
}

#[test]
fn a_format_one_tile_solves_as_format_zero_does() {
    let c = ok(&tile1("stack { base\n dirt }"));
    let d = &c.solved.as_ref().unwrap().bodies[0];
    let rim = d.at(0.0, 0.5);
    assert!(close(rim.lo, 3.4) && close(rim.hi, 3.4), "{rim:?}");
    let mid = d.at(0.5, 0.5);
    assert!(close(mid.lo, 5.0) && close(mid.hi, 6.0), "{mid:?}");
}

#[test]
fn override_replaces_a_library_height_and_the_ones_that_read_it() {
    let c = ok(&tile1(
        "override $groundHeight 8\n stack { base\n dirt top=\"$groundHeight\" }",
    ));
    let d = &c.solved.as_ref().unwrap().bodies[0];
    let mid = d.at(0.5, 0.5);
    assert!(close(mid.lo, 8.0) && close(mid.hi, 8.0), "{mid:?}");
    assert!(
        c.explain()
            .iter()
            .any(|l| l.contains("override $groundHeight"))
    );
}

#[test]
fn a_let_is_read_where_it_is_used_so_it_follows_an_override_below_it() {
    // `$hump` reads `$groundHeight`; the override sits in the stack block,
    // below the `let`, and the dirt in the block sees it
    let c = ok(&tile1(
        "let $hump \"$groundHeight + 1\"\n stack { override $groundHeight 8\n base\n dirt top=\"$hump\" }",
    ));
    let mid = c.solved.as_ref().unwrap().bodies[0].at(0.5, 0.5);
    // the same as writing the sum where it is used (the rim eases it a little)
    let direct = ok(&tile1(
        "stack { override $groundHeight 8\n base\n dirt top=\"$groundHeight + 1\" }",
    ));
    let want = direct.solved.as_ref().unwrap().bodies[0].at(0.5, 0.5);
    assert!(close(mid.lo, want.lo) && mid.lo > 8.5, "{mid:?} {want:?}");
}

#[test]
fn an_override_may_read_the_value_it_replaces() {
    let c = ok(&tile1(
        "override $groundHeight \"$groundHeight + 2\"\n stack { base\n dirt top=\"$groundHeight\" }",
    ));
    let mid = c.solved.as_ref().unwrap().bodies[0].at(0.5, 0.5);
    assert!(close(mid.lo, 7.0), "{mid:?}");
}

#[test]
fn let_on_a_name_the_context_has_says_use_override() {
    let e = refused(&tile1("let $groundHeight 6\n stack { base\n dirt }"));
    assert_eq!(codes(&e), ["dt::name"]);
    assert!(e[0].message.contains("already has `$groundHeight`"));
    assert!(
        e[0].help
            .as_deref()
            .unwrap()
            .contains("override $groundHeight")
    );
}

#[test]
fn override_of_a_name_no_caller_installed_suggests_the_near_one() {
    let e = refused(&tile1("override $groundHeigth 6\n stack { base\n dirt }"));
    assert_eq!(codes(&e), ["dt::name"]);
    assert!(
        e[0].help
            .as_deref()
            .unwrap()
            .contains("`override $groundHeight")
    );
}

#[test]
fn a_block_scope_ends_with_its_block() {
    // `$q` is installed inside the scatter only: the dirt after it can't read it
    let e = refused(&tile1(
        "stack { base\n scatter count=2 { let $q 3\n rock size=\"$q\" }\n dirt top=\"$q\" }",
    ));
    assert_eq!(codes(&e), ["dt::unknown-name"]);
    assert!(e[0].message.contains("`$q`"));
    // and inside it works
    let c = ok(&tile1(
        "stack { base\n scatter count=2 { let $q 3\n rock size=\"$q\" } }",
    ));
    assert_eq!(rock_sizes(&c), [3.0]);
}

#[test]
fn a_scope_changes_the_context_for_what_it_calls_only() {
    let parts = "dharmatiles version=1\n\
        define smaller { stack { override $s 2\n scatter count=2 { rock size=\"$s\" } } }\n\
        define plain { stack { scatter count=2 { rock size=\"$s\" } } }\n";
    let tile =
        tile1("let $s 5\n stack { base\n smaller\n plain\n scatter count=2 { rock size=\"$s\" } }")
            .replace("tile size", "use \"./parts\" smaller plain\ntile size");
    let c = check(
        &mem(&[("t.dtile", &tile), ("parts.dtile", parts)]),
        "t.dtile",
        &preview(),
    )
    .unwrap_or_else(|e| panic!("{e:#?}"));
    // the override inside `smaller` is gone when it ends
    assert_eq!(rock_sizes(&c), [2.0, 5.0, 5.0]);
}

#[test]
fn a_definition_reads_the_context_of_its_caller_and_check_says_where_it_is_missing() {
    let parts = "dharmatiles version=1\n\
        define stones { stack { scatter count=2 { rock size=\"$stoneSize\" } } }\n";
    let with = tile1("let $stoneSize 6\n stack { base\n stones }")
        .replace("tile size", "use \"./parts\" stones\ntile size");
    let c = check(
        &mem(&[("t.dtile", &with), ("parts.dtile", parts)]),
        "t.dtile",
        &preview(),
    )
    .unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(rock_sizes(&c), [6.0]);
    let without =
        tile1("stack { base\n stones }").replace("tile size", "use \"./parts\" stones\ntile size");
    let e = check(
        &mem(&[("t.dtile", &without), ("parts.dtile", parts)]),
        "t.dtile",
        &preview(),
    )
    .err()
    .unwrap();
    assert_eq!(codes(&e), ["dt::unknown-name"]);
    assert!(e[0].message.contains("`$stoneSize`"), "{}", e[0].message);
}

#[test]
fn a_position_is_not_drawn_once_so_it_cannot_be_a_size() {
    let e = refused(&tile1(
        "stack { base\n scatter count=2 { rock size=\"$x\" } }",
    ));
    assert_eq!(codes(&e), ["dt::type"]);
    // through a let, too: the rule follows the names a formula reads
    let e = refused(&tile1(
        "let $w \"$x + 4\"\n stack { base\n scatter count=2 { rock size=\"$w\" } }",
    ));
    assert_eq!(codes(&e), ["dt::type"]);
    // a library height that reads no position is a fine size
    ok(&tile1(
        "stack { base\n scatter count=2 { rock size=\"$groundHeight\" } }",
    ));
    // and where a height is worked out at each point it is welcome
    ok(&tile1(
        "let $wave \"$groundHeight + 0.4 * cos(720 * $x)\"\n stack { base\n dirt top=\"$wave\" }",
    ));
}

#[test]
fn what_an_element_sets_cannot_be_declared() {
    for name in ["$x", "$y", "$bottom", "$d", "$a", "$region"] {
        for kw in ["let", "override"] {
            let e = refused(&tile1(&format!("{kw} {name} 3\n stack {{ base\n dirt }}")));
            assert_eq!(codes(&e), ["dt::name"], "{kw} {name}");
            assert!(e[0].message.contains(name), "{}", e[0].message);
        }
    }
}

#[test]
fn a_plain_position_says_to_write_the_dollar() {
    let e = refused(&tile1("stack { base\n dirt top=\"groundHeight + x\" }"));
    assert!(codes(&e).contains(&"dt::name"), "{e:#?}");
    let help = e.iter().find(|d| d.code == "dt::name").unwrap();
    assert!(help.help.as_deref().unwrap().contains("$x"));
}

#[test]
fn reserved_words_cannot_name_a_let() {
    for w in [
        "x", "d", "a", "parent", "sin", "clamp", "point", "polar", "high", "bell", "mix", "sq",
        "mm", "deg", "pi", "dmm",
    ] {
        let e = refused(&tile1(&format!("let {w} 3\n stack {{ base\n dirt }}")));
        assert_eq!(codes(&e), ["dt::name"], "{w}");
        assert!(e[0].message.contains(&format!("`{w}`")), "{}", e[0].message);
    }
    // `$` names are another namespace
    ok(&tile1("let $high 3\n stack { base\n dirt }"));
}

#[test]
fn a_param_hides_nothing_of_the_context() {
    let parts = "dharmatiles version=1\n\
        define lawn { param length 4\n stack { scatter count=2 { rock size=length } } }\n";
    let tile = tile1("stack { base\n lawn length=7 }")
        .replace("tile size", "use \"./parts\" lawn\ntile size");
    let c = check(
        &mem(&[("t.dtile", &tile), ("parts.dtile", parts)]),
        "t.dtile",
        &preview(),
    )
    .unwrap_or_else(|e| panic!("{e:#?}"));
    assert_eq!(rock_sizes(&c), [7.0]);
    let bad = "dharmatiles version=1\ndefine lawn { param high 4\n stack { base } }\n";
    let tile = tile1("stack { base }").replace("tile size", "use \"./parts\" lawn\ntile size");
    let e = check(
        &mem(&[("t.dtile", &tile), ("parts.dtile", bad)]),
        "t.dtile",
        &preview(),
    )
    .err()
    .unwrap();
    assert_eq!(codes(&e), ["dt::name"]);
}

#[test]
fn a_tile_and_its_parts_share_a_format() {
    let parts = "define lawn { stack { base } }\n";
    let tile = tile1("stack { base }").replace("tile size", "use \"./parts\" lawn\ntile size");
    let e = check(
        &mem(&[("t.dtile", &tile), ("parts.dtile", parts)]),
        "t.dtile",
        &preview(),
    )
    .err()
    .unwrap();
    assert_eq!(codes(&e), ["dt::format-mixed"]);
}

#[test]
fn format_zero_does_not_read_dollar_names() {
    let text = "tile size=\"1x1\" seed=3 {\n stack { base\n dirt top=\"$groundHeight\" }\n}\n";
    let e = check(&mem(&[("t.dtile", text)]), "t.dtile", &preview())
        .err()
        .unwrap();
    assert_eq!(codes(&e), ["dt::name"]);
    assert!(e[0].help.as_deref().unwrap().contains("version=1"));
}
