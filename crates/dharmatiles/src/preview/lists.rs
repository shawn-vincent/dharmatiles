//! Starting and filling a print list from the editor.
//!
//! Every call is an edit of the list's text, made by `dt_doc::map` and written
//! to the file (the worker sees it and plans the plates again), and every call
//! that changes a list answers with `before`, the text it replaced, `text`,
//! the text it wrote, and `docRev`, the revision the file has now (see
//! [`super::docs`]), so the page can offer Undo and Redo (`/api/lists/restore`
//! writes a text back).  An edit names its piece by id and the `docRev` it was
//! made against, and a file that has moved on is refused with a 409 and the
//! current text.  A new list is
//! made through the places' rules like a new tile: in My tiles or a folder of
//! a place the page names, never over a file.  A mistake in a list's text is
//! shown as data (`{ok: false, errors}`), and nothing is written.
//!
//! - `/api/lists/new` `{name, root?, dir?}`: `<name>.dmap`, an empty map laid
//!   out for the printer last chosen.
//! - `/api/lists/add` `{tile, list?, seed?, system?}`: one more copy of a tile
//!   in a list (a new list when none is named); a piece already there at the
//!   same seed and system has its `count` raised.
//! - `/api/lists/count` `{list, id, count, docRev}`, `/api/lists/remove`
//!   `{list, id, docRev}`: a piece's copies, or the piece itself.
//! - `/api/lists/restore` `{list, text, docRev}`: Undo or Redo, a text
//!   written back, if the file is still at `docRev`; else 409.
//!
//! The first edit the editor makes to a file whose pieces have no ids writes
//! an id (`p1`, `p2`, …) on every one, in the same write, and says so once in
//! the answer's `note`.

use std::path::PathBuf;

use dt_doc::map::{DEFAULT_PRINTER, Mode, Want};
use dt_eval::Diag;
use serde_json::{Value, json};

use super::docs::{self, Edited};
use super::messages::Scope;
use super::{Shared, edit, fail, lock, places};
use crate::errors_json;

pub(super) fn bad(msg: &str) -> (&'static str, Value) {
    fail::refused("400 Bad Request", msg)
}

pub(super) fn diags(e: &[dt_doc::DocError]) -> (&'static str, Value) {
    let d: Vec<Diag> = e.iter().map(Diag::from).collect();
    ("200 OK", json!({"ok": false, "errors": errors_json(&d)}))
}

/// `/api/tiles/ID/check` for a print list or the inventory: its decoder's
/// mistakes at their text, as the tile's check gives its own.
pub(super) fn check_text(shared: &Shared, tile: &str, text: &str) -> (&'static str, Value) {
    let result = if super::inventory::is_inventory(tile) {
        dt_doc::inventory::decode(tile, text).map(|_| ())
    } else {
        dt_doc::map::decode(tile, text).map(|_| ())
    };
    let id = super::tile_id(tile);
    match result {
        Ok(()) => {
            // what the last reading of a map noted (its crowns) is still true of this text
            let notes = lock(&shared.tiles)
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.scene_notes.clone())
                .unwrap_or_default();
            shared.report(&id, Scope::Check, |name| {
                notes
                    .iter()
                    .map(|n| {
                        crate::message::Message::new(
                            crate::message::Kind::Note,
                            crate::message::Severity::Note,
                            Scope::Check,
                            &id,
                            name,
                            n.text.clone(),
                        )
                        .code(n.code)
                        .help(n.help.as_deref())
                    })
                    .collect()
            });
            ("200 OK", json!({"ok": true, "errors": []}))
        }
        Err(e) => {
            let d: Vec<Diag> = e.iter().map(Diag::from).collect();
            shared.report(&id, Scope::Check, |name| {
                super::messages::of_errors(&d, Scope::Check, &id, name, &[])
            });
            ("200 OK", json!({"ok": false, "errors": errors_json(&d)}))
        }
    }
}

/// The printer last chosen (the title bar's), where it holds a bed.
fn printer_for(shared: &Shared) -> String {
    let name = shared.options().printer.name;
    match dt_eval::printer_machine(&name, shared.home.as_deref()) {
        Ok(m) if m.bed.is_some() => name,
        _ => DEFAULT_PRINTER.to_owned(),
    }
}

pub(super) fn list_path(shared: &Shared, id: &str) -> Result<String, (&'static str, Value)> {
    lock(&shared.tiles)
        .iter()
        .find(|t| t.id == id && super::plates::is_plates(&t.path))
        .map(|t| t.path.clone())
        .ok_or_else(|| fail::refused("404 Not Found", "no such print list"))
}

/// A list's file changed: planned again.
pub(super) fn changed(shared: &Shared, path: &str) {
    super::plates::again(shared, &super::tile_id(path));
}

