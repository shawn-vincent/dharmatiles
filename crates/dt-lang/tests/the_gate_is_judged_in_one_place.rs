//! The printability gate is judged in one place: `dt_eval::Built::verdict` (which reads the audit in `dt-kit`).  A front
//! end that calls `.failures(` itself is deciding "passed" a second way, and this test fails on it.

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
fn nothing_outside_the_audit_and_dt_eval_reads_the_gates_failures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("xtask"), &mut files);
    assert!(files.len() > 50, "found only {} Rust files", files.len());
    // the pattern is built here so that this file does not contain it
    let call = format!(".{}(", "failures");
    let mut found = Vec::new();
    for f in files {
        let rel = f.strip_prefix(&root).unwrap_or(&f).to_path_buf();
        if rel.starts_with("crates/dt-kit/src/audit")
            || rel.starts_with("crates/dt-eval/")
            || rel.ends_with("the_gate_is_judged_in_one_place.rs")
        {
            continue;
        }
        let text = std::fs::read_to_string(&f).expect("a readable Rust file");
        for (n, line) in text.lines().enumerate() {
            if line.contains(&call) {
                found.push(format!("{}:{}: {}", rel.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "the gate's failures are read outside dt-kit's audit and dt-eval; ask `Built::verdict` (dt_eval::Verdict) instead:\n{}",
        found.join("\n")
    );
}
