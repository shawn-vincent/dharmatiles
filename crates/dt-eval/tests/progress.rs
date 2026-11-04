//! The progress side channel (docs/rust/preview.html, "Progress"): a
//! reporter never changes a build, and the bar it drives never goes back
//! and ends at 100%.

#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use dt_eval::progress::{Status, Timings, Tracker, plan_stages};
use dt_eval::{Built, Library, Options, Progress, build, build_with, check};

/// One tile for each look that reports sub-progress.
const TILES: &[&str] = &[
    "lib:tiles/test/2x2-grass.kdl",
    "lib:tiles/test/1x1-wall-cut-stone.kdl",
    "lib:tiles/test/2x2-rocks.kdl",
    "lib:tiles/test/1x1-tree.kdl",
];

fn tracked(path: &str, threads: usize) -> (Built, Vec<Status>) {
    let opts = Options::default();
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = Arc::clone(&log);
    let checked = check(&Library, path, &opts).unwrap();
    let stages = plan_stages(&checked, path, &opts, &Timings::default());
    let tracker = Arc::new(Tracker::new(
        stages,
        Box::new(move |s| l.lock().unwrap().push(s.clone())),
    ));
    let progress = Progress::new(tracker.clone());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap();
    let b = pool
        .install(|| build_with(&Library, path, &opts, &progress))
        .unwrap();
    tracker.finish();
    let v = log.lock().unwrap().clone();
    (b, v)
}

fn fingerprint(b: &Built) -> (Vec<(String, u64)>, u64) {
    (
        b.stages.iter().map(|s| (s.name.clone(), s.hash)).collect(),
        b.export.mesh.content_hash(),
    )
}

#[test]
fn a_reporter_changes_nothing_at_1_4_and_16_threads() {
    for path in TILES {
        let plain = fingerprint(&build(&Library, path, &Options::default()).unwrap());
        for threads in [1, 4, 16] {
            let (b, _) = tracked(path, threads);
            assert_eq!(fingerprint(&b), plain, "{path} at {threads} threads");
        }
    }
}

#[test]
fn progress_is_monotonic_and_ends_at_100_percent() {
    for path in TILES {
        let (b, v) = tracked(path, 4);
        assert!(v.len() > 3, "{path}: {} reports", v.len());
        for w in v.windows(2) {
            assert!(w[1].fraction >= w[0].fraction, "{path}: {w:?}");
            assert!(w[1].eta_s >= 0.0);
        }
        let last = v.last().unwrap();
        assert!(
            (last.fraction - 1.0).abs() < 1e-12 && last.finished,
            "{path}"
        );
        assert!(v.iter().rev().skip(1).all(|s| s.fraction < 1.0), "{path}");
        // every stage reported its own time
        assert!(b.stage_secs.iter().any(|(k, _)| k == "union"), "{path}");
        assert!(b.stage_secs.iter().any(|(k, _)| k == "audit"), "{path}");
    }
}

#[derive(Default)]
struct Subs(Mutex<Vec<(String, usize, usize)>>);
impl dt_kit::Reporter for Subs {
    fn report(&self, e: &dt_kit::Event<'_>) {
        if let dt_kit::Event::Sub {
            what, done, total, ..
        } = e
        {
            self.0
                .lock()
                .unwrap()
                .push(((*what).to_owned(), *done, *total));
        }
    }
}

#[test]
fn looks_with_a_loop_report_their_counts() {
    for (path, what) in [
        ("lib:tiles/test/1x1-wall-cut-stone.kdl", "stones"),
        ("lib:tiles/test/2x2-grass.kdl", "blades"),
        ("lib:tiles/test/2x2-rocks.kdl", "stones"),
    ] {
        let subs = Arc::new(Subs::default());
        build_with(
            &Library,
            path,
            &Options::default(),
            &Progress::new(subs.clone()),
        )
        .unwrap();
        let v = subs.0.lock().unwrap();
        let mine: Vec<_> = v.iter().filter(|s| s.0 == what).collect();
        assert!(!mine.is_empty(), "{path} reports its {what}");
        assert!(
            mine.iter().all(|s| s.1 <= s.2),
            "{path}: done never passes total"
        );
        // threads may report out of order; the end is the largest count
        assert!(
            mine.iter().any(|s| s.1 == s.2),
            "{path}: the loop reports its end"
        );
    }
}
