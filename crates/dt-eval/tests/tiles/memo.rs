//! Each test tile, built once for the whole binary, with every observer on.
//!
//! A tile is built on a two-thread pool (so the pool a build ran on is never
//! the machine's), with the progress tracker, the sketches and the editor's
//! debug layer and clouds all listening.  Every test that wants the tile
//! reads this one build.  That the observers change nothing is not checked
//! here by building again: the build's stage hashes are checked against the
//! snapshots in `golden`, which were recorded from builds with none of them
//! (`golden_hashes` for the golden tiles, `observers_change_nothing` for
//! the rest), so a match proves that threads, progress, sketching and the
//! debug layer leave every hash where it was.
//!
//! A test that wants any other build (another tier, cells, system or seed, or
//! a document written in the test) asks [`build`], which keeps one build for
//! everything it reads, whatever the test, so no tile is built twice with the
//! same inputs.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dt_eval::progress::{Status, Timings, Tracker, plan_stages};
use dt_eval::{Built, Library, Memory, Options, Progress, build_with, check};
use dt_kit::{Event, Material, Reporter, TriMesh};
use dt_testkit::{Slot, slot};

/// A tile's build and what its observers saw.
pub struct Observed {
    pub built: Built,
    /// The tracker's reports, in the order they came.
    pub status: Vec<Status>,
    /// The `done`/`total` counts of elements that report their loops.
    pub subs: Vec<(String, usize, usize)>,
    /// `(triangles, rough)` of each sketch.
    pub sketches: Vec<(usize, bool)>,
}

/// Forwards to the tracker and keeps what else it hears.
struct Observer {
    tracker: Arc<Tracker>,
    subs: Mutex<Vec<(String, usize, usize)>>,
    sketches: Mutex<Vec<(usize, bool)>>,
}

impl Reporter for Observer {
    fn report(&self, event: &Event<'_>) {
        if let Event::Sub {
            what, done, total, ..
        } = event
        {
            self.subs
                .lock()
                .unwrap()
                .push(((*what).to_owned(), *done, *total));
        }
        self.tracker.report(event);
    }
    fn sketching(&self) -> bool {
        true
    }
    fn sketch(&self, mesh: &TriMesh, materials: &[Material], rough: bool) {
        assert_eq!(mesh.triangles.len(), materials.len());
        self.sketches
            .lock()
            .unwrap()
            .push((mesh.triangles.len(), rough));
    }
}

fn observe(path: &str) -> Observed {
    let opts = dt_testkit::seeded(&Options {
        debug: true,
        clouds: true,
        ..Options::default()
    });
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = Arc::clone(&log);
    let checked = check(&Library, path, &opts).unwrap_or_else(|e| panic!("{path}: {e:?}"));
    let stages = plan_stages(&checked, path, &opts, &Timings::default());
    let tracker = Arc::new(Tracker::new(
        stages,
        Box::new(move |s| l.lock().unwrap().push(s.clone())),
    ));
    let observer = Arc::new(Observer {
        tracker: Arc::clone(&tracker),
        subs: Mutex::default(),
        sketches: Mutex::default(),
    });
    let progress = Progress::new(observer.clone());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .unwrap();
    let built = pool
        .install(|| build_with(&Library, path, &opts, &progress))
        .unwrap_or_else(|e| panic!("{path}: {e:?}"));
    tracker.finish();
    let status = log.lock().unwrap().clone();
    let subs = observer.subs.lock().unwrap().clone();
    let sketches = observer.sketches.lock().unwrap().clone();
    Observed {
        built,
        status,
        subs,
        sketches,
    }
}

/// The tile, built once with every observer on.  Never call this from a
/// rayon worker: the build runs on its own pool and the caller waits on it.
pub fn observed(path: &str) -> &'static Observed {
    warm_reads(path);
    static MEMO: Mutex<BTreeMap<String, Slot<Observed>>> = Mutex::new(BTreeMap::new());
    slot(&MEMO, path).get_or_init(|| Box::leak(Box::new(observe(path))))
}

/// Build every one of `paths` that is not built yet, several side by side: each build keeps its own pool of two
/// threads (the proof that threads change nothing), so one at a time leaves most of the machine idle. A test that
/// reads many tiles calls this first; the builds are the same, only sooner.
pub fn prebuild(paths: &[&str]) {
    /// Builds at once: six two-thread pools fill twelve cores.
    const AT_ONCE: usize = 6;
    let next = std::sync::atomic::AtomicUsize::new(0);
    // the workers carry the test's name, so the warm rule knows whose builds they are
    let name = format!("{}/prebuild", std::thread::current().name().unwrap_or(""));
    std::thread::scope(|s| {
        for _ in 0..AT_ONCE {
            let worker = std::thread::Builder::new().name(name.clone());
            worker
                .spawn_scoped(s, || {
                    while let Some(p) =
                        paths.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                    {
                        observed(p);
                    }
                })
                .expect("a prebuild worker starts");
        }
    });
}

/// A warm test reads only the torture set ([`super::golden::TILES`]): a test tile outside it is built only for a
/// `thorough_` test, which `cargo xtask check --full` runs, so the warm check never pays for it.
fn warm_reads(path: &str) {
    if !path.starts_with("lib:tests/") || super::golden::TILES.contains(&path) {
        return;
    }
    let t = std::thread::current();
    let name = t.name().unwrap_or("");
    assert!(
        name.contains("thorough_"),
        "{name}: a warm test reads {path}, which is not in the torture set (golden::TILES); read a torture tile, or make the test thorough_ (an ignored test that cargo xtask check --full runs)"
    );
}

/// The tile's observed build.
pub fn built(path: &str) -> &'static Built {
    &observed(path).built
}

/// The memo's one general entry, shared with the other crates' tests (see `dt_testkit`), held to the warm rule.
pub fn build<S: dt_testkit::Reads>(
    src: &S,
    path: &str,
    opts: &Options,
) -> Result<&'static Built, &'static Vec<dt_eval::Diag>> {
    warm_reads(path);
    dt_testkit::build(src, path, opts)
}

/// [`build`], with the seconds the one build took.
pub fn timed<S: dt_testkit::Reads>(
    src: &S,
    path: &str,
    opts: &Options,
) -> (Result<&'static Built, &'static Vec<dt_eval::Diag>>, f64) {
    warm_reads(path);
    dt_testkit::timed(src, path, opts)
}

/// The tile written in `text`, built with the default options once however
/// many tests ask.
pub fn from_text(text: &str) -> &'static Built {
    let src = Memory(BTreeMap::from([("t.dtile".to_owned(), text.to_owned())]));
    build(&src, "t.dtile", &Options::default()).unwrap_or_else(|e| panic!("{e:#?}"))
}

/// A build's stage names and hashes.
pub fn hashes(b: &Built) -> String {
    b.report
        .stages
        .iter()
        .map(|s| format!("{:016x}  {}\n", s.hash, s.name))
        .collect()
}
