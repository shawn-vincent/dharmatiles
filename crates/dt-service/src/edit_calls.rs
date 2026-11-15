//! The edit family of the tile calls: `edit` (a text edit by node, made by `dt_doc::ops`) and `save` (the text written
//! if the file is still at the revision the page edited it from).  Their types, and the one function each that the
//! program's HTTP handler and a web worker's `Service` both call through [`TileHost`].

use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::tile_host::{TileHost, TileRef};

// An edit's types and the one function that makes it are `crate::edit`'s (the site's cards call it with no host); this is
// the host's way in.
pub use crate::edit::{EditOp, EditReq, EditRes, InsertPlace, PointSlot, Scalar};

#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveReq {
    pub text: String,
    /// The revision of the file the text was made from.
    pub doc_rev: u64,
    pub file: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct SaveRes {
    pub doc_rev: u64,
}

/// Where the document an edit is read as stands, for a reader that needs a place to resolve a `use` from: the
/// library's tiles folder, so a part is found as it is from a library tile.
pub const EDIT_PATH: &str = "lib:tiles/edit.dtile";

fn bad(msg: impl Into<String>) -> Refusal {
    Refusal::refused(Status::BadRequest, msg)
}

/// How an edit that cannot be made is told: the mistakes it would have added are a 422's `errors`, so the page
/// says why in the words it says any mistake in (`whyOf`); any other refusal is its words.
#[must_use]
pub fn refusal(e: &dt_doc::ops::EditError) -> Refusal {
    use dt_doc::ops::EditError;
    let EditError::Adds(added) = e else {
        return Refusal::refused(Status::Unprocessable, e.to_string());
    };
    let list: Vec<dt_eval::Mistake> = added
        .iter()
        .map(|f| dt_eval::Mistake {
            code: f.code.clone(),
            message: f.message.clone(),
            file: None,
            line: None,
            column: None,
            help: Some(
                "The edit was not made: it would add a mistake the text does not have.".to_owned(),
            ),
            fix: None,
            unexpected: false,
        })
        .collect();
    let message = added
        .first()
        .map_or_else(|| e.to_string(), |f| f.message.clone());
    let mut r = Refusal::refused(Status::Unprocessable, message);
    r.errors = Some(list);
    r
}

/// The seams of the edge contract, read from the host's library.
fn edge_seams(host: &impl TileHost) -> Result<dt_doc::seams::SeamTable, Refusal> {
    match dt_eval::doc::read(host.library(), dt_eval::doc::EDGES) {
        Ok(r) => r.file.doc.seams.clone().ok_or_else(|| {
            Refusal::failure(Status::Internal, "the edge contract has no `seams` block")
        }),
        Err(es) => Err(Refusal::of_mistakes(&es)),
    }
}

/// `edit`: a text edit.  Nothing is written.
pub const EDIT: Verb<EditReq, EditRes> = Verb::new("edit");
/// `save`: the text written, against the revision the page edited it from.
pub const SAVE: Verb<SaveReq, SaveRes> = Verb::new("save");

/// `/api/tiles/ID/save` `{text, docRev, file?}`: the text written, if the file is still at the revision the page
/// edited it from (else a 409 with the file's current text and revision).  Answers the revision the file has now.
///
/// # Errors
/// [`Refusal`]: the file is not the tile's, is read only, is stale, or cannot be written.
pub fn save<H: TileHost>(host: &H, t: &TileRef, v: SaveReq) -> Result<SaveRes, Refusal> {
    let file = t.resolve_file(v.file.as_deref())?;
    if !crate::tile_host::writable(host.storage(), &file) {
        return Err(Refusal::refused(
            Status::Forbidden,
            "this file is read only: a library tile is copied into My tiles when it is first changed",
        ));
    }
    let (e, ()) = host
        .docs()
        .edit(&file, Some(v.doc_rev), |_| Ok((v.text, ())))
        .map_err(crate::docs::DocFail::into_refusal)?;
    Ok(SaveRes { doc_rev: e.rev })
}

/// `edit`: see [`EditReq`]: [`crate::edit::apply`], reading as the host's library does, its failures as refusals.
///
/// # Errors
/// [`Refusal`]: the edit names no reading, a stale one, or a node the text does not have, or would add a mistake.
pub fn edit<H: TileHost>(host: &H, req: EditReq) -> Result<EditRes, Refusal> {
    use crate::edit::Failure;
    let opts = host.options();
    let reader = dt_eval::Evaluated {
        base: host.library(),
        path: EDIT_PATH,
        opts: &opts,
    };
    crate::edit::apply(&reader, &req, &|| edge_seams(host)).map_err(|f| match f {
        Failure::Bad(m) => bad(m),
        Failure::Stale => Refusal::refused(Status::Conflict, crate::edit::STALE),
        Failure::Seams(r) => r,
        Failure::Refused(e) => refusal(&e),
    })
}
