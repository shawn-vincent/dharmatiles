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
//!
//! **A run at another seed.**  `DT_TEST_SEED=N` builds every tile a test
//! builds on seed N instead of the tile's own, unless the test sets a seed
//! itself ([`seeded`]): `DT_TEST_SEED=7 cargo xtask check`, say, to see what
//! holds at seed 7. The snapshots and the library baseline are recorded at
//! each tile's own seed, so a run at another seed compares only what does not
//! depend on it (the gate's verdict, the repair budgets) and says so once, at
//! the start, past the test harness's capture.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use dt_eval::{Built, Diag, Files, Library, Memory, Options, Source};

/// The seed every test build is made on in place of each tile's own: `DT_TEST_SEED`, or none (the default: each
/// tile on its own seed).
///
/// # Panics
/// If `DT_TEST_SEED` is set and is not a whole number: a run asked for a seed it would not get.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: an environment variable or directory that is not set or not readable is the meaning (the default applies); a failure that panics with its message named (a broken invariant or a failing test): shown loudly, not swallowed"
)]
pub fn test_seed() -> Option<u64> {
    static SEED: OnceLock<Option<u64>> = OnceLock::new();
    *SEED.get_or_init(|| {
        let s = std::env::var("DT_TEST_SEED").ok()?;
        let n: u64 = s
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("DT_TEST_SEED={s}: not a seed (a whole number)"));
        // straight to stderr, not through `eprintln!`, which the test harness keeps for failing tests only
        use std::io::Write as _;
        std::io::stderr()
            .write_all(
                format!(
                    "DT_TEST_SEED={n}: every tile a test builds is built on seed {n} (a test that sets its own seed \
                     keeps it); the golden hashes and metrics and the library baseline were recorded at each tile's \
                     own seed, so only the gate's verdict and the repair budgets are compared with them\n"
                )
                .as_bytes(),
            )
            .expect("stderr takes the note");
        Some(n)
    })
}

/// `opts` with the [`test_seed`] put in, where the test names no seed of its own.
#[must_use]
pub fn seeded(opts: &Options) -> Options {
    let mut o = opts.clone();
    if o.seed.is_none() {
        o.seed = test_seed();
    }
    o
}

/// Whether this run builds the tiles on the seeds their snapshots were recorded at (no [`test_seed`]).
#[must_use]
pub fn at_recorded_seeds() -> bool {
    test_seed().is_none()
}

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
    let opts = &seeded(opts);
    let key = format!("{}\0{path}\0{opts:?}\0{}", src.reads(), opts.printer.name);
    let slot = slot(&MEMO, &key);
    let made = *slot.get_or_init(|| {
        let t0 = Instant::now();
        // the memo's one build: every other test build reads it
        let result = dt_eval::build(src, path, opts);
        let secs = t0.elapsed().as_secs_f64();
        if let Ok(b) = &result {
            log_build(path, secs, b);
        }
        Box::leak(Box::new(Made { result, secs }))
    });
    (made.result.as_ref(), made.secs)
}

/// With `DT_TEST_TIMES` set, one line per build to stderr (past the harness's capture): the test that asked first,
/// the path, the seconds and each stage's seconds, to see which tests and tiles the CPU goes to.
pub fn log_build(path: &str, secs: f64, b: &Built) {
    if std::env::var_os("DT_TEST_TIMES").is_none() {
        return;
    }
    use std::io::Write as _;
    let stages: Vec<String> = b
        .report
        .stage_secs
        .iter()
        .map(|(k, v)| format!("{k}={v:.2}"))
        .collect();
    let line = format!(
        "TIMES\t{}\t{path}\t{secs:.2}\t{}\n",
        std::thread::current().name().unwrap_or("?"),
        stages.join(" ")
    );
    std::io::stderr()
        .write_all(line.as_bytes())
        .expect("stderr takes the line");
}
