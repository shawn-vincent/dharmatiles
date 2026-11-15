//! `cargo xtask parity`: the same tile in the browser as in the program.
//!
//! Builds the wasm bundle, runs `parity_line` (dt-web: the build's gate verdict, its audit numbers and the SHA-256
//! of its STL's triangles) for a fixed set of tiles under Node, runs the same function natively, and fails on
//! any difference, naming the tile. Manifold is C++ on both sides, built with floating-point contraction off
//! (`-ffp-contract=off`, third_party/manifold-csg-sys), so a fused multiply-add cannot make a borderline tile pass
//! in one and fail in the other. The rest of what makes the two sides one: the engine's transcendental maths is
//! `libm`'s everywhere (clippy's `disallowed-methods`), glam is scalar, and obvhs builds its BVH sequentially.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// What is built on both sides, `tile@system`: the torture set's coverage (an element each) and a few
/// library tiles, on both systems. Single-threaded wasm sets the budget: keep the whole run to a minute or two.
pub const SET: &[&str] = &[
    "lib:tests/ground/1x1-flat.dtile@db",
    "lib:tests/water/1x1-bands-pond.dtile@db",
    "lib:tests/walls/1x1-wall-cut-stone-corner.dtile@db",
    "lib:tests/plants/1x1-grove-small.dtile@db",
    "lib:tests/floors/1x1-floor-fieldstone.dtile@db",
    "lib:tests/rocks/1x1-rock-tops.dtile@db",
    "lib:tests/plants/1x1-grass-flowers.dtile@db",
    "lib:tests/walls/1x1-wall-brick.dtile@ol",
    "lib:tests/shapes/1x2-diagonal.dtile@db",
    "lib:tests/ground/1x1-soil.dtile@ol",
    "lib:tiles/ground/1x1-grass.dtile@db",
    "lib:tests/cliffs/1x1-cliff-needles.dtile@db",
    "lib:tests/cliffs/1x1-cliff-canyon.dtile@ol",
    "lib:tests/cliffs/1x1-cliff-escarpment.dtile@ol",
    "lib:tests/cliffs/1x1-cliff-plateau.dtile@ol",
];

pub fn run_parity(root: &Path) -> Result<(), String> {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), Into::into);
    let bundle = target.join("parity-bundle");
    crate::wasm::run_wasm(root, &[bundle.display().to_string()])?;

    let jobs: Vec<String> = SET.iter().map(|s| (*s).to_owned()).collect();
    let lines = |out: Vec<u8>| {
        String::from_utf8(out)
            .map(|s| s.lines().map(str::to_owned).collect::<Vec<_>>())
            .map_err(|e| format!("not UTF-8: {e}"))
    };
    let browser = capture(
        Command::new("node")
            .current_dir(root)
            .arg("xtask/parity.mjs")
            .arg(&bundle)
            .args(&jobs),
    )?;
    let native = capture(
        Command::new("cargo")
            .current_dir(root)
            .args([
                "run",
                "-q",
                "--release",
                "-p",
                "dt-web",
                "--example",
                "parity",
                "--",
            ])
            .arg(bundle.join("library.json"))
            .args(&jobs),
    )?;
    let (browser, native) = (lines(browser)?, lines(native)?);
    if browser.len() != jobs.len() || native.len() != jobs.len() {
        return Err(format!(
            "parity: {} tiles asked, the browser answered {}, the program {}",
            jobs.len(),
            browser.len(),
            native.len()
        ));
    }
    let mut different = Vec::new();
    for ((job, b), n) in jobs.iter().zip(&browser).zip(&native) {
        if b == n {
            println!("same      {job}");
        } else {
            println!("DIFFERENT {job}");
            different.push(describe(job, b, n));
        }
    }
    if different.is_empty() {
        println!("parity: {} tiles the same byte for byte", jobs.len());
        Ok(())
    } else {
        Err(format!(
            "parity: the browser and the program differ on {} of {} tiles\n{}",
            different.len(),
            jobs.len(),
            different.join("\n")
        ))
    }
}

/// Which fields of the two answers differ, by name.
fn describe(job: &str, browser: &str, native: &str) -> String {
    let (Ok(Value::Object(b)), Ok(Value::Object(n))) = (
        serde_json::from_str::<Value>(browser),
        serde_json::from_str::<Value>(native),
    ) else {
        return format!(
            "  {job}: an answer is not a JSON object\n    browser: {browser}\n    native:  {native}"
        );
    };
    let mut out = format!("  {job}:");
    for (k, bv) in &b {
        if n.get(k) != Some(bv) {
            out.push_str(&format!(
                "\n    {k}: browser {bv}, native {}",
                n.get(k).unwrap_or(&Value::Null)
            ));
        }
    }
    out
}

/// Run `cmd`, its stdout returned, its stderr shown; a failure is an error.
fn capture(cmd: &mut Command) -> Result<Vec<u8>, String> {
    let out = cmd
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("{cmd:?}: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!("{cmd:?} failed: {}", out.status))
    }
}
