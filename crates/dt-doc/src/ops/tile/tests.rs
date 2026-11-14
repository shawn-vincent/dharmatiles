//! The tile operations: every edit keeps each byte it does not name, and an id the text does not have is refused.

use kdl::{KdlDocument, KdlValue};

use super::super::{Done, EditError, Kind, Op, Place, apply};
use crate::error::Fix;
use crate::nodeid;

fn run(text: &str, op: &Op) -> Result<Done, EditError> {
    apply(Kind::Tile, text, op)
}

fn go(text: &str, op: &Op) -> String {
    run(text, op).expect("the edit is made").text
}

fn set(text: &str, id: &str, name: &str, v: impl Into<KdlValue>) -> String {
    go(
        text,
        &Op::SetControl {
            id,
            name,
            value: Some(v.into()),
        },
    )
}

fn unset(text: &str, id: &str, name: &str) -> Result<Done, EditError> {
    run(
        text,
        &Op::SetControl {
            id,
            name,
            value: None,
        },
    )
}

fn take(text: &str, id: &str) -> String {
    go(text, &Op::Take { id })
}

fn at(text: &str, word: &str) -> usize {
    text.find(word).unwrap()
}

fn parses(t: &str) {
    KdlDocument::parse(t).unwrap_or_else(|e| panic!("{e}\n{t}"));
}

const SRC: &str = "// top\nthing a=1   b=\"x\" { // note\n    kid 4.5 // keep\n    kid 6\n}\n";

// ---- ids -----------------------------------------------------------------------------------------

#[test]
fn a_node_is_named_by_its_path_its_name_or_its_id() {
    let t = "define grass { stack { blade } }\ntile size=\"1x1\" {\n    line shore { through \"0,0\" }\n    region pool { fill \"1,1\" }\n    stack {\n        dirt\n        rock id=old-oak\n        rock\n        scatter count=2 { rock size=\"6..10\" }\n        scatter\n    }\n}\n";
    let ids: Vec<String> = nodeid::table(t)
        .unwrap()
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert_eq!(
        ids,
        [
            "define grass",
            "define grass/stack",
            "define grass/stack/blade",
            "tile",
            "tile/line shore",
            "tile/line shore/through",
            "tile/region pool",
            "tile/region pool/fill",
            "tile/stack",
            "tile/stack/dirt",
            "tile/stack/rock id=old-oak",
            "tile/stack/rock#2",
            "tile/stack/scatter",
            "tile/stack/scatter/rock",
            "tile/stack/scatter#2",
        ]
    );
    for n in nodeid::table(t).unwrap() {
        assert_eq!(nodeid::id_at(t, n.at).unwrap(), n.id);
    }
}

#[test]
fn a_named_node_keeps_its_id_when_the_text_around_it_moves() {
    let a = "tile {\n    line shore { through \"0,0\" }\n    stack {\n        rock id=a\n        rock id=b\n    }\n}\n";
    let b = format!(
        "// moved\n\n{}",
        a.replace(
            "rock id=a\n        rock id=b",
            "rock id=b\n        rock id=a"
        )
    );
    for id in [
        "tile/line shore",
        "tile/stack/rock id=a",
        "tile/stack/rock id=b",
    ] {
        let one = nodeid::table(a)
            .unwrap()
            .into_iter()
            .find(|n| n.id == id)
            .unwrap();
        let two = nodeid::table(&b)
            .unwrap()
            .into_iter()
            .find(|n| n.id == id)
            .unwrap();
        assert_eq!(
            a[one.at..].lines().next(),
            b[two.at..].lines().next(),
            "{id}"
        );
    }
}

#[test]
fn the_decoders_paths_are_the_ids() {
    let t = "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack {\n        dirt\n        scatter count=2 { rock }\n        scatter count=3 { boulder }\n    }\n}\n";
    let doc = crate::decode("t.dtile", t, &crate::Known::new()).expect("reads");
    let tile = doc.tile().expect("a tile");
    let table = nodeid::table(t).unwrap();
    for e in &tile.stack {
        let id = e.path.join("/");
        let n = table
            .iter()
            .find(|n| n.id == id)
            .unwrap_or_else(|| panic!("{id}"));
        assert_eq!(n.at, e.span.offset(), "{id}");
        for c in &e.children {
            let id = c.path.join("/");
            assert!(table.iter().any(|n| n.id == id), "{id}");
        }
    }
}

