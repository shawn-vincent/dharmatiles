//! The core stays WebAssembly-compatible: `dt-lang`, `dt-value`, `dt-doc` and
//! `dt-web` must compile for `wasm32-unknown-unknown`.  This shells out to
//! `cargo check --target`, the same check as `cargo xtask wasm --check`.

use std::process::Command;

#[test]
fn the_language_crates_compile_for_the_browser() {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["check", "--target", "wasm32-unknown-unknown"])
        .args([
            "-p", "dt-lang", "-p", "dt-value", "-p", "dt-doc", "-p", "dt-web",
        ])
        .output()
        .expect("cannot run cargo");
    assert!(
        out.status.success(),
        "a core crate no longer compiles for wasm32-unknown-unknown (is the target installed: `rustup target add wasm32-unknown-unknown`?)\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
