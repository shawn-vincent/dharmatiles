//! Repo chores, run as `cargo xtask <command>`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage: cargo xtask <command>

commands:
  parity <look> [--seeds 1 2 3 4] [--no-python] [--dev]
           build the look's Rust tile at each seed and write the contact
           sheet to out/parity/<look>/: Python beside Rust where the look
           has a Rust tile, Python only where it does not yet
           (looks: flat, dirt)
  help     show this message";

/// Each look the harness knows: its Rust tile, if it has been ported.
const LOOKS: &[(&str, Option<&str>)] = &[
    ("flat", Some("lib:tiles/test/flat-1x1.kdl")),
    ("dirt", None),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("parity") => match parity(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("parity: {e}");
                ExitCode::FAILURE
            }
        },
        Some("help" | "-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command: {other}\n\n{USAGE}");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let status = cmd
        .status()
        .map_err(|e| format!("cannot run {cmd:?}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{cmd:?} failed ({status})"))
    }
}

/// The parity harness, Rust half (plan.html, "The parity harness"): build
/// seeds 1–4 with `dharmatiles build --png`, then hand them to the Python
/// half (`tests/parity/run_lab.py --beside`), which builds the lab at the
/// same seeds, computes the same metrics from both STLs and lays out the
/// sheet: four views per seed (painted, top, low, 20 mm close-up), Python
/// beside Rust, overhangs red.
fn parity(args: &[String]) -> Result<(), String> {
    let look = args.first().ok_or(USAGE)?.as_str();
    let tile = LOOKS
        .iter()
        .find(|(l, _)| *l == look)
        .ok_or_else(|| format!("unknown look `{look}` (looks: flat, dirt)"))?
        .1;
    let flag = |f: &str| args.iter().any(|a| a == f);
    let seeds: Vec<String> = match args.iter().position(|a| a == "--seeds") {
        Some(i) => args[i + 1..]
            .iter()
            .take_while(|a| !a.starts_with("--"))
            .cloned()
            .collect(),
        None => ["1", "2", "3", "4"].map(String::from).to_vec(),
    };
    let root = root();
    let out = root.join("out").join("parity").join(look);
    let rust = out.join("rust");
    if let Some(tile) = tile {
        let release = !flag("--dev");
        let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cargo
            .current_dir(&root)
            .args(["build", "-p", "dharmatiles"]);
        if release {
            cargo.arg("--release");
        }
        run(&mut cargo)?;
        let target =
            std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
        let exe = target
            .join(if release { "release" } else { "debug" })
            .join(format!("dharmatiles{}", std::env::consts::EXE_SUFFIX));
        for s in &seeds {
            let dir = rust.join(format!("s{s}"));
            let _ = std::fs::remove_dir_all(&dir);
            // exit 3 (audit failed) still writes the STL and the views
            let status = Command::new(&exe)
                .args(["build", tile, "--png", "--seed", s, "--out"])
                .arg(&dir)
                .status()
                .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
            if !matches!(status.code(), Some(0 | 3)) {
                return Err(format!(
                    "dharmatiles build {tile} --seed {s} failed ({status})"
                ));
            }
        }
        println!("parity: Rust builds in {}", rust.display());
    }
    if flag("--no-python") {
        return Ok(());
    }
    let python = std::env::var("PYTHON").unwrap_or_else(|_| "python3".into());
    let src = root.join("src");
    let path = std::env::var_os("PYTHONPATH").map_or_else(
        || src.clone().into_os_string(),
        |p| {
            let mut all = vec![src.clone()];
            all.extend(std::env::split_paths(&p));
            std::env::join_paths(all).unwrap_or_default()
        },
    );
    let mut py = Command::new(python);
    py.current_dir(&root)
        .env("PYTHONPATH", path)
        .arg(root.join("tests/parity/run_lab.py"))
        .args(["--look", look, "--out"])
        .arg(&out)
        .arg("--seeds")
        .args(&seeds);
    if tile.is_some() {
        py.arg("--beside").arg(&rust);
    }
    run(&mut py)?;
    println!(
        "parity: contact sheet {}",
        out.join("contact-sheet.png").display()
    );
    Ok(())
}