#[test]
fn an_id_the_text_does_not_have_is_refused() {
    for id in ["tile/stack/rock#9", "nope", "tile/stack/rock id=gone", ""] {
        assert!(matches!(
            run(SRC, &Op::Take { id }),
            Err(EditError::NoNode(_))
        ));
        assert!(matches!(
            run(SRC, &Op::Hide { id }),
            Err(EditError::NoNode(_))
        ));
        assert!(matches!(unset(SRC, id, "a"), Err(EditError::NoNode(_))));
    }
    let t = "tile { stack { rock; tree } }";
    assert!(matches!(
        run(
            t,
            &Op::Insert {
                place: Place::Before("tile/stack/gone"),
                node: "x"
            }
        ),
        Err(EditError::NoNode(_))
    ));
    assert!(matches!(
        run(
            t,
            &Op::Move {
                id: "tile/stack/rock",
                before: Some("tile/stack/gone")
            }
        ),
        Err(EditError::NoNode(_))
    ));
    // an edit of text that is not KDL is refused, never made
    assert!(matches!(
        run("tile {", &Op::Take { id: "tile" }),
        Err(EditError::Parse(_))
    ));
}

// ---- controls ------------------------------------------------------------------------------------

#[test]
fn a_control_is_set_where_it_is_written() {
    let t = "tile {\n    stack {\n        dirt\n        scatter count=2 { rock size=\"6..10\" }\n        scatter\n    }\n}\n";
    let out = set(t, "tile/stack/scatter/rock", "size", "8..12");
    assert!(out.contains("rock size=\"8..12\" }"), "{out}");
    let out = set(t, "tile/stack/dirt", "wear", "worn");
    assert!(out.contains("        dirt wear=worn\n"), "{out}");
    let out = unset(t, "tile/stack/scatter", "count").unwrap().text;
    assert!(out.contains("        scatter { rock"), "{out}");
    // the answer says which node, now
    let d = run(
        t,
        &Op::SetControl {
            id: "tile/stack/dirt",
            name: "wear",
            value: Some("worn".into()),
        },
    )
    .unwrap();
    assert_eq!(d.id.as_deref(), Some("tile/stack/dirt"));
    assert!(d.text[d.at.unwrap()..].starts_with("dirt"));
}

#[test]
fn a_control_is_changed_in_place_and_nothing_else() {
    let out = set(SRC, "thing", "a", 2);
    assert_eq!(out, SRC.replace("a=1", "a=2"));
    let out = set(SRC, "thing", "kid", 9);
    assert_eq!(out, SRC.replace("kid 4.5", "kid 9"));
    let out = set(SRC, "thing", "b", "9..15");
    assert_eq!(out, SRC.replace("b=\"x\"", "b=\"9..15\""));
    let out = set(SRC, "thing", "b", "round");
    assert_eq!(out, SRC.replace("b=\"x\"", "b=round"));
    let out = set(SRC, "thing", "z", "1,1");
    assert_eq!(out, SRC.replace("b=\"x\" {", "b=\"x\" z=\"1,1\" {"));
}

#[test]
fn a_control_is_taken_out_in_either_form() {
    let out = unset(SRC, "thing", "a").unwrap().text;
    assert_eq!(
        out,
        SRC.replace("a=1", "").replace("thing    b", "thing   b")
    );
    let out = unset(SRC, "thing", "kid").unwrap().text;
    assert_eq!(out, SRC.replace("    kid 4.5 // keep\n", ""));
    // the parent line's comment stays
    assert!(out.contains("{ // note\n    kid 6\n}"), "{out}");
    assert!(matches!(
        unset(SRC, "thing", "q"),
        Err(EditError::NoEntry(_))
    ));
}

#[test]
fn an_argument_is_replaced_in_place() {
    let out = go(
        SRC,
        &Op::SetArg {
            id: "thing/kid",
            index: 0,
            value: 5.25.into(),
        },
    );
    assert_eq!(out, SRC.replace("kid 4.5", "kid 5.25"));
    let out = go(
        SRC,
        &Op::SetArg {
            id: "thing/kid#2",
            index: 0,
            value: 7.into(),
        },
    );
    assert_eq!(out, SRC.replace("kid 6", "kid 7"));
    assert!(matches!(
        run(
            SRC,
            &Op::SetArg {
                id: "thing",
                index: 4,
                value: 1.into()
            }
        ),
        Err(EditError::NoEntry(_))
    ));
}

