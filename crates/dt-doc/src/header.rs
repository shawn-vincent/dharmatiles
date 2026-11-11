//! The header line: `dharmatiles version=1`, the first node of a document
//! of any kind (a tile, a part, a printer, a palette).
//!
//! Only the major part of the version is the document's format.  A document
//! with no header is format 0.  The header is not an element or a
//! definition: nothing but this module reads it, and edits leave it as
//! written.

use std::sync::Arc;

use kdl::{KdlDocument, KdlNode, KdlValue};
use miette::{NamedSource, SourceSpan};

use crate::decode::{syntax, value_span};
use crate::error::{DocError, Src};

/// The node's name.
pub const NODE: &str = "dharmatiles";

/// The highest document format this program reads.  Format 0 is a document
/// with no header.
pub const FORMAT: u32 = 0;

/// The formats this program reads, for `--version` and the About box: `0`,
/// or `0-N` once there is more than one.
#[must_use]
pub fn formats() -> String {
    if FORMAT == 0 {
        "0".to_owned()
    } else {
        format!("0-{FORMAT}")
    }
}

/// The format in development: read only when the reader is told to preview
/// it (`DT_FORMAT_PREVIEW=1` at the command line and the preview), until
/// 1.0.0 ships it.  Format 1 is the `$` names, `let` and `override` in any
/// block and the reserved words.
pub const PREVIEW_FORMAT: u32 = 1;

/// The highest format this reader reads: [`FORMAT`], or [`PREVIEW_FORMAT`]
/// when previewing.
#[must_use]
pub fn max_format(preview: bool) -> u32 {
    if preview { PREVIEW_FORMAT } else { FORMAT }
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
    preview: bool,
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
    let max = max_format(preview);
    if format > max {
        let help = if format == PREVIEW_FORMAT {
            "format 1 is in development and is read only when DT_FORMAT_PREVIEW=1 is set; until 1.0.0 ships, use a copy of the file in format 0"
        } else {
            "use a newer dharmatiles, or a copy of the file in the older format"
        };
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

/// The header of a parsed document, with every problem with it: not
/// first, repeated, malformed, or of a format newer than [`FORMAT`].
pub(crate) fn read(src: &Src, kdl: &KdlDocument, preview: bool) -> (Option<Header>, Vec<DocError>) {
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
            header = read_node(src, n, kdl, &mut errors, preview);
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
    check_text_with(name, text, false)
}

/// [`check_text`], reading the format in development too when `preview`.
///
/// # Errors
/// As [`check_text`].
pub fn check_text_with(
    name: &str,
    text: &str,
    preview: bool,
) -> Result<Option<Header>, Vec<DocError>> {
    let src = Arc::new(NamedSource::new(name, text.to_owned()));
    let kdl = match KdlDocument::parse(text) {
        Ok(d) => d,
        Err(e) => return Err(syntax::syntax_errors(&src, text, &e)),
    };
    let (h, errors) = read(&src, &kdl, preview);
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
        assert_eq!(check_text("t", "tile size=\"1x1\"").unwrap(), None);
    }

    #[test]
    fn a_bad_header_is_refused_with_a_code() {
        assert_eq!(code("dharmatiles version=\"1.0\"\n"), ["dt::format-newer"]);
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
    fn format_one_is_read_only_when_previewed() {
        let t = "dharmatiles version=1\n";
        let e = check_text("t", t).unwrap_err();
        assert_eq!(e[0].code, "dt::format-newer");
        assert!(e[0].help.as_deref().unwrap().contains("in development"));
        assert_eq!(check_text_with("t", t, true).unwrap().unwrap().format, 1);
        assert_eq!(
            check_text_with("t", "dharmatiles version=2\n", true).unwrap_err()[0].code,
            "dt::format-newer"
        );
    }

    #[test]
    fn the_newer_format_names_the_kind_of_document() {
        let e = check_text("t", "dharmatiles version=2\ntile size=\"1x1\"\n").unwrap_err();
        assert_eq!(
            e[0].message,
            "this tile is format 2; this dharmatiles reads up to 0"
        );
    }
}
