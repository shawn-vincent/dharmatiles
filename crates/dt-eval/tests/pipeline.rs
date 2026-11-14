//! The pipeline end to end on small tiles: `check` plans, `build` builds
//! what was planned and the audit gate judges it, and a plan or a region
//! that is refused stops both with the same word.

#![allow(clippy::unwrap_used)]

use dt_eval::plan::Step;
use dt_eval::{Memory, Options, build, check};

fn mem(text: &str) -> Memory {
    Memory([("t.dtile".to_owned(), text.to_owned())].into())
}

fn tile(body: &str) -> String {
    format!("dharmatiles version=1\ntile size=\"1x1\" seed=5 {{\n{body}\n}}\n")
}

/// Drafts: coarser cells, a fraction of the time, the same plan.
fn draft() -> Options {
    Options {
        draft: true,
        ..Options::default()
    }
}

#[test]
fn check_plans_the_stack_in_the_order_it_is_written_and_build_builds_that_plan() {
    let text = tile(
        "    stack {\n        base\n        dirt\n        rock size=\"8dmm\" at=\"(0.3sq, 0.3sq)\"\n        rock size=\"6dmm\" at=\"(0.7sq, 0.6sq)\"\n    }",
    );
    // a full build: the gate judges a build, not a draft
    let opts = Options::default();
    let c = check(&mem(&text), "t.dtile", &opts).unwrap();
    let t = c.tile.as_ref().unwrap();
    // the stack's entries are built in the order listed, and the ground is
    // lifted before the solids stand on it
    let entries: Vec<usize> = c
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::Entry(i) => Some(*i),
            Step::Lift => None,
        })
        .collect();
    let mut listed = entries.clone();
    listed.sort_unstable();
    assert_eq!(entries, listed, "{:?}", c.steps);
    assert_eq!(
        entries.len(),
        t.stack.len() + 1,
        "the connector is the last step"
    );
    let lift = c.steps.iter().position(|s| *s == Step::Lift).unwrap();
    let first_rock = c
        .steps
        .iter()
        .position(|s| matches!(s, Step::Entry(i) if t.stack[*i].what.value == "rock"))
        .unwrap();
    assert!(lift < first_rock, "{:?}", c.steps);

    // build agrees with check about the stack it built, and the gate holds
    let b = build(&mem(&text), "t.dtile", &opts).unwrap();
    assert_eq!(
        b.report
            .stack
            .iter()
            .map(|n| n.label.clone())
            .collect::<Vec<_>>(),
        t.names(&opts.system)
            .iter()
            .map(|n| n.label.clone())
            .collect::<Vec<_>>()
    );
    assert!(b.audit.failures(&opts.printer).is_empty(), "{:?}", b.audit);
    // and every triangle belongs to a listed entry
    assert_eq!(b.report.entries.len(), b.materials.len());
}

#[test]
fn a_plan_that_is_refused_is_refused_by_check_and_by_build_in_the_same_words() {
    // a tree stands on the ground, and the ground is listed below it
    let text = tile(
        "    stack {\n        base\n        tree at=\"(0.5sq, 0.5sq)\" top=\"$bottom + 30dmm\"\n        dirt\n    }",
    );
    let opts = draft();
    let c = check(&mem(&text), "t.dtile", &opts).err().unwrap();
    let b = build(&mem(&text), "t.dtile", &opts).err().unwrap();
    assert_eq!(c[0].code, dt_lang::codes::ORDER, "{c:?}");
    assert_eq!(
        (c[0].code, &c[0].message, &c[0].help),
        (b[0].code, &b[0].message, &b[0].help)
    );
}

#[test]
fn regions_that_name_each_other_are_a_mistake_at_the_region_not_an_empty_one() {
    let text = tile(
        "    region a { within \"4dmm\" of=b }\n    region b { within \"4dmm\" of=a }\n    stack {\n        base\n        dirt at=a\n    }",
    );
    let e = check(&mem(&text), "t.dtile", &draft()).err().unwrap();
    assert!(e.iter().any(|d| d.code == dt_lang::codes::REGION), "{e:?}");
    assert!(
        e.iter().all(|d| d.code != dt_lang::codes::EMPTY_REGION),
        "{e:?}"
    );
}

#[test]
fn a_region_is_made_once_and_every_stage_sees_the_same_one() {
    // the pond's mask is the empty-region check's, the region tree's, the
    // solve's and the build's: building twice from one check's text is the
    // same mesh, and the tree holds the region the document wrote
    let text = tile(
        "    line shore { through \"(0sq, 0.5sq)\" \"(1sq, 0.5sq)\" }\n    region pool { boundedBy shore; fill \"(0.5sq, 0.8sq)\" }\n    stack {\n        base\n        dirt\n        water at=pool\n    }",
    );
    let opts = draft();
    let c = check(&mem(&text), "t.dtile", &opts).unwrap();
    let tree = c.regions.as_ref().unwrap();
    assert!(
        tree.nodes
            .iter()
            .any(|n| n.label == "pool" && n.mask.as_ref().is_some_and(|m| m.count() > 0)),
        "the pool covers ground"
    );
    let a = build(&mem(&text), "t.dtile", &opts).unwrap();
    let b = build(&mem(&text), "t.dtile", &opts).unwrap();
    assert_eq!(a.export.mesh.content_hash(), b.export.mesh.content_hash());
}
