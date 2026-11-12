//! Turning stage events into a progress bar: the stages of a build, their
//! expected durations, and a percentage and ETA that never go backwards.
//!
//! The evaluator reports through [`dt_kit::Progress`] (see
//! [`crate::build_with`]); a front end attaches a [`Tracker`], which
//! weights the stages by how long they are expected to take:
//!
//! 1. the durations the last build of the same document and system
//!    measured ([`Timings`], kept in a small file under `out/.cache/`);
//! 2. else a default per element, per square of the tile ([`default_secs`]),
//!    from a quick profile of the library.
//!
//! Inside a stage an element may report done/total of its main loop; without
//! that, the bar creeps through the stage by its expected duration and
//! never reaches the end of it before the stage reports it is over.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dt_kit::{Event, Reporter};

use crate::doc::Tile;
use crate::plan::Step;
use crate::{Checked, Options};

/// The stage key of the union of all parts.
pub const UNION: &str = "union";
/// The stage key of the printability audit.
pub const AUDIT: &str = "audit";

/// `(key, label)` for each step of a build: `3:grass`, `lift`.
#[must_use]
pub fn stage_keys(tile: &Tile, steps: &[Step]) -> Vec<(String, String)> {
    steps
        .iter()
        .map(|s| match s {
            Step::Lift => ("lift".to_owned(), "lifting the ground".to_owned()),
            Step::Entry(i) => {
                let name = tile
                    .stack
                    .get(*i)
                    .map_or("connector", |e| e.what.value.as_str());
                (format!("{i}:{name}"), name.to_owned())
            }
        })
        .collect()
}

/// A rough duration in seconds of an element on a tile of `squares` squares,
/// from a profile of the 45 library tiles (release build, one Apple-silicon
/// Mac): the tree's cost is the tree's whatever the tile, a grass sward's
/// grows with its area, a wall's with its length (about one square).  Only
/// the fallback for a tile never built here; real timings replace it.
#[must_use]
pub fn default_secs(element: &str, squares: f64) -> f64 {
    match dt_doc::schema::element(element) {
        Some(e) => e.cost.secs(squares),
        None => match element {
            "terrain" | "connector" | "lift" => 0.01,
            _ => 0.1,
        },
    }
}

/// The union and the audit cost about what the elements built: from the
/// same profile, the union half of it and the audit a third, plus a
/// little.
#[must_use]
pub fn default_join_secs(elements_secs: f64) -> (f64, f64) {
    (0.5 * elements_secs + 0.1, 0.3 * elements_secs + 0.1)
}

/// One stage of the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct StageInfo {
    /// Its key.
    pub key: String,
    /// What to call it.
    pub label: String,
    /// Expected seconds.
    pub secs: f64,
}

/// The stages of `c`'s build, weighted from `timings` where the same
/// document and system were built before, else from [`default_secs`].
/// Front ends append their own (`encode`, `write`) with
/// [`StageInfo::new`].
#[must_use]
pub fn plan_stages(c: &Checked, doc: &str, opts: &Options, timings: &Timings) -> Vec<StageInfo> {
    let Some(tile) = &c.tile else {
        return Vec::new();
    };
    let squares = f64::from(tile.cols) * f64::from(tile.rows);
    let mut v: Vec<StageInfo> = stage_keys(tile, &c.steps)
        .into_iter()
        .map(|(key, label)| {
            let secs = timings
                .get(doc, &opts.system, &key)
                .unwrap_or_else(|| default_secs(&label, squares));
            StageInfo { key, label, secs }
        })
        .collect();
    let (union, audit) = default_join_secs(v.iter().map(|s| s.secs).sum());
    for (key, label, default) in [
        (UNION, "joining the parts", union),
        (AUDIT, "checking printability", audit),
    ] {
        let secs = timings.get(doc, &opts.system, key).unwrap_or(default);
        v.push(StageInfo {
            key: key.to_owned(),
            label: label.to_owned(),
            secs,
        });
    }
    v
}

