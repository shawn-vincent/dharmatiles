//! The check's tests, run in parallel: `cargo test` runs its test binaries one after another and holds the build
//! lock until the last ends, so every small binary waited behind dt-eval's tile tests while the machine sat mostly
//! idle. Here the build is `cargo test --no-run` (the same command, so the same features), the binaries it made are
//! read from its messages, and a few workers take them from one queue, the slowest first. Each binary is run as
//! cargo runs it: from its package's folder, every output kept and shown when it fails, every failure listed.
//!
//! The queue is ordered by each binary's time in its last run (`dt-test-times.tsv` in the git common folder, beside
//! `dt-times.tsv`; a binary never timed goes first). The lanes were once picked by hand (dt-eval's tile tests,
//! dharmatiles' own, and every other binary in one lane), and that last lane summed to about 80 s beside a 55 s tile
//! test: a binary added there lengthened the check, and the cores sat idle once the shorter lanes were done.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;

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

/// How many binaries run at once: what the three hand-picked lanes ran, so the load on the machine is the same.
const WORKERS: usize = 3;

/// The binaries' times in the last run: `package/target`, a tab, seconds.
fn times_file(root: &Path) -> Result<PathBuf, String> {
    let common = crate::work::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("dt-test-times.tsv"))
}

/// What each binary took last time; nothing before the first run (a missing file is the meaning: nothing is known).
///
/// # Errors
/// A file that exists and cannot be read or does not read.
fn last_times(file: &Path) -> Result<BTreeMap<String, f64>, String> {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(format!("{}: {e}", file.display())),
    };
    text.lines()
        .map(|l| {
            let (name, secs) = l
                .split_once('\t')
                .ok_or_else(|| format!("{}: a line with no tab: {l}", file.display()))?;
            let secs = secs
                .parse()
                .map_err(|e| format!("{}: {secs}: {e}", file.display()))?;
            Ok((name.to_owned(), secs))
        })
        .collect()
}

/// One binary run: its name, what it took and, when it failed, its output.
type Ran = (String, f64, Option<String>);

/// Run one binary.
fn run_one(b: &Binary) -> Ran {
    let t = Instant::now();
    let out = Command::new(&b.exe)
        .arg("-q")
        .current_dir(&b.dir)
        .env("CARGO_MANIFEST_DIR", &b.dir)
        .env("CARGO_PKG_NAME", &b.package)
        .output();
    let secs = t.elapsed().as_secs_f64();
    let name = format!("{}/{}", b.package, b.target);
    let failed = match out {
        Ok(o) if o.status.success() => {
            println!("  ok   {secs:>5.1} s  {name}");
            None
        }
        Ok(o) => {
            println!("  FAIL {secs:>5.1} s  {name}");
            Some(format!(
                "{name} ({}):\n{}{}",
                o.status,
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ))
        }
        Err(e) => Some(format!("{name}: cannot run {}: {e}", b.exe.display())),
    };
    (name, secs, failed)
}

/// Build what `cargo` names (with `build`), or run its tests: [`WORKERS`] workers take the binaries from one queue,
/// the slowest of the last run first.
///
/// # Errors
/// The build's, or every failing binary, with its output.
pub fn tests(root: &Path, cargo: Command, build: bool) -> Result<(), String> {
    let bins = built(cargo)?;
    if build {
        return Ok(());
    }
    let file = times_file(root)?;
    let known = last_times(&file)?;
    let mut order: Vec<&Binary> = bins.iter().collect();
    // never timed: first (it may be the slowest); then the slowest of the last run
    let weight = |b: &Binary| {
        known
            .get(&format!("{}/{}", b.package, b.target))
            .copied()
            .unwrap_or(f64::INFINITY)
    };
    order.sort_by(|a, b| weight(b).total_cmp(&weight(a)));
    let queue = Mutex::new(order.into_iter());
    let results: Vec<Ran> = std::thread::scope(|s| {
        let running: Vec<_> = (0..WORKERS)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        // the guard is dropped at the end of this statement, before the binary runs
                        let next = match queue.lock() {
                            Ok(mut q) => q.next(),
                            // another worker panicked: the queue is still whole, and that panic is reported below
                            Err(poisoned) => poisoned.into_inner().next(),
                        };
                        match next {
                            Some(b) => mine.push(run_one(b)),
                            None => return mine,
                        }
                    }
                })
            })
            .collect();
        running
            .into_iter()
            .flat_map(|r| match r.join() {
                Ok(ran) => ran,
                Err(_) => vec![(
                    "a test worker".to_owned(),
                    0.0,
                    Some("a test worker panicked".to_owned()),
                )],
            })
            .collect()
    });
    // kept for the next run's order: a binary that ran keeps its new time, the others keep theirs
    let mut times = known;
    for (name, secs, _) in &results {
        times.insert(name.clone(), *secs);
    }
    let text: String = times
        .iter()
        .map(|(n, s)| format!("{n}\t{s:.1}\n"))
        .collect();
    std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    let failed: Vec<String> = results.into_iter().filter_map(|r| r.2).collect();
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
