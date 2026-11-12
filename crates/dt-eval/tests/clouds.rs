//! Clouds on every top: a body's `top=` and a pin's top may be a low surface,
//! a high surface and one weighting. The solve keeps the odds where the body
//! rolls its own heights and drops them where a pin holds; arithmetic on a
//! cloud follows rule B (shift and scale keep the weighting, a negative
//! factor swaps low and high with the odds turned round, `min max clamp
//! smooth` act on the surfaces, `sin cos abs` and two clouds are refused);
//! the new test tiles build and pass the gate; a flat weighting builds as the
//! plain range always did.

#![allow(clippy::unwrap_used)]

use dt_eval::{Diag, Library, Memory, Options, build, check};
use dt_kit::Weighting;

fn mem(text: &str) -> Memory {
    Memory(
        [("t.dtile".to_owned(), text.to_owned())]
            .into_iter()
            .collect(),
    )
}

/// A 3x3 tile: the middle is 52 mm from every rim, where the library's rim
/// pin has eased away whatever the top.
fn tile(stack: &str) -> String {
    format!(
        "dharmatiles version=1\ntile size=\"3x3\" seed=7 {{\n    stack {{\n        base\n{stack}\n    }}\n}}\n"
    )
}

/// A cliff has no default pin and does not ease: its band is the top as
/// written, a good place to read what a formula makes.
fn dirt(top: &str) -> String {
    tile(&format!("        cliff top=\"{top}\""))
}

fn solved(text: &str) -> dt_eval::Checked {
    check(&mem(text), "t.dtile", &Options::default()).unwrap_or_else(|e| panic!("{text}\n{e:#?}"))
}

fn errors(text: &str) -> Vec<Diag> {
    match check(&mem(text), "t.dtile", &Options::default()) {
        Ok(c) => panic!("expected a refusal; notes {:?}\n{text}", c.notes()),
        Err(e) => e,
    }
}

fn refused(text: &str, code: &str, help: &str) -> Diag {
    let e = errors(text);
    let d = e
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("want {code}: {e:?}"));
    assert!(
        d.help.as_deref().is_some_and(|h| h.contains(help)),
        "{code} help {:?} lacks `{help}`",
        d.help
    );
    assert!(d.location().is_some(), "{code} points at the document");
    d.clone()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The solved band of the first body at the middle of the tile, and its odds.
fn middle(text: &str) -> (dt_kit::Iv, Option<Weighting>) {
    let c = solved(text);
    let b = &c.solved.as_ref().unwrap().bodies[0];
    (b.at(1.5, 1.5), b.odds_at(1.5, 1.5).cloned())
}

#[test]
fn a_cloud_top_is_a_low_a_high_and_a_weighting() {
    let (iv, w) = middle(&dirt("$groundHeight + low(0dmm .. 4dmm)"));
    assert!(close(iv.lo, 5.0) && close(iv.hi, 9.0), "{iv:?}");
    assert_eq!(w, Some(Weighting::Low(4.0)));
    // extra plain numbers stay (question 23)
    let (_, w) = middle(&dirt("$groundHeight + low(0dmm .. 4dmm, 11)"));
    assert_eq!(w, Some(Weighting::Low(11.0)));
    let (_, w) = middle(&dirt("$groundHeight + bell(0dmm .. 4dmm, 0.3, 0.25)"));
    assert_eq!(
        w,
        Some(Weighting::Bell {
            spread: 0.3,
            at: 0.25
        })
    );
}

#[test]
fn a_plain_range_has_no_odds() {
    for top in [
        "[$groundHeight .. $groundHeight + 4dmm]",
        "$groundHeight + flat(0dmm .. 4dmm)",
        "$groundHeight + low(0dmm .. 4dmm, 1)",
    ] {
        let (iv, w) = middle(&dirt(top));
        assert!(close(iv.lo, 5.0) && close(iv.hi, 9.0), "{top}: {iv:?}");
        assert_eq!(w, None, "{top}");
    }
}

