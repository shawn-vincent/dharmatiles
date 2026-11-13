//! Editing a map from the editor: placing, moving, turning, raising,
//! duplicating and removing pieces, new variants, and pins on a plate.
//!
//! Like the print list's calls (`lists`), every call is an edit of the map's
//! text made by `dt_doc::map` under the file's lock ([`super::docs`]), written
//! to the file (the worker sees it and plans again), and answers with
//! `before` and `text` (the text it replaced and the text it wrote, for Undo
//! and Redo), `docRev` (the revision the file has now) and the map's scene as
//! it now reads (`scene`, cheap: nothing is built), so the page shows the
//! change at once.  A piece is named by its `id`, and every call says the
//! `docRev` it was made against: a file that has moved on is refused with a
//! 409 and the current text, scene and `docRev`.
//!
//! - `/api/maps/add` `{list, tile, at, rot?, seed?, system?, docRev}`: a new placed
//!   piece, with its own id.
//! - `/api/maps/set` `{list, id, set, docRev}` and `/api/maps/batch` `{list, edits:
//!   [{id, set}], docRev}`: words of one piece, or of several in one change (`at`,
//!   `rot`, `bottom`, `seed`, `print`, `system`; null takes a word off).
//! - `/api/maps/dup` `{list, id, at, docRev}`, `/api/maps/remove` `{list, id, docRev}`.
//! - `/api/maps/pin` `{list, id, plate, x, y, turn, docRev}`, `/api/maps/unpin`
//!   `{list, id, docRev}`, `/api/maps/unpin-all` `{list, docRev}`.

use dt_doc::map::{At, Prop, decode};
use serde::{Deserialize, Deserializer, Serialize};

use super::api::Out;
use super::lists::{ListEdited, bad, edit_list};
use super::scene::SceneJson;
use super::{Shared, fail, lock, scene};

// ---- the calls' types ------------------------------------------------------------------

