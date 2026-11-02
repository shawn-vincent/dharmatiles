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
