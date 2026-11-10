//! The progress side channel: a
//! reporter never changes a build, and the bar it drives never goes back
//! and ends at 100%.

#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use dt_eval::progress::{Status, Timings, Tracker, plan_stages};
use dt_eval::{Built, Library, Options, Progress, build, build_with, check};
use rayon::prelude::*;

/// One tile for each element that reports sub-progress.
const TILES: &[&str] = &[
    "lib:tests/plants/2x2-grass.dtile",
    "lib:tests/walls/1x1-wall-cut-stone.dtile",
    "lib:tests/rocks/2x2-rocks.dtile",
    "lib:tests/plants/1x1-tree.dtile",
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

/// Each tile tracked on two threads and built plain on the whole machine,
/// all at once: the reporter changes nothing, and progress only climbs,
/// to 100% at the end.
#[test]
fn progress_climbs_to_100_percent_and_changes_nothing() {
    TILES.par_iter().for_each(|path| {
        let plain = fingerprint(&build(&Library, path, &Options::default()).unwrap());
        let (b, v) = tracked(path, 2);
        assert_eq!(
            fingerprint(&b),
            plain,
            "{path}: the reporter changed the build"
        );
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
    });
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
fn elements_with_a_loop_report_their_counts() {
    [
        ("lib:tests/walls/1x1-wall-cut-stone.dtile", "stones"),
        ("lib:tests/plants/2x2-grass.dtile", "blades"),
        ("lib:tests/rocks/2x2-rocks.dtile", "stones"),
    ]
    .par_iter()
    .for_each(|&(path, what)| {
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
    });
}

/// Takes the sketches a build sends, and nothing else.
struct Sketches(Mutex<Vec<(usize, bool)>>);

impl dt_kit::Reporter for Sketches {
    fn report(&self, _: &dt_kit::Event<'_>) {}
    fn sketching(&self) -> bool {
        true
    }
    fn sketch(&self, mesh: &dt_kit::TriMesh, materials: &[dt_kit::Material], rough: bool) {
        assert_eq!(mesh.triangles.len(), materials.len());
        self.0.lock().unwrap().push((mesh.triangles.len(), rough));
    }
}

/// A front end that wants sketches gets the cliff's rough boxes, then each
/// stage's parts, and the build is the same as one without.
#[test]
fn sketches_come_as_the_build_goes_and_change_nothing() {
    let path = "lib:tests/cliffs/1x1-cliff-mound.dtile";
    let opts = Options::default();
    let plain = fingerprint(&build(&Library, path, &opts).unwrap());
    let seen = Arc::new(Sketches(Mutex::new(Vec::new())));
    let b = build_with(&Library, path, &opts, &Progress::new(seen.clone())).unwrap();
    assert_eq!(fingerprint(&b), plain, "sketching changed the build");
    let v = seen.0.lock().unwrap().clone();
    let first_rough = v.iter().position(|s| s.1).expect("the cliff's rough boxes");
    assert!(v[..first_rough].iter().all(|s| !s.1 && s.0 > 0), "{v:?}");
    assert!(
        v.iter().filter(|s| !s.1).count() >= 3,
        "ground, cliff, base at least: {v:?}"
    );
    assert!(
        !v.last().unwrap().1,
        "the last sketch is finished parts: {v:?}"
    );
}
