//! The inventory: a `.dinv` document, a log of what you have printed.
//!
//! ```kdl
//! dharmatiles version="1.0.0"
//! inventory {
//!     print "lib:tiles/ground/1x1-grass" seed=97 system=db colour=multi result=ok on="2026-11-09" hash="3c1f9a4e" from="crossing/plate-1"
//!     print "lib:tiles/cliffs/2x1-cliff" seed=4 system=db colour=multi result=failed on="2026-11-09" note="top layer stringing"
//!     gone "lib:tiles/ground/1x1-grass" seed=97 system=db colour=multi on="2026-11-11" note="dropped on the floor"
//!     renamed "home:meadow" to="home:meadows/glade"
//! }
//! ```
//!
//! The truth is the log: one line a print, a loss or a rename, never edited
//! away.  Everything else (how many you have, which seeds) is worked out from
//! it by [`Inventory::pieces`] and never stored.  A piece is the same piece
//! when its tile, seed, system and colour mode are the same; two prints of
//! one piece are two.  A failed print is kept (it says what your printer does
//! badly) and does not count.
//!
//! This module reads and writes the text and counts; where the file lives and
//! which tile a path means is the evaluator's (`dt_eval::inventory`).

use std::collections::BTreeMap;
use std::sync::Arc;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::{NamedSource, SourceSpan};

use crate::decode::{arg_str, syntax, value_span};
use crate::error::{DocError, Src};
use crate::schema::did_you_mean;

/// The file extension of an inventory.
pub const EXT: &str = "dinv";

/// The file's name in My tiles.
pub const FILE: &str = "inventory.dinv";

/// Single colour or multicolour: part of what makes a piece the same piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Colour {
    /// One filament, no paint.
    Single,
    /// Each material in its own filament.
    Multi,
}

impl Colour {
    /// As written.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multi => "multi",
        }
    }

    /// From a word, or `multicolour` as the print list writes it.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        match w {
            "single" => Some(Self::Single),
            "multi" | "multicolour" => Some(Self::Multi),
            _ => None,
        }
    }
}

/// What makes two prints the same piece.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identity {
    /// The tile: `lib:tiles/ground/1x1-grass` or `home:meadows/glade`.
    pub tile: String,
    /// The seed it was built with.
    pub seed: u64,
    /// `db` or `ol`: they never count for each other.
    pub system: String,
    /// Single colour or multicolour.
    pub colour: Colour,
}

/// One line of the log.
#[derive(Debug, Clone)]
pub enum Entry {
    /// A piece came off the printer (good or not).
    Print {
        /// Which piece.
        id: Identity,
        /// `result=ok`: it counts.
        ok: bool,
        /// The date, `2026-11-09`.
        on: String,
        /// The tile document's hash when it was built.
        hash: Option<String>,
        /// The list and plate it came from, `crossing/plate-1`.
        from: Option<String>,
        /// A note.
        note: Option<String>,
        /// The line.
        span: SourceSpan,
    },
    /// A piece you had is lost or broken.
    Gone {
        /// Which piece.
        id: Identity,
        /// The date.
        on: String,
        /// A note.
        note: Option<String>,
        /// The line.
        span: SourceSpan,
    },
    /// A tile has another path now: its prints follow it.
    Renamed {
        /// The old path.
        from: String,
        /// The new one.
        to: String,
        /// The date, if written.
        on: Option<String>,
        /// The line.
        span: SourceSpan,
    },
}

impl Entry {
    /// The line in the file.
    #[must_use]
    pub const fn span(&self) -> SourceSpan {
        match self {
            Self::Print { span, .. } | Self::Gone { span, .. } | Self::Renamed { span, .. } => {
                *span
            }
        }
    }
}

/// An inventory as written.
#[derive(Debug, Clone)]
pub struct Inventory {
    /// The log, in the order written.
    pub entries: Vec<Entry>,
    /// The text, for diagnostics.
    pub src: Src,
    /// Where the `inventory` node is written (empty when there is none).
    pub span: SourceSpan,
}

