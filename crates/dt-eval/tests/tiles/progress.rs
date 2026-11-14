//! The progress side channel: the bar a reporter drives never goes back and
//! ends at 100%, elements with a loop report its counts, and a front end that
//! wants sketches gets them as the build goes.  That a reporter changes no
//! build is held by `golden`: the tiles here are built once, with the tracker
//! and the sketching reporter listening (`memo`), against hashes recorded
//! without them.

#![allow(clippy::unwrap_used)]

use super::memo::observed;

/// One tile for each element that reports sub-progress.
const TILES: &[&str] = &[
    "lib:tests/plants/2x2-grass.dtile",
    "lib:tests/walls/1x1-wall-cut-stone.dtile",
    "lib:tests/rocks/2x2-rocks.dtile",
    "lib:tests/plants/1x1-tree.dtile",
];

/// Each tile tracked: progress only climbs, to 100% at the end.
#[test]
fn progress_climbs_to_100_percent() {
    for path in TILES {
        let o = observed(path);
        let (b, v) = (&o.built, &o.status);
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
        assert!(
            b.report.stage_secs.iter().any(|(k, _)| k == "union"),
            "{path}"
        );
        assert!(
            b.report.stage_secs.iter().any(|(k, _)| k == "audit"),
            "{path}"
        );
    }
}

#[test]
fn elements_with_a_loop_report_their_counts() {
    for (path, what) in [
        ("lib:tests/walls/1x1-wall-cut-stone.dtile", "stones"),
        ("lib:tests/plants/2x2-grass.dtile", "blades"),
        ("lib:tests/rocks/2x2-rocks.dtile", "stones"),
    ] {
        let mine: Vec<_> = observed(path).subs.iter().filter(|s| s.0 == what).collect();
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
    }
}

/// A front end that wants sketches gets the cliff's rough boxes, then each
/// stage's parts.
#[test]
fn sketches_come_as_the_build_goes() {
    let v = &observed("lib:tests/cliffs/1x1-cliff-mound.dtile").sketches;
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
