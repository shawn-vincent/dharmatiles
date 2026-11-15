//! The edit family of the tile calls: `edit` (a text edit by node, made by `dt_doc::ops`) and `save` (the text written
//! if the file is still at the revision the page edited it from).  Their types, and the one function each that the
//! program's HTTP handler and a web worker's `Service` both call through [`TileHost`].

use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::tile_calls::text_rev;
use crate::tile_host::{TileHost, TileRef};

/// A control's or argument's value: a number, a string or a switch.
#[derive(Deserialize, Clone, Serialize, Debug)]
#[derive(ts_rs::TS)]
#[ts(optional_fields = nullable)]
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
#[derive(Deserialize, Clone, Copy, Serialize, Debug)]
#[derive(ts_rs::TS)]
#[ts(optional_fields = nullable)]
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

/// Which value of a node a point is written to.
#[derive(Deserialize, Clone, Serialize, Debug)]
#[derive(ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(rename_all = "camelCase")]
pub enum PointSlot {
    /// The positional argument at this index (a `through`'s third point is 2).
    Arg(usize),
    /// The control of this name (`at`).
    Control(String),
}

/// `/api/edit`: a text edit, one kind by `op`, made by `dt_doc::ops` to the node the `id` names (the ids a check
/// lists in its `nodes`).  An id the text does not have is refused.  Nothing is written to disk.
#[derive(Deserialize, Serialize, Debug)]
#[derive(ts_rs::TS)]
#[ts(optional_fields = nullable)]
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
    /// The tile's description: the `about` line's string made `value`, or the line written when there is none.  The
    /// program writes the string.
    About {
        text: String,
        value: String,
    },
    /// A node's positional argument replaced.
    Arg {
        text: String,
        id: String,
        index: usize,
        value: Scalar,
    },
    /// A node's positional arguments all replaced (a region's `boundedBy` lines, an `except`'s regions).
    Args {
        text: String,
        id: String,
        values: Vec<String>,
    },
    /// A point written where a node holds one, from where it is (squares from the south-west corner): the program
    /// writes the text.  `snap` (a line's point) lands a point on or near the outline by the seam rule.
    Point {
        text: String,
        id: String,
        slot: PointSlot,
        q: [f64; 2],
        snap: Option<bool>,
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
#[derive(Deserialize, Serialize, Debug)]
#[derive(ts_rs::TS)]
#[ts(optional_fields = nullable)]
pub struct EditReq {
    /// The `nodesRev` of the check the ids come from.
    pub read: Option<String>,
    #[serde(flatten)]
    #[ts(flatten)]
    pub op: EditOp,
}

/// The text after an edit, and (for a change that leaves a node to select) the node: its id and the byte it starts at.
#[derive(Serialize, Deserialize, Debug)]
#[derive(ts_rs::TS)]
pub struct EditRes {
    pub text: String,
    pub id: Option<String>,
    pub at: Option<usize>,
}

#[derive(Deserialize, Serialize, Debug)]
#[derive(ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SaveReq {
    pub text: String,
    /// The revision of the file the text was made from.
    pub doc_rev: u64,
    pub file: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(ts_rs::TS)]
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

/// `edit`: see [`EditReq`].
///
/// # Errors
/// [`Refusal`]: the edit names no reading, a stale one, or a node the text does not have, or would add a mistake.
pub fn edit<H: TileHost>(host: &H, req: EditReq) -> Result<EditRes, Refusal> {
    use dt_doc::ops::{Kind, Op, Place, apply_with};
    let opts = host.options();
    let reader = dt_eval::Evaluated {
        base: host.library(),
        path: EDIT_PATH,
        opts: &opts,
    };
    let made = |text: &str, op: Op| match apply_with(&reader, Kind::Tile, text, &op) {
        Ok(d) => Ok(EditRes {
            text: d.text,
            id: d.id,
            at: d.at,
        }),
        Err(e) => Err(refusal(&e)),
    };
    let v = &req.op;
    let text_of = match v {
        EditOp::Insert { text, .. }
        | EditOp::Move { text, .. }
        | EditOp::Remove { text, .. }
        | EditOp::RemoveChildren { text, .. }
        | EditOp::Rename { text, .. }
        | EditOp::Arg { text, .. }
        | EditOp::About { text, .. }
        | EditOp::Args { text, .. }
        | EditOp::Point { text, .. }
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
            return Err(Refusal::refused(
                Status::Conflict,
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
        EditOp::Args { text, id, values } => made(text, Op::SetArgs { id, values }),
        EditOp::About { text, value } => made(text, Op::SetAbout { text: value }),
        EditOp::Point {
            text,
            id,
            slot,
            q,
            snap,
        } => {
            let table = if *snap == Some(true) {
                Some(edge_seams(host)?)
            } else {
                None
            };
            let slot = match slot {
                PointSlot::Arg(i) => dt_doc::ops::Slot::Arg(*i),
                PointSlot::Control(name) => dt_doc::ops::Slot::Control(name),
            };
            made(
                text,
                Op::SetPoint {
                    id,
                    slot,
                    q: *q,
                    seams: table.as_ref(),
                },
            )
        }
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
