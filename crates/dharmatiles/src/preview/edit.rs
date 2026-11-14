//! The editor's calls (`POST`, with this run's token; see `server`):
//!
//! - `/api/edit` `{text, at, name, value}`: set a control on the node whose
//!   name starts at byte `at`, in whichever form it is written, keeping
//!   everything else byte for byte; `value: null` takes it out.  With
//!   `op: insert` `{place: stack|tile, node}` it writes a new node (at the
//!   end of the stack, or before it) and says at which byte; with
//!   `place: before` and `at` it writes the node on a line of its own above
//!   the node at `at` (an entry between two others); with `op: move`
//!   `{at, before?}` it moves the node at `at` whole (its comments and
//!   children) to stand before its sibling at byte `before`, or last, and
//!   says at which byte it now starts; with
//!   `op: remove` it takes out the node at `at`; with `place: in` and `at`
//!   it writes `node` as the last child of the node at `at` (a clause of a
//!   region), and `op: remove-children` `{at, names}` takes out that node's
//!   children called one of `names` (a region's `boundedBy` and `fill`).  Nothing is written to
//!   disk: the page saves the result.
//! - `/api/tiles/ID/offers` `{text, at, file?}`: what the Add menu may offer
//!   at place `at` of the stack, each with the node it would write and
//!   whether the plan lets it stand there (see `offers`).
//! - `/api/names` `{text}`: the definitions, regions, lines and points the
//!   text makes, read by the parser (`{ok, defs, regions, lines, points}`),
//!   for completion between checks.
//! - `/api/check-value` `{element, control, typed}`: what the reader would
//!   refuse in `typed` for that control (`{message, label, help}`, or
//!   `{message: null}`), by the same check as a written value, so the sheet
//!   draws the program's answer and checks nothing itself.
//! - `/api/tiles/ID/check` `{text, file?}`: read and plan the tile with
//!   `text` in place of its document (or of `file`, one it uses): the
//!   errors, the stack and the map (see `map`), under a second, for the
//!   text pane and the overlay as you type.
//! - `/api/tiles/ID/save` `{text, base, file?}`: write the document (or a
//!   file it uses) when what is on disk is still `base`; else `409` with
//!   what is there.  Library files are never written.  The worker sees the
//!   new file and rebuilds.
//! - `/api/tiles/ID/mine`: copy a library tile into My tiles, its `use`s
//!   pointing back at the library, and return the copy's id and state.
//! - `/api/tiles/ID/delete`: delete a tile's file from a folder the preview
//!   lists (never the library): the editor's Undo of the copy it just made.
//! - `/api/tiles/ID/rename` `{name}`: rename the tile's file in its folder
//!   (a tile's file name is its name), never over another file, and
//!   return the new path, id and state (a new path is a new id).  Library
//!   tiles are not renamed.
//! - `/api/new` `{name, size, root?, dir?}`: make a flat, printable tile
//!   called `name` in My tiles (or in folder `dir` of place `root`), never
//!   over another file.
//! - the places and what is done in them: add and remove a place, new, rename and
//!   trash a folder, move, duplicate and trash a tile, undo a trash (see `places`).
//! - `/api/tiles/ID/draft` `{text, file?}`: build a draft of the tile from
//!   `text` (see `dt_eval::Options::draft`) ahead of any full build, and
//!   answer with its mesh: `200` and the binary buffer, `422` when the text
//!   has a mistake, `409` when a newer draft of the tile took its place.
//!   Nothing is written.

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};

use dt_eval::Source;
use serde::{Deserialize, Serialize};

use super::api::{Json, Mistake, Out, Reply, TileRef};
use super::places::{Listed, Nothing};
use super::queue::Outcome;
use super::roots::personal_home;
use super::*;

// ---- the calls' types ------------------------------------------------------------------

/// A control's or argument's value: a number, a string or a switch.
#[derive(Deserialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(untagged)]
pub enum Scalar {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Scalar {
    fn kdl(&self) -> kdl::KdlValue {
        match self {
            Self::Bool(b) => kdl::KdlValue::Bool(*b),
            Self::Int(i) => kdl::KdlValue::Integer(i128::from(*i)),
            Self::Float(x) => kdl::KdlValue::Float(*x),
            Self::Str(s) => kdl::KdlValue::String(s.clone()),
        }
    }
}

/// Where a new node goes.
#[derive(Deserialize, Clone, Copy)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(rename_all = "kebab-case")]
pub enum InsertPlace {
    /// At the end of the stack (the default).
    Stack,
    /// In the `tile` node, before the stack.
    Tile,
    /// On a line of its own above the node `id`.
    Before,
    /// As the last child of the node `id`.
    In,
}

/// `/api/edit`: a text edit, one kind by `op`, made by `dt_doc::ops` to the node the `id` names (the ids a check
/// lists in its `nodes`).  An id the text does not have is refused.  Nothing is written to disk.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum EditOp {
    /// A control set on the node `id` (`value` null or left out takes it out).
    Set {
        text: String,
        id: String,
        name: String,
        value: Option<Scalar>,
    },
    /// A new node.
    Insert {
        text: String,
        node: String,
        place: Option<InsertPlace>,
        id: Option<String>,
    },
    /// A node moved whole to stand before its sibling `before`, or last.
    Move {
        text: String,
        id: String,
        before: Option<String>,
    },
    Remove {
        text: String,
        id: String,
    },
    /// A node's children of those names out.
    RemoveChildren {
        text: String,
        id: String,
        names: Vec<String>,
    },
    /// A line or region renamed, with what names it.
    Rename {
        text: String,
        id: String,
        new: String,
    },
    /// A node's positional argument replaced.
    Arg {
        text: String,
        id: String,
        index: usize,
        value: Scalar,
    },
    /// A node commented out.
    Hide {
        text: String,
        id: String,
    },
    /// The program's own fix for a mistake laid in.
    Fix {
        text: String,
        fix: crate::message::FixJson,
    },
}

