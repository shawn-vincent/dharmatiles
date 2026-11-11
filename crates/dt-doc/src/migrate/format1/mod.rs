//! Migrating a document from format 0 to format 1, text to text.
//!
//! What each old form becomes (the language guide's "Existing tiles"):
//!
//! - the header line `dharmatiles version="…"` goes first;
//! - names gain their `$`: `x`, `y`, `bottom` and the library's heights
//!   (`override groundHeight 6` is `override $groundHeight 6`); a tile's own
//!   `let`s and a definition's `param`s stay plain;
//! - a point `"x,y"` is `"(Xsq, Ysq)"`, counted down from the north-west
//!   corner (`Y` is `rows − y`, written so the reader flips it back to the
//!   same bits); a point in a pin of a body that acts in a region or on a
//!   line is `point($tile, …)`, so its meaning cannot move with the frame;
//! - every length gets `dmm` (a size, a top, a pin, a `let`, a range's two
//!   ends, `(mm)4.5` is `4.5dmm`), every angle `deg`, every trig argument
//!   `deg`; counts, shares and weightings' extra numbers stay plain;
//! - the tide, and any formula reading `y`, reads `$y` the other way up;
//! - a printer's numbers are printed units (`layer "0.2mm"`, `overhang
//!   "55deg"`); parts and palettes take the header only.
//!
//! A `param` or `let` with no unit takes its kind from where it is used
//! (through nested definitions and the parts a tile uses); used both as a
//! length and as a count, or never used, it stops the file for a hand fix.
//! Everything else (comments, layout, order) stays as written.

mod adorn;
mod walk;

use std::collections::HashMap;
use std::sync::Arc;

use kdl::{KdlDocument, KdlNode};
use miette::{NamedSource, SourceSpan};

use self::walk::Walk;
use crate::decode::{decode, syntax, uses, value_span};
use crate::error::{DocError, Src};
use crate::known::{DefInfo, Known};
use crate::model::DefKind;

/// The format this migration produces.
pub const TARGET: u32 = 1;

/// What came of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Already at the target format (or newer): left alone, whatever its
    /// minor or build part says.
    Current(u32),
    /// The document rewritten.
    Migrated(Migration),
}

/// A document carried to format 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// The new text.
    pub text: String,
    /// The format it was read as: 0, whether stamped or not.
    pub from: u32,
    /// True when the document had no header.
    pub unstamped: bool,
}

