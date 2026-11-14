//! The edit operations on a tile (`.dtile`), a print list (`.dmap`) and an inventory
//! (`.dinv`): one set of operations, addressed by the id every piece or node carries, with
//! one error type ([`EditError`]).  A tile's nodes are named by [`crate::nodeid`]
//! (`tile/stack/rock#2`, `tile/line shore`); the tile's operations are in [`tile`].
//!
//! An edit never touches what it does not name: every other byte of the file
//! stays as it was.  [`apply`] is the one call: it reads the text as the
//! document kind it is given (a text that does not read is refused with the
//! document's own mistakes, so nothing is ever added to a file that is not
//! understood), gives every piece an id first when the operation names one
//! (so the editor's first change to a file names all of them in one write),
//! makes the operation, and answers the new text.
//!
//! A tile's edits are made one way: each works out the byte ranges it changes from the
//! text's own spans and writes only those (see [`tile`]), so comments, blank lines and
//! alignment stay.  A print list's edits splice at a piece's span, or change the piece in
//! the kdl tree and print the tree back; both keep comments, blank lines and alignment.

pub mod tile;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};

use crate::error::Fix;
use crate::inventory;
use crate::map::{self, At, PrintMode, Want};

/// Why an edit could not be made.
#[derive(Debug, Clone)]
pub enum EditError {
    /// The text is not valid KDL v2.
    Parse(String),
    /// No node has this id: the text is not the text the id was made from.
    NoNode(String),
    /// The node has no such entry or control.
    NoEntry(String),
    /// A name that can't be used here, and why.
    BadName(String),
    /// The edit cannot be made on text written this way, and what to change.
    Cannot(String),
    /// No piece of a print list has this id.
    NoPiece(String),
    /// The text does not read as the document it should be: its own mistakes,
    /// so nothing is ever edited in a file that is not understood.
    Unread(Vec<crate::error::DocError>),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Parse(e) => write!(f, "not KDL v2: {e}"),
            EditError::NoNode(s) => write!(
                f,
                "no node matches `{s}`: the text has changed since it was read"
            ),
            EditError::NoEntry(s) => write!(f, "the node has no `{s}`"),
            EditError::BadName(s) | EditError::Cannot(s) => f.write_str(s),
            EditError::NoPiece(id) => write!(f, "the list has no piece {id}"),
            EditError::Unread(e) => f.write_str(
                e.first()
                    .map_or("the document does not read", |d| d.message.as_str()),
            ),
        }
    }
}

impl std::error::Error for EditError {}

/// What kind of document the text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A print list or map: `map { piece … }` and a `plates` line.
    Map,
    /// An inventory: the log of prints, losses and renames.
    Inventory,
    /// A tile (or a part): `tile { stack { … } }`, read as KDL v2 and changed at its nodes.
    Tile,
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
    /// A control of a node written, in whichever form it is written (`height=35` on the line, or `height 35`
    /// on its own line), else on the line as a property; `None` takes it out.  `group/sub` is a property of the
    /// group's own line (`stones/height`).  (Tile.)
    SetControl {
        /// The node.
        id: &'a str,
        /// The control.
        name: &'a str,
        /// What it becomes, or `None` to take it out.
        value: Option<KdlValue>,
    },
    /// A node's positional argument replaced: a `within`'s distance, say.  (Tile.)
    SetArg {
        /// The node.
        id: &'a str,
        /// Which argument, from 0.
        index: usize,
        /// What it becomes.
        value: KdlValue,
    },
    /// Every positional argument of a node replaced by `values`, each a word or a string: a region's `boundedBy`
    /// lines, an `except`'s regions.  (Tile.)
    SetArgs {
        /// The node.
        id: &'a str,
        /// What the arguments become, at least one.
        values: &'a [String],
    },
    /// A point written where a node holds one (a line's `through`, a region's `fill`, a thing's `at=`): the
    /// program writes the point from where it is, in squares from the south-west corner, so no page makes
    /// the text of one.  With `seams`, a point on or near the outline lands by the seam rule
    /// ([`crate::seams::SeamTable::snap`]); without, it is a free pair.  (Tile.)
    SetPoint {
        /// The node.
        id: &'a str,
        /// Which of its values.
        slot: Slot<'a>,
        /// Where, in squares from the south-west corner.
        q: [f64; 2],
        /// The edge contract's seams, for a point of a line.
        seams: Option<&'a crate::seams::SeamTable>,
    },
    /// A new node (one or more lines of KDL, unindented) written in a place.  (Tile.)
    Insert {
        /// Where it goes.
        place: Place<'a>,
        /// The node.
        node: &'a str,
    },
    /// A node moved whole (its children, the comment lines above it and a comment after it on its line) to
    /// stand before its sibling `before`, or last.  (Tile.)
    Move {
        /// The node.
        id: &'a str,
        /// The sibling it goes before.
        before: Option<&'a str>,
    },
    /// A node taken out with its children, and its line when it had the line to itself.  (Tile.)
    Take {
        /// The node.
        id: &'a str,
    },
    /// Every child of a node named one of `names` taken out.  (Tile.)
    TakeChildren {
        /// The node.
        id: &'a str,
        /// The children's names.
        names: &'a [&'a str],
    },
    /// A node commented out (`/-`).  (Tile.)
    Hide {
        /// The node.
        id: &'a str,
    },
    /// A line or region renamed, and every place the tile names it.  (Tile.)
    Rename {
        /// The `line` or `region` node.
        id: &'a str,
        /// Its new name.
        new: &'a str,
    },
    /// The program's own fixes for mistakes laid in, each only where the text still reads what it was made for.
    /// (Tile.)
    Fix(&'a [Fix]),
}

