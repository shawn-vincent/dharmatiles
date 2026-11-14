//! Diagnostics with source spans, rendered by miette.

use std::fmt;
use std::sync::Arc;

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceCode, SourceSpan};

/// A named document text, shared by its diagnostics.
pub type Src = Arc<NamedSource<String>>;

/// An edit that mends a mistake: the text at `[offset, offset + len)` is written as `replacement`.  Made only by
/// [`suggest`], together with the help that says the same thing, so the words and the button cannot differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// Byte offset of the text to replace.
    pub offset: usize,
    /// Its length in bytes.
    pub len: usize,
    /// What is there now: a page applies the fix only to the text it was made for.
    pub was: String,
    /// What to write there (a quoted word stays quoted).
    pub replacement: String,
    /// The button's words: `Change to dirt`.
    pub label: String,
}

/// The words of a suggestion, `did you mean `dirt`?`: the one place they are written.  A suggestion with a text to mend
/// is made with [`suggest`], which says the same words and carries the fix; a word with no text to mend (one typed on
/// the command line) says only these.
#[must_use]
pub fn did_you_mean_help(word: &str) -> String {
    format!("did you mean `{word}`?")
}

/// Where `word` first appears within `outer` of `text` (the whole of `outer` when it does not): the text a suggestion
/// replaces, when the error's own span is wider than the word.
#[must_use]
pub fn locate(text: &str, outer: SourceSpan, word: &str) -> SourceSpan {
    let inner = text
        .get(outer.offset()..outer.offset() + outer.len())
        .unwrap_or("");
    inner.find(word).map_or(outer, |i| {
        SourceSpan::new((outer.offset() + i).into(), word.len())
    })
}

/// The one place a suggestion is made: the help (`did you mean `dirt`?`) and the fix that writes `word` over the text at
/// `span` of `text`.  A word written in quotes in the source is replaced in quotes.
#[must_use]
pub fn suggest(text: &str, span: SourceSpan, word: &str) -> (String, Fix) {
    let (offset, len) = (span.offset().min(text.len()), span.len());
    let old = text
        .get(offset..(offset + len).min(text.len()))
        .unwrap_or("");
    let quoted = old.len() >= 2 && old.starts_with('"') && old.ends_with('"');
    let replacement = if quoted {
        format!("\"{word}\"")
    } else {
        word.to_owned()
    };
    (
        did_you_mean_help(word),
        Fix {
            offset,
            len,
            was: old.to_owned(),
            replacement,
            label: format!("Change to {word}"),
        },
    )
}

/// One problem in a document, pointing at the text that caused it.
#[derive(Debug, Clone)]
pub struct DocError {
    /// Stable code, such as `dt::unknown-control`.
    pub code: &'static str,
    /// The headline.
    pub message: String,
    /// Where in the source.
    pub span: SourceSpan,
    /// Text shown under the span.
    pub label: String,
    /// What to do about it.
    pub help: Option<String>,
    /// An edit that mends it, when there is one (made with [`DocError::suggest`], with the help); boxed so an error stays small.
    pub fix: Option<Box<Fix>>,
    src: Src,
}

impl DocError {
    pub fn new(
        src: &Src,
        code: &'static str,
        span: SourceSpan,
        message: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            span,
            label: label.into(),
            help: None,
            fix: None,
            src: Arc::clone(src),
        }
    }

    /// The text it points into, named as decoded.
    #[must_use]
    pub fn src(&self) -> &Src {
        &self.src
    }

    /// Suggest `word` for the text at `span`: the help and the fix, from one place.
    pub fn suggest(&mut self, span: SourceSpan, word: &str) {
        let (help, fix) = suggest(self.src.inner(), span, word);
        self.help = Some(help);
        self.fix = Some(Box::new(fix));
    }

    /// Suggest `near` for the text at `span` when there is a near word, else say `otherwise`.
    pub fn suggest_or(
        &mut self,
        span: SourceSpan,
        near: Option<&str>,
        otherwise: impl FnOnce() -> String,
    ) {
        match near {
            Some(w) => self.suggest(span, w),
            None => self.help = Some(otherwise()),
        }
    }

    /// Help for an unknown word at `span`: `first` (what the program knows about the word) wins, then a `near` word
    /// as a suggestion with its fix, then `last`.
    pub fn advise(
        &mut self,
        span: SourceSpan,
        first: Option<String>,
        near: Option<&str>,
        last: impl FnOnce() -> Option<String>,
    ) {
        if first.is_some() {
            self.help = first;
        } else if let Some(w) = near {
            self.suggest(span, w);
        } else {
            self.help = last();
        }
    }

    pub fn maybe_help(mut self, help: Option<String>) -> Self {
        self.help = help;
        self
    }
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DocError {}

impl Diagnostic for DocError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new(self.code))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h) as Box<dyn fmt::Display + 'a>)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        Some(self.src.as_ref())
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            Some(self.label.clone()),
            self.span,
        ))))
    }
}
