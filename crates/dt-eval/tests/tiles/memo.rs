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
//! A test that needs other options (another tier, cells, system or seed)
//! builds that itself; [`made`] memoises such a build by its options.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use dt_eval::progress::{Status, Timings, Tracker, plan_stages};
use dt_eval::{Built, Library, Memory, Options, Progress, build_with, check};
use dt_kit::{Event, Material, Reporter, TriMesh};

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
    let opts = Options {
        debug: true,
        clouds: true,
        ..Options::default()
    };
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

type Slot<T> = Arc<OnceLock<&'static T>>;

/// The slot for `key`, made on first ask; the map's lock is held only for
/// that, so builds of different tiles run side by side and the tests that
/// want the same one wait for it.
fn slot<T>(map: &Mutex<BTreeMap<String, Slot<T>>>, key: &str) -> Slot<T> {
    Arc::clone(
        map.lock()
            .unwrap()
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new(OnceLock::new())),
    )
}

/// The tile, built once with every observer on.  Never call this from a
/// rayon worker: the build runs on its own pool and the caller waits on it.
pub fn observed(path: &str) -> &'static Observed {
    static MEMO: Mutex<BTreeMap<String, Slot<Observed>>> = Mutex::new(BTreeMap::new());
    slot(&MEMO, path).get_or_init(|| Box::leak(Box::new(observe(path))))
}

/// The tile's observed build.
pub fn built(path: &str) -> &'static Built {
    &observed(path).built
}

/// A build with `opts` the observed one does not have (another tier, cells,
/// system or seed), made once per tile and options.
pub fn made(path: &str, opts: &Options, key: &str) -> &'static Built {
    static MEMO: Mutex<BTreeMap<String, Slot<Built>>> = Mutex::new(BTreeMap::new());
    slot(&MEMO, &format!("{path} {key}")).get_or_init(|| {
        Box::leak(Box::new(
            dt_eval::build(&Library, path, opts).unwrap_or_else(|e| panic!("{path}: {e:?}")),
        ))
    })
}

/// The tile written in `text`, built with the default options once however
/// many tests ask.
pub fn from_text(text: &str) -> &'static Built {
    static MEMO: Mutex<BTreeMap<String, Slot<Built>>> = Mutex::new(BTreeMap::new());
    slot(&MEMO, text).get_or_init(|| {
        let src = Memory(BTreeMap::from([("t.dtile".to_owned(), text.to_owned())]));
        Box::leak(Box::new(
            dt_eval::build(&src, "t.dtile", &Options::default())
                .unwrap_or_else(|e| panic!("{e:#?}")),
        ))
    })
}

/// A build's stage names and hashes.
pub fn hashes(b: &Built) -> String {
    b.report
        .stages
        .iter()
        .map(|s| format!("{:016x}  {}\n", s.hash, s.name))
        .collect()
}