/// A piece, as the log adds up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// Which piece (the tile as the log last names it, through its renames).
    pub id: Identity,
    /// How many good prints are still on your shelf.
    pub have: u32,
    /// Good prints in all.
    pub printed: u32,
    /// Failed prints (they do not count).
    pub failed: u32,
    /// Prints marked gone.
    pub gone: u32,
    /// The hash of the latest good print's tile document.
    pub hash: Option<String>,
    /// The date of the latest line about it.
    pub last: Option<String>,
    /// The lines (indexes into [`Inventory::entries`]) that are about it.
    pub lines: Vec<usize>,
}

impl Inventory {
    /// An inventory with nothing in it (the file is not there yet).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            src: Arc::new(NamedSource::new(FILE, String::new())),
            span: SourceSpan::new(0.into(), 0),
        }
    }

    /// The pieces the log adds up to, in the order each is first written.
    /// Renames are applied where they stand in the log: the prints of the old
    /// path move to the new one (and join its pieces), later lines naming the
    /// old path are another tile's.
    #[must_use]
    pub fn pieces(&self) -> Vec<Piece> {
        let mut out: Vec<Piece> = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            match e {
                Entry::Print {
                    id, ok, on, hash, ..
                } => {
                    let p = piece_of(&mut out, id);
                    p.lines.push(i);
                    p.last = Some(on.clone());
                    if *ok {
                        p.have += 1;
                        p.printed += 1;
                        if hash.is_some() {
                            p.hash.clone_from(hash);
                        }
                    } else {
                        p.failed += 1;
                    }
                }
                Entry::Gone { id, on, .. } => {
                    let p = piece_of(&mut out, id);
                    p.lines.push(i);
                    p.last = Some(on.clone());
                    // a loss of a piece you never printed takes nothing from nothing
                    p.have = p.have.saturating_sub(1);
                    p.gone += 1;
                }
                Entry::Renamed { from, to, .. } => {
                    if from == to {
                        continue;
                    }
                    let (moved, kept): (Vec<Piece>, Vec<Piece>) =
                        out.into_iter().partition(|p| &p.id.tile == from);
                    out = kept;
                    for m in moved {
                        let p = piece_of(
                            &mut out,
                            &Identity {
                                tile: to.clone(),
                                ..m.id.clone()
                            },
                        );
                        p.have += m.have;
                        p.printed += m.printed;
                        p.failed += m.failed;
                        p.gone += m.gone;
                        if m.hash.is_some() {
                            p.hash = m.hash;
                        }
                        p.last = m.last.or(p.last.take());
                        p.lines.extend(m.lines);
                        p.lines.sort_unstable();
                    }
                    if let Some(p) = out.last_mut() {
                        p.lines.push(i);
                    }
                }
            }
        }
        out
    }

    /// How many of a piece you have (its tile, seed, system and colour mode).
    #[must_use]
    pub fn have(&self, id: &Identity) -> u32 {
        self.pieces()
            .iter()
            .filter(|p| &p.id == id)
            .map(|p| p.have)
            .sum()
    }
}

/// The piece for `id` in `out`, added when it is not there yet.
fn piece_of<'a>(out: &'a mut Vec<Piece>, id: &Identity) -> &'a mut Piece {
    if let Some(i) = out.iter().position(|p| &p.id == id) {
        return &mut out[i];
    }
    out.push(Piece {
        id: id.clone(),
        have: 0,
        printed: 0,
        failed: 0,
        gone: 0,
        hash: None,
        last: None,
        lines: Vec::new(),
    });
    let n = out.len() - 1;
    &mut out[n]
}

/// What you have of each piece, by tile, for quick questions (the pills, the
/// plates): `(single, multi)` for a tile, seed and system.
#[derive(Debug, Clone, Default)]
pub struct Shelf {
    have: BTreeMap<Identity, u32>,
}

impl Shelf {
    /// The shelf the inventory adds up to.
    #[must_use]
    pub fn of(inv: &Inventory) -> Self {
        let mut have = BTreeMap::new();
        for p in inv.pieces() {
            if p.have > 0 {
                *have.entry(p.id).or_insert(0) += p.have;
            }
        }
        Self { have }
    }

