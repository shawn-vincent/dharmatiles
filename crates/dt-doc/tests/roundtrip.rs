//! Every library file prints back byte for byte, and edits keep every
//! byte outside the edited node (plan.html R6: "comments and layout
//! survive the edit byte for byte outside the edited node").
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use common::*;
use dt_doc::edit::{EditError, Key, insert_property, remove_control, set_control, set_value};
use dt_doc::model::{Form, Value};

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

/// Lines that differ, as (line number, old, new); line counts must match.
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

fn redecode(name: &str, text: &str) -> dt_doc::model::Document {
    dt_doc::decode(name, text, &known()).unwrap_or_else(|e| panic!("{}", render(&e)))
}

#[test]
fn every_file_prints_back_unchanged() {
    for (name, text) in ALL {
        let doc = kdl::KdlDocument::parse(text).unwrap();
        assert_eq!(doc.to_string(), text, "{name}");
    }
}

#[test]
fn a_noop_edit_is_byte_identical_for_every_entry() {
    // Setting each control to the text it already has changes nothing,
    // for every entry path the decoder reports, in every tile.
    for (name, text) in TILES {
        let doc = redecode(name, text);
        for e in doc.tile().unwrap().entries() {
            let path: Vec<&str> = e.path.iter().map(String::as_str).collect();
            for s in e.controls.iter().filter(|s| s.form == Form::Property) {
                let Some(v) = &s.value else { continue };
                let raw = &text[v.span.offset()..v.span.offset() + v.span.len()];
                let kv: kdl::KdlValue = match raw.parse::<i128>() {
                    Ok(i) => i.into(),
                    Err(_) => match raw.parse::<f64>() {
                        Ok(f) => f.into(),
                        Err(_) => raw.trim_matches('"').into(),
                    },
                };
                let out = set_control(text, &path, &s.name.value, kv).unwrap();
                assert_eq!(out, text, "{name} {path:?} {}", s.name.value);
            }
        }
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
    assert_eq!(changed(text, &new), ("5..9", "9..15"));
    assert_eq!(changed_lines(text, &new).len(), 1);
    let doc = redecode(name, &new);
    let call = &doc.definition("long-grass").unwrap().stack[0];
    let Some(Value::Num(n)) = call.value("length") else {
        panic!("length")
    };
    assert_eq!(n.expr.to_string(), "9..15");
}

#[test]
fn leaf_length_edit_in_a_group() {
    let (name, text) = GRASS_TREE;
    let new = set_control(
        text,
        &["tile grass-tree", "stack", "tree", "leaf"],
        "length",
        5.5,
    )
    .unwrap();
    assert_eq!(changed(text, &new), ("4", "5"));
    let lines = changed_lines(text, &new);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].2, "            leaf length=5.5 width=3");
    redecode(name, &new);
}

#[test]
fn child_form_edit_stays_a_child_line() {
    let (name, text) = WALL_CORNER;
    let path = ["tile wall-fieldstone-corner", "stack", "wall"];
    let new = set_control(text, &path, "thickness", 6.5).unwrap();
    let lines = changed_lines(text, &new);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].2, "            thickness 6.5");
    // A word, in child form, comment kept.
    let new = set_control(text, &path, "coping", "on-edge").unwrap();
    let lines = changed_lines(text, &new);
    assert_eq!(
        lines[0].2,
        "            coping on-edge                    // or: on-edge"
    );
    let doc = redecode(name, &new);
    let wall = &doc.tile().unwrap().stack[1];
    assert_eq!(wall.control("coping").unwrap().form, Form::Child);
}

#[test]
fn insertion_goes_on_the_line_as_a_property() {
    let (name, text) = GRASS_TREE;
    let path = ["tile grass-tree", "stack", "tree"];
    let new = set_control(text, &path, "at", "1,1").unwrap();
    let lines = changed_lines(text, &new);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0].2,
        "        tree height=40 at=\"1,1\" {               // anywhere on the tile; or at=\"1,1\""
    );
    let doc = redecode(name, &new);
    let tree = &doc.tile().unwrap().stack[2];
    assert!(tree.place.at.is_some());

    // On a line with only a name and a comment.
    let new = insert_property(text, &["tile grass-tree", "stack", "ground"], "level", 4).unwrap();
    assert_eq!(
        changed_lines(text, &new)[0].2,
        "        ground level=4                          // flat, at the ground level (5 mm)"
    );
    redecode(name, &new);

    // Into a group line.
    let new = insert_property(
        text,
        &["tile grass-tree", "stack", "tree", "crown"],
        "clump-size",
        6,
    )
    .unwrap();
    assert!(
        changed_lines(text, &new)[0]
            .2
            .ends_with("branching=normal clump-size=6")
    );
    redecode(name, &new);

    // A control already written, in either form, is set, not inserted.
    assert_eq!(
        insert_property(text, &path, "height", 50),
        Err(EditError::Exists("height".into()))
    );
    assert_eq!(
        insert_property(text, &path, "leaf", 1),
        Err(EditError::Exists("leaf".into()))
    );
}

#[test]
fn insertion_into_the_second_of_two_same_named_entries() {
    let (name, text) = PATH_ROCKS;
    let doc = redecode(name, text);
    let second = &doc.tile().unwrap().stack[1];
    let path: Vec<&str> = second.path.iter().map(String::as_str).collect();
    let new = set_control(text, &path, "bumps", 1).unwrap();
    let lines = changed_lines(text, &new);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0]
            .2
            .contains("dirt along=trail level=\"ground - 1\" bumps=1")
    );
    redecode(name, &new);
}

#[test]
fn removal_in_either_form() {
    let (name, text) = WALL_CORNER;
    let path = ["tile wall-fieldstone-corner", "stack", "wall"];
    // A property on the line.
    let new = remove_control(text, &path, "seed").unwrap();
    assert_eq!(new, text.replace(" seed=15", ""));
    redecode(name, &new);
    // A child line, with the comment block above the next line kept.
    let new = remove_control(text, &path, "coping").unwrap();
    assert_eq!(new.lines().count(), text.lines().count() - 1);
    assert_eq!(
        new,
        text.replace(
            "            coping loose                    // or: on-edge\n",
            ""
        )
    );
    redecode(name, &new);
    // The first child, whose leading text holds the comment above it.
    let new = remove_control(text, &path, "height").unwrap();
    assert_eq!(
        new,
        text.replace(
            "            height full                     // up to the wall-top level (33.1 mm)\n",
            ""
        )
    );
    assert!(matches!(
        remove_control(text, &path, "ruin"),
        Err(EditError::NoEntry(_))
    ));
}

#[test]
fn edits_compose() {
    let (name, text) = CLIFF_RUN;
    let a = set_control(text, &["tile cliff-run", "stack", "cliff"], "height", 40).unwrap();
    let b = set_control(
        &a,
        &["tile cliff-run", "stack", "scatter#2"],
        "per-square",
        3,
    )
    .unwrap();
    let c = set_control(
        &b,
        &["tile cliff-run", "stack", "scatter", "stone"],
        "mostly",
        "even",
    )
    .unwrap();
    let lines = changed_lines(text, &c);
    assert_eq!(lines.len(), 3);
    assert!(lines[0].2.contains("cliff along=north height=40 {"));
    assert!(lines[1].2.contains("{ stone size=\"1..6\" mostly=even }"));
    assert!(lines[2].2.contains("per-square=3 { boulder"));
    // The multi-line `about` string survives.
    assert!(c.contains("        Rock band against the north edge. Both ends finish in the\n"));
    redecode(name, &c);
}
