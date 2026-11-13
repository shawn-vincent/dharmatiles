//! The header line: `dharmatiles version=1`, the first node of a document
//! of any kind (a tile, a part, a printer, a palette).
//!
//! Only the major part of the version is the document's format.  A document
//! with no header is format 0, which this program no longer reads (the
//! decoder refuses it, `dt::format-0`, and names the release that can carry
//! it over).  The header is not an element or a definition: nothing but
//! this module reads it, and edits leave it as written.

use std::sync::Arc;

use kdl::{KdlDocument, KdlNode, KdlValue};
use miette::{NamedSource, SourceSpan};

use crate::decode::{syntax, value_span};
use crate::error::{DocError, Src};

/// The node's name.
pub const NODE: &str = "dharmatiles";

/// The document format this program reads: the only one.  A document of an
/// older format (no header, or `version=0`) is refused with help to run
/// `dharmatiles migrate`; a newer one is refused too.
pub const FORMAT: u32 = 1;

/// The format this program reads, for `--version` and the About box:
/// its major version, `1.x`.
#[must_use]
pub fn formats() -> String {
    format!("{FORMAT}.x")
}

/// The version a document the program writes is stamped with: the program's own
/// when its major is the format, else the format's first release with the
/// program's build details (a program before 1.0.0 writes format 1 with its own
/// build details).
#[must_use]
pub fn stamp(app_version: &str) -> String {
    let major: Option<u32> = app_version
        .split(['.', '+'])
        .next()
        .and_then(|m| m.parse().ok());
    if major == Some(FORMAT) {
        return app_version.to_owned();
    }
    match app_version.split_once('+') {
        Some((_, build)) => format!("{FORMAT}.0.0+{build}"),
        None => format!("{FORMAT}.0.0"),
    }
}

/// A document's header, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The version as written: `1`, `1.0`, `1.0.0+5.gf9481c8.dirty`.
    pub version: String,
    /// Its major part: the format.
    pub format: u32,
}

/// The major part of a version string `major[.minor[.patch]][+build]`.
fn parse_version(text: &str) -> Option<u32> {
    let (core, build) = text
        .split_once('+')
        .map_or((text, None), |(c, b)| (c, Some(b)));
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() > 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    if let Some(b) = build
        && !b
            .split('.')
            .all(|i| !i.is_empty() && i.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
    {
        return None;
    }
    parts[0].parse().ok()
}

const HELP_FORM: &str = "write `dharmatiles version=1` (a whole number), or quote a longer version: `dharmatiles version=\"1.0\"`";

fn noun(kdl: &KdlDocument) -> &'static str {
    let has = |n: &str| kdl.nodes().iter().any(|x| x.name().value() == n);
    if has("tile") {
        "tile"
    } else if has("printer") {
        "printer"
    } else {
        "document"
    }
}

fn err(
    src: &Src,
    code: &'static str,
    span: SourceSpan,
    msg: &str,
    label: &str,
    help: &str,
) -> DocError {
    let mut e = DocError::new(src, code, span, msg, label);
    e.help = Some(help.to_owned());
    e
}

