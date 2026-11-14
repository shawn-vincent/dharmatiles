//! The editor's debug layer changes nothing that is built: the same tile
//! with and without it writes the same STL, stage by stage.

#![allow(clippy::unwrap_used)]

mod common;

use dt_eval::{Options, build};

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
        grass at=bank
        water
    }
}"#;

#[test]
fn the_debug_layer_changes_nothing_built() {
    let m = common::memory(&[("probe.dtile", TILE)]);
    let off = build(&m, "probe.dtile", &Options::default()).unwrap();
    let on = build(
        &m,
        "probe.dtile",
        &Options {
            debug: true,
            clouds: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(off.report.debug.is_none() && off.report.clouds.is_none());
    assert_eq!(
        off.export.mesh.content_hash(),
        on.export.mesh.content_hash()
    );
    let hashes = |b: &dt_eval::Built| {
        b.report
            .stages
            .iter()
            .map(|s| (s.name.clone(), s.hash))
            .collect::<Vec<_>>()
    };
    assert_eq!(hashes(&off), hashes(&on));

    let d = on.report.debug.as_ref().unwrap();
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
    let sheets = &on.report.clouds.as_ref().unwrap().sheets;
    assert!(
        sheets
            .iter()
            .any(|s| s.class == "band-hi" && s.owner.contains("dirt")),
        "{:?}",
        sheets
            .iter()
            .map(|s| (&s.owner, s.class))
            .collect::<Vec<_>>()
    );
    // the layer's marks are only marks, the clouds only clouds
    assert!(d.sheets.is_empty() && d.columns.is_empty());
    assert!(on.report.clouds.as_ref().unwrap().dots.is_empty());
}
