//! The print list (`.dmap`): what it reads, and that a mistake is shown at
//! its text with a hint.

#![allow(clippy::unwrap_used)]

use dt_doc::map::{Group, Mode, decode};

const LIST: &str = r#"dharmatiles version="1.0.0"
map {
    about "A stream crossing."
    piece "lib:tiles/ground/1x1-grass" count=8
    piece p2 "lib:tiles/water/1x1-water-corridor+grass" count=4 seed=12 system="ol"
    piece "home:meadows/glade" print=skip
    piece "lib:tiles/walls/1x1-wall-stone" at="(4sq, 1sq)" rot=90 bottom="20dmm"
}
plates printer="bambu/x1c-ams" system="db" mode="multicolour" group="filament" gap="3mm" margin="5mm" turn="allowed"
"#;

fn errors(text: &str) -> Vec<(String, String)> {
    decode("t.dmap", text)
        .err()
        .unwrap()
        .into_iter()
        .map(|e| (e.message, e.help.unwrap_or_default()))
        .collect()
}

fn one(body: &str) -> (String, String) {
    let mut e = errors(&format!("dharmatiles version=1\n{body}\n"));
    assert_eq!(e.len(), 1, "{e:?}");
    e.remove(0)
}

#[test]
fn a_print_list_reads_pieces_and_plates() {
    let l = decode("t.dmap", LIST).unwrap();
    assert_eq!(l.about.as_deref(), Some("A stream crossing."));
    assert_eq!(l.pieces.len(), 4);
    let p = &l.pieces[0];
    assert_eq!(
        (p.tile.value.as_str(), p.count.value),
        ("lib:tiles/ground/1x1-grass", 8)
    );
    assert!(p.seed.is_none() && p.system.is_none() && p.print == dt_doc::map::PrintMode::Missing);
    let q = &l.pieces[1];
    assert_eq!(q.id.as_ref().unwrap().value, "p2");
    assert_eq!(q.seed.as_ref().unwrap().value, 12);
    assert_eq!(q.system.as_ref().unwrap().value, "ol");
    assert!(l.pieces[2].print == dt_doc::map::PrintMode::Skip);
    assert_eq!(l.pieces[3].rot.as_ref().unwrap().value, 90);
    assert_eq!(l.pieces[3].at.as_ref().unwrap().value, "(4sq, 1sq)");
    let pl = &l.plates;
    assert_eq!(pl.printer.as_ref().unwrap().value, "bambu/x1c-ams");
    assert_eq!(pl.mode.as_ref().unwrap().value, Mode::Multicolour);
    assert_eq!(pl.group.as_ref().unwrap().value, Group::Filament);
    assert_eq!(pl.gap.as_ref().unwrap().value, 3.0);
    assert_eq!(pl.margin.as_ref().unwrap().value, 5.0);
    assert!(pl.turn.as_ref().unwrap().value);
}

#[test]
fn every_word_of_the_plates_line_is_optional() {
    let l = decode(
        "t.dmap",
        "dharmatiles version=1\nmap {\n piece \"lib:tiles/ground/1x1-grass\"\n}\n",
    )
    .unwrap();
    assert!(l.plates.printer.is_none() && l.plates.mode.is_none() && l.plates.gap.is_none());
    assert_eq!(l.pieces[0].count.value, 1);
}

