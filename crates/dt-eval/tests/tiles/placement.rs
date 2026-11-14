//! Where things are placed, as coordinates.
//!
//! A tile's seed and regions decide where its rocks, trees, flowers and
//! clearings go (`Built::placed`).  The tiers of a build (full, low) must
//! agree on every one of them to the last bit, so a map that swaps one for
//! the other never moves a rock.  They do because a low build keeps the
//! full build's lattice: every decision that reads the lattice (an area's
//! mask, the ground a tree must keep clear of, the flow grass runs along)
//! reads the same one.
//!
//! The lattice itself is not a free knob: `Options::cells` moves what is
//! placed, and [`placement_by_cells`] measures how far (ignored; it prints).

#![allow(clippy::unwrap_used, clippy::print_stderr)]

use dt_eval::{Built, Options};

use super::memo::{built, made};

/// The test tiles that place something, one square each or a little more,
/// so that the sweep stays short.
const PLACING: &[&str] = &[
    "lib:tests/rocks/1x1-rock-tops.dtile",
    "lib:tests/plants/1x1-tree.dtile",
    "lib:tests/plants/1x1-grass-flowers.dtile",
    "lib:tests/plants/2x2-meadow.dtile",
    "lib:tests/water/1x1-water+grass.dtile",
    "lib:tests/cliffs/1x1-cliff-mound.dtile",
];

/// The tile built with `f` applied to the default options, once per `key`
/// (the observed build serves the default ones).
fn with(path: &str, key: &str, f: impl FnOnce(&mut Options)) -> &'static Built {
    let mut opts = Options::default();
    f(&mut opts);
    made(path, &opts, key)
}

#[test]
fn a_low_build_places_everything_where_the_full_build_does() {
    let mut anything = 0;
    for path in PLACING {
        let full = built(path);
        let low = with(path, "low", |o| o.low = true);
        let of = |b: &Built, grass: bool| -> Vec<dt_kit::Placement> {
            b.report
                .placed
                .iter()
                .copied()
                .filter(|p| (p.kind == dt_kit::Kind::Grass) == grass)
                .collect()
        };
        // rocks, trees, flowers: to the last bit
        assert_eq!(
            of(full, false),
            of(low, false),
            "{path}: the low build moved something"
        );
        // a grass clearing is placed round the footprints of the rocks and
        // trunks as drawn, and a low rock is its hull: the same clearings, a
        // little apart
        let (a, b) = (of(full, true), of(low, true));
        assert_eq!(a.len(), b.len(), "{path}: clearings");
        for (p, q) in a.iter().zip(&b) {
            let d = (p.at[0] - q.at[0]).hypot(p.at[1] - q.at[1]);
            assert!(d < 3.0, "{path}: a clearing moved {d:.2} mm");
        }
        anything += full.report.placed.len();
    }
    assert!(anything > 20, "only {anything} placements in the sweep");
}

#[test]
fn placements_are_recorded_for_every_placing_element() {
    let kinds = |path: &str| -> Vec<&'static str> {
        let mut k: Vec<&'static str> = built(path)
            .report
            .placed
            .iter()
            .map(|p| p.kind.name())
            .collect();
        k.sort_unstable();
        k.dedup();
        k
    };
    assert!(kinds("lib:tests/rocks/1x1-rock-tops.dtile").contains(&"rock"));
    assert!(kinds("lib:tests/plants/1x1-tree.dtile").contains(&"tree"));
    assert!(kinds("lib:tests/plants/2x2-meadow.dtile").contains(&"grass"));
    assert!(kinds("lib:tests/plants/1x1-grass-flowers.dtile").contains(&"flowers"));
}

/// What a coarser lattice leaves where it is: the rocks and flowers a tile
/// scatters over its whole area stand at the same coordinates at 32 cells
/// (the coarsest, and the quickest build; 48 and 128 are in
/// [`rocks_and_flowers_stand_where_they_did_at_more_cells`]).  (Trees do
/// not: see [`placement_by_cells`].)
fn stand_where_they_did(cells: &[u32]) {
    for path in [
        "lib:tests/rocks/1x1-rock-tops.dtile",
        "lib:tests/plants/1x1-grass-flowers.dtile",
    ] {
        let full = built(path);
        assert!(!full.report.placed.is_empty(), "{path}");
        for &cells in cells {
            let other = with(path, &format!("cells {cells}"), |o| o.cells = Some(cells));
            assert_eq!(
                full.report.placed, other.report.placed,
                "{path} at {cells} cells"
            );
        }
    }
}

#[test]
fn rocks_and_flowers_stand_where_they_did_at_other_cells() {
    stand_where_they_did(&[32]);
}

#[test]
#[ignore = "the slow sweep: the same as the fast test at 48 and 128 cells (one representative, 32, runs by default)"]
fn rocks_and_flowers_stand_where_they_did_at_more_cells() {
    stand_where_they_did(&[48, 128]);
}

/// How far each placement of a build at `cells` is from the full build's,
/// mm (infinite for one with no match: a count that changed).
fn drift(full: &Built, other: &Built) -> (usize, usize, f64) {
    let (a, b) = (&full.report.placed, &other.report.placed);
    let mut worst: f64 = 0.0;
    for p in a {
        let near = b
            .iter()
            .filter(|q| q.kind == p.kind)
            .map(|q| (q.at[0] - p.at[0]).hypot(q.at[1] - p.at[1]))
            .fold(f64::INFINITY, f64::min);
        worst = worst.max(near);
    }
    (a.len(), b.len(), worst)
}

/// Not a check: what placement does when the lattice is made coarser, which
/// is why the tiers keep the full one.  `cargo test --release -p dt-eval
/// --test tiles placement_by_cells -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement: prints how far each placement moves with cells per square"]
fn placement_by_cells() {
    for path in PLACING {
        let full = built(path);
        for cells in [32, 48, 128] {
            let other = with(path, &format!("cells {cells}"), |o| o.cells = Some(cells));
            let (n, m, worst) = drift(full, other);
            eprintln!(
                "{path}: at {cells} cells {m} placed against {n}, the farthest moved {worst:.2} mm"
            );
        }
    }
}
