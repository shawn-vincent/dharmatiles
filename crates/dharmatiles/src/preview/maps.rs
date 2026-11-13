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
use serde_json::{Value, json};

use super::lists::{bad, edit_list};
use super::{Shared, fail, lock, scene};

/// The scene of `text` (the file at revision `rev`) as the page reads it (no builds).
pub(super) fn scene_of(shared: &Shared, path: &str, text: &str, rev: u64) -> Value {
    match decode(path, text) {
        Ok(list) => {
            let mut s = scene::read(shared, path, &list, rev);
            s.json["notes"] = json!([]);
            s.json
        }
        Err(_) => Value::Null,
    }
}

fn id_of(v: &Value) -> Option<&str> {
    v["id"].as_str()
}

fn first_message(e: &[dt_doc::DocError]) -> String {
    e.first()
        .map_or_else(|| "the map does not read".to_owned(), |d| d.message.clone())
}

/// A place from the call: `"(2sq, 1sq)"` as written.
fn at_of(v: &Value) -> Result<At, String> {
    let s = v
        .as_str()
        .ok_or("give the place as text, like (2sq, 1sq)")?;
    dt_doc::map::parse_at(s)
}

/// The words a call may set on a piece, from `{at, rot, bottom, seed, print, system}`.
fn props_of(set: &Value) -> Result<Vec<(&'static str, Option<Prop>)>, String> {
    let mut out = Vec::new();
    for k in ["at", "rot", "bottom", "seed", "print", "system"] {
        let Some(v) = set.get(k) else { continue };
        let prop = match (k, v) {
            (_, Value::Null) => None,
            ("rot" | "seed", n) => Some(Prop::Int(
                n.as_i64()
                    .ok_or_else(|| format!("`{k}` is a whole number"))?,
            )),
            // the default is no word at all
            ("print", s) if s.as_str() == Some("missing") => None,
            (_, s) => Some(Prop::Str(
                s.as_str()
                    .ok_or_else(|| format!("`{k}` is text"))?
                    .to_owned(),
            )),
        };
        out.push((k, prop));
    }
    Ok(out)
}

/// `/api/maps/add`.
pub(super) fn add(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(list), Some(tile)) = (v["list"].as_str(), v["tile"].as_str()) else {
        return bad("give the map and the tile");
    };
    let Some(path) = lock(&shared.tiles)
        .iter()
        .find(|t| t.id == tile)
        .map(|t| t.path.clone())
        .filter(|p| p.to_ascii_lowercase().ends_with(".dtile"))
    else {
        return fail::refused("404 Not Found", "no such tile");
    };
    let at = match at_of(&v["at"]) {
        Ok(a) => a,
        Err(m) => return bad(&m),
    };
    let rot = v["rot"].as_i64().unwrap_or(0);
    if ![0, 90, 180, 270].contains(&rot) {
        return bad("`rot` is 0, 90, 180 or 270");
    }
    let system = match v["system"].as_str() {
        None => None,
        Some(s @ ("db" | "ol")) => Some(s),
        Some(_) => return bad("the system is db or ol"),
    };
    let canon = dt_eval::inventory::canonical(&path, shared.home.as_deref());
    let seed = v["seed"].as_u64();
    edit_list(shared, list, v, |text, l| {
        let id = dt_doc::map::next_id(l);
        let line = dt_doc::map::placed_line(&id, &canon, at, rot, seed, system);
        let new = dt_doc::map::add_pieces(text, &[line]).map_err(|e| first_message(&e))?;
        Ok((new, json!({"id": id})))
    })
}

/// `/api/maps/set`.
pub(super) fn set(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(list), Some(piece)) = (v["list"].as_str(), id_of(v)) else {
        return bad("give the map and the piece (its id)");
    };
    let props = match props_of(&v["set"]) {
        Ok(p) => p,
        Err(m) => return bad(&m),
    };
    edit_list(shared, list, v, |text, _| {
        dt_doc::map::set_piece(text, piece, &props).map(|t| (t, json!({})))
    })
}

