//! The terminal's colours for a document: the error snippets miette draws show
//! the source as the editor and the site do, coloured by `dt_lang::highlight`'s
//! one set of rules.  Which colours is that module's (`Class::look`); this only
//! turns a look into ANSI.
//!
//! Whether to colour at all is miette's own decision, which the program leaves as
//! it is: colour when stderr is a terminal that supports it, never with `NO_COLOR`,
//! always with `FORCE_COLOR` or `CLICOLOR_FORCE`.  The highlighter is not asked
//! when colour is off, so a piped or redirected report is plain text.

use dt_lang::highlight::{Class, Lexer};
use miette::highlighters::{Highlighter, HighlighterState};
use miette::{MietteHandlerOpts, SpanContents};
use owo_colors::{AnsiColors, Style, Styled};

/// Draw the diagnostics' source with the language's colours: set once, first thing in `main`.
///
/// # Errors
/// If a handler was installed already, which would be a second owner of how errors are drawn.
pub(crate) fn install() -> Result<(), miette::InstallError> {
    miette::set_hook(Box::new(|_| {
        Box::new(
            MietteHandlerOpts::new()
                .with_syntax_highlighting(Dtile)
                .build(),
        )
    }))
}

struct Dtile;

impl Highlighter for Dtile {
    fn start_highlighter_state<'h>(
        &'h self,
        _source: &dyn SpanContents<'_>,
    ) -> Box<dyn HighlighterState + 'h> {
        Box::new(Lines(Lexer::new()))
    }
}

/// The lines of one snippet, in order.
struct Lines(Lexer);

impl HighlighterState for Lines {
    fn highlight_line<'s>(&mut self, line: &'s str) -> Vec<Styled<&'s str>> {
        let plain = Style::new();
        let mut out = Vec::new();
        let mut at = 0;
        for p in self.0.line(line) {
            if p.start > at {
                out.push(plain.style(&line[at..p.start]));
            }
            out.push(style(p.class).style(&line[p.start..p.end]));
            at = p.end;
        }
        if at < line.len() {
            out.push(plain.style(&line[at..]));
        }
        out
    }
}

/// The class's look as an ANSI style.
fn style(class: Class) -> Style {
    let look = class.look();
    let mut s = Style::new();
    if let Some(code) = look.fg {
        s = s.color(color(code));
    }
    if look.bold {
        s = s.bold();
    }
    if look.dim {
        s = s.dimmed();
    }
    if look.italic {
        s = s.italic();
    }
    s
}

/// The ANSI colour a foreground code (30-37, 90-97) names.
fn color(code: u8) -> AnsiColors {
    match code {
        30 => AnsiColors::Black,
        31 => AnsiColors::Red,
        32 => AnsiColors::Green,
        33 => AnsiColors::Yellow,
        34 => AnsiColors::Blue,
        35 => AnsiColors::Magenta,
        36 => AnsiColors::Cyan,
        37 => AnsiColors::White,
        90 => AnsiColors::BrightBlack,
        91 => AnsiColors::BrightRed,
        92 => AnsiColors::BrightGreen,
        93 => AnsiColors::BrightYellow,
        94 => AnsiColors::BrightBlue,
        95 => AnsiColors::BrightMagenta,
        96 => AnsiColors::BrightCyan,
        97 => AnsiColors::BrightWhite,
        // the table in dt-lang is the one place a look is written: a code it uses is named here
        other => {
            unreachable!("the look table uses ANSI colour {other}, which terminal.rs does not name")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dt_lang::highlight::CLASSES;

    #[test]
    fn every_look_the_table_writes_is_a_colour_the_terminal_names() {
        for c in CLASSES {
            let _ = style(c);
        }
    }

    #[test]
    fn no_class_looks_like_what_the_error_renderer_draws_for_itself() {
        // the docs read a captured terminal back into classes by look (`xtask/src/highlight.rs`)
        let own = miette::ThemeStyles::ansi();
        let mut theirs = vec![
            own.error,
            own.warning,
            own.advice,
            own.help,
            own.link,
            own.linum,
        ];
        theirs.extend(own.highlights.iter().copied());
        let show = |s: Style| s.style("x").to_string();
        for c in CLASSES {
            for t in &theirs {
                assert_ne!(
                    show(style(c)),
                    show(*t),
                    "{c:?} looks like the error renderer's own style"
                );
            }
        }
    }

    #[test]
    fn a_line_comes_back_whole_in_styled_pieces() {
        let line = "rock size=\"8dmm\" // x";
        let mut state = Lines(Lexer::new());
        let pieces = state.highlight_line(line);
        let text: String = pieces.iter().map(|s| *s.inner()).collect();
        assert_eq!(text, line);
        assert!(pieces.len() > 4);
    }
}
