//! `cargo xtask parity`: the same tile in the browser as in the program.
//!
//! Builds the wasm bundle, runs `parity_line` (dt-web: the build's gate verdict, its audit numbers and the SHA-256
//! of its STL's triangles) for a fixed set of tiles under Node, runs the same function natively, and fails on
//! any difference, naming the tile. Manifold is C++ on both sides, built with floating-point contraction off
//! (`-ffp-contract=off`, third_party/manifold-csg-sys), so a fused multiply-add cannot make a borderline tile pass
//! in one and fail in the other. The rest of what makes the two sides one: the engine's transcendental maths is
//! `libm`'s everywhere (clippy's `disallowed-methods`), glam is scalar, and obvhs builds its BVH sequentially.

use std::path::{Path, PathBuf};
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
    "lib:tiles/cliffs/1x1-cliff-spire.dtile@db",
    "lib:tiles/cliffs/1x1-cliff-spire.dtile@ol",
    "lib:tiles/cliffs/1x1-cliff-corner.dtile@db",
    "lib:tiles/cliffs/1x1-cliff-mound.dtile@db",
    "lib:tests/cliffs/1x1-cliff-columns.dtile@ol",
];

#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: an environment fact that is not readable is the meaning (the shard count falls back to 4)"
)]
pub fn run_parity(root: &Path, args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--too-big") {
        return run_too_big(root);
    }
    if args.iter().any(|a| a == "--service") {
        return run_service(root);
    }
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
    // the browser half is single-threaded wasm, one tile after another in a Node process: the tiles are spread over a
    // few processes, and the native half runs beside them, so the run takes the slowest share and not the sum
    let shards = std::thread::available_parallelism()
        .map_or(4, |n| n.get() / 2)
        .clamp(1, jobs.len().max(1));
    let library = bundle.join(crate::wasm::read_manifest(&bundle)?.library);
    let (shares, native) = std::thread::scope(|s| {
        let native = s.spawn(|| {
            capture(
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
                    .arg(&library)
                    .args(&jobs),
            )
        });
        let shares: Vec<_> = (0..shards)
            .map(|k| {
                // tile i goes to process i % shards, so each share's answers come back in its own order
                let mine: Vec<&String> = jobs.iter().skip(k).step_by(shards).collect();
                let bundle = &bundle;
                s.spawn(move || {
                    capture(
                        Command::new("node")
                            .current_dir(root)
                            .arg("xtask/parity.mjs")
                            .arg(bundle)
                            .args(mine),
                    )
                })
            })
            .collect();
        let shares: Vec<Result<Vec<u8>, String>> = shares
            .into_iter()
            .map(|h| match h.join() {
                Ok(done) => done,
                Err(_) => Err("a parity process panicked".to_owned()),
            })
            .collect();
        let native = match native.join() {
            Ok(done) => done,
            Err(_) => Err("the native parity build panicked".to_owned()),
        };
        (shares, native)
    });
    let shares = shares
        .into_iter()
        .map(|r| r.and_then(lines))
        .collect::<Result<Vec<_>, String>>()?;
    // the answers back in the set's order: tile i is answer i / shards of share i % shards
    let mut browser = Vec::with_capacity(jobs.len());
    for i in 0..jobs.len() {
        let line = shares[i % shards]
            .get(i / shards)
            .ok_or_else(|| format!("parity: the browser gave no answer for {}", jobs[i]))?;
        browser.push(line.clone());
    }
    let browser = browser.join("\n").into_bytes();
    let native = native?;
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

