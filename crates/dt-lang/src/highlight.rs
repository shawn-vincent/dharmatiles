//! How a document is coloured: the one set of rules.  The terminal's error
//! snippets, the editor's source pane and the site's code blocks all take
//! their pieces from here; none keeps a tokenizer of its own.
//!
//! The tokenizer reads text a line at a time ([`Lexer`]) and gives back
//! [`Piece`]s (a byte range and a [`Class`]); text no rule names is left out
//! (plain).  It knows KDL v2 as the language writes it: comments (`//`, `/* */`
//! and the slashdash `/-`), strings (regular, raw and multi-line), property
//! names (`name=`), `#true`-style keywords, numbers, the first word of a node,
//! punctuation, and, inside a string, the language's own words: `$` names,
//! numbers with their unit and calls of a function or weighting.  It never
//! fails: a broken document is coloured as far as it reads, and an unterminated
//! string ends at its line, so the lines after it are coloured as they would be
//! alone.
//!
//! Each class carries its [`Look`] on an ANSI terminal and its CSS class
//! ([`Class::css`], coloured by the theme in `docs/assets/theme.css`), so the
//! mapping of classes to colours is written here and in the theme, nowhere else.

use crate::{UNITS, is_function, is_weighting};

/// What a piece of a document is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// `// …`, `/* … */` and the slashdash `/-`.
    Comment,
    /// The first word of a node: the element's name, `tile`, `region`, `line`…
    Node,
    /// The name of a property, the word before `=`: a control.
    Property,
    /// A quoted string, or the part of one that is not one of the words below.
    String,
    /// A number, with its unit when it has one (`5`, `8dmm`, `50%`).
    Number,
    /// `#true`, `#false`, `#null`, `#inf`, `#-inf`, `#nan`.
    Keyword,
    /// A `$` name inside a string (`$x`, `$region`).
    Name,
    /// A function or weighting called inside a string (`smooth` in `smooth($d)`).
    Function,
    /// `{ } ; = ( ) \`.
    Punct,
}

/// Every class, in the order the editor's reply numbers them.
pub const CLASSES: [Class; 9] = [
    Class::Comment,
    Class::Node,
    Class::Property,
    Class::String,
    Class::Number,
    Class::Keyword,
    Class::Name,
    Class::Function,
    Class::Punct,
];

/// How a class looks on an ANSI terminal: a foreground colour code (30-37 or
/// 90-97) and whether it is bold, dim and italic.  No two classes look alike,
/// and none looks like what the error renderer draws for itself, so the one
/// table also reads a captured terminal back into classes (`cargo xtask docs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Look {
    pub fg: Option<u8>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
}

impl Class {
    /// The CSS class the editor and the site give the piece: coloured in `docs/assets/theme.css`.
    #[must_use]
    pub const fn css(self) -> &'static str {
        match self {
            Class::Comment => "tk-comment",
            Class::Node => "tk-node",
            Class::Property => "tk-property",
            Class::String => "tk-string",
            Class::Number => "tk-number",
            Class::Keyword => "tk-keyword",
            Class::Name => "tk-name",
            Class::Function => "tk-function",
            Class::Punct => "tk-punct",
        }
    }

    /// The class's place in [`CLASSES`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// How the class looks on an ANSI terminal.
    #[must_use]
    pub const fn look(self) -> Look {
        let (fg, bold, dim, italic) = match self {
            Class::Comment => (None, false, true, true),
            Class::Node => (Some(36), true, false, false),
            Class::Property => (Some(34), false, false, false),
            Class::String => (Some(32), false, false, false),
            Class::Number | Class::Keyword => (Some(35), false, false, false),
            Class::Name | Class::Function => (Some(96), false, false, false),
            Class::Punct => (Some(90), false, false, false),
        };
        Look {
            fg,
            bold,
            dim,
            italic,
        }
    }

    /// The class whose look is `look`, if one is: how a captured terminal is read back.  Number and Keyword
    /// share one look, as do Name and Function, so those read back as the first of each pair.
    #[must_use]
    pub fn from_look(look: Look) -> Option<Class> {
        CLASSES.iter().copied().find(|c| c.look() == look)
    }
}