/// What a call that changed a list answers: the text it replaced (`before`),
/// the text it wrote (`text`), the revision the file has now (`docRev`) and
/// the map's scene as it now reads (cheap: nothing is built), so the page
/// shows the change at once.
pub(super) fn done(shared: &Shared, path: &str, e: &Edited) -> (&'static str, Value) {
    let mut v = json!({
        "ok": true, "list": super::tile_id(path), "path": path, "before": e.before,
        "text": e.after, "docRev": e.rev,
        "scene": super::maps::scene_of(shared, path, &e.after, e.rev),
    });
    if let (Some(o), Some(x)) = (v.as_object_mut(), e.extra.as_object()) {
        o.extend(x.clone());
    }
    ("200 OK", v)
}

const GAVE_IDS: &str =
    "Gave every piece an id (p1, p2, …) so edits can name it; the file keeps them.";

/// Edit the list `id` made against the revision the call names: `f` is given
/// the text (with an id on every piece) and what it reads as, and answers the
/// new text and what to add to the answer.  The edit is made under the
/// file's lock, checked to read, written whole, and the list is planned again.
pub(super) fn edit_list(
    shared: &Shared,
    id: &str,
    v: &Value,
    f: impl FnOnce(&str, &dt_doc::map::PrintList) -> Result<(String, Value), String>,
) -> (&'static str, Value) {
    let rev = match docs::expected(v) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let path = match list_path(shared, id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let r = shared.docs.edit(&path, Some(rev), |text| {
        dt_doc::map::decode(&path, text).map_err(|e| diags(&e))?;
        let (named, gave) = dt_doc::map::give_ids(text).map_err(|m| bad(&m))?;
        let list = dt_doc::map::decode(&path, &named).map_err(|e| diags(&e))?;
        let (new, mut extra) = f(&named, &list).map_err(|m| bad(&m))?;
        dt_doc::map::decode(&path, &new).map_err(|e| diags(&e))?;
        if gave {
            extra["note"] = json!(GAVE_IDS);
        }
        Ok((new, extra))
    });
    match r {
        Ok(e) => {
            changed(shared, &path);
            done(shared, &path, &e)
        }
        Err(e) => with_current(shared, &path, e),
    }
}

/// A refusal because the file moved on carries the file's scene as it is now,
/// so the page takes the current state at once, not when the worker's next word comes.
pub(super) fn with_current(
    shared: &Shared,
    path: &str,
    mut reply: (&'static str, Value),
) -> (&'static str, Value) {
    if reply.0.starts_with("409")
        && let (Some(text), Some(rev)) = (reply.1["text"].as_str(), reply.1["docRev"].as_u64())
    {
        let scene = super::maps::scene_of(shared, path, text, rev);
        reply.1["scene"] = scene;
    }
    reply
}

/// `/api/lists/new`.
pub(super) fn new_list(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let mut name = v.clone();
    if name["name"].as_str().is_none_or(|n| n.trim().is_empty()) {
        name["name"] = json!("Print list");
    }
    let stem = match edit::tile_name(&name) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let into = match places::new_tile_dir(shared, v) {
        Some(r) => r,
        None => edit::home(shared),
    };
    let mut text = dt_doc::map::new_list(
        &dt_doc::header::stamp(env!("DT_ENGINE_VERSION")),
        "Tiles to print.",
        &[],
        Some(&printer_for(shared)),
        &shared.options().system,
        None,
    );
    // a map has a grid (it is what opens a document as a map, before a piece is placed)
    if v["map"].as_bool() == Some(true) {
        text = text.replacen(
            "    about \"Tiles to print.\"",
            "    grid cols=8 rows=6\n    about \"A map.\"",
            1,
        );
    }
    match into.and_then(|d| edit::write_new_ext(&d, &stem, "dmap", text.as_bytes())) {
        Ok(dest) => edit::listed(shared, &dest),
        Err(e) => e,
    }
}

/// What a piece added to a list did.
pub(super) struct Put {
    pub(super) path: PathBuf,
    pub(super) made: bool,
    pub(super) count: u32,
    pub(super) before: Option<String>,
    /// The text the list holds now.
    pub(super) after: String,
    /// The revision it has now.
    pub(super) rev: u64,
    /// Said once: the pieces were given ids.
    pub(super) note: Option<&'static str>,
}

/// One more copy of `want` in the list `list` (a listed `.dmap`'s id), or in
/// a new list in My tiles when none is named; `mode` lays a new list out in
/// that colour mode.  Adding names no revision: it puts one piece at the end
/// of a list the caller is not looking at.
///
/// # Errors
/// The list is not there, does not read (its mistakes), or cannot be written.
pub(super) fn put(
    shared: &Shared,
    want: &Want,
    list: Option<&str>,
    mode: Option<Mode>,
) -> Result<Put, (&'static str, Value)> {
    if let Some(id) = list {
        let path = list_path(shared, id)?;
        let mut count = 0;
        let mut gave = false;
        let e = shared.docs.edit(&path, None, |text| {
            let l = dt_doc::map::decode(&path, text).map_err(|e| diags(&e))?;
            gave = l.pieces.iter().any(|p| p.id.is_none());
            let (new, n) = dt_doc::map::add_or_bump(text, want).map_err(|e| diags(&e))?;
            count = n;
            Ok((new, json!({})))
        })?;
        changed(shared, &path);
        return Ok(Put {
            path: PathBuf::from(path),
            made: false,
            count,
            before: Some(e.before),
            after: e.after,
            rev: e.rev,
            note: gave.then_some(GAVE_IDS),
        });
    }
    let home = edit::home(shared)?;
    let text = dt_doc::map::new_list(
        &dt_doc::header::stamp(env!("DT_ENGINE_VERSION")),
        "Tiles to print.",
        &[dt_doc::map::piece_line(
            Some("p1"),
            want.tile,
            1,
            want.seed,
            want.system,
            want.always,
        )],
        Some(&printer_for(shared)),
        &shared.options().system,
        mode,
    );
    let dest = edit::write_new_ext(&home, "Print list", "dmap", text.as_bytes())?;
    let rev = shared.docs.rev(&dest.to_string_lossy())?;
    Ok(Put {
        path: dest,
        made: true,
        count: 1,
        before: None,
        after: text,
        rev,
        note: None,
    })
}

/// `/api/lists/add`.
pub(super) fn add(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let Some(id) = v["tile"].as_str() else {
        return bad("give the tile");
    };
    let Some(path) = lock(&shared.tiles)
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.path.clone())
        .filter(|p| p.to_ascii_lowercase().ends_with(".dtile"))
    else {
        return fail::refused("404 Not Found", "no such tile");
    };
    let canon = dt_eval::inventory::canonical(&path, shared.home.as_deref());
    let system = match v["system"].as_str() {
        None => None,
        Some(s @ ("db" | "ol")) => Some(s),
        Some(_) => return bad("the system is db or ol"),
    };
    let want = Want {
        tile: &canon,
        seed: v["seed"].as_u64(),
        system,
        always: false,
    };
    match put(shared, &want, v["list"].as_str(), None) {
        Ok(p) => {
            if p.made {
                shared.rescan();
                places::announce(shared);
                shared.want(&super::tile_id(&p.path.to_string_lossy()));
            }
            let path = p.path.to_string_lossy();
            let mut v = json!({
                "ok": true, "list": super::tile_id(&path), "path": path, "before": p.before,
                "text": p.after, "docRev": p.rev, "count": p.count, "made": p.made,
            });
            if let Some(n) = p.note {
                v["note"] = json!(n);
            }
            ("200 OK", v)
        }
        Err(e) => e,
    }
}

/// `/api/lists/count`.
pub(super) fn set_count(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(list), Some(piece), Some(n)) = (
        v["list"].as_str(),
        v["id"].as_str(),
        v["count"].as_u64().and_then(|n| u32::try_from(n).ok()),
    ) else {
        return bad("give the list, the piece (its id) and the count");
    };
    edit_list(shared, list, v, |t, _| {
        dt_doc::map::set_count(t, piece, n).map(|t| (t, json!({})))
    })
}

/// `/api/lists/remove`.
pub(super) fn remove(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(list), Some(piece)) = (v["list"].as_str(), v["id"].as_str()) else {
        return bad("give the list and the piece (its id)");
    };
    edit_list(shared, list, v, |t, _| {
        dt_doc::map::remove_piece(t, piece).map(|t| (t, json!({})))
    })
}

/// `/api/lists/restore` `{list, text, docRev}`: Undo or Redo, a text written
/// back (only if it reads as a print list), and only if the file is still at
/// `docRev`, the revision of the change this takes back.  A file that changed
/// since (another tab, a text editor) is refused with a 409 and its current
/// text, and left as it is: this would lose that change.
pub(super) fn restore(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(id), Some(text)) = (v["list"].as_str(), v["text"].as_str()) else {
        return bad("give the list and its text");
    };
    let rev = match docs::expected(v) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let path = match list_path(shared, id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let r = shared.docs.edit(&path, Some(rev), |_| {
        dt_doc::map::decode(&path, text).map_err(|e| diags(&e))?;
        Ok((text.to_owned(), json!({})))
    });
    match r {
        Ok(e) => {
            changed(shared, &path);
            done(shared, &path, &e)
        }
        Err(e) => with_current(shared, &path, e),
    }
}