/// `cargo xtask parity --too-big`: a 2x2 cliff needs more than the 4 GB a wasm32 module can address, and must say so:
/// the run fails, and its answer names the stage and the module's memory, not a memory fault. (About 100 s, so it
/// is not in the parity set.)  Then the C++ runtime's `operator new` is asked for more than the module can give and must
/// say the same, not trap without a word (`operator-new.cpp`).
fn run_too_big(root: &Path) -> Result<(), String> {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), Into::into);
    let bundle = target.join("parity-bundle");
    crate::wasm::run_wasm(root, &[bundle.display().to_string()])?;
    let out = Command::new("node")
        .current_dir(root)
        .arg("xtask/parity.mjs")
        .arg(&bundle)
        .arg("lib:tiles/cliffs/2x2-cliff-corner.dtile@db")
        .output()
        .map_err(|e| format!("node: {e}"))?;
    let said = String::from_utf8_lossy(&out.stdout);
    if !out.status.success()
        && said.contains("out of memory in the union stage")
        && said.contains("a browser tab can give it")
    {
        println!("too-big: the 2x2 cliff stopped with its stage and memory:\n{said}");
        return run_operator_new_fails(root, &bundle);
    }
    Err(format!(
        "parity --too-big: a 2x2 cliff should stop in the browser naming the stage and the memory; it gave status {}, stdout {said}, stderr {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    ))
}

/// The tile the service's Node test puts in the worker's service.
const SERVICE_TILE: &str = "lib:tiles/ground/1x1-grass.dtile";

/// `cargo xtask parity --service`: the editor's service as a web worker will run it, under Node.  The bundle's
/// `Service` is given one library tile and called by verb: `check`, `save`, then `build` (the HTTP verb: the record comes on the event stream).  The program builds the same
/// tile (`dharmatiles build`), and the two STLs' triangles must have the same SHA-256.
fn run_service(root: &Path) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), Into::into);
    let bundle = target.join("parity-bundle");
    crate::wasm::run_wasm(root, &[bundle.display().to_string()])?;
    let job = format!("{SERVICE_TILE}@db");
    // the files the service made are kept here, to compare with the program's
    let dump = target.join("service-dump");
    match std::fs::remove_dir_all(&dump) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot clear {}: {e}", dump.display())),
    }
    std::fs::create_dir_all(&dump).map_err(|e| format!("{}: {e}", dump.display()))?;
    let line = String::from_utf8(capture(
        Command::new("node")
            .current_dir(root)
            .env("SERVICE_DUMP", &dump)
            .arg("xtask/service.mjs")
            .arg(&bundle)
            .arg(&job),
    )?)
    .map_err(|e| format!("not UTF-8: {e}"))?;
    let browser: Value =
        serde_json::from_str(line.trim()).map_err(|e| format!("service: {e}: {line}"))?;
    // the program's own build of the same tile, in a scratch folder of its own (never the user's out/ or cache)
    let scratch = target.join("service-scratch");
    match std::fs::remove_dir_all(&scratch) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot clear {}: {e}", scratch.display())),
    }
    let status = Command::new("cargo")
        .current_dir(root)
        .args(["build", "-q", "--release", "-p", "dharmatiles"])
        .status()
        .map_err(|e| format!("cargo build: {e}"))?;
    if !status.success() {
        return Err(format!("cargo build -p dharmatiles failed: {status}"));
    }
    let out = scratch.join("out");
    capture(
        Command::new(target.join("release/dharmatiles"))
            .current_dir(root)
            .env("DT_HOME", scratch.join("home"))
            .env("DT_CACHE", scratch.join("cache"))
            .args(["build", SERVICE_TILE, "--no-cache", "--out"])
            .arg(&out),
    )?;
    let stl_path = out.join("db/ground/1x1-grass-db.stl");
    let stl =
        std::fs::read(&stl_path).map_err(|e| format!("cannot read {}: {e}", stl_path.display()))?;
    let native: String = Sha256::digest(&stl[80..])
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    // the same tile exported as a download is the same STL
    if browser["exportedStlSha256"].as_str() != Some(native.as_str()) {
        return Err(format!(
            "service: exportTile's STL is not `dharmatiles build`'s for {SERVICE_TILE}\n  service: {}\n  program: {native}",
            browser["exportedStlSha256"]
        ));
    }
    service_plates(root, &target, &scratch, &dump, &browser)?;
    let said = browser["stlSha256"]
        .as_str()
        .ok_or_else(|| format!("service: the worker's answer has no stlSha256: {browser}"))?;
    if said == native && browser["passed"] == Value::Bool(true) {
        println!(
            "service: check, save and build by verb under Node gave the program's STL for {SERVICE_TILE} ({native})"
        );
        Ok(())
    } else {
        Err(format!(
            "service: the worker's service and `dharmatiles build` differ on {SERVICE_TILE}\n  service: {browser}\n  program: {native}"
        ))
    }
}

