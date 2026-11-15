//! A text edit, below every transport: the wire types of an edit (`EditReq`, `EditOp`, `EditRes`, `NodeJson`), the
//! fingerprint of a text a node id is read at (`text_rev`), and [`apply`], the one function that turns an `EditOp` into a
//! `dt_doc::ops::Op` and makes it.  The program's `/api/edit` and the browser engine's `edit_tile` both call it, so the
//! editor and the site's cards write tile text the same way: by node id, with a fingerprint, through `dt_doc::ops`.

use dt_doc::ops::{EditError, Kind, Op, Place, Reader, apply_with};
use serde::{Deserialize, Serialize};

/// A control's or argument's value: a number, a string or a switch.
#[derive(Deserialize, Clone, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(untagged)]
pub enum Scalar {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Scalar {
    #[must_use]
    pub fn kdl(&self) -> kdl::KdlValue {
        match self {
            Self::Bool(b) => kdl::KdlValue::Bool(*b),
            Self::Int(i) => kdl::KdlValue::Integer(i128::from(*i)),
            Self::Float(x) => kdl::KdlValue::Float(*x),
            Self::Str(s) => kdl::KdlValue::String(s.clone()),
        }
    }
}

/// Where a new node goes.
#[derive(Deserialize, Clone, Copy, Serialize, Debug, ts_rs::TS)]
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
#[derive(Deserialize, Clone, Serialize, Debug, ts_rs::TS)]
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
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
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
    /// A new node made from its parts (what the editor's place tool puts down): the program writes its words, quotes
    /// and points.  `place` and `id` are as for `Insert`.
    Make {
        text: String,
        node: dt_doc::ops::NewNode,
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

/// `/api/edit`: an edit and the revision of the text its ids were read at (`nodesRev` of a check).  An edit whose `text`
/// is not that text is refused with a 409, so an id like `rock#2` never names another node than the one the page saw.  Only
/// the program's own fix names no node, and needs no `read`.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
pub struct EditReq {
    /// The `nodesRev` of the check the ids come from.
    pub read: Option<String>,
    #[serde(flatten)]
    #[ts(flatten)]
    pub op: EditOp,
}

/// The text after an edit, and (for a change that leaves a node to select) the node: its id and the byte it starts at.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct EditRes {
    pub text: String,
    pub id: Option<String>,
    pub at: Option<usize>,
}

// one node of a text and a text's revision are the check's (`tile_calls`): an edit names nodes by what a check listed
pub use crate::tile_calls::{NodeJson, text_rev};

/// Why an edit was not made.  `E` is what the host's edge-seam table can fail with (a point edit with `snap` reads it).
#[derive(Debug)]
pub enum Failure<E> {
    /// The request is incomplete (no `read`, an empty `names`, no `id` for a place that needs one).
    Bad(String),
    /// The text is not the one the ids were read from: the edit could name the wrong node.
    Stale,
    /// The seam table a snapping point needs could not be had.
    Seams(E),
    /// The edit would make or leave a mistake, or names nothing.
    Refused(EditError),
}

/// What a stale edit says.
pub const STALE: &str = "The text changed since the program listed its nodes, so this edit could name the wrong one; it will be made again once the text is checked.";

impl<E: std::fmt::Display> std::fmt::Display for Failure<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bad(m) => f.write_str(m),
            Self::Stale => f.write_str(STALE),
            Self::Seams(e) => e.fmt(f),
            Self::Refused(e) => e.fmt(f),
        }
    }
}

/// Where a node goes: the place named, and the id it needs.
fn place_of<E>(place: Option<InsertPlace>, id: Option<&str>) -> Result<Place<'_>, Failure<E>> {
    let named = |what: &str| {
        id.ok_or_else(|| Failure::Bad(format!("give the id of the node to write {what}")))
    };
    Ok(match place.unwrap_or(InsertPlace::Stack) {
        InsertPlace::In => Place::In(named("into")?),
        InsertPlace::Before => Place::Before(named("above")?),
        InsertPlace::Tile => Place::Tile,
        InsertPlace::Stack => Place::Stack,
    })
}

/// Make `req`: its op as a `dt_doc::ops::Op` applied to the tile text it carries, read by `reader`.  `seams` gives the
/// edge-seam table, asked for only by a point edit that snaps.
///
/// # Errors
/// [`Failure`]: the request is incomplete or stale, the seam table is unavailable, or the edit is refused.
pub fn apply<E>(
    reader: &dyn Reader,
    req: &EditReq,
    seams: &dyn Fn() -> Result<dt_doc::seams::SeamTable, E>,
) -> Result<EditRes, Failure<E>> {
    let made = |text: &str, op: Op| match apply_with(reader, Kind::Tile, text, &op) {
        Ok(d) => Ok(EditRes {
            text: d.text,
            id: d.id,
            at: d.at,
        }),
        Err(e) => Err(Failure::Refused(e)),
    };
    let bad = Failure::Bad;
    let v = &req.op;
    let text_of = match v {
        EditOp::Insert { text, .. }
        | EditOp::Make { text, .. }
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
        let read = req.read.as_deref().ok_or_else(|| {
            bad("say which reading of the text the ids are from (`read`)".to_owned())
        })?;
        if read != text_rev(text_of) {
            return Err(Failure::Stale);
        }
    }
    match v {
        EditOp::Insert {
            text,
            node,
            place,
            id,
        } => {
            let place = place_of::<E>(*place, id.as_deref())?;
            made(text, Op::Insert { place, node })
        }
        EditOp::Make {
            text,
            node,
            place,
            id,
        } => {
            let place = place_of::<E>(*place, id.as_deref())?;
            made(text, Op::Make { place, node })
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
                return Err(bad("give the names of the children to take out".to_owned()));
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
                Some(seams().map_err(Failure::Seams)?)
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
