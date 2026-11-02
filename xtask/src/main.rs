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
    ("dirt", Some("lib:tiles/test/dirt-2x2.kdl")),
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
        if tile.is_some() {
            let sheet = out.join("contact-sheet.png");
            rust_sheet(&rust, &seeds, &sheet)?;
            println!("parity: contact sheet (Rust only) {}", sheet.display());
        }
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

/// The four views each build writes, in contact-sheet column order.
const VIEWS: [&str; 4] = ["painted", "top", "low", "close"];

/// A Rust-only contact sheet for runs without Python (CI): one row per
/// seed, one column per view, on a white ground with a 12 px gutter.
fn rust_sheet(rust: &Path, seeds: &[String], sheet: &Path) -> Result<(), String> {
    const GAP: usize = 12;
    let mut rows = Vec::new();
    for s in seeds {
        let pngs = find_pngs(&rust.join(format!("s{s}")));
        let row = VIEWS
            .iter()
            .map(|v| {
                let p = pngs
                    .iter()
                    .find(|p| p.to_string_lossy().ends_with(&format!("-{v}.png")))
                    .ok_or_else(|| format!("seed {s}: no `{v}` view under {}", rust.display()))?;
                read_rgb(p)
            })
            .collect::<Result<Vec<_>, String>>()?;
        rows.push(row);
    }
    // Layout in usize; read_rgb returns usize sizes, so only the header converts.
    let cw = rows.iter().flatten().map(|i| i.0).max().unwrap_or(0);
    let ch = rows.iter().flatten().map(|i| i.1).max().unwrap_or(0);
    let (w, h) = (
        GAP + VIEWS.len() * (cw + GAP),
        GAP + rows.len() * (ch + GAP),
    );
    let mut buf = vec![255u8; w * h * 3];
    for (r, row) in rows.iter().enumerate() {
        for (c, (iw, ih, px)) in row.iter().enumerate() {
            let (x0, y0) = (GAP + c * (cw + GAP), GAP + r * (ch + GAP));
            for y in 0..*ih {
                let dst = ((y0 + y) * w + x0) * 3;
                buf[dst..dst + iw * 3].copy_from_slice(&px[y * iw * 3..(y + 1) * iw * 3]);
            }
        }
    }
    let too_big = |_| format!("{}: contact sheet too large", sheet.display());
    let (w, h) = (
        u32::try_from(w).map_err(too_big)?,
        u32::try_from(h).map_err(too_big)?,
    );
    if let Some(dir) = sheet.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let file = std::fs::File::create(sheet).map_err(|e| format!("{}: {e}", sheet.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(&buf))
        .map_err(|e| format!("{}: {e}", sheet.display()))
}

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

/// Decode one of the renderer's 8-bit RGB PNGs.
fn read_rgb(path: &Path) -> Result<(usize, usize, Vec<u8>), String> {
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(|e| err(&e))?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .map_err(|e| err(&e))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| err(&"image too large"))?;
    let mut px = vec![0; size];
    let info = reader.next_frame(&mut px).map_err(|e| err(&e))?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(err(&"not an 8-bit PNG"));
    }
    if info.color_type != png::ColorType::Rgb {
        return Err(err(&"not an RGB PNG (dharmatiles renders are RGB)"));
    }
    px.truncate(info.buffer_size());
    let dim = |v: u32| usize::try_from(v).map_err(|e| err(&e));
    Ok((dim(info.width)?, dim(info.height)?, px))
}
