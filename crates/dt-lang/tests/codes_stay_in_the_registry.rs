//! Every diagnostic code comes from the registry (`src/codes.rs`): a `dt::…` code written as a string anywhere else
//! in the program, its tests or its chores is a second path to the same fact, and this test fails on it.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
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

#[test]
fn no_code_is_written_outside_the_registry() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("xtask"), &mut files);
    assert!(files.len() > 50, "found only {} Rust files", files.len());
    // the pattern is built here so that this file does not contain it
    let quote_then_prefix = format!("\"{}", "dt::");
    let registry = Path::new("dt-lang/src/codes.rs");
    let mut found = Vec::new();
    for f in files {
        if f.ends_with(registry) {
            continue;
        }
        let text = std::fs::read_to_string(&f).expect("a readable Rust file");
        for (n, line) in text.lines().enumerate() {
            let mut rest = line;
            while let Some(i) = rest.find(&quote_then_prefix) {
                let after = &rest[i + quote_then_prefix.len()..];
                // `"dt::"` alone (a prefix test) is not a code
                if after.starts_with(|c: char| c.is_ascii_lowercase()) {
                    found.push(format!("{}:{}: {}", f.display(), n + 1, line.trim()));
                }
                rest = after;
            }
        }
    }
    assert!(
        found.is_empty(),
        "a diagnostic code is written outside dt-lang/src/codes.rs; use its constant (dt_lang::codes::NAME), and add the code there if it is new:\n{}",
        found.join("\n")
    );
}
