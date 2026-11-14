//! The inventory document: the log, what it adds up to, and what is written.

#![allow(clippy::unwrap_used)]

use dt_doc::inventory::{
    Colour, Entry, Identity, Shelf, decode, gone_line, new_text, print_line, renamed_line,
};

const LOG: &str = r#"dharmatiles version="1.0.0"

// a log
inventory {
    print "lib:tiles/ground/1x1-grass" seed=97 system=db colour=multi result=ok on="2026-11-09" hash="3c1f9a4e" from="crossing/plate-1"
    print "lib:tiles/ground/1x1-grass" seed=12 system=db colour=multi result=ok on="2026-11-09" hash="3c1f9a4e"
    print "lib:tiles/walls/1x1-wall-stone" seed=14 system=db colour=single result=ok on="2026-11-09" hash="a07d22b1"
    print "lib:tiles/cliffs/2x1-cliff" seed=4 system=db colour=multi result=failed on="2026-11-09" note="top layer stringing"
    print "lib:tiles/cliffs/2x1-cliff" seed=4 system=db colour=multi result=ok on="2026-11-10"
    gone "lib:tiles/ground/1x1-grass" seed=12 system=db colour=multi on="2026-11-11" note="dropped on the floor"
}
"#;

fn id(tile: &str, seed: u64, system: &str, colour: Colour) -> Identity {
    Identity {
        tile: tile.to_owned(),
        seed,
        system: system.to_owned(),
        colour,
    }
}

#[test]
fn the_log_adds_up_and_nothing_is_stored() {
    let inv = decode("inventory.dinv", LOG).unwrap();
    assert_eq!(inv.entries.len(), 6);
    let grass97 = id("lib:tiles/ground/1x1-grass", 97, "db", Colour::Multi);
    let grass12 = id("lib:tiles/ground/1x1-grass", 12, "db", Colour::Multi);
    let cliff = id("lib:tiles/cliffs/2x1-cliff", 4, "db", Colour::Multi);
    assert_eq!(inv.have(&grass97), 1);
    assert_eq!(inv.have(&grass12), 0, "gone takes it away");
    assert_eq!(
        inv.have(&cliff),
        1,
        "the failed print is kept and does not count; the reprint does"
    );
    let p = inv.pieces();
    let c = p.iter().find(|p| p.id == cliff).unwrap();
    assert_eq!((c.printed, c.failed, c.have, c.gone), (1, 1, 1, 0));
    let g = p.iter().find(|p| p.id == grass12).unwrap();
    assert_eq!((g.printed, g.have, g.gone), (1, 0, 1));
    assert_eq!(
        p.iter().find(|p| p.id == grass97).unwrap().hash.as_deref(),
        Some("3c1f9a4e")
    );
}

#[test]
fn a_piece_is_tile_seed_system_and_colour_and_two_prints_are_two() {
    let t = "lib:tiles/ground/1x1-grass";
    let text = format!(
        "dharmatiles version=1\ninventory {{\n{}\n{}\n{}\n{}\n}}\n",
        print_line(
            &id(t, 1, "db", Colour::Single),
            true,
            "2026-11-09",
            None,
            None,
            None
        ),
        print_line(
            &id(t, 1, "db", Colour::Single),
            true,
            "2026-11-09",
            None,
            None,
            None
        ),
        print_line(
            &id(t, 1, "ol", Colour::Single),
            true,
            "2026-11-09",
            None,
            None,
            None
        ),
        print_line(
            &id(t, 1, "db", Colour::Multi),
            true,
            "2026-11-09",
            None,
            None,
            None
        ),
    );
    let inv = decode("i.dinv", &text).unwrap();
    assert_eq!(inv.have(&id(t, 1, "db", Colour::Single)), 2);
    assert_eq!(
        inv.have(&id(t, 1, "ol", Colour::Single)),
        1,
        "DungeonBlocks and OpenLOCK never count for each other"
    );
    assert_eq!(inv.have(&id(t, 1, "db", Colour::Multi)), 1);
    assert_eq!(inv.have(&id(t, 2, "db", Colour::Single)), 0);
    let shelf = Shelf::of(&inv);
    assert_eq!(shelf.have(&id(t, 1, "db", Colour::Single)), 2);
    assert_eq!(
        shelf.other_colour(&id(t, 1, "ol", Colour::Multi)),
        1,
        "a multicolour piece wanted where only single colour is owned"
    );
    assert_eq!(shelf.other_colour(&id(t, 7, "ol", Colour::Multi)), 0);
}

#[test]
fn a_loss_of_what_you_never_had_takes_nothing_from_nothing() {
    let text = format!(
        "dharmatiles version=1\ninventory {{\n{}\n}}\n",
        gone_line(&id("home:a", 3, "db", Colour::Single), "2026-11-09", None)
    );
    let inv = decode("i.dinv", &text).unwrap();
    assert_eq!(inv.have(&id("home:a", 3, "db", Colour::Single)), 0);
}

#[test]
fn a_rename_carries_the_prints_to_the_new_path() {
    let text = format!(
        "dharmatiles version=1\ninventory {{\n{}\n{}\n{}\n{}\n}}\n",
        print_line(
            &id("home:old", 5, "db", Colour::Multi),
            true,
            "2026-11-09",
            Some("aabbccdd"),
            None,
            None
        ),
        print_line(
            &id("home:new", 5, "db", Colour::Multi),
            true,
            "2026-11-09",
            None,
            None,
            None
        ),
        renamed_line("home:old", "home:new", "2026-11-10"),
        print_line(
            &id("home:new", 6, "db", Colour::Multi),
            true,
            "2026-11-11",
            None,
            None,
            None
        ),
    );
    let inv = decode("i.dinv", &text).unwrap();
    assert_eq!(
        inv.have(&id("home:new", 5, "db", Colour::Multi)),
        2,
        "the old path's print joined the new one"
    );
    assert_eq!(inv.have(&id("home:old", 5, "db", Colour::Multi)), 0);
    assert_eq!(inv.have(&id("home:new", 6, "db", Colour::Multi)), 1);
    assert_eq!(
        inv.pieces()
            .iter()
            .find(|p| p.id.seed == 5)
            .unwrap()
            .hash
            .as_deref(),
        Some("aabbccdd")
    );
}