/// A word of a piece to change: left out is left as it is, `null` takes it off.
fn word<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// The words a call may set on a piece.
#[derive(Deserialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PieceWords {
    /// Where, as written: `(2sq, 1sq)`.
    #[serde(default, deserialize_with = "word")]
    #[cfg_attr(test, ts(type = "string | null", optional))]
    pub at: Option<Option<String>>,
    /// Quarter turns clockwise, in degrees: 0, 90, 180, 270.
    #[serde(default, deserialize_with = "word")]
    #[cfg_attr(test, ts(type = "number | null", optional))]
    pub rot: Option<Option<i64>>,
    /// A height: a `$` name or a length.
    #[serde(default, deserialize_with = "word")]
    #[cfg_attr(test, ts(type = "string | null", optional))]
    pub bottom: Option<Option<String>>,
    #[serde(default, deserialize_with = "word")]
    #[cfg_attr(test, ts(type = "number | null", optional))]
    pub seed: Option<Option<i64>>,
    /// `missing`, or the default (no word).
    #[serde(default, deserialize_with = "word")]
    #[cfg_attr(test, ts(type = "string | null", optional))]
    pub print: Option<Option<String>>,
    #[serde(default, deserialize_with = "word")]
    #[cfg_attr(test, ts(type = "string | null", optional))]
    pub system: Option<Option<String>>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MapsAddReq {
    pub list: String,
    pub tile: String,
    pub at: String,
    pub rot: Option<i64>,
    pub seed: Option<u64>,
    pub system: Option<String>,
    pub doc_rev: u64,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MapsSetReq {
    pub list: String,
    pub id: String,
    pub set: PieceWords,
    pub doc_rev: u64,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PieceEdit {
    pub id: String,
    pub set: PieceWords,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MapsBatchReq {
    pub list: String,
    pub edits: Vec<PieceEdit>,
    pub doc_rev: u64,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MapsDupReq {
    pub list: String,
    pub id: String,
    pub at: String,
    pub doc_rev: u64,
}

/// A call about one piece.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MapsPieceReq {
    pub list: String,
    pub id: String,
    pub doc_rev: u64,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MapsPinReq {
    pub list: String,
    pub id: String,
    /// The plate, counting from 1.
    pub plate: u32,
    pub x: f64,
    pub y: f64,
    pub turn: Option<bool>,
    pub doc_rev: u64,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MapsListReq {
    pub list: String,
    pub doc_rev: u64,
}

/// An edit that made or named a piece: its id.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MapsPieced {
    #[serde(flatten)]
    pub edited: ListEdited,
    pub id: String,
}

/// An edit of several pieces: how many.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MapsBatched {
    #[serde(flatten)]
    pub edited: ListEdited,
    pub changed: usize,
}

/// The scene of `text` (the file at revision `rev`) as the page reads it (no builds).
pub(super) fn scene_of(shared: &Shared, path: &str, text: &str, rev: u64) -> Option<SceneJson> {
    decode(path, text)
        .ok()
        .map(|list| scene::read(shared, path, &list, rev).json)
}

/// [`scene_of`] as JSON, for the body of a 409.
pub(super) fn scene_value(shared: &Shared, path: &str, text: &str, rev: u64) -> serde_json::Value {
    serde_json::to_value(scene_of(shared, path, text, rev)).unwrap_or(serde_json::Value::Null)
}

fn first_message(e: &[dt_doc::DocError]) -> String {
    e.first()
        .map_or_else(|| "the map does not read".to_owned(), |d| d.message.clone())
}

/// A place from the call: `"(2sq, 1sq)"` as written.
fn at_of(s: &str) -> Result<At, String> {
    dt_doc::map::parse_at(s)
}

/// The words a call may set on a piece.
fn props_of(set: &PieceWords) -> Result<Vec<(&'static str, Option<Prop>)>, String> {
    let mut out = Vec::new();
    let text = |k: &'static str, v: &Option<Option<String>>, out: &mut Vec<_>| {
        if let Some(v) = v {
            out.push((k, v.clone().map(Prop::Str)));
        }
    };
    text("at", &set.at, &mut out);
    if let Some(r) = &set.rot {
        out.push(("rot", r.map(Prop::Int)));
    }
    text("bottom", &set.bottom, &mut out);
    if let Some(s) = &set.seed {
        out.push(("seed", s.map(Prop::Int)));
    }
    if let Some(p) = &set.print {
        // the default is no word at all
        out.push(("print", p.clone().filter(|s| s != "missing").map(Prop::Str)));
    }
    text("system", &set.system, &mut out);
    Ok(out)
}

/// `/api/maps/add`.
pub(super) fn add(shared: &Shared, v: MapsAddReq) -> Out<MapsPieced> {
    let Some(path) = lock(&shared.tiles)
        .iter()
        .find(|t| t.id == v.tile)
        .map(|t| t.path.clone())
        .filter(|p| p.to_ascii_lowercase().ends_with(".dtile"))
    else {
        return Err(fail::refused("404 Not Found", "no such tile"));
    };
    let at = at_of(&v.at).map_err(|m| bad(&m))?;
    let rot = v.rot.unwrap_or(0);
    if ![0, 90, 180, 270].contains(&rot) {
        return Err(bad("`rot` is 0, 90, 180 or 270"));
    }
    let system = match v.system.as_deref() {
        None => None,
        Some(s @ ("db" | "ol")) => Some(s),
        Some(_) => return Err(bad("the system is db or ol")),
    };
    let canon = dt_eval::inventory::canonical(&path, shared.home.as_deref());
    let (edited, id) = edit_list(shared, &v.list, v.doc_rev, |text, l| {
        let id = dt_doc::map::next_id(l);
        let line = dt_doc::map::placed_line(&id, &canon, at, rot, v.seed, system);
        let new = dt_doc::map::add_pieces(text, &[line]).map_err(|e| first_message(&e))?;
        Ok((new, id))
    })?;
    Ok(MapsPieced { edited, id })
}

/// `/api/maps/set`.
pub(super) fn set(shared: &Shared, v: MapsSetReq) -> Out<ListEdited> {
    let props = props_of(&v.set).map_err(|m| bad(&m))?;
    let (e, ()) = edit_list(shared, &v.list, v.doc_rev, |text, _| {
        dt_doc::map::set_piece(text, &v.id, &props).map(|t| (t, ()))
    })?;
    Ok(e)
}

/// `/api/maps/batch`.
pub(super) fn batch(shared: &Shared, v: MapsBatchReq) -> Out<MapsBatched> {
    let mut all = Vec::new();
    for e in &v.edits {
        all.push((e.id.as_str(), props_of(&e.set).map_err(|m| bad(&m))?));
    }
    let (edited, changed) = edit_list(shared, &v.list, v.doc_rev, |text, _| {
        let mut t = text.to_owned();
        for (i, p) in &all {
            t = dt_doc::map::set_piece(&t, i, p)?;
        }
        Ok((t, all.len()))
    })?;
    Ok(MapsBatched { edited, changed })
}

/// `/api/maps/dup`.
pub(super) fn dup(shared: &Shared, v: MapsDupReq) -> Out<MapsPieced> {
    let at = at_of(&v.at).map_err(|m| bad(&m))?;
    let (edited, id) = edit_list(shared, &v.list, v.doc_rev, |text, l| {
        let id = dt_doc::map::next_id(l);
        let t = dt_doc::map::duplicate_piece(text, &v.id, &id, at)?;
        // a copy starts with the original's seed: the same piece until it is made a variant
        Ok((t, id))
    })?;
    Ok(MapsPieced { edited, id })
}

/// `/api/maps/remove`: the piece, and the pin it had.
pub(super) fn remove(shared: &Shared, v: MapsPieceReq) -> Out<ListEdited> {
    let (e, ()) = edit_list(shared, &v.list, v.doc_rev, |text, _| {
        let t = dt_doc::map::clear_pin(text, &v.id).map_err(|e| first_message(&e))?;
        dt_doc::map::remove_piece(&t, &v.id).map(|t| (t, ()))
    })?;
    Ok(e)
}

/// `/api/maps/pin`: the piece stays where it was dropped on its plate.
pub(super) fn pin(shared: &Shared, v: MapsPinReq) -> Out<MapsPieced> {
    if v.plate == 0 || !v.x.is_finite() || !v.y.is_finite() {
        return Err(bad("the plate counts from 1"));
    }
    let turn = v.turn.unwrap_or(false);
    let (edited, id) = edit_list(shared, &v.list, v.doc_rev, |text, l| {
        if !dt_doc::map::piece_ids(l).contains(&v.id) {
            return Err(format!("the map has no piece {}", v.id));
        }
        let at = At {
            x: dt_doc::map::Len::Mm((v.x * 10.0).round() / 10.0),
            y: dt_doc::map::Len::Mm((v.y * 10.0).round() / 10.0),
        };
        let t =
            dt_doc::map::set_pin(text, &v.id, v.plate, at, turn).map_err(|e| first_message(&e))?;
        Ok((t, v.id.clone()))
    })?;
    Ok(MapsPieced { edited, id })
}

/// `/api/maps/unpin`.
pub(super) fn unpin(shared: &Shared, v: MapsPieceReq) -> Out<ListEdited> {
    let (e, ()) = edit_list(shared, &v.list, v.doc_rev, |text, _| {
        dt_doc::map::clear_pin(text, &v.id)
            .map(|t| (t, ()))
            .map_err(|e| first_message(&e))
    })?;
    Ok(e)
}

/// `/api/maps/unpin-all`.
pub(super) fn unpin_all(shared: &Shared, v: MapsListReq) -> Out<ListEdited> {
    let (e, ()) = edit_list(shared, &v.list, v.doc_rev, |text, _| {
        dt_doc::map::clear_pins(text)
            .map(|t| (t, ()))
            .map_err(|e| first_message(&e))
    })?;
    Ok(e)
}
