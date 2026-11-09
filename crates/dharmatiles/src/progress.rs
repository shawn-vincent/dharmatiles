//! Progress for the command line: one status line on a terminal.
//!
//! `build` on a TTY (no `--json`) redraws a single stderr line as the build
//! goes: the tile's place in a batch, a bar, the percentage, the stage (with
//! an element's own count when it has one) and the elapsed time against the
//! estimate.  Piped or `--json` output is untouched.  The bar's weights come
//! from the last build's timings (`out/.cache/timings.tsv`) or the elements'
//! defaults (see `dt_eval::progress`).

use std::io::Write;
use std::sync::Arc;

use dt_eval::progress::{StageInfo, Status, Timings, Tracker, plan_stages};
use dt_eval::{Options, Progress, Source};

/// The stage the CLI adds after the build: writing the files.
pub(crate) const WRITE: &str = "write";

/// A tile's tracker, from a plan pass (`None` when the document does not
/// plan: the build will report why).
pub(crate) fn tracker_for(
    sources: &dyn Source,
    path: &str,
    opts: &Options,
    timings: &Timings,
    extra: Vec<StageInfo>,
    on_change: Box<dyn Fn(&Status) + Send + Sync>,
) -> Option<(Arc<Tracker>, Progress)> {
    // no progress bar when the plan fails: the build that follows reports why
    let checked = dt_eval::check(sources, path, opts).ok()?;
    let mut stages = plan_stages(&checked, path, opts, timings);
    if stages.is_empty() {
        return None;
    }
    stages.extend(extra);
    let t = Arc::new(Tracker::new(stages, on_change));
    let p = Progress::new(t.clone());
    Some((t, p))
}

/// `1m05s` or `7.1s`.
pub(crate) fn secs(s: f64) -> String {
    if s >= 60.0 {
        let m = (s / 60.0).floor();
        format!("{m:.0}m{:02.0}s", s - m * 60.0)
    } else {
        format!("{s:.1}s")
    }
}

/// The status line (no colour, no cursor codes), at most `width` columns.
pub(crate) fn line(prefix: &str, name: &str, s: &Status, width: usize) -> String {
    const BAR: usize = 20;
    let pct = (s.fraction * 100.0).floor().clamp(0.0, 100.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let filled = ((s.fraction * BAR as f64).floor() as usize).min(BAR);
    let bar: String = "#".repeat(filled) + &"-".repeat(BAR - filled);
    let stage = match &s.count {
        Some((what, _, 0)) => format!("{}: {what}", s.label),
        Some((what, d, t)) => format!("{} {what} {d}/{t}", s.label),
        None => s.label.clone(),
    };
    let time = if s.finished {
        secs(s.elapsed_s)
    } else {
        format!("{} of ~{}", secs(s.elapsed_s), secs(s.estimate_s))
    };
    let mut out = format!("{prefix}{name} [{bar}] {pct:.0}% {stage} {time}");
    if out.chars().count() > width {
        out = out
            .chars()
            .take(width.saturating_sub(1))
            .collect::<String>()
            + "…";
    }
    out
}

/// A status line on stderr that redraws in place, at most ten times a
/// second (the tracker throttles).
pub(crate) fn tty_sink(prefix: String, name: String) -> Box<dyn Fn(&Status) + Send + Sync> {
    let width = std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(40, 200)
        - 1;
    Box::new(move |s| {
        let mut e = std::io::stderr().lock();
        // Allowed silence: the status line is drawn on stderr, the only place
        // to report a failure to draw it.
        let _ = write!(e, "\r\x1b[2K{}", line(&prefix, &name, s, width));
        let _ = e.flush();
    })
}

/// Erase the status line.
pub(crate) fn clear() {
    let mut e = std::io::stderr().lock();
    let _ = write!(e, "\r\x1b[2K");
    let _ = e.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(f: f64) -> Status {
        Status {
            fraction: f,
            label: "grass".into(),
            key: "2:grass".into(),
            count: Some(("blades".into(), 120, 500)),
            elapsed_s: 7.14,
            eta_s: 3.9,
            estimate_s: 11.04,
            finished: false,
        }
    }

    #[test]
    fn the_line_shows_bar_stage_count_and_time_against_the_estimate() {
        let l = line("[3/42] ", "grass-tree", &status(0.62), 100);
        assert_eq!(
            l,
            "[3/42] grass-tree [############--------] 62% grass blades 120/500 7.1s of ~11.0s"
        );
    }

    #[test]
    fn the_line_fits_the_terminal_and_long_times_read_in_minutes() {
        assert!(line("", "n", &status(0.1), 30).chars().count() <= 30);
        assert_eq!(secs(65.0), "1m05s");
        assert_eq!(secs(7.14), "7.1s");
    }
}
