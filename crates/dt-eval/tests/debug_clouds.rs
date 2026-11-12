//! The debug layer draws every cloud: a body's low and high sheets shaded
//! by how often each height comes up, a tree tip's and a stone's cloud as a
//! span.  It is marks, never geometry: with the layer off nothing is made,
//! and what is built never changes.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use std::path::PathBuf;

use dt_eval::{Built, Memory, Options, build};

fn library_tile(rel: &str) -> (Memory, String) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../library/tests")
        .join(rel);
    let text = std::fs::read_to_string(&path).unwrap();
    let name = "t.dtile".to_owned();
    (Memory([(name.clone(), text)].into_iter().collect()), name)
}

fn built(rel: &str, debug: bool) -> Built {
    let (m, name) = library_tile(rel);
    build(
        &m,
        &name,
        &Options {
            clouds: debug,
            ..Options::default()
        },
    )
    .unwrap_or_else(|e| panic!("{rel}: {e:#?}"))
}

fn sheet<'a>(b: &'a Built, class: &str) -> &'a dt_kit::debug::Sheet {
    let d = b.clouds.as_ref().unwrap();
    d.sheets
        .iter()
        .find(|s| s.class == class)
        .unwrap_or_else(|| panic!("no {class} sheet"))
}

/// The layer on changes no hash, no stage and no triangle.
fn same_built(rel: &str) {
    let (off, on) = (built(rel, false), built(rel, true));
    assert!(off.clouds.is_none(), "{rel}: nothing collected with it off");
    assert_eq!(
        off.export.mesh.content_hash(),
        on.export.mesh.content_hash(),
        "{rel}"
    );
    let h = |b: &Built| b.stages.iter().map(|s| s.hash).collect::<Vec<_>>();
    assert_eq!(h(&off), h(&on), "{rel}");
}

#[test]
fn a_dirt_cloud_draws_two_sheets_shaded_by_its_odds() {
    // low(0 .. 3dmm): the low end comes up far more often than the high
    let b = built("ground/1x1-cloud-dirt.dtile", true);
    let (lo, hi) = (sheet(&b, "band-lo"), sheet(&b, "band-hi"));
    for s in [lo, hi] {
        assert_eq!(s.shade.len(), s.z.z.len(), "a shade per node");
        assert!(s.shade.iter().all(|v| (0.0..=1.0).contains(v)));
    }
    let mean = |s: &dt_kit::debug::Sheet| {
        let (sum, n) = s
            .shade
            .iter()
            .zip(&s.region.bits)
            .filter(|&(_, &b)| b)
            .fold((0.0, 0), |(a, n), (v, _)| (a + f64::from(*v), n + 1));
        sum / f64::from(n)
    };
    assert!(
        mean(lo) > mean(hi) + 0.2,
        "low sheet {} is the common height, high {}",
        mean(lo),
        mean(hi)
    );
    // the two sheets are the band: the high is never under the low
    assert!(lo.z.z.iter().zip(&hi.z.z).all(|(l, h)| h >= l));
}

#[test]
fn a_pinned_cloud_and_a_water_cloud_draw_their_sheets() {
    let pinned = built("ground/1x1-cloud-pinned.dtile", true);
    assert!(sheet(&pinned, "band-lo").owner.contains("dirt"));
    assert!(sheet(&pinned, "band-hi").owner.contains("dirt"));
    let water = built("water/1x1-cloud-water.dtile", true);
    let hi = water
        .clouds
        .as_ref()
        .unwrap()
        .sheets
        .iter()
        .find(|s| s.class == "band-hi" && s.owner.contains("water"))
        .expect("the water's own high sheet");
    // bell(): the ends are rare, so the shade there is low, under a flat 0.5
    let bell: Vec<f32> = hi
        .shade
        .iter()
        .zip(&hi.region.bits)
        .filter(|&(_, &b)| b)
        .map(|(v, _)| *v)
        .collect();
    assert!(!bell.is_empty());
    // (the nodes the edge pin holds have no odds: a flat 0.5)
    assert!(bell.iter().all(|&v| v <= 0.5));
    let rare = bell.iter().filter(|&&v| v < 0.3).count();
    assert!(rare * 2 > bell.len(), "{rare} of {} nodes", bell.len());
}