#[test]
fn a_pin_replaces_the_cloud_where_it_holds_and_the_body_eases_back() {
    // a pin below the cloud: dirt eases down to it no steeper than its slope
    let c = solved(&tile(
        "        dirt top=\"$groundHeight + low(0dmm .. 4dmm)\" {\n            pin top=\"$groundHeight - 1dmm\" at=center\n        }",
    ));
    let b = &c.solved.as_ref().unwrap().bodies[0];
    // exact at the pin: one height, no odds
    let at = b.at(1.5, 1.5);
    assert!(close(at.lo, 4.0) && close(at.hi, 4.0), "{at:?}");
    assert_eq!(b.odds_at(1.5, 1.5), None);
    // the library's rim pin holds the edge: no odds there either
    assert_eq!(b.odds_at(0.0, 1.5), None);
    // away from both the body's cloud is back, low 5 to high 9
    let far = b.at(2.4, 1.5);
    assert!(close(far.lo, 5.0) && close(far.hi, 9.0), "{far:?}");
    assert_eq!(b.odds_at(2.4, 1.5), Some(&Weighting::Low(4.0)));
    // between, the band comes down into the pin; its odds are the body's
    let ease = b.at(1.7, 1.5);
    assert!(
        ease.lo < far.lo && ease.hi < far.hi && ease.lo >= 4.0,
        "{ease:?}"
    );
    assert!(ease.lo < ease.hi, "{ease:?}");
    assert_eq!(b.odds_at(1.7, 1.5), Some(&Weighting::Low(4.0)));
}

#[test]
fn a_pin_of_a_cliff_holds_its_place_and_nothing_eases() {
    let c = solved(&tile(
        "        cliff top=\"$dirtEdgeHeight + high(10dmm .. 30dmm)\" {\n            pin top=\"$dirtEdgeHeight + 5dmm\" at=center\n        }",
    ));
    let b = &c.solved.as_ref().unwrap().bodies[0];
    let at = b.at(1.5, 1.5);
    assert!(close(at.lo, 8.4) && close(at.hi, 8.4), "{at:?}");
    assert_eq!(b.odds_at(1.5, 1.5), None);
    let near = b.at(1.7, 1.5);
    assert!(close(near.lo, 13.4) && close(near.hi, 33.4), "{near:?}");
    assert_eq!(b.odds_at(1.7, 1.5), Some(&Weighting::High(4.0)));
}

#[test]
fn a_pin_may_be_a_cloud_of_its_own() {
    let c = solved(&tile(
        "        dirt top=\"$groundHeight + low(0dmm .. 4dmm)\" {\n            pin top=\"$groundHeight + 6dmm + high(0dmm .. 2dmm)\" at=center\n        }",
    ));
    let b = &c.solved.as_ref().unwrap().bodies[0];
    let at = b.at(1.5, 1.5);
    assert!(close(at.lo, 11.0) && close(at.hi, 13.0), "{at:?}");
    assert_eq!(b.odds_at(1.5, 1.5), Some(&Weighting::High(4.0)));
    assert_eq!(b.odds_at(2.4, 1.5), Some(&Weighting::Low(4.0)));
}

#[test]
fn shifting_and_scaling_keep_the_weighting() {
    for (top, lo, hi) in [
        ("$groundHeight + low(0dmm .. 4dmm) + 2dmm", 7.0, 11.0),
        (
            "2dmm + low($groundHeight .. $groundHeight + 4dmm)",
            7.0,
            11.0,
        ),
        ("$groundHeight + 2 * low(0dmm .. 4dmm)", 5.0, 13.0),
        ("$groundHeight + low(0dmm .. 4dmm) * 0.5", 5.0, 7.0),
        ("$groundHeight + low(0dmm .. 4dmm) / 2", 5.0, 7.0),
        ("$groundHeight - low(-4dmm .. 0dmm)", 5.0, 9.0),
    ] {
        let (iv, w) = middle(&dirt(top));
        assert!(close(iv.lo, lo) && close(iv.hi, hi), "{top}: {iv:?}");
        let want = if top.contains("- low") {
            Weighting::High(4.0)
        } else {
            Weighting::Low(4.0)
        };
        assert_eq!(w, Some(want), "{top}");
    }
}

