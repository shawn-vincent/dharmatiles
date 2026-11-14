//! The site's colour, written when the docs are generated: every code block of every page is coloured by
//! `dt_lang::highlight`'s one set of rules (the editor and the terminal read the same), and the terminal's
//! own message on an error page is read back from its ANSI colours into the same classes.  Nothing is
//! coloured in the browser.
//!
//! [`pre_blocks`] takes a page's HTML and returns it with each tile-language `<pre><code>` block set as
//! coloured spans (`tk-node`, …: `docs/assets/theme.css` colours them).  A block may carry `<mark>` change
//! marks (an error page's broken document and its fix): they are read out first and laid back over the
//! coloured pieces, so a mark may begin or end in the middle of a coloured word.  The pass is idempotent: the
//! spans it wrote are read out and written again, so a run over its own output changes nothing.
//!
//! Which blocks are tile language: any `<pre><code>` with a `{`, `}` or `=` in it whose first line is not a
//! shell command, unless the `<pre>` is marked `no-hl`; a block with any other markup is left as written.

use std::fmt::Write as _;

use dt_lang::highlight::{self, Class, Look};

/// A change mark in a block: bytes `from..to` of its plain text, and the mark's `class` attribute.
struct Mark {
    from: usize,
    to: usize,
    class: String,
}

/// `html` with every tile-language code block coloured.
pub fn pre_blocks(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + html.len() / 4);
    let mut rest = html;
    while let Some(at) = rest.find("<pre") {
        let after = &rest[at + 4..];
        // `<pre>` or `<pre …>`, not `<preview>`
        if !after.starts_with(['>', ' ']) {
            out.push_str(&rest[..at + 4]);
            rest = after;
            continue;
        }
        let Some(open_end) = after.find('>') else {
            break;
        };
        let attrs = &after[..open_end];
        let body_start = at + 4 + open_end + 1;
        let Some(close) = rest[body_start..].find("</pre>") else {
            break;
        };
        let body = &rest[body_start..body_start + close];
        out.push_str(&rest[..body_start]);
        match (attrs.contains("no-hl"), body.strip_prefix("<code")) {
            (false, Some(code)) => out.push_str(&block(body, code)),
            _ => out.push_str(body),
        }
        rest = &rest[body_start + close..];
    }
    out.push_str(rest);
    out
}

/// The coloured `<code>` element for the body of a `<pre>` (`code` is the body after `<code`), or the body as it is.
fn block(body: &str, code: &str) -> String {
    let Some(open_end) = code.find('>') else {
        return body.to_owned();
    };
    let Some(inner_end) = code.rfind("</code>") else {
        return body.to_owned();
    };
    if inner_end < open_end || !code[inner_end + 7..].trim().is_empty() {
        return body.to_owned();
    }
    let attrs = &code[..open_end];
    let Some((text, marks)) = plain(&code[open_end + 1..inner_end]) else {
        return body.to_owned();
    };
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    if is_shell(first) || !text.contains(['{', '}', '=']) {
        return body.to_owned();
    }
    format!("<code{attrs}>{}</code>", render(&text, &marks))
}

/// A shell command (or the program's own invocation): not a document.
fn is_shell(line: &str) -> bool {
    let l = line.trim_start();
    [
        "$", "cargo ", "git ", "cd ", "./", "brew ", "python", "cp ", "mkdir ", "export ", "ls ",
    ]
    .iter()
    .any(|p| l.starts_with(p))
        || (l.starts_with("dharmatiles ") && !l.starts_with("dharmatiles version="))
}

/// A coloured block's HTML as the text it shows (colour spans and change marks dropped, entities read): what a check of
/// the sample must see.  `None` for a block with other markup.
pub fn text_of(html: &str) -> Option<String> {
    plain(html).map(|(text, _)| text)
}

