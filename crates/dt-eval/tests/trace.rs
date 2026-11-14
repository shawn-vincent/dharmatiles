//! Every written triangle names the stack entry that made it (the preview's
//! click-to-select): surfaces by their masks, solids by their owner path,
//! the connector last.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeMap;

use dt_eval::{NO_ENTRY, Options, build};

const TILE: &str = r#"dharmatiles version="1.0.0"; tile size="1x1" seed=97 {
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
        scatter at=pool count=2 { rock size="6dmm .. 10dmm" }
        water
    }
}"#;

#[test]
fn every_triangle_has_its_entry() {
    let b = build(
        &common::memory(&[("probe.dtile", TILE)]),
        "probe.dtile",
        &Options::default(),
    )
    .unwrap();
    let n = b.materials.len();
    assert_eq!(b.report.entries.len(), n);
    assert_eq!(b.report.pieces.len(), n);
    let mut count: BTreeMap<u16, usize> = BTreeMap::new();
    let mut rocks: BTreeMap<u16, usize> = BTreeMap::new();
    for (e, p) in b.report.entries.iter().zip(&b.report.pieces) {
        *count.entry(*e).or_default() += 1;
        if *e == 1 {
            *rocks.entry(*p).or_default() += 1;
        }
    }
    eprintln!("entries {count:?} rock pieces {rocks:?}");
    assert!(!count.contains_key(&NO_ENTRY), "{count:?}");
    // dirt, scatter, water, then the connector
    for e in [0, 1, 2, 3] {
        assert!(
            count.contains_key(&e),
            "entry {e} has no triangles: {count:?}"
        );
    }
    assert_eq!(rocks.len(), 2, "{rocks:?}");
}

#[test]
fn a_draft_is_quick_and_still_traced() {
    let full = Options::default();
    let draft = Options {
        draft: true,
        ..Options::default()
    };
    {
        let tile = "lib:tests/water/1x1-water+grass.dtile";
        let t0 = std::time::Instant::now();
        let f = dt_eval::build(&dt_eval::Library, tile, &full).unwrap();
        let tf = t0.elapsed().as_secs_f64();
        let t0 = std::time::Instant::now();
        let d = dt_eval::build(&dt_eval::Library, tile, &draft).unwrap();
        let td = t0.elapsed().as_secs_f64();
        eprintln!(
            "{tile}: full {tf:.2} s, {} triangles; draft {td:.2} s, {} triangles",
            f.materials.len(),
            d.materials.len()
        );
        assert_eq!(d.report.entries.len(), d.materials.len());
        assert!(
            !d.report.entries.contains(&NO_ENTRY),
            "every draft triangle is traced"
        );
    }
}