fn read_node(
    src: &Src,
    n: &KdlNode,
    kdl: &KdlDocument,
    errors: &mut Vec<DocError>,
) -> Option<Header> {
    let mut ok = true;
    if let Some(c) = n.children() {
        ok = false;
        errors.push(err(
            src,
            "dt::header",
            c.span(),
            "the header line has no children",
            "nothing goes inside it",
            "the header is one line: `dharmatiles version=1`",
        ));
    }
    let mut version = None;
    for e in n.entries() {
        match e.name().map(kdl::KdlIdentifier::value) {
            Some("version") => version = Some(e),
            Some(other) => {
                ok = false;
                errors.push(err(
                    src,
                    "dt::header",
                    e.span(),
                    &format!("the header line has no `{other}`"),
                    "not a header property",
                    "the header takes only `version=`",
                ));
            }
            None => {
                ok = false;
                errors.push(err(
                    src,
                    "dt::header",
                    e.span(),
                    "the header line takes `version=…`, not a bare value",
                    "write `version=` before it",
                    HELP_FORM,
                ));
            }
        }
    }
    let Some(v) = version else {
        if ok {
            errors.push(err(
                src,
                "dt::header-version",
                n.name().span(),
                "the header line has no version",
                "needs `version=…`",
                HELP_FORM,
            ));
        }
        return None;
    };
    let span = value_span(v);
    let (text, major) = match v.value() {
        KdlValue::Integer(i) => (i.to_string(), u32::try_from(*i).ok()),
        KdlValue::String(s) => (s.clone(), parse_version(s)),
        KdlValue::Float(_) => {
            errors.push(err(
                src,
                "dt::header-version",
                span,
                "a version with a decimal point must be quoted",
                "read as a number, not a version",
                "KDL reads a bare 1.10 as 1.1: write `version=\"1.10\"`",
            ));
            return None;
        }
        _ => {
            errors.push(err(
                src,
                "dt::header-version",
                span,
                "the version must be a whole number or a quoted string",
                "not a version",
                HELP_FORM,
            ));
            return None;
        }
    };
    let Some(format) = major else {
        errors.push(err(
            src,
            "dt::header-version",
            span,
            &format!("`{text}` is not a version"),
            "expected major[.minor[.patch]][+build], like \"1.0.0+5.gf9481c8\"",
            HELP_FORM,
        ));
        return None;
    };
    let max = FORMAT;
    if format > max {
        let help = "use a newer dharmatiles, or a copy of the file in the older format";
        errors.push(err(
            src,
            "dt::format-newer",
            span,
            &format!(
                "this {} is format {format}; this dharmatiles reads up to {max}",
                noun(kdl)
            ),
            "a newer format than this program reads",
            help,
        ));
        return None;
    }
    ok.then_some(Header {
        version: text,
        format,
    })
}

/// The refusal of a document older than [`FORMAT`] (no header, or a
/// version whose major part is 0): at its version, or at its first node.
#[must_use]
pub fn format_old(src: &Src, kdl: &KdlDocument, header: Option<&Header>) -> DocError {
    let span = kdl
        .nodes()
        .iter()
        .find(|n| n.name().value() == NODE)
        .and_then(|n| n.entries().first())
        .map(value_span)
        .or_else(|| kdl.nodes().first().map(|n| n.name().span()))
        .unwrap_or_else(|| SourceSpan::new(0.into(), 0));
    let was = header.map_or(0, |h| h.format);
    err(
        src,
        "dt::format-0",
        span,
        &format!(
            "this {} is format {was}, and this dharmatiles reads format {FORMAT}",
            noun(kdl)
        ),
        if header.is_some() {
            "an older format"
        } else {
            "no header: format 0"
        },
        "release 1.0.0 (tag `v1.0.0`) carries a document to format 1 with `dharmatiles migrate FILE`: run that release once on the file, then use this one",
    )
}

/// The header of a parsed document, with every problem with it: not
/// first, repeated, malformed, or of a format newer than [`FORMAT`].
pub fn read(src: &Src, kdl: &KdlDocument) -> (Option<Header>, Vec<DocError>) {
    let mut errors = Vec::new();
    let mut header = None;
    let mut seen = false;
    for (i, n) in kdl.nodes().iter().enumerate() {
        if n.name().value() != NODE {
            continue;
        }
        if seen {
            errors.push(err(
                src,
                "dt::header-twice",
                n.name().span(),
                "a document has one header line",
                "a second one",
                "keep `dharmatiles version=…` once, as the first node of the file",
            ));
        } else if i > 0 {
            errors.push(err(
                src,
                "dt::header-first",
                n.name().span(),
                "the header line must come first",
                "not the first node",
                "move `dharmatiles version=…` to the top of the file",
            ));
        } else {
            header = read_node(src, n, kdl, &mut errors);
        }
        seen = true;
    }
    (header, errors)
}

