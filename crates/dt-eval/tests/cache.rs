//! The build cache (`dt_eval::Cache`): a build that starts from the scene
//! the last build kept is the same, hash for hash, as one from nothing,
//! and an edit rebuilds only the steps from the first one it changes.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use dt_eval::{Built, Cache, Library, Memory, Options, Progress, Source, build, build_cached};

fn fingerprint(b: &Built) -> (Vec<(String, u64)>, u64) {
    (
        b.report
            .stages
            .iter()
            .map(|s| (s.name.clone(), s.hash))
            .collect(),
        b.export.mesh.content_hash(),
    )
}

/// Drafts: the cache keeps a draft's steps as it keeps a full build's, and
/// a draft is a fraction of the time.
fn draft() -> Options {
    Options {
        draft: true,
        ..Options::default()
    }
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
    assert_eq!(first.report.reused, 0, "an empty cache gives nothing");
    assert!(!cache.is_empty());
    let steps = cache.len();
    // the same text again: every step comes from the cache
    let again = build_cached(&src(&text), path, opts, &Progress::none(), Some(&cache)).unwrap();
    assert_eq!(again.report.reused, steps);
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
    cached.report.reused
}

/// Water, banks and two scatters: changing the scatter in the water keeps
/// the painted and lifted ground.
#[test]
fn an_edit_late_in_the_plan_reuses_the_steps_before_it() {
    let reused = edit(
        "lib:tests/water/2x2-shore.dtile",
        "count=3",
        "count=4",
        &draft(),
    );
    assert!(reused >= 2, "only {reused} steps reused");
}

/// The stack is built in the order it is listed: the grass comes after the
/// dirt and the land's rocks, and keeps both; the dirt's level is the first
/// step, so nothing can be reused.
#[test]
fn an_edit_reuses_only_what_comes_before_it() {
    let reused = edit(
        "lib:tests/water/2x2-shore.dtile",
        "bare=\"15%\"",
        "bare=\"25%\"",
        &draft(),
    );
    assert!(reused >= 2, "only {reused} steps reused");
    let reused = edit(
        "lib:tests/water/2x2-shore.dtile",
        "top=\"$dirtEdgeHeight\"",
        "top=\"$dirtEdgeHeight + 0.5dmm\"",
        &draft(),
    );
    assert_eq!(reused, 0);
}

/// A cliff draft: the boulders change, the cliff is not carved again.
#[test]
fn a_cliff_is_not_carved_again_for_its_boulders() {
    let opts = draft();
    let reused = edit(
        "lib:tests/cliffs/1x1-cliff-columns.dtile",
        "perSquare=2.5",
        "perSquare=3.5",
        &opts,
    );
    assert!(reused >= 3, "only {reused} steps reused");
}

/// A tree whose top reads the region it is scattered in: editing that
/// region rebuilds the tree, never serving the one the cache kept.  (A
/// step whose formula reads regions keys on every region's shape, so this
/// holds however far up `parent()` reaches.)
#[test]
fn editing_a_region_a_trees_top_reads_rebuilds_the_tree() {
    let tile = |r: &str| {
        format!(
            "dharmatiles version=\"1.0.0\"\n\
             tile size=\"2x2\" seed=4 {{\n\
             \x20   region clearing {{ within \"{r}\" of=center }}\n\
             \x20   stack {{\n\
             \x20       base\n\
             \x20       dirt clods=none\n\
             \x20       scatter at=clearing count=2 gap=\"4dmm\" {{\n\
             \x20           tree top=\"$bottom + 55dmm + 12dmm * d(parent(parent($region)), %)\"\n\
             \x20       }}\n\
             \x20   }}\n\
             }}\n"
        )
    };
    let path = "mem/tile.dtile";
    let src = |t: &str| Memory(BTreeMap::from([(path.to_owned(), t.to_owned())]));
    let opts = draft();
    let cache = Cache::new();
    let before = tile("22dmm");
    let after = tile("30dmm");
    build_cached(&src(&before), path, &opts, &Progress::none(), Some(&cache)).unwrap();
    let cached = build_cached(&src(&after), path, &opts, &Progress::none(), Some(&cache)).unwrap();
    let plain = build(&src(&after), path, &opts).unwrap();
    let old = build(&src(&before), path, &opts).unwrap();
    assert_ne!(
        old.export.mesh.content_hash(),
        plain.export.mesh.content_hash(),
        "the edit must change the trees, or this test proves nothing"
    );
    assert_eq!(
        fingerprint(&cached),
        fingerprint(&plain),
        "the cache served trees built for the old region"
    );
}

