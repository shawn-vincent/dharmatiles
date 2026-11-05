//! One diagnostic type for everything the evaluator reports: a message at
//! a span of a named source, with a hint, rendered by miette.

use std::fmt;
use std::sync::Arc;

use dt_doc::DocError;
use miette::{Diagnostic, LabeledSpan, SourceCode, SourceSpan};

pub use dt_doc::error::Src;

/// One problem, pointing at the text that caused it.
#[derive(Debug, Clone)]
pub struct Diag {
    /// Stable code, such as `dt::unknown-element`.
    pub code: &'static str,
    /// The headline.
    pub message: String,
    /// Text under the span.
    pub label: String,
    /// What to do about it.
    pub help: Option<String>,
    /// Where (absent for problems with no text, such as a missing file).
    pub at: Option<(Src, SourceSpan)>,
}

impl Diag {
    /// A problem at `span` of `src`.
    pub fn at(
        src: &Src,
        span: SourceSpan,
        code: &'static str,
        message: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            label: label.into(),
            help: None,
            at: Some((Arc::clone(src), span)),
        }
    }

    /// A problem with no text to point at.
    pub fn plain(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            label: String::new(),
            help: None,
            at: None,
        }
    }

    /// With a hint.
    #[must_use]
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// With a hint, if there is one.
    #[must_use]
    pub fn maybe_help(mut self, help: Option<String>) -> Self {
        self.help = help;
        self
    }

    /// File name, 1-based line and column, if the problem has a place.
    #[must_use]
    pub fn location(&self) -> Option<(String, usize, usize)> {
        let (src, span) = self.at.as_ref()?;
        let text = src.inner();
        let off = span.offset().min(text.len());
        let before = &text[..off];
        let line = before.matches('\n').count() + 1;
        let col = before.rfind('\n').map_or(off, |i| off - i - 1) + 1;
        Some((src.name().to_owned(), line, col))
    }
}

impl From<&DocError> for Diag {
    /// A dt-doc error, at the text it was decoded from.
    fn from(e: &DocError) -> Self {
        Self::at(e.src(), e.span, e.code, e.message.clone(), e.label.clone())
            .maybe_help(e.help.clone())
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Diag {}

impl Diagnostic for Diag {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new(self.code))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h) as Box<dyn fmt::Display + 'a>)
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.at.as_ref().map(|(s, _)| s.as_ref() as &dyn SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let (_, span) = self.at.as_ref()?;
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            Some(self.label.clone()),
            *span,
        ))))
    }
}