#[test]
fn a_negative_factor_swaps_low_and_high_and_turns_the_odds_round() {
    // 10 - high(0..4) runs from 6 to 10, and is low(…): mostly low
    let (iv, w) = middle(&dirt("$groundHeight + 10dmm - high(0dmm .. 4dmm)"));
    assert!(close(iv.lo, 11.0) && close(iv.hi, 15.0), "{iv:?}");
    assert_eq!(w, Some(Weighting::Low(4.0)));
    let (iv, w) = middle(&dirt("$groundHeight - 2 * high(0dmm .. 2dmm)"));
    assert!(close(iv.lo, 1.0) && close(iv.hi, 5.0), "{iv:?}");
    assert_eq!(w, Some(Weighting::Low(4.0)));
    let (_, w) = middle(&dirt("$groundHeight + high(0dmm .. 2dmm) / -1"));
    assert_eq!(w, Some(Weighting::Low(4.0)));
    // a factor that changes sign across the tile swaps where it is negative
    let c = solved(&dirt(
        "$groundHeight + 10dmm + cos(240deg * $x) * high(1dmm .. 3dmm)",
    ));
    let b = &c.solved.as_ref().unwrap().bodies[0];
    // cos(360deg) = 1: the cloud as written
    let mid = b.at(1.5, 1.5);
    assert!(close(mid.lo, 16.0) && close(mid.hi, 18.0), "{mid:?}");
    assert_eq!(b.odds_at(1.5, 1.5), Some(&Weighting::High(4.0)));
    // cos(180deg) = -1: swapped, mirrored
    let swapped = b.at(0.75, 1.5);
    assert!(
        close(swapped.lo, 12.0) && close(swapped.hi, 14.0),
        "{swapped:?}"
    );
    assert_eq!(b.odds_at(0.75, 1.5), Some(&Weighting::Low(4.0)));
}

#[test]
fn min_max_clamp_and_smooth_act_on_the_surfaces_and_keep_the_weighting() {
    for (top, lo, hi) in [
        // squeezed under 15: no shelf, the same odds over the shorter range
        (
            "min($groundHeight + high(0dmm .. 20dmm), $groundHeight + 10dmm)",
            5.0,
            15.0,
        ),
        (
            "max($groundHeight + high(0dmm .. 20dmm), $groundHeight + 4dmm)",
            9.0,
            25.0,
        ),
        (
            "clamp($groundHeight + high(0dmm .. 20dmm), $groundHeight + 2dmm, $groundHeight + 10dmm)",
            7.0,
            15.0,
        ),
    ] {
        let (iv, w) = middle(&dirt(top));
        assert!(close(iv.lo, lo) && close(iv.hi, hi), "{top}: {iv:?}");
        assert_eq!(w, Some(Weighting::High(4.0)), "{top}");
    }
    let (_, w) = middle(&dirt(
        "$groundHeight + 2dmm * smooth(0.5) + high(0dmm .. 2dmm)",
    ));
    assert_eq!(w, Some(Weighting::High(4.0)));
}

#[test]
fn a_cloud_in_a_let_is_read_where_it_is_used() {
    let text = "dharmatiles version=1\ntile size=\"3x3\" seed=7 {\n    let hump \"$groundHeight + peak(0dmm .. 3dmm)\"\n    stack {\n        base\n        cliff top=\"hump + 1dmm\"\n    }\n}\n";
    let (iv, w) = middle(text);
    assert!(close(iv.lo, 6.0) && close(iv.hi, 9.0), "{iv:?}");
    assert!(matches!(w, Some(Weighting::Peak { .. })), "{w:?}");
}

#[test]
fn every_body_that_has_a_top_takes_a_cloud() {
    for stack in [
        "        dirt\n        water top=\"$waterSurfaceHeight + bell(-0.5dmm .. 0.5dmm)\"",
        "        flagstones top=\"$floorHeight + high(-1dmm .. 0dmm)\"",
        "        planks top=\"$floorHeight + high(-1dmm .. 0dmm)\"",
        "        cliff top=\"$dirtEdgeHeight + high(10dmm .. 40dmm)\"",
    ] {
        let c = solved(&tile(stack));
        let bodies = &c.solved.as_ref().unwrap().bodies;
        let b = bodies.last().unwrap();
        assert!(b.band.odds().is_some(), "{stack}");
    }
}

