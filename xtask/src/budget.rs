//! Every xtask command, and every step of a check, is timed against a budget: it prints how long it took against
//! its budget, every run is logged to `dt-times.tsv` in the repository's common git folder (shared by the
//! worktrees), and a run past its budget says OVER BUDGET loudly, on both streams, so a slow creep is noticed the
//! day it starts and not when a change takes half an hour (Shawn, 14 Nov 2026: "all the build steps and checks
//! should be timed, print their runtimes, and if over budget complain so we notice").
//!
//! The budgets are here and nowhere else (docs/standards/performance.html explains them and where each stands).
//! Past a budget the loop has slowed: the next task is to make it fast again, before anything else. A budget is
//! raised only by editing this table, where the diff shows it.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// The loop's budgets, seconds: the checks run as work goes on, two minutes; everything before a push or a
/// release, ten; a warm build one; a cold build five.
pub const CHECK: f64 = 120.0;
pub const FULL: f64 = 600.0;
pub const WARM_BUILD: f64 = 60.0;
pub const COLD_BUILD: f64 = 300.0;

/// Each step of `cargo xtask check`, seconds. The build is its own step, held to a warm or a cold build's budget
/// ([`build`]), so the steps after it time only their own work.
pub mod step {
    pub const SCANS: f64 = 15.0;
    pub const FMT: f64 = 10.0;
    pub const CLIPPY: f64 = 30.0;
    pub const TESTS: f64 = 60.0;
    /// `check --full`'s run of dt-eval: warm, thorough and the whole-library sweep (227 s on 14 Nov 2026).
    pub const DT_EVAL_FULL: f64 = 300.0;
    pub const PAGE: f64 = 30.0;
    pub const DOCS: f64 = 30.0;
}

/// The build's budget in `root`: a warm build's, or a cold one's when this checkout has no release build yet (a
/// fresh clone or worktree).
#[must_use]
pub fn build(root: &Path) -> f64 {
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    if target.join("release/deps").is_dir() {
        WARM_BUILD
    } else {
        COLD_BUILD
    }
}

/// Each command's budget, seconds, by its name as typed (`cargo xtask <name>`); `check` sets its own.
#[must_use]
pub fn of_command(name: &str, args: &[String]) -> Option<f64> {
    let check_only = args.iter().any(|a| a == "--check");
    Some(match name {
        // the whole library, 60 tiles five at a time
        "sweep" => 240.0,
        "sheet" => 120.0,
        "pictures" => 300.0,
        "docs" => 45.0,
        "regen" => 90.0,
        "examples" => 300.0,
        "shots" => 300.0,
        "wasm" if check_only => 90.0,
        "wasm" => 300.0,
        // the checks, the release gate and the baseline of every tile
        "release" => 1200.0,
        _ => return None,
    })
}

/// Run `f`, then say how long it took against `budget` and log it.
///
/// # Errors
/// `f`'s error (its time still said and logged), or the log's.
pub fn timed<T>(
    root: &Path,
    what: &str,
    budget: f64,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let t = Instant::now();
    let out = f();
    let secs = t.elapsed().as_secs_f64();
    record(root, what, secs, budget, out.is_ok())?;
    out
}

/// Say how long `what` took against `budget`, loudly when past it, and keep the time in `dt-times.tsv`.
///
/// # Errors
/// When git cannot name the common folder or the log cannot be written.
pub fn record(root: &Path, what: &str, secs: f64, budget: f64, ok: bool) -> Result<(), String> {
    use std::io::Write as _;
    let log = log_file(root)?;
    let branch = crate::work::git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
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
    let how = if ok { "took" } else { "failed after" };
    if secs > budget {
        let banner = format!(
            "\n!!! OVER BUDGET: {what} took {secs:.0} s, past its {budget:.0} s ({:.0}% over).\n!!! The loop has slowed: queue a task to make it fast again, next, before other work.\n!!! Times: {}\n",
            (secs / budget - 1.0) * 100.0,
            log.display()
        );
        println!("{banner}");
        eprintln!("{banner}");
    } else {
        println!("[time] {what} {how} {secs:.0} s of its {budget:.0} s");
    }
    Ok(())
}

/// `dt-times.tsv` in the common git folder.
fn log_file(root: &Path) -> Result<PathBuf, String> {
    let common = crate::work::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("dt-times.tsv"))
}
