//! One stream of what happened.  Everything the program learns about a
//! tile while the editor is open (a check, a build with its timing, the
//! printability gate and its numbers, an export, a mistake, a note, a
//! failure of the server's own) is one [`Message`]: a kind, a severity, a
//! time, the thing it is about (tile, stack entry, span) and one stable text
//! and JSON form.  Every surface (the status line, the messages panel, the
//! marks in the source and on the stack) draws from the stream, and an
//! assistant reads it the same way (`GET /api/messages`).
//!
//! A tile's messages are kept in a [`Log`] with a state: **open** until a
//! later check or build of the same [`Scope`] no longer reports them, then
//! **cleared**.  Progress is not a message that is kept: the one that says a
//! build is under way is dropped when the build ends, and its ticks are the
//! `progress` events of the stream.
//!
//! The JSON form (stable; `seq` orders it and resumes it):
//!
//! ```json
//! {"seq": 412, "t": "2026-11-12T10:41:40.771Z", "kind": "error",
//!  "severity": "error", "scope": "build", "code": dt_lang::codes::LOOK,
//!  "tile": "ab6ca6099010d82a", "tile_name": "tree-err",
//!  "node": "tile/tree/0",
//!  "span": {"file": "tree-err.dtile", "line": 8, "col": 9, "offset": 112, "len": 62},
//!  "text": "the trunk (40.0 mm) leaves no room for a crown …",
//!  "help": "make trunk height less, or the top higher",
//!  "data": {}, "state": "open"}
//! ```
//!
//! The text form is [`Message::text`]: a header line, then the words, the
//! help and the numbers under it.

use dt_eval::Diag;
use serde_json::{Value, json};

/// What the message is.  A closed list, so a reader can rely on it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A build is under way (never kept once it ends; the ticks are `progress` events).
    Progress,
    /// A check read and planned the tile.
    Check,
    /// A build finished, with its timing.
    Build,
    /// The printability gate's verdict and its numbers.
    Gate,
    /// A mistake in the tile.
    Error,
    /// A note from the checker (`dt::steep`, `dt::edge`): the tile is fine, but look here.
    Note,
    /// An export, or why it failed.
    Export,
    /// A failure of the program's own, or of the page: not a mistake in the tile.
    Server,
}

impl Kind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Progress => "progress",
            Self::Check => "check",
            Self::Build => "build",
            Self::Gate => "gate",
            Self::Error => "error",
            Self::Note => "note",
            Self::Export => "export",
            Self::Server => "server",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
    Info,
}

impl Severity {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
            Self::Info => "info",
        }
    }
}

/// What reports a message, and so what clears it: a new report of the same
/// scope for the same tile replaces the last.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// A check (the plan pass of a build, or the editor's check of the text).
    Check,
    /// A build, its record and its gate.
    Build,
    /// An export.
    Export,
    /// The server's own failures and the page's: open until the page dismisses them.
    Server,
}

/// Where in a file a message points.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Span {
    pub file: String,
    pub line: usize,
    pub col: usize,
    /// Byte offset and length in the file's text.
    pub offset: usize,
    pub len: usize,
}

/// One thing that happened.
#[derive(Clone, Debug)]
pub(crate) struct Message {
    /// Order in the stream (0 until the log takes it).
    pub seq: u64,
    /// Milliseconds since 1970 (0 until the log takes it).
    pub at_ms: u64,
    pub kind: Kind,
    pub severity: Severity,
    pub scope: Scope,
    pub code: Option<String>,
    /// The tile's id, and its name.
    pub tile: String,
    pub tile_name: String,
    /// The stack entry it is about (its key, the id the page and the view share).
    pub node: Option<String>,
    pub span: Option<Span>,
    pub text: String,
    pub help: Option<String>,
    /// An edit that mends it, when the program has one.
    pub fix: Option<FixJson>,
    pub data: Value,
    /// The tile file it is about, as the program names it (a path in My tiles, or `lib:` in the library): the file a
    /// pasted copy needs, which the tile's name alone does not give (a map's piece is not the document the message is in).
    pub file: Option<String>,
    /// What was being done, in words: the build, picture or export, with its system, printer, seed and tier.
    pub doing: Option<String>,
    /// The line of the file the span is on, as written, so a copy shows the mistake without the file.
    pub source: Option<String>,
    pub open: bool,
}

