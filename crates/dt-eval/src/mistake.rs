//! A mistake of a document as data: the one record of a diagnostic, wherever it is said (the page's check, a message's fix,
//! `--json` on the command line).

use crate::diag::{Diag, Fix};

/// An edit that mends a mistake, on the wire: write `replacement` over the `len` bytes at `offset` of the file the message
/// points into.  The one form of a fix, in a message and in a check's mistake alike.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct FixJson {
    pub offset: usize,
    pub len: usize,
    /// The text there now: the page applies the fix only where it still reads this.
    pub was: String,
    pub replacement: String,
    /// The button's words: `Change to dirt`.
    pub label: String,
}

impl From<&FixJson> for Fix {
    fn from(f: &FixJson) -> Self {
        Self {
            offset: f.offset,
            len: f.len,
            was: f.was.clone(),
            replacement: f.replacement.clone(),
            label: f.label.clone(),
            moved: None,
        }
    }
}

impl From<&Fix> for FixJson {
    fn from(f: &Fix) -> Self {
        Self {
            offset: f.offset,
            len: f.len,
            was: f.was.clone(),
            replacement: f.replacement.clone(),
            label: f.label.clone(),
        }
    }
}

/// Is `code` a refusal of the document (a mistake the reader can fix) rather than a failure of the program (a panic, an
/// unreadable or unwritable file, a kernel that failed)?  The page shows the second kind as a bug.
#[must_use]
pub fn is_document_error(code: &str) -> bool {
    !matches!(
        code,
        dt_lang::codes::PANIC
            | dt_lang::codes::SOURCE
            | dt_lang::codes::WRITE
            | dt_lang::codes::BUILD
    )
}

/// One mistake of a document, at its place.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Mistake {
    pub code: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub help: Option<String>,
    /// An edit that mends it, when the program has one.
    pub fix: Option<FixJson>,
    /// A failure of the program, not of the document: shown as a bug.
    pub unexpected: bool,
}

impl From<&Diag> for Mistake {
    fn from(d: &Diag) -> Self {
        let (file, line, column) = d
            .location()
            .map_or((None, None, None), |(f, l, c)| (Some(f), Some(l), Some(c)));
        Self {
            code: d.code.to_owned(),
            message: d.message.clone(),
            file,
            line,
            column,
            help: d.help.clone(),
            fix: d.fix.as_deref().map(FixJson::from),
            unexpected: !is_document_error(d.code),
        }
    }
}

/// The mistakes of a document as the contract's type.
#[must_use]
pub fn mistakes(errs: &[Diag]) -> Vec<Mistake> {
    errs.iter().map(Mistake::from).collect()
}