#[test]
fn a_flat_cloud_shades_at_half() {
    use dt_geom::Weighting;
    use dt_kit::debug::shade_of;
    assert_eq!(shade_of(None, 0.0), 0.5);
    assert_eq!(shade_of(Some(&Weighting::Flat), 0.7), 0.5);
    let low = Weighting::Low(3.0);
    assert!(shade_of(Some(&low), 0.0) > shade_of(Some(&low), 1.0));
}

#[test]
fn a_rock_cloud_draws_a_span_for_every_topped_stone() {
    let b = built("rocks/2x2-rock-clouds.dtile", true);
    let d = b.clouds.as_ref().unwrap();
    let stones: Vec<_> = d
        .columns
        .iter()
        .filter(|c| c.class == "stone-cloud")
        .collect();
    // ten placements; a rock may be a cluster of stones, each with its cloud
    assert!(stones.len() >= 10, "stones: {}", stones.len());
    for c in &stones {
        assert!(c.span[0] < c.span[1], "{c:?}");
        assert!(
            c.rolled >= c.span[0] - 1e-9 && c.rolled <= c.span[1] + 1e-9,
            "{c:?}"
        );
        assert!(c.shade.iter().all(|v| (0.0..=1.0).contains(v)));
        assert!(c.owner.contains("scatter"), "{}", c.owner);
        // high(): the high end is the common one
        assert!(c.shade[1] > c.shade[0], "{c:?}");
    }
}

#[test]
fn a_grove_draws_a_span_for_every_branch_tip() {
    let b = built("plants/2x2-grove.dtile", true);
    let d = b.clouds.as_ref().unwrap();
    let tips: Vec<_> = d
        .columns
        .iter()
        .filter(|c| c.class == "tip-cloud")
        .collect();
    assert!(tips.len() >= 5 * 10, "tips: {}", tips.len());
    for c in &tips {
        assert!(
            c.span[0] < c.span[1] && c.rolled <= c.span[1] + 1e-9,
            "{c:?}"
        );
    }
}

#[test]
fn clouds_are_drawn_the_same_every_time_and_change_nothing_built() {
    for rel in [
        "ground/1x1-cloud-dirt.dtile",
        "ground/1x1-cloud-pinned.dtile",
        "water/1x1-cloud-water.dtile",
        "rocks/2x2-rock-clouds.dtile",
    ] {
        same_built(rel);
        let (a, b) = (built(rel, true), built(rel, true));
        let (da, db) = (a.clouds.unwrap(), b.clouds.unwrap());
        assert_eq!(da.sheets.len(), db.sheets.len(), "{rel}");
        for (x, y) in da.sheets.iter().zip(&db.sheets) {
            assert_eq!((&x.z.z, &x.shade), (&y.z.z, &y.shade), "{rel}");
        }
        let key = |c: &dt_kit::debug::Column| format!("{c:?}");
        assert_eq!(
            da.columns.iter().map(key).collect::<Vec<_>>(),
            db.columns.iter().map(key).collect::<Vec<_>>(),
            "{rel}"
        );
    }
}

#[test]
fn the_grove_builds_the_same_with_the_layer_on() {
    same_built("plants/2x2-grove.dtile");
}

#[test]
fn debug_marks_never_reach_the_export() {
    let b = built("rocks/2x2-rock-clouds.dtile", true);
    assert!(!b.clouds.as_ref().unwrap().columns.is_empty());
    // the printed mesh is the solids' union: the same triangles with the
    // layer off (checked above by hash); and the gate saw no extra part
    let off = built("rocks/2x2-rock-clouds.dtile", false);
    assert_eq!(
        b.export.mesh.triangles.len(),
        off.export.mesh.triangles.len()
    );
    let p = Options::default().printer;
    assert_eq!(b.audit.failures(&p), off.audit.failures(&p));
}
