//! Edge cases found in review: each test is one repro,
//! kept so the fix stays fixed.

#![allow(clippy::unwrap_used)]

mod common;

use dt_eval::{Memory, Options, check};

/// These tests write their documents in format 1 and read them from memory.
fn legacy() -> Options {
    Options::default()
}

fn mem(files: &[(&str, &str)]) -> Memory {
    common::memory(files)
}

#[test]
fn a_use_from_a_file_above_the_working_folder_keeps_its_dot_dots() {
    let src = mem(&[
        (
            "../u/sub/t.dtile",
            "dharmatiles version=\"1.0.0\"; use \"./p\" p\ntile size=\"1x1\" { stack { p } }\n",
        ),
        (
            "../u/sub/p.dtile",
            "dharmatiles version=\"1.0.0\"; define p { stack { dirt } }\n",
        ),
        (
            "u/sub/p.dtile",
            "dharmatiles version=\"1.0.0\"; define p { stack { rock size=\"9dmm\" } }\n",
        ),
    ]);
    let t = check(&src, "../u/sub/t.dtile", &legacy())
        .unwrap()
        .tile
        .unwrap();
    assert_eq!(t.stack[0].src.name(), "../u/sub/p.dtile");
}

fn errors(text: &str) -> Vec<dt_eval::Diag> {
    match check(&mem(&[("t.dtile", text)]), "t.dtile", &legacy()) {
        Ok(_) => panic!("expected an error"),
        Err(e) => e,
    }
}

#[test]
fn a_huge_tile_is_refused_before_any_grid_is_made() {
    // was: check allocated 163 TB and aborted; 2^25 wrapped to 1 node
    for size in ["100000x100000", "33554432x1", "65x1"] {
        let e = errors(&format!(
            "dharmatiles version=1; tile size=\"{size}\" {{ stack {{ dirt }} }}\n"
        ));
        assert_eq!(e[0].code, "dt::tile", "{size}");
        assert!(e[0].help.is_some());
    }
}

#[test]
fn a_fill_point_on_its_line_is_an_empty_region() {
    let e = errors(
        "dharmatiles version=\"1.0.0\"; tile size=\"1x1\" {\n  line split { through west east }\n  region s { boundedBy split; fill \"(0.5sq, 0.5sq)\" }\n  stack { dirt at=s }\n}\n",
    );
    assert_eq!(e[0].code, "dt::empty-region");
    assert_eq!(e[0].location().unwrap().1, 3);
    assert!(e[0].help.as_deref().unwrap().contains("off the line"));
}

#[test]
fn a_fill_point_beside_its_line_is_not_empty() {
    let src = mem(&[(
        "t.dtile",
        "dharmatiles version=\"1.0.0\"; tile size=\"1x1\" {\n  line split { through west east }\n  region s { boundedBy split; fill \"(0.5sq, 0.8sq)\" }\n  stack { dirt at=s }\n}\n",
    )]);
    assert!(check(&src, "t.dtile", &legacy()).is_ok());
}

#[test]
fn a_tile_with_nothing_in_it_is_an_error() {
    for text in [
        "dharmatiles version=\"1.0.0\"; tile size=\"1x1\" { stack { } }\n",
        "dharmatiles version=\"1.0.0\"; tile size=\"1x1\" { }\n",
    ] {
        let e = errors(text);
        assert_eq!(e[0].code, "dt::empty", "{text}");
        assert_eq!(e[0].location().unwrap().1, 1);
    }
}

#[test]
fn definitions_that_multiply_stop_at_the_entry_cap() {
    // was: 262 144 entries and 4.6 GB in `check` from seven lines
    let mut text = String::from(
        "dharmatiles version=\"1.0.0\"; define f1 { stack { dirt; dirt; dirt; dirt; dirt; dirt; dirt; dirt } }\n",
    );
    for k in 2..=6 {
        let p = k - 1;
        text += &format!(
            "define f{k} {{ stack {{ f{p}; f{p}; f{p}; f{p}; f{p}; f{p}; f{p}; f{p} }} }}\n"
        );
    }
    text += "tile size=\"1x1\" { stack {\n  f6\n} }\n";
    let e = errors(&text);
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].code, "dt::too-many");
    assert_eq!(e[0].location().unwrap().1, 8);
}

#[test]
fn ten_thousand_entries_are_allowed() {
    let src = mem(&[(
        "t.dtile",
        &format!(
            "dharmatiles version=\"1.0.0\"; define d {{ stack {{ {} }} }}\ntile size=\"1x1\" {{ stack {{ {} }} }}\n",
            vec!["rock"; 100].join("; "),
            vec!["d"; 99].join("; ")
        ),
    )]);
    let t = check(&src, "t.dtile", &legacy()).unwrap().tile.unwrap();
    assert_eq!(t.stack.len(), 9900);
}

#[test]
fn entries_inside_an_id_or_seed_call_get_their_own_seed_paths() {
    // was: both rocks got tile/a/rock/x, and seed=5 pinned both to 5
    let src = mem(&[(
        "t.dtile",
        "dharmatiles version=\"1.0.0\"; define two { stack { rock; rock size=\"9dmm\" } }\ntile size=\"1x1\" { stack { two id=x; two seed=5 } }\n",
    )]);
    let t = check(&src, "t.dtile", &legacy()).unwrap().tile.unwrap();
    let ids: Vec<_> = t.stack.iter().map(|e| e.id.to_string()).collect();
    assert_eq!(
        ids,
        [
            "tile/rock/x/rock/0",
            "tile/rock/x/rock/1",
            "tile/rock/2",
            "tile/rock/3"
        ]
    );
    let seeds: Vec<_> = t.stack.iter().map(|e| e.seed).collect();
    assert_eq!(seeds[..2], [None, None]);
    let (a, b) = (seeds[2].unwrap(), seeds[3].unwrap());
    assert!(a != b && a != 5 && b != 5, "{seeds:?}");
}

#[test]
fn a_value_out_of_range_is_refused_when_read() {
    // a parameter is checked at the call, against the control it feeds
    let e = errors(
        "dharmatiles version=1; define tall {\n    param size \"6dmm..9dmm\"\n    stack { rock size=size }\n}\n\
         tile size=\"1x1\" { stack { dirt; tall size=\"250dmm\" } }\n",
    );
    assert!(
        e[0].message.starts_with("`tall size` is "),
        "{}",
        e[0].message
    );
    // a tree's top over its foot is checked against a tree's heights,
    // through a call too
    let e = errors(
        "dharmatiles version=1; define tall {\n    stack { tree top=\"$bottom + 50dmm\" }\n}\n\
         tile size=\"1x1\" { stack { dirt; tall top=\"$bottom + 150dmm\" } }\n",
    );
    assert_eq!(
        e[0].message,
        "a tree is 20dmm to 120dmm tall over its foot, and this top makes it 150dmm"
    );
}
