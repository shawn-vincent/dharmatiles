//! The core stays WebAssembly-compatible: `dt-lang`, `dt-value` and `dt-doc`
//! must compile for `wasm32-unknown-unknown`, and so must the engine
//! (`dt-eval`, and `dt-web` over it).  These shell out to
//! `cargo check --target`, the same checks as `cargo xtask wasm --check`.
//!
//! The engine's check compiles Manifold with clang, so it needs
//! `brew install llvm lld cmake` and runs with `--ignored` (`cargo xtask wasm
//! --check` runs both).

use std::process::Command;

fn check(crates: &[&str]) {
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()));
    cmd.current_dir(env!("CARGO_MANIFEST_DIR")).args([
        "check",
        "--target",
        "wasm32-unknown-unknown",
    ]);
    // clang with a wasm backend and wasm-ld, where Homebrew puts them
    let mut path: Vec<std::path::PathBuf> =
        ["/opt/homebrew/opt/lld/bin", "/opt/homebrew/opt/llvm/bin"]
            .iter()
            .map(std::path::PathBuf::from)
            .filter(|d| d.is_dir())
            .collect();
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    cmd.env("PATH", std::env::join_paths(path).expect("PATH"));
    for c in crates {
        cmd.args(["-p", c]);
    }
    let out = cmd.output().expect("cannot run cargo");
    assert!(
        out.status.success(),
        "{crates:?} no longer compile for wasm32-unknown-unknown (is the target installed: `rustup target add wasm32-unknown-unknown`?)\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn the_language_crates_compile_for_the_browser() {
    check(&["dt-lang", "dt-value", "dt-doc"]);
}

#[test]
#[ignore = "compiles Manifold with clang: needs llvm, lld and cmake (brew install llvm lld cmake)"]
fn the_engine_compiles_for_the_browser() {
    check(&["dt-eval", "dt-web"]);
}