impl StageInfo {
    /// A stage of a front end's own, expecting `default_secs` unless
    /// `timings` knows better.
    #[must_use]
    pub fn new(
        key: &str,
        label: &str,
        default_secs: f64,
        doc: &str,
        system: &str,
        timings: &Timings,
    ) -> Self {
        Self {
            key: key.to_owned(),
            label: label.to_owned(),
            secs: timings.get(doc, system, key).unwrap_or(default_secs),
        }
    }
}

/// Measured stage durations, remembered between builds in a small text
/// file (`document<TAB>system<TAB>stage<TAB>seconds`).
#[derive(Clone, Debug, Default)]
pub struct Timings {
    path: Option<PathBuf>,
    map: BTreeMap<String, f64>,
}

impl Timings {
    /// Where timings live under a project's `out/` folder.
    #[must_use]
    pub fn file_in(out: &Path) -> PathBuf {
        out.join(".cache").join("timings.tsv")
    }

    /// Read `path` (empty if missing or unreadable); [`Timings::save`]
    /// writes it back.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let mut map = BTreeMap::new();
        // Allowed silence: these timings only size the progress bar's
        // estimate, never what is built; a file not there, or unreadable,
        // means estimates from the elements' defaults.
        if let Ok(text) = std::fs::read_to_string(path) {
            for l in text.lines() {
                if let Some((k, v)) = l.rsplit_once('\t')
                    && let Ok(v) = v.parse::<f64>()
                    && v.is_finite()
                    && v >= 0.0
                {
                    map.insert(k.to_owned(), v);
                }
            }
        }
        Self {
            path: Some(path.to_owned()),
            map,
        }
    }

    fn key(doc: &str, system: &str, stage: &str) -> String {
        format!("{doc}\t{system}\t{stage}")
    }

    /// Seconds the stage took last time.
    #[must_use]
    pub fn get(&self, doc: &str, system: &str, stage: &str) -> Option<f64> {
        self.map.get(&Self::key(doc, system, stage)).copied()
    }

    /// Remember this build's stage durations (replacing the last ones).
    pub fn record(&mut self, doc: &str, system: &str, secs: &[(String, f64)]) {
        for (stage, s) in secs {
            self.map.insert(Self::key(doc, system, stage), *s);
        }
    }

    /// Write the file (best effort: a cache that cannot be written is
    /// only a worse estimate next time).  Atomic by rename.
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            // best effort, like the rest of this: the write below fails the same way
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = self
            .map
            .iter()
            .map(|(k, v)| format!("{k}\t{v:.4}\n"))
            .collect();
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, path).is_err() {
            // best-effort cleanup of the temporary file
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// Where a build stands.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    /// 0..=1, never decreasing; 1 only once finished.
    pub fraction: f64,
    /// The current stage's label.
    pub label: String,
    /// The current stage's key.
    pub key: String,
    /// What the stage is counting, if it is: `(what, done, total)`; with
    /// `total` 0, just the step it is on (`carving the core`).
    pub count: Option<(String, usize, usize)>,
    /// Seconds since the tracker started.
    pub elapsed_s: f64,
    /// Seconds still expected.
    pub eta_s: f64,
    /// Seconds expected in all (`elapsed_s + eta_s`).
    pub estimate_s: f64,
    /// Finished.
    pub finished: bool,
}

struct State {
    stages: Vec<StageInfo>,
    done: Vec<bool>,
    cur: Option<usize>,
    cur_start: Instant,
    sub: f64,
    count: Option<(String, usize, usize)>,
    best: f64,
    finished: bool,
    last_emit: Option<Instant>,
}

/// A [`Reporter`] that turns events into [`Status`]es, calling `on_change`
/// at most about ten times a second (and at once when a stage changes).
pub struct Tracker {
    t0: Instant,
    state: Mutex<State>,
    on_change: Box<dyn Fn(&Status) + Send + Sync>,
}