/// The plates the service made (one STL and one 3MF per plate, for the print list it holds) are the program's, byte for byte:
/// the list's text is put in a scratch home with the tile it names, and `dharmatiles plates` lays it out in both formats.
fn service_plates(
    root: &Path,
    target: &Path,
    scratch: &Path,
    dump: &Path,
    browser: &Value,
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let home = scratch.join("home");
    std::fs::create_dir_all(&home).map_err(|e| format!("{}: {e}", home.display()))?;
    let tile_file = browser["tileSource"]
        .as_str()
        .ok_or("service: no tileSource in the worker's answer")?
        .trim_start_matches("lib:");
    // the list names the worker's copy of the tile (`home:1x1-grass 2`): the program's home holds it by the same name
    let name = browser["tileFile"]
        .as_str()
        .ok_or("service: no tileFile in the worker's answer")?;
    let write = |p: PathBuf, text: &[u8]| {
        std::fs::write(&p, text).map_err(|e| format!("cannot write {}: {e}", p.display()))
    };
    write(
        home.join(name),
        &std::fs::read(root.join("library").join(tile_file))
            .map_err(|e| format!("cannot read the library's {tile_file}: {e}"))?,
    )?;
    let list_name = browser["listFile"]
        .as_str()
        .ok_or("service: no listFile in the worker's answer")?;
    let list_text = browser["listText"]
        .as_str()
        .ok_or("service: no listText in the worker's answer")?;
    let list = home.join(list_name);
    write(list.clone(), list_text.as_bytes())?;
    let title = Path::new(list_name)
        .file_stem()
        .ok_or("the list has no name")?
        .to_string_lossy()
        .into_owned();
    for (format, key) in [("stl", "platesStl"), ("3mf", "plates3mf")] {
        let out = scratch.join(format!("plates-{format}"));
        capture(
            Command::new(target.join("release/dharmatiles"))
                .current_dir(root)
                .env("DT_HOME", &home)
                .env("DT_CACHE", scratch.join("cache"))
                .args(["plates"])
                .arg(&list)
                .args(["--no-cache", "--all", "--format", format, "--out"])
                .arg(&out),
        )?;
        let said = browser[key]
            .as_object()
            .ok_or_else(|| format!("service: no {key} in the worker's answer"))?;
        for (file, hash) in said {
            let path = out.join("plates").join(&title).join(file);
            let bytes = std::fs::read(&path)
                .map_err(|e| format!("cannot read the program's {}: {e}", path.display()))?;
            let same = if file.ends_with(".stl") {
                // an STL's 80-byte header names the document it was made from: the list's path, which is not the worker's
                let native: String = Sha256::digest(&bytes[80..])
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                hash.as_str() == Some(native.as_str())
            } else {
                // a 3MF is a zip of its model and two small files; the model names the engine that made it (the program's
                // is named by its git version, the worker's by the engine crate's), so that line is the one thing left out
                model_of(&path)? == model_of(&dump.join(file))?
            };
            if !same {
                return Err(format!(
                    "service: the plate {file} ({format}) is not the program's (the worker's is kept in {})",
                    dump.display()
                ));
            }
        }
        println!(
            "service: {} {format} plate file(s) are the program's{}",
            said.len(),
            if format == "3mf" {
                ": the same model in every byte but the name of the engine that made it"
            } else {
                ", byte for byte after the header that names the list's path"
            }
        );
    }
    Ok(())
}

/// The model of a 3MF, without the line that names the engine.
fn model_of(file: &Path) -> Result<String, String> {
    let text = String::from_utf8(capture(
        Command::new("unzip")
            .args(["-p"])
            .arg(file)
            .arg("3D/3dmodel.model"),
    )?)
    .map_err(|e| format!("{}: not UTF-8: {e}", file.display()))?;
    Ok(text
        .lines()
        .filter(|l| !l.contains(r#"<metadata name="Application">"#))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// `operator new` asked for all but a few hundred bytes of the 4 GB a wasm32 module can address says what it asked for
/// and the module's memory, in the words a failing Manifold allocation uses.
fn run_operator_new_fails(root: &Path, bundle: &Path) -> Result<(), String> {
    let out = Command::new("node")
        .current_dir(root)
        .arg("xtask/parity.mjs")
        .arg(bundle)
        .arg("probe-new:4294967000")
        .output()
        .map_err(|e| format!("node: {e}"))?;
    let said = String::from_utf8_lossy(&out.stdout);
    if !out.status.success()
        && said.contains("out of memory")
        && said.contains("the geometry kernel asked for 4096 MB")
        && said.contains("a browser tab can give it")
    {
        println!("too-big: operator new said its size and the memory:\n{said}");
        return Ok(());
    }
    Err(format!(
        "parity --too-big: a failing operator new should say its size and the memory; it gave status {}, stdout {said}, stderr {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    ))
}
