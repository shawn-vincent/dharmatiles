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

use dt_doc::map::{At, decode};
use dt_doc::ops::{Kind, Op, Prop, apply};
use serde::{Deserialize, Deserializer, Serialize};

use super::api::Out;
use super::lists::{ListEdited, bad, edit_list};
use super::placing::{Geom, Near, Resolver, SeedWord, at_word};
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
    /// Instead of `at`: the nearest free square to this point, settled by the program in the write.
    #[serde(default)]
    pub near: Option<Near>,
    /// Instead of `seed`: a seed nobody owns, that the map does not use and no other piece of this edit has.
    #[serde(default, rename = "freshSeed")]
    pub fresh_seed: Option<bool>,
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
    /// The place, as written; or `near`.
    pub at: Option<String>,
    /// The nearest free square to this point (squares of the piece's system), settled in the write.
    pub near: Option<Near>,
    pub rot: Option<i64>,
    pub seed: Option<u64>,
    /// A seed to give it, settled in the write, when `seed` is not given.
    pub seed_word: Option<SeedWord>,
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
    pub at: Option<String>,
    pub near: Option<Near>,
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
    /// The piece went to another square than the one wished (the nearest free one).
    pub shifted: usize,
    /// The seed it was given (settled in the write), when it was asked for one.
    pub seed: Option<u64>,
}

/// An edit of several pieces: how many.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MapsBatched {
    #[serde(flatten)]
    pub edited: ListEdited,
    pub changed: usize,
    /// How many pieces went to another square than the one wished.
    pub shifted: usize,
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

/// What the program settled of one piece's intents: the words to write and what it says of them.
fn words_for(
    set: &PieceWords,
    piece: &super::scene::ScenePiece,
    rs: &mut Resolver<'_>,
) -> Result<Vec<(&'static str, Option<Prop>)>, String> {
    let mut words = props_of(set)?;
    if set.near.is_some() && set.at.is_some() {
        return Err("a piece has `at` or `near`, not both".to_owned());
    }
    if set.fresh_seed == Some(true) && set.seed.is_some() {
        return Err("a piece has `seed` or `freshSeed`, not both".to_owned());
    }
    if let Some(near) = set.near {
        let rot = match &set.rot {
            Some(r) => r.unwrap_or(0),
            None => piece.rot.unwrap_or(0),
        };
        let (x, y) = rs.place(&Geom::of(piece), rot, near)?;
        words.push(("at", Some(Prop::Str(at_word(x, y)))));
    }
    if set.fresh_seed == Some(true) {
        let s = rs.fresh(
            &piece.canon,
            &piece.system,
            &piece.colour,
            Some(piece.seed_used),
        )?;
        words.push(("seed", i64::try_from(s).ok().map(Prop::Int)));
    }
    Ok(words)
}

/// The map as the call's text reads, for settling intents.
fn scene_at(shared: &Shared, path: &str, list: &dt_doc::map::PrintList, rev: u64) -> SceneJson {
    scene::read(shared, path, list, rev).json
}

