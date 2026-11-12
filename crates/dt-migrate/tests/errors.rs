//! Deliberate mistakes in the real tiles, and what the author sees: a
//! miette report with the line and a hint.  Each report is compared with
//! its snapshot in tests/snapshots/; run with `UPDATE_SNAPSHOTS=1` to
//! rewrite them after a deliberate change, and review the diff.
//! `cargo run -p dt-migrate --example errors` prints the same reports.
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use std::path::PathBuf;

use common::*;
use miette::Diagnostic;

fn snapshot(slug: &str, shown: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(format!("{slug}.txt"));
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, shown).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "no snapshot {}; run with UPDATE_SNAPSHOTS=1",
            path.display()
        )
    });
    assert_eq!(
        shown, want,
        "snapshot {slug} changed; UPDATE_SNAPSHOTS=1 rewrites it"
    );
}

#[test]
fn every_case_matches_its_snapshot() {
    for case in &CASES {
        let (_, shown) = case.report();
        snapshot(case.slug, &shown);
    }
}

#[test]
fn every_error_has_a_line_and_a_hint() {
    for case in &CASES {
        let (errors, shown) = case.report();
        for e in &errors {
            assert!(e.help().is_some(), "{}: no hint\n{shown}", case.title);
            assert!(e.labels().is_some(), "{}", case.title);
            assert!(!e.span.is_empty(), "{}", case.title);
        }
        // The report shows the file name, a line and a column.
        assert!(shown.contains(&format!("[{}:", case.tile.0)), "{shown}");
    }
}

#[test]
fn spans_point_at_the_mistake() {
    let at = |case: &Case, i: usize| {
        let (e, _) = case.report();
        let b = case.broken();
        let s = e[i].span;
        b[s.offset()..s.offset() + s.len()].to_owned()
    };
    let case = |slug: &str| CASES.iter().find(|c| c.slug == slug).unwrap();
    assert_eq!(at(case("typo"), 0), "clumsp");
    assert_eq!(at(case("unit-suffix"), 0), "mm");
    assert_eq!(at(case("kdl-v1"), 0), "true");
    assert_eq!(at(case("missing-along"), 0), "wall");
    assert_eq!(at(case("unquoted-range"), 0), "1..6");
    assert_eq!(at(case("old-vocabulary"), 0), "hills");
    assert_eq!(at(case("thing-count"), 0), "count");
    assert_eq!(at(case("expert-in-tile"), 0), "branch");
    assert_eq!(at(case("side-on-edge"), 0), "left");
}

#[test]
fn decoding_goes_on_past_the_first_mistake() {
    let case = CASES.iter().find(|c| c.slug == "several").unwrap();
    let (e, shown) = case.report();
    assert_eq!(e.len(), 2, "{shown}");
    assert_eq!(e[0].help.as_deref(), Some("did you mean `mixed`?"));
    assert!(
        e[1].help.as_deref().unwrap().contains("\"135%\""),
        "{shown}"
    );
}
