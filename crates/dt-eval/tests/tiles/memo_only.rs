//! The memo is the only way a test in this binary builds.
//!
//! A tile built twice with the same inputs is time spent for nothing, and
//! across the test files it was: 78 builds of one flat tile.  So every test
//! builds through [`memo::build`](crate::memo::build), which keeps one build
//! for everything the build reads.  A test that must build fresh (it tests
//! the build, the cache or the threads, or builds twice to compare) calls
//! `dt_eval::build` (or `build_with`, `build_cached`, `build_checked`) itself
//! and says why in a comment starting `fresh:` on that line or the one above,
//! and passes its options through `dt_testkit::seeded`, so a run at another
//! seed (`DT_TEST_SEED`) builds it on that seed too, like the memo's builds it
//! is compared with.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failing unwrap is the failure"
)]

use std::path::Path;

/// The builds a test may not call without saying why.
const BUILDS: [&str; 4] = ["build", "build_with", "build_cached", "build_checked"];

/// The words of `line` outside its trailing comment that name a build: a call
/// not made through the memo (`memo::build(` and a method `.build(` are not
/// it), a path to one in `dt_eval`, or one imported from `dt_eval`.
fn names_a_build(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or("");
    // a path through the crate: `dt_eval::build(…)`, or `dt_eval::{…, build, …}` on one line
    if BUILDS.iter().any(|b| {
        code.match_indices(&format!("dt_eval::{b}")).any(|(i, m)| {
            !code[i + m.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
    }) {
        return true;
    }
    for b in BUILDS {
        let mut from = 0;
        while let Some(i) = code[from..].find(b) {
            let at = from + i;
            from = at + b.len();
            let before = code[..at].chars().next_back();
            let after = code[from..].chars().next();
            let word = !before.is_some_and(|c| c.is_alphanumeric() || c == '_');
            let call = after == Some('(');
            let through_the_memo = code[..at].ends_with("memo::");
            let method = before == Some('.');
            if word && call && !through_the_memo && !method && !code.contains(&format!("fn {b}(")) {
                return true;
            }
        }
    }
    false
}

/// Whether the build on `lines[at]` says why it is fresh.
fn says_why(lines: &[&str], at: usize) -> bool {
    let says = |l: &str| {
        l.split_once("//")
            .is_some_and(|(_, c)| c.trim_start().starts_with("fresh:"))
    };
    says(lines[at])
        || (at > 0 && lines[at - 1].trim_start().starts_with("//") && says(lines[at - 1]))
}

/// Whether the build that starts on `lines[at]` takes the test seed: its options go through `seeded`, on that
/// line or the call's next lines (a call `cargo fmt` breaks over several, up to the one that ends it).
fn takes_the_seed(lines: &[&str], at: usize) -> bool {
    let code = |l: &str| l.split("//").next().unwrap_or("").to_owned();
    // the call's open brackets: it ends on the line that closes them
    let mut open = 0usize;
    for l in &lines[at..] {
        let c = code(l);
        if c.contains("seeded(") {
            return true;
        }
        open = (open + c.matches('(').count()).saturating_sub(c.matches(')').count());
        if open == 0 {
            return false;
        }
    }
    false
}

/// Every offending line of the test sources in `dir`, as `file:line: text`.
fn offences(dir: &Path, skip: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    for p in files {
        let name = p.file_name().unwrap().to_str().unwrap().to_owned();
        if skip.contains(&name.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if l.trim_start().starts_with("//") {
                continue;
            }
            // an import is no call: the call that follows is the one seen
            if l.trim_start().starts_with("use ") {
                continue;
            }
            if names_a_build(l) && (!says_why(&lines, i) || !takes_the_seed(&lines, i)) {
                out.push(format!("{name}:{}: {}", i + 1, l.trim()));
            }
        }
    }
    out
}

#[test]
fn the_memo_is_the_only_way_a_test_builds() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tiles");
    // the memo is where the one build is made; this file names the calls in words
    let bad = offences(&dir, &["memo.rs", "memo_only.rs"]);
    assert!(
        bad.is_empty(),
        "a test builds without the memo; call `memo::build`, or say why it must build fresh in a `// fresh: …` comment on that line or the one above and pass its options through `dt_testkit::seeded`:\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_scan_sees_a_build_that_skips_the_memo() {
    assert!(names_a_build(
        "    let b = build(&src, \"t.dtile\", &o).unwrap();"
    ));
    assert!(names_a_build(
        "    let b = dt_eval::build(&src, p, &o).unwrap();"
    ));
    assert!(names_a_build("    build_cached(&src, p, &o, &none, None)"));
    assert!(!names_a_build(
        "    let b = memo::build(&src, p, &o).unwrap();"
    ));
    assert!(!names_a_build("    pool.build().unwrap()"));
    assert!(!names_a_build("    let x = 1; // build(…) in a comment"));
    let lines = [
        "// fresh: why",
        "let a = dt_eval::build(&s, p, &o);",
        "let b = dt_eval::build(&s, p, &o); // fresh: why",
        "let c = dt_eval::build(&s, p, &o);",
    ];
    assert!(says_why(&lines, 1) && says_why(&lines, 2) && !says_why(&lines, 3));
    assert!(takes_the_seed(
        &["dt_eval::build(&s, p, &dt_testkit::seeded(&o)); // fresh: why"],
        0
    ));
    assert!(takes_the_seed(
        &[
            "build_cached(",
            "    src,",
            "    &dt_testkit::seeded(opts),",
            ")"
        ],
        0
    ));
    assert!(!takes_the_seed(
        &[
            "dt_eval::build(&s, p, &o); // fresh: seeded(…) in a comment",
            "seeded(x)"
        ],
        0
    ));
}
