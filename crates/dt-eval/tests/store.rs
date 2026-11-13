//! The disk cache (`dt_eval::store`): a build kept and read back is the same
//! bytes, any input change misses, a damaged entry is reported and made
//! again, the folder stays under its limit and writers do not collide.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use dt_eval::store::{CacheState, Tier};
use dt_eval::{Built, Library, Memory, Options, Progress, Source, Store, build, build_stored};
use dt_kit::debug::Collect;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dt-store-{}-{name}", std::process::id()));
    if d.exists() {
        std::fs::remove_dir_all(&d).unwrap();
    }
    d
}

const FLAT: &str = "lib:tests/ground/1x1-flat.dtile";

fn stored(store: &Store, src: &dyn Source, path: &str, opts: &Options) -> Built {
    build_stored(src, path, opts, &Progress::none(), None, Some(store)).unwrap()
}

/// Everything about a build that anything reads, as text and bytes.
fn same(a: &Built, b: &Built) {
    assert_eq!(a.export, b.export);
    assert_eq!(a.materials, b.materials);
    assert_eq!(a.audit, b.audit);
    assert_eq!(a.stages, b.stages);
    assert_eq!(a.record.document, b.record.document);
    assert_eq!(a.record.document_hash, b.record.document_hash);
    assert_eq!(a.record.engine, b.record.engine);
    assert_eq!(a.out, b.out);
    assert_eq!(a.footprint, b.footprint);
    assert_eq!(a.notes, b.notes);
    assert_eq!(a.left_out, b.left_out);
    assert_eq!(
        a.stage_secs.iter().map(|s| &s.0).collect::<Vec<_>>(),
        b.stage_secs.iter().map(|s| &s.0).collect::<Vec<_>>()
    );
    assert_eq!(a.entries, b.entries);
    assert_eq!(a.pieces, b.pieces);
    assert_eq!(a.brick, b.brick);
    assert_eq!(format!("{:?}", a.stack), format!("{:?}", b.stack));
    assert_eq!(format!("{:?}", a.surface), format!("{:?}", b.surface));
    assert_eq!(format!("{:?}", a.covered), format!("{:?}", b.covered));
    assert_eq!(format!("{:?}", a.clouds), format!("{:?}", b.clouds));
    assert_eq!(format!("{:?}", a.debug), format!("{:?}", b.debug));
    assert_eq!(a.colours(), b.colours());
    assert_eq!(
        dt_kit::stl(&a.export.mesh, &a.record),
        dt_kit::stl(&b.export.mesh, &b.record)
    );
}

#[test]
fn a_hit_is_the_same_bytes_as_a_fresh_build() {
    let dir = temp("hit");
    let store = Store::new(&dir, "test-engine");
    let opts = Options::default();
    let fresh = build(&Library, FLAT, &opts).unwrap();
    let first = stored(&store, &Library, FLAT, &opts);
    assert_eq!(first.cache.state, CacheState::Miss);
    assert!(first.cache.problems.is_empty());
    same(&first, &fresh);
    let second = stored(&store, &Library, FLAT, &opts);
    assert_eq!(second.cache.state, CacheState::Hit);
    assert_eq!(second.cache.made_by.as_deref(), Some("test-engine"));
    same(&second, &fresh);
    assert_eq!(
        second.stage_secs, first.stage_secs,
        "a hit keeps the times it was made in"
    );
    // the engine label of the record is the one asked for, not the entry's
    let relabelled = Options {
        engine: "another-label".to_owned(),
        ..Options::default()
    };
    let third = stored(&store, &Library, FLAT, &relabelled);
    assert_eq!(third.cache.state, CacheState::Hit);
    assert_eq!(third.record.engine, "another-label");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The debug probe: clouds, marks (with their classes), a surface, water.
const PROBE: &str = r#"dharmatiles version="1.0.0"; tile size="1x1" seed=97 {
    line shore wobble="5dmm" { through "(0sq, 0.62sq)" "(1sq, 0.58sq)" }
    region pool { boundedBy shore; fill "(0.5sq, 0.75sq)" }
    region bank { boundedBy shore; fill "(0.5sq, 0.25sq)" }
    region shoreBand { within "7dmm" of=shore }
    stack {
        base
        dirt {
            pin top="[$waterBedHeight .. $groundHeight + 1dmm]" at=shoreBand
            pin top="$waterBedHeight" at=pool
        }
        grass at=bank
        water
    }
}"#;