    /// How many of this exact piece.
    #[must_use]
    pub fn have(&self, id: &Identity) -> u32 {
        self.have.get(id).copied().unwrap_or(0)
    }

    /// How many of this tile and seed in the system, in the other colour mode.
    #[must_use]
    pub fn other_colour(&self, id: &Identity) -> u32 {
        let other = Identity {
            colour: match id.colour {
                Colour::Single => Colour::Multi,
                Colour::Multi => Colour::Single,
            },
            ..id.clone()
        };
        self.have(&other)
    }
}

// ---- reading --------------------------------------------------------------------------------------

struct Reader {
    src: Src,
    errors: Vec<DocError>,
}

const LINES: [&str; 3] = ["print", "gone", "renamed"];
const PRINT_KEYS: [&str; 8] = [
    "seed", "system", "colour", "result", "on", "hash", "from", "note",
];
const GONE_KEYS: [&str; 6] = ["seed", "system", "colour", "on", "note", "hash"];
const RENAMED_KEYS: [&str; 2] = ["to", "on"];

impl Reader {
    fn err(
        &mut self,
        span: SourceSpan,
        msg: impl Into<String>,
        label: impl Into<String>,
    ) -> &mut DocError {
        self.errors
            .push(DocError::new(&self.src, "dt::inventory", span, msg, label));
        let n = self.errors.len() - 1;
        &mut self.errors[n]
    }

    fn string(&mut self, key: &str, e: &KdlEntry) -> Option<String> {
        if let KdlValue::String(s) = e.value() {
            Some(s.clone())
        } else {
            self.err(
                value_span(e),
                format!("`{key}` takes a word or text"),
                "not text",
            )
            .help = Some(format!("write `{key}=\"…\"`"));
            None
        }
    }

    fn word(&mut self, key: &str, e: &KdlEntry, words: &[&str]) -> Option<String> {
        let KdlValue::String(s) = e.value() else {
            self.err(value_span(e), format!("`{key}` takes a word"), "not a word");
            return None;
        };
        if words.contains(&s.as_str()) {
            return Some(s.clone());
        }
        let near = did_you_mean(s, words.iter().copied());
        self.err(value_span(e), format!("`{s}` is not a {key}"), "not that")
            .suggest_or(value_span(e), near, || {
                format!("`{key}` is {}", words.join(" or "))
            });
        None
    }

    fn date(&mut self, key: &str, e: &KdlEntry) -> Option<String> {
        let s = self.string(key, e)?;
        let b = s.as_bytes();
        let ok = b.len() == 10
            && b[4] == b'-'
            && b[7] == b'-'
            && b.iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
        if !ok {
            self.err(
                value_span(e),
                format!("`{key}` is a date like `\"2026-11-09\"`"),
                "not a date",
            );
            return None;
        }
        Some(s)
    }