/// A coloured stretch of a text: bytes `start..end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub start: usize,
    pub end: usize,
    pub class: Class,
}

/// What a line leaves open for the next: the one thing that spans lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Carry {
    #[default]
    Nothing,
    /// Inside a `/* */` comment, this many deep.
    Block(usize),
    /// Inside a `"""` string opened with this many `#`.
    Multi(usize),
}

/// The tokenizer: give it the lines of a text in order.
#[derive(Clone, Debug, Default)]
pub struct Lexer {
    carry: Carry,
}

impl Lexer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The pieces of the next line (no line break in it), with offsets into the line.
    pub fn line(&mut self, line: &str) -> Vec<Piece> {
        let mut out = Vec::new();
        let b = line.as_bytes();
        let mut i = 0;
        match self.carry {
            Carry::Nothing => {}
            Carry::Block(depth) => i = self.block(b, 0, 0, depth, &mut out),
            Carry::Multi(hashes) => i = self.multi(b, 0, 0, hashes, &mut out),
        }
        let mut at_node = true;
        let mut prose = false; // the node being read is `about`: its string is a sentence
        while i < b.len() {
            match b[i] {
                b' ' | b'\t' | b'\r' => i += 1,
                b'/' if b.get(i + 1) == Some(&b'/') => {
                    push(&mut out, i, b.len(), Class::Comment);
                    i = b.len();
                }
                b'/' if b.get(i + 1) == Some(&b'*') => i = self.block(b, i, i + 2, 1, &mut out),
                b'/' if b.get(i + 1) == Some(&b'-') => {
                    push(&mut out, i, i + 2, Class::Comment);
                    i += 2;
                }
                b'{' | b'}' | b';' => {
                    push(&mut out, i, i + 1, Class::Punct);
                    at_node = true;
                    prose = false;
                    i += 1;
                }
                b'=' | b'(' | b')' | b'\\' => {
                    push(&mut out, i, i + 1, Class::Punct);
                    i += 1;
                }
                b'"' => {
                    i = self.string(b, i, i, 0, prose, &mut out);
                    at_node = false;
                }
                b'#' => {
                    let hashes = b[i..].iter().take_while(|&&h| h == b'#').count();
                    if b.get(i + hashes) == Some(&b'"') {
                        i = self.string(b, i, i + hashes, hashes, true, &mut out);
                    } else {
                        let end = word_end(b, i + 1);
                        if is_keyword(&line[i + 1..end]) {
                            push(&mut out, i, end, Class::Keyword);
                        }
                        i = end.max(i + 1);
                    }
                    at_node = false;
                }
                b'0'..=b'9' | b'+' | b'-' if starts_number(b, i) => {
                    let end = word_end(b, i + 1);
                    push(&mut out, i, end, Class::Number);
                    i = end;
                    at_node = false;
                }
                _ => {
                    let end = word_end(b, i);
                    if end == i {
                        i += 1; // a stray character no rule names: plain
                        continue;
                    }
                    if b.get(end) == Some(&b'=') {
                        push(&mut out, i, end, Class::Property);
                    } else if at_node {
                        push(&mut out, i, end, Class::Node);
                        prose = &line[i..end] == "about";
                    }
                    at_node = false;
                    i = end;
                }
            }
        }
        out
    }

    /// A block comment that starts at `start`, read from `i` (`depth` deep there): to its end or the end of the
    /// line.  Returns where the line is read to.
    fn block(
        &mut self,
        b: &[u8],
        start: usize,
        mut i: usize,
        mut depth: usize,
        out: &mut Vec<Piece>,
    ) -> usize {
        while i < b.len() {
            if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                depth += 1;
                i += 2;
            } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    self.carry = Carry::Nothing;
                    push(out, start, i, Class::Comment);
                    return i;
                }
            } else {
                i += 1;
            }
        }
        self.carry = Carry::Block(depth);
        push(out, start, b.len(), Class::Comment);
        b.len()
    }

    /// A `"""` string that starts at `start`, read from `i`: to its closing `"""` (and the `#`s) or the end of
    /// the line.
    fn multi(
        &mut self,
        b: &[u8],
        start: usize,
        i: usize,
        hashes: usize,
        out: &mut Vec<Piece>,
    ) -> usize {
        let mut at = i;
        while at + 3 <= b.len() {
            if &b[at..at + 3] == b"\"\"\"" && hashes_at(b, at + 3) >= hashes {
                let end = at + 3 + hashes;
                self.carry = Carry::Nothing;
                push(out, start, end, Class::String);
                return end;
            }
            at += 1;
        }
        self.carry = Carry::Multi(hashes);
        push(out, start, b.len(), Class::String);
        b.len()
    }

    /// A string that starts at `start` (its `#`s, if any, then the quote at `q`): its pieces, and where it ends.
    /// A raw string and a sentence (`plain`) stay one piece.  One that does not close ends with its line.
    fn string(
        &mut self,
        b: &[u8],
        start: usize,
        q: usize,
        hashes: usize,
        plain: bool,
        out: &mut Vec<Piece>,
    ) -> usize {
        if b[q..].starts_with(b"\"\"\"") {
            return self.multi(b, start, q + 3, hashes, out);
        }
        let mut i = q + 1;
        while i < b.len() {
            match b[i] {
                b'\\' if hashes == 0 => i += 2,
                b'"' if hashes_at(b, i + 1) >= hashes => {
                    i += 1 + hashes;
                    break;
                }
                _ => i += 1,
            }
        }
        let end = i.min(b.len());
        if plain || hashes > 0 {
            push(out, start, end, Class::String);
        } else {
            inner(b, start, end, out);
        }
        end
    }
}