/// An edit that mends a mistake, on the wire: write `replacement` over the `len` bytes at `offset` of the file the message
/// points into.  The one form of a fix, in a message and in a check's mistake alike.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FixJson {
    pub offset: usize,
    pub len: usize,
    /// The text there now: the page applies the fix only where it still reads this.
    pub was: String,
    pub replacement: String,
    /// The button's words: `Change to dirt`.
    pub label: String,
}

impl From<&FixJson> for dt_eval::Fix {
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

impl From<&dt_eval::Fix> for FixJson {
    fn from(f: &dt_eval::Fix) -> Self {
        Self {
            offset: f.offset,
            len: f.len,
            was: f.was.clone(),
            replacement: f.replacement.clone(),
            label: f.label.clone(),
        }
    }
}

/// Where in a file a message points, on the wire.
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SpanJson {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub offset: usize,
    pub len: usize,
}

/// `open` until a later report of the scope no longer says it, then `cleared`.
#[derive(Clone, Copy, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum MessageState {
    Open,
    Cleared,
}

/// A message on the wire: the stream's `message` events, `GET /api/messages` and the page's panel read this one
/// document.  A message is its own document, shared with the command line and an assistant, so its fields keep
/// `snake_case` everywhere it appears (the page's other JSON is `camelCase`: see `api.rs`).  `data` is the
/// numbers of the kind: [`GateData`] for a `gate`, [`BuildData`] for a `build`, [`ServerData`] for a `server`,
/// [`DiagData`] for an `error` or a `note`; the others (a check, an export, plates, the inventory) are read by
/// no part of the page by name and are not typed.
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MessageJson {
    /// Order in the stream.
    pub seq: u64,
    /// When, ISO 8601 UTC.
    pub t: String,
    pub kind: Kind,
    pub severity: Severity,
    pub scope: Scope,
    pub code: Option<String>,
    /// The tile's id, and its name.
    pub tile: String,
    pub tile_name: String,
    /// The stack entry it is about (its key).
    pub node: Option<String>,
    pub span: Option<SpanJson>,
    pub text: String,
    pub help: Option<String>,
    /// An edit that mends it (a button in the page), when the program has one.
    pub fix: Option<FixJson>,
    /// The tile file it is about, as the program names it.
    pub file: Option<String>,
    /// What was being done (a build, a picture, an export, with its system, printer, seed and tier).
    pub doing: Option<String>,
    /// The line of the file the span is on.
    pub source: Option<String>,
    pub data: Value,
    pub state: MessageState,
    /// The program went wrong (a bug or failure of its own, a gate failure): the editor offers Report this defect.
    /// Never true for a mistake in the document.
    pub reportable: bool,
}

/// `GET /api/messages` as JSON.
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MessagesRes {
    pub epoch: String,
    pub messages: Vec<MessageJson>,
}

/// The `data` of a `gate` message: the printability gate's numbers.  A number the audit does not carry is `null`.
#[derive(Clone, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct GateData {
    pub triangles: Option<u64>,
    pub watertight: Option<bool>,
    pub band_mm2: Option<f64>,
    pub band_budget_mm2: Option<f64>,
    pub overhang_mm2: Option<f64>,
    pub overhang_budget_mm2: Option<f64>,
    /// How many bodies the gate dropped.
    pub dropped: u64,
    pub holes: Option<u64>,
    pub voids_filled: Option<u64>,
    pub failures: Vec<String>,
}

/// The `data` of a finished build's `build` message.
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BuildData {
    pub build_s: Option<f64>,
    pub triangles: Option<u64>,
    pub target: String,
    /// Seconds each stage took, by stage name.
    pub stage_secs: Value,
    pub reused: usize,
    /// `hit` or `miss`.
    pub cache: String,
    /// On a cache hit, how long the tile took when it was made.
    pub made_s: Option<f64>,
}

/// The `data` of a `server` message: a failure of the program's own, or the page's.
#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ServerData {
    /// `server` or `page`.
    pub origin: String,
    pub kind: String,
    pub route: Option<String>,
    pub causes: Value,
    pub backtrace: Value,
    pub version: Option<String>,
}