/// The same for rocks: a scattered rock whose top reads the region it
/// stands in is rebuilt when that region is edited.
#[test]
fn editing_a_region_a_rocks_top_reads_rebuilds_the_rock() {
    let tile = |r: &str| {
        format!(
            "dharmatiles version=\"1.0.0\"\n\
             tile size=\"1x1\" seed=4 {{\n\
             \x20   region patch {{ within \"{r}\" of=center }}\n\
             \x20   stack {{\n\
             \x20       base\n\
             \x20       dirt clods=none\n\
             \x20       scatter at=patch count=4 {{\n\
             \x20           rock size=\"5dmm\" top=\"$bottom + 3dmm + 6dmm * d(parent(parent($region)), %)\"\n\
             \x20       }}\n\
             \x20   }}\n\
             }}\n"
        )
    };
    let path = "mem/tile.dtile";
    let src = |t: &str| Memory(BTreeMap::from([(path.to_owned(), t.to_owned())]));
    let opts = draft();
    let cache = Cache::new();
    let before = tile("10dmm");
    let after = tile("14dmm");
    build_cached(&src(&before), path, &opts, &Progress::none(), Some(&cache)).unwrap();
    let cached = build_cached(&src(&after), path, &opts, &Progress::none(), Some(&cache)).unwrap();
    let plain = build(&src(&after), path, &opts).unwrap();
    let old = build(&src(&before), path, &opts).unwrap();
    assert_ne!(
        old.export.mesh.content_hash(),
        plain.export.mesh.content_hash(),
        "the edit must change the rocks, or this test proves nothing"
    );
    assert_eq!(
        fingerprint(&cached),
        fingerprint(&plain),
        "the cache served rocks built for the old region"
    );
}

/// What a build collects for the editor (clouds, the debug layer's marks) is
/// not part of what a scene is keyed on: asking for the layer after a build
/// without it gets it (rebuilt, once), and a build without it after one with
/// it keeps every step.
#[test]
fn the_debug_layer_is_not_in_the_key_but_a_scene_without_it_is_not_given_to_a_build_that_asks() {
    let path = "mem/tile.dtile";
    let text = Library.read("lib:tests/plants/2x2-grass.dtile").unwrap();
    let src = Memory(BTreeMap::from([(path.to_owned(), text)]));
    let plain = Options {
        draft: true,
        ..Options::default()
    };
    let layer = Options {
        debug: true,
        clouds: true,
        ..plain.clone()
    };
    let cache = Cache::new();
    let none = build_cached(&src, path, &plain, &Progress::none(), Some(&cache)).unwrap();
    assert!(none.report.debug.is_none() && none.report.clouds.is_none());
    let steps = cache.len();
    // the same build again: every step kept
    let again = build_cached(&src, path, &plain, &Progress::none(), Some(&cache)).unwrap();
    assert_eq!(again.report.reused, steps);
    // asking for the layer: the scenes kept have none, so none is given, and the marks are there
    let with = build_cached(&src, path, &layer, &Progress::none(), Some(&cache)).unwrap();
    assert_eq!(
        with.report.reused, 0,
        "a scene without the marks cannot give them"
    );
    assert!(
        with.report
            .debug
            .as_ref()
            .is_some_and(|d| !d.dots.is_empty())
    );
    assert_eq!(
        fingerprint(&with),
        fingerprint(&none),
        "nothing built differs"
    );
    // the layer off again: the scenes that have it serve every step
    let off = build_cached(&src, path, &plain, &Progress::none(), Some(&cache)).unwrap();
    assert_eq!(
        off.report.reused, steps,
        "toggling the layer off rebuilds nothing"
    );
    assert!(off.report.debug.is_none() && off.report.clouds.is_none());
    // and on again, from the same scenes
    let on = build_cached(&src, path, &layer, &Progress::none(), Some(&cache)).unwrap();
    assert_eq!(on.report.reused, steps, "the scenes still have the marks");
    assert!(on.report.debug.as_ref().is_some_and(|d| !d.dots.is_empty()));
}