/// A block's HTML as its text and its change marks; the colour spans written before are dropped.  `None` for a
/// block with any other markup, or an entity that is not one of the four the writer escapes.
fn plain(html: &str) -> Option<(String, Vec<Mark>)> {
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut open: Option<(usize, String)> = None;
    let mut spans = 0usize; // colour spans open: their closing tags are dropped too
    let mut rest = html;
    while let Some(i) = rest.find(['<', '&']) {
        text.push_str(&rest[..i]);
        if rest[i..].starts_with('&') {
            let end = rest[i..].find(';')? + i;
            text.push(match &rest[i..=end] {
                "&amp;" => '&',
                "&lt;" => '<',
                "&gt;" => '>',
                "&quot;" => '"',
                _ => return None,
            });
            rest = &rest[end + 1..];
            continue;
        }
        let end = rest[i..].find('>')? + i;
        let tag = &rest[i + 1..end];
        if let Some(attrs) = tag.strip_prefix("mark") {
            let class = attrs.trim().strip_prefix("class=\"")?.strip_suffix('"')?;
            open = Some((text.len(), class.to_owned()));
        } else if tag == "/mark" {
            let (from, class) = open.take()?;
            marks.push(Mark {
                from,
                to: text.len(),
                class,
            });
        } else if tag.starts_with("span class=\"tk-") || tag.starts_with("span class=\"a-") {
            spans += 1;
        } else if tag == "/span" && spans > 0 {
            spans -= 1;
        } else {
            return None;
        }
        rest = &rest[end + 1..];
    }
    text.push_str(rest);
    (open.is_none() && spans == 0).then_some((text, marks))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The text as coloured spans with the marks laid over them: the colour is cut where a mark starts or ends, and
/// each mark is opened and shut between the pieces.
fn render(text: &str, marks: &[Mark]) -> String {
    let pieces = highlight::tokens(text);
    let mut cuts: Vec<usize> = vec![0, text.len()];
    cuts.extend(pieces.iter().flat_map(|p| [p.start, p.end]));
    cuts.extend(marks.iter().flat_map(|m| [m.from, m.to]));
    cuts.sort_unstable();
    cuts.dedup();
    let mut html = String::new();
    let mut open: Option<usize> = None;
    let shut = |html: &mut String, open: &mut Option<usize>| {
        if open.take().is_some() {
            html.push_str("</mark>");
        }
    };
    let empties = |html: &mut String, open: &mut Option<usize>, at: usize| {
        for m in marks.iter().filter(|m| m.from == at && m.to == at) {
            shut(html, open);
            let _ = write!(html, "<mark class=\"{}\"></mark>", m.class);
        }
    };
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        empties(&mut html, &mut open, a);
        let mark = marks.iter().position(|m| m.from <= a && a < m.to);
        if mark != open {
            shut(&mut html, &mut open);
            if let Some(i) = mark {
                let _ = write!(html, "<mark class=\"{}\">", marks[i].class);
                open = mark;
            }
        }
        let part = esc(&text[a..b]);
        match pieces.iter().find(|p| p.start <= a && a < p.end) {
            Some(p) => {
                let _ = write!(html, "<span class=\"{}\">{part}</span>", p.class.css());
            }
            None => html.push_str(&part),
        }
    }
    shut(&mut html, &mut open);
    empties(&mut html, &mut open, text.len());
    html
}

/// What a terminal's text is made of: its words as plain text, and as HTML with each ANSI colour read back into the
/// class that has that look (`tk-…`) or, for the error renderer's own colours, a generic `a-…` class
/// (`docs/assets/site.css`).
pub struct Said {
    pub plain: String,
    pub html: String,
}