/// The `data` of an `error` or a `note`.
#[derive(Clone, Default, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DiagData {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub unexpected: Option<bool>,
}

impl Message {
    /// A message about `tile` (an id) called `name`.
    pub(crate) fn new(
        kind: Kind,
        severity: Severity,
        scope: Scope,
        tile: &str,
        name: &str,
        text: impl Into<String>,
    ) -> Self {
        Self {
            seq: 0,
            at_ms: 0,
            kind,
            severity,
            scope,
            code: None,
            tile: tile.to_owned(),
            tile_name: name.to_owned(),
            node: None,
            span: None,
            text: text.into(),
            help: None,
            fix: None,
            data: json!({}),
            file: None,
            doing: None,
            source: None,
            open: true,
        }
    }

    #[must_use]
    pub(crate) fn code(mut self, code: &str) -> Self {
        self.code = Some(code.to_owned());
        self
    }

    #[must_use]
    pub(crate) fn help(mut self, help: Option<&str>) -> Self {
        self.help = help.map(str::to_owned);
        self
    }

    /// The tile file it is about and what was being done.
    #[must_use]
    pub(crate) fn about(mut self, file: &str, doing: impl Into<String>) -> Self {
        self.file = Some(file.to_owned());
        self.doing = Some(doing.into());
        self
    }

    #[must_use]
    pub(crate) fn data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    /// The numbers of the kind, as their type (so the names on the wire are the type's).
    #[must_use]
    pub(crate) fn typed<T: serde::Serialize>(mut self, data: &T) -> Self {
        self.data = json!(data);
        self
    }

    /// A diagnostic as a message: an error (a mistake in the tile, or, for the
    /// codes only the program can raise, a failure of its own), or a note
    /// (`as_note`).  `stack` is the plan's stack items, from which the entry
    /// the span lies in is found.
    pub(crate) fn from_diag(
        d: &Diag,
        scope: Scope,
        as_note: bool,
        tile: &str,
        name: &str,
        stack: &[Value],
    ) -> Self {
        let unexpected = !crate::build::is_document_error(d.code);
        let (kind, severity) = if as_note {
            (Kind::Note, Severity::Note)
        } else if unexpected {
            (Kind::Server, Severity::Error)
        } else {
            (Kind::Error, Severity::Error)
        };
        let mut m = Self::new(kind, severity, scope, tile, name, d.message.clone())
            .code(d.code)
            .help(d.help.as_deref());
        m.fix = d.fix.as_deref().map(FixJson::from);
        if let (Some((file, line, col)), Some((src, span))) = (d.location(), d.at.as_ref()) {
            m.source = src
                .inner()
                .lines()
                .nth(line - 1)
                .map(|l| l.trim_end().to_owned());
            m.file = Some(file.clone());
            m.node = node_at(stack, &file, span.offset());
            m.span = Some(Span {
                file,
                line,
                col,
                offset: span.offset(),
                len: span.len(),
            });
        }
        m.data = json!(DiagData {
            label: (!d.label.is_empty()).then(|| d.label.clone()),
            unexpected: unexpected.then_some(true),
        });
        m
    }

    /// What identifies a message across reports of its scope: the same thing
    /// said again is the same message (it stays open, with its time and seq).
    fn same_as(&self, o: &Self) -> bool {
        self.kind == o.kind
            && self.severity == o.severity
            && self.scope == o.scope
            && self.code == o.code
            && self.tile == o.tile
            && self.node == o.node
            && self.span == o.span
            && self.text == o.text
            && self.help == o.help
            && self.fix == o.fix
            && self.file == o.file
            && self.doing == o.doing
    }