    /// A line's tile path (its one argument) and properties.
    fn identity(
        &mut self,
        n: &KdlNode,
        keys: &[&str],
    ) -> Option<(Identity, Vec<(String, KdlEntry)>)> {
        let mut pos: Vec<&KdlEntry> = Vec::new();
        let (mut seed, mut system, mut colour) = (None, None, None);
        let mut rest: Vec<(String, KdlEntry)> = Vec::new();
        let node = n.name().value().to_owned();
        for e in n.entries() {
            let Some(k) = e.name() else {
                pos.push(e);
                continue;
            };
            match k.value() {
                "seed" => match e.value() {
                    KdlValue::Integer(i) if (0..=i128::from(i64::MAX)).contains(i) => {
                        seed = u64::try_from(*i).ok();
                    }
                    _ => {
                        self.err(value_span(e), "`seed` is a whole number", "not that");
                    }
                },
                "system" => system = self.word("system", e, &["db", "ol"]),
                "colour" => colour = self.word("colour", e, &["single", "multi"]),
                other if keys.contains(&other) => rest.push((other.to_owned(), e.clone())),
                other => {
                    self.err(k.span(), format!("`{node}` has no `{other}`"), "unknown")
                        .suggest_or(k.span(), did_you_mean(other, keys.iter().copied()), || {
                            format!("`{node}` takes: {}", keys.join(", "))
                        });
                }
            }
        }
        let tile = match pos.as_slice() {
            [p] => match arg_str(p) {
                Some(s) if !s.trim().is_empty() => Some(s.to_owned()),
                _ => {
                    self.err(
                        value_span(p),
                        "a tile is named by its path in quotes",
                        "not a path",
                    );
                    None
                }
            },
            _ => {
                self.err(
                    n.span(),
                    format!("`{node}` names its tile"),
                    format!("needs `{node} \"lib:tiles/ground/1x1-grass\"`"),
                )
                .help = Some(
                    "write the tile's path in quotes, then `seed=`, `system=` and `colour=`"
                        .to_owned(),
                );
                None
            }
        };
        let miss = |r: &mut Self, what: &str, hint: &str| {
            r.err(n.span(), format!("`{node}` needs `{what}`"), "missing")
                .help = Some(hint.to_owned());
        };
        if seed.is_none()
            && !n
                .entries()
                .iter()
                .any(|e| e.name().is_some_and(|k| k.value() == "seed"))
        {
            miss(
                self,
                "seed",
                "write `seed=97`: the seed is what makes a piece the one you have",
            );
        }
        if system.is_none()
            && !n
                .entries()
                .iter()
                .any(|e| e.name().is_some_and(|k| k.value() == "system"))
        {
            miss(self, "system", "write `system=db` or `system=ol`");
        }
        if colour.is_none()
            && !n
                .entries()
                .iter()
                .any(|e| e.name().is_some_and(|k| k.value() == "colour"))
        {
            miss(self, "colour", "write `colour=single` or `colour=multi`");
        }
        let (tile, seed, system, colour) = (tile?, seed?, system?, colour?);
        Some((
            Identity {
                tile,
                seed,
                system,
                colour: Colour::from_word(&colour)?,
            },
            rest,
        ))
    }

    fn line(&mut self, n: &KdlNode) -> Option<Entry> {
        let span = n.span();
        if let Some(c) = n.children() {
            self.err(
                c.span(),
                format!("a `{}` line has no children", n.name().value()),
                "nothing goes inside it",
            );
        }
        match n.name().value() {
            "print" => {
                let (id, rest) = self.identity(n, &PRINT_KEYS)?;
                let (mut ok, mut on, mut hash, mut from, mut note) = (None, None, None, None, None);
                for (k, e) in &rest {
                    match k.as_str() {
                        "result" => {
                            ok = self.word("result", e, &["ok", "failed"]).map(|w| w == "ok")
                        }
                        "on" => on = self.date("on", e),
                        "hash" => hash = self.string("hash", e),
                        "from" => from = self.string("from", e),
                        "note" => note = self.string("note", e),
                        _ => {}
                    }
                }
                let has = |k: &str| rest.iter().any(|(n, _)| n == k);
                if !has("result") {
                    self.err(span, "`print` needs `result`", "missing").help =
                        Some("write `result=ok` or `result=failed`".to_owned());
                }
                if !has("on") {
                    self.err(span, "`print` needs `on`", "missing").help =
                        Some("write the date, `on=\"2026-11-09\"`".to_owned());
                }
                Some(Entry::Print {
                    id,
                    ok: ok?,
                    on: on?,
                    hash,
                    from,
                    note,
                    span,
                })
            }
            "gone" => {
                let (id, rest) = self.identity(n, &GONE_KEYS)?;
                let (mut on, mut note) = (None, None);
                for (k, e) in &rest {
                    match k.as_str() {
                        "on" => on = self.date("on", e),
                        "note" => note = self.string("note", e),
                        "hash" => {
                            self.string("hash", e);
                        }
                        _ => {}
                    }
                }
                if !rest.iter().any(|(n, _)| n == "on") {
                    self.err(span, "`gone` needs `on`", "missing").help =
                        Some("write the date, `on=\"2026-11-11\"`".to_owned());
                }
                Some(Entry::Gone {
                    id,
                    on: on?,
                    note,
                    span,
                })
            }
            "renamed" => {
                let mut pos = Vec::new();
                let (mut to, mut on) = (None, None);
                for e in n.entries() {
                    if e.name().is_none() {
                        pos.push(e);
                    }
                    if let Some(k) = e.name() {
                        match k.value() {
                            "to" => to = self.string("to", e),
                            "on" => on = self.date("on", e),
                            other => {
                                self.err(
                                    k.span(),
                                    format!("`renamed` has no `{other}`"),
                                    "unknown",
                                )
                                .suggest_or(
                                    k.span(),
                                    did_you_mean(other, RENAMED_KEYS),
                                    || format!("`renamed` takes: {}", RENAMED_KEYS.join(", ")),
                                );
                            }
                        }
                    }
                }
                let from = match pos.as_slice() {
                    [p] => arg_str(p)
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_owned),
                    _ => None,
                };
                if from.is_none() {
                    self.err(
                        span,
                        "`renamed` names the old path",
                        "needs `renamed \"home:old\" to=\"home:new\"`",
                    );
                }
                if to.is_none()
                    && !n
                        .entries()
                        .iter()
                        .any(|e| e.name().is_some_and(|k| k.value() == "to"))
                {
                    self.err(span, "`renamed` needs `to`", "missing").help =
                        Some("write the new path, `to=\"home:new\"`".to_owned());
                }
                Some(Entry::Renamed {
                    from: from?,
                    to: to?,
                    on,
                    span,
                })
            }
            other => {
                self.err(
                    n.name().span(),
                    format!(
                        "an inventory holds `print`, `gone` and `renamed` lines, not `{other}`"
                    ),
                    "unknown",
                )
                .suggest_or(n.name().span(), did_you_mean(other, LINES), || {
                    "a line is `print \"lib:tiles/…\" seed=… system=… colour=… result=ok on=\"…\"`"
                        .to_owned()
                });
                None
            }
        }
    }
}

