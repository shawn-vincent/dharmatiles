//! The tile calls that need nothing of a transport: what a text is read as (`check`), its types and the one function
//! that answers it.  The program's HTTP handler and a web worker's `Service` both call [`check`]; neither knows the
//! other, and neither holds a copy.

use dt_eval::{Diag, Options, Source};
use serde::{Deserialize, Serialize};

use crate::message::{CheckData, Kind, Message, MessageData, Scope, Severity};
use crate::stack::{self, StackItem};

/// FNV-1a, 64 bits: what names a text in a key.
#[must_use]
pub fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The revision of a text that a check names its nodes by: what an edit must carry back.
#[must_use]
pub fn text_rev(text: &str) -> String {
    format!("{:016x}", fnv(text))
}

/// Something the service could not do (never a mistake in a document: that is an answer).  The transport says it
/// loudly (HTTP: a 500).
#[derive(Debug)]
pub struct Failed(pub String);

/// What a check asks of whoever runs it: the build target, and a place to say what it found.
pub trait CheckHost {
    /// The build target the tile is read for.
    fn options(&self) -> Options;

    /// Say what a check of tile `id` found (the messages `make` builds from the tile's name); nothing when the tile is not listed.
    fn report(&self, id: &str, scope: Scope, make: impl FnOnce(&str) -> Vec<Message>);
}

/// What is read of a document: the check's (and the lists') verdict.  A
/// document that does not read is an answer here, not a failure: the text pane
/// shows its mistakes as you type.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct CheckRes {
    pub ok: bool,
    pub errors: Vec<dt_eval::Mistake>,
    pub stack: Option<Vec<StackItem>>,
    pub map: Option<dt_eval::map::TileMap>,
    /// Every node of the text that was read, by the id an edit names it with (`/api/edit`), for a tile.
    pub nodes: Vec<NodeJson>,
    /// The revision of the text those nodes were read from; an edit names them with it (`EditReq::read`).
    #[serde(rename = "nodesRev")]
    #[ts(rename = "nodesRev")]
    pub nodes_rev: Option<String>,
}

/// One node of a tile's text: the id an edit names it by, what it is, and where its text is.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
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

#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct TextReq {
    pub text: String,
    /// A file the tile uses, instead of its own document.
    pub file: Option<String>,
}


/// What a check says: that it read the tile, and each note it made (`dt::steep`, `dt::edge`).  `stack` is the
/// plan's stack items, for the entry each note is about.
#[must_use]
pub fn of_check(c: &dt_eval::Checked, id: &str, name: &str, stack: &[StackItem]) -> Vec<Message> {
    let mut out = Vec::new();
    if let Some(t) = &c.tile {
        out.push(
            Message::new(
                Kind::Check,
                Severity::Info,
                Scope::Check,
                id,
                name,
                format!(
                    "Read {name}: {}×{} squares, {} stack entries",
                    t.cols,
                    t.rows,
                    t.stack.len()
                ),
            )
            .data(MessageData::Check(CheckData {
                cols: t.cols,
                rows: t.rows,
                entries: t.stack.len(),
            })),
        );
    }
    out.extend(
        c.notes()
            .iter()
            .map(|d| Message::from_diag(d, Scope::Check, true, id, name, stack)),
    );
    out
}

/// A check or a build that stopped on mistakes: one message each.
#[must_use]
pub fn of_errors(errs: &[Diag], scope: Scope, id: &str, name: &str, stack: &[StackItem]) -> Vec<Message> {
    errs.iter()
        .map(|d| Message::from_diag(d, scope, false, id, name, stack))
        .collect()
}

/// Read the tile at `tile` from `src` (which holds the text being edited) and say what it came to.  A document that
/// does not read is an answer (`ok: false` with its mistakes), not a failure.
///
/// # Errors
/// [`Failed`]: the program could not describe what it read (a bug, not the user's document).
pub fn check<H: CheckHost>(host: &H, src: &dyn Source, tile: &str) -> Result<CheckRes, Failed> {
    let opts = host.options();
    let id = crate::tile::tile_id(tile);
    // what this check found is said in the stream too: its mistakes, and the notes that were never shown
    let said = |errs: &[Diag]| {
        host.report(&id, Scope::Check, |name| of_errors(errs, Scope::Check, &id, name, &[]));
    };
    let unread = |errs: &[Diag]| CheckRes {
        ok: false,
        errors: dt_eval::mistakes(errs),
        stack: None,
        map: None,
        nodes: Vec::new(),
        nodes_rev: None,
    };
    match dt_eval::check(src, tile, &opts) {
        Ok(c) => {
            let stack = match c
                .tile
                .as_ref()
                .map(|t| stack::items(t, c.solved.as_ref(), opts.system))
            {
                Some(Err(e)) => return Err(Failed(format!("cannot describe the stack of {tile}: {e}"))),
                Some(Ok(items)) => Some(items),
                None => None,
            };
            let own = src
                .read(tile)
                .map_err(|e| Failed(format!("cannot read {tile}: {e}")))?;
            let map = match dt_eval::map::json(&c, &own, opts.units()) {
                Ok(m) => m,
                Err(errs) => {
                    said(&errs);
                    return Ok(unread(&errs));
                }
            };
            host.report(&id, Scope::Check, |name| {
                of_check(&c, &id, name, stack.as_deref().unwrap_or(&[]))
            });
            Ok(CheckRes {
                ok: true,
                errors: Vec::new(),
                stack,
                map,
                // the tile's own text read as KDL: it read, or the check would not have got here
                nodes: dt_doc::nodeid::table(&own)
                    .map_err(|e| Failed(format!("cannot list the nodes of {tile}: {e}")))?
                    .into_iter()
                    .map(NodeJson::from)
                    .collect(),
                nodes_rev: Some(text_rev(&own)),
            })
        }
        Err(errs) => {
            said(&errs);
            Ok(unread(&errs))
        }
    }
}

/// `check`: the tile's text read, and what it came to.  The verb of the HTTP route `/api/tiles/:id/check` and of a
/// worker's `Service::call`; both answer it with [`check`].
pub const CHECK: crate::call::Verb<TextReq, CheckRes> = crate::call::Verb::new("check");