    /// The stable JSON form, as its type (the contract's `MessageJson`).
    pub(crate) fn wire(&self) -> MessageJson {
        MessageJson {
            seq: self.seq,
            t: iso(self.at_ms),
            kind: self.kind,
            severity: self.severity,
            scope: self.scope,
            code: self.code.clone(),
            tile: self.tile.clone(),
            tile_name: self.tile_name.clone(),
            node: self.node.clone(),
            span: self.span.as_ref().map(|s| SpanJson {
                file: s.file.clone(),
                line: s.line,
                col: s.col,
                offset: s.offset,
                len: s.len,
            }),
            text: self.text.clone(),
            help: self.help.clone(),
            fix: self.fix.clone(),
            file: self.file.clone(),
            doing: self.doing.clone(),
            source: self.source.clone(),
            data: self.data.clone(),
            state: if self.open {
                MessageState::Open
            } else {
                MessageState::Cleared
            },
            reportable: self.reportable(),
        }
    }

    /// Did the program go wrong, rather than the maker?  A failure or bug of its own (kind `server`), a gate failure (the
    /// geometry is the program's), or a diagnostic the registry calls a failure or the build marked unexpected.  A mistake
    /// in the document, a note and the records are never a defect.
    pub(crate) fn reportable(&self) -> bool {
        match self.kind {
            Kind::Server => true,
            Kind::Gate => self.severity == Severity::Error,
            Kind::Error => {
                self.data["unexpected"].as_bool() == Some(true)
                    || self
                        .code
                        .as_deref()
                        .and_then(dt_lang::codes::lookup)
                        .is_some_and(|c| c.kind == dt_lang::codes::Kind::Failure)
            }
            _ => false,
        }
    }

    /// [`Message::wire`] as a JSON value (the form a test reads).
    #[cfg(test)]
    #[allow(clippy::expect_used)]
    pub(crate) fn json(&self) -> Value {
        serde_json::to_value(self.wire()).expect("a message is plain data")
    }

    /// What sort of thing this is, for a pasted copy: a mistake in the tile, or the program's own failure (a bug when it
    /// is one), with who found it.
    fn kind_words(&self) -> String {
        match self.kind {
            Kind::Error => "mistake (in the tile)".to_owned(),
            Kind::Note => "note (the tile is fine)".to_owned(),
            Kind::Server => {
                let d = &self.data;
                let kind = d["kind"].as_str().unwrap_or("failure");
                match d["origin"].as_str() {
                    Some(o) => format!("{kind} (the program's own, found by the {o})"),
                    None => format!("{kind} (the program's own)"),
                }
            }
            k => k.as_str().to_owned(),
        }
    }

    /// The one text form, for the panel's Copy, the command line and an
    /// assistant: a header (`10:41:40.771 error dt::look tree-err tile/tree/0
    /// tree-err.dtile:8:9`), then the words, `help:` and the numbers, indented.
    pub(crate) fn text(&self) -> String {
        let mut head = vec![clock(self.at_ms), self.severity.as_str().to_owned()];
        if let Some(c) = &self.code {
            head.push(c.clone());
        } else if self.kind != Kind::Error && self.kind != Kind::Note {
            head.push(self.kind.as_str().to_owned());
        }
        head.push(self.tile_name.clone());
        if let Some(n) = &self.node {
            head.push(n.clone());
        }
        if let Some(s) = &self.span {
            let file = s.file.rsplit('/').next().unwrap_or(&s.file);
            head.push(format!("{file}:{}:{}", s.line, s.col));
        }
        if !self.open {
            head.push("(cleared)".to_owned());
        }
        let mut out = head.join(" ");
        let mut field = |k: &str, v: &str| {
            out.push_str("\n  ");
            out.push_str(k);
            out.push_str(": ");
            out.push_str(v);
        };
        if let Some(f) = &self.file {
            field("file", f);
        }
        // a piece of a map or a print list is not the document the message is in: say which document shows it
        let stem = |f: &str| {
            f.rsplit('/')
                .next()
                .unwrap_or(f)
                .trim_end_matches(".dtile")
                .to_owned()
        };
        if self
            .file
            .as_deref()
            .is_some_and(|f| stem(f) != self.tile_name)
        {
            field("seen from", &self.tile_name);
        }
        if let Some(d) = &self.doing {
            field("doing", d);
        }
        for line in self.text.lines() {
            out.push_str("\n  ");
            out.push_str(line);
        }
        if let Some(h) = &self.help {
            out.push_str("\n  help: ");
            out.push_str(h);
        }
        if let (Some(src), Some(s)) = (&self.source, &self.span) {
            // the line as the command line prints it, the span marked under it
            let gutter = format!("{:>4} | ", s.line);
            let pad = src
                .chars()
                .take(s.col.saturating_sub(1))
                .map(|c| if c == '\t' { '\t' } else { ' ' })
                .collect::<String>();
            let width = s
                .len
                .clamp(1, src.chars().count().saturating_sub(s.col - 1).max(1));
            out.push_str(&format!(
                "\n  {gutter}{src}\n  {}{pad}{}",
                " ".repeat(gutter.len()),
                "^".repeat(width)
            ));
        }
        out.push_str(&format!("\n  kind: {}", self.kind_words()));
        let numbers = scalars(&self.data);
        if !numbers.is_empty() {
            out.push_str("\n  ");
            out.push_str(&numbers);
        }
        // a failure of the program's own carries its causes and where it happened
        if self.kind == Kind::Server {
            for c in self.data["causes"].as_array().into_iter().flatten() {
                if let Some(c) = c.as_str() {
                    out.push_str("\n  caused by: ");
                    out.push_str(c);
                }
            }
            if let Some(b) = self.data["backtrace"].as_str().filter(|b| !b.is_empty()) {
                out.push_str("\n  backtrace:");
                for l in b.lines() {
                    out.push_str("\n    ");
                    out.push_str(l);
                }
            }
        }
        out
    }
}

