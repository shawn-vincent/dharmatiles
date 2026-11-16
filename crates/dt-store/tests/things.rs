//! The things a tile makes are kept by their own key, on the same store as the builds: a thing built by one build is read by
//! the next, through the disk's `Store` and the browser's `MemStore` alike, and what is read is the bytes of what was
//! built.  A key differs when anything the body is made from, the engine, the build target or the tier differs.

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failing unwrap is the failure"
)]

mod common;

use std::path::PathBuf;

use dt_eval::{Built, Options, Progress, Source};
use dt_kit::ThingId;
use dt_store::{Kept, MemStore, Store, Tier, build_stored, make_thing_key};

/// Pebbles of four variants (each a body, copied), pebbles that are each their own, and a rock between: every thing that is
/// kept as a body.  `about` is the edit that touches no thing: the tile's text, and so its key, changes; its things do not.
fn tile(about: &str) -> String {
    format!(
        "dharmatiles version=\"1.0.0\"\n\
        tile size=\"1x1\" seed=7 {{\n\
            about \"{about}\"\n\
            stack {{\n\
                base\n\
                dirt\n\
                scatter perSquare=30 variants=4 {{ pebble size=\"1.5dmm .. 3dmm\" }}\n\
                scatter perSquare=5 {{ pebble size=\"2dmm .. 4dmm\" }}\n\
                rock at=\"(0.5sq, 0.5sq)\" size=\"8dmm\"\n\
            }}\n\
        }}\n"
    )
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dt-things-{}-{name}", std::process::id()));
    if d.exists() {
        std::fs::remove_dir_all(&d).unwrap();
    }
    d
}

fn build(src: &dyn Source, store: &dyn Kept) -> Built {
    build_stored(
        src,
        "t.dtile",
        &Options::default(),
        &Progress::none(),
        None,
        Some(store),
    )
    .unwrap_or_else(|e| panic!("{e:#?}"))
}

/// What a build did for its things, as three numbers.
fn used(b: &Built) -> (u32, u32, u32) {
    let u = b.report.things;
    (u.built, u.shared, u.kept)
}

/// The same checks through either store: the first build makes its things, an edit that touches none of them
/// rebuilds the tile and reads every one, and what it reads is the bytes it would have made.
fn kept_across_builds(store: &dyn Kept) {
    let first = build(&common::memory(&[("t.dtile", &tile("one"))]), store);
    let (built, shared, kept) = used(&first);
    assert!(
        built > 0 && kept == 0,
        "the first build makes: {built} {kept}"
    );
    // a variant is built once and copied: more stones than variants
    assert!(shared > 0, "a variant is copied: {shared}");

    let src = common::memory(&[("t.dtile", &tile("two"))]);
    let second = build(&src, store);
    assert_eq!(
        second.report.cache.state,
        dt_eval::CacheState::Miss,
        "the text changed, so the tile is built again"
    );
    let (built2, shared2, kept2) = used(&second);
    assert_eq!(built2, 0, "nothing the edit touched is built again");
    assert_eq!(
        kept2, built,
        "every body the first build made, one for each, is read"
    );
    assert_eq!(shared2, shared);

    // the bodies read are the bodies made: the same mesh as a build with nothing kept
    let fresh = dt_eval::build_cached(
        &src,
        "t.dtile",
        &Options::default(),
        &Progress::none(),
        None,
    )
    .unwrap();
    assert_eq!(second.export, fresh.export);
    assert_eq!(second.materials, fresh.materials);
    assert_eq!(used(&fresh), (built, shared, 0));
}

#[test]
fn a_thing_kept_by_one_build_is_read_by_the_next_through_the_memory_store() {
    let mem = MemStore::new("test-engine", 256 << 20);
    kept_across_builds(&mem);
    // the tiles' two entries and each body's
    assert!(mem.len() > 2);
}

#[test]
fn a_thing_kept_by_one_build_is_read_by_the_next_through_the_disk_store() {
    let dir = temp("across");
    let store = Store::new(&dir, "test-engine");
    kept_across_builds(&store);
    assert!(store.list().unwrap().len() > 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A thing built for one target is not read for another: the target is in its key.
#[test]
fn another_target_does_not_read_a_thing_built_for_this_one() {
    let mem = MemStore::new("test-engine", 256 << 20);
    let src = common::memory(&[("t.dtile", &tile("one"))]);
    let ol = Options {
        system: dt_lang::System::Ol,
        ..Options::default()
    };
    let at = |o: &Options| {
        build_stored(&src, "t.dtile", o, &Progress::none(), None, Some(&mem))
            .unwrap_or_else(|e| panic!("{e:#?}"))
    };
    let db = at(&Options::default());
    let other = at(&ol);
    assert_eq!(
        used(&other).2,
        0,
        "nothing of DungeonBlocks is read for OpenLOCK"
    );
    assert_eq!(used(&other).0, used(&db).0);
}

fn id(kind: &'static str, n: f64) -> ThingId {
    ThingId::new(kind, |c| {
        c.put(&n);
    })
}

/// The key of a body is the same for the same thing, and differs for the definition (its kind, and each number its builder
/// reads, which a control or a seed is), the engine, the build target and the tier.
#[test]
fn a_things_key_differs_by_what_it_is_made_from_the_engine_the_target_and_the_tier() {
    let opts = Options::default();
    let key = |engine: &str, id: &ThingId, o: &Options, tier: Tier| {
        make_thing_key(engine, id, o, tier).id().to_owned()
    };
    let base = key("e", &id("rock", 1.0), &opts, Tier::Full);
    assert_eq!(base, key("e", &id("rock", 1.0), &opts, Tier::Full));
    // the definition: its kind, a number it reads (a control, a seed)
    assert_ne!(base, key("e", &id("flower", 1.0), &opts, Tier::Full));
    assert_ne!(base, key("e", &id("rock", 1.5), &opts, Tier::Full));
    let two = ThingId::new("rock", |c| {
        c.put(&1.0_f64).put(&2_u64);
    });
    assert_ne!(base, key("e", &two, &opts, Tier::Full));
    // the engine, so that changed code cannot serve an old body
    assert_ne!(base, key("f", &id("rock", 1.0), &opts, Tier::Full));
    // the target: the system, and the printer's name and numbers
    let ol = Options {
        system: dt_lang::System::Ol,
        ..Options::default()
    };
    assert_ne!(base, key("e", &id("rock", 1.0), &ol, Tier::Full));
    let mut other = Options::default();
    other.printer.name = "fine".to_owned();
    assert_ne!(base, key("e", &id("rock", 1.0), &other, Tier::Full));
    // the tier
    assert_ne!(base, key("e", &id("rock", 1.0), &opts, Tier::Low));
    // a tile's own key is another key: a body never answers a build
    assert_ne!(
        base,
        dt_store::make_key(
            "e",
            &dt_store::Reads::default(),
            "thing:rock",
            &opts,
            Tier::Full
        )
        .id()
    );
}