/// Which value of a node [`Op::SetPoint`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot<'a> {
    /// The positional argument at this index, from 0 (`through "a" "b"`: 1 is `"b"`).
    Arg(usize),
    /// The control of this name (`at`).
    Control(&'a str),
}

/// Where [`Op::Insert`] puts a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place<'a> {
    /// The end of the tile's `stack` (made when the tile has none).
    Stack,
    /// The tile's own body, before its `stack`: a `line` or a `region`.
    Tile,
    /// Before this node, among its siblings: a stack entry written between two others.
    Before(&'a str),
    /// As the last child of this node: a clause in a region.
    In(&'a str),
}

/// What an edit made.
#[derive(Debug, Clone, PartialEq)]
pub struct Done {
    /// The new text.
    pub text: String,
    /// Whether ids were written on pieces that had none.
    pub gave_ids: bool,
    /// For [`Op::AddOrBump`], the piece's count now.
    pub count: Option<u32>,
    /// For a tile's operation that leaves a node to select (set, insert, move), its id in the new text.
    pub id: Option<String>,
    /// The byte that node starts at in the new text.
    pub at: Option<usize>,
    /// For [`Op::Fix`], which fixes were made and how many wait.
    pub mended: Option<Mended>,
}

/// What [`Op::Fix`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct Mended {
    /// The fixes that were made, in the order of the text.
    pub applied: Vec<Fix>,
    /// How many were left for another round: fixes whose text overlaps one made (the next check says them again
    /// if they still hold).
    pub deferred: usize,
}

impl Done {
    /// An edit that made `text` and nothing else to say.
    #[must_use]
    pub fn text(text: String) -> Self {
        Self {
            text,
            gave_ids: false,
            count: None,
            id: None,
            at: None,
            mended: None,
        }
    }
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
                Ok(Done::text(inventory::insert_in_block(
                    text, inv.span, lines,
                )))
            }
            _ => Err(EditError::Cannot(
                "an inventory is added to, nothing else".to_owned(),
            )),
        },
        Kind::Map => map_op(text, op),
        Kind::Tile => tile::apply(text, op),
    }
}

fn map_op(text: &str, op: &Op) -> Result<Done, EditError> {
    // read first: a file that is not understood is never edited
    let list = map::decode("map", text).map_err(EditError::Unread)?;
    if let Op::GiveIds = op {
        let (t, gave) = give_ids(text, &list)?;
        return Ok(Done {
            gave_ids: gave,
            ..Done::text(t)
        });
    }
    if let Op::Append(lines) = op {
        return Ok(Done::text(inventory::insert_in_block(
            text, list.span, lines,
        )));
    }
    // every other operation names pieces: they all have ids by now
    let (text, gave) = give_ids(text, &list)?;
    let list = if gave {
        map::decode("map", &text).map_err(EditError::Unread)?
    } else {
        list
    };
    let done = |text: String, count: Option<u32>| Done {
        gave_ids: gave,
        count,
        ..Done::text(text)
    };
    match op {
        Op::GiveIds | Op::Append(_) => unreachable_op(),
        Op::SetControl { .. }
        | Op::SetArg { .. }
        | Op::SetArgs { .. }
        | Op::SetPoint { .. }
        | Op::Insert { .. }
        | Op::Move { .. }
        | Op::Take { .. }
        | Op::TakeChildren { .. }
        | Op::Hide { .. }
        | Op::Rename { .. }
        | Op::Fix(_) => Err(EditError::Cannot(
            "that edit is made to a tile, not to a print list".to_owned(),
        )),
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
