//! The sheet shows every control of an element, written or not: an unwritten
//! one shows what a build uses, from where.  This runs every element of the
//! schema with nothing written and refuses a control that would show nothing
//! (or "not set"), so a new control cannot arrive without its default, the
//! value its element works out, or the words for what leaving it out means.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;

use dt_doc::schema::{Control, Element};
use dt_eval::unwritten::unwritten;
use dt_eval::{Options, check};

fn tile(stack: &str) -> String {
    format!(
        "dharmatiles version=\"1.0.0\"; tile size=\"2x2\" seed=3 {{\n region mid {{ within \"8dmm\" of=center }}\n stack {{\n{stack}\n}}\n}}\n"
    )
}

/// One stack that holds the element (and what it stands on), written with
/// nothing but what it must have.
const STACKS: &[(&str, &str)] = &[
    ("dirt", "dirt"),
    ("grass", "dirt\n grass"),
    ("reeds", "dirt\n reeds"),
    ("water", "water"),
    ("rock", "dirt\n rock at=mid"),
    ("stone", "dirt\n stone at=mid"),
    ("boulder", "dirt\n boulder at=mid"),
    ("pebble", "dirt\n pebble at=mid"),
    ("flower", "dirt\n flower at=mid"),
    ("tree", "dirt\n tree at=mid"),
    ("wall", "dirt\n wall at=\"north\""),
    ("flagstones", "flagstones"),
    ("planks", "planks"),
    ("cliff", "cliff at=mid"),
    ("scatter", "dirt\n scatter at=mid count=2 { rock }"),
];

/// The scatters that are not rocks only: each kind of scatter works out its
/// own `perSquare`, `gap` and `reach`.
const SCATTERS: &[&str] = &[
    "dirt\n scatter at=mid perSquare=2 { rock }",
    "dirt\n scatter at=mid count=2 { flower }",
    "dirt\n scatter at=mid count=2 { tree }",
    "dirt\n scatter at=mid count=2 { rock; flower }",
];

/// Every path that is a row of the sheet: a control with a value of its own
/// that is not required and not written, groups only by their sub-controls.
fn rows(c: &'static Control, path: &str, out: &mut Vec<String>) {
    for s in c.subs {
        rows(s, &format!("{path}/{}", s.name), out);
    }
    if !c.is_group() && !c.required {
        out.push(path.to_owned());
    }
}

fn run(stack: &str, seen: &mut BTreeSet<String>, absent: &mut BTreeSet<String>) {
    let text = tile(stack);
    let mem = common::memory(&[("t.dtile", &text)]);
    let c =
        check(&mem, "t.dtile", &Options::default()).unwrap_or_else(|e| panic!("{stack}: {e:#?}"));
    let t = c.tile.as_ref().unwrap();
    let solved = c.solved.as_ref().unwrap();
    for (k, e) in t.stack.iter().enumerate() {
        let name = e.what.value.as_str();
        let top = solved.bodies.iter().find(|b| b.entry == k).map(|b| &b.top);
        let got = unwritten(name, &e.controls, &e.children, top)
            .unwrap_or_else(|why| panic!("{stack}: {why}"));
        let element: &'static Element = dt_doc::schema::element(name).unwrap();
        let mut want = Vec::new();
        for c in element.controls {
            rows(c, c.name, &mut want);
        }
        // a written control is not an unwritten one
        want.retain(|p| {
            !e.controls
                .iter()
                .any(|c| c.origin == dt_eval::doc::Origin::Written && &c.name == p)
        });
        for path in &want {
            let one: Vec<_> = got.iter().filter(|u| &u.control == path).collect();
            assert_eq!(
                one.len(),
                1,
                "{name}: `{path}` shows once where it is not written (shows {})",
                one.len()
            );
            let u = one[0];
            assert!(!u.text.trim().is_empty(), "{name}: `{path}` shows nothing");
            assert!(
                !u.text.to_lowercase().contains("not set"),
                "{name}: `{path}` says it is not set"
            );
            seen.insert(format!("{name} {path}"));
            if u.from.is_empty() {
                absent.insert(format!("{name} {path}: {}", u.text));
            }
        }
        // and nothing is shown that is not a control
        for u in &got {
            assert!(
                want.contains(&u.control),
                "{name}: `{}` is no row",
                u.control
            );
        }
    }
}

#[test]
fn every_control_shows_a_value_or_says_why_not() {
    let mut seen = BTreeSet::new();
    let mut absent = BTreeSet::new();
    for (name, stack) in STACKS {
        run(stack, &mut seen, &mut absent);
        assert!(
            seen.iter().any(|s| s.starts_with(&format!("{name} "))),
            "`{name}` was not run through the sheet"
        );
    }
    for stack in SCATTERS {
        run(stack, &mut seen, &mut absent);
    }
    // every element of the schema was run
    for e in dt_doc::elements::ELEMENTS {
        assert!(
            STACKS.iter().any(|(n, _)| *n == e.name),
            "`{}` has no stack in this test",
            e.name
        );
    }
    println!("controls that say what leaving them out means:");
    for a in &absent {
        println!("  {a}");
    }
}

/// An unwritten wall shows its style's numbers, and writing the style moves
/// them: one source (the wall's own table), read two ways.
#[test]
fn a_walls_unwritten_controls_follow_its_style() {
    let of = |stack: &str, control: &str| -> (String, String) {
        let text = tile(&format!("dirt\n {stack}"));
        let mem = common::memory(&[("t.dtile", &text)]);
        let c = check(&mem, "t.dtile", &Options::default()).unwrap();
        let t = c.tile.as_ref().unwrap();
        let e = t.stack.iter().find(|e| e.what.value == "wall").unwrap();
        let got = unwritten("wall", &e.controls, &e.children, None).unwrap();
        let u = got.iter().find(|u| u.control == control).unwrap();
        (u.text.clone(), u.from.clone())
    };
    assert_eq!(
        of("wall at=\"north\"", "stones/height"),
        ("5.5dmm .. 8.5dmm".to_owned(), "style cut-stone".to_owned())
    );
    assert_eq!(
        of("wall at=\"north\" style=brick", "stones/height"),
        ("2.5dmm .. 3dmm".to_owned(), "style brick".to_owned())
    );
    assert_eq!(
        of("wall at=\"north\" style=regular-brick", "bond").0,
        "running"
    );
    assert_eq!(of("wall at=\"north\"", "bond").0, "irregular");
    // a texture written moves the corners' rounding and says so
    assert_eq!(
        of(
            "wall at=\"north\" { stones texture=worn }",
            "edges/rounding"
        )
        .1,
        "stones texture worn"
    );
}
