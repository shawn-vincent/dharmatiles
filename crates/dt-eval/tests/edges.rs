//! Edge cases found by the Phase 1 exit review: each test is one repro,
//! kept so the fix stays fixed.

#![allow(clippy::unwrap_used)]

use dt_eval::{Memory, Options, check};

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
        ("../u/sub/t.kdl", "use \"./p\" p\ntile t { stack { p } }\n"),
        ("../u/sub/p.kdl", "define p { stack { ground } }\n"),
        ("u/sub/p.kdl", "define p { stack { ground level=9 } }\n"),
    ]);
    let t = check(&src, "../u/sub/t.kdl", &Options::default())
        .unwrap()
        .tile
        .unwrap();
    assert_eq!(t.stack[0].src.name(), "../u/sub/p.kdl");
}

fn errors(text: &str) -> Vec<dt_eval::Diag> {
    match check(&mem(&[("t.kdl", text)]), "t.kdl", &Options::default()) {
        Ok(_) => panic!("expected an error"),
        Err(e) => e,
    }
}

#[test]
fn a_huge_tile_is_refused_before_any_grid_is_made() {
    // was: check allocated 163 TB and aborted; 2^25 wrapped to 1 node
    for size in ["100000x100000", "33554432x1", "65x1"] {
        let e = errors(&format!(
            "tile a size=\"{size}\" {{ stack {{ ground }} }}\n"
        ));
        assert_eq!(e[0].code, "dt::tile", "{size}");
        assert!(e[0].help.is_some());
    }
}

#[test]
fn a_fill_point_on_its_line_is_an_empty_region() {
    let e = errors(
        "tile a {\n  line split { through west east }\n  region s { bounded-by split; fill \"0.5,0.5\" }\n  stack { ground in=s }\n}\n",
    );
    assert_eq!(e[0].code, "dt::empty-region");
    assert_eq!(e[0].location().unwrap().1, 3);
    assert!(e[0].help.as_deref().unwrap().contains("off the line"));
}

#[test]
fn a_fill_point_beside_its_line_is_not_empty() {
    let src = mem(&[(
        "t.kdl",
        "tile a {\n  line split { through west east }\n  region s { bounded-by split; fill \"0.5,0.2\" }\n  stack { ground in=s }\n}\n",
    )]);
    assert!(check(&src, "t.kdl", &Options::default()).is_ok());
}

#[test]
fn a_tile_with_nothing_in_it_is_an_error() {
    for text in ["tile a { stack { } }\n", "tile a { }\n"] {
        let e = errors(text);
        assert_eq!(e[0].code, "dt::empty", "{text}");
        assert_eq!(e[0].location().unwrap().1, 1);
    }
}

#[test]
fn definitions_that_multiply_stop_at_the_entry_cap() {
    // was: 262 144 entries and 4.6 GB in `check` from seven lines
    let mut text = String::from(
        "define f1 { stack { ground; ground; ground; ground; ground; ground; ground; ground } }\n",
    );
    for k in 2..=6 {
        let p = k - 1;
        text += &format!(
            "define f{k} {{ stack {{ f{p}; f{p}; f{p}; f{p}; f{p}; f{p}; f{p}; f{p} }} }}\n"
        );
    }
    text += "tile a { stack {\n  f6\n} }\n";
    let e = errors(&text);
    assert_eq!(e.len(), 1, "{e:?}");
    assert_eq!(e[0].code, "dt::too-many");
    assert_eq!(e[0].location().unwrap().1, 8);
}

#[test]
fn ten_thousand_entries_are_allowed() {
    let src = mem(&[(
        "t.kdl",
        &format!(
            "define d {{ stack {{ {} }} }}\ntile a {{ stack {{ {} }} }}\n",
            vec!["ground"; 100].join("; "),
            vec!["d"; 99].join("; ")
        ),
    )]);
    let t = check(&src, "t.kdl", &Options::default())
        .unwrap()
        .tile
        .unwrap();
    assert_eq!(t.stack.len(), 9900);
}