/// `/api/edit`: an edit and the revision of the text its ids were read at (`CheckRes::nodes_rev`).  An edit whose `text`
/// is not that text is refused with a 409, so an id like `rock#2` never names another node than the one the page saw.  Only
/// the program's own fix names no node, and needs no `read`.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
pub struct EditReq {
    /// The `nodesRev` of the check the ids come from.
    pub read: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(test, ts(flatten))]
    pub op: EditOp,
}

/// The revision of a text that a check names its nodes by: what an edit must carry back.
pub(super) fn text_rev(text: &str) -> String {
    format!("{:016x}", super::tiers::fnv(text))
}

/// The text after an edit, and (for a change that leaves a node to select) the node: its id and the byte it starts at.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EditRes {
    pub text: String,
    pub id: Option<String>,
    pub at: Option<usize>,
}

/// What is read of a document: the check's (and the lists') verdict.  A
/// document that does not read is an answer here, not a failure: the text pane
/// shows its mistakes as you type.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CheckRes {
    pub ok: bool,
    pub errors: Vec<Mistake>,
    pub stack: Option<Vec<Json>>,
    pub map: Option<Json>,
    /// Every node of the text that was read, by the id an edit names it with (`/api/edit`), for a tile.
    pub nodes: Vec<NodeJson>,
    /// The revision of the text those nodes were read from; an edit names them with it (`EditReq::read`).
    #[serde(rename = "nodesRev")]
    #[cfg_attr(test, ts(rename = "nodesRev"))]
    pub nodes_rev: Option<String>,
}

/// One node of a tile's text: the id an edit names it by, what it is, and where its text is.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct NodeJson {
    pub id: String,
    /// The node's name as written (`rock`).
    pub what: String,
    /// The first word of a node that names itself (`shore` of `line shore`).
    pub name: Option<String>,
    /// The byte its text starts at.
    pub at: usize,
    /// Its length in bytes.
    pub len: usize,
}