#[test]
fn mistakes_are_shown_at_their_text_with_a_hint() {
    let (m, h) = one("map {\n piece \"a\" cuont=2\n}");
    assert!(m.contains("no `cuont`") && h.contains("count"), "{m} {h}");
    let (m, _) = one("map {\n piece \"a\" count=0\n}");
    assert!(m.contains("count"));
    let (m, _) = one("map {\n piece\n}");
    assert!(m.contains("names its tile"));
    let (m, _) = one("map {\n piece \"a\" system=\"dx\"\n}");
    assert!(m.contains("not a system"));
    let (m, h) = one("map {\n piece \"a\" print=skp\n}");
    assert!(m.contains("not a print") && h.contains("skip"), "{m} {h}");
    let (m, _) = one("map {\n piece \"a\" rot=45\n}");
    assert!(m.contains("rot"));
    let (m, _) = one("map {\n pieces \"a\"\n}");
    assert!(m.contains("`piece` lines"));
    let (m, _) = one("map {\n grid cols=8\n}");
    assert!(m.contains("both"), "{m}");
    let (m, _) = one("map {\n piece \"a\" at=\"(2, 1)\"\n}");
    assert!(m.contains("unit"), "{m}");
    let (m, _) = one("map {\n piece \"a\" at=\"(2sq, 1sq)\" count=2\n}");
    assert!(m.contains("one piece"), "{m}");
    let (m, _) = one("map {\n piece \"a\" bottom=\"20\"\n}");
    assert!(m.contains("unit"), "{m}");
    let (m, h) = one("map {\n piece \"a\" height=\"20dmm\"\n}");
    assert!(
        m.contains("no `height`") && h.contains("`height` is now `bottom`"),
        "{m} {h}"
    );
    let (m, _) = one("map {\n let plinth \"20dmm\"\n}");
    assert!(m.contains("not a name"), "{m}");
    let (m, h) = one("map {\n piece \"a\"\n}\nplates printr=\"x\"");
    assert!(m.contains("no `printr`") && h.contains("printer"));
    let (m, _) = one("map {\n piece \"a\"\n}\nplates gap=3");
    assert!(m.contains("unit") || m.contains("gap"));
    let (m, _) = one("map {\n piece \"a\"\n}\nplates gap=\"3\"");
    assert!(m.contains("length"));
    let (m, _) = one("map {\n piece \"a\"\n}\nplates mode=\"colourful\"");
    assert!(m.contains("not a mode"));
    let (m, h) = one("map {\n piece \"a\"\n}\nplates {\n pin p1 plate=1 at=\"(0mm, 0mm)\"\n}");
    assert!(
        m.contains("no piece is called") && h.contains("id"),
        "{m} {h}"
    );
    let (m, _) = one("map {\n piece \"a\"\n}\nplates\nplates");
    assert!(m.contains("one plates line"));
    let (m, _) = one("tile size=\"1x1\" {\n}");
    assert!(m.contains("unknown top-level node"));
    let (m, _) = one("plates mode=\"single\"");
    assert!(m.contains("no `map`"));
}

#[test]
fn a_mistake_in_every_place_comes_together() {
    let e = errors(
        "dharmatiles version=1\nmap {\n piece \"a\" count=0\n piece \"b\" seed=-1\n piece\n}\nplates mode=\"x\" gap=\"9\"\n",
    );
    assert!(e.len() >= 5, "{e:?}");
}

#[test]
fn the_header_is_checked_as_in_every_document() {
    let e = decode("t.dmap", "map {\n piece \"a\"\n}\n").err().unwrap();
    assert_eq!(e.last().unwrap().code, dt_lang::codes::FORMAT_0);
    let e = decode("t.dmap", "dharmatiles version=2\nmap {\n piece \"a\"\n}\n")
        .err()
        .unwrap();
    assert_eq!(e[0].code, dt_lang::codes::FORMAT_NEWER);
    let e = decode("t.dmap", "dharmatiles version=1\nmap {{\n")
        .err()
        .unwrap();
    assert_eq!(e[0].code, dt_lang::codes::SYNTAX);
}

/// The edits of a print list as the tests say them: each one an [`Op`] through
/// the one engine.
mod edits {
    use dt_doc::edit::EditError;
    use dt_doc::map::{At, Want};
    pub use dt_doc::ops::Prop;
    use dt_doc::ops::{Kind, Op, apply};

    fn run(text: &str, op: &Op) -> Result<dt_doc::ops::Done, EditError> {
        apply(Kind::Map, text, op)
    }

