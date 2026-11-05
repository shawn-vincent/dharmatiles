//! The engine version for the build record: the workspace version, plus
//! the commit when built from git, and `-dirty` when the engine's inputs
//! differ from it: `0.0.0+g1ad8119`, `0.0.0+g1ad8119-dirty`, or plain
//! `0.0.0` without git.
//!
//! Only the engine's inputs count (the Rust crates, the bundled library
//! and the workspace manifests), so a docs edit elsewhere in the
//! repository neither marks the engine dirty nor re-runs this script.  An
//! unrelated git tag never names the engine (`git describe` once printed
//! `walls-fieldstone-e27-125-g…`).

use std::path::Path;
use std::process::Command;

/// The engine's inputs, relative to the workspace root.
const INPUTS: [&str; 6] = [
    "crates",
    "library",
    "xtask",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
];

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let root = Path::new(&manifest).join("../..");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    // Re-run on a commit or checkout (HEAD, the index) and on any edit to
    // the inputs (a directory is scanned whole).
    for p in ["HEAD", "index"] {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", p]) {
            println!("cargo::rerun-if-changed={}", root.join(path).display());
        }
    }
    for p in INPUTS {
        println!("cargo::rerun-if-changed={}", root.join(p).display());
    }
    let engine = match git(&root, &["rev-parse", "--short=7", "HEAD"]).filter(|h| !h.is_empty()) {
        Some(hash) => {
            let mut status = vec!["status", "--porcelain", "--"];
            status.extend(INPUTS);
            let dirty = git(&root, &status).is_some_and(|s| !s.is_empty());
            format!("{version}+g{hash}{}", if dirty { "-dirty" } else { "" })
        }
        None => version,
    };
    println!("cargo::rustc-env=DT_ENGINE_VERSION={engine}");
}