#[test]
fn a_line_is_read_back_as_written_even_with_odd_text() {
    let i = id("home:with \"quotes\"", 9, "ol", Colour::Single);
    let l = print_line(
        &i,
        false,
        "2026-11-09",
        Some("00112233"),
        Some("kit/plate-2"),
        Some("said \"no\"\\yes"),
    );
    let text = new_text("1.0.0", &[l]);
    let inv = decode("i.dinv", &text).unwrap();
    match &inv.entries[0] {
        Entry::Print {
            id, ok, note, from, ..
        } => {
            assert_eq!(id, &i);
            assert!(!ok);
            assert_eq!(note.as_deref(), Some("said \"no\"\\yes"));
            assert_eq!(from.as_deref(), Some("kit/plate-2"));
        }
        other => panic!("{other:?}"),
    }
}

/// The log's one edit, through the one engine.
fn append(text: &str, lines: &[String]) -> Result<String, dt_doc::edit::EditError> {
    dt_doc::ops::apply(
        dt_doc::ops::Kind::Inventory,
        text,
        &dt_doc::ops::Op::Append(lines),
    )
    .map(|d| d.text)
}

#[test]
fn appending_keeps_every_other_byte_and_the_comments() {
    let line = print_line(
        &id("home:x", 1, "db", Colour::Single),
        true,
        "2026-11-12",
        None,
        None,
        None,
    );
    let out = append(LOG, std::slice::from_ref(&line)).unwrap();
    assert!(out.starts_with("dharmatiles version=\"1.0.0\"\n\n// a log\ninventory {"));
    assert!(out.contains("note=\"dropped on the floor\"\n    print \"home:x\""));
    assert!(out.trim_end().ends_with('}'));
    let inv = decode("i.dinv", &out).unwrap();
    assert_eq!(inv.entries.len(), 7);
    // a one-line inventory, and an empty one with no braces
    let one = "dharmatiles version=1\ninventory { }\n";
    let o1 = append(one, std::slice::from_ref(&line)).unwrap();
    assert_eq!(decode("i.dinv", &o1).unwrap().entries.len(), 1);
    let bare = "dharmatiles version=1\ninventory\n";
    let o2 = append(bare, &[line]).unwrap();
    assert_eq!(decode("i.dinv", &o2).unwrap().entries.len(), 1);
}

#[test]
fn nothing_is_added_to_a_file_that_is_not_understood() {
    let bad = LOG.replace("seed=97", "seed=\"x\"");
    assert!(
        append(
            &bad,
            &["gone \"x\" seed=1 system=db colour=single on=\"2026-11-12\"".to_owned()]
        )
        .is_err()
    );
}

fn mistakes(text: &str) -> Vec<(String, String)> {
    let e = decode("i.dinv", text).unwrap_err();
    e.iter()
        .map(|d| (d.message.clone(), d.help.clone().unwrap_or_default()))
        .collect()
}

#[test]
fn mistakes_point_at_their_text_with_a_hint() {
    let head = "dharmatiles version=1\ninventory {\n";
    let m = mistakes(&format!(
        "{head} print \"a\" system=db colour=single result=ok on=\"2026-11-09\"\n}}\n"
    ));
    assert!(m[0].0.contains("needs `seed`"), "{m:?}");
    let m = mistakes(&format!(
        "{head} print \"a\" seed=1 system=dg colour=single result=ok on=\"2026-11-09\"\n}}\n"
    ));
    assert!(
        m[0].0.contains("not a system") && m[0].1.contains("db"),
        "{m:?}"
    );
    let m = mistakes(&format!(
        "{head} print \"a\" seed=1 system=db colour=single result=fine on=\"2026-11-09\"\n}}\n"
    ));
    assert!(m[0].0.contains("not a result"), "{m:?}");
    let m = mistakes(&format!(
        "{head} print \"a\" seed=1 system=db colour=single result=ok on=\"yesterday\"\n}}\n"
    ));
    assert!(m[0].0.contains("date"), "{m:?}");
    let m = mistakes(&format!(
        "{head} prnt \"a\" seed=1 system=db colour=single result=ok on=\"2026-11-09\"\n}}\n"
    ));
    assert!(m[0].1.contains("print"), "{m:?}");
    let m = mistakes(&format!(
        "{head} print \"a\" seed=1 system=db colour=single result=ok on=\"2026-11-09\" colur=x\n}}\n"
    ));
    assert!(m[0].1.contains("colour"), "{m:?}");
    let m = mistakes(&format!("{head} renamed \"a\"\n}}\n"));
    assert!(m[0].0.contains("needs `to`"), "{m:?}");
    let m = mistakes("dharmatiles version=1\nprint \"a\"\n");
    assert!(m[0].0.contains("unknown top-level node"), "{m:?}");
    let m = mistakes("dharmatiles version=1\n");
    assert!(m[0].0.contains("no `inventory`"), "{m:?}");
    let m = mistakes("inventory {\n}\n");
    assert!(
        m.iter().any(|(m, _)| m.contains("format")),
        "a document with no header is not format 1: {m:?}"
    );
    let m = mistakes("dharmatiles version=1\ninventory {\n print \"a\" seed=1 system=db\n");
    assert!(!m.is_empty(), "KDL that does not parse is said");
}