/// The text form of several messages, in the order given, a blank line apart.
pub(crate) fn render(messages: &[&Message]) -> String {
    messages
        .iter()
        .map(|m| m.text())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `k=v k=v` for the numbers, strings and switches of an object (nothing nested).
fn scalars(data: &Value) -> String {
    let Some(o) = data.as_object() else {
        return String::new();
    };
    o.iter()
        .filter_map(|(k, v)| match v {
            Value::Number(n) => Some(format!(
                "{k}={}",
                n.as_f64()
                    .filter(|_| n.is_f64())
                    .map_or_else(|| n.to_string(), |f| format!("{f:.2}"))
            )),
            Value::Bool(b) => Some(format!("{k}={b}")),
            Value::String(s) if !s.contains('\n') && s.len() < 60 => Some(format!("{k}={s}")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The innermost stack entry (its key) whose span in `file` holds byte `offset`.
fn node_at(stack: &[Value], file: &str, offset: usize) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for it in stack {
        let (Some(key), Some(f), Some(span)) = (
            it["key"].as_str(),
            it["file"].as_str(),
            it["span"].as_array(),
        ) else {
            continue;
        };
        let (Some(start), Some(len)) = (
            span.first().and_then(Value::as_u64),
            span.get(1).and_then(Value::as_u64),
        ) else {
            continue;
        };
        let (start, len) = (usize::try_from(start).ok()?, usize::try_from(len).ok()?);
        if f == file
            && offset >= start
            && offset < start + len.max(1)
            && best.is_none_or(|(w, _)| len < w)
        {
            best = Some((len, key));
        }
    }
    best.map(|(_, k)| k.to_owned())
}

/// `10:41:40.771` (UTC) of a time in milliseconds since 1970.
fn clock(ms: u64) -> String {
    let day = ms % 86_400_000;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        day / 3_600_000,
        day / 60_000 % 60,
        day / 1000 % 60,
        day % 1000
    )
}

/// `2026-11-12T10:41:40.771Z` of a time in milliseconds since 1970.
fn iso(ms: u64) -> String {
    // civil date from days since 1970 (Howard Hinnant's algorithm)
    let days = i64::try_from(ms / 86_400_000).unwrap_or(0) + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{}Z", clock(ms))
}

/// The code of the progress message of a map's pieces loading (a build's has none).
pub(crate) const JOB: &str = "pieces";

/// How many messages a tile keeps: the oldest cleared ones go first.
const KEPT: usize = 200;

/// Every tile's messages, in the order they happened.
#[derive(Default)]
pub(crate) struct Log {
    seq: u64,
    items: Vec<Message>,
}

impl Log {
    fn take(&mut self, mut m: Message, at_ms: u64) -> Message {
        self.seq += 1;
        m.seq = self.seq;
        m.at_ms = at_ms;
        self.items.push(m.clone());
        m
    }

    /// A tile's report of `scope`: what it says now.  Messages of that scope
    /// it no longer reports are cleared; ones it says again stay as they were;
    /// new ones are opened.  A progress message of the tile is dropped (not
    /// kept) when a `Build` is reported.  Returns the messages that changed
    /// (new, or newly cleared), to send.
    pub(crate) fn report(
        &mut self,
        at_ms: u64,
        tile: &str,
        scope: Scope,
        found: Vec<Message>,
    ) -> Vec<Message> {
        let mut changed = Vec::new();
        let mut fresh: Vec<Option<Message>> = found.into_iter().map(Some).collect();
        for old in self
            .items
            .iter_mut()
            .filter(|m| m.open && m.tile == tile && m.scope == scope && m.kind != Kind::Progress)
        {
            let again = fresh
                .iter_mut()
                .find(|f| f.as_ref().is_some_and(|f| old.same_as(f)));
            match again {
                Some(slot) => *slot = None,
                None => {
                    old.open = false;
                    changed.push(old.clone());
                }
            }
        }
        for m in fresh.into_iter().flatten() {
            changed.push(self.take(m, at_ms));
        }
        self.trim(tile);
        changed
    }

    /// One message with nothing to clear (a failure of the server's, the page's).
    pub(crate) fn say(&mut self, at_ms: u64, m: Message) -> Message {
        let tile = m.tile.clone();
        let m = self.take(m, at_ms);
        self.trim(&tile);
        m
    }

    /// The build of `tile` begins: its progress message (dropped when it ends).
    pub(crate) fn begin_progress(&mut self, at_ms: u64, m: Message) -> Message {
        self.take(m, at_ms)
    }

    /// The build of `tile` ended: its progress message is dropped, and said so
    /// (cleared, for the page to remove it), never kept.
    pub(crate) fn end_progress(&mut self, tile: &str) -> Vec<Message> {
        self.end_progress_where(tile, |m| m.code.as_deref() != Some(JOB))
    }

    /// The loading of a map's pieces ended: its progress message is dropped the same way.
    pub(crate) fn end_job(&mut self, tile: &str) -> Vec<Message> {
        self.end_progress_where(tile, |m| m.code.as_deref() == Some(JOB))
    }

    fn end_progress_where(&mut self, tile: &str, which: impl Fn(&Message) -> bool) -> Vec<Message> {
        let mut gone = Vec::new();
        self.items.retain(|m| {
            let drop = m.kind == Kind::Progress && m.tile == tile && which(m);
            if drop {
                let mut m = m.clone();
                m.open = false;
                gone.push(m);
            }
            !drop
        });
        gone
    }

    /// Tile `tile` has gone: its messages with it.
    pub(crate) fn forget(&mut self, tile: &str) {
        self.items.retain(|m| m.tile != tile);
    }

    fn trim(&mut self, tile: &str) {
        let mut n = self.items.iter().filter(|m| m.tile == tile).count();
        while n > KEPT {
            let at = self
                .items
                .iter()
                .position(|m| m.tile == tile && !m.open)
                .or_else(|| self.items.iter().position(|m| m.tile == tile));
            let Some(at) = at else { return };
            self.items.remove(at);
            n -= 1;
        }
    }

    /// Every message, oldest first.
    pub(crate) fn all(&self) -> &[Message] {
        &self.items
    }

    /// The messages asked for: of `tile` (an id, a path or a name) when given,
    /// after `since`, filtered by `keep`.
    pub(crate) fn query<'a>(
        &'a self,
        tile: Option<&str>,
        since: u64,
        keep: impl Fn(&Message) -> bool + 'a,
    ) -> Vec<&'a Message> {
        self.items
            .iter()
            .filter(|m| m.seq > since)
            .filter(|m| tile.is_none_or(|t| m.tile == t || m.tile_name == t))
            .filter(|m| keep(m))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(text: &str) -> Message {
        Message::new(Kind::Note, Severity::Note, Scope::Check, "t1", "pond", text)
            .code(dt_lang::codes::STEEP)
    }

    fn mistake(text: &str) -> Message {
        Message::new(
            Kind::Error,
            Severity::Error,
            Scope::Check,
            "t1",
            "pond",
            text,
        )
        .code(dt_lang::codes::UNKNOWN_ELEMENT)
    }

    #[test]
    fn the_time_is_utc_and_the_date_is_civil() {
        // 2026-11-12T10:41:40.771Z
        let ms = 1_794_480_100_771;
        assert_eq!(clock(ms), "10:41:40.771");
        assert_eq!(iso(ms), "2026-11-12T10:41:40.771Z");
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn the_text_form_is_a_header_then_the_words_the_help_and_the_numbers() {
        let mut m = note("the dirt is forced to 64.2°\nsteeper than its slope")
            .help(Some("soften the heights"))
            .data(json!({"degrees": 64.25, "pins": 2, "label": "here"}));
        m.node = Some("tile/dirt/0".to_owned());
        m.span = Some(Span {
            file: "lib:parts/edges.dtile".to_owned(),
            line: 13,
            col: 59,
            offset: 300,
            len: 12,
        });
        m.seq = 7;
        m.at_ms = 1_794_480_100_771;
        assert_eq!(
            m.text(),
            "10:41:40.771 note dt::steep pond tile/dirt/0 edges.dtile:13:59\n  the dirt is forced to 64.2°\n  steeper than its slope\n  help: soften the heights\n  kind: note (the tile is fine)\n  degrees=64.25 label=here pins=2"
        );
        m.open = false;
        assert!(m.text().lines().next().unwrap().ends_with("(cleared)"));
        assert_eq!(render(&[&m, &m]).matches("\n\n").count(), 1);
    }

    #[test]
    fn the_json_form_is_stable() {
        let mut m = mistake("no such element").help(Some("try dirt"));
        m.seq = 3;
        m.at_ms = 0;
        let j = m.json();
        let keys: Vec<&str> = j.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "code",
                "data",
                "doing",
                "file",
                "fix",
                "help",
                "kind",
                "node",
                "reportable",
                "scope",
                "seq",
                "severity",
                "source",
                "span",
                "state",
                "t",
                "text",
                "tile",
                "tile_name"
            ]
        );
        assert_eq!(j["state"], "open");
        assert_eq!(j["kind"], "error");
        assert_eq!(j["scope"], "check");
        assert_eq!(j["t"], "1970-01-01T00:00:00.000Z");
        assert!(j["span"].is_null() && j["node"].is_null());
    }

    #[test]
    fn a_report_clears_what_it_no_longer_says_and_keeps_what_it_says_again() {
        let mut log = Log::default();
        let first = log.report(10, "t1", Scope::Check, vec![mistake("a"), note("b")]);
        assert_eq!(first.len(), 2);
        assert!(first.iter().all(|m| m.open && m.seq > 0));
        // the same note again, the mistake gone: one cleared, nothing new
        let second = log.report(20, "t1", Scope::Check, vec![note("b")]);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].text, "a");
        assert!(!second[0].open);
        let open: Vec<_> = log
            .query(Some("t1"), 0, |m| m.open)
            .iter()
            .map(|m| (m.text.clone(), m.at_ms))
            .collect();
        assert_eq!(
            open,
            [("b".to_owned(), 10)],
            "an unchanged message keeps its time"
        );
        // another scope, another tile: untouched
        let z = Message::new(
            Kind::Error,
            Severity::Error,
            Scope::Check,
            "t2",
            "marsh",
            "z",
        );
        log.report(30, "t2", Scope::Check, vec![z]);
        let b = Message::new(
            Kind::Build,
            Severity::Info,
            Scope::Build,
            "t1",
            "pond",
            "Built",
        );
        log.report(40, "t1", Scope::Build, vec![b]);
        log.report(50, "t1", Scope::Check, vec![]);
        assert_eq!(log.query(Some("t1"), 0, |m| m.open).len(), 1);
        assert_eq!(log.query(Some("t2"), 0, |m| m.open).len(), 1);
        assert_eq!(log.query(None, 0, |_| true).len(), 4);
        assert_eq!(
            log.query(Some("pond"), 3, |_| true).len(),
            1,
            "since, and by name"
        );
        assert_eq!(log.query(Some("marsh"), 0, |_| true).len(), 1);
    }

    #[test]
    fn progress_is_never_kept() {
        let mut log = Log::default();
        let p = Message::new(
            Kind::Progress,
            Severity::Info,
            Scope::Build,
            "t1",
            "pond",
            "Building",
        );
        log.begin_progress(1, p);
        // the previous build's rows stay while it runs
        log.report(2, "t1", Scope::Check, vec![note("n")]);
        assert_eq!(log.all().len(), 2);
        // a report of the build does not touch it, and ending it drops it
        let gone = log.end_progress("t1");
        assert_eq!(gone.len(), 1);
        assert!(!gone[0].open);
        assert_eq!(log.all().len(), 1);
        assert!(log.all().iter().all(|m| m.kind != Kind::Progress));
    }

    #[test]
    fn a_tile_keeps_a_bounded_log_and_forgets_with_the_tile() {
        let mut log = Log::default();
        for i in 0..(KEPT + 30) {
            log.report(
                i as u64,
                "t1",
                Scope::Check,
                vec![mistake(&format!("m{i}"))],
            );
        }
        assert_eq!(log.query(Some("t1"), 0, |_| true).len(), KEPT);
        assert_eq!(log.query(Some("t1"), 0, |m| m.open).len(), 1);
        log.forget("t1");
        assert!(log.all().is_empty());
    }

    #[test]
    fn the_text_of_a_mistake_and_of_a_bug_carries_what_a_paste_needs() {
        let mut m = Message::new(
            Kind::Error,
            Severity::Error,
            Scope::Server,
            "t1",
            "first-plate",
            "column 1, row 1, west side, at 0.62 is not a seam",
        )
        .code(dt_lang::codes::SEAM)
        .help(Some("the nearest seam is `wsw`"))
        .about(
            "/home/me/My tiles/pool.dtile",
            "the super-low build of pool",
        );
        m.span = Some(Span {
            file: "/home/me/My tiles/pool.dtile".to_owned(),
            line: 4,
            col: 5,
            offset: 0,
            len: 3,
        });
        m.source = Some("    line shore".to_owned());
        let t = m.text();
        let header = format!("{} first-plate pool.dtile:4:5", dt_lang::codes::SEAM);
        for want in [
            header.as_str(),
            "file: /home/me/My tiles/pool.dtile",
            "seen from: first-plate",
            "doing: the super-low build of pool",
            "help: the nearest seam is `wsw`",
            "   4 |     line shore\n",
            "^^^",
            "kind: mistake (in the tile)",
        ] {
            assert!(t.contains(want), "{want} in\n{t}");
        }
        let bug = Message::new(
            Kind::Server,
            Severity::Error,
            Scope::Server,
            "t1",
            "pool",
            "boom",
        )
        .code("bug")
        .data(json!({"origin": "page", "kind": "bug", "causes": ["inner"], "backtrace": "a\nb"}));
        let t = bug.text();
        for want in [
            "kind: bug (the program's own, found by the page)",
            "caused by: inner",
            "backtrace:\n    a\n    b",
        ] {
            assert!(t.contains(want), "{want} in\n{t}");
        }
    }

    #[test]
    fn a_diagnostic_finds_its_entry_by_the_span_it_lies_in() {
        let stack = vec![
            json!({"key": "tile/dirt/0", "file": "a.dtile", "span": [10, 20]}),
            json!({"key": "tile/dirt/0/pin", "file": "a.dtile", "span": [14, 6]}),
            json!({"key": "tile/grass/0", "file": "a.dtile", "span": [40, 10]}),
        ];
        assert_eq!(
            node_at(&stack, "a.dtile", 15).as_deref(),
            Some("tile/dirt/0/pin")
        );
        assert_eq!(
            node_at(&stack, "a.dtile", 25).as_deref(),
            Some("tile/dirt/0")
        );
        assert_eq!(
            node_at(&stack, "a.dtile", 45).as_deref(),
            Some("tile/grass/0")
        );
        assert_eq!(node_at(&stack, "b.dtile", 15), None);
        assert_eq!(node_at(&stack, "a.dtile", 35), None);
    }
}
