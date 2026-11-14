//! The edit operations on a print list (`.dmap`) and an inventory (`.dinv`):
//! one set of operations, addressed by the id every piece carries, with one
//! error type ([`EditError`], the one `.dtile` edits report too).
//!
//! An edit never touches what it does not name: every other byte of the file
//! stays as it was.  [`apply`] is the one call: it reads the text as the
//! document kind it is given (a text that does not read is refused with the
//! document's own mistakes, so nothing is ever added to a file that is not
//! understood), gives every piece an id first when the operation names one
//! (so the editor's first change to a file names all of them in one write),
//! makes the operation, and answers the new text.
//!
//! The same two techniques as every edit here, and only these: an operation
//! that adds or takes away whole lines splices the text at the node's span;
//! one that changes a node's words changes the node in the kdl tree and
//! prints the tree back.  Both keep comments, blank lines and alignment.

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};

use crate::edit::EditError;
use crate::inventory;
use crate::map::{self, At, PrintMode, Want};

/// What kind of document the text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A print list or map: `map { piece … }` and a `plates` line.
    Map,
    /// An inventory: the log of prints, losses and renames.
    Inventory,
}

/// A value a piece's property is set to.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    /// A quoted string (`at`, `bottom`, `system`, `print`).
    Str(String),
    /// A whole number (`rot`, `seed`).
    Int(i64),
}

impl Prop {
    fn value(&self) -> KdlValue {
        match self {
            Self::Str(s) => KdlValue::String(s.clone()),
            Self::Int(i) => KdlValue::Integer(i128::from(*i)),
        }
    }
}

/// One edit.  Every piece is named by its id, never by its place in the file.
#[derive(Debug, Clone)]
pub enum Op<'a> {
    /// Write an id (`pN`) on every piece that has none.  Every other
    /// operation that names a piece does this first; this is the operation
    /// alone.  (Map.)
    GiveIds,
    /// One more copy of a piece: the same tile, seed, system and `print` word
    /// has its `count` raised, else a new `piece` line is added.  Answers the
    /// count now.  (Map.)
    AddOrBump(&'a Want<'a>),
    /// Lines added as the last children of the document's block: pieces of a
    /// map, the log of an inventory.  (Map, Inventory.)
    Append(&'a [String]),
    /// A piece's `count`, 1 to 10 000.  (Map.)
    SetCount {
        /// The piece.
        id: &'a str,
        /// How many.
        count: u32,
    },
    /// A piece taken out, and the pin it had.  (Map.)
    Remove {
        /// The piece.
        id: &'a str,
    },
    /// Properties of a piece set (`Some`) or taken off (`None`).  (Map.)
    SetPiece {
        /// The piece.
        id: &'a str,
        /// The properties and what they become.
        edits: &'a [(&'a str, Option<Prop>)],
    },
    /// A copy of a piece under a new id at a place, put just after it.  (Map.)
    Duplicate {
        /// The piece to copy.
        id: &'a str,
        /// The copy's id.
        new_id: &'a str,
        /// Where the copy stands.
        at: At,
    },
    /// A piece pinned to a plate at a place: an existing pin of it is
    /// replaced, else a `pin` line is added inside the `plates` line (made
    /// when there is none).  (Map.)
    Pin {
        /// The piece.
        id: &'a str,
        /// The plate, from 1.
        plate: u32,
        /// Where on the plate.
        at: At,
        /// Turned a quarter.
        turn: bool,
    },
    /// A piece's pin taken away (no pin: nothing changes).  (Map.)
    Unpin {
        /// The piece.
        id: &'a str,
    },
    /// Every pin taken away.  (Map.)
    UnpinAll,
}

/// What an edit made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// The new text.
    pub text: String,
    /// Whether ids were written on pieces that had none.
    pub gave_ids: bool,
    /// For [`Op::AddOrBump`], the piece's count now.
    pub count: Option<u32>,
}

/// Make `op` on `text`, read as a `kind` document.
///
/// # Errors
/// The text does not read (its mistakes: [`EditError::Unread`]), the piece is
/// not there ([`EditError::NoPiece`]), the operation is not one of this kind
/// of document, or a value is out of range.
pub fn apply(kind: Kind, text: &str, op: &Op) -> Result<Done, EditError> {
    match kind {
        Kind::Inventory => match op {
            Op::Append(lines) => {
                let inv = inventory::decode(inventory::FILE, text).map_err(EditError::Unread)?;
                Ok(Done {
                    text: inventory::insert_in_block(text, inv.span, lines),
                    gave_ids: false,
                    count: None,
                })
            }
            _ => Err(EditError::Cannot(
                "an inventory is added to, nothing else".to_owned(),
            )),
        },
        Kind::Map => map_op(text, op),
    }
}