/// Read an inventory.
///
/// # Errors
/// Every problem found, each pointing at its text with a hint: not KDL v2, a
/// bad header, an unknown line, a line missing its seed, system, colour,
/// result or date.
pub fn decode(name: &str, text: &str) -> Result<Inventory, Vec<DocError>> {
    let src: Src = Arc::new(NamedSource::new(name, text.to_owned()));
    let kdl = match KdlDocument::parse(text) {
        Ok(d) => d,
        Err(e) => return Err(syntax::syntax_errors(&src, text, &e)),
    };
    let (header, header_errors) = crate::header::read(&src, &kdl);
    if header_errors.iter().any(|e| e.code == "dt::format-newer") {
        return Err(header_errors);
    }
    if header
        .as_ref()
        .is_none_or(|h| h.format < crate::header::FORMAT)
    {
        let mut errors = header_errors;
        errors.push(crate::header::format_old(&src, &kdl, header.as_ref()));
        return Err(errors);
    }
    let mut r = Reader {
        src: Arc::clone(&src),
        errors: header_errors,
    };
    let mut inv = Inventory {
        entries: Vec::new(),
        src,
        span: SourceSpan::new(0.into(), 0),
    };
    let mut seen = 0;
    for n in kdl.nodes() {
        match n.name().value() {
            crate::header::NODE => {}
            "inventory" => {
                seen += 1;
                if seen > 1 {
                    r.err(
                        n.name().span(),
                        "a file holds one inventory",
                        "a second one",
                    )
                    .help = Some("keep one `inventory { … }`".to_owned());
                    continue;
                }
                inv.span = n.span();
                if let Some(e) = n.entries().first() {
                    r.err(
                        e.span(),
                        "`inventory` takes no arguments",
                        "put the lines inside",
                    );
                }
                for c in n.iter_children() {
                    if let Some(e) = r.line(c) {
                        inv.entries.push(e);
                    }
                }
            }
            other => {
                r.err(
                    n.name().span(),
                    format!("unknown top-level node `{other}`"),
                    "not inventory",
                )
                .suggest_or(
                    n.name().span(),
                    did_you_mean(other, ["inventory"]),
                    || "an inventory is `inventory { print … }`".to_owned(),
                );
            }
        }
    }
    if seen == 0 && r.errors.is_empty() {
        let span = kdl
            .nodes()
            .first()
            .map_or_else(|| SourceSpan::new(0.into(), 0), KdlNode::span);
        r.err(
            span,
            "this document has no `inventory`",
            "no inventory here",
        )
        .help = Some(
            "an inventory holds `inventory { print \"lib:tiles/ground/1x1-grass\" seed=97 … }`"
                .to_owned(),
        );
    }
    if r.errors.is_empty() {
        Ok(inv)
    } else {
        Err(r.errors)
    }
}

