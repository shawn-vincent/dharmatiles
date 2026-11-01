//! Diagnostics with source spans, rendered by miette.

use std::fmt;
use std::sync::Arc;

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceCode, SourceSpan};

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
    src: Arc<NamedSource<String>>,
}

impl DocError {
    pub(crate) fn new(
        src: &Arc<NamedSource<String>>,
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
            src: Arc::clone(src),
        }
    }

    pub(crate) fn maybe_help(mut self, help: Option<String>) -> Self {
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