/// `/api/maps/add`.
pub(super) fn add(shared: &Shared, v: MapsAddReq) -> Out<MapsPieced> {
    let Some(tile_path) = lock(&shared.tiles)
        .iter()
        .find(|t| t.id == v.tile)
        .map(|t| t.path.clone())
        .filter(|p| p.to_ascii_lowercase().ends_with(".dtile"))
    else {
        return Err(fail::refused("404 Not Found", "no such tile"));
    };
    if v.at.is_some() == v.near.is_some() {
        return Err(bad("give the place as `at` or as `near`, one of them"));
    }
    if v.seed.is_some() && v.seed_word.is_some() {
        return Err(bad("give `seed` or `seedWord`, not both"));
    }
    let rot = v.rot.unwrap_or(0);
    if ![0, 90, 180, 270].contains(&rot) {
        return Err(bad("`rot` is 0, 90, 180 or 270"));
    }
    let system = match v.system.as_deref() {
        None => None,
        Some(s @ ("db" | "ol")) => Some(s),
        Some(_) => return Err(bad("the system is db or ol")),
    };
    let canon = dt_print::inventory::canonical(&tile_path, shared.home.as_deref());
    let path = super::lists::list_path(shared, &v.list)?;
    let (mut shifted, mut seed_given) = (0, None);
    let (edited, id) = edit_list(shared, &v.list, v.doc_rev, |text, l| {
        let id = dt_doc::map::next_id(l);
        let needs_scene = v.near.is_some() || v.seed_word.is_some();
        let mut rs =
            needs_scene.then(|| Resolver::new(shared, scene_at(shared, &path, l, v.doc_rev), &[]));
        let sys = system.map_or_else(
            || {
                rs.as_ref()
                    .map_or_else(String::new, |r| r.scene().system.clone())
            },
            str::to_owned,
        );
        // the tile as a piece of this map would be: its size in squares and its own seed
        let own = |sys: &str| -> Result<(u32, u32, u64, f64), String> {
            let opts = dt_eval::Options {
                system: sys.to_owned(),
                seed: None,
                ..shared.options()
            };
            let c = dt_eval::check(&dt_eval::Files, &tile_path, &opts).map_err(|e| {
                e.first().map_or_else(
                    || "the tile does not check".to_owned(),
                    |d| d.message.clone(),
                )
            })?;
            let t = c.tile.as_ref().ok_or("that is not a tile")?;
            Ok((
                t.cols,
                t.rows,
                c.seed,
                dt_kit::Frame::for_system(sys, 1, 1).square_mm,
            ))
        };
        let at = match (&v.at, v.near) {
            (Some(a), _) => dt_doc::map::parse_at(a)?,
            (None, Some(near)) => {
                let r = rs.as_mut().ok_or("no map")?;
                let (cols, rows, _, sq) = own(&sys)?;
                let g = Geom {
                    w: f64::from(cols) * sq,
                    d: f64::from(rows) * sq,
                    sq,
                    bottom: 0.0,
                };
                let (x, y) = r.place(&g, rot, near)?;
                shifted = r.shifted;
                dt_doc::map::parse_at(&at_word(x, y))?
            }
            (None, None) => return Err("no place".to_owned()),
        };
        let seed = match (v.seed, v.seed_word, rs.as_mut()) {
            (Some(s), _, _) => Some(s),
            (None, Some(SeedWord::Fresh), Some(r)) => {
                let (_, _, tile_seed, _) = own(&sys)?;
                Some(r.fresh(&canon, &sys, &r.scene().colour.clone(), Some(tile_seed))?)
            }
            (None, Some(SeedWord::IfNeeded), Some(r)) => {
                let colour = r.scene().colour.clone();
                match r.taken_seed(&canon, &sys, &colour) {
                    Some(on_map) => Some(r.fresh(&canon, &sys, &colour, on_map)?),
                    None => None,
                }
            }
            _ => None,
        };
        seed_given = seed;
        let line = dt_doc::map::placed_line(&id, &canon, at, rot, seed, system);
        let new = apply(Kind::Map, text, &Op::Append(&[line])).map_err(|e| e.to_string())?;
        Ok((new.text, id))
    })?;
    Ok(MapsPieced {
        edited,
        id,
        shifted,
        seed: seed_given,
    })
}

/// `/api/maps/set`.
pub(super) fn set(shared: &Shared, v: MapsSetReq) -> Out<ListEdited> {
    let (e, ()) = batch_edit(shared, &v.list, v.doc_rev, &[(v.id.as_str(), &v.set)])
        .map(|(e, _, ())| (e, ()))?;
    Ok(e)
}

/// The words of several pieces, intents settled under the file's lock: `(the answer, how many went elsewhere, ())`.
fn batch_edit(
    shared: &Shared,
    list: &str,
    doc_rev: u64,
    edits: &[(&str, &PieceWords)],
) -> Out<(ListEdited, usize, ())> {
    // the plain words are checked before anything is read
    for (_, s) in edits {
        props_of(s).map_err(|m| bad(&m))?;
    }
    let path = super::lists::list_path(shared, list)?;
    let intents = edits
        .iter()
        .any(|(_, s)| s.near.is_some() || s.fresh_seed == Some(true));
    // the pieces this edit moves do not stand in the way of their own new places
    let leave: Vec<&str> = edits
        .iter()
        .filter(|(_, s)| s.near.is_some())
        .map(|(i, _)| *i)
        .collect();
    let mut shifted = 0;
    let (edited, ()) = edit_list(shared, list, doc_rev, |text, l| {
        let mut rs =
            intents.then(|| Resolver::new(shared, scene_at(shared, &path, l, doc_rev), &leave));
        let mut t = text.to_owned();
        for (id, set) in edits {
            let words = match rs.as_mut() {
                Some(r) => {
                    let piece = r.piece(id)?.clone();
                    words_for(set, &piece, r)?
                }
                None => props_of(set)?,
            };
            t = apply(Kind::Map, &t, &Op::SetPiece { id, edits: &words })
                .map_err(|e| e.to_string())?
                .text;
        }
        shifted = rs.as_ref().map_or(0, |r| r.shifted);
        Ok((t, ()))
    })?;
    Ok((edited, shifted, ()))
}

