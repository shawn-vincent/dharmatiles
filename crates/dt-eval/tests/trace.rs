//! Every written triangle names the stack entry that made it (the preview's
//! click-to-select): surfaces by their masks, solids by their owner path,
//! the connector last.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use dt_eval::{Memory, NO_ENTRY, Options, build};

const TILE: &str = r#"tile size="1x1" seed=97 {
    line shore wobble=5 { through "0,0.38" "1,0.42" }
    region pool { bounded-by shore; fill "0.5,0.25" }
    region bank { bounded-by shore; fill "0.5,0.75" }
    region shoreBand { within 7 of=shore }
    stack {
        base
        dirt {
            pin top="[waterBedHeight .. groundHeight + 1]" at=shoreBand
            pin top=waterBedHeight at=pool
        }
        scatter in=pool count=2 { rock size="6..10" }
        water
    }
}"#;

#[test]
fn every_triangle_has_its_entry() {
    let mut m = BTreeMap::new();
    m.insert("probe.dtile".to_owned(), TILE.to_owned());
    let b = build(&Memory(m), "probe.dtile", &Options::default()).unwrap();
    let n = b.materials.len();
    assert_eq!(b.entries.len(), n);
    assert_eq!(b.pieces.len(), n);
    let mut count: BTreeMap<u16, usize> = BTreeMap::new();
    let mut rocks: BTreeMap<u16, usize> = BTreeMap::new();
    for (e, p) in b.entries.iter().zip(&b.pieces) {
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
    for tile in [
        "lib:tiles/water/1x1-water+grass.dtile",
        "lib:tiles/test/2x2-meadow.dtile",
    ] {
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
        assert_eq!(d.entries.len(), d.materials.len());
        assert!(
            !d.entries.contains(&NO_ENTRY),
            "every draft triangle is traced"
        );
    }
}