/// The terminal's `text` read as [`Said`].
///
/// # Errors
/// On an escape sequence this does not know: nothing is dropped unseen.
pub fn ansi(text: &str) -> Result<Said, String> {
    #[derive(Clone, Copy, Default, PartialEq)]
    struct State {
        look: Look,
        underline: bool,
    }
    let flush = |html: &mut String, s: State, run: &mut String| {
        if run.is_empty() {
            return;
        }
        let part = esc(run);
        let class = if s.underline {
            None
        } else {
            Class::from_look(s.look).map(|c| c.css().to_owned())
        };
        let class = class.or_else(|| {
            let mut v = Vec::new();
            if let Some(fg) = s.look.fg {
                v.push(format!("a-{fg}"));
            }
            for (on, c) in [
                (s.look.bold, "a-b"),
                (s.look.dim, "a-d"),
                (s.look.italic, "a-i"),
                (s.underline, "a-u"),
            ] {
                if on {
                    v.push(c.to_owned());
                }
            }
            (!v.is_empty()).then(|| v.join(" "))
        });
        match class {
            Some(c) => {
                let _ = write!(html, "<span class=\"{c}\">{part}</span>");
            }
            None => html.push_str(&part),
        }
        run.clear();
    };
    let (mut html, mut plain_text, mut run) = (String::new(), String::new(), String::new());
    let mut state = State::default();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            run.push(c);
            plain_text.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                let mut params = String::new();
                let last = loop {
                    match chars.next() {
                        Some(d) if d.is_ascii_digit() || d == ';' => params.push(d),
                        Some(d) => break d,
                        None => {
                            return Err(format!(
                                "an escape sequence that does not end: ESC[{params}"
                            ));
                        }
                    }
                };
                if last != 'm' {
                    return Err(format!(
                        "an escape sequence this does not read: ESC[{params}{last}"
                    ));
                }
                flush(&mut html, state, &mut run);
                for p in params.split(';') {
                    match p.parse::<u16>().unwrap_or(0) {
                        0 => state = State::default(),
                        1 => state.look.bold = true,
                        2 => state.look.dim = true,
                        3 => state.look.italic = true,
                        4 => state.underline = true,
                        22 => {
                            state.look.bold = false;
                            state.look.dim = false;
                        }
                        23 => state.look.italic = false,
                        24 => state.underline = false,
                        n @ (30..=37 | 90..=97) => {
                            state.look.fg = Some(u8::try_from(n).expect("30..=97 fits a byte"))
                        }
                        39 => state.look.fg = None,
                        n => {
                            return Err(format!(
                                "a terminal style this does not read: {n} in ESC[{params}m"
                            ));
                        }
                    }
                }
            }
            other => {
                return Err(format!(
                    "an escape sequence this does not read: ESC {other:?}"
                ));
            }
        }
    }
    flush(&mut html, state, &mut run);
    Ok(Said {
        plain: plain_text.trim_matches('\n').to_owned(),
        html: html.trim_matches('\n').to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_is_coloured_and_colouring_it_again_changes_nothing() {
        let page = "<p>x</p><pre><code>rock size=&quot;8dmm&quot; // hi &amp; bye\n</code></pre>";
        let once = pre_blocks(page);
        assert!(
            once.contains("<span class=\"tk-node\">rock</span>"),
            "{once}"
        );
        assert!(
            once.contains("<span class=\"tk-number\">8dmm</span>"),
            "{once}"
        );
        assert!(once.contains("// hi &amp; bye"), "{once}");
        assert_eq!(pre_blocks(&once), once);
    }

    #[test]
    fn a_shell_command_a_no_hl_block_and_other_markup_are_left_alone() {
        for page in [
            "<pre><code>cargo build --release a=b\n</code></pre>",
            "<pre class=\"said no-hl\"><code>rock a=1</code></pre>",
            "<pre><code>rock <b>a</b>=1</code></pre>",
            "<pre><code>just some words\n</code></pre>",
            "<pre>rock a=1</pre>",
        ] {
            assert_eq!(pre_blocks(page), page);
        }
    }

    #[test]
    fn a_mark_lies_over_the_colour_and_may_start_inside_a_piece() {
        let page = "<pre><code>rock size=\"<mark class=\"del\">abc\"</mark> x=<mark class=\"ins at\"></mark>1\n</code></pre>";
        let out = pre_blocks(page);
        // the string is cut where the mark starts; the mark is one element across the cut
        assert!(
            out.contains("<span class=\"tk-string\">\"</span><mark class=\"del\"><span class=\"tk-string\">abc\"</span></mark>"),
            "{out}"
        );
        assert!(out.contains("<mark class=\"ins at\"></mark>"), "{out}");
        assert_eq!(pre_blocks(&out), out);
    }

    #[test]
    fn the_terminal_is_read_back_into_the_same_classes() {
        let node = Class::Node.look();
        assert_eq!(node.fg, Some(36));
        let said = ansi("\u{1b}[31merror\u{1b}[39m: \u{1b}[1m\u{1b}[36mrock\u{1b}[39m\u{1b}[22m \u{1b}[36mhelp\u{1b}[39m <x>\u{1b}[2m 1\u{1b}[0m").unwrap();
        assert_eq!(said.plain, "error: rock help <x> 1");
        assert!(
            said.html.contains("<span class=\"a-31\">error</span>"),
            "{}",
            said.html
        );
        assert!(
            said.html.contains("<span class=\"tk-node\">rock</span>"),
            "{}",
            said.html
        );
        assert!(
            said.html.contains("<span class=\"a-36\">help</span>"),
            "{}",
            said.html
        );
        assert!(said.html.contains("&lt;x&gt;"), "{}", said.html);
        assert!(
            said.html.contains("<span class=\"a-d\"> 1</span>"),
            "{}",
            said.html
        );
        assert!(ansi("\u{1b}[5Hx").is_err());
    }
}
