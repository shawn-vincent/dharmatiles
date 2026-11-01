//! Edits through the kdl document API keep every byte outside the edited
//! value (plan.html R6: "comments and layout survive the edit byte for
//! byte outside the edited node").
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use common::*;
use dt_doc::edit::{Key, set_value};
use dt_doc::model::Value;

/// The one changed stretch: (before, after) with the common prefix and
/// suffix removed.
fn changed<'a>(old: &'a str, new: &'a str) -> (&'a str, &'a str) {
    let p = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let s = old[p..]
        .bytes()
        .rev()
        .zip(new[p..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    (&old[p..old.len() - s], &new[p..new.len() - s])
}

/// Lines that differ, as (line number, old, new).
fn changed_lines(old: &str, new: &str) -> Vec<(usize, String, String)> {
    let (a, b): (Vec<_>, Vec<_>) = (old.lines().collect(), new.lines().collect());
    assert_eq!(a.len(), b.len(), "an edit must not add or remove lines");
    a.iter()
        .zip(&b)
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .map(|(i, (x, y))| (i + 1, (*x).to_owned(), (*y).to_owned()))
        .collect()
}

#[test]
fn every_file_prints_back_unchanged() {
    for (name, text) in ALL {
        let doc = kdl::KdlDocument::parse(text).unwrap();
        assert_eq!(doc.to_string(), text, "{name}");
    }
}

#[test]
fn grass_length_edit_touches_only_the_value() {
    let (name, text) = GRASS;
    let new = set_value(
        text,
        &["define long-grass", "stack", "lawn"],
        Key::Prop("length"),
        "9..15",
    )
    .unwrap();
    assert_eq!(changed(text, &new), ("8..14", "9..15"));
    let lines = changed_lines(text, &new);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].1.replace("8..14", "9..15"), lines[0].2);

    // The edited file still decodes, with the new value.
    let doc = dt_doc::decode(name, &new, &dt_doc::Known::new()).unwrap();
    let call = &doc.definition("long-grass").unwrap().stack[0];
    let Some(Value::Expr(x)) = call.setting("length") else {
        panic!("length")
    };
    assert_eq!(x.to_string(), "9..15");
}

#[test]
fn leaf_length_edit_in_a_group() {
    let (_, text) = GRASS_TREE;
    let new = set_value(
        text,
        &["tile grass-tree", "stack", "tree", "leaf"],
        Key::Prop("length"),
        5.5,
    )
    .unwrap();
    assert_eq!(changed(text, &new), ("4", "5"));
    let lines = changed_lines(text, &new);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0]
            .2
            .contains("leaf length=5.5 width=3.0           // mm"),
        "{}",
        lines[0].2
    );
    // The slashdashed debug node and every comment survive.
    assert!(new.contains("/- debug attractors=#true           // slashdash: switched off"));
}

#[test]
fn cliff_height_edit_in_child_form() {
    let (name, text) = CLIFF_RUN;
    let new = set_value(
        text,
        &["tile cliff-run", "stack", "cliff", "height"],
        Key::Arg(0),
        1.2,
    )
    .unwrap();
    let lines = changed_lines(text, &new);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0].2,
        "            height 1.2            // 35 mm above the foot: the run form's cap"
    );
    // The multi-line `about` string survives too.
    assert!(new.contains("        Rock band against the north edge. Both ends finish in the\n"));
    dt_doc::decode(name, &new, &known()).unwrap();
}

#[test]
fn two_edits_compose() {
    let (_, text) = WALL_CORNER;
    let a = set_value(
        text,
        &["tile wall-fieldstone-corner", "stack", "wall"],
        Key::Prop("seed"),
        15,
    )
    .unwrap();
    let b = set_value(
        &a,
        &["tile wall-fieldstone-corner", "stack", "wall", "stones"],
        Key::Prop("bay"),
        "6..18",
    )
    .unwrap();
    let lines = changed_lines(text, &b);
    assert_eq!(lines.len(), 2);
    assert!(lines[0].2.contains("seed=15 {"));
    assert!(lines[1].2.contains("bay=\"6..18\" joint=0"));
}
