//! The workshop: how a change is checked, and how heavy jobs share the machine.
//!
//! - `cargo xtask check [--full] [--plan]`: the checks a change can fail, chosen from
//!   what it changes against main (`--full`: all of them, the full test suite
//!   included, which is what runs before a push; `--plan`: print them only).
//! - `cargo xtask sweep [element|lib:tile …]`: the library baseline, for the
//!   tiles that use those elements (all of them when none is named).
//! - `cargo xtask heavy <command …>`: run a command that loads the whole
//!   machine only when no other heavy job runs, in any worktree.
//! - `cargo xtask regen`: write every generated file again (the page contract,
//!   the docs), the way to settle a rebase that stopped on one.
//!
//! Heavy jobs (`sweep`, `sheet`, `pictures`, `wasm`, `heavy`) take one lock,
//! a file in the repository's common git folder, so every worktree shares it:
//! above the core count nothing finishes sooner, and two sweeps at once each
//! take twice as long.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Instant;

use serde_json::Value;

use crate::run;

/// The heavy-job lock, held until this process ends; taken once (a release
/// that runs the sweep and the pictures holds it throughout).
static HELD: OnceLock<File> = OnceLock::new();

/// Wait for the heavy-job lock and hold it until this process ends.
pub fn heavy_lock(root: &Path) -> Result<(), String> {
    if HELD.get().is_some() {
        return Ok(());
    }
    let common = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let path = PathBuf::from(common).join("dt-heavy.lock");
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            eprintln!(
                "waiting for the heavy job running elsewhere to finish ({})",
                path.display()
            );
            file.lock()
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Err(std::fs::TryLockError::Error(e)) => return Err(format!("{}: {e}", path.display())),
    }
    // a second set is impossible: the first `get` above returned None on this thread, and xtask is one thread here
    HELD.set(file)
        .map_err(|_| "the heavy-job lock was taken twice".to_owned())
}

/// `cargo xtask heavy <command …>`.
pub fn heavy(root: &Path, args: &[String]) -> Result<(), String> {
    let (cmd, rest) = args
        .split_first()
        .ok_or("usage: cargo xtask heavy <command …>")?;
    heavy_lock(root)?;
    run(Command::new(cmd).args(rest))
}

/// `cargo xtask sweep [element|lib:tile …]`.
pub fn sweep(root: &Path, only: &[String]) -> Result<(), String> {
    heavy_lock(root)?;
    let mut c = cargo();
    c.current_dir(root).args([
        "test",
        "--release",
        "-p",
        "dt-eval",
        "--test",
        "baseline",
        "--",
        "--ignored",
    ]);
    if !only.is_empty() {
        c.env("DT_BASELINE_ONLY", only.join(","));
    }
    run(&mut c)
}

/// `cargo xtask regen`: every generated file, written again.
pub fn regen(root: &Path) -> Result<(), String> {
    run(cargo().current_dir(root).env("DT_CONTRACT", "write").args([
        "test",
        "--release",
        "-p",
        "dharmatiles",
        "--bins",
        "the_page_contract_is_current",
    ]))?;
    crate::docs::run(root, false)?;
    let changed = git(root, &["status", "--short"])?;
    println!(
        "{}",
        if changed.is_empty() {
            "nothing changed".to_owned()
        } else {
            changed
        }
    );
    Ok(())
}

