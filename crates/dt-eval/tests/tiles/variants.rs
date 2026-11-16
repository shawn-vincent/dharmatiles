//! A scatter's `variants=K`: each placement draws one of K variants, each variant is built once and copied, and what a
//! build says it built is in its record.  Unset, every placement is its own draw and its own build.
//!
//! A variant of pebbles is one stone, or (a quarter of the time) a cluster of two to four, so K variants are K to 4K
//! stones; every stone that stands is a body made or a body copied, and the stones' footprints say which shapes there
//! are: the radius of each (a copy of a stone is the same radius to the bit).

#![expect(
    clippy::unwrap_used,
    reason = "a test: a failing unwrap is the failure"
)]

use std::collections::{BTreeMap, BTreeSet};

use dt_eval::{Built, Memory, Options};
use dt_kit::Kind;

use super::memo;

/// A bed of pebbles on dirt, nothing else, with the scatter's own words after `scatter`.
fn pebbles(words: &str) -> &'static Built {
    let text = format!(
        "dharmatiles version=\"1.0.0\"\n\
        tile size=\"1x1\" seed=11 {{\n\
        stack {{\n\
            base\n\
            dirt clods=none\n\
            scatter perSquare=60 {words} {{ pebble size=\"2dmm .. 4dmm\" }}\n\
        }}\n\
        }}\n"
    );
    let src = Memory(BTreeMap::from([("t.dtile".to_owned(), text)]));
    let opts = dt_testkit::seeded(&Options::default());
    memo::build(&src, "t.dtile", &opts).unwrap()
}

/// The stones that stand, and how many different shapes they are (by footprint).
fn stones(b: &Built) -> (u32, u32) {
    let radii: Vec<u64> = b
        .report
        .placed
        .iter()
        .filter(|p| p.kind == Kind::Rock)
        .map(|p| p.r.to_bits())
        .collect();
    let shapes: BTreeSet<u64> = radii.iter().copied().collect();
    (
        u32::try_from(radii.len()).unwrap(),
        u32::try_from(shapes.len()).unwrap(),
    )
}

/// With `variants=K` the stones are K variants' bodies built once and the rest copies: the shapes that stand are exactly
/// the bodies the record says were built, and none is built twice.
#[test]
fn a_scatter_with_variants_builds_exactly_the_shapes_that_stand() {
    for k in [1_u32, 3, 6] {
        let b = pebbles(&format!("variants={k}"));
        let (stood, shapes) = stones(b);
        let u = b.report.things;
        assert!(
            stood > 3 * k,
            "enough stones to repeat every variant: {stood}"
        );
        assert_eq!(u.built, shapes, "variants={k}: {u:?}");
        assert!((1..=4 * k).contains(&u.built), "variants={k}: {u:?}");
        assert_eq!(u.shared, stood - u.built, "every other is a copy: {u:?}");
        assert_eq!(u.kept, 0);
    }
}

/// Unset, every placement is its own draw, built once: nothing is a copy.
#[test]
fn a_scatter_without_variants_builds_every_stone() {
    let b = pebbles("");
    let (stood, shapes) = stones(b);
    let u = b.report.things;
    assert_eq!((u.built, u.shared, u.kept), (stood, 0, 0));
    assert_eq!(shapes, stood, "every stone is its own shape");
}
