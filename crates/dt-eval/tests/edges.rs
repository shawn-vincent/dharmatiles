//! Edge cases found in review: each test is one repro,
//! kept so the fix stays fixed.

#![allow(clippy::unwrap_used)]

use dt_eval::{Memory, Options, check};

/// These tests write format 0 documents: built as written, not migrated as
/// they load (that is `migrate.rs`'s business).
fn legacy() -> Options {
    Options {
        current_format: 0,
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

#[test]
fn a_use_from_a_file_above_the_working_folder_keeps_its_dot_dots() {
    let src = mem(&[
        ("../u/sub/t.dtile", "use \"./p\" p\ntile { stack { p } }\n"),
        ("../u/sub/p.dtile", "define p { stack { dirt } }\n"),
        ("u/sub/p.dtile", "define p { stack { rock size=9 } }\n"),
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
        let e = errors(&format!("tile size=\"{size}\" {{ stack {{ dirt }} }}\n"));
        assert_eq!(e[0].code, "dt::tile", "{size}");
        assert!(e[0].help.is_some());
    }
}

#[test]
fn a_fill_point_on_its_line_is_an_empty_region() {
    let e = errors(
        "tile {\n  line split { through west east }\n  region s { boundedBy split; fill \"0.5,0.5\" }\n  stack { dirt at=s }\n}\n",
    );
    assert_eq!(e[0].code, "dt::empty-region");
    assert_eq!(e[0].location().unwrap().1, 3);
    assert!(e[0].help.as_deref().unwrap().contains("off the line"));
}

#[test]
fn a_fill_point_beside_its_line_is_not_empty() {
    let src = mem(&[(
        "t.dtile",
        "tile {\n  line split { through west east }\n  region s { boundedBy split; fill \"0.5,0.2\" }\n  stack { dirt at=s }\n}\n",
    )]);
    assert!(check(&src, "t.dtile", &legacy()).is_ok());
}

#[test]
fn a_tile_with_nothing_in_it_is_an_error() {
    for text in ["tile { stack { } }\n", "tile { }\n"] {
        let e = errors(text);
        assert_eq!(e[0].code, "dt::empty", "{text}");
        assert_eq!(e[0].location().unwrap().1, 1);
    }
}

#[test]
fn definitions_that_multiply_stop_at_the_entry_cap() {
    // was: 262 144 entries and 4.6 GB in `check` from seven lines
    let mut text =
        String::from("define f1 { stack { dirt; dirt; dirt; dirt; dirt; dirt; dirt; dirt } }\n");
    for k in 2..=6 {
        let p = k - 1;
        text += &format!(
            "define f{k} {{ stack {{ f{p}; f{p}; f{p}; f{p}; f{p}; f{p}; f{p}; f{p} }} }}\n"
        );
    }
    text += "tile { stack {\n  f6\n} }\n";
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
            "define d {{ stack {{ {} }} }}\ntile {{ stack {{ {} }} }}\n",
            vec!["dirt"; 100].join("; "),
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
        "define two { stack { rock; rock size=9 } }\ntile { stack { two id=x; two seed=5 } }\n",
    )]);
    let t = check(&src, "t.dtile", &legacy()).unwrap().tile.unwrap();
    let ids: Vec<_> = t.stack.iter().map(|e| e.id.clone()).collect();
    assert_eq!(
        ids,
        [Some("x/rock/0".into()), Some("x/rock/1".into()), None, None]
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
        "define tall {\n    param size \"6..9\"\n    stack { rock size=size }\n}\n\
         tile { stack { dirt; tall size=250 } }\n",
    );
    assert!(
        e[0].message.starts_with("`tall size` is "),
        "{}",
        e[0].message
    );
    // a tree's top over its foot is checked against a tree's heights,
    // through a call too
    let e = errors(
        "define tall {\n    stack { tree top=\"bottom + 50\" }\n}\n\
         tile { stack { dirt; tall top=\"bottom + 150\" } }\n",
    );
    assert_eq!(
        e[0].message,
        "a tree is 20 mm to 120 mm tall over its foot, and this top makes it 150 mm"
    );
}
