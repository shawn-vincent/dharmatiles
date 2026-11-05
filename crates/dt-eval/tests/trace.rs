//! Every written triangle names the stack entry that made it (the preview's
//! click-to-select): surfaces by their masks, solids by their owner path,
//! the connector last.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use dt_eval::{Memory, NO_ENTRY, Options, build};

const TILE: &str = r#"tile probe size="1x1" seed=97 {
    line shore wobble=5 { through "0,0.38" "1,0.42" }
    region pool { bounded-by shore; fill "0.5,0.25" }
    region bank { bounded-by shore; fill "0.5,0.75" }
    stack {
        dirt in=bank
        ground in=pool level="water - 2.2"
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
        if *e == 2 {
            *rocks.entry(*p).or_default() += 1;
        }
    }
    eprintln!("entries {count:?} rock pieces {rocks:?}");
    assert!(!count.contains_key(&NO_ENTRY), "{count:?}");
    // dirt, scatter, water, then the connector; the pool's ground is all
    // under the water, which owns the surface it filled
    assert!(!count.contains_key(&1), "{count:?}");
    for e in [0, 2, 3, 4] {
        assert!(
            count.contains_key(&e),
            "entry {e} has no triangles: {count:?}"
        );
    }
    assert_eq!(rocks.len(), 2, "{rocks:?}");
}
