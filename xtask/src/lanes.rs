//! The check's tests, run in lanes: `cargo test` runs its test binaries one after another and holds the build
//! lock until the last ends, so dharmatiles' and every small binary waited behind dt-eval's tile tests (38 s) while
//! the machine sat mostly idle. Here the build is `cargo test --no-run` (the same command, so the same features),
//! the binaries it made are read from its messages, and the tile tests run in a lane of their own beside every other
//! binary. Each binary is run as cargo runs it: from its package's folder, every output kept and shown when it
//! fails, every failure listed.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

/// The binaries big enough for a lane of their own, by package and test target: dt-eval's tile tests, and dharmatiles'
/// own (13 s alone), which otherwise ran last in the other lane, after the tile tests had finished and left the cores
/// idle.  Every other binary shares one more lane.
const OWN_LANE: &[(&str, &str)] = &[("dt-eval", "tiles"), ("dharmatiles", "dharmatiles")];

/// One of cargo's build messages (`--message-format=json`): the artifacts, of which a test binary has an
/// executable; every other kind of message is passed over.
#[derive(serde::Deserialize)]
#[serde(tag = "reason")]
enum Message {
    #[serde(rename = "compiler-artifact")]
    Artifact(Artifact),
    #[serde(other)]
    Other,
}

#[derive(serde::Deserialize)]
struct Artifact {
    manifest_path: PathBuf,
    target: Target,
    profile: Profile,
    executable: Option<PathBuf>,
}

#[derive(serde::Deserialize)]
struct Target {
    name: String,
}

#[derive(serde::Deserialize)]
struct Profile {
    test: bool,
}

/// A test binary: its package's folder and name, its target and its file.
struct Binary {
    dir: PathBuf,
    package: String,
    target: String,
    exe: PathBuf,
}

/// Build the test binaries `cargo` (`cargo test …` without `--no-run`) names, and return them.
///
/// # Errors
/// A build that fails, or messages that do not read.
fn built(mut cargo: Command) -> Result<Vec<Binary>, String> {
    let out = cargo
        .args(["--no-run", "--message-format=json-render-diagnostics"])
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("cannot run {cargo:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{cargo:?} failed ({})", out.status));
    }
    let text = String::from_utf8(out.stdout).map_err(|e| e.to_string())?;
    let mut bins = Vec::new();
    for line in text.lines().filter(|l| l.starts_with('{')) {
        let m: Message =
            serde_json::from_str(line).map_err(|e| format!("cargo's message: {e}: {line}"))?;
        if let Message::Artifact(a) = m
            && a.profile.test
            && let Some(exe) = a.executable
        {
            let dir = a
                .manifest_path
                .parent()
                .ok_or("a manifest with no folder")?
                .to_path_buf();
            let package = dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            bins.push(Binary {
                dir,
                package,
                target: a.target.name,
                exe,
            });
        }
    }
    Ok(bins)
}

/// Run each binary in turn; the failures, each with its output.
fn lane(bins: &[&Binary]) -> Vec<String> {
    let mut failed = Vec::new();
    for b in bins {
        let t = Instant::now();
        let out = Command::new(&b.exe)
            .arg("-q")
            .current_dir(&b.dir)
            .env("CARGO_MANIFEST_DIR", &b.dir)
            .env("CARGO_PKG_NAME", &b.package)
            .output();
        let secs = t.elapsed().as_secs_f64();
        let name = format!("{}/{}", b.package, b.target);
        match out {
            Ok(o) if o.status.success() => println!("  ok   {secs:>5.1} s  {name}"),
            Ok(o) => {
                println!("  FAIL {secs:>5.1} s  {name}");
                failed.push(format!(
                    "{name} ({}):\n{}{}",
                    o.status,
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                ));
            }
            Err(e) => failed.push(format!("{name}: cannot run {}: {e}", b.exe.display())),
        }
    }
    failed
}

/// Build what `cargo` names (with `build`), or run its tests in two lanes.
///
/// # Errors
/// The build's, or every failing binary, with its output.
pub fn tests(cargo: Command, build: bool) -> Result<(), String> {
    let bins = built(cargo)?;
    if build {
        return Ok(());
    }
    let lane_of = |b: &Binary| {
        OWN_LANE
            .iter()
            .position(|(p, t)| b.package == *p && b.target == *t)
            .unwrap_or(OWN_LANE.len())
    };
    let mut lanes: Vec<Vec<&Binary>> = vec![Vec::new(); OWN_LANE.len() + 1];
    for b in &bins {
        lanes[lane_of(b)].push(b);
    }
    let failed: Vec<String> = std::thread::scope(|s| {
        let running: Vec<_> = lanes.iter().map(|l| s.spawn(|| lane(l))).collect();
        running
            .into_iter()
            .flat_map(|r| match r.join() {
                Ok(failed) => failed,
                Err(_) => vec!["a test lane panicked".to_owned()],
            })
            .collect()
    });
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} of {} test binaries failed:\n\n{}",
            failed.len(),
            bins.len(),
            failed.join("\n\n")
        ))
    }
}
