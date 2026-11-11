//! The `dharmatiles version=…` header line through the real decoder: accepted
//! first in every kind of document, refused anywhere else it goes wrong, and
//! never an element.
#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use common::*;

const TILE: &str = "tile size=\"1x1\" seed=1 {\n    stack {\n        dirt\n    }\n}\n";

#[test]
fn a_header_is_accepted_and_changes_nothing() {
    let plain = snippet(TILE);
    assert!(plain.header.is_none());
    for h in [
        "dharmatiles version=0",
        "dharmatiles version=\"0.9\"",
        "dharmatiles version=\"0.9.0+120.gf9481c8.dirty\"",
    ] {
        let doc = snippet(&format!("{h}\n{TILE}"));
        assert_eq!(doc.header.as_ref().unwrap().format, 0, "{h}");
        assert_eq!(doc.definitions.len(), plain.definitions.len());
    }
    // a header alone, as a palette or a printer file may start
    assert!(snippet("dharmatiles version=0\n").definitions.is_empty());
}

#[test]
fn a_header_in_the_wrong_place_or_form_is_refused() {
    for (text, code) in [
        (format!("{TILE}dharmatiles version=0\n"), "dt::header-first"),
        (
            format!("dharmatiles version=0\ndharmatiles version=0\n{TILE}"),
            "dt::header-twice",
        ),
        (format!("dharmatiles\n{TILE}"), "dt::header-version"),
        (
            format!("dharmatiles version=1.0\n{TILE}"),
            "dt::header-version",
        ),
        (
            format!("dharmatiles version=\"a\"\n{TILE}"),
            "dt::header-version",
        ),
        (format!("dharmatiles version=0 up=1\n{TILE}"), "dt::header"),
        (
            format!("dharmatiles version=0 {{ x; }}\n{TILE}"),
            "dt::header",
        ),
        (
            format!("dharmatiles version=\"3.0.1\"\n{TILE}"),
            "dt::format-newer",
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
