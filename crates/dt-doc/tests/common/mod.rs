//! The library's heights as the tests read them, and a few helpers over the
//! decoder.
#![allow(dead_code)]

use dt_doc::{DocError, Known, decode, model::Document};

/// The library's heights, as the program ships them (format 1).
pub const HEIGHTS: &str = include_str!("../../../../library/parts/heights.dtile");

/// What every document sees: the library's heights.
pub fn known() -> Known {
    let mut k = Known::new();
    k.add_heights("library/parts/heights.dtile", HEIGHTS)
        .unwrap_or_else(|e| panic!("{e}"));
    k
}

/// Decode a snippet; panics with the report if it fails.
pub fn snippet(text: &str) -> Document {
    decode("snippet.dtile", text, &known()).unwrap_or_else(|e| panic!("{text}\n{}", render(&e)))
}

/// Decode a snippet that must fail; returns the errors and the report.
pub fn fails(text: &str) -> (Vec<DocError>, String) {
    let e = decode("snippet.dtile", text, &known()).expect_err(text);
    let shown = render(&e);
    (e, shown)
}

/// miette's graphical report, without colour, as a reader sees it.
pub fn render(errors: &[DocError]) -> String {
    let handler =
        miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor())
            .with_width(88);
    let mut out = String::new();
    for e in errors {
        handler.render_report(&mut out, e).expect("render");
    }
    out
}

/// A report with miette's line wrapping undone, for `contains` checks.
pub fn flat(shown: &str) -> String {
    shown.split_whitespace().collect::<Vec<_>>().join(" ")
}
