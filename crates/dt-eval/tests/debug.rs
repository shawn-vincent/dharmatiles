//! The editor's debug layer changes nothing that is built: the same tile
//! with and without it writes the same STL, stage by stage.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use dt_eval::{Memory, Options, build};

const TILE: &str = r#"tile size="1x1" seed=97 {
    line shore wobble=5 { through "0,0.38" "1,0.42" }
    region pool { boundedBy shore; fill "0.5,0.25" }
    region bank { boundedBy shore; fill "0.5,0.75" }
    region shoreBand { within 7 of=shore }
    stack {
        base
        dirt {
            pin top="[waterBedHeight .. groundHeight + 1]" at=shoreBand
            pin top=waterBedHeight at=pool
        }
        grass at=bank
        water
    }
}"#;

#[test]
fn the_debug_layer_changes_nothing_built() {
    let m = Memory(BTreeMap::from([(
        "probe.dtile".to_owned(),
        TILE.to_owned(),
    )]));
    let off = build(&m, "probe.dtile", &Options::default()).unwrap();
    let on = build(
        &m,
        "probe.dtile",
        &Options {
            debug: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(off.debug.is_none());
    assert_eq!(
        off.export.mesh.content_hash(),
        on.export.mesh.content_hash()
    );
    let hashes = |b: &dt_eval::Built| {
        b.stages
            .iter()
            .map(|s| (s.name.clone(), s.hash))
            .collect::<Vec<_>>()
    };
    assert_eq!(hashes(&off), hashes(&on));

    let d = on.debug.as_ref().unwrap();
    let class = |c: &str| d.dots.iter().filter(|p| p.class == c).count();
    assert!(class("grew") > 100, "grown starts: {}", class("grew"));
    assert!(class("outside") > 0, "starts outside the bank");
    let spines = d
        .trails
        .iter()
        .filter(|t| t.class.starts_with("spine"))
        .count();
    let grown = d
        .dots
        .iter()
        .filter(|p| p.class == "grew" || p.class == "cut")
        .count();
    assert_eq!(spines, grown, "a spine for each grown start");
    assert!(d.trails.iter().any(|t| t.class == "flow"));
    // the dirt's pinned band
    assert!(
        d.sheets
            .iter()
            .any(|s| s.class == "band-hi" && s.owner.contains("dirt")),
        "{:?}",
        d.sheets
            .iter()
            .map(|s| (&s.owner, s.class))
            .collect::<Vec<_>>()
    );
}