/// How many `#` there are from `i` on.
fn hashes_at(b: &[u8], i: usize) -> usize {
    b.get(i..)
        .map_or(0, |r| r.iter().take_while(|&&h| h == b'#').count())
}

/// Add a piece unless it is empty, joining a string to the string it touches.
fn push(out: &mut Vec<Piece>, start: usize, end: usize, class: Class) {
    if start >= end {
        return;
    }
    if let Some(last) = out.last_mut()
        && last.class == class
        && last.end == start
        && class == Class::String
    {
        last.end = end;
        return;
    }
    out.push(Piece { start, end, class });
}

fn is_keyword(w: &str) -> bool {
    matches!(w, "true" | "false" | "null" | "inf" | "-inf" | "nan")
}

fn word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The end of the bare word that starts at `i`: up to white space or a character KDL does not allow in one.
fn word_end(b: &[u8], mut i: usize) -> usize {
    while i < b.len()
        && !matches!(
            b[i],
            b' ' | b'\t'
                | b'\r'
                | b'{'
                | b'}'
                | b'('
                | b')'
                | b';'
                | b'='
                | b'"'
                | b'\\'
                | b'/'
                | b'['
                | b']'
        )
    {
        i += 1;
    }
    i
}

/// A number starts at `i`: a digit, or a sign and a digit.
fn starts_number(b: &[u8], i: usize) -> bool {
    match b[i] {
        b'0'..=b'9' => true,
        _ => b.get(i + 1).is_some_and(u8::is_ascii_digit),
    }
}

