//! A suggestion is made in one place: `dt_doc::error::suggest` writes the help and builds the fix together, so no
//! diagnostic can say "did you mean" without carrying the edit the page turns into a button.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

mod common;

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

#[test]
fn the_camelcase_advice_is_written_once_and_is_never_bare_text() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for c in std::fs::read_dir(&crates).unwrap() {
        let src = c.unwrap().path().join("src");
        if src.is_dir() {
            sources(&src, &mut files);
        }
    }
    let mut found = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        for (n, line) in text.lines().enumerate() {
            if line.contains("camelCase: write") && !line.trim_start().starts_with("//") {
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
        "{found:?}: advise an old spelling with DocError::advise_old (help and fix together)"
    );
}

/// The fix of the first error of `text`: the old word, what replaces it, and the help beside it.
fn only_fix(text: &str) -> (String, String, String) {
    let (errors, _) = common::fails(text);
    let e = &errors[0];
    let f = e
        .fix
        .as_ref()
        .unwrap_or_else(|| panic!("{}: no fix, help {:?}", e.message, e.help));
    assert_eq!(
        &text[f.offset..f.offset + f.len],
        f.was,
        "the span is the old word"
    );
    (
        f.was.clone(),
        f.replacement.clone(),
        e.help.clone().unwrap(),
    )
}

#[test]
fn an_old_hyphenated_control_name_carries_the_camelcase_edit() {
    let entry = "dharmatiles version=1\ntile size=\"1x1\" seed=1 {\n    stack {\n        scatter per-square=3 { rock }\n    }\n}\n";
    assert_eq!(
        only_fix(entry),
        (
            "per-square".to_owned(),
            "perSquare".to_owned(),
            "control names are camelCase: write `perSquare`".to_owned()
        )
    );
    let region = "dharmatiles version=1\ntile size=\"1x1\" seed=1 {\n    line a { through \"(0sq, 0sq)\" \"(1sq, 1sq)\" }\n    region r { bounded-by a }\n    stack {\n        dirt\n    }\n}\n";
    let (was, now, help) = only_fix(region);
    assert_eq!((was.as_str(), now.as_str()), ("bounded-by", "boundedBy"));
    assert!(help.contains("camelCase"));
}