#[test]
fn clouds_marks_surface_and_stack_come_back_exactly() {
    let dir = temp("probe");
    let store = Store::new(&dir, "test-engine");
    let m = common::memory(&[("probe.dtile", PROBE)]);
    let opts = Options {
        debug: true,
        clouds: true,
        ..Options::default()
    };
    let fresh = build(&m, "probe.dtile", &opts).unwrap();
    assert!(fresh.debug.is_some() && fresh.clouds.is_some() && fresh.surface.is_some());
    let first = stored(&store, &m, "probe.dtile", &opts);
    same(&first, &fresh);
    let again = stored(&store, &m, "probe.dtile", &opts);
    assert_eq!(again.cache.state, CacheState::Hit);
    same(&again, &fresh);
    // what was not asked for is left off, as a fresh build leaves it
    let plain = Options::default();
    let bare = stored(&store, &m, "probe.dtile", &plain);
    assert_eq!(bare.cache.state, CacheState::Hit);
    assert!(bare.clouds.is_none() && bare.debug.is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_entry_without_the_marks_does_not_serve_a_build_that_wants_them() {
    let dir = temp("marks");
    let store = Store::new(&dir, "test-engine");
    let m = common::memory(&[("probe.dtile", PROBE)]);
    let plain = Options::default();
    let first = stored(&store, &m, "probe.dtile", &plain);
    assert_eq!(first.cache.state, CacheState::Miss);
    let marks = Options {
        debug: true,
        ..Options::default()
    };
    let wants = stored(&store, &m, "probe.dtile", &marks);
    assert_eq!(wants.cache.state, CacheState::Miss, "marks were not kept");
    assert!(wants.debug.is_some());
    // the entry now holds them, and serves both
    assert_eq!(
        stored(&store, &m, "probe.dtile", &marks).cache.state,
        CacheState::Hit
    );
    assert_eq!(
        stored(&store, &m, "probe.dtile", &plain).cache.state,
        CacheState::Hit
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A tile and a part it uses, in memory.
fn with_part(part: &str) -> Memory {
    Memory(BTreeMap::from([
        (
            "mem/t.dtile".to_owned(),
            "dharmatiles version=\"1.0.0\"\nuse \"./p\" mine\ntile size=\"1x1\" seed=1 {\n    stack {\n        base\n        mine\n    }\n}\n"
                .to_owned(),
        ),
        ("mem/p.dtile".to_owned(), part.to_owned()),
    ]))
}

const PART: &str = "dharmatiles version=\"1.0.0\"\ndefine mine {\n    stack {\n        dirt top=\"3.4dmm\" clods=none\n    }\n}\n";

fn key_of(store: &Store, src: &dyn Source, path: &str, opts: &Options) -> dt_eval::store::Key {
    let tracked = dt_eval::store::Tracked::new(src);
    dt_eval::check(&tracked, path, opts).unwrap();
    store.key(&tracked.reads(), path, opts)
}

#[test]
fn any_input_change_misses() {
    let dir = temp("keys");
    let store = Store::new(&dir, "test-engine");
    let opts = Options::default();
    let base = key_of(&store, &with_part(PART), "mem/t.dtile", &opts);
    // the same inputs, the same key
    assert_eq!(base, key_of(&store, &with_part(PART), "mem/t.dtile", &opts));
    let differs = |k: dt_eval::store::Key, what: &str| {
        assert_ne!(base.id(), k.id(), "{what} must change the key");
    };
    // a used part's text
    differs(
        key_of(
            &store,
            &with_part(&PART.replace("3.4dmm", "3.6dmm")),
            "mem/t.dtile",
            &opts,
        ),
        "a part's text",
    );
    // the tile's text
    let mut tile_edit = with_part(PART);
    let t = tile_edit.0.get_mut("mem/t.dtile").unwrap();
    *t = t.replace("seed=1", "seed=2");
    differs(key_of(&store, &tile_edit, "mem/t.dtile", &opts), "the tile");
    // the system
    let ol = Options {
        system: "ol".to_owned(),
        ..Options::default()
    };
    differs(
        key_of(&store, &with_part(PART), "mem/t.dtile", &ol),
        "the system",
    );
    // the seed
    let seeded = Options {
        seed: Some(5),
        ..Options::default()
    };
    differs(
        key_of(&store, &with_part(PART), "mem/t.dtile", &seeded),
        "the seed",
    );
    // the printer: another profile, and the same name with other numbers
    let mut fine = Options::default();
    fine.use_printer("fine", None).unwrap();
    differs(
        key_of(&store, &with_part(PART), "mem/t.dtile", &fine),
        "the printer",
    );
    // the tier
    let draft = Options {
        draft: true,
        ..Options::default()
    };
    differs(
        key_of(&store, &with_part(PART), "mem/t.dtile", &draft),
        "the tier",
    );
    // the engine (version and source hash)
    let other = Store::new(&dir, "test-engine-2");
    differs(
        key_of(&other, &with_part(PART), "mem/t.dtile", &opts),
        "the engine",
    );
    // the document's path
    let mut moved = with_part(PART);
    let text = moved.0.remove("mem/t.dtile").unwrap();
    moved.0.insert("mem/u.dtile".to_owned(), text);
    differs(key_of(&store, &moved, "mem/u.dtile", &opts), "the path");
    // what does not change what is built does not change the key
    let label = Options {
        engine: "x".to_owned(),
        clouds: true,
        debug: true,
        ..Options::default()
    };
    assert_eq!(
        base,
        key_of(&store, &with_part(PART), "mem/t.dtile", &label)
    );
    // and the part really is read through the tracker
    let held = with_part(PART);
    let tracked = dt_eval::store::Tracked::new(&held);
    dt_eval::check(&tracked, "mem/t.dtile", &opts).unwrap();
    assert!(format!("{:?}", tracked.reads()).contains("mem/p.dtile"));
    assert_eq!(base.slot().len(), 16);
    assert_eq!(Tier::of(&draft).name(), "draft");
}

#[test]
fn a_changed_part_is_built_again_and_the_old_one_is_still_the_previous() {
    let dir = temp("previous");
    let store = Store::new(&dir, "test-engine");
    let opts = Options::default();
    let a = stored(&store, &with_part(PART), "mem/t.dtile", &opts);
    assert_eq!(a.cache.state, CacheState::Miss);
    let edited = with_part(&PART.replace("3.4dmm", "3.6dmm"));
    let key = key_of(&store, &edited, "mem/t.dtile", &opts);
    assert!(!store.contains(&key));
    let old = store
        .previous(&key, Collect::default())
        .unwrap()
        .expect("the first build is the same slot's");
    same(&old.built, &a);
    assert_eq!(old.made_by, "test-engine");
    let b = stored(&store, &edited, "mem/t.dtile", &opts);
    assert_eq!(b.cache.state, CacheState::Miss);
    assert!(store.contains(&key));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_damaged_entry_is_reported_removed_and_built_again() {
    let dir = temp("damaged");
    let store = Store::new(&dir, "test-engine");
    let opts = Options::default();
    let fresh = build(&Library, FLAT, &opts).unwrap();
    stored(&store, &Library, FLAT, &opts);
    let list = store.list().unwrap();
    assert_eq!(list.len(), 1);
    let file = &list[0].file;
    // flip a byte in the middle of the body
    let mut bytes = std::fs::read(file).unwrap();
    let at = bytes.len() / 2;
    bytes[at] ^= 0x55;
    std::fs::write(file, &bytes).unwrap();
    let again = stored(&store, &Library, FLAT, &opts);
    assert_eq!(again.cache.state, CacheState::Miss);
    assert_eq!(again.cache.problems.len(), 1, "{:?}", again.cache.problems);
    assert!(
        again.cache.problems[0].contains("damaged"),
        "{:?}",
        again.cache.problems
    );
    // the build that replaced it is real
    same(&again, &fresh);
    // and the replacement is whole
    let third = stored(&store, &Library, FLAT, &opts);
    assert_eq!(third.cache.state, CacheState::Hit);
    assert!(third.cache.problems.is_empty());
    same(&third, &fresh);
    // a truncated entry
    let bytes = std::fs::read(file).unwrap();
    std::fs::write(file, &bytes[..bytes.len() / 3]).unwrap();
    let fourth = stored(&store, &Library, FLAT, &opts);
    assert_eq!(fourth.cache.problems.len(), 1);
    same(&fourth, &fresh);
    // one that is not an entry at all
    std::fs::write(file, b"not a cache entry").unwrap();
    let fifth = stored(&store, &Library, FLAT, &opts);
    assert_eq!(fifth.cache.problems.len(), 1);
    same(&fifth, &fresh);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_cache_that_cannot_be_written_is_told_and_the_build_stands() {
    let dir = temp("unwritable");
    // the cache folder is a file: nothing can be made under it
    std::fs::write(&dir, b"in the way").unwrap();
    let store = Store::new(&dir, "test-engine");
    let opts = Options::default();
    let fresh = build(&Library, FLAT, &opts).unwrap();
    let b = stored(&store, &Library, FLAT, &opts);
    assert_eq!(b.cache.state, CacheState::Miss);
    assert_eq!(b.cache.problems.len(), 2, "{:?}", b.cache.problems);
    assert!(
        b.cache.problems[0].contains("cannot"),
        "{:?}",
        b.cache.problems
    );
    same(&b, &fresh);
    std::fs::remove_file(&dir).unwrap();
}

#[test]
fn the_folder_stays_under_its_limit_and_the_least_recently_used_go_first() {
    let dir = temp("evict");
    let opts = Options::default();
    // measure one entry
    let probe = Store::new(&dir, "e");
    stored(&probe, &Library, FLAT, &opts);
    let one = probe.stats().unwrap().bytes;
    assert!(one > 10_000, "an entry has a mesh: {one}");
    probe.clear().unwrap();
    // room for two and a half
    let store = Store::new(&dir, "e").with_limit(one * 5 / 2);
    let seeds = [1_u64, 2, 3, 4];
    let mut keys = Vec::new();
    for s in seeds {
        let o = Options {
            seed: Some(s),
            ..Options::default()
        };
        stored(&store, &Library, FLAT, &o);
        keys.push(key_of(&store, &Library, FLAT, &o));
        let st = store.stats().unwrap();
        assert!(
            st.bytes <= store.limit(),
            "{} > {}",
            st.bytes,
            store.limit()
        );
        // a moment apart so the order of use is plain on any file system
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    // the newest are kept, the oldest gone
    assert!(!store.contains(&keys[0]) && !store.contains(&keys[1]));
    assert!(store.contains(&keys[2]) && store.contains(&keys[3]));
    assert_eq!(store.stats().unwrap().entries, 2);
    // reading an entry is a use: it outlives a newer one that was not read
    let o2 = Options {
        seed: Some(3),
        ..Options::default()
    };
    assert_eq!(
        stored(&store, &Library, FLAT, &o2).cache.state,
        CacheState::Hit
    );
    std::thread::sleep(std::time::Duration::from_millis(30));
    let o5 = Options {
        seed: Some(5),
        ..Options::default()
    };
    stored(&store, &Library, FLAT, &o5);
    assert!(store.contains(&keys[2]), "the one just read stays");
    assert!(!store.contains(&keys[3]), "the one not read goes");
    // a clear empties it
    let (n, bytes) = store.clear().unwrap();
    assert_eq!(n, 2);
    assert!(bytes > 0);
    assert_eq!(store.stats().unwrap().entries, 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn writers_in_a_crowd_leave_whole_entries() {
    let dir = temp("crowd");
    let store = Store::new(&dir, "test-engine");
    let opts = Options::default();
    let fresh = build(&Library, FLAT, &opts).unwrap();
    // eight threads each build and keep the same key, and read it back
    let built: Vec<Built> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..8)
            .map(|_| s.spawn(|| stored(&store, &Library, FLAT, &opts)))
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for b in &built {
        assert!(b.cache.problems.is_empty(), "{:?}", b.cache.problems);
        same(b, &fresh);
    }
    let after = stored(&store, &Library, FLAT, &opts);
    assert_eq!(after.cache.state, CacheState::Hit);
    same(&after, &fresh);
    let st = store.stats().unwrap();
    assert_eq!(st.entries, 1, "one key, one entry");
    assert_eq!(st.strays, 0);
    // no temporary is left in the slot's folder
    let slot = std::fs::read_dir(dir.join(format!("v{}", dt_eval::store::FORMAT)))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    for e in std::fs::read_dir(slot.path()).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().into_owned();
        assert!(name.ends_with(".dtb"), "{name}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_draft_tier_has_its_own_entries() {
    let dir = temp("tiers");
    let store = Store::new(&dir, "test-engine");
    let full = Options::default();
    let draft = Options {
        draft: true,
        ..Options::default()
    };
    let d = stored(&store, &Library, FLAT, &draft);
    assert_eq!(d.cache.state, CacheState::Miss);
    // a full build is not answered with a preview
    let f = stored(&store, &Library, FLAT, &full);
    assert_eq!(f.cache.state, CacheState::Miss);
    assert_ne!(
        f.export.mesh.content_hash(),
        d.export.mesh.content_hash(),
        "the draft is a different mesh"
    );
    assert_eq!(
        stored(&store, &Library, FLAT, &draft).cache.state,
        CacheState::Hit
    );
    assert_eq!(
        stored(&store, &Library, FLAT, &full).cache.state,
        CacheState::Hit
    );
    assert_eq!(store.stats().unwrap().entries, 2);
    std::fs::remove_dir_all(&dir).unwrap();
}