// ---- writing --------------------------------------------------------------------------------------

/// A string as KDL writes it: quoted, with what needs it escaped.
#[must_use]
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A `print` line.
#[must_use]
pub fn print_line(
    id: &Identity,
    ok: bool,
    on: &str,
    hash: Option<&str>,
    from: Option<&str>,
    note: Option<&str>,
) -> String {
    let mut l = format!(
        "print {} seed={} system={} colour={} result={} on={}",
        quote(&id.tile),
        id.seed,
        id.system,
        id.colour.word(),
        if ok { "ok" } else { "failed" },
        quote(on)
    );
    if let Some(h) = hash {
        l.push_str(&format!(" hash={}", quote(h)));
    }
    if let Some(f) = from {
        l.push_str(&format!(" from={}", quote(f)));
    }
    if let Some(n) = note {
        l.push_str(&format!(" note={}", quote(n)));
    }
    l
}

/// A `gone` line.
#[must_use]
pub fn gone_line(id: &Identity, on: &str, note: Option<&str>) -> String {
    let mut l = format!(
        "gone {} seed={} system={} colour={} on={}",
        quote(&id.tile),
        id.seed,
        id.system,
        id.colour.word(),
        quote(on)
    );
    if let Some(n) = note {
        l.push_str(&format!(" note={}", quote(n)));
    }
    l
}

/// A `renamed` line.
#[must_use]
pub fn renamed_line(from: &str, to: &str, on: &str) -> String {
    format!("renamed {} to={} on={}", quote(from), quote(to), quote(on))
}

/// A new inventory's text, holding `lines`.
#[must_use]
pub fn new_text(version: &str, lines: &[String]) -> String {
    let mut t = format!(
        "dharmatiles version={}\n\n// One line for each print, loss and rename: a log, safe to edit by hand.\n// What you have is worked out from it and never stored.\ninventory {{\n",
        quote(version)
    );
    for l in lines {
        t.push_str("    ");
        t.push_str(l);
        t.push('\n');
    }
    t.push_str("}\n");
    t
}

/// `text` with `lines` added as the last children of the node written at
/// `span`: before its closing brace, on lines of their own, every other byte
/// as it was.  A node with no braces is given a body.
#[must_use]
pub(crate) fn insert_in_block(text: &str, span: SourceSpan, lines: &[String]) -> String {
    let start = span.offset();
    let end = (start + span.len()).min(text.len());
    let node = &text[start..end];
    let mut add = String::new();
    for l in lines {
        add.push_str("    ");
        add.push_str(l);
        add.push('\n');
    }
    if node.trim_end().ends_with('}') {
        let close = start + node.trim_end().len() - 1;
        let before = &text[..close];
        let kept = before.trim_end_matches([' ', '\t']);
        let sep = if kept.ends_with('\n') { "" } else { "\n" };
        format!("{kept}{sep}{add}{}", &text[close..])
    } else {
        let at = start + node.trim_end().len();
        format!("{} {{\n{add}}}{}", &text[..at], &text[at..])
    }
}

/// `text` (an inventory that reads) with `lines` added at the end of its log,
/// every other byte as it was.
///
/// # Errors
/// The text does not read as an inventory: its mistakes, so a line is never
/// added to a file that is not understood.
pub fn append(text: &str, lines: &[String]) -> Result<String, Vec<DocError>> {
    let inv = decode(FILE, text)?;
    Ok(insert_in_block(text, inv.span, lines))
}
