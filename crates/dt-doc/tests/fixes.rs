//! A suggestion is made in one place: `dt_doc::error::suggest` writes the help and builds the fix together, so no
//! diagnostic can say "did you mean" without carrying the edit the page turns into a button.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n != "tests" && n != "target") {
                sources(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn the_words_of_a_suggestion_are_written_once() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for c in std::fs::read_dir(&crates).unwrap() {
        let src = c.unwrap().path().join("src");
        if src.is_dir() {
            sources(&src, &mut files);
        }
    }
    assert!(files.len() > 50, "found the sources");
    let mut found = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.contains("did you mean") && !code.starts_with("//") {
                found.push(format!(
                    "{}:{}",
                    f.strip_prefix(&crates).unwrap().display(),
                    n + 1
                ));
            }
        }
    }
    let places: Vec<&str> = found.iter().map(|f| f.split(':').next().unwrap()).collect();
    assert_eq!(
        places,
        ["dt-doc/src/error.rs"],
        "{found:?}: write a suggestion with DocError::suggest or Diag::suggest (help and fix together), never as help text"
    );
}

#[test]
fn a_suggestion_says_its_words_and_carries_its_edit() {
    let text = "tile {\n  dirtt\n}\n";
    let (help, fix) = dt_doc::error::suggest(text, (9, 5).into(), "dirt");
    assert_eq!(help, "did you mean `dirt`?");
    assert_eq!((fix.offset, fix.len, fix.was.as_str()), (9, 5, "dirtt"));
    assert_eq!(
        (fix.replacement.as_str(), fix.label.as_str()),
        ("dirt", "Change to dirt")
    );
}
