//! The build cache (`dt_eval::Cache`): a build that starts from the scene
//! the last build kept is the same, hash for hash, as one from nothing,
//! and an edit rebuilds only the steps from the first one it changes.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use dt_eval::{Built, Cache, Library, Memory, Options, Progress, Source, build, build_cached};

fn fingerprint(b: &Built) -> (Vec<(String, u64)>, u64) {
    (
        b.stages.iter().map(|s| (s.name.clone(), s.hash)).collect(),
        b.export.mesh.content_hash(),
    )
}

/// `tile` as written, then with `from` replaced by `to`: each built with
/// the cache and from nothing.  Returns the steps the edit reused.
fn edit(tile: &str, from: &str, to: &str, opts: &Options) -> usize {
    let text = Library.read(tile).unwrap();
    assert!(text.contains(from), "{tile} has no `{from}`");
    let path = "mem/tile.dtile";
    let src = |t: &str| Memory(BTreeMap::from([(path.to_owned(), t.to_owned())]));
    let cache = Cache::new();
    let first = build_cached(&src(&text), path, opts, &Progress::none(), Some(&cache)).unwrap();
    assert_eq!(first.reused, 0, "an empty cache gives nothing");
    assert!(!cache.is_empty());
    let steps = cache.len();
    // the same text again: every step comes from the cache
    let again = build_cached(&src(&text), path, opts, &Progress::none(), Some(&cache)).unwrap();
    assert_eq!(again.reused, steps);
    assert_eq!(fingerprint(&again), fingerprint(&first));
    let edited = text.replace(from, to);
    let cached = build_cached(&src(&edited), path, opts, &Progress::none(), Some(&cache)).unwrap();
    let plain = build(&src(&edited), path, opts).unwrap();
    assert_eq!(
        fingerprint(&cached),
        fingerprint(&plain),
        "{tile}: `{to}` from the cache differs from a fresh build"
    );
    assert_eq!(cache.len(), steps);
    cached.reused
}

/// Water, banks and two scatters: changing the scatter in the water keeps
/// the painted and lifted ground.
#[test]
fn an_edit_late_in_the_plan_reuses_the_steps_before_it() {
    let reused = edit(
        "lib:tiles/test/2x2-shore.dtile",
        "count=3",
        "count=4",
        &Options::default(),
    );
    assert!(reused >= 3, "only {reused} steps reused");
}

/// Grass keeps the ground; the dirt's level is the first step, so nothing
/// can be reused.
#[test]
fn an_edit_reuses_only_what_comes_before_it() {
    let reused = edit(
        "lib:tiles/test/2x2-shore.dtile",
        "bare=\"15%\"",
        "bare=\"25%\"",
        &Options::default(),
    );
    assert!(reused >= 3, "only {reused} steps reused");
    let reused = edit(
        "lib:tiles/test/2x2-shore.dtile",
        "level=edge",
        "level=\"edge + 0.5\"",
        &Options::default(),
    );
    assert_eq!(reused, 0);
}

/// A cliff draft: the boulders change, the cliff is not carved again.
#[test]
fn a_cliff_is_not_carved_again_for_its_boulders() {
    let opts = Options {
        draft: true,
        ..Options::default()
    };
    let reused = edit(
        "lib:tiles/cliffs/1x1-cliff.dtile",
        "per-square=2.5",
        "per-square=3.5",
        &opts,
    );
    assert!(reused >= 3, "only {reused} steps reused");
}