const MIN_GAP: Duration = Duration::from_millis(100);
/// A stage with no count creeps only this far before it reports its end.
const CREEP_CAP: f64 = 0.95;

impl Tracker {
    /// Track `stages` (in order); `on_change` hears each new status.
    #[must_use]
    pub fn new(stages: Vec<StageInfo>, on_change: Box<dyn Fn(&Status) + Send + Sync>) -> Self {
        let n = stages.len();
        Self {
            t0: Instant::now(),
            state: Mutex::new(State {
                stages,
                done: vec![false; n],
                cur: None,
                cur_start: Instant::now(),
                sub: 0.0,
                count: None,
                best: 0.0,
                finished: false,
                last_emit: None,
            }),
            on_change,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The current status.
    #[must_use]
    pub fn status(&self) -> Status {
        let mut s = self.lock();
        self.compute(&mut s)
    }

    /// The build is over: 100%.
    pub fn finish(&self) {
        let mut s = self.lock();
        s.finished = true;
        s.cur = None;
        let st = self.compute(&mut s);
        drop(s);
        (self.on_change)(&st);
    }

    #[allow(clippy::cast_precision_loss)]
    fn compute(&self, s: &mut State) -> Status {
        let elapsed = self.t0.elapsed().as_secs_f64();
        let total: f64 = s.stages.iter().map(|x| x.secs.max(0.01)).sum();
        let mut done_w: f64 = s
            .stages
            .iter()
            .zip(&s.done)
            .filter(|(_, d)| **d)
            .map(|(x, _)| x.secs.max(0.01))
            .sum();
        let (mut label, mut key) = (String::new(), String::new());
        if let Some(i) = s.cur {
            let w = s.stages[i].secs.max(0.01);
            let creep = (s.cur_start.elapsed().as_secs_f64() / w).min(CREEP_CAP);
            done_w += w * s.sub.max(creep).min(1.0);
            label.clone_from(&s.stages[i].label);
            key.clone_from(&s.stages[i].key);
        }
        let raw = if total > 0.0 { done_w / total } else { 0.0 };
        let fraction = if s.finished {
            1.0
        } else {
            s.best = s.best.max(raw.min(0.99));
            s.best
        };
        // how much slower (or faster) than expected this build is running
        // (trusted more as more of the build is behind us: early sub-progress
        // can run ahead of the clock)
        let ratio = if done_w > 0.5 {
            let r = (elapsed / done_w).clamp(0.25, 8.0);
            let trust = (done_w / (0.4 * total)).min(1.0);
            1.0 + (r - 1.0) * trust
        } else {
            1.0
        };
        let eta = if s.finished {
            0.0
        } else {
            ((total - done_w).max(0.0) * ratio).max(0.0)
        };
        Status {
            fraction,
            label,
            key,
            count: s.count.clone(),
            elapsed_s: elapsed,
            eta_s: eta,
            estimate_s: elapsed + eta,
            finished: s.finished,
        }
    }

    fn emit(&self, force: bool) {
        let mut s = self.lock();
        let now = Instant::now();
        if !force && s.last_emit.is_some_and(|t| now.duration_since(t) < MIN_GAP) {
            return;
        }
        s.last_emit = Some(now);
        let st = self.compute(&mut s);
        drop(s);
        (self.on_change)(&st);
    }
}

impl Reporter for Tracker {
    fn report(&self, e: &Event<'_>) {
        match *e {
            Event::Begin { key, label } => {
                {
                    let mut s = self.lock();
                    let i = s
                        .stages
                        .iter()
                        .position(|x| x.key == key)
                        .unwrap_or_else(|| {
                            // a stage nobody planned: a small one
                            s.stages.push(StageInfo {
                                key: key.to_owned(),
                                label: label.to_owned(),
                                secs: 0.05,
                            });
                            s.done.push(false);
                            s.stages.len() - 1
                        });
                    s.cur = Some(i);
                    s.cur_start = Instant::now();
                    s.sub = 0.0;
                    s.count = None;
                }
                self.emit(true);
            }
            Event::End { key } => {
                {
                    let mut s = self.lock();
                    if let Some(i) = s.stages.iter().position(|x| x.key == key) {
                        s.done[i] = true;
                    }
                    s.cur = None;
                    s.sub = 0.0;
                    s.count = None;
                }
                self.emit(true);
            }
            Event::Sub {
                what,
                done,
                total,
                fraction,
            } => {
                {
                    let mut s = self.lock();
                    s.sub = s.sub.max(fraction.clamp(0.0, 1.0));
                    s.count =
                        (total > 0 || !what.is_empty()).then(|| (what.to_owned(), done, total));
                }
                self.emit(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn tracker(stages: &[(&str, f64)]) -> (Arc<Tracker>, Arc<Mutex<Vec<Status>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let l = Arc::clone(&log);
        let t = Tracker::new(
            stages
                .iter()
                .map(|(k, s)| StageInfo {
                    key: (*k).to_owned(),
                    label: (*k).to_owned(),
                    secs: *s,
                })
                .collect(),
            Box::new(move |s| l.lock().unwrap().push(s.clone())),
        );
        (Arc::new(t), log)
    }

    #[test]
    fn the_bar_never_goes_back_and_ends_at_one() {
        let (t, log) = tracker(&[("a", 1.0), ("b", 3.0), ("c", 1.0)]);
        let p = dt_kit::Progress::new(t.clone());
        p.begin("a", "a");
        p.end("a");
        p.begin("b", "b");
        let c = p.counter("things", 100, 0.0, 1.0);
        for _ in 0..100 {
            c.tick();
        }
        // a second counter restarting lower must not pull the bar back
        p.fraction("x", 0.1);
        p.end("b");
        p.begin("c", "c");
        p.end("c");
        t.finish();
        let v = log.lock().unwrap();
        for w in v.windows(2) {
            assert!(w[1].fraction >= w[0].fraction, "{:?} then {:?}", w[0], w[1]);
        }
        assert!((v.last().unwrap().fraction - 1.0).abs() < 1e-12);
        assert!(v.iter().rev().skip(1).all(|s| s.fraction < 1.0));
        assert!(v.last().unwrap().eta_s.abs() < 1e-12);
    }

    #[test]
    fn stages_are_weighted_by_expected_time_not_by_count() {
        let (t, _) = tracker(&[("fast", 1.0), ("slow", 9.0)]);
        let p = dt_kit::Progress::new(t.clone());
        p.begin("fast", "fast");
        p.end("fast");
        // one of two stages done, but only a tenth of the time
        let f = t.status().fraction;
        assert!((f - 0.1).abs() < 0.01, "{f}");
    }

    #[test]
    fn a_stage_with_no_count_creeps_but_never_completes_early() {
        let (t, _) = tracker(&[("a", 0.001), ("b", 1.0)]);
        let p = dt_kit::Progress::new(t.clone());
        p.begin("a", "a");
        std::thread::sleep(Duration::from_millis(30));
        assert!(t.status().fraction < 0.5, "capped at its own weight");
    }

    #[test]
    fn timings_round_trip_through_a_file() {
        let dir = std::env::temp_dir().join(format!("dt-timings-{}", std::process::id()));
        let file = Timings::file_in(&dir);
        let mut t = Timings::load(&file);
        assert!(t.get("d.dtile", "db", "union").is_none());
        t.record("d.dtile", "db", &[("union".to_owned(), 1.25)]);
        t.save();
        let again = Timings::load(&file);
        assert_eq!(again.get("d.dtile", "db", "union"), Some(1.25));
        assert!(again.get("d.dtile", "ol", "union").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
