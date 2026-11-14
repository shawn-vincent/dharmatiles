//! What the tests share: **one build of anything, kept for everything the
//! build reads**, so a test run never builds the same tile twice with the
//! same inputs.
//!
//! A test binary is its own process, so each binary has its own memo; the
//! memo here is the one piece of code behind all of them (dt-eval's `tiles`
//! binary adds the observers on top of it, see its `memo`).  It is a
//! dev-dependency only: the program does not carry it.
//!
//! A test that must build again (it tests the build, the cache, the disk
//! store or the threads, or builds twice to compare) calls `dt_eval::build`
//! itself and says why in a `// fresh: …` comment on that line or the one
//! above; dt-eval's `memo_only` test scans for any that do not.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use dt_eval::{Built, Diag, Files, Library, Memory, Options, Source};

/// A once-made value, leaked so that every test can hold it for the run.
pub type Slot<T> = Arc<OnceLock<&'static T>>;

/// The slot for `key`, made on first ask; the map's lock is held only for
/// that, so builds of different tiles run side by side and the tests that
/// want the same one wait for it.
pub fn slot<T>(map: &Mutex<BTreeMap<String, Slot<T>>>, key: &str) -> Slot<T> {
    Arc::clone(
        map.lock()
            .unwrap()
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new(OnceLock::new())),
    )
}

/// What a document is read from, and the key of everything it reads: the
/// same key is the same bytes in, so one build stands for every ask.
pub trait Reads: Source {
    /// Names the source's whole content (the library on disk, the documents
    /// held in memory, the files on disk), apart from the path asked for.
    fn reads(&self) -> String;
}

impl Reads for Library {
    fn reads(&self) -> String {
        "library".to_owned()
    }
}

impl Reads for Files {
    fn reads(&self) -> String {
        "files".to_owned()
    }
}

impl Reads for Memory {
    /// Every document held, text and all.
    fn reads(&self) -> String {
        format!("{:?}", self.0)
    }
}

/// A build and how long it took.
struct Made {
    result: Result<Built, Vec<Diag>>,
    secs: f64,
}

/// **The one entry**: `path` read from `src`, built with `opts`, once however
/// many tests ask, and however many threads.  Its key is everything the
/// build reads: the source's content ([`Reads`]), the path and every field of
/// [`Options`] (its derived `Debug`, so a field added later is part of the key
/// without anyone remembering to add it, and the printer's name, which
/// `Debug` leaves out).  A build that fails is kept too: the diagnostics are
/// the answer.
pub fn build<S: Reads>(
    src: &S,
    path: &str,
    opts: &Options,
) -> Result<&'static Built, &'static Vec<Diag>> {
    timed(src, path, opts).0
}

/// [`build`], and the seconds the one build took (the first ask's: later asks
/// read it back, they do not build).
pub fn timed<S: Reads>(
    src: &S,
    path: &str,
    opts: &Options,
) -> (Result<&'static Built, &'static Vec<Diag>>, f64) {
    static MEMO: Mutex<BTreeMap<String, Slot<Made>>> = Mutex::new(BTreeMap::new());
    let key = format!("{}\0{path}\0{opts:?}\0{}", src.reads(), opts.printer.name);
    let slot = slot(&MEMO, &key);
    let made = *slot.get_or_init(|| {
        let t0 = Instant::now();
        // the memo's one build: every other test build reads it
        let result = dt_eval::build(src, path, opts);
        let secs = t0.elapsed().as_secs_f64();
        Box::leak(Box::new(Made { result, secs }))
    });
    (made.result.as_ref(), made.secs)
}