impl From<dt_doc::nodeid::Node> for NodeJson {
    fn from(n: dt_doc::nodeid::Node) -> Self {
        Self {
            id: n.id,
            what: n.what,
            name: n.name,
            at: n.at,
            len: n.len,
        }
    }
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct TextReq {
    pub text: String,
    /// A file the tile uses, instead of its own document.
    pub file: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveReq {
    pub text: String,
    /// The revision of the file the text was made from.
    pub doc_rev: u64,
    pub file: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SaveRes {
    pub doc_rev: u64,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Deleted {
    pub path: String,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct RenameReq {
    pub name: String,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct NewReq {
    pub name: Option<String>,
    /// Squares across and up, like `2x1`.
    pub size: Option<String>,
    /// The folder place and the folder in it, when not My tiles.
    pub root: Option<usize>,
    pub dir: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct NamesReq {
    pub text: String,
}

/// The names a text makes; `ok` false (and `message`) when it does not parse: its
/// mistakes are the check's to show, and the page keeps the names it had.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct NamesRes {
    pub ok: bool,
    pub message: Option<String>,
    pub defs: Vec<String>,
    pub regions: Vec<String>,
    pub lines: Vec<String>,
    pub points: Vec<String>,
}

/// A text to colour.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct TokensReq {
    pub text: String,
}

/// How a text is coloured, by `dt_lang::highlight`'s one set of rules: `classes` are the CSS classes (`tk-node`,
/// …), and `pieces` runs three numbers for each coloured piece: its first and last-plus-one UTF-8 byte in the
/// text and its place in `classes`.  What no piece covers is plain.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TokensRes {
    pub classes: Vec<String>,
    pub pieces: Vec<u32>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct CheckValueReq {
    pub element: String,
    pub control: String,
    pub typed: String,
}

/// What the reader refuses in a typed value (`message` null when it takes it).
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CheckValueRes {
    pub message: Option<String>,
    pub label: Option<String>,
    pub help: Option<String>,
}

/// One written value to say the printed size of: the control it is for and
/// the text as written.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PrintedItem {
    pub element: String,
    pub control: String,
    pub typed: String,
}

/// `/api/tiles/ID/printed`: the sizes `items` print on the current target,
/// reading the `$` heights and `let`s of `text` (the tile's document).
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PrintedReq {
    pub text: String,
    pub items: Vec<PrintedItem>,
}

/// What a value prints, mm to 0.01: one end for a number, two for a range;
/// `editable` when the printed size can be typed in.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PrintedSize {
    pub ends: Vec<String>,
    pub editable: bool,
}

/// One answer for each item, in order: `null` where no printed size is shown
/// (not a length, printing as written, or naming what the document does not
/// give).
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PrintedRes {
    pub items: Vec<Option<PrintedSize>>,
}

/// `/api/printed-set`: the value `typed` with its end `end` set so that it
/// prints `mm`, in its own unit.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PrintedSetReq {
    pub typed: String,
    pub end: u32,
    pub mm: f64,
}

/// The value as it is now written; the page writes it like any typed value.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PrintedSetRes {
    pub typed: String,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct TargetReq {
    pub system: Option<String>,
    pub printer: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TargetRes {
    pub system: String,
    pub printer: String,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct RevealReq {
    pub path: String,
}

/// A call that has nothing to give back but that it was done.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(type = "Record<string, never>"))]
pub struct Done {}

/// `/api/messages`: the page reports a failure of its own.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PageMessageReq {
    pub tile: Option<String>,
    pub what: Option<String>,
    pub text: String,
    pub help: Option<String>,
    pub causes: Option<Json>,
    pub backtrace: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct DraftReq {
    pub text: String,
    pub file: Option<String>,
    pub debug: Option<bool>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct ExportReq {
    /// Also write the painted 3MF.
    pub threemf: Option<bool>,
    /// Only say where the files would go.
    #[serde(rename = "where")]
    pub ask_where: Option<bool>,
}

/// An export: where it would go, or what it wrote.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(untagged)]
pub enum ExportRes {
    Where { out: String, shown: String },
    Built(ExportBuilt),
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ExportBuilt {
    pub path: String,
    /// Whether the build passed the printability gate.
    pub ok: bool,
    pub failures: Vec<String>,
    pub files: Vec<String>,
    /// The build's record as `dharmatiles build --json` prints it.
    pub build: Json,
}

/// 128 random bits, hex: the run's token.
pub(super) fn token() -> String {
    let word = |salt: u64| {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u128(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.write_u32(std::process::id());
        h.finish()
    };
    format!("{:016x}{:016x}", word(1), word(2))
}

/// The text of a file, overlaid on the disk: the document being edited.
pub(super) struct Overlay(pub BTreeMap<String, String>);

impl Source for Overlay {
    fn read(&self, path: &str) -> Result<String, String> {
        self.0
            .get(path)
            .cloned()
            .map_or_else(|| dt_eval::Files.read(path), Ok)
    }
}

pub(super) fn bad(msg: impl Into<String>) -> Reply {
    fail::refused("400 Bad Request", msg.into())
}

/// Route one editor call: see [`super::calls`].
#[cfg(test)]
pub(super) fn post(shared: &Shared, path: &str, body: &[u8]) -> super::route::Reply {
    super::calls::dispatch(shared, path, body)
}

/// `/api/messages` `{tile?, what, text, help?, causes?, backtrace?}`: the page
/// reports a failure of its own, so it joins the stream (kind `server`, origin
/// `page`) beside the program's, where an assistant reads it too.
pub(super) fn page_message(shared: &Shared, v: PageMessageReq) -> Out<Done> {
    let tile = v.tile.as_deref().unwrap_or("");
    let name = shared.name_of(tile).unwrap_or_default();
    let error = api::ErrorDetail {
        kind: "bug".to_owned(),
        message: v.text,
        help: v.help,
        causes: v
            .causes
            .as_ref()
            .and_then(Value::as_array)
            .map_or_else(Vec::new, |a| {
                a.iter()
                    .filter_map(|c| c.as_str().map(str::to_owned))
                    .collect()
            }),
        backtrace: v.backtrace.unwrap_or_default(),
        context: api::ErrorContext {
            route: v.what,
            tile: None,
            version: env!("DT_ENGINE_VERSION").to_owned(),
            at_ms: 0,
        },
    };
    shared.say(messages::of_server_error(
        &fail::to_value(&error),
        (tile, &name),
        "page",
    ));
    Ok(Done {})
}

/// Choose the build target: `system` (`db` or `ol`) and/or `printer` (a
/// profile's name), for every build from now on; the tiles built so far
/// are built again.
pub(super) fn target(shared: &Shared, v: TargetReq) -> Out<TargetRes> {
    shared
        .set_target(v.system.as_deref(), v.printer.as_deref())
        .map_err(bad)?;
    let o = shared.options();
    Ok(TargetRes {
        system: o.system,
        printer: o.printer.name,
    })
}

/// A draft: queued ahead of the full builds and waited for, so the reply is
/// the mesh itself.  `debug: true` asks for the debug layer's marks too.
pub(super) fn draft(shared: &Shared, tile: &TileRef, v: DraftReq) -> super::route::Reply {
    let file = match tile.file(v.file.as_deref()) {
        Ok(f) => f,
        Err(e) => return e.into(),
    };
    match shared
        .submit_draft(&tile.id, &tile.path, &file, &v.text, v.debug == Some(true))
        .recv_timeout(Duration::from_secs(120))
    {
        Ok(Outcome::Mesh(mesh)) => super::route::Reply::Bytes(mesh),
        Ok(Outcome::Mistake) => {
            fail::refused("422 Unprocessable Entity", "the text has a mistake").into()
        }
        Ok(Outcome::Superseded) => {
            fail::refused_with("409 Conflict", "a newer draft took its place", |b| {
                b.superseded = Some(true)
            })
            .into()
        }
        Ok(Outcome::Failed(e)) => e.reply().into(),
        Err(e) => fail::failure_from(
            "500 Internal Server Error",
            format!("the draft was not answered ({e}): the build worker may have died"),
            &e,
        )
        .into(),
    }
}

/// The file stem for a tile name: the name as written, with only what a
/// file name cannot hold (path separators, `:`, control characters)
/// turned into `-`, and no leading dot.  A tile's file name is its name.
fn file_stem(name: &str) -> String {
    let out: String = name
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let out = out.trim_start_matches('.').trim();
    if out.is_empty() {
        "Untitled tile".to_owned()
    } else {
        out.to_owned()
    }
}

pub(super) fn tile_name(name: Option<&str>) -> Result<String, Reply> {
    let name = name.unwrap_or("").trim();
    if name.chars().count() > 80 || name.contains(['\n', '\r']) {
        return Err(bad("a tile name is one line of at most 80 characters"));
    }
    Ok(file_stem(name))
}

/// Write `text` as `<stem>.dtile` in `dir`, or `<stem> 2.dtile`, … when
/// that is taken: created new, so another file is never overwritten.
fn write_new(dir: &Path, stem: &str, text: &str) -> Result<PathBuf, (&'static str, Value)> {
    write_new_bytes(dir, stem, text.as_bytes())
}

/// [`write_new`] for bytes: a copy of a document is written as it was read.
pub(super) fn write_new_bytes(
    dir: &Path,
    stem: &str,
    text: &[u8],
) -> Result<PathBuf, (&'static str, Value)> {
    write_new_ext(dir, stem, "dtile", text)
}

/// [`write_new_bytes`] for any document extension (a print list is `dmap`).
pub(super) fn write_new_ext(
    dir: &Path,
    stem: &str,
    ext: &str,
    text: &[u8],
) -> Result<PathBuf, (&'static str, Value)> {
    use std::io::Write;
    for n in 1..1000 {
        let dest = dir.join(if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem} {n}.{ext}")
        });
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(mut f) => {
                return f.write_all(text).map(|()| dest.clone()).map_err(|e| {
                    // best-effort cleanup of the half-made file; the write error is what is reported
                    let _ = std::fs::remove_file(&dest);
                    fail::failure_from(
                        "500 Internal Server Error",
                        format!("cannot write {}: {e}", dest.display()),
                        &e,
                    )
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(fail::failure_from(
                    "500 Internal Server Error",
                    format!("cannot write {}: {e}", dest.display()),
                    &e,
                ));
            }
        }
    }
    Err(fail::failure("500 Internal Server Error", "no free name"))
}

pub(super) fn listed(shared: &Shared, dest: &Path) -> Listed {
    shared.rescan();
    let path = dest.to_string_lossy().into_owned();
    let id = tile_id(&path);
    let tile = shared.summary(&id);
    if tile.is_some() {
        shared.want(&id);
    }
    Listed {
        path,
        id: tile.as_ref().map(|_| id),
        tile,
    }
}

pub(super) fn home(shared: &Shared) -> Result<PathBuf, (&'static str, Value)> {
    let home = shared
        .roots()
        .iter()
        .find_map(|r| match &r.kind {
            RootKind::Dir(d) => Some(d.clone()),
            _ => None,
        })
        .map_or_else(
            || personal_home().map_err(|e| fail::failure("500 Internal Server Error", e)),
            Ok,
        )?;
    std::fs::create_dir_all(&home).map_err(|e| {
        fail::failure_from(
            "500 Internal Server Error",
            format!("cannot make {}: {e}", home.display()),
            &e,
        )
    })?;
    Ok(home)
}

/// Rename a tile's file in its folder.  Never over another file: a link
/// to the new name is made first (which fails if the name is taken), then
/// the old name removed.  Only a change of case, on a disk that ignores
/// case, renames in place.
pub(super) fn rename(shared: &Shared, tile: &TileRef, v: RenameReq) -> Out<Listed> {
    let from = places::owned_tile(shared, &tile.path, "renamed")?;
    let stem = tile_name(Some(&v.name))?;
    let Some(dir) = from.parent() else {
        return Err(bad("the tile has no folder"));
    };
    let to = dir.join(format!("{stem}.dtile"));
    if to == from {
        return Ok(listed(shared, &to));
    }
    match std::fs::hard_link(&from, &to) {
        Ok(()) => {
            if let Err(e) = std::fs::remove_file(&from) {
                // best-effort cleanup of the new link; the failure to remove the old name is what is reported
                let _ = std::fs::remove_file(&to);
                return Err(fail::failure_from(
                    "500 Internal Server Error",
                    format!("cannot rename {}: {e}", from.display()),
                    &e,
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let same = matches!(
                (std::fs::canonicalize(&from), std::fs::canonicalize(&to)),
                (Ok(a), Ok(b)) if a == b
            );
            if !same {
                return Err(fail::refused(
                    "409 Conflict",
                    format!("a tile called “{stem}” is already in this folder"),
                ));
            }
            if let Err(e) = std::fs::rename(&from, &to) {
                return Err(fail::failure_from(
                    "500 Internal Server Error",
                    format!("cannot rename {}: {e}", from.display()),
                    &e,
                ));
            }
        }
        Err(e) => {
            return Err(fail::failure_from(
                "500 Internal Server Error",
                format!("cannot rename {}: {e}", from.display()),
                &e,
            ));
        }
    }
    // its prints in the inventory follow it
    super::inventory::note_rename(shared, &from, &to);
    Ok(listed(shared, &to))
}

/// Delete a tile's file from a folder the preview lists: what the editor's
/// Undo does to the copy it made of a library tile.  Never the library, never
/// a file outside the folders, never anything but a tile document.
pub(super) fn delete(shared: &Shared, tile: &TileRef, _: Nothing) -> Out<Deleted> {
    let path = places::owned_tile(shared, &tile.path, "deleted")?;
    std::fs::remove_file(&path).map_err(|e| {
        fail::failure_from(
            "500 Internal Server Error",
            format!("cannot delete {}: {e}", path.display()),
            &e,
        )
    })?;
    shared.rescan();
    Ok(Deleted {
        path: tile.path.clone(),
    })
}

/// Make a small, valid tile in My tiles and return its slot. Starting from a
/// printable flat square gives the editor something real to select and change;
/// every later gesture remains ordinary `.dtile` text.
pub(super) fn new_tile(shared: &Shared, v: NewReq) -> Out<Listed> {
    // squares across, then up, as the size picker offers: 1 to 8 each way
    let size = v.size.as_deref().unwrap_or("1x1").trim();
    let valid = size.split_once('x').is_some_and(|(c, r)| {
        let ok =
            |n: &str| n.parse::<u32>().is_ok_and(|n| (1..=8).contains(&n)) && !n.starts_with('0');
        ok(c) && ok(r)
    });
    if !valid {
        return Err(bad(
            "size must be squares across and up, like 2x1 (1 to 8 each way)",
        ));
    }
    let stem = tile_name(v.name.as_deref())?;
    // a new document is written in the current format, with its header
    let version = dt_doc::header::stamp(env!("DT_ENGINE_VERSION"));
    let text = format!(
        "dharmatiles version=\"{version}\"\n\ntile size=\"{size}\" seed=1 {{\n    about \"A new tile.\"\n    stack {{\n        base\n        dirt\n    }}\n}}\n"
    );
    // in a folder of a place (the page's folder level), else in My tiles
    let into = match places::new_tile_dir(shared, v.root, v.dir.as_deref()) {
        Some(r) => r?,
        None => home(shared)?,
    };
    let dest = write_new(&into, &stem, &text)?;
    Ok(listed(shared, &dest))
}

/// `/api/names` `{text}`: the names the text makes (definitions, regions,
/// lines, points), by the parser, for completion between checks.  A text
/// that does not parse says so (`{ok: false, message}`): its mistakes are
/// the check's to show, and the page keeps the names it had.
pub(super) fn names(_: &Shared, v: NamesReq) -> Out<NamesRes> {
    Ok(match dt_doc::nodeid::names_in(&v.text) {
        Ok(n) => NamesRes {
            ok: true,
            message: None,
            defs: n.defs,
            regions: n.regions,
            lines: n.lines,
            points: n.points,
        },
        Err(e) => NamesRes {
            ok: false,
            message: Some(e.to_string()),
            defs: Vec::new(),
            regions: Vec::new(),
            lines: Vec::new(),
            points: Vec::new(),
        },
    })
}

/// `/api/tokens` `{text}`: the text's colouring, for the editor's source pane.  Any text colours (a broken one as
/// far as it reads); a text too long to number in 32 bits is the one failure.
pub(super) fn tokens(_: &Shared, v: TokensReq) -> Out<TokensRes> {
    let too_long = |_| bad("the text is too long to colour");
    let mut pieces = Vec::new();
    for p in dt_lang::highlight::tokens(&v.text) {
        pieces.push(u32::try_from(p.start).map_err(too_long)?);
        pieces.push(u32::try_from(p.end).map_err(too_long)?);
        pieces.push(u32::try_from(p.class.index()).map_err(too_long)?);
    }
    Ok(TokensRes {
        classes: dt_lang::highlight::CLASSES
            .iter()
            .map(|c| c.css().to_owned())
            .collect(),
        pieces,
    })
}

/// `/api/check-value`: the reader's check of one typed value.
pub(super) fn check_value(shared: &Shared, v: CheckValueReq) -> Out<CheckValueRes> {
    // a control the schema does not know (the tile's own, a line's) has
    // nothing to check against here
    // a value is read with units, as the reader reads one
    let opts = shared.options();
    let (element, control, typed) = (&v.element, &v.control, &v.typed);
    let flaw = dt_doc::schema::element(element)
        .and_then(|e| e.control_at(control))
        .and_then(|c| dt_doc::decode::check_text_in(c, element, typed, opts.units()));
    // the reader's check first; then the weighting's numbers against their
    // limits, which the reader only meets when the value is lowered
    Ok(match flaw {
        Some(r) => CheckValueRes {
            message: Some(r.message),
            label: Some(r.label.to_owned()),
            help: r.help,
        },
        None => match dt_eval::weighting_flaw_in(typed, opts.units()) {
            Some(w) => CheckValueRes {
                message: Some(w.message),
                label: Some("here".to_owned()),
                help: w.help,
            },
            None => CheckValueRes {
                message: None,
                label: None,
                help: None,
            },
        },
    })
}

/// `/api/tiles/ID/printed`: what each written value prints on the current
/// target (the system's square, the printer's layer), worked out by
/// [`dt_doc::printed`] against the tile's own heights.
pub(super) fn printed(shared: &Shared, t: &TileRef, v: PrintedReq) -> Out<PrintedRes> {
    let opts = shared.options();
    let units = opts.units();
    let src = Overlay(BTreeMap::from([(t.path.clone(), v.text)]));
    // A document with a mistake in it gives no heights: a value that reads a
    // `$` name then has no printed size.  The mistake itself is the check's
    // to show (`/api/tiles/ID/check`), as the text is typed.
    let read = dt_eval::doc::read_with(&src, &t.path, units).ok();
    let consts = read
        .as_ref()
        .and_then(|r| r.tile.as_ref())
        .map(|tile| &tile.heights.consts);
    let items = v
        .items
        .iter()
        .map(|i| {
            let axis = dt_doc::schema::element(&i.element)
                .and_then(|e| e.control_at(&i.control))
                .and_then(|c| c.value)?;
            dt_doc::printed::printed_size(axis, &i.typed, units, consts).map(|p| PrintedSize {
                ends: p.ends,
                editable: p.editable,
            })
        })
        .collect();
    Ok(PrintedRes { items })
}

/// `/api/printed-set`: a printed size typed in, written in the value's own
/// unit (see [`dt_doc::printed::set_printed`]).
pub(super) fn printed_set(shared: &Shared, v: PrintedSetReq) -> Out<PrintedSetRes> {
    dt_doc::printed::set_printed(&v.typed, v.end as usize, v.mm, shared.options().units())
        .map(|typed| PrintedSetRes { typed })
        .map_err(|e| fail::refused("422 Unprocessable Entity", e.0))
}

pub(super) fn edit(_: &Shared, req: EditReq) -> Out<EditRes> {
    use dt_doc::ops::{Kind, Op, Place, apply};
    let made = |text: &str, op: Op| match apply(Kind::Tile, text, &op) {
        Ok(d) => Ok(EditRes {
            text: d.text,
            id: d.id,
            at: d.at,
        }),
        Err(e) => Err(fail::refused("422 Unprocessable Entity", e.to_string())),
    };
    let v = &req.op;
    let text_of = match v {
        EditOp::Insert { text, .. }
        | EditOp::Move { text, .. }
        | EditOp::Remove { text, .. }
        | EditOp::RemoveChildren { text, .. }
        | EditOp::Rename { text, .. }
        | EditOp::Arg { text, .. }
        | EditOp::Hide { text, .. }
        | EditOp::Set { text, .. }
        | EditOp::Fix { text, .. } => text,
    };
    if !matches!(v, EditOp::Fix { .. }) {
        let read = req
            .read
            .as_deref()
            .ok_or_else(|| bad("say which reading of the text the ids are from (`read`)"))?;
        if read != text_rev(text_of) {
            return Err(fail::refused(
                "409 Conflict",
                "The text changed since the program listed its nodes, so this edit could name the wrong one; it will be made again once the text is checked.",
            ));
        }
    }
    match v {
        EditOp::Insert {
            text,
            node,
            place,
            id,
        } => {
            let named = |what: &str| {
                id.as_deref()
                    .ok_or_else(|| bad(format!("give the id of the node to write {what}")))
            };
            let place = match place.unwrap_or(InsertPlace::Stack) {
                InsertPlace::In => Place::In(named("into")?),
                InsertPlace::Before => Place::Before(named("above")?),
                InsertPlace::Tile => Place::Tile,
                InsertPlace::Stack => Place::Stack,
            };
            made(text, Op::Insert { place, node })
        }
        EditOp::Move { text, id, before } => made(
            text,
            Op::Move {
                id,
                before: before.as_deref(),
            },
        ),
        EditOp::Remove { text, id } => made(text, Op::Take { id }),
        EditOp::RemoveChildren { text, id, names } => {
            if names.is_empty() {
                return Err(bad("give the names of the children to take out"));
            }
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            made(text, Op::TakeChildren { id, names: &names })
        }
        EditOp::Rename { text, id, new } => made(
            text,
            Op::Rename {
                id,
                new: new.trim(),
            },
        ),
        EditOp::Arg {
            text,
            id,
            index,
            value,
        } => made(
            text,
            Op::SetArg {
                id,
                index: *index,
                value: value.kdl(),
            },
        ),
        EditOp::Hide { text, id } => made(text, Op::Hide { id }),
        EditOp::Fix { text, fix } => {
            let fixes = [dt_doc::error::Fix::from(fix)];
            made(text, Op::Fix(&fixes))
        }
        EditOp::Set {
            text,
            id,
            name,
            value,
        } => made(
            text,
            Op::SetControl {
                id,
                name,
                value: value.as_ref().map(Scalar::kdl),
            },
        ),
    }
}

pub(super) fn check(shared: &Shared, t: &TileRef, v: TextReq) -> Out<CheckRes> {
    let file = t.file(v.file.as_deref())?;
    let (tile, file, text) = (t.path.as_str(), file.as_str(), v.text);
    // a print list or the inventory is read by its own decoder: its mistakes, at their text
    if file == tile && (super::plates::is_plates(tile) || super::inventory::is_inventory(tile)) {
        return Ok(super::lists::check_text(shared, tile, &text));
    }
    let src = Overlay(BTreeMap::from([(file.to_owned(), text)]));
    let opts = shared.options();
    let id = tile_id(tile);
    // what this check found is said in the stream too: its mistakes, and the notes that were never shown
    let said = |scope, errs: &[dt_eval::Diag]| {
        shared.report(&id, scope, |name| {
            messages::of_errors(errs, scope, &id, name, &[])
        });
    };
    let unread = |errs: &[dt_eval::Diag]| CheckRes {
        ok: false,
        errors: api::mistakes(errs),
        stack: None,
        map: None,
        nodes: Vec::new(),
        nodes_rev: None,
    };
    match dt_eval::check(&src, tile, &opts) {
        Ok(c) => {
            let stack = match c
                .tile
                .as_ref()
                .map(|t| stack::items(t, c.solved.as_ref(), &opts.system))
            {
                Some(Err(e)) => {
                    return Err(fail::failure(
                        "500 Internal Server Error",
                        format!("cannot describe the stack of {tile}: {e}"),
                    ));
                }
                Some(Ok(items)) => Some(items),
                None => None,
            };
            let own = src.read(tile).map_err(|e| {
                fail::failure(
                    "500 Internal Server Error",
                    format!("cannot read {tile}: {e}"),
                )
            })?;
            let map = match dt_eval::map::json(&c, &own, opts.units()) {
                Ok(m) => m,
                Err(errs) => {
                    said(messages::Scope::Check, &errs);
                    return Ok(unread(&errs));
                }
            };
            shared.report(&id, messages::Scope::Check, |name| {
                messages::of_check(&c, &id, name, stack.as_deref().unwrap_or(&[]))
            });
            Ok(CheckRes {
                ok: true,
                errors: Vec::new(),
                stack: stack.map(|s| s.iter().map(api::camel_keys).collect()),
                map: Some(api::camel_keys(&map)),
                // the tile's own text read as KDL: it read, or the check would not have got here
                nodes: dt_doc::nodeid::table(&own)
                    .map_err(|e| {
                        fail::failure(
                            "500 Internal Server Error",
                            format!("cannot list the nodes of {tile}: {e}"),
                        )
                    })?
                    .into_iter()
                    .map(NodeJson::from)
                    .collect(),
                nodes_rev: Some(text_rev(&own)),
            })
        }
        Err(errs) => {
            said(messages::Scope::Check, &errs);
            Ok(unread(&errs))
        }
    }
}

/// May the editor write `file`?  Never the library, nor a read-only file.
pub(super) fn writable(file: &str) -> bool {
    !file.starts_with(dt_eval::LIB)
        && std::fs::metadata(file).is_ok_and(|m| m.is_file() && !m.permissions().readonly())
}

/// `/api/tiles/ID/save` `{text, docRev, file?}`: the text written, if the file is
/// still at the revision the page edited it from (else a 409 with the file's
/// current text and revision).  Answers the revision the file has now.
pub(super) fn save(shared: &Shared, t: &TileRef, v: SaveReq) -> Out<SaveRes> {
    let file = t.file(v.file.as_deref())?;
    if !writable(&file) {
        return Err(fail::refused(
            "403 Forbidden",
            "this file is read only: a library tile is copied into My tiles when it is first changed",
        ));
    }
    let (e, ()) = shared
        .docs
        .edit(&file, Some(v.doc_rev), |_| Ok((v.text, ())))?;
    Ok(SaveRes { doc_rev: e.rev })
}

/// `use "parts/grass"` in a library tile names a library file; in a copy
/// it must say so (`use "lib:parts/grass"`).  The build's files say which.
fn pin_uses(text: &str, files: &BTreeMap<String, Option<SystemTime>>) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        let pinned = t.strip_prefix("use ").and_then(|rest| {
            let rest = rest.trim_start();
            let target = rest.strip_prefix('"')?.split('"').next()?;
            if target.starts_with(dt_eval::LIB) || target.starts_with('/') {
                return None;
            }
            let want = format!("/{}", target.trim_end_matches(".dtile"));
            let lib = files.keys().find(|f| {
                f.starts_with(dt_eval::LIB)
                    && (f.trim_end_matches(".dtile").ends_with(&want)
                        || f.trim_end_matches(".dtile")
                            == format!("{}{}", dt_eval::LIB, &want[1..]))
            })?;
            Some(line.replacen(
                &format!("\"{target}\""),
                &format!("\"{}\"", lib.trim_end_matches(".dtile")),
                1,
            ))
        });
        out.push_str(pinned.as_deref().unwrap_or(line));
    }
    out
}

pub(super) fn mine(shared: &Shared, t: &TileRef, _: Nothing) -> Out<Listed> {
    let (tile, files) = (t.path.as_str(), &t.files);
    if !tile.starts_with(dt_eval::LIB) {
        return Err(bad("only a library tile is copied"));
    }
    let Ok(text) = dt_eval::Files.read(tile) else {
        return Err(fail::refused(
            "404 Not Found",
            format!("cannot read {tile}"),
        ));
    };
    let stem = tile
        .rsplit('/')
        .next()
        .unwrap_or("tile")
        .trim_end_matches(".dtile")
        .to_owned();
    let dest = write_new(&home(shared)?, &stem, &pin_uses(&text, files))?;
    Ok(listed(shared, &dest))
}

/// Export: the same build as `dharmatiles build` (the saved file, through
/// the gate), written to [`export_dir`].  With `where`, only says where.
/// It builds for the target the page chose (the title bar's chip);
/// `threemf` adds the painted 3MF.
pub(super) fn export(shared: &Shared, t: &TileRef, v: ExportReq) -> Out<ExportRes> {
    // asking where is no build; a build waits its turn on the export lane
    if v.ask_where == Some(true) {
        return export_now(shared, t, &v);
    }
    let (tile, rev) = (t.clone(), shared.rev_of(&t.path));
    lane_out(shared.on_lane(
        queue::Lane::Export,
        format!("export|{}", t.id),
        rev,
        &t.id,
        move |shared, _| Ok(export_now(shared, &tile, &v)),
    ))
}

/// What a request gets for an export that ran on its lane (see [`export`]): its answer; a 409 when a newer one took its
/// place; a panic raised again.
pub(super) fn lane_out<T>(r: Result<Out<T>, queue::Interrupted>) -> Out<T> {
    match r {
        Ok(out) => out,
        Err(queue::Interrupted::Superseded) => Err(fail::refused(
            "409 Conflict",
            "a newer request for this took its place",
        )),
        Err(queue::Interrupted::Panicked(p)) => std::panic::resume_unwind(p),
    }
}

fn export_now(shared: &Shared, t: &TileRef, v: &ExportReq) -> Out<ExportRes> {
    let tile = t.path.as_str();
    let threemf = v.threemf.unwrap_or(false);
    let template = if threemf {
        crate::bambu::template(None).map_err(|e| fail::failure("500 Internal Server Error", e))?
    } else {
        None
    };
    let out = export_dir(tile).map_err(|e| fail::failure("500 Internal Server Error", e))?;
    if v.ask_where == Some(true) {
        // shown with ~ for the home folder
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let shown = home
            .as_deref()
            .and_then(|h| out.strip_prefix(h).ok())
            .map_or_else(
                || out.display().to_string(),
                |r| format!("~/{}", r.display()),
            );
        return Ok(ExportRes::Where {
            out: out.display().to_string(),
            shown,
        });
    }
    let palette = shared
        .palette_checked()
        .map_err(|e| fail::failure("500 Internal Server Error", e))?;
    let extras = crate::build::Extras {
        progress: false,
        tile_no: None,
        png: false,
        threemf,
        template: template.as_deref(),
        palette: &palette,
        cache: None,
        store: shared.store.as_deref(),
        tier: dt_store::Tier::Full,
    };
    let opts = shared.options();
    let target = opts.target();
    let t0 = std::time::Instant::now();
    let mut b = match dt_store::build_stored(
        &dt_eval::Files,
        tile,
        &opts,
        &dt_eval::Progress::none(),
        None,
        shared.store.as_deref(),
    ) {
        Ok(b) => b,
        Err(errs) => {
            let id = tile_id(tile);
            shared.report(&id, messages::Scope::Export, |name| {
                messages::of_errors(&errs, messages::Scope::Export, &id, name, &[])
            });
            return Err(fail::mistakes(&errs));
        }
    };
    // a file of yours goes to out/<target>/<name>-<target>.stl, not under
    // the folders it sits in (a library tile mirrors the library, as build does)
    if !tile.starts_with("lib:") {
        let stem = Path::new(tile)
            .file_stem()
            .map_or_else(|| "tile".to_owned(), |s| s.to_string_lossy().into_owned());
        b.out = PathBuf::from(&target).join(format!("{stem}-{target}.stl"));
    }
    let built_s = t0.elapsed().as_secs_f64();
    let failures = b.audit.failures(&opts.printer);
    let files = crate::build::write(&b, &failures, built_s, &out, extras)
        .map_err(|e| fail::failure("500 Internal Server Error", e))?;
    let record = crate::build::tile_json(&b, &failures, built_s);
    let written: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
    let id = tile_id(tile);
    shared.report(&id, messages::Scope::Export, |name| {
        vec![messages::of_export(&record, &written, &id, name)]
    });
    Ok(ExportRes::Built(ExportBuilt {
        path: tile.to_owned(),
        ok: failures.is_empty(),
        failures,
        files: written,
        build: record,
    }))
}

/// Where `tile` exports to: never the library.  A library tile (`lib:`,
/// or a file inside the library folder) goes to `out/` in My tiles; any
/// other file to its project's `out/` (beside the nearest
/// `dharmatiles.dtile` above it), else `out/` beside it.
///
/// # Errors
/// When the library folder or the tile's folder cannot be resolved to compare
/// them, or there is no personal home: a guess could write into the library.
pub(super) fn export_dir(tile: &str) -> Result<PathBuf, String> {
    let in_library = if tile.starts_with("lib:") {
        true
    } else if let Some(lib) = dt_eval::source::library_dir() {
        let l = lib
            .canonicalize()
            .map_err(|e| format!("cannot resolve the library folder {}: {e}", lib.display()))?;
        let folder = Path::new(tile)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let f = folder
            .canonicalize()
            .map_err(|e| format!("cannot resolve the folder of {tile}: {e}"))?;
        f.starts_with(l)
    } else {
        false
    };
    if in_library {
        return Ok(personal_home()?.join("out"));
    }
    let dir = Path::new(tile).parent().unwrap_or_else(|| Path::new("."));
    let project = dt_eval::discover(dir);
    if let Some(why) = &project.problem {
        super::tell_once(&format!(
            "note: {why}; the project file is ignored, so exports go to out/"
        ));
    }
    Ok(project.out)
}

/// Show a written file in the Finder (macOS `open -R`; elsewhere the
/// folder is opened).  Only an STL or 3MF that exists.
pub(super) fn reveal(_: &Shared, v: RevealReq) -> Out<Done> {
    let p = PathBuf::from(&v.path);
    let kind = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !matches!(kind, "stl" | "3mf") || !p.is_file() {
        return Err(bad("only a written STL or 3MF"));
    }
    let r = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&p)
            .status()
    } else {
        let dir = p.parent().unwrap_or_else(|| Path::new("."));
        std::process::Command::new(if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        })
        .arg(dir)
        .status()
    };
    r.map_err(|e| fail::failure("500 Internal Server Error", e.to_string()))?;
    Ok(Done {})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_points_its_uses_at_the_library() {
        let files = BTreeMap::from([
            ("lib:tiles/water/a.dtile".to_owned(), None),
            ("lib:parts/grass.dtile".to_owned(), None),
        ]);
        let text = "// a\nuse \"parts/grass\" lawn\nuse \"lib:parts/trees\" oak\ntile {}\n";
        assert_eq!(
            pin_uses(text, &files),
            "// a\nuse \"lib:parts/grass\" lawn\nuse \"lib:parts/trees\" oak\ntile {}\n"
        );
    }

    fn new_named(shared: &Shared, name: &str) -> Listed {
        new_tile(
            shared,
            NewReq {
                name: Some(name.to_owned()),
                size: None,
                root: None,
                dir: None,
            },
        )
        .unwrap()
    }

    fn tile_of(path: &str) -> TileRef {
        TileRef {
            id: tile_id(path),
            path: path.to_owned(),
            files: BTreeMap::new(),
        }
    }

    fn status(r: Out<impl Sized>) -> &'static str {
        r.map_or_else(|e| e.0, |_| "200 OK")
    }

    #[test]
    fn a_new_tile_is_safe_named_valid_and_listed() {
        let dir = std::env::temp_dir().join(format!("dt-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
        let shared = Shared::with_roots(roots, &[], &dt_eval::Options::default());
        let made = new_tile(
            &shared,
            NewReq {
                name: Some("Shore / first try".to_owned()),
                size: Some("2x1".to_owned()),
                root: None,
                dir: None,
            },
        )
        .unwrap();
        let path = made.path.as_str();
        assert!(path.ends_with("Shore - first try.dtile"), "{path}");
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("tile size=\"2x1\""));
        assert!(text.contains("base\n        dirt\n"));
        dt_eval::check(&dt_eval::Files, path, &Options::default()).unwrap();
        assert!(made.id.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_delete_takes_out_a_tile_of_a_listed_folder_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("dt-delete-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
        let shared = Shared::with_roots(roots, &[], &dt_eval::Options::default());
        let pond = new_named(&shared, "Pond").path;
        let r = delete(
            &shared,
            &tile_of("lib:tests/ground/1x1-flat.dtile"),
            Nothing {},
        );
        assert_eq!(status(r), "400 Bad Request");
        let outside = std::env::temp_dir().join(format!("dt-outside-{}.dtile", std::process::id()));
        std::fs::write(&outside, "x").unwrap();
        let r = delete(&shared, &tile_of(&outside.to_string_lossy()), Nothing {});
        assert_eq!(status(r), "400 Bad Request");
        assert!(outside.exists());
        let _ = std::fs::remove_file(&outside);
        let r = delete(&shared, &tile_of(&pond), Nothing {});
        assert_eq!(status(r), "200 OK");
        assert!(!Path::new(&pond).exists());
        assert!(
            shared.summary(&tile_id(&pond)).is_none(),
            "the list drops it at once"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rename_moves_the_file_and_never_clobbers() {
        let dir = std::env::temp_dir().join(format!("dt-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
        let shared = Shared::with_roots(roots, &[], &dt_eval::Options::default());
        let pond = new_named(&shared, "Pond").path;
        let marsh = new_named(&shared, "Marsh").path;
        let name = |n: &str| RenameReq { name: n.to_owned() };

        let moved = rename(&shared, &tile_of(&pond), name("Still lake")).unwrap();
        let lake = moved.path.clone();
        assert!(lake.ends_with("Still lake.dtile"), "{lake}");
        assert!(!Path::new(&pond).exists());
        assert!(moved.id.is_some());

        let r = rename(&shared, &tile_of(&marsh), name("Still lake"));
        assert_eq!(status(r), "409 Conflict");
        assert!(Path::new(&marsh).exists());

        let r = rename(
            &shared,
            &tile_of("lib:tests/ground/1x1-flat.dtile"),
            name("x"),
        );
        assert_eq!(status(r), "400 Bad Request");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