/// A quoted string's pieces, `b[start..end]` quotes and all: its `$` names, numbers with their unit and calls of
/// a function or weighting, and the rest as strings.
fn inner(b: &[u8], start: usize, end: usize, out: &mut Vec<Piece>) {
    let mut i = start;
    let mut plain_from = start;
    let flush = |out: &mut Vec<Piece>, from: usize, to: usize| push(out, from, to, Class::String);
    while i < end {
        let c = b[i];
        let boundary = i == start || !(word_byte(b[i - 1]) || b[i - 1] == b'.' || b[i - 1] == b'$');
        let found = if c == b'$'
            && b.get(i + 1)
                .is_some_and(|d| d.is_ascii_alphabetic() || *d == b'_')
        {
            let mut j = i + 1;
            while j < end && word_byte(b[j]) {
                j += 1;
            }
            Some((j, Class::Name))
        } else if boundary
            && (c.is_ascii_digit() || (c == b'-' && b.get(i + 1).is_some_and(u8::is_ascii_digit)))
        {
            number_with_unit(b, i, end).map(|j| (j, Class::Number))
        } else if boundary && c.is_ascii_alphabetic() {
            let mut j = i;
            while j < end && word_byte(b[j]) {
                j += 1;
            }
            let word = std::str::from_utf8(&b[i..j]).unwrap_or_default();
            (b.get(j) == Some(&b'(') && j < end && (is_function(word) || is_weighting(word)))
                .then_some((j, Class::Function))
        } else {
            None
        };
        match found {
            Some((j, class)) => {
                flush(out, plain_from, i);
                push(out, i, j, class);
                i = j;
                plain_from = j;
            }
            None => i += 1,
        }
    }
    flush(out, plain_from, end);
}

/// The end of a number at `i` that is followed by a unit word or `%` (`8dmm`, `0.5sq`, `50%`), within `end`.
fn number_with_unit(b: &[u8], i: usize, end: usize) -> Option<usize> {
    let mut j = i + usize::from(b[i] == b'-');
    while j < end && b[j].is_ascii_digit() {
        j += 1;
    }
    if j + 1 < end && b[j] == b'.' && b[j + 1].is_ascii_digit() {
        j += 1;
        while j < end && b[j].is_ascii_digit() {
            j += 1;
        }
    }
    if j < end && b[j] == b'%' {
        return Some(j + 1);
    }
    let mut k = j;
    while k < end && b[k].is_ascii_alphabetic() {
        k += 1;
    }
    let word = std::str::from_utf8(&b[j..k]).unwrap_or_default();
    (k > j && (k >= end || !word_byte(b[k])) && UNITS.contains(&word)).then_some(k)
}