/// `/api/maps/batch`.
pub(super) fn batch(shared: &Shared, v: MapsBatchReq) -> Out<MapsBatched> {
    let edits: Vec<(&str, &PieceWords)> = v.edits.iter().map(|e| (e.id.as_str(), &e.set)).collect();
    let (edited, shifted, ()) = batch_edit(shared, &v.list, v.doc_rev, &edits)?;
    Ok(MapsBatched {
        edited,
        changed: edits.len(),
        shifted,
    })
}

/// `/api/maps/dup`.
pub(super) fn dup(shared: &Shared, v: MapsDupReq) -> Out<MapsPieced> {
    if v.at.is_some() == v.near.is_some() {
        return Err(bad("give the place as `at` or as `near`, one of them"));
    }
    let path = super::lists::list_path(shared, &v.list)?;
    let mut shifted = 0;
    let (edited, id) = edit_list(shared, &v.list, v.doc_rev, |text, l| {
        let id = dt_doc::map::next_id(l);
        let at = match (&v.at, v.near) {
            (Some(a), _) => dt_doc::map::parse_at(a)?,
            (None, Some(near)) => {
                let mut rs = Resolver::new(shared, scene_at(shared, &path, l, v.doc_rev), &[]);
                let p = rs.piece(&v.id)?.clone();
                let (x, y) = rs.place(&Geom::of(&p), p.rot.unwrap_or(0), near)?;
                shifted = rs.shifted;
                dt_doc::map::parse_at(&at_word(x, y))?
            }
            (None, None) => return Err("no place".to_owned()),
        };
        let t = apply(
            Kind::Map,
            text,
            &Op::Duplicate {
                id: &v.id,
                new_id: &id,
                at,
            },
        )
        .map_err(|e| e.to_string())?
        .text;
        // a copy starts with the original's seed: the same piece until it is made a variant
        Ok((t, id))
    })?;
    Ok(MapsPieced {
        edited,
        id,
        shifted,
        seed: None,
    })
}

/// `/api/maps/remove`: the piece, and the pin it had.
pub(super) fn remove(shared: &Shared, v: MapsPieceReq) -> Out<ListEdited> {
    let (e, ()) = edit_list(shared, &v.list, v.doc_rev, |text, _| {
        apply(Kind::Map, text, &Op::Remove { id: &v.id })
            .map(|d| (d.text, ()))
            .map_err(|e| e.to_string())
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
        let t = apply(
            Kind::Map,
            text,
            &Op::Pin {
                id: &v.id,
                plate: v.plate,
                at,
                turn,
            },
        )
        .map_err(|e| e.to_string())?
        .text;
        Ok((t, v.id.clone()))
    })?;
    Ok(MapsPieced {
        edited,
        id,
        shifted: 0,
        seed: None,
    })
}

/// `/api/maps/unpin`.
pub(super) fn unpin(shared: &Shared, v: MapsPieceReq) -> Out<ListEdited> {
    let (e, ()) = edit_list(shared, &v.list, v.doc_rev, |text, _| {
        apply(Kind::Map, text, &Op::Unpin { id: &v.id })
            .map(|d| (d.text, ()))
            .map_err(|e| e.to_string())
    })?;
    Ok(e)
}

/// `/api/maps/unpin-all`.
pub(super) fn unpin_all(shared: &Shared, v: MapsListReq) -> Out<ListEdited> {
    let (e, ()) = edit_list(shared, &v.list, v.doc_rev, |text, _| {
        apply(Kind::Map, text, &Op::UnpinAll)
            .map(|d| (d.text, ()))
            .map_err(|e| e.to_string())
    })?;
    Ok(e)
}