#[test]
fn a_sub_control_is_a_property_of_its_groups_line() {
    let t = "tile {\n    stack {\n        wall at=north {\n            height full\n        }\n    }\n}\n";
    let wall = "tile/stack/wall";
    // no group line yet: it is made, as a child line
    let a = set(t, wall, "stones/height", "3dmm");
    assert!(a.contains("            stones height=\"3dmm\"\n"), "{a}");
    // a group line already written: the property goes on it
    let b = set(&a, wall, "stones/length", "9dmm");
    assert!(b.contains("stones height=\"3dmm\" length=\"9dmm\""), "{b}");
    // and changes in place
    let c = set(&b, wall, "stones/height", "4dmm");
    assert!(c.contains("stones height=\"4dmm\" length=\"9dmm\""), "{c}");
    // taken out, the line stays while it holds another, and goes with the last
    let d = unset(&c, wall, "stones/height").unwrap().text;
    assert!(
        d.contains("stones length=\"9dmm\"") && !d.contains("height=\"4"),
        "{d}"
    );
    let e = unset(&d, wall, "stones/length").unwrap().text;
    assert!(!e.contains("stones"), "{e}");
    assert!(e.contains("height full"), "{e}");
    parses(&e);
    assert!(matches!(
        unset(t, wall, "stones/height"),
        Err(EditError::NoEntry(_))
    ));
}

// ---- inserting and taking out ---------------------------------------------------------------------

const T: &str = "tile size=\"1x1\" {\n    line shore { through \"0,0.38\" east }\n    stack {\n        dirt at=bank // hi\n        rock at=\"0.5,0.5\"\n    }\n}\n";

fn insert(text: &str, place: Place, node: &str) -> Done {
    run(text, &Op::Insert { place, node }).expect("the node is written")
}

#[test]
fn a_new_entry_goes_at_the_end_of_the_stack() {
    let d = insert(T, Place::Stack, "boulder at=\"0.2,0.3\"");
    assert!(
        d.text
            .contains("        rock at=\"0.5,0.5\"\n        boulder at=\"0.2,0.3\"\n    }"),
        "{}",
        d.text
    );
    assert!(d.text[d.at.unwrap()..].starts_with("boulder"));
    assert_eq!(d.id.as_deref(), Some("tile/stack/boulder"));
    let d = insert(T, Place::Stack, "scatter at=pool count=3 {\n    rock\n}");
    assert!(
        d.text
            .contains("        scatter at=pool count=3 {\n            rock\n        }\n    }"),
        "{}",
        d.text
    );
    assert_eq!(d.id.as_deref(), Some("tile/stack/scatter"));
}

#[test]
fn a_line_goes_before_the_stack() {
    let d = insert(T, Place::Tile, "line path { through west east }");
    assert!(
        d.text
            .contains("}\n    line path { through west east }\n    stack {"),
        "{}",
        d.text
    );
    assert_eq!(d.id.as_deref(), Some("tile/line path"));
}

#[test]
fn a_tile_without_a_stack_gets_one() {
    let d = insert("tile size=\"1x1\" {\n}\n", Place::Stack, "rock");
    assert_eq!(
        d.text,
        "tile size=\"1x1\" {\n    stack {\n        rock\n    }\n}\n"
    );
    assert_eq!(d.id.as_deref(), Some("tile/stack/rock"));
    let d = insert("tile {}", Place::Stack, "rock");
    assert!(
        d.text[d.at.unwrap()..].starts_with("rock"),
        "{} {:?}",
        d.text,
        d.at
    );
    parses(&d.text);
}

#[test]
fn a_new_entry_goes_between_two_others_above_the_comments_on_the_second() {
    let d = insert(S, Place::Before("tile/stack/rock"), "boulder");
    assert!(
        d.text
            .contains("dirt at=bank // soil\n\n        boulder\n        // stones\n        rock"),
        "{}",
        d.text
    );
    assert_eq!(d.id.as_deref(), Some("tile/stack/boulder"));
    parses(&d.text);
}

#[test]
fn a_node_comes_out_with_its_line() {
    let t = take(T, "tile/stack/dirt");
    assert!(t.contains("    stack {\n        rock at"), "{t}");
    let t = take(T, "tile/line shore");
    assert!(t.starts_with("tile size=\"1x1\" {\n    stack {"), "{t}");
    let one = "tile { stack { rock; tree } }";
    assert_eq!(take(one, "tile/stack/rock"), "tile { stack { tree } }");
}