/// The pieces of a whole text, with offsets into it.
#[must_use]
pub fn tokens(text: &str) -> Vec<Piece> {
    let mut lexer = Lexer::new();
    let mut out = Vec::new();
    let mut at = 0;
    for line in text.split('\n') {
        for p in lexer.line(line) {
            out.push(Piece {
                start: at + p.start,
                end: at + p.end,
                class: p.class,
            });
        }
        at += line.len() + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text of each piece with its class, for reading a result.
    fn shown(text: &str) -> Vec<(String, Class)> {
        tokens(text)
            .iter()
            .map(|p| (text[p.start..p.end].to_owned(), p.class))
            .collect()
    }

    fn pair(t: &str, c: Class) -> (String, Class) {
        (t.to_owned(), c)
    }

    #[test]
    fn a_node_its_properties_and_its_strings_are_told_apart() {
        use Class::*;
        assert_eq!(
            shown("rock size=\"6..10\" at=bank // a comment"),
            vec![
                pair("rock", Node),
                pair("size", Property),
                pair("=", Punct),
                pair("\"6..10\"", String),
                pair("at", Property),
                pair("=", Punct),
                pair("// a comment", Comment),
            ]
        );
    }

    #[test]
    fn a_string_shows_the_languages_own_words() {
        use Class::*;
        assert_eq!(
            shown("dirt level=\"smooth($d) * 8dmm + 50%\""),
            vec![
                pair("dirt", Node),
                pair("level", Property),
                pair("=", Punct),
                pair("\"", String),
                pair("smooth", Function),
                pair("(", String),
                pair("$d", Name),
                pair(") * ", String),
                pair("8dmm", Number),
                pair(" + ", String),
                pair("50%", Number),
                pair("\"", String),
            ]
        );
        // a number with no unit, and a word that is not a unit, stay string
        assert_eq!(
            shown("fill \"0.5,0.25\" \"3furlong\""),
            vec![
                pair("fill", Node),
                pair("\"0.5,0.25\"", String),
                pair("\"3furlong\"", String)
            ]
        );
    }

    #[test]
    fn a_sentence_after_about_stays_one_string() {
        use Class::*;
        assert_eq!(
            shown("about \"a 8mm rock at $x\""),
            vec![pair("about", Node), pair("\"a 8mm rock at $x\"", String)]
        );
    }

    #[test]
    fn numbers_keywords_and_punctuation() {
        use Class::*;
        assert_eq!(
            shown("tile size=1 wobble=-2.5 on=#true { a; b }"),
            vec![
                pair("tile", Node),
                pair("size", Property),
                pair("=", Punct),
                pair("1", Number),
                pair("wobble", Property),
                pair("=", Punct),
                pair("-2.5", Number),
                pair("on", Property),
                pair("=", Punct),
                pair("#true", Keyword),
                pair("{", Punct),
                pair("a", Node),
                pair(";", Punct),
                pair("b", Node),
                pair("}", Punct),
            ]
        );
    }

    #[test]
    fn a_node_after_a_brace_on_the_same_line_is_a_node() {
        use Class::*;
        let got = shown("region pool { boundedBy shore; fill \"0,0\" }");
        assert!(got.contains(&pair("boundedBy", Node)), "{got:?}");
        assert!(got.contains(&pair("fill", Node)), "{got:?}");
        assert!(!got.contains(&pair("shore", Node)), "{got:?}");
    }

    #[test]
    fn an_unterminated_string_ends_with_its_line() {
        use Class::*;
        let got = shown("rock size=\"6..10\nwater // next\n");
        assert_eq!(got[0], pair("rock", Node));
        assert_eq!(got[3], pair("\"6..10", String));
        assert!(got.contains(&pair("water", Node)), "{got:?}");
        assert!(got.contains(&pair("// next", Comment)), "{got:?}");
    }

    #[test]
    fn a_block_comment_and_a_multi_line_string_span_lines_and_end() {
        use Class::*;
        assert_eq!(
            shown("a /* one\ntwo */ b\nc"),
            vec![
                pair("a", Node),
                pair("/* one", Comment),
                pair("two */", Comment),
                pair("b", Node),
                pair("c", Node),
            ]
        );
        let got = shown("a \"\"\"\nraw $x\n\"\"\" b=1");
        assert_eq!(got[1], pair("\"\"\"", String));
        assert_eq!(got[2], pair("raw $x", String));
        assert!(got.contains(&pair("b", Property)), "{got:?}");
        // one that never closes runs to the end of the text and no further
        assert_eq!(shown("a /* open\nb").len(), 3);
    }

    #[test]
    fn a_raw_string_and_stray_bytes_do_not_trip_the_reader() {
        use Class::*;
        assert_eq!(
            shown("a #\"he said \"hi\"\"# b"),
            vec![pair("a", Node), pair("#\"he said \"hi\"\"#", String)]
        );
        // stray characters, a lone `#`, a slash, non-ASCII text: nothing panics, every piece is a valid range
        for text in [
            "#",
            "/ a",
            "é \"é$x\" ü=é",
            "[ ] ,",
            "\"\\",
            "a=\"\\\"",
            "\"\"\"",
            "#\"",
            "\u{1F600} 5mm",
        ] {
            for p in tokens(text) {
                assert!(
                    text.is_char_boundary(p.start)
                        && text.is_char_boundary(p.end)
                        && p.start < p.end,
                    "{text:?}"
                );
            }
        }
    }

    #[test]
    fn pieces_never_overlap_and_the_class_table_is_complete() {
        let doc = include_str!("../../../library/tiles/ground/1x1-grass.dtile");
        let ps = tokens(doc);
        assert!(ps.len() > 20);
        assert!(ps.windows(2).all(|w| w[0].end <= w[1].start));
        for (i, c) in CLASSES.iter().enumerate() {
            assert_eq!(c.index(), i);
        }
        // the pairs that share a look read back as the first of the pair
        for c in CLASSES {
            let back = Class::from_look(c.look()).unwrap();
            assert_eq!(back.look(), c.look());
        }
    }
}