    pub fn add_or_bump(text: &str, want: &Want) -> Result<(String, u32), EditError> {
        let d = run(text, &Op::AddOrBump(want))?;
        Ok((d.text, d.count.unwrap_or(0)))
    }
    pub fn give_ids(text: &str) -> Result<(String, bool), EditError> {
        run(text, &Op::GiveIds).map(|d| (d.text, d.gave_ids))
    }
    pub fn set_count(text: &str, id: &str, count: u32) -> Result<String, EditError> {
        run(text, &Op::SetCount { id, count }).map(|d| d.text)
    }
    pub fn remove_piece(text: &str, id: &str) -> Result<String, EditError> {
        run(text, &Op::Remove { id }).map(|d| d.text)
    }
    pub fn add_pieces(text: &str, lines: &[String]) -> Result<String, EditError> {
        run(text, &Op::Append(lines)).map(|d| d.text)
    }
    pub fn set_piece(
        text: &str,
        id: &str,
        edits: &[(&str, Option<Prop>)],
    ) -> Result<String, EditError> {
        run(text, &Op::SetPiece { id, edits }).map(|d| d.text)
    }
    pub fn duplicate_piece(
        text: &str,
        id: &str,
        new_id: &str,
        at: At,
    ) -> Result<String, EditError> {
        run(text, &Op::Duplicate { id, new_id, at }).map(|d| d.text)
    }
    pub fn set_pin(
        text: &str,
        id: &str,
        plate: u32,
        at: At,
        turn: bool,
    ) -> Result<String, EditError> {
        run(
            text,
            &Op::Pin {
                id,
                plate,
                at,
                turn,
            },
        )
        .map(|d| d.text)
    }
    pub fn clear_pin(text: &str, id: &str) -> Result<String, EditError> {
        run(text, &Op::Unpin { id }).map(|d| d.text)
    }
    pub fn clear_pins(text: &str) -> Result<String, EditError> {
        run(text, &Op::UnpinAll).map(|d| d.text)
    }
}

#[test]
fn a_piece_is_added_counted_and_taken_out_keeping_every_other_byte() {
    use dt_doc::map::{Want, piece_ids};
    use edits::{add_or_bump, give_ids, remove_piece, set_count};
    let text = "dharmatiles version=1\n// kept\nmap {\n    piece \"lib:tiles/ground/1x1-grass\" count=2 // two\n    piece \"home:glade\" seed=7\n}\nplates printer=\"sovol/sv06\"\n";
    let want = |tile, seed| Want {
        tile,
        seed,
        system: None,
        always: false,
    };
    // the same tile, seed and system: the count is raised
    let (t, n) = add_or_bump(text, &want("lib:tiles/ground/1x1-grass", None)).unwrap();
    assert_eq!(n, 3);
    assert!(
        t.contains("count=3") && t.contains("// two") && t.contains("// kept"),
        "{t}"
    );
    // another seed is another piece, added at the end of the map (and the editor's edit names them all)
    let (t, n) = add_or_bump(text, &want("lib:tiles/ground/1x1-grass", Some(9))).unwrap();
    assert_eq!(n, 1);
    assert!(
        t.contains("piece p3 \"lib:tiles/ground/1x1-grass\" count=1 seed=9\n}")
            && t.contains("piece p1 \"lib:tiles/ground/1x1-grass\" count=2")
            && t.contains("piece p2 \"home:glade\" seed=7"),
        "{t}"
    );
    assert_eq!(decode("l", &t).unwrap().pieces.len(), 3);
    // a tile with its .dtile still found; a seed already there counts up
    let (_, n) = add_or_bump(text, &want("home:glade.dtile", Some(7))).unwrap();
    assert_eq!(n, 2);
    // an empty map takes a first piece
    let empty = "dharmatiles version=1\nmap {\n}\n";
    let (t, _) = add_or_bump(empty, &want("home:a", None)).unwrap();
    assert_eq!(decode("l", &t).unwrap().pieces.len(), 1);
    // count and removal, by id: the ids are written first, so an unnamed piece is addressable
    let (named, gave) = give_ids(text).unwrap();
    assert!(gave && give_ids(&named).unwrap() == (named.clone(), false));
    assert!(
        named.contains("// two") && named.contains("// kept"),
        "{named}"
    );
    let ids = piece_ids(&decode("l", text).unwrap());
    assert_eq!(ids, ["p1", "p2"]);
    assert!(
        set_count(text, "p2", 4)
            .unwrap()
            .contains("piece p2 \"home:glade\" seed=7 count=4")
    );
    let t = set_count(&named, "p2", 4).unwrap();
    assert!(t.contains("seed=7") && t.contains("count=4"), "{t}");
    assert!(set_count(&named, "p1", 0).is_err() && set_count(&named, "p5", 2).is_err());
    let t = remove_piece(&named, "p1").unwrap();
    assert!(
        !t.contains("1x1-grass") && t.contains("home:glade") && t.contains("plates printer"),
        "{t}"
    );
    assert!(remove_piece(&named, "p7").is_err());
    // a list with a mistake is never edited
    assert!(
        add_or_bump(
            "dharmatiles version=1\nmap {\n piece\n}\n",
            &want("home:a", None)
        )
        .is_err()
    );
}