/// The version a migrated document is stamped with: the program's own
/// when its major is the format, else the format's first release with the
/// program's build details (a program before 1.0.0 writes format 1 for its
/// preview).
#[must_use]
pub fn stamp(app_version: &str) -> String {
    let major: Option<u32> = app_version
        .split(['.', '+'])
        .next()
        .and_then(|m| m.parse().ok());
    if major == Some(TARGET) {
        return app_version.to_owned();
    }
    match app_version.split_once('+') {
        Some((_, build)) => format!("{TARGET}.0.0+{build}"),
        None => format!("{TARGET}.0.0"),
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

/// The first thing written the format 1 way: a unit after a number, a `$`
/// name, `point(` or `polar(`.  A point `"(0.5, 0.5)"` is format 0 too.
fn first_new_form(kdl: &KdlDocument) -> Option<SourceSpan> {
    fn is_new(s: &str) -> Option<usize> {
        let t = s.trim_start();
        let lead = s.len() - t.len();
        if t.starts_with("point(") || t.starts_with("polar(") {
            return Some(lead);
        }
        let b = s.as_bytes();
        for (i, c) in s.char_indices() {
            if c == '$' && b.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
                return Some(i);
            }
            if c.is_ascii_digit() || c == '.' {
                let rest = &s[i + 1..];
                for u in ["sq", "dmm", "mm", "in", "layer", "deg", "rad"] {
                    if let Some(after) = rest.strip_prefix(u)
                        && !after.starts_with(|a: char| a.is_alphanumeric())
                        && !s[..i].ends_with(|p: char| p.is_alphabetic())
                    {
                        return Some(i + 1);
                    }
                }
            }
        }
        None
    }
    fn walk(n: &KdlNode) -> Option<SourceSpan> {
        if n.name().value() != "about" {
            for e in n.entries() {
                if let Some(s) = e.value().as_string() {
                    let span = value_span(e);
                    if let Some(at) = is_new(s) {
                        let repr = e.format().map_or("", |f| f.value_repr.as_str());
                        let inner = repr.find(s).unwrap_or(0) + at;
                        let at = (span.offset() + inner).min(span.offset() + span.len() - 1);
                        return Some(SourceSpan::new(at.into(), 1));
                    }
                }
            }
        }
        n.iter_children().find_map(walk)
    }
    kdl.nodes().iter().find_map(walk)
}

fn defs_of(
    name: &str,
    text: &str,
    known: &Known,
    doc: &crate::model::Document,
) -> HashMap<String, DefInfo> {
    let mut defs = HashMap::new();
    if let Ok(us) = uses(name, text) {
        for u in us {
            if let Some(file) = known.file(u.key()) {
                for n in &u.names {
                    if let Some(info) = file.get(&n.value) {
                        defs.insert(n.value.clone(), info.clone());
                    }
                }
            }
        }
    }
    for d in &doc.definitions {
        if matches!(d.kind, DefKind::Define) {
            defs.insert(d.name.value.clone(), DefInfo::from(d));
        }
    }
    defs
}

fn apply(text: &str, mut edits: Vec<walk::Edit>) -> Option<String> {
    edits.sort_by_key(|e| (e.start, e.end));
    edits.dedup_by(|b, a| a.start == b.start && a.end == b.end && a.with == b.with);
    for w in edits.windows(2) {
        if w[0].end > w[1].start {
            return None;
        }
    }
    let mut out = text.to_owned();
    for e in edits.iter().rev() {
        out.replace_range(e.start..e.end, &e.with);
    }
    Some(out)
}

/// The edit that makes the header the target format's: the stamp's version
/// in place, or the line first with a blank line after it.
fn header_edit(
    kdl: &KdlDocument,
    src: &Src,
    app_version: &str,
) -> Result<walk::Edit, Vec<DocError>> {
    let version = stamp(app_version);
    match kdl
        .nodes()
        .iter()
        .find(|n| n.name().value() == crate::header::NODE)
    {
        Some(h) => {
            let Some(e) = h.entry("version") else {
                return Err(vec![err(
                    src,
                    "dt::header-version",
                    h.name().span(),
                    "the header line has no version",
                    "needs `version=…`",
                    "write `dharmatiles version=0`",
                )]);
            };
            let v = value_span(e);
            Ok(walk::Edit {
                start: v.offset(),
                end: v.offset() + v.len(),
                with: format!("\"{version}\""),
            })
        }
        None => Ok(walk::Edit {
            start: 0,
            end: 0,
            with: format!("dharmatiles version=\"{version}\"\n\n"),
        }),
    }
}

fn with_header(
    text: &str,
    kdl: &KdlDocument,
    src: &Src,
    app_version: &str,
) -> Result<String, Vec<DocError>> {
    let e = header_edit(kdl, src, app_version)?;
    let mut out = text.to_owned();
    out.replace_range(e.start..e.end, &e.with);
    Ok(out)
}

/// Carry the format 0 document `text` to format 1.
///
/// `known` is what the document can see: the library's heights and the
/// parts it uses (their definitions' parameters say what each value a call
/// passes measures).  `app_version` is the program's own, for the header.
///
/// # Errors
/// The document is not valid format 0 (its own errors), has some of both
/// formats' forms and no header, or holds something with no exact format 1
/// form (a `param` of no known unit, a point in a part): each with its text.
pub fn migrate(
    name: &str,
    text: &str,
    known: &Known,
    app_version: &str,
) -> Result<Outcome, Vec<DocError>> {
    let src: Src = Arc::new(NamedSource::new(name, text.to_owned()));
    let kdl = match KdlDocument::parse(text) {
        Ok(d) => d,
        Err(e) => return Err(syntax::syntax_errors(&src, text, &e)),
    };
    let (header, header_errors) = crate::header::read(&src, &kdl, true);
    if !header_errors.is_empty() {
        return Err(header_errors);
    }
    if let Some(h) = &header
        && h.format >= TARGET
    {
        return Ok(Outcome::Current(h.format));
    }
    let preview = known.clone().with_format_preview(true);
    // a palette (colours) or any document of none of the language's nodes
    // has nothing to carry over but its header
    let language = [
        "tile", "define", "use", "let", "override", "defaults", "printer",
    ];
    let header_only = !kdl
        .nodes()
        .iter()
        .any(|n| language.contains(&n.name().value()));
    if header_only {
        return Ok(Outcome::Migrated(Migration {
            text: with_header(text, &kdl, &src, app_version)?,
            from: 0,
            unstamped: header.is_none(),
        }));
    }
    if header.is_none()
        && let Some(at) = first_new_form(&kdl)
    {
        // a fresh format 1 document that forgot its header, or a mix
        let with_header = format!("dharmatiles version={TARGET}\n{text}");
        return Err(if decode(name, &with_header, &preview).is_ok() {
            vec![err(
                &src,
                "dt::migrate-header",
                at,
                "this document is written in format 1 and has no header",
                "a format 1 form",
                "add `dharmatiles version=1` at the top: a document with no header is format 0",
            )]
        } else {
            vec![err(
                &src,
                "dt::migrate-mixed",
                at,
                "this document mixes format 0 and format 1, and has no header to say which it is",
                "a format 1 form",
                "an unstamped document is format 0: write the old forms throughout, or add `dharmatiles version=1` and write format 1 throughout",
            )]
        });
    }
    let known0 = known.clone().with_format_preview(false);
    let doc = decode(name, text, &known0)?;
    let defs = defs_of(name, text, known, &doc);
    let mut w = Walk::new(Arc::clone(&src), defs);

    // the library's heights, by the plain name they are read by
    for c in known.consts() {
        w.names
            .library
            .insert(c.name.value.trim_start_matches('$').to_owned());
    }
    let file_lets: Vec<&KdlNode> = kdl
        .nodes()
        .iter()
        .filter(|n| n.name().value() == "let")
        .collect();
    for n in &file_lets {
        if let Some(a) = n.entries().first().and_then(|e| e.value().as_string()) {
            w.names.library.insert(a.to_owned());
        }
    }

    let mut edits: Vec<walk::Edit> = Vec::new();
    let mut problems: Vec<DocError> = Vec::new();
    for node in kdl.nodes() {
        match node.name().value() {
            "tile" | "define" => {
                w.names.kinds.clear();
                for _ in 0..8 {
                    w.reset_round();
                    w.unit(node);
                    if !w.settle() {
                        break;
                    }
                }
                w.reset_round();
                w.unit(node);
                w.unsettled();
                edits.append(&mut w.edits);
                problems.append(&mut w.problems);
            }
            "let" => {
                w.reset_round();
                w.library_let(node);
                edits.append(&mut w.edits);
                problems.append(&mut w.problems);
            }
            "defaults" => {
                w.reset_round();
                w.defaults(node);
                edits.append(&mut w.edits);
                problems.append(&mut w.problems);
            }
            "printer" => {
                w.reset_round();
                w.printer(node);
                edits.append(&mut w.edits);
                problems.append(&mut w.problems);
            }
            _ => {}
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }

    edits.push(header_edit(&kdl, &src, app_version)?);
    let Some(out) = apply(text, edits) else {
        return Err(vec![err(
            &src,
            "dt::migrate-by-hand",
            SourceSpan::new(0.into(), 1),
            "two changes would overlap",
            "here",
            "migrate this document by hand",
        )]);
    };
    // proof the result is a format 1 document the reader accepts
    decode(name, &out, &preview)?;
    Ok(Outcome::Migrated(Migration {
        text: out,
        from: 0,
        unstamped: header.is_none(),
    }))
}
