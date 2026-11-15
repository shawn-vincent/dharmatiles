//! The memory store the browser keeps builds in (`dt_store::MemStore`): its watcher hears every change, what it told loads
//! back into a fresh store (a later visit), another engine's entries are dropped, and the limit with its least recently used
//! eviction is the store's alone.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failing unwrap is the failure"
)]

use std::sync::{Arc, Mutex};

use dt_eval::CacheState;
use dt_eval::{Library, Options, Progress};
use dt_store::{Change, MemStore, build_stored};

const FLAT: &str = "lib:tests/ground/1x1-flat.dtile";

/// What a watcher heard: (id, bytes, used) for a put, (id, empty, 0) for a drop.
type Heard = Arc<Mutex<Vec<(String, Vec<u8>, u64, bool)>>>;

fn heard_by(store: &MemStore) -> Heard {
    let heard: Heard = Arc::default();
    let h = heard.clone();
    store.watch(Box::new(move |c| {
        h.lock().unwrap().push(match c {
            Change::Put { id, bytes, used } => (id.to_owned(), bytes.to_vec(), used, true),
            Change::Drop { id } => (id.to_owned(), Vec::new(), 0, false),
        });
    }));
    heard
}

fn build(store: &MemStore, opts: &Options) -> CacheState {
    build_stored(&Library, FLAT, opts, &Progress::none(), None, Some(store))
        .unwrap()
        .report
        .cache
        .state
}

#[test]
fn what_the_watcher_heard_loads_into_a_later_visit_and_is_read_there() {
    let first = MemStore::new("engine-1", 256 << 20);
    let heard = heard_by(&first);
    let opts = Options::default();
    assert_eq!(build(&first, &opts), CacheState::Miss);
    assert_eq!(build(&first, &opts), CacheState::Hit);
    let puts: Vec<(Vec<u8>, u64)> = heard
        .lock()
        .unwrap()
        .iter()
        .filter(|h| h.3)
        .map(|h| (h.1.clone(), h.2))
        .collect();
    assert_eq!(puts.len(), 1, "one build kept: the second read it");

    // a later visit: a fresh store takes the entries back and reads the build, with no build made
    let later = MemStore::new("engine-1", 256 << 20);
    assert!(later.load(puts.clone()).is_empty());
    assert_eq!(later.len(), 1);
    assert_eq!(build(&later, &opts), CacheState::Hit);

    // another engine's entry is dropped on load, never read, and the keeper is told
    let other = MemStore::new("engine-2", 256 << 20);
    let told = heard_by(&other);
    assert!(other.load(puts).is_empty());
    assert!(other.is_empty());
    let told = told.lock().unwrap();
    assert_eq!(told.len(), 1);
    assert!(!told[0].3, "the entry of another engine is told dropped");
    drop(told);
    assert_eq!(build(&other, &opts), CacheState::Miss);
}

#[test]
fn a_damaged_entry_is_said_and_never_read() {
    let store = MemStore::new("engine-1", 256 << 20);
    let heard = heard_by(&store);
    build(&store, &Options::default());
    let (bytes, used) = {
        let h = heard.lock().unwrap();
        (h[0].1.clone(), h[0].2)
    };
    let mut bad = bytes;
    let mid = bad.len() / 2;
    bad[mid] ^= 0xff;
    let later = MemStore::new("engine-1", 256 << 20);
    let problems = later.load(vec![(bad, used)]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("damaged"), "{problems:?}");
    assert!(later.is_empty());
}

#[test]
fn the_limit_and_the_order_of_eviction_are_the_stores() {
    // one entry's size, from a store that keeps it
    let probe = MemStore::new("e", usize::MAX);
    let heard = heard_by(&probe);
    let a = Options::default();
    build(&probe, &a);
    let one = heard.lock().unwrap()[0].1.len();

    // room for two: the third build (another seed, another key) pushes out the least recently used
    let store = MemStore::new("e", one * 2 + one / 2);
    let told = heard_by(&store);
    let seeded = |seed| Options {
        seed: Some(seed),
        ..Options::default()
    };
    build(&store, &seeded(1));
    build(&store, &seeded(2));
    assert_eq!(
        build(&store, &seeded(1)),
        CacheState::Hit,
        "seed 1 is the newest use now"
    );
    build(&store, &seeded(3));
    assert_eq!(store.len(), 2);
    let drops = told.lock().unwrap().iter().filter(|h| !h.3).count();
    assert_eq!(drops, 1, "one entry went, and was told");
    assert_eq!(
        build(&store, &seeded(1)),
        CacheState::Hit,
        "the one used last stayed"
    );
    assert_eq!(
        build(&store, &seeded(2)),
        CacheState::Miss,
        "the least recently used went"
    );

    // a smaller limit on load keeps the newest
    let entries: Vec<(Vec<u8>, u64)> = told
        .lock()
        .unwrap()
        .iter()
        .filter(|h| h.3)
        .map(|h| (h.1.clone(), h.2))
        .collect();
    let tight = MemStore::new("e", one + one / 2);
    assert!(tight.load(entries).is_empty());
    assert_eq!(tight.len(), 1);
}