#[test]
fn a_list_is_a_cloud_of_the_values_written() {
    let (iv, w) = middle(&dirt("$groundHeight + [0dmm:3, 2dmm, 4dmm:1]"));
    assert!(close(iv.lo, 5.0) && close(iv.hi, 9.0), "{iv:?}");
    assert!(matches!(w, Some(Weighting::List(_))), "{w:?}");
}

#[test]
fn two_ranges_are_two_rolls_with_or_without_odds() {
    // the sum spans the interval of the sum, and its odds are those of two
    // independent rolls: a cloud and a plain range, or two clouds
    for top in [
        "$groundHeight + low(0dmm .. 2dmm) + low(0dmm .. 2dmm)",
        "$groundHeight + low(0dmm .. 2dmm) + [0dmm .. 2dmm]",
        "min($groundHeight + low(0dmm .. 4dmm), $groundHeight + high(0dmm .. 4dmm))",
    ] {
        let (iv, w) = middle(&dirt(top));
        assert!(iv.width() > 0.0, "{top}: {iv:?}");
        assert!(matches!(w, Some(Weighting::Steps(_))), "{top}: {w:?}");
    }
    // a cloud scaled by a range has no weighting: one roll is the factor
    let d = refused(
        &dirt("$groundHeight + low(0dmm .. 2dmm) * [1 .. 2]"),
        "dt::cloud",
        "multiply a cloud by a number",
    );
    assert!(d.message.contains("multiplies a cloud"), "{}", d.message);
}

#[test]
fn a_cloud_is_not_divided_into_or_folded() {
    let d = refused(
        &dirt("$groundHeight + 4dmm / low(1 .. 2)"),
        "dt::cloud",
        "shifted",
    );
    assert!(d.message.contains("divides by a cloud"), "{}", d.message);
    for f in ["sin", "cos"] {
        let d = refused(
            &dirt(&format!("$groundHeight + 2dmm * {f}(low(0deg .. 90deg))")),
            "dt::cloud",
            "inside the range",
        );
        assert!(d.message.contains("fold"), "{}", d.message);
    }
    refused(
        &dirt("$groundHeight + abs(low(-2dmm .. 2dmm))"),
        "dt::cloud",
        "inside the range",
    );
}

#[test]
fn two_plain_ranges_span_their_sum_and_come_up_like_two_dice() {
    // [0..2] + [0..2] spans [0..4]; the middle comes up more than the ends
    let (iv, w) = middle(&dirt("$groundHeight + [0dmm .. 2dmm] + [0dmm .. 2dmm]"));
    assert!(close(iv.lo, 5.0) && close(iv.hi, 9.0), "{iv:?}");
    let Some(Weighting::Steps(bins)) = w else {
        panic!("two dice have odds: {w:?}");
    };
    let (ends, mid) = (bins[0] + bins[bins.len() - 1], bins[bins.len() / 2] * 2.0);
    assert!(mid > 4.0 * ends, "a triangle: {bins:?}");
    // one range alone has no odds, as always
    let (_, w) = middle(&dirt("$groundHeight + [0dmm .. 2dmm] + 1dmm"));
    assert_eq!(w, None);
    let (iv, _) = middle(&dirt("$groundHeight + abs([-2dmm .. 2dmm])"));
    assert!(close(iv.lo, 5.0) && close(iv.hi, 7.0), "{iv:?}");
    // [1..2] - [1..2] is two dice: a band from -1 to 1, a triangle
    let (iv, w) = middle(&dirt(
        "$groundHeight + 10dmm + [1dmm .. 2dmm] - [1dmm .. 2dmm]",
    ));
    assert!(
        iv.width() > 0.0 && matches!(w, Some(Weighting::Steps(_))),
        "{iv:?} {w:?}"
    );
    assert_eq!(
        middle(&dirt(
            "$groundHeight + 10dmm + [1dmm .. 2dmm] - [1dmm .. 2dmm]"
        ))
        .1,
        w,
        "the same value gives the same odds"
    );
    let (iv, _) = middle(&dirt("$groundHeight + abs([-2dmm .. 2dmm])"));
    assert!(close(iv.lo, 5.0) && close(iv.hi, 7.0), "{iv:?}");
}