/// Check the header of any document text: a palette, a tile, anything.
///
/// # Errors
/// The text isn't KDL v2, or its header is malformed, misplaced, repeated
/// or of a newer format.
pub fn check_text(name: &str, text: &str) -> Result<Option<Header>, Vec<DocError>> {
    let src = Arc::new(NamedSource::new(name, text.to_owned()));
    let kdl = match KdlDocument::parse(text) {
        Ok(d) => d,
        Err(e) => return Err(syntax::syntax_errors(&src, text, &e)),
    };
    let (h, errors) = read(&src, &kdl);
    if errors.is_empty() {
        Ok(h)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(text: &str) -> Vec<&'static str> {
        check_text("t.dtile", text)
            .err()
            .unwrap_or_default()
            .iter()
            .map(|e| e.code)
            .collect()
    }

    #[test]
    fn versions_read_by_their_major() {
        for t in [
            "dharmatiles version=0\n",
            "dharmatiles version=\"0.9\"\n",
            "dharmatiles version=\"0.9.0+5.gf9481c8.dirty\"\n",
            "dharmatiles version=\"0.9.0+cloud-tops.5.gf9481c8\"\n",
        ] {
            assert_eq!(check_text("t", t).unwrap().unwrap().format, 0, "{t}");
        }
        for t in [
            "dharmatiles version=1\n",
            "dharmatiles version=\"1.0\"\n",
            "dharmatiles version=\"1.0.0+5.gf9481c8.dirty\"\n",
        ] {
            assert_eq!(check_text("t", t).unwrap().unwrap().format, 1, "{t}");
        }
        assert_eq!(check_text("t", "tile size=\"1x1\"").unwrap(), None);
    }

    #[test]
    fn a_bad_header_is_refused_with_a_code() {
        assert_eq!(code("dharmatiles version=\"2.0\"\n"), ["dt::format-newer"]);
        assert_eq!(code("dharmatiles version=2\n"), ["dt::format-newer"]);
        assert_eq!(code("dharmatiles\n"), ["dt::header-version"]);
        assert_eq!(code("dharmatiles version=0.9\n"), ["dt::header-version"]);
        assert_eq!(code("dharmatiles version=#true\n"), ["dt::header-version"]);
        assert_eq!(code("dharmatiles version=\"x\"\n"), ["dt::header-version"]);
        assert_eq!(
            code("dharmatiles version=\"1.2.3.4\"\n"),
            ["dt::header-version"]
        );
        assert_eq!(code("dharmatiles version=\"0+\"\n"), ["dt::header-version"]);
        assert_eq!(code("dharmatiles 0\n"), ["dt::header"]);
        assert_eq!(code("dharmatiles version=0 extra=1\n"), ["dt::header"]);
        assert_eq!(code("dharmatiles version=0 { x; }\n"), ["dt::header"]);
        assert_eq!(
            code("tile size=\"1x1\"\ndharmatiles version=0\n"),
            ["dt::header-first"]
        );
        assert_eq!(
            code("dharmatiles version=0\ndharmatiles version=0\n"),
            ["dt::header-twice"]
        );
    }

    #[test]
    fn format_one_is_read_and_format_two_is_newer() {
        let t = "dharmatiles version=1\n";
        assert_eq!(check_text("t", t).unwrap().unwrap().format, 1);
        assert_eq!(
            check_text("t", "dharmatiles version=2\n").unwrap_err()[0].code,
            "dt::format-newer"
        );
    }

    #[test]
    fn the_newer_format_names_the_kind_of_document() {
        let e = check_text("t", "dharmatiles version=2\ntile size=\"1x1\"\n").unwrap_err();
        assert_eq!(
            e[0].message,
            "this tile is format 2; this dharmatiles reads up to 1"
        );
    }
}