/// `/api/maps/batch`.
pub(super) fn batch(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(list), Some(edits)) = (v["list"].as_str(), v["edits"].as_array()) else {
        return bad("give the map and the edits");
    };
    let mut all = Vec::new();
    for e in edits {
        let (Some(i), props) = (id_of(e), props_of(&e["set"])) else {
            return bad("each edit names its piece (its id)");
        };
        match props {
            Ok(p) => all.push((i, p)),
            Err(m) => return bad(&m),
        }
    }
    edit_list(shared, list, v, |text, _| {
        let mut t = text.to_owned();
        for (i, p) in &all {
            t = dt_doc::map::set_piece(&t, i, p)?;
        }
        Ok((t, json!({"changed": all.len()})))
    })
}

/// `/api/maps/dup`.
pub(super) fn dup(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(list), Some(piece)) = (v["list"].as_str(), id_of(v)) else {
        return bad("give the map and the piece (its id)");
    };
    let at = match at_of(&v["at"]) {
        Ok(a) => a,
        Err(m) => return bad(&m),
    };
    edit_list(shared, list, v, |text, l| {
        let id = dt_doc::map::next_id(l);
        let t = dt_doc::map::duplicate_piece(text, piece, &id, at)?;
        // a copy starts with the original's seed: the same piece until it is made a variant
        Ok((t, json!({"id": id})))
    })
}

/// `/api/maps/remove`: the piece, and the pin it had.
pub(super) fn remove(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(list), Some(piece)) = (v["list"].as_str(), id_of(v)) else {
        return bad("give the map and the piece (its id)");
    };
    edit_list(shared, list, v, |text, _| {
        let t = dt_doc::map::clear_pin(text, piece).map_err(|e| first_message(&e))?;
        dt_doc::map::remove_piece(&t, piece).map(|t| (t, json!({})))
    })
}

/// `/api/maps/pin`: the piece stays where it was dropped on its plate.
pub(super) fn pin(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(list), Some(piece)) = (v["list"].as_str(), id_of(v)) else {
        return bad("give the map and the piece (its id)");
    };
    let (Some(plate), Some(x), Some(y)) = (
        v["plate"].as_u64().and_then(|p| u32::try_from(p).ok()),
        v["x"].as_f64(),
        v["y"].as_f64(),
    ) else {
        return bad("give the plate and where the piece stands");
    };
    if plate == 0 || !x.is_finite() || !y.is_finite() {
        return bad("the plate counts from 1");
    }
    let turn = v["turn"].as_bool().unwrap_or(false);
    edit_list(shared, list, v, |text, l| {
        if !dt_doc::map::piece_ids(l).iter().any(|i| i == piece) {
            return Err(format!("the map has no piece {piece}"));
        }
        let at = At {
            x: dt_doc::map::Len::Mm((x * 10.0).round() / 10.0),
            y: dt_doc::map::Len::Mm((y * 10.0).round() / 10.0),
        };
        let t =
            dt_doc::map::set_pin(text, piece, plate, at, turn).map_err(|e| first_message(&e))?;
        Ok((t, json!({"id": piece})))
    })
}

/// `/api/maps/unpin`.
pub(super) fn unpin(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(list), Some(piece)) = (v["list"].as_str(), id_of(v)) else {
        return bad("give the map and the piece (its id)");
    };
    edit_list(shared, list, v, |text, _| {
        dt_doc::map::clear_pin(text, piece)
            .map(|t| (t, json!({})))
            .map_err(|e| first_message(&e))
    })
}

/// `/api/maps/unpin-all`.
pub(super) fn unpin_all(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let Some(list) = v["list"].as_str() else {
        return bad("give the map");
    };
    edit_list(shared, list, v, |text, _| {
        dt_doc::map::clear_pins(text)
            .map(|t| (t, json!({})))
            .map_err(|e| first_message(&e))
    })
}