const MAP: &str = r#"dharmatiles version="1.0.0"

// A map, as the editor writes it.
map {
    about "A stream crossing."
    grid cols=8 rows=5
    let $plinth "20dmm"

    piece p1 "lib:tiles/ground/2x2-glade" at="(0sq, 0sq)" rot=0
    piece p2 "lib:tiles/water/1x1-water-corridor+grass" at="(2sq, 1sq)" rot=90 seed=3
    piece p3 "lib:tiles/cliffs/2x1-cliff" at="(4sq, 0sq)" rot=0 bottom="$wallHeight"
    piece p4 "lib:tiles/walls/1x1-wall-stone" at="(5sq, 1sq)" rot=0 bottom="$plinth + 2mm" // raised
    piece "lib:tiles/ground/1x1-grass" count=2
}
plates printer="bambu/x1c-ams" {
    pin p2 plate=1 at="(70mm, 5.5mm)" turn=90
}
"#;

#[test]
fn a_map_reads_its_grid_names_places_heights_and_pins() {
    use dt_doc::map::{HeightTerm, Len, parse_height};
    let l = decode("t.dmap", MAP).unwrap();
    let g = l.grid.unwrap();
    assert_eq!((g.cols, g.rows), (8, 5));
    assert_eq!(l.lets[0].name.value, "plinth");
    assert_eq!(l.lets[0].expr.value, "20dmm");
    let at = l.pieces[1].place.unwrap();
    assert_eq!((at.x, at.y), (Len::Sq(2.0), Len::Sq(1.0)));
    assert_eq!(l.pieces[1].rot.as_ref().unwrap().value, 90);
    assert!(l.pieces[4].place.is_none());
    let pin = &l.plates.pins[0];
    assert_eq!(
        (pin.id.value.as_str(), pin.plate.value, pin.turn),
        ("p2", 1, true)
    );
    let a = pin.at.unwrap().0;
    assert_eq!((a.x, a.y), (Len::Mm(70.0), Len::Mm(5.5)));
    // heights are sums of names and lengths
    let h = parse_height("$plinth + 2mm").unwrap();
    assert_eq!(h.terms.len(), 2);
    assert_eq!(h.terms[0].1, HeightTerm::Name("plinth".to_owned()));
    let named = |n: &str| match n {
        "plinth" => Ok(20.0),
        _ => Err(format!("no `${n}`")),
    };
    assert!((h.mm(35.0, &named).unwrap() - 22.0).abs() < 1e-9);
    // dmm scales with the system, mm does not
    let d = parse_height("33.1dmm").unwrap();
    assert!((d.mm(25.4, &named).unwrap() - 33.1 * 25.4 / 35.0).abs() < 1e-9);
    assert!(
        (parse_height("$plinth - 5mm")
            .unwrap()
            .mm(35.0, &named)
            .unwrap()
            - 15.0)
            .abs()
            < 1e-9
    );
    assert!(parse_height("$wall height").is_err());
}