#[test]
fn a_weighting_that_cannot_be_made_is_said_where_it_is_written() {
    refused(
        &dirt("$groundHeight + low(0dmm .. 4dmm, -2)"),
        "dt::range",
        "1 is flat",
    );
    refused(
        &dirt("$groundHeight + bell(0dmm .. 4dmm, 2)"),
        "dt::range",
        "share of the width",
    );
    // a cloud at the end of a range, and a range with odds already
    refused(
        &dirt("[$groundHeight .. $groundHeight + low(0dmm .. 4dmm)]"),
        "dt::cloud",
        "around the whole range",
    );
    // a distance is not a cloud
    refused(
        "dharmatiles version=1\ntile size=\"1x1\" seed=7 {\n    line l { through \"(0sq, 0.5sq)\" \"(1sq, 0.5sq)\" }\n    region r { within \"low(2dmm .. 6dmm)\" of=l }\n    stack { base\n dirt at=r }\n}\n",
        "dt::weighting",
        "write the range alone",
    );
}

fn real_dirt(top: &str) -> String {
    format!(
        "dharmatiles version=1\ntile size=\"1x1\" seed=7 {{\n    stack {{\n        base\n        dirt top=\"{top}\"\n    }}\n}}\n"
    )
}

/// The stage hashes of a build (a draft: the same draws at a fraction of the
/// time).
fn hash(text: &str) -> Vec<(String, u64)> {
    let draft = Options {
        draft: true,
        ..Options::default()
    };
    let b = build(&mem(text), "t.dtile", &draft).unwrap_or_else(|e| panic!("{e:?}"));
    b.stages.iter().map(|s| (s.name.clone(), s.hash)).collect()
}

#[test]
fn a_flat_cloud_builds_as_the_plain_range_to_the_last_bit() {
    let plain = hash(&real_dirt("[$groundHeight .. $groundHeight + 4dmm]"));
    for same in [
        "$groundHeight + flat(0dmm .. 4dmm)",
        "flat([$groundHeight .. $groundHeight + 4dmm])",
        "$groundHeight + low(0dmm .. 4dmm, 1)",
    ] {
        assert_eq!(hash(&real_dirt(same)), plain, "{same}");
    }
    // the same range with odds is another surface
    assert_ne!(hash(&real_dirt("$groundHeight + low(0dmm .. 4dmm)")), plain);
    assert_ne!(
        hash(&real_dirt("$groundHeight + low(0dmm .. 4dmm)")),
        hash(&real_dirt("$groundHeight + high(0dmm .. 4dmm)"))
    );
}

const CLOUD_TILES: &[&str] = &[
    "lib:tests/ground/1x1-cloud-dirt.dtile",
    "lib:tests/ground/1x1-cloud-pinned.dtile",
    "lib:tests/water/1x1-cloud-water.dtile",
];

#[test]
fn the_cloud_tiles_pass_the_gate_and_build_alike_on_any_threads() {
    let printer = Options::default().printer;
    for path in CLOUD_TILES {
        let build_on = |threads: usize| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| build(&Library, path, &Options::default()))
                .unwrap_or_else(|e| panic!("{path}: {e:?}"))
        };
        let one = build_on(1);
        let four = build_on(4);
        let a = &one.audit;
        assert!(
            a.failures(&printer).is_empty(),
            "{path}: {:?}",
            a.failures(&printer)
        );
        assert!(a.watertight, "{path}");
        assert!(a.dropped.is_empty(), "{path}: {:?}", a.dropped);
        let hashes = |b: &dt_eval::Built| -> Vec<(String, u64)> {
            b.stages.iter().map(|s| (s.name.clone(), s.hash)).collect()
        };
        assert_eq!(hashes(&one), hashes(&four), "{path}: threads move the hash");
    }
}

#[test]
fn the_cloud_tiles_build_for_openlock_too() {
    let opts = Options {
        system: "ol".to_owned(),
        ..Options::default()
    };
    for path in CLOUD_TILES {
        let b = build(&Library, path, &opts).unwrap_or_else(|e| panic!("{path}: {e:?}"));
        let f = b.audit.failures(&opts.printer);
        assert!(f.is_empty(), "{path}: {f:?}");
    }
}
