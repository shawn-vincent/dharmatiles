//! Tree crowns roll inside their cloud: a tree's `top=` is a cloud like any
//! other top, read at each branch tip (`$x $y $d $a $bottom`, `$region` the
//! tree's own disc and `parent($region)` the region it stands in). The tips
//! roll smoothly inside it and the crown, clumps and leaves included, stays
//! under the cloud's high; nothing grows past the height asked for.

#![allow(clippy::unwrap_used)]

use dt_eval::{Memory, Options, build, check};

fn mem(text: &str) -> Memory {
    Memory(
        [("t.dtile".to_owned(), text.to_owned())]
            .into_iter()
            .collect(),
    )
}

/// A 2x2 tile with one tree at `at` and the given `top`.
fn tile(at: &str, top: &str) -> String {
    format!(
        "dharmatiles version=1\ntile size=\"2x2\" seed=3 {{\n    stack {{\n        base\n        dirt clods=none\n        tree at=\"{at}\" top=\"{top}\" {{\n            trunk height=\"14dmm\"\n            crown width=\"16dmm\" clumps=24 shape=round branching=normal\n        }}\n    }}\n}}\n"
    )
}

/// What the tree came to: its height over its foot, mm, from the build
/// record (a gate failure's build is a build all the same).
fn height(text: &str) -> f64 {
    let opts = Options::default();
    match build(&mem(text), "t.dtile", &opts) {
        Ok(b) => b
            .notes
            .iter()
            .find(|(k, _)| k == "tree.height_mm")
            .map(|(_, v)| *v)
            .expect("a tree"),
        Err(e) => panic!("{text}\n{e:#?}"),
    }
}

#[test]
fn the_crown_stays_under_the_high_of_its_cloud() {
    // the overshoot of the clumps and leaves is no longer on top of the
    // height asked for: a 45dmm tree is at most 45dmm
    for top in [
        "$bottom + 45dmm",
        "$bottom + high(36dmm .. 46dmm)",
        "$bottom + [38dmm .. 46dmm]",
    ] {
        let high = if top.ends_with("46dmm]") || top.contains("46dmm)") {
            46.0
        } else {
            45.0
        };
        let h = height(&tile("(1sq, 1sq)", top));
        assert!(h <= high + 1e-6, "{top}: {h} over {high}");
        assert!(h > high - 8.0, "{top}: {h} is nowhere near {high}");
    }
}

#[test]
fn a_tree_top_reads_the_place_of_its_tips() {
    // a tip's $d is its place in the tree's own disc; `$a` the angle round
    // the trunk; $x and $y the tile's coordinates
    for top in [
        "$bottom + 50dmm - 6dmm * $d",
        "$bottom + 46dmm + 3dmm * sin(360deg * $a)",
        "$bottom + 46dmm + 4dmm * $x - 2dmm * $y",
        "$bottom + 46dmm - d($region, %) * 4dmm",
    ] {
        let text = tile("(1sq, 1sq)", top);
        check(&mem(&text), "t.dtile", &Options::default())
            .unwrap_or_else(|e| panic!("{top}\n{e:#?}"));
    }
    // a crown shaped by $d is still under its high
    let h = height(&tile("(1sq, 1sq)", "$bottom + 50dmm - 6dmm * $d"));
    assert!(h <= 50.0 + 1e-6, "{h}");
}

#[test]
fn a_grove_is_taller_in_the_middle() {
    // the tree reads d(parent($region)): 0% at the middle of the tile, 100%
    // at its edge
    let top = "$bottom + 26dmm + 40dmm * (1 - d(parent($region), %))";
    let middle = height(&tile("(1sq, 1sq)", top));
    let rim = height(&tile("(0.3sq, 1sq)", top));
    assert!(middle > rim + 5.0, "middle {middle} rim {rim}");
}

#[test]
fn a_parent_the_tree_does_not_have_is_refused_before_any_build() {
    let text = tile(
        "(1sq, 1sq)",
        "$bottom + 45dmm + d(parent(parent($region)), %) * 1dmm",
    );
    let e = match check(&mem(&text), "t.dtile", &Options::default()) {
        Ok(_) => panic!("a tree has no grandparent below the tile"),
        Err(e) => e,
    };
    assert!(e.iter().any(|d| d.code == "dt::region"), "{e:?}");
}

#[test]
fn a_top_that_leaves_no_room_for_the_crown_is_refused_where_it_is_low() {
    // 14dmm of trunk and a top at 22dmm: the tips stand under the clumps
    let text = tile("(1sq, 1sq)", "$bottom + 22dmm");
    let e = build(&mem(&text), "t.dtile", &Options::default())
        .err()
        .expect("no room for a crown");
    assert!(format!("{e:?}").contains("no room"), "{e:?}");
}

#[test]
fn a_tree_with_nothing_written_builds() {
    // no `top=`, no trunk, no crown: the defaults must leave the crown room
    // (a 30dmm trunk under a 40dmm tree once left it 0.2 mm and refused)
    let text = "dharmatiles version=1\ntile size=\"1x1\" seed=1 {\n    stack {\n        base\n        dirt clods=none\n        tree at=\"(0.5sq, 0.5sq)\" leaves=bare\n    }\n}\n";
    let h = height(text);
    // its top is 40dmm; the tips stand a clump's depth under it
    assert!(
        (25.0..=40.0).contains(&h),
        "a default tree stands under 40dmm, not {h}"
    );
}

#[test]
fn a_top_the_crown_cannot_fit_is_the_tiles_to_fix() {
    // a refusal with help, about a control: dt::look, shown as a mistake at
    // the tree, not dt::build (a bug)
    let text = tile("(1sq, 1sq)", "$bottom + 20dmm").replace("14dmm", "19dmm");
    let opts = Options::default();
    let errs = build(&mem(&text), "t.dtile", &opts).err().expect("refused");
    assert!(errs.iter().any(|d| d.code == "dt::look"), "{errs:#?}");
    check(&mem(&text), "t.dtile", &opts).unwrap();
}
