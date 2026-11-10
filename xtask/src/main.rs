//! Repo chores, run as `cargo xtask <command>`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod docs;
mod image;
mod pictures;
mod release;

const USAGE: &str = r#"usage: cargo xtask <command>

commands:
  sheet <tile> [--seeds 1 2 3 4] [--system db|ol] [--dev]
           build a tile at each seed and lay its four views (painted, top,
           low, 20 mm close-up) out as one contact sheet, a row per seed:
           out/sheet/<tile>[-<system>]/contact-sheet.png. <tile> is a path, a lib:
           path, or a short name for a test tile (flat, dirt,
           rocks, water, grass, tree, wall-cut-stone, wall-brick,
           wall-regular-brick, wall-fieldstone, floor-stone,
           floor-fieldstone, flagstones-heaved, planks, cliff,
           cliff-<form>)
  pictures [group …]
           rebuild the docs' element pictures from docs/elements/demo/
           into docs/elements/img/ (groups: line, region, use, define,
           stack, walls, floors, meadow; all when none is named)
  docs [--check]
           fill each docs/elements/ page's generated reference block from
           the element schema; --check changes nothing and fails if stale
  release <version> "<summary>" [--no-tests] [--no-push]
           number a release: the changelog's Unreleased entry becomes
           <version>, dated today and led by <summary>; Cargo.toml's
           version is bumped; fmt, clippy, docs --check and the tests run
           (--no-tests skips the tests); then one commit, the tag
           v<version>, and a push of both (--no-push keeps them local)
  help     show this message"#;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("sheet") => sheet(&args[1..]),
        Some("pictures") => pictures::run(&root(), &args[1..]),
        Some("docs") => docs::run(&root(), args.iter().any(|a| a == "--check")),
        Some("release") => release::run_release(&root(), &args[1..]),
        Some("help" | "-h" | "--help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some(other) => Err(format!("unknown command: {other}\n\n{USAGE}")),
        None => Err(USAGE.to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
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

/// Build the `dharmatiles` binary and return its path.
fn program(root: &Path, release: bool) -> Result<PathBuf, String> {
    let mut cargo = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cargo.current_dir(root).args(["build", "-p", "dharmatiles"]);
    if release {
        cargo.arg("--release");
    }
    run(&mut cargo)?;
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    Ok(target
        .join(if release { "release" } else { "debug" })
        .join(format!("dharmatiles{}", std::env::consts::EXE_SUFFIX)))
}

/// `dharmatiles build <tile> --png [--seed s] --out <dir>`. Exit 3 (the
/// audit failed) still writes the STL and the views, so it counts.
fn build_views(
    exe: &Path,
    tile: &str,
    seed: Option<&str>,
    system: Option<&str>,
    dir: &Path,
) -> Result<(), String> {
    // a stale folder would be mistaken for this build's pictures: clear it,
    // or stop (a folder that is not there is already clear)
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot clear {}: {e}", dir.display())),
    }
    let mut cmd = Command::new(exe);
    cmd.args(["build", tile, "--png"]);
    if let Some(s) = seed {
        cmd.args(["--seed", s]);
    }
    if let Some(sys) = system {
        cmd.args(["--system", sys]);
    }
    let status = cmd
        .arg("--out")
        .arg(dir)
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    if matches!(status.code(), Some(0 | 3)) {
        Ok(())
    } else {
        Err(format!("dharmatiles build {tile} failed ({status})"))
    }
}

/// The test tile behind a short name (`flat`, `rocks`, `wall-rim`: a test
/// tile's file name without its size, or with it), or the argument itself.
fn tile_for(name: &str) -> String {
    let tests = root().join("library/tests");
    let Ok(groups) = std::fs::read_dir(&tests) else {
        return name.to_owned();
    };
    let mut found: Vec<String> = Vec::new();
    for g in groups.flatten() {
        let Ok(files) = std::fs::read_dir(g.path()) else {
            continue;
        };
        for f in files.flatten() {
            let file = f.file_name().to_string_lossy().into_owned();
            let Some(stem) = file.strip_suffix(".dtile") else {
                continue;
            };
            let bare = stem.split_once('-').map_or(stem, |(_, r)| r);
            if stem == name || bare == name {
                found.push(format!(
                    "lib:tests/{}/{file}",
                    g.file_name().to_string_lossy()
                ));
            }
        }
    }
    found.sort();
    found.into_iter().next().unwrap_or_else(|| name.to_owned())
}

/// The four views each build writes, in contact-sheet column order.
const VIEWS: [&str; 4] = ["painted", "top", "low", "close"];

/// `cargo xtask sheet`: one row per seed, one column per view, on the
/// plate with a 12 px gutter.
fn sheet(args: &[String]) -> Result<(), String> {
    let name = args.first().ok_or(USAGE)?;
    let tile = tile_for(name);
    let seeds: Vec<String> = match args.iter().position(|a| a == "--seeds") {
        Some(i) => args[i + 1..]
            .iter()
            .take_while(|a| !a.starts_with("--"))
            .cloned()
            .collect(),
        None => ["1", "2", "3", "4"].map(String::from).to_vec(),
    };
    let system = args
        .iter()
        .position(|a| a == "--system")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str);
    let root = root();
    let exe = program(&root, !args.iter().any(|a| a == "--dev"))?;
    let mut stem = Path::new(&tile)
        .file_stem()
        .map_or_else(|| name.clone(), |s| s.to_string_lossy().into_owned());
    if let Some(sys) = system {
        stem = format!("{stem}-{sys}");
    }
    let out = root.join("out").join("sheet").join(&stem);
    let mut rows = Vec::new();
    for s in &seeds {
        let dir = out.join(format!("s{s}"));
        build_views(&exe, &tile, Some(s), system, &dir)?;
        let pngs = find_pngs(&dir);
        let row = VIEWS
            .iter()
            .map(|v| {
                let p = pngs
                    .iter()
                    .find(|p| p.to_string_lossy().ends_with(&format!("-{v}.png")))
                    .ok_or_else(|| format!("seed {s}: no `{v}` view under {}", dir.display()))?;
                image::Rgb::read_png(p)
            })
            .collect::<Result<Vec<_>, String>>()?;
        rows.push(row);
    }
    let sheet = out.join("contact-sheet.png");
    image::grid(&rows, 12, image::PLATE).write_png(&sheet)?;
    println!("sheet: {}", sheet.display());
    Ok(())
}

/// Every PNG under `dir`, sorted.
fn find_pngs(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            found.extend(find_pngs(&p));
        } else if p.extension().is_some_and(|x| x == "png") {
            found.push(p);
        }
    }
    found.sort();
    found
}