fn map_op(text: &str, op: &Op) -> Result<Done, EditError> {
    // read first: a file that is not understood is never edited
    let list = map::decode("map", text).map_err(EditError::Unread)?;
    if let Op::GiveIds = op {
        let (t, gave) = give_ids(text, &list)?;
        return Ok(Done {
            text: t,
            gave_ids: gave,
            count: None,
        });
    }
    if let Op::Append(lines) = op {
        return Ok(Done {
            text: inventory::insert_in_block(text, list.span, lines),
            gave_ids: false,
            count: None,
        });
    }
    // every other operation names pieces: they all have ids by now
    let (text, gave) = give_ids(text, &list)?;
    let list = if gave {
        map::decode("map", &text).map_err(EditError::Unread)?
    } else {
        list
    };
    let done = |text: String, count: Option<u32>| Done {
        text,
        gave_ids: gave,
        count,
    };
    match op {
        Op::GiveIds | Op::Append(_) => unreachable_op(),
        Op::AddOrBump(want) => {
            let found = list.pieces.iter().position(|p| {
                same_tile(&p.tile.value, want.tile)
                    && p.seed.as_ref().map(|s| s.value) == want.seed
                    && p.system.as_ref().map(|s| s.value.as_str()) == want.system
                    && (p.print == PrintMode::Always) == want.always
            });
            if let Some(i) = found {
                let n = list.pieces[i].count.value + 1;
                let id = map::piece_ids(&list)[i].clone();
                let t = with_piece(&text, &id, |node| {
                    node.insert("count", KdlValue::Integer(i128::from(n)));
                    Ok(())
                })?;
                return Ok(done(t, Some(n)));
            }
            let line = map::piece_line(
                Some(&map::next_id(&list)),
                want.tile,
                1,
                want.seed,
                want.system,
                want.always,
            );
            Ok(done(
                inventory::insert_in_block(&text, list.span, &[line]),
                Some(1),
            ))
        }
        Op::SetCount { id, count } => {
            if !(1..=10_000).contains(count) {
                return Err(EditError::Cannot(
                    "a count is from 1 to 10 000: take the piece out to have none".to_owned(),
                ));
            }
            let t = with_piece(&text, id, |node| {
                node.insert("count", KdlValue::Integer(i128::from(*count)));
                Ok(())
            })?;
            Ok(done(t, None))
        }
        Op::Remove { id } => {
            if !map::piece_ids(&list).iter().any(|i| i == id) {
                return Err(no_piece(id));
            }
            let t = unpin(&text, &list, id);
            let mut doc = parse(&t)?;
            let nodes = map_nodes(&mut doc)?;
            let at = piece_at(nodes, id)?;
            nodes.remove(at);
            Ok(done(doc.to_string(), None))
        }
        Op::SetPiece { id, edits } => {
            let t = with_piece(&text, id, |node| {
                for (k, v) in *edits {
                    match v {
                        Some(v) => {
                            node.insert(*k, v.value());
                        }
                        None => {
                            // by name: `KdlNode::remove` compares the written form too, and misses
                            node.entries_mut()
                                .retain(|e| e.name().is_none_or(|n| n.value() != *k));
                        }
                    }
                }
                Ok(())
            })?;
            Ok(done(t, None))
        }
        Op::Duplicate { id, new_id, at } => {
            let mut doc = parse(&text)?;
            let nodes = map_nodes(&mut doc)?;
            let at_i = piece_at(nodes, id)?;
            let mut copy = nodes[at_i].clone();
            // the copy is its own piece: it has an id of its own
            if let Some(e) = copy.entries_mut().iter_mut().find(|e| e.name().is_none()) {
                e.set_value(KdlValue::String((*new_id).to_owned()));
                e.clear_format();
            }
            copy.insert("at", KdlValue::String(at.text()));
            nodes.insert(at_i + 1, copy);
            Ok(done(doc.to_string(), None))
        }
        Op::Pin {
            id,
            plate,
            at,
            turn,
        } => {
            if !map::piece_ids(&list).iter().any(|i| i == id) {
                return Err(no_piece(id));
            }
            let mut l = format!(
                "pin {} plate={plate} at={}",
                inventory::quote(id),
                inventory::quote(&at.text())
            );
            if *turn {
                l.push_str(" turn=90");
            }
            if let Some(p) = list.plates.pins.iter().find(|p| p.id.value == *id) {
                let (s, e) = (p.span.offset(), p.span.offset() + p.span.len());
                return Ok(done(format!("{}{l}{}", &text[..s], &text[e..]), None));
            }
            if list.plates.span.is_empty() {
                let sep = if text.ends_with('\n') { "" } else { "\n" };
                return Ok(done(format!("{text}{sep}\nplates {{\n    {l}\n}}\n"), None));
            }
            Ok(done(
                inventory::insert_in_block(&text, list.plates.span, &[l]),
                None,
            ))
        }
        Op::Unpin { id } => Ok(done(unpin(&text, &list, id), None)),
        Op::UnpinAll => {
            let mut out = text.clone();
            for p in list.plates.pins.iter().rev() {
                let l = map::decode("map", &out).map_err(EditError::Unread)?;
                out = unpin(&out, &l, &p.id.value);
            }
            Ok(done(out, None))
        }
    }
}

