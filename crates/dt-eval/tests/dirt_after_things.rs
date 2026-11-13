//! The stack is built in the order it is listed: a dirt listed after a tree
//! is built after it and banks against the trunk; a thing listed above the
//! ground it stands on is refused, never reordered.

#![allow(clippy::unwrap_used)]

mod common;

use dt_eval::plan::Step;
use dt_eval::{Built, Memory, Options, build, check};
use dt_kit::Material;

fn mem(text: &str) -> Memory {
    common::memory(&[("t.dtile", text)])
}

const TREE: &str = "tree at=\"(0.5sq, 0.5sq)\" top=\"$bottom + 30dmm\" { trunk height=\"10dmm\"; crown width=\"12dmm\" clumps=12 shape=round branching=normal; leaf length=\"4.5dmm\" width=\"3dmm\" }";

fn tile(stack: &str) -> String {
    format!(
        "dharmatiles version=\"1.0.0\"; tile size=\"1x1\" seed=3 {{\n    stack {{\n        base\n{stack}\n    }}\n}}\n"
    )
}

/// The highest earth vertex within `r` mm of the tile's middle.
fn earth_top(b: &Built, r: f64) -> f64 {
    let mut top = f64::MIN;
    for (t, m) in b.export.mesh.triangles.iter().zip(&b.materials) {
        if *m != Material::Earth {
            continue;
        }
        for &i in t {
            let p = b.export.mesh.positions[i as usize];
            if (p[0] - 17.5).hypot(p[1] - 17.5) <= r {
                top = top.max(p[2]);
            }
        }
    }
    top
}

#[test]
fn a_dirt_after_a_tree_is_built_after_it_and_banks_on_the_trunk() {
    let after = tile(&format!(
        "        dirt clods=none\n        {TREE}\n        dirt top=\"$bottom + [1dmm .. 1.2dmm]\" clods=none"
    ));
    let checked = check(&mem(&after), "t.dtile", &Options::default()).unwrap();
    // the base (the connector) is built last; the lift follows the last dirt, and the tree is between
    let order: Vec<Step> = checked.steps.clone();
    assert_eq!(
        order,
        vec![
            Step::Entry(0),
            Step::Entry(1),
            Step::Entry(2),
            Step::Lift,
            Step::Entry(3)
        ]
    );
    let with = build(&mem(&after), "t.dtile", &Options::default()).unwrap();
    let before = tile(&format!("        dirt clods=none\n        {TREE}"));
    let without = build(&mem(&before), "t.dtile", &Options::default()).unwrap();
    // beside the trunk the second dirt lies about a millimetre higher
    let (a, b) = (earth_top(&with, 6.0), earth_top(&without, 6.0));
    assert!(a > b + 0.6, "dirt banks on the trunk: {a} against {b}");
}

#[test]
fn a_tree_above_its_ground_is_refused_not_reordered() {
    let text = tile(&format!("        {TREE}\n        dirt"));
    let Err(e) = check(&mem(&text), "t.dtile", &Options::default()) else {
        panic!("a tree above its ground is refused");
    };
    assert_eq!(e[0].code, "dt::order");
    assert!(e[0].help.as_deref().unwrap().contains("`dirt`"));
}
