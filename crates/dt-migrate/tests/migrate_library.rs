//! The library as it was written in format 0 (the fixtures), carried to
//! format 1 in memory: every file reads as format 1 afterwards, and carrying
//! the result over again changes nothing.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use dt_doc::{Known, decode};
use dt_migrate::files::{Legacy, Moved, Reader, migrate_files};

struct Library0;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library0")
}

impl Reader for Library0 {
    fn read(&self, path: &str) -> Result<String, String> {
        let rel = path
            .strip_prefix("lib:")
            .ok_or_else(|| format!("`{path}` is not in the library"))?;
        std::fs::read_to_string(root().join(rel)).map_err(|e| format!("{path}: {e}"))
    }
}

fn walk(dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "dtile") {
            let rel = p.strip_prefix(root()).unwrap();
            out.push(format!("lib:{}", rel.to_string_lossy()));
        }
    }
}

fn migrated() -> Vec<Moved> {
    let mut paths = Vec::new();
    walk(&root(), &mut paths);
    paths.sort();
    assert!(paths.len() > 50, "found {} files", paths.len());
    let old = Legacy(&Library0);
    migrate_files(&old, &paths, "1.0.0").unwrap_or_else(|e| panic!("{e:#?}"))
}

#[test]
fn every_format_0_library_file_migrates_and_reads_as_format_1() {
    let moved = migrated();
    let text = |p: &str| {
        moved
            .iter()
            .find(|m| m.path == p)
            .unwrap_or_else(|| panic!("{p} was not migrated"))
            .text
            .clone()
    };
    let mut known = Known::new();
    known
        .add_heights(
            "lib:parts/heights.dtile",
            &std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/parts/heights.dtile"),
            )
            .unwrap(),
        )
        .unwrap();
    for part in ["grass", "trees"] {
        let path = format!("lib:parts/{part}.dtile");
        let doc = decode(&path, &text(&path), &known)
            .unwrap_or_else(|e| panic!("{path}: {e:?}\n{}", text(&path)));
        known.add(&format!("parts/{part}"), &doc);
    }
    for m in &moved {
        if m.path.starts_with("lib:palettes/") {
            continue;
        }
        decode(&m.path, &m.text, &known)
            .unwrap_or_else(|e| panic!("{}: {e:?}\n{}", m.path, m.text));
    }
}

#[test]
fn migrating_the_result_again_changes_nothing() {
    struct Moved1(Vec<Moved>);
    impl Reader for Moved1 {
        fn read(&self, path: &str) -> Result<String, String> {
            self.0
                .iter()
                .find(|m| m.path == path)
                .map(|m| m.text.clone())
                .ok_or_else(|| format!("no `{path}`"))
        }
    }
    let first = Moved1(migrated());
    let paths: Vec<String> = first.0.iter().map(|m| m.path.clone()).collect();
    let again = migrate_files(&first, &paths, "1.0.0").unwrap();
    assert!(
        again.is_empty(),
        "{:?}",
        again.iter().map(|m| &m.path).collect::<Vec<_>>()
    );
}