/// The operations that are answered before the pieces are named.
fn unreachable_op() -> Result<Done, EditError> {
    Err(EditError::Cannot(
        "this operation is made before pieces are named".to_owned(),
    ))
}

fn same_tile(a: &str, b: &str) -> bool {
    a.strip_suffix(".dtile").unwrap_or(a) == b.strip_suffix(".dtile").unwrap_or(b)
}

fn no_piece(id: &str) -> EditError {
    EditError::NoPiece(id.to_owned())
}

fn parse(text: &str) -> Result<KdlDocument, EditError> {
    KdlDocument::parse(text).map_err(|e| EditError::Parse(e.to_string()))
}

/// The nodes of a list's map, in order.
fn map_nodes(doc: &mut KdlDocument) -> Result<&mut Vec<KdlNode>, EditError> {
    let map = doc
        .nodes_mut()
        .iter_mut()
        .find(|n| n.name().value() == "map")
        .ok_or_else(|| EditError::NoNode("map".to_owned()))?;
    Ok(map
        .children_mut()
        .as_mut()
        .ok_or_else(|| EditError::NoNode("map > piece".to_owned()))?
        .nodes_mut())
}

/// The id a piece node is written with (`piece p1 "tile"`), if any.
fn node_id(n: &KdlNode) -> Option<String> {
    let args: Vec<&KdlEntry> = n.entries().iter().filter(|e| e.name().is_none()).collect();
    if args.len() == 2 {
        args[0].value().as_string().map(str::to_owned)
    } else {
        None
    }
}

/// Where piece `id` is among the map's nodes.
fn piece_at(nodes: &[KdlNode], id: &str) -> Result<usize, EditError> {
    nodes
        .iter()
        .position(|n| n.name().value() == "piece" && node_id(n).as_deref() == Some(id))
        .ok_or_else(|| no_piece(id))
}

/// Change piece `id`'s node with `f`, and print the document back.
fn with_piece(
    text: &str,
    id: &str,
    f: impl FnOnce(&mut KdlNode) -> Result<(), EditError>,
) -> Result<String, EditError> {
    let mut doc = parse(text)?;
    let nodes = map_nodes(&mut doc)?;
    let at = piece_at(nodes, id)?;
    f(&mut nodes[at])?;
    Ok(doc.to_string())
}

/// `text` with an id written on every piece that has none (`pN`, as
/// [`map::piece_ids`] says), every other byte kept; and whether any was written.
fn give_ids(text: &str, list: &map::PrintList) -> Result<(String, bool), EditError> {
    if list.pieces.iter().all(|p| p.id.is_some()) {
        return Ok((text.to_owned(), false));
    }
    let ids = map::piece_ids(list);
    let mut doc = parse(text)?;
    let nodes = map_nodes(&mut doc)?;
    for (k, n) in nodes
        .iter_mut()
        .filter(|n| n.name().value() == "piece")
        .enumerate()
    {
        if node_id(n).is_none() {
            n.entries_mut()
                .insert(0, KdlEntry::new(KdlValue::String(ids[k].clone())));
        }
    }
    Ok((doc.to_string(), true))
}

/// The text of a pinned piece's `pin` line taken away, the whole line (no
/// pin: the text as it was).
fn unpin(text: &str, list: &map::PrintList, id: &str) -> String {
    let Some(p) = list.plates.pins.iter().find(|p| p.id.value == id) else {
        return text.to_owned();
    };
    let (mut s, mut e) = (p.span.offset(), p.span.offset() + p.span.len());
    // the whole line: its indent and its newline go with it
    while s > 0 && matches!(text.as_bytes()[s - 1], b' ' | b'\t') {
        s -= 1;
    }
    if text[e..].starts_with('\n') {
        e += 1;
    }
    format!("{}{}", &text[..s], &text[e..])
}
