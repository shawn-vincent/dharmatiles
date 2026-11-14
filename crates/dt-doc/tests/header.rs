//! The `dharmatiles version=…` header line through the real decoder: accepted
//! first in every kind of document, refused anywhere else it goes wrong, and
//! never an element.  A document older than the format the decoder reads is
//! refused, with help naming the release that carried it over.
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use common::*;

const TILE: &str = "tile size=\"1x1\" seed=1 {\n    stack {\n        dirt\n    }\n}\n";

#[test]
fn a_header_is_accepted_and_changes_nothing_else() {
    for h in [
        "dharmatiles version=1",
        "dharmatiles version=\"1.0\"",
        "dharmatiles version=\"1.0.0+120.gf9481c8.dirty\"",
    ] {
        let doc = snippet(&format!("{h}\n{TILE}"));
        assert_eq!(doc.header.as_ref().unwrap().format, 1, "{h}");
        assert_eq!(doc.definitions.len(), 1);
    }
    // a header alone, as a palette or a printer file may start
    assert!(snippet("dharmatiles version=1\n").definitions.is_empty());
}

#[test]
fn a_header_in_the_wrong_place_or_form_is_refused() {
    for (text, code) in [
        (
            format!("{TILE}dharmatiles version=1\n"),
            dt_lang::codes::HEADER_FIRST,
        ),
        (
            format!("dharmatiles version=1\ndharmatiles version=1\n{TILE}"),
            dt_lang::codes::HEADER_TWICE,
        ),
        (
            format!("dharmatiles\n{TILE}"),
            dt_lang::codes::HEADER_VERSION,
        ),
        (
            format!("dharmatiles version=1.0\n{TILE}"),
            dt_lang::codes::HEADER_VERSION,
        ),
        (
            format!("dharmatiles version=\"a\"\n{TILE}"),
            dt_lang::codes::HEADER_VERSION,
        ),
        (
            format!("dharmatiles version=1 up=1\n{TILE}"),
            dt_lang::codes::HEADER,
        ),
        (
            format!("dharmatiles version=1 {{ x; }}\n{TILE}"),
            dt_lang::codes::HEADER,
        ),
        (
            format!("dharmatiles version=\"3.0.1\"\n{TILE}"),
            dt_lang::codes::FORMAT_NEWER,
        ),
    ] {
        let (errs, shown) = fails(&text);
        assert!(errs.iter().any(|e| e.code == code), "{code}: {shown}");
        assert!(errs.iter().all(|e| e.help.is_some()), "{shown}");
    }
}

#[test]
fn a_newer_format_is_the_only_complaint() {
    // the rest of the file may be in a language this program doesn't know
    let (errs, shown) = fails("dharmatiles version=2\ntile size=\"1x1\" { bogus }\n");
    assert_eq!(errs.len(), 1, "{shown}");
    assert_eq!(
        errs[0].message,
        "this tile is format 2; this dharmatiles reads up to 1"
    );
}

#[test]
fn the_decoder_refuses_format_0_and_names_the_release_that_migrates() {
    // no header is format 0; so is `version=0`, in any of its spellings
    for text in [
        TILE.to_owned(),
        format!("dharmatiles version=0\n{TILE}"),
        format!("dharmatiles version=\"0.9.0+120.gf9481c8\"\n{TILE}"),
        "dharmatiles version=0\n".to_owned(),
    ] {
        let (errs, shown) = fails(&text);
        let e = errs
            .iter()
            .find(|e| e.code == dt_lang::codes::FORMAT_0)
            .unwrap_or_else(|| panic!("no dt::format-0 for\n{text}\n{shown}"));
        let help = e.help.as_deref().unwrap();
        assert!(help.contains("v1.0.0"), "{help}");
        assert!(flat(&shown).contains("format 0"), "{shown}");
        // only the format is complained of: the old forms are not read
        assert_eq!(errs.len(), 1, "{shown}");
    }
}