/// The part of `a` and `b` that differs: what is left of each between the longest shared start and end.
fn changed<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    let pre = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    let suf = a.as_bytes()[pre..]
        .iter()
        .rev()
        .zip(b.as_bytes()[pre..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    (&a[pre..a.len() - suf], &b[pre..b.len() - suf])
}

const MESSY: &str = "// c\ntile   size=\"1x1\"\tseed=3 { // note\n\n  stack {\n\tdirt  at=bank   wear=worn;  rock size=\"6dmm\" // end\n   grass\n  }\n}\n";

#[test]
fn setting_a_control_to_what_it_is_changes_nothing_in_any_layout() {
    let doc = KdlDocument::parse(MESSY).unwrap();
    let table = nodeid::nodes_of(&doc);
    let mut made = 0;
    for node in &table {
        let kdl = nodeid::locate(&doc, &node.id).unwrap();
        for e in kdl.entries().iter().filter(|e| e.name().is_some()) {
            let name = e.name().unwrap().value();
            let out = set(MESSY, &node.id, name, e.value().clone());
            assert_eq!(out, MESSY, "{} {name}", node.id);
            made += 1;
        }
    }
    assert_eq!(made, 5);
}

#[test]
fn an_edit_touches_only_the_bytes_it_names() {
    // a changed value: just the value
    let out = set(MESSY, "tile/stack/rock", "size", "9dmm");
    assert_eq!(changed(MESSY, &out), ("6", "9"));
    // a control added: just the words, after the last entry of the line
    let out = set(MESSY, "tile/stack/grass", "wear", "worn");
    assert_eq!(changed(MESSY, &out), ("", " wear=worn"));
    // a control taken out: it and the space before it
    let out = unset(MESSY, "tile/stack/dirt", "wear").unwrap().text;
    assert_eq!(changed(MESSY, &out), ("   wear=worn", ""));
    // a node taken out: its own line, whole
    let out = take(MESSY, "tile/stack/grass");
    assert_eq!(out, MESSY.replace("   grass\n", ""));
    // hidden: three bytes
    let out = go(
        MESSY,
        &Op::Hide {
            id: "tile/stack/rock",
        },
    );
    assert_eq!(changed(MESSY, &out), ("", "/- "));
    // the document keeps reading as one with the node gone
    parses(&out);
}

#[test]
fn a_node_is_hidden_where_it_stands() {
    let t = go(
        T,
        &Op::Hide {
            id: "tile/stack/rock",
        },
    );
    assert_eq!(t, T.replace("rock at", "/- rock at"));
    parses(&t);
    // the hidden node is not a node of the text any more
    assert!(matches!(
        run(
            &t,
            &Op::Take {
                id: "tile/stack/rock"
            }
        ),
        Err(EditError::NoNode(_))
    ));
}

const REGION: &str = "tile size=\"1x1\" {\n    region pool {\n        boundedBy shore\n        fill \"0.5,0.2\"\n        within 6 of=center\n    }\n}\n";

#[test]
fn a_clause_is_written_last_in_its_region() {
    let d = insert(REGION, Place::In("tile/region pool"), "except crag");
    assert!(
        d.text
            .contains("        within 6 of=center\n        except crag\n    }"),
        "{}",
        d.text
    );
    assert_eq!(d.id.as_deref(), Some("tile/region pool/except"));
}

#[test]
fn a_clause_goes_into_a_region_on_one_line() {
    let src = "tile size=\"1x1\" {\n    region a { within 4 of=center }\n}\n";
    let d = insert(src, Place::In("tile/region a"), "except b");
    parses(&d.text);
    assert!(d.text.contains("except b"));
}

#[test]
fn the_bucket_comes_out_together() {
    let t = go(
        REGION,
        &Op::TakeChildren {
            id: "tile/region pool",
            names: &["boundedBy", "fill"],
        },
    );
    assert_eq!(
        t,
        "tile size=\"1x1\" {\n    region pool {\n        within 6 of=center\n    }\n}\n"
    );
}

#[test]
fn the_header_line_is_left_exactly_as_written() {
    let head = "dharmatiles   version=\"0.9.0+5.gf9481c8.dirty\" // as written\n";
    let text = format!("{head}{T}");
    let t = insert(&text, Place::Stack, "boulder at=\"0.2,0.3\"").text;
    assert!(t.starts_with(head), "{t}");
    let d = insert(&text, Place::Tile, "line path { through west east }");
    assert!(d.text.starts_with(head), "{}", d.text);
    // the node taken out again leaves the text as it was
    let t = take(&d.text, d.id.as_deref().unwrap());
    assert_eq!(t, text);
    let t = take(&text, "tile/stack/rock");
    assert!(t.starts_with(head), "{t}");
    let t = go(
        &text,
        &Op::Rename {
            id: "tile/line shore",
            new: "bank",
        },
    );
    assert!(t.starts_with(head), "{t}");
}

// ---- names ---------------------------------------------------------------------------------------

#[test]
fn renames_a_line_or_region_and_what_names_it() {
    let t = "tile size=\"1x1\" {\n    line shore wobble=5 { through \"0,0.38\" east }\n    line trail { through south north }\n    region pool { boundedBy shore trail; fill \"0.5,0.25\" }\n    region bank { boundedBy \"shore\"; fill \"0.5,0.75\" }\n    stack {\n        dirt at=bank // the bank\n        wall at=shore\n        rock at=\"bank\"\n    }\n}\n";
    let rename = |id: &str, new: &str| run(t, &Op::Rename { id, new });
    let r = rename("tile/line shore", "beach").unwrap().text;
    assert_eq!(
        r,
        t.replace("line shore", "line beach")
            .replace("By shore", "By beach")
            .replace("By \"shore\"", "By \"beach\"")
            .replace("at=shore", "at=beach")
    );
    let r = rename("tile/region bank", "meadow").unwrap().text;
    assert_eq!(
        r,
        t.replace("region bank", "region meadow")
            .replace("at=bank", "at=meadow")
            .replace("at=\"bank\"", "at=\"meadow\"")
    );
    for (id, new) in [
        ("tile/line shore", "trail"),
        ("tile/line shore", "north"),
        ("tile/line shore", "2nd"),
    ] {
        assert!(
            matches!(rename(id, new), Err(EditError::BadName(_))),
            "{new}"
        );
    }
    assert!(matches!(
        rename("tile/region nope", "x"),
        Err(EditError::NoNode(_))
    ));
    assert!(matches!(
        rename("tile/stack", "x"),
        Err(EditError::Cannot(_))
    ));
}

// ---- moving --------------------------------------------------------------------------------------

const S: &str = "tile size=\"1x1\" {\n    stack {\n        base\n        // the bank\n        dirt at=bank // soil\n\n        // stones\n        rock at=\"0.5,0.5\"\n        scatter count=3 {\n            pebble\n        }\n        grass\n    }\n}\n";

fn mv(text: &str, id: &str, before: Option<&str>) -> Result<Done, EditError> {
    run(text, &Op::Move { id, before })
}

#[test]
fn a_node_moves_down_with_its_comments_and_children() {
    let d = mv(S, "tile/stack/dirt", Some("tile/stack/scatter")).unwrap();
    assert_eq!(
        d.text,
        "tile size=\"1x1\" {\n    stack {\n        base\n\n        // stones\n        rock at=\"0.5,0.5\"\n        // the bank\n        dirt at=bank // soil\n        scatter count=3 {\n            pebble\n        }\n        grass\n    }\n}\n"
    );
    assert!(d.text[d.at.unwrap()..].starts_with("dirt at=bank"));
    assert_eq!(d.id.as_deref(), Some("tile/stack/dirt"));
    parses(&d.text);
}

#[test]
fn a_node_with_children_moves_up_and_to_the_end() {
    let d = mv(S, "tile/stack/scatter", Some("tile/stack/dirt")).unwrap();
    assert!(
        d.text.contains(
            "base\n        scatter count=3 {\n            pebble\n        }\n        // the bank\n        dirt at=bank // soil\n"
        ),
        "{}",
        d.text
    );
    assert!(d.text[d.at.unwrap()..].starts_with("scatter count=3 {"));
    let d = mv(S, "tile/stack/rock", None).unwrap();
    assert!(
        d.text
            .ends_with("        grass\n        // stones\n        rock at=\"0.5,0.5\"\n    }\n}\n"),
        "{}",
        d.text
    );
    assert!(d.text[d.at.unwrap()..].starts_with("rock at"));
    parses(&d.text);
}

#[test]
fn a_blank_line_is_not_doubled_where_the_node_left() {
    let t = "tile {\n    stack {\n        base\n\n        dirt\n\n        rock\n        grass\n    }\n}\n";
    let d = mv(t, "tile/stack/dirt", None).unwrap();
    assert_eq!(
        d.text,
        "tile {\n    stack {\n        base\n\n        rock\n        grass\n        dirt\n    }\n}\n"
    );
}

#[test]
fn a_node_already_in_place_changes_nothing() {
    let d = mv(S, "tile/stack/rock", Some("tile/stack/scatter")).unwrap();
    assert_eq!(d.text, S);
    assert_eq!(d.at, Some(at(S, "rock")));
    assert_eq!(mv(S, "tile/stack/grass", None).unwrap().text, S);
    assert_eq!(
        mv(S, "tile/stack/rock", Some("tile/stack/rock"))
            .unwrap()
            .text,
        S
    );
}

#[test]
fn a_hidden_entry_stays_where_it_is_and_hides_only_itself() {
    let t = "tile {\n    stack {\n        dirt\n        /- grass\n        rock\n        tree\n    }\n}\n";
    let d = mv(t, "tile/stack/dirt", Some("tile/stack/tree")).unwrap();
    assert_eq!(
        d.text,
        "tile {\n    stack {\n        /- grass\n        rock\n        dirt\n        tree\n    }\n}\n"
    );
    assert!(d.text[d.at.unwrap()..].starts_with("dirt"));
    let d = mv(t, "tile/stack/tree", Some("tile/stack/rock")).unwrap();
    assert_eq!(
        d.text,
        "tile {\n    stack {\n        dirt\n        /- grass\n        tree\n        rock\n    }\n}\n"
    );
}

#[test]
fn what_cannot_be_moved_is_refused_in_words() {
    let one = "tile { stack { rock; tree } }";
    assert!(matches!(
        mv(one, "tile/stack/rock", None),
        Err(EditError::Cannot(_))
    ));
    // a target that is not a sibling
    assert!(matches!(
        mv(S, "tile/stack/dirt", Some("tile/stack/scatter/pebble")),
        Err(EditError::Cannot(_))
    ));
}

// ---- fixes ---------------------------------------------------------------------------------------

fn fix(text: &str, was: &str, with: &str) -> Fix {
    Fix {
        offset: text.find(was).unwrap(),
        len: was.len(),
        was: was.into(),
        replacement: with.into(),
        label: format!("Change to {with}"),
        moved: None,
    }
}

#[test]
fn fixes_are_laid_in_from_the_end_and_only_where_the_text_matches() {
    let t = "tile { stack { dirt; rock } }";
    let fixes = [fix(t, "rock", "boulder"), fix(t, "dirt", "ground")];
    let d = run(t, &Op::Fix(&fixes)).unwrap();
    assert_eq!(d.text, "tile { stack { ground; boulder } }");
    assert_eq!(d.mended.unwrap().applied.len(), 2);
    let stale = Fix {
        offset: 15,
        len: 4,
        was: "xxxx".into(),
        ..fix(t, "dirt", "q")
    };
    assert!(matches!(
        run(t, &Op::Fix(&[stale])),
        Err(EditError::Cannot(_))
    ));
}

#[test]
fn overlapping_fixes_wait_and_a_repeat_is_made_once() {
    let t = "tile { stack { abcdef } }";
    let fixes = [fix(t, "abc", "1"), fix(t, "abc", "1"), fix(t, "bcd", "2")];
    let d = run(t, &Op::Fix(&fixes)).unwrap();
    assert_eq!(d.text, "tile { stack { 1def } }");
    assert_eq!(d.mended.unwrap().deferred, 1);
}

// ---- reads ---------------------------------------------------------------------------------------

#[test]
fn names_come_from_nodes_not_from_text() {
    let text = "// define fake\ndefine meadow { stack { dirt } }\nuse \"lib:parts/trees\" oak elm\n\
        tile size=\"1x1\" {\n about \"a line Foo\"\n line shore { through \"0,0\" }\n\
        /- region ghost { fill \"1,1\" }\n region pool { fill \"1,1\" }\n point p \"1,1\"\n}\n";
    let n = nodeid::names_in(text).unwrap();
    assert_eq!(n.defs, ["meadow", "oak", "elm"]);
    assert_eq!(n.lines, ["shore"]);
    assert_eq!(n.regions, ["pool"]);
    assert_eq!(n.points, ["p"]);
    assert!(nodeid::names_in("define {").is_err());
}

#[test]
fn a_nodes_properties_and_where_their_values_are() {
    let p = nodeid::props_at(T, at(T, "rock")).unwrap();
    assert_eq!(p[0].0, "at");
    assert_eq!(p[0].1, "\"0.5,0.5\"");
    assert_eq!(&T[p[0].2[0]..p[0].2[0] + p[0].2[1]], "\"0.5,0.5\"");
}
