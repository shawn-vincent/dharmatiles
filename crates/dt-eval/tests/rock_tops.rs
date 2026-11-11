//! A rock's `top=` is a cloud over the ground at its foot, worked out at the
//! stone's middle (`$x $y $d $a $bottom`, `$region` its disc): the stone
//! rolls one height in it and its peak stands there.  Rocks without a `top=`
//! build as they always did.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use dt_eval::{Built, Memory, Options, build, check};

fn mem(text: &str) -> Memory {
    Memory(
        [("t.dtile".to_owned(), text.to_owned())]
            .into_iter()
            .collect(),
    )
}

/// A 1x1 tile on bare dirt with one `body` line.
fn tile(body: &str) -> String {
    format!(
        "dharmatiles version=1\ntile size=\"1x1\" seed=3 {{\n    stack {{\n        base\n        dirt top=\"$dirtEdgeHeight\"\n        {body}\n    }}\n}}\n"
    )
}

fn built(text: &str, threads: Option<usize>) -> Built {
    let opts = Options::default();
    let run = || build(&mem(text), "t.dtile", &opts);
    let r = match threads {
        Some(n) => rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build()
            .unwrap()
            .install(run),
        None => run(),
    };
    r.unwrap_or_else(|e| panic!("{text}\n{e:#?}"))
}

/// The highest point of the mesh, mm.
fn peak(b: &Built) -> f64 {
    b.export
        .mesh
        .positions
        .iter()
        .map(|p| p[2])
        .fold(f64::MIN, f64::max)
}

fn hashes(b: &Built) -> Vec<u64> {
    b.stages.iter().map(|s| s.hash).collect()
}

#[test]
fn a_rock_top_is_read_and_checked() {
    for body in [
        "rock at=\"(0.5sq, 0.5sq)\" top=\"$bottom + [4dmm .. 9dmm]\"",
        "stone at=\"(0.5sq, 0.5sq)\" top=\"$bottom + 3dmm + 3dmm * $x\"",
        "boulder at=\"(0.5sq, 0.5sq)\" top=\"$bottom + 8dmm * (1 - $d * $d)\"",
        "pebble at=\"(0.5sq, 0.5sq)\" top=\"$bottom + high(1.5dmm .. 2.5dmm)\"",
        "scatter count=3 { rock top=\"$bottom + [4dmm .. 9dmm] * (1 - $d)\" }",
        "scatter count=3 { rock top=\"$bottom + 5dmm\"\n pebble }",
    ] {
        check(&mem(&tile(body)), "t.dtile", &Options::default())
            .unwrap_or_else(|e| panic!("{body}\n{e:#?}"));
    }
}

#[test]
fn a_rock_peak_stands_at_its_top() {
    // on flat dirt a lone rock's peak is the highest point of the tile,
    // $bottom + 8dmm give or take the grain
    let flat = built(
        &tile("rock at=\"(0.5sq, 0.5sq)\" size=\"10dmm\" top=\"$bottom + 8dmm\""),
        None,
    );
    let low = built(
        &tile("rock at=\"(0.5sq, 0.5sq)\" size=\"10dmm\" top=\"$bottom + 4dmm\""),
        None,
    );
    assert!(
        peak(&flat) > peak(&low) + 3.0,
        "{} {}",
        peak(&flat),
        peak(&low)
    );
    let plain = built(&tile("rock at=\"(0.5sq, 0.5sq)\" size=\"10dmm\""), None);
    assert_ne!(hashes(&plain), hashes(&flat));
    assert!(
        flat.notes
            .iter()
            .any(|(k, v)| k == "topped_stones" && *v == 1.0)
    );
    assert!(!plain.notes.iter().any(|(k, _)| k == "topped_stones"));
}

#[test]
fn a_cloud_rolls_inside_its_range() {
    // a plain range: every rolled peak is between its ends over the foot
    let lo = built(
        &tile("rock at=\"(0.5sq, 0.5sq)\" size=\"10dmm\" top=\"$bottom + 4dmm\""),
        None,
    );
    let hi = built(
        &tile("rock at=\"(0.5sq, 0.5sq)\" size=\"10dmm\" top=\"$bottom + 9dmm\""),
        None,
    );
    let mid = built(
        &tile("rock at=\"(0.5sq, 0.5sq)\" size=\"10dmm\" top=\"$bottom + [4dmm .. 9dmm]\""),
        None,
    );
    assert!(peak(&mid) >= peak(&lo) - 1e-6 && peak(&mid) <= peak(&hi) + 1e-6);
}

#[test]
fn rolls_are_deterministic_and_the_same_on_any_thread_count() {
    let text = tile("scatter count=6 { rock top=\"$bottom + high(3dmm .. 9dmm)\" }");
    let a = built(&text, Some(1));
    let b = built(&text, Some(4));
    let c = built(&text, None);
    assert_eq!(hashes(&a), hashes(&b));
    assert_eq!(hashes(&a), hashes(&c));
    assert_eq!(peak(&a), peak(&b));
}

#[test]
fn rocks_without_a_top_are_untouched_by_the_ones_with() {
    // the stage hashes of a tile with no top do not depend on this feature:
    // a scatter of plain rocks builds the same twice and differs from topped
    let plain = tile("scatter count=4 { rock }");
    let topped = tile("scatter count=4 { rock top=\"$bottom + 6dmm\" }");
    assert_eq!(hashes(&built(&plain, None)), hashes(&built(&plain, None)));
    assert_ne!(hashes(&built(&plain, None)), hashes(&built(&topped, None)));
}

#[test]
fn a_top_under_the_foot_is_refused_with_its_place() {
    let text = tile("rock at=\"(0.5sq, 0.5sq)\" top=\"$bottom - 3dmm\"");
    let e = build(&mem(&text), "t.dtile", &Options::default())
        .err()
        .expect("under the foot");
    let msg = format!("{e:?}");
    assert!(msg.contains("peak stands"), "{msg}");
    assert!(e.iter().any(|d| d.at.is_some()), "{e:?}");
}

#[test]
fn what_a_rock_does_not_set_is_refused() {
    // a rock's top reads the place and ground; `$nope` is no name
    let text = tile("rock at=\"(0.5sq, 0.5sq)\" top=\"$bottom + $nope\"");
    let e = check(&mem(&text), "t.dtile", &Options::default())
        .err()
        .expect("unknown name");
    assert!(e.iter().any(|d| d.at.is_some()), "{e:?}");
}

#[test]
fn a_clouds_arithmetic_rules_hold_on_a_rock() {
    // two clouds added are refused, as on any top
    let text =
        tile("rock at=\"(0.5sq, 0.5sq)\" top=\"$bottom + high(1dmm .. 5dmm) + low(1dmm .. 3dmm)\"");
    let e = build(&mem(&text), "t.dtile", &Options::default())
        .err()
        .expect("two clouds");
    assert!(e.iter().any(|d| d.code == "dt::cloud"), "{e:?}");
}
