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
    piece "lib:tiles/walls/1x1-wall-stone" at="(4sq, 1sq)" rot=90 height="20dmm"
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
    assert_eq!((p.tile.value.as_str(), p.count.value), ("lib:tiles/ground/1x1-grass", 8));
    assert!(p.seed.is_none() && p.system.is_none() && !p.skip);
    let q = &l.pieces[1];
    assert_eq!(q.id.as_ref().unwrap().value, "p2");
    assert_eq!(q.seed.as_ref().unwrap().value, 12);
    assert_eq!(q.system.as_ref().unwrap().value, "ol");
    assert!(l.pieces[2].skip);
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
    let (_, h) = one("map {\n grid cols=8 rows=5\n}");
    assert!(h.contains("map view"));
    let (m, h) = one("map {\n piece \"a\"\n}\nplates printr=\"x\"");
    assert!(m.contains("no `printr`") && h.contains("printer"));
    let (m, _) = one("map {\n piece \"a\"\n}\nplates gap=3");
    assert!(m.contains("unit") || m.contains("gap"));
    let (m, _) = one("map {\n piece \"a\"\n}\nplates gap=\"3\"");
    assert!(m.contains("length"));
    let (m, _) = one("map {\n piece \"a\"\n}\nplates mode=\"colourful\"");
    assert!(m.contains("not a mode"));
    let (m, h) = one("map {\n piece \"a\"\n}\nplates {\n pin p1 plate=1\n}");
    assert!(m.contains("settings only") && h.contains("pins"));
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
    assert_eq!(e.last().unwrap().code, "dt::format-0");
    let e = decode("t.dmap", "dharmatiles version=2\nmap {\n piece \"a\"\n}\n")
        .err()
        .unwrap();
    assert_eq!(e[0].code, "dt::format-newer");
    let e = decode("t.dmap", "dharmatiles version=1\nmap {{\n").err().unwrap();
    assert_eq!(e[0].code, "dt::syntax");
}