fn cargo() -> Command {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Every file this branch changes against main: committed, staged, in the
/// working tree, or new.
fn changed(root: &Path) -> Result<Vec<String>, String> {
    let base = git(root, &["merge-base", "HEAD", "main"])?;
    let mut all = BTreeSet::new();
    for args in [
        vec!["diff", "--name-only", base.as_str()],
        vec!["ls-files", "--others", "--exclude-standard"],
    ] {
        all.extend(git(root, &args)?.lines().map(str::to_owned));
    }
    Ok(all.into_iter().filter(|l| !l.is_empty()).collect())
}

/// The workspace's packages: each one's folder (relative to the root) and
/// the workspace packages it depends on, dev-dependencies included.
fn packages(root: &Path) -> Result<BTreeMap<String, (String, Vec<String>)>, String> {
    let out = cargo()
        .current_dir(root)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let meta: Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    let root_s = root.to_string_lossy().into_owned();
    let mut map = BTreeMap::new();
    for p in meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?
    {
        let name = p["name"]
            .as_str()
            .ok_or("cargo metadata: a package has no name")?;
        let manifest = p["manifest_path"]
            .as_str()
            .ok_or("cargo metadata: a package has no manifest")?;
        let dir = Path::new(manifest)
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = dir
            .strip_prefix(&root_s)
            .unwrap_or(&dir)
            .trim_start_matches('/')
            .to_owned();
        let deps = p["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d.get("path").is_some())
            .filter_map(|d| d["name"].as_str().map(str::to_owned))
            .collect();
        map.insert(name.to_owned(), (dir, deps));
    }
    Ok(map)
}

/// The elements whose code is in a `dt-elements/src` file or folder: a
/// change there sweeps the tiles that use them. A file not named here (the
/// shared helpers) sweeps the whole library.
const ELEMENT_FILES: &[(&str, &[&str])] = &[
    ("cliff", &["cliff"]),
    ("dirt.rs", &["dirt"]),
    ("floor.rs", &["floor"]),
    ("flowers.rs", &["flower"]),
    ("grass", &["grass"]),
    ("reeds.rs", &["reeds"]),
    ("rock.rs", &["rock", "stone", "boulder", "pebble"]),
    ("tree", &["tree"]),
    ("wall", &["wall"]),
    ("water.rs", &["water"]),
];

/// What a change runs.
#[derive(Debug, Default, PartialEq, Eq)]
struct Plan {
    /// `cargo fmt --check` and clippy.
    rust: bool,
    /// The packages whose tests run: the changed ones and every workspace
    /// package that depends on them.
    tests: BTreeSet<String>,
    /// `npm test` in web/preview.
    page: bool,
    /// `cargo xtask docs --check`.
    docs: bool,
    /// The sweep: `None` none, `Some(empty)` the whole library, else the
    /// elements and tiles named.
    sweep: Option<BTreeSet<String>>,
}

impl Plan {
    fn sweep_all(&mut self) {
        self.sweep = Some(BTreeSet::new());
    }
    fn sweep_some(&mut self, what: impl IntoIterator<Item = String>) {
        match &mut self.sweep {
            Some(s) if s.is_empty() => {} // already the whole library
            Some(s) => s.extend(what),
            None => self.sweep = Some(what.into_iter().collect()),
        }
    }
}

/// Folders whose change can move any tile's geometry: the whole library is swept.
const GEOMETRY: &[&str] = &[
    "crates/dt-geom/",
    "crates/dt-kit/",
    "crates/dt-value/",
    "crates/dt-par/",
    "third_party/",
    "library/parts/",
];

fn plan(files: &[String], pkgs: &BTreeMap<String, (String, Vec<String>)>) -> Plan {
    let mut p = Plan::default();
    let mut touched = BTreeSet::new();
    for f in files {
        let f = f.as_str();
        if f.ends_with(".rs") || f.ends_with("Cargo.toml") || f == "Cargo.lock" {
            p.rust = true;
        }
        if f == "Cargo.lock" || f == "Cargo.toml" || f.starts_with("rust-toolchain") {
            // the compiler or a dependency: everything
            touched.extend(pkgs.keys().cloned());
            p.sweep_all();
            p.page = true;
            p.docs = true;
        }
        for (name, (dir, _)) in pkgs {
            if !dir.is_empty() && f.starts_with(&format!("{dir}/")) {
                touched.insert(name.clone());
            }
        }
        if GEOMETRY.iter().any(|g| f.starts_with(g)) {
            p.sweep_all();
        }
        if let Some(rest) = f.strip_prefix("crates/dt-elements/src/") {
            let head = rest.split('/').next().unwrap_or(rest);
            match ELEMENT_FILES.iter().find(|(file, _)| *file == head) {
                Some((_, names)) => p.sweep_some(names.iter().map(|n| (*n).to_owned())),
                None => p.sweep_all(),
            }
        }
        if f.starts_with("library/tiles/") && f.ends_with(".dtile") {
            p.sweep_some([format!("lib:{}", &f["library/".len()..])]);
        }
        if f.starts_with("library/") {
            // the test tiles and the goldens, the library's decoding
            touched.extend(["dt-eval", "dt-doc"].map(str::to_owned));
        }
        if f.starts_with("web/preview/") || f.starts_with("docs/assets/") {
            p.page = true;
            // the program serves the page and checks its contract
            touched.insert("dharmatiles".into());
        }
        if f.starts_with("docs/") || f.starts_with("crates/") || f.starts_with("xtask/") {
            p.docs = true;
        }
    }
    // a package's tests, and the tests of every package that depends on it
    let mut tests = touched.clone();
    loop {
        let more: Vec<String> = pkgs
            .iter()
            .filter(|(n, (_, deps))| !tests.contains(*n) && deps.iter().any(|d| tests.contains(d)))
            .map(|(n, _)| n.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        tests.extend(more);
    }
    p.tests = tests;
    p
}

/// How long the loop may take (Shawn, 14 Nov 2026): the checks run as work goes on, two minutes; everything,
/// before a push or a release, ten. A cold build is held to five minutes and a warm one to one, measured by hand.
/// Past a budget the loop has slowed: the next task is to make it fast again, before anything else.
const CHECK_BUDGET: f64 = 120.0;
const FULL_BUDGET: f64 = 600.0;

/// Say loudly when a run took longer than its budget, and keep every run's time in the repository's common git
/// folder (`dt-times.tsv`, shared by the worktrees), so a slow creep shows before it is half an hour.
fn over_budget(root: &Path, what: &str, secs: f64, budget: f64) -> Result<(), String> {
    use std::io::Write as _;
    let common = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let log = PathBuf::from(common).join("dt-times.tsv");
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let when = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let mut f = File::options()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| format!("{}: {e}", log.display()))?;
    writeln!(f, "{when}\t{branch}\t{what}\t{secs:.0}\t{budget:.0}")
        .map_err(|e| format!("{}: {e}", log.display()))?;
    if secs > budget {
        println!(
            "\nOVER BUDGET: {what} took {secs:.0} s, past its {budget:.0} s. The loop has slowed: queue a task to make it fast again, next, before other work (times: {})",
            log.display()
        );
    }
    Ok(())
}

/// One check: what it is, and the call that runs it.
type Step<'a> = Box<dyn Fn() -> Result<(), String> + 'a>;

/// `cargo xtask check [--full]`.
pub fn check(root: &Path, args: &[String]) -> Result<(), String> {
    let full = args.iter().any(|a| a == "--full");
    let pkgs = packages(root)?;
    let files = changed(root)?;
    let mut p = plan(&files, &pkgs);
    if full {
        p.rust = true;
        p.tests = pkgs.keys().cloned().collect();
        p.page = true;
        p.docs = true;
    }
    println!(
        "{} files changed against main{}",
        files.len(),
        if full { "; --full: every check" } else { "" }
    );
    let mut steps: Vec<(String, Step)> = Vec::new();
    // always: the cheap checks that read the whole tree
    steps.push((
        "the scans (dt-lang's tests: no code outside the registry; the changelog's waiting changes)".into(),
        Box::new(|| {
            crate::release::check_changes(root)?;
            run(cargo().current_dir(root).args(["test", "-q", "--release", "-p", "dt-lang"]))
        }),
    ));
    if p.rust {
        steps.push((
            "cargo fmt --check".into(),
            Box::new(|| run(cargo().current_dir(root).args(["fmt", "--check"]))),
        ));
        steps.push((
            "cargo clippy".into(),
            Box::new(|| {
                run(cargo().current_dir(root).args([
                    "clippy",
                    "--release",
                    "--workspace",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ]))
            }),
        ));
    }
    if !p.tests.is_empty() {
        let names: Vec<String> = p.tests.iter().cloned().collect();
        steps.push((
            format!("cargo test --release for {}", names.join(", ")),
            Box::new(move || {
                let mut c = cargo();
                // every test binary runs, so one failure does not hide another
                c.current_dir(root)
                    .args(["test", "-q", "--release", "--no-fail-fast"]);
                if names.len() == pkgs.len() {
                    c.arg("--workspace");
                } else {
                    for n in &names {
                        c.args(["-p", n]);
                    }
                }
                run(&mut c)
            }),
        ));
    }
    if p.page {
        steps.push((
            "npm test (web/preview)".into(),
            Box::new(|| {
                run(Command::new("npm")
                    .current_dir(root.join("web/preview"))
                    .args(["test", "--silent"]))
            }),
        ));
    }
    if p.docs {
        steps.push((
            "cargo xtask docs --check".into(),
            Box::new(|| crate::docs::run(root, true)),
        ));
    }
    if let Some(only) = &p.sweep {
        let only: Vec<String> = only.iter().cloned().collect();
        let what = if only.is_empty() {
            "the whole library".to_owned()
        } else {
            only.join(", ")
        };
        steps.push((
            format!("the sweep: {what}"),
            Box::new(move || sweep(root, &only)),
        ));
    }
    for (k, (what, _)) in steps.iter().enumerate() {
        println!("  {}. {what}", k + 1);
    }
    if args.iter().any(|a| a == "--plan") {
        return Ok(());
    }
    let t0 = Instant::now();
    // every check runs, so one failure does not hide the next
    let mut failed = Vec::new();
    for (k, (what, step)) in steps.iter().enumerate() {
        let t = Instant::now();
        println!("\n== {}. {what}", k + 1);
        let secs = || t.elapsed().as_secs_f64();
        match step() {
            Ok(()) => println!("== {}. passed in {:.0} s", k + 1, secs()),
            Err(e) => {
                println!("== {}. FAILED in {:.0} s: {e}", k + 1, secs());
                failed.push(format!("  {}. {what}: {e}", k + 1));
            }
        }
    }
    let total = t0.elapsed().as_secs_f64();
    over_budget(
        root,
        if full { "check --full" } else { "check" },
        total,
        if full { FULL_BUDGET } else { CHECK_BUDGET },
    )?;
    if failed.is_empty() {
        println!("\nall {} checks passed in {total:.0} s", steps.len());
        Ok(())
    } else {
        Err(format!(
            "\n{} of {} checks failed ({total:.0} s):\n{}",
            failed.len(),
            steps.len(),
            failed.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkgs() -> BTreeMap<String, (String, Vec<String>)> {
        let p = |dir: &str, deps: &[&str]| {
            (
                dir.to_owned(),
                deps.iter().map(|d| (*d).to_owned()).collect(),
            )
        };
        BTreeMap::from([
            ("dt-lang".into(), p("crates/dt-lang", &[])),
            ("dt-geom".into(), p("crates/dt-geom", &[])),
            ("dt-elements".into(), p("crates/dt-elements", &["dt-geom"])),
            ("dt-doc".into(), p("crates/dt-doc", &["dt-lang"])),
            (
                "dt-eval".into(),
                p("crates/dt-eval", &["dt-elements", "dt-doc"]),
            ),
            ("dharmatiles".into(), p("crates/dharmatiles", &["dt-eval"])),
            ("xtask".into(), p("xtask", &["dt-lang"])),
        ])
    }
    fn files(f: &[&str]) -> Vec<String> {
        f.iter().map(|s| (*s).to_owned()).collect()
    }
    fn set(s: &[&str]) -> BTreeSet<String> {
        s.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_page_change_runs_the_page_and_the_program_that_serves_it() {
        let p = plan(&files(&["web/preview/js/sheet.js"]), &pkgs());
        assert!(p.page && !p.rust && p.sweep.is_none() && !p.docs);
        assert_eq!(p.tests, set(&["dharmatiles"]));
    }

    #[test]
    fn an_element_sweeps_its_tiles_and_tests_what_depends_on_it() {
        let p = plan(&files(&["crates/dt-elements/src/grass/blade.rs"]), &pkgs());
        assert_eq!(p.sweep, Some(set(&["grass"])));
        assert_eq!(p.tests, set(&["dt-elements", "dt-eval", "dharmatiles"]));
        assert!(p.rust && p.docs && !p.page);
        // a shared helper of the elements: the whole library
        let p = plan(&files(&["crates/dt-elements/src/lib.rs"]), &pkgs());
        assert_eq!(p.sweep, Some(set(&[])));
    }

    #[test]
    fn geometry_and_dependencies_sweep_the_whole_library() {
        for f in [
            "crates/dt-geom/src/stack.rs",
            "Cargo.lock",
            "third_party/obvhs/src/lib.rs",
        ] {
            let p = plan(&files(&[f, "crates/dt-elements/src/water.rs"]), &pkgs());
            assert_eq!(p.sweep, Some(set(&[])), "{f}");
        }
    }

    #[test]
    fn a_library_tile_sweeps_itself() {
        let p = plan(&files(&["library/tiles/ground/1x1-grass.dtile"]), &pkgs());
        assert_eq!(p.sweep, Some(set(&["lib:tiles/ground/1x1-grass.dtile"])));
        assert!(!p.rust && !p.page);
    }

    #[test]
    fn docs_alone_check_the_docs() {
        let p = plan(&files(&["docs/guide/lines.html"]), &pkgs());
        assert!(p.docs && !p.rust && !p.page && p.tests.is_empty() && p.sweep.is_none());
    }
}
