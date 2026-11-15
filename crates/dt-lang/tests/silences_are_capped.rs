//! A silence is a reviewed choice.  `clippy.toml` refuses `Result::ok`, `unwrap_or`, `unwrap_or_default` and the
//! like (docs/standards/errors.html); a site that keeps one carries an `expect` of `clippy::disallowed_methods` with
//! its reason.  This counts those sites and fails when the count rises above the number recorded here, so a new
//! silence is a visible change in the same commit.  Lowering the count is free: lower the number with it.

use std::path::{Path, PathBuf};

/// The sites in the workspace's Rust that keep a refused call under an expectation of that lint.
const SILENCES: usize = 165;

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("a readable directory");
    for e in entries {
        let path = e.expect("a readable directory entry").path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n == "target" || n == "node_modules")
            {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// How many times `needle` follows an `expect(` (rustfmt may break the line between them).
fn count(text: &str, needle: &str) -> usize {
    text.match_indices("expect(")
        .filter(|(i, _)| text[i + "expect(".len()..].trim_start().starts_with(needle))
        .count()
}

#[test]
fn the_silences_never_grow_without_the_number_changing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("xtask"), &mut files);
    assert!(files.len() > 50, "found only {} Rust files", files.len());
    // the patterns are built here so that this file does not count itself
    let lint = format!("clippy::{}", "disallowed_methods");
    let undecided = format!("{} silence", "unreviewed");
    let (mut silences, mut unreviewed) = (0, 0);
    for f in files {
        let text = std::fs::read_to_string(&f).expect("a readable Rust file");
        silences += count(&text, &lint);
        unreviewed += text.matches(&undecided).count();
    }
    assert!(
        silences <= SILENCES,
        "{silences} `expect({})` sites, over the {SILENCES} recorded: make the failure travel (`?`), or make the \
         conversion infallible, instead of a new silence; a permitted kind (docs/standards/errors.html) raises SILENCES \
         in this file, in the same commit",
        "disallowed_methods"
    );
    assert_eq!(
        unreviewed, 0,
        "{unreviewed} undecided silences: a silence is never marked unreviewed; show the failure, make it infallible, \
         or name its permitted kind (docs/standards/errors.html)"
    );
}
