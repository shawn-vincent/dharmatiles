//! `cargo xtask wasm <out-dir>`: the browser build of the engine.
//!
//! Builds `dt-web` (the language crates and the engine behind dt-eval, with
//! Manifold compiled through wasm-cxx-shim, single-threaded) for
//! `wasm32-unknown-unknown`, runs `wasm-bindgen` over it
//! (`--target web`), shrinks it with `wasm-opt` when that is installed, and
//! writes the bundle: the `.wasm`, its JS glue, `library.json` (the library's
//! parts, printers and tiles, `lib:` path to text, which the page hands to the
//! module), and the page `web/wasm/index.html`, which runs on the site beside `assets/`: it reads `assets/theme.css` and builds in the cards' one worker, `assets/live/worker.js`.
//!
//! Compiling Manifold for wasm needs cmake, git and an LLVM clang with
//! `wasm-ld` (`brew install llvm lld cmake`); their Homebrew folders are put
//! on the path here when they exist.
//!
//! The files are written under names that carry their content's hash (`dt_web.<hash>.js`, `dt_web_bg.<hash>.wasm`,
//! `library.<hash>.json`) with a small manifest, `engine.json` ([`EngineManifest`]), which the page reads first: a glue
//! file can never be paired with a module it was not made with, even when a deploy lands in the middle of a visit, and
//! the page keeps the files (the Cache API) under the engine's hash.
//!
//! `cargo xtask wasm --check` only checks that the crates still compile for
//! the browser: the language crates, then the engine (the tests in
//! `dt-web/tests/wasm_core.rs` run the same two checks).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::run;

/// `engine.json`, the one definition of the manifest the page reads before anything of the engine: the files, by
/// the names that carry their hashes, and the engine's own hash (of the three), which the page keys everything it
/// keeps between visits by.  The page's TypeScript type (`docs/assets/live/engine-manifest.d.ts`) is generated from it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EngineManifest {
    /// The engine's hash: 16 hex digits of the three files' hashes together.
    pub hash: String,
    /// The glue, beside the manifest.
    pub js: String,
    /// The module.
    pub wasm: String,
    /// The library as JSON.
    pub library: String,
}

/// 16 hex digits of the SHA-256 of `bytes`.
fn short_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Rename `out/<plain>` to the name that carries its content's hash (`stem.<hash>.ext`); returns the new name and the hash.
fn hashed(out: &Path, plain: &str, stem: &str, ext: &str) -> Result<(String, String), String> {
    let from = out.join(plain);
    let bytes = fs::read(&from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    let hash = short_hash(&bytes);
    let name = format!("{stem}.{hash}.{ext}");
    fs::rename(&from, out.join(&name)).map_err(|e| format!("cannot name {name}: {e}"))?;
    Ok((name, hash))
}

/// Remove what an earlier run left under a hashed name (and the plain names of before the manifest), so a folder holds one engine.
fn clear_old(out: &Path) -> Result<(), String> {
    let read = |e: std::io::Error| format!("cannot read {}: {e}", out.display());
    for e in fs::read_dir(out).map_err(read)? {
        let name = e.map_err(read)?.file_name().to_string_lossy().into_owned();
        let ours = (name.starts_with("dt_web")
            && (name.ends_with(".js") || name.ends_with(".wasm")))
            || (name.starts_with("library") && name.ends_with(".json"))
            || name == "engine.json";
        if ours {
            fs::remove_file(out.join(&name)).map_err(|e| format!("cannot remove {name}: {e}"))?;
        }
    }
    Ok(())
}

/// The bundle's manifest, as `bundle` (a folder `cargo xtask wasm` wrote) says it.
///
/// # Errors
/// There is no readable manifest in `bundle`.
pub fn read_manifest(bundle: &Path) -> Result<EngineManifest, String> {
    let file = bundle.join("engine.json");
    let text =
        fs::read_to_string(&file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))
}

const TARGET: &str = "wasm32-unknown-unknown";

/// The language crates, which must keep compiling for the browser (no C++).
pub const CORE: [&str; 4] = ["dt-lang", "dt-value", "dt-doc", "dt-service"];

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

    clear_old(&out)?;
    run(Command::new("cargo")
        .current_dir(root)
        .env("PATH", path_with_llvm()?)
        .args(["build", "--release", "--target", TARGET, "-p", "dt-web"]))?;
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let built = target.join(TARGET).join("release/dt_web.wasm");
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

    // the three files under the names their contents give, and the manifest that says which they are
    let (js, jh) = hashed(&out, "dt_web.js", "dt_web", "js")?;
    let (wasm_name, wh) = hashed(&out, "dt_web_bg.wasm", "dt_web_bg", "wasm")?;
    let (library, lh) = hashed(&out, "library.json", "library", "json")?;
    let manifest = EngineManifest {
        hash: short_hash(format!("{jh}{wh}{lh}").as_bytes()),
        js,
        wasm: wasm_name,
        library,
    };
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("cannot write engine.json: {e}"))?;
    fs::write(out.join("engine.json"), text)
        .map_err(|e| format!("cannot write engine.json: {e}"))?;

    // the page runs on the site, beside `assets/`: it reads the one visual system and the cards' one build worker from there
    fs::copy(root.join("web/wasm/index.html"), out.join("index.html"))
        .map_err(|e| format!("cannot copy web/wasm/index.html: {e}"))?;

    for f in [
        "index.html",
        "engine.json",
        manifest.js.as_str(),
        manifest.wasm.as_str(),
        manifest.library.as_str(),
    ] {
        println!(
            "{:>10} bytes  {}",
            size(&out.join(f))?,
            out.join(f).display()
        );
    }
    println!("engine {}", manifest.hash);
    Ok(())
}

fn size(p: &Path) -> Result<u64, String> {
    fs::metadata(p)
        .map(|m| m.len())
        .map_err(|e| format!("cannot read {}: {e}", p.display()))
}

#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a probe where a tool that cannot be run is the answer false"
)]
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

#[cfg(test)]
mod tests {
    /// `docs/assets/live/engine-manifest.d.ts` is what [`super::EngineManifest`] says; `DT_CONTRACT=write` rewrites it.
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: a best-effort filesystem probe (a missing or unreadable file means none yet, read again on the next call); a failure that panics with its message named (a broken invariant or a failing test): shown loudly, not swallowed"
    )]
    fn the_engine_manifest_type_is_current() {
        let mut all = dt_doc::contract::Contract::default();
        all.add::<super::EngineManifest>();
        let want = format!(
            "// Generated from xtask/src/wasm.rs (`EngineManifest`) by `DT_CONTRACT=write cargo test -p xtask the_engine_manifest_type_is_current`.\n\
             // Do not edit: a test fails when this file is not what the Rust type says.  The cards' engine.js is typed by it.\n\n\
             {}export type {{ EngineManifest }};\n",
            all.declarations()
        );
        let file = crate::root().join("docs/assets/live/engine-manifest.d.ts");
        if std::env::var_os("DT_CONTRACT").is_some_and(|v| v == "write") {
            std::fs::write(&file, &want).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            return;
        }
        let have = std::fs::read_to_string(&file).unwrap_or_default();
        assert!(
            have == want,
            "{} is not what xtask's `EngineManifest` says; run\n  DT_CONTRACT=write cargo test -p xtask the_engine_manifest_type_is_current",
            file.display()
        );
    }

    #[test]
    fn a_hash_is_sixteen_hex_digits_of_the_content() {
        assert_eq!(super::short_hash(b"abc"), "ba7816bf8f01cfea");
        assert_ne!(super::short_hash(b"abc"), super::short_hash(b"abd"));
    }
}
