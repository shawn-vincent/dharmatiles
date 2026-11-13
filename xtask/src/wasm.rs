//! `cargo xtask wasm <out-dir>`: the browser build of the engine.
//!
//! Builds `dt-web` (the language crates and the engine behind dt-eval, with
//! Manifold compiled through wasm-cxx-shim, single-threaded) for
//! `wasm32-unknown-unknown`, runs `wasm-bindgen` over it
//! (`--target web`), shrinks it with `wasm-opt` when that is installed, and
//! writes the bundle: the `.wasm`, its JS glue, `library.json` (the library's
//! parts, printers and tiles, `lib:` path to text, which the page hands to the
//! module), the page `web/wasm/index.html` and its build worker `worker.js`.
//!
//! Compiling Manifold for wasm needs cmake, git and an LLVM clang with
//! `wasm-ld` (`brew install llvm lld cmake`); their Homebrew folders are put
//! on the path here when they exist.
//!
//! `cargo xtask wasm --check` only checks that the crates still compile for
//! the browser: the language crates, then the engine (the tests in
//! `dt-web/tests/wasm_core.rs` run the same two checks).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::run;

const TARGET: &str = "wasm32-unknown-unknown";

/// The language crates, which must keep compiling for the browser (no C++).
pub const CORE: [&str; 3] = ["dt-lang", "dt-value", "dt-doc"];

/// The engine, which must too: dt-web brings in dt-eval, dt-elements, dt-kit
/// and dt-geom (and Manifold, so this needs the C++ toolchain).
pub const ENGINE: [&str; 2] = ["dt-eval", "dt-web"];

/// `PATH` with the Homebrew LLVM and lld folders in front, where they exist.
fn path_with_llvm() -> Result<std::ffi::OsString, String> {
    let mut dirs: Vec<PathBuf> = ["/opt/homebrew/opt/lld/bin", "/opt/homebrew/opt/llvm/bin"]
        .iter()
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .collect();
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    std::env::join_paths(dirs).map_err(|e| format!("cannot extend PATH: {e}"))
}

/// `cargo check` of `crates` for the browser.
pub fn check_crates(root: &Path, crates: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(root)
        .env("PATH", path_with_llvm()?)
        .args(["check", "--target", TARGET]);
    for c in crates {
        cmd.args(["-p", c]);
    }
    run(&mut cmd).map_err(|e| {
        format!(
            "{} no longer compile for {TARGET} (is the target installed: `rustup target add {TARGET}`?): {e}",
            crates.join(", ")
        )
    })
}

/// Both checks.
pub fn check(root: &Path) -> Result<(), String> {
    check_crates(root, &CORE)?;
    check_crates(root, &ENGINE)
}

pub fn run_wasm(root: &Path, args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--check") {
        return check(root);
    }
    let [out] = args else {
        return Err("usage: cargo xtask wasm <out-dir> | cargo xtask wasm --check".to_owned());
    };
    let out = PathBuf::from(out);
    fs::create_dir_all(&out).map_err(|e| format!("cannot make {}: {e}", out.display()))?;

    run(Command::new("cargo")
        .current_dir(root)
        .env("PATH", path_with_llvm()?)
        .args(["build", "--release", "--target", TARGET, "-p", "dt-web"]))?;
    let built = root.join("target").join(TARGET).join("release/dt_web.wasm");
    run(Command::new("wasm-bindgen")
        .args(["--target", "web", "--no-typescript", "--out-name", "dt_web"])
        .arg("--out-dir")
        .arg(&out)
        .arg(&built))
    .map_err(|e| {
        format!(
            "{e}\n(wasm-bindgen-cli must match the wasm-bindgen crate: `cargo install wasm-bindgen-cli --version 0.2.130 --locked`)"
        )
    })?;

    let wasm = out.join("dt_web_bg.wasm");
    let before = size(&wasm)?;
    if which("wasm-opt") {
        let shrunk = out.join("dt_web_bg.opt.wasm");
        run(Command::new("wasm-opt")
            .args([
                "-Oz",
                "--enable-bulk-memory",
                "--enable-nontrapping-float-to-int",
            ])
            .arg(&wasm)
            .arg("-o")
            .arg(&shrunk))?;
        fs::rename(&shrunk, &wasm).map_err(|e| format!("cannot replace the wasm: {e}"))?;
        println!("wasm-opt -Oz: {} -> {} bytes", before, size(&wasm)?);
    } else {
        println!("wasm-opt not installed: the .wasm is as wasm-bindgen wrote it");
    }

    // the library the page hands to the module
    let mut lib = BTreeMap::new();
    for group in ["parts", "printers", "tiles", "tests"] {
        collect(
            &root.join("library"),
            &root.join("library").join(group),
            &mut lib,
        )?;
    }
    let json =
        serde_json::to_string(&lib).map_err(|e| format!("cannot write library.json: {e}"))?;
    fs::write(out.join("library.json"), json)
        .map_err(|e| format!("cannot write library.json: {e}"))?;

    for page in ["index.html", "worker.js"] {
        let from = root.join("web/wasm").join(page);
        fs::copy(&from, out.join(page))
            .map_err(|e| format!("cannot copy {}: {e}", from.display()))?;
    }

    for f in [
        "index.html",
        "worker.js",
        "dt_web.js",
        "dt_web_bg.wasm",
        "library.json",
    ] {
        println!(
            "{:>10} bytes  {}",
            size(&out.join(f))?,
            out.join(f).display()
        );
    }
    Ok(())
}

fn size(p: &Path) -> Result<u64, String> {
    fs::metadata(p)
        .map(|m| m.len())
        .map_err(|e| format!("cannot read {}: {e}", p.display()))
}

fn which(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Every `.dtile` under `dir`, as `lib:` paths relative to `root`.
fn collect(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<(), String> {
    let read = |e: std::io::Error| format!("cannot read {}: {e}", dir.display());
    for e in fs::read_dir(dir).map_err(read)? {
        let p = e.map_err(read)?.path();
        if p.is_dir() {
            collect(root, &p, out)?;
        } else if p.extension().is_some_and(|x| x == "dtile") {
            let rel = p
                .strip_prefix(root)
                .map_err(|e| format!("{}: {e}", p.display()))?;
            let text =
                fs::read_to_string(&p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
            out.insert(
                format!("lib:{}", rel.to_string_lossy().replace('\\', "/")),
                text,
            );
        }
    }
    Ok(())
}