#[test]
fn a_map_editor_writes_places_and_keeps_every_other_byte() {
    use dt_doc::map::{At, Len, next_id, piece_ids, placed_line};
    use edits::{
        Prop, add_pieces, clear_pin, clear_pins, duplicate_piece, give_ids, set_piece, set_pin,
    };
    let at = |x, y| At {
        x: Len::Sq(x),
        y: Len::Sq(y),
    };
    let mm = |x, y| At {
        x: Len::Mm(x),
        y: Len::Mm(y),
    };
    // move and turn one piece; a seed only when it is not the default
    let t = set_piece(
        MAP,
        "p2",
        &[
            ("at", Some(Prop::Str(at(3.0, 2.0).text()))),
            ("rot", Some(Prop::Int(180))),
            ("seed", None),
        ],
    )
    .unwrap();
    assert!(t.contains(r#"at="(3sq, 2sq)" rot=180"#), "{t}");
    assert!(!t.contains("seed=3"), "{t}");
    assert!(t.contains("// raised") && t.contains("// A map, as the editor writes it."));
    let l = decode("t.dmap", &t).unwrap();
    assert_eq!(l.pieces[1].rot.as_ref().unwrap().value, 180);
    // a new piece is added with its own id, and the next id is free
    let l0 = decode("t.dmap", MAP).unwrap();
    assert_eq!(next_id(&l0), "p5");
    let line = placed_line(
        "p5",
        "lib:tiles/ground/1x1-grass",
        at(1.0, 3.5),
        0,
        Some(12),
        Some("ol"),
    );
    assert_eq!(
        line,
        r#"piece p5 "lib:tiles/ground/1x1-grass" at="(1sq, 3.5sq)" rot=0 seed=12 system=ol"#
    );
    let t = add_pieces(MAP, &[line]).unwrap();
    let l = decode("t.dmap", &t).unwrap();
    assert_eq!(l.pieces.len(), 6);
    assert_eq!(l.pieces[5].place.unwrap().y, Len::Sq(3.5));
    // a duplicate has its own id and its own place; the original stays
    let t = duplicate_piece(MAP, "p3", "p9", at(6.0, 3.0)).unwrap();
    let l = decode("t.dmap", &t).unwrap();
    assert_eq!(l.pieces.len(), 6);
    assert_eq!(l.pieces[3].id.as_ref().unwrap().value, "p9");
    assert_eq!(l.pieces[3].bottom.as_ref().unwrap().value, "$wallHeight");
    assert_eq!(l.pieces[2].place.unwrap().x, Len::Sq(4.0));
    // a piece with no id is named as the ids will be written; the first edit writes them all
    let (t, gave) = give_ids(MAP).unwrap();
    assert!(gave);
    let l = decode("t.dmap", &t).unwrap();
    assert_eq!(l.pieces[4].id.as_ref().unwrap().value, "p5");
    assert_eq!(piece_ids(&l0)[4], "p5");
    assert!(t.contains("// raised") && t.contains("// A map, as the editor writes it."));
    let (same, gave) = give_ids(&t).unwrap();
    assert_eq!((same.as_str(), gave), (t.as_str(), false));
    // a hand-written id that is the next free one is skipped, not reused
    let odd = "dharmatiles version=1\nmap {\n piece \"a\"\n piece p1 \"b\"\n piece \"c\"\n}\n";
    assert_eq!(piece_ids(&decode("t", odd).unwrap()), ["p2", "p1", "p3"]);
    // pins: replaced, added inside the plates line, and taken away
    let t = set_pin(MAP, "p2", 2, mm(10.0, 20.0), false).unwrap();
    let l = decode("t.dmap", &t).unwrap();
    assert_eq!(l.plates.pins.len(), 1);
    assert_eq!(
        (l.plates.pins[0].plate.value, l.plates.pins[0].turn),
        (2, false)
    );
    let t = set_pin(&t, "p3", 1, mm(0.0, 0.0), true).unwrap();
    assert_eq!(decode("t.dmap", &t).unwrap().plates.pins.len(), 2);
    let t = clear_pin(&t, "p2").unwrap();
    let l = decode("t.dmap", &t).unwrap();
    assert_eq!(l.plates.pins.len(), 1);
    assert_eq!(l.plates.pins[0].id.value, "p3");
    assert_eq!(
        decode("t.dmap", &clear_pins(&t).unwrap())
            .unwrap()
            .plates
            .pins
            .len(),
        0
    );
    // no plates line yet: one is made
    let bare = "dharmatiles version=1\nmap {\n piece p1 \"a\" at=\"(0sq, 0sq)\"\n}\n";
    let t = set_pin(bare, "p1", 1, mm(1.0, 2.0), false).unwrap();
    assert_eq!(decode("t.dmap", &t).unwrap().plates.pins.len(), 1, "{t}");
}

#[test]
fn ids_are_unique_and_a_pin_names_one() {
    let (m, _) = one("map {\n piece p1 \"a\"\n piece p1 \"b\"\n}");
    assert!(m.contains("two pieces"), "{m}");
    let (m, _) = one(
        "map {\n piece p1 \"a\"\n}\nplates {\n pin p1 plate=1 at=\"(0mm, 0mm)\"\n pin p1 plate=2 at=\"(0mm, 0mm)\"\n}",
    );
    assert!(m.contains("pinned twice"), "{m}");
}
