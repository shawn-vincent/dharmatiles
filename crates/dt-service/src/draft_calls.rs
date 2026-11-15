//! The draft family of the tile calls: `draft` (a quick build of the text the page holds, answered as the mesh itself)
//! and `step` (an edit and the draft of what it made, in one call).  Their types and the one function each that the
//! program's HTTP handler and a web worker's `Service` both call, written on [`TileHost`].
//!
//! A draft is built by the host's draft lane ([`TileHost::draft`]): the program queues it ahead of its full builds and
//! waits for it, the worker builds it in its own turn.  What the lane answers is a [`Drafted`]; this module turns it
//! into the mesh, or into the refusal the page reads (a mistake in the text is a 422, a draft that a newer one replaced
//! is a 409 marked `superseded`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::api::DraftTier;
use crate::call::{Refusal, Status, Verb};
use crate::edit_calls::EditReq;
use crate::tile_calls::text_rev;
use crate::tile_host::{TileHost, TileRef};

/// `draft`: the text to build a quick draft of.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct DraftReq {
    pub text: String,
    /// A file the tile uses, instead of its own document.
    pub file: Option<String>,
    /// Ask for the debug layer's marks too.
    pub debug: Option<bool>,
}

/// `step`: one step of a drag or a scrub, an edit and the draft of what it made in one request.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct StepReq {
    /// The edit, as `edit` takes it (with the `read` of the text it is made to).
    pub edit: EditReq,
    /// A file the tile uses, instead of its own document.
    pub file: Option<String>,
    pub debug: Option<bool>,
    /// How coarse the draft is: `low` while a drag is going on (the step has to keep up with the pointer), left out
    /// (the usual draft) for the step that ends it and for every other change.
    pub tier: Option<DraftTier>,
}

/// What became of the draft of a step.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub enum StepDraft {
    /// The mesh follows the head.
    Mesh,
    /// The edited text has a mistake (the next check says which): no mesh.
    Mistake,
    /// A newer draft of the tile took its place: no mesh.
    Superseded,
}

/// The head of a step's answer: the edit's result and the revision of its text (what the next step reads at), and
/// what became of the draft.  The answer is this as JSON, its length in four bytes (big-endian) before it, and
/// the draft's mesh after it.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct StepHead {
    pub text: String,
    pub id: Option<String>,
    pub at: Option<usize>,
    /// The revision of `text` (`EditReq::read` of an edit made to it, as a check's `nodesRev`).
    pub rev: String,
    pub draft: StepDraft,
}

/// A draft the host's lane is asked for: the tile, the file the text is of, the text and how to build it.
#[derive(Clone, Debug)]
pub struct DraftAsk {
    pub id: String,
    pub path: String,
    pub file: String,
    pub text: String,
    /// The page has the debug layer on: collect its marks.
    pub marks: bool,
    pub tier: DraftTier,
}

/// What the host's draft lane came to.  A draft that panicked or could not be encoded is the lane's `Err`.
pub enum Drafted {
    /// The draft's mesh (a [`crate::payload`] buffer).
    Mesh(Arc<Vec<u8>>),
    /// The text has a mistake: the check says where.
    Mistake,
    /// A newer draft of the tile took its place.
    Superseded,
}

/// A draft's answer: the mesh's bytes.
pub struct Mesh(pub Arc<Vec<u8>>);

/// A step's answer: the head, and the mesh when the text could be drafted.
pub struct Stepped {
    pub head: StepHead,
    pub mesh: Option<Arc<Vec<u8>>>,
}

impl Stepped {
    /// The answer's bytes: the head as JSON, its length in four bytes (big-endian) before it, and the mesh after it.
    ///
    /// # Errors
    /// [`Refusal`] (internal): the head cannot be written.
    pub fn bytes(&self) -> Result<Vec<u8>, Refusal> {
        let json = serde_json::to_vec(&self.head).map_err(|e| {
            Refusal::failure(Status::Internal, format!("cannot write the answer: {e}"))
                .caused_by(&e)
        })?;
        let mesh = self.mesh.as_deref().map_or(&[][..], Vec::as_slice);
        let mut body = Vec::with_capacity(4 + json.len() + mesh.len());
        // a step's head is a few hundred bytes of JSON plus the text: far under 4 GiB
        body.extend_from_slice(&dt_value::idx(json.len()).to_be_bytes());
        body.extend_from_slice(&json);
        body.extend_from_slice(mesh);
        Ok(body)
    }
}

/// `draft`: a quick build of the text, answered as its mesh.
pub const DRAFT: Verb<DraftReq, Mesh> = Verb::new("draft");
/// `step`: an edit and the draft of what it made.
pub const STEP: Verb<StepReq, Stepped> = Verb::new("step");

/// A draft: queued ahead of the full builds and waited for, so the answer is the mesh itself.
///
/// # Errors
/// [`Refusal`]: not a file of the tile (403); the text has a mistake (422); a newer draft took its place (409,
/// `superseded`); the draft could not be made.
pub fn draft<H: TileHost>(host: &H, tile: &TileRef, v: DraftReq) -> Result<Mesh, Refusal> {
    let file = tile.resolve_file(v.file.as_deref())?;
    match host.draft(DraftAsk {
        id: tile.id.clone(),
        path: tile.path.clone(),
        file,
        text: v.text,
        marks: v.debug == Some(true),
        tier: DraftTier::Draft,
    })? {
        Drafted::Mesh(mesh) => Ok(Mesh(mesh)),
        Drafted::Mistake => Err(Refusal::refused(
            Status::Unprocessable,
            "the text has a mistake",
        )),
        Drafted::Superseded => Err(Refusal::superseded("a newer draft took its place")),
    }
}

/// A step: the edit is made to the text the request carries, and the answer says the new text, its revision and the
/// draft of it (queued ahead of the full builds, as `draft` is).  The edit is refused as `edit` refuses it (a stale
/// `read` is a 409), and a draft that cannot be made of the new text does not undo the edit.
///
/// # Errors
/// [`Refusal`]: the file is not the tile's, the edit is refused, or the draft could not be made.
pub fn step<H: TileHost>(host: &H, tile: &TileRef, v: StepReq) -> Result<Stepped, Refusal> {
    let file = tile.resolve_file(v.file.as_deref())?;
    let made = crate::edit_calls::edit(host, v.edit)?;
    let mut head = StepHead {
        rev: text_rev(&made.text),
        text: made.text,
        id: made.id,
        at: made.at,
        draft: StepDraft::Mesh,
    };
    let mesh = match host.draft(DraftAsk {
        id: tile.id.clone(),
        path: tile.path.clone(),
        file,
        text: head.text.clone(),
        marks: v.debug == Some(true),
        tier: v.tier.unwrap_or_default(),
    })? {
        Drafted::Mesh(mesh) => Some(mesh),
        Drafted::Mistake => {
            head.draft = StepDraft::Mistake;
            None
        }
        Drafted::Superseded => {
            head.draft = StepDraft::Superseded;
            None
        }
    };
    Ok(Stepped { head, mesh })
}
