//! Starting and filling a print list from the editor.
//!
//! Every call is an edit of the list's text, made by `dt_doc::map` and written
//! to the file (the worker sees it and plans the plates again), and every call
//! that changes a list answers with `before`, the text it replaced, so the
//! page can offer Undo (`/api/lists/restore` writes it back).  A new list is
//! made through the places' rules like a new tile: in My tiles or a folder of
//! a place the page names, never over a file.  A mistake in a list's text is
//! shown as data (`{ok: false, errors}`), and nothing is written.
//!
//! - `/api/lists/new` `{name, root?, dir?}`: `<name>.dmap`, an empty map laid
//!   out for the printer last chosen.
//! - `/api/lists/add` `{tile, list?, seed?, system?}`: one more copy of a tile
//!   in a list (a new list when none is named); a piece already there at the
//!   same seed and system has its `count` raised.
//! - `/api/lists/count` `{list, index, count}`, `/api/lists/remove` `{list,
//!   index}`: a piece's copies, or the piece itself.
//! - `/api/lists/restore` `{list, text}`: Undo.

use std::path::{Path, PathBuf};

use dt_doc::map::{DEFAULT_PRINTER, Mode, Want};
use dt_eval::Diag;
use serde_json::{Value, json};

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
            shared.report(&id, Scope::Check, |_| Vec::new());
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

/// A list's text changed: written, and planned again.
pub(super) fn write(shared: &Shared, path: &str, text: &str) -> Result<(), (&'static str, Value)> {
    std::fs::write(path, text).map_err(|e| {
        fail::failure(
            "500 Internal Server Error",
            format!("cannot write {path}: {e}"),
        )
    })?;
    super::plates::again(shared, &super::tile_id(path));
    Ok(())
}

pub(super) fn read(path: &str) -> Result<String, (&'static str, Value)> {
    std::fs::read_to_string(path).map_err(|e| {
        fail::failure(
            "500 Internal Server Error",
            format!("cannot read {path}: {e}"),
        )
    })
}

/// What a call that changed a list answers.
pub(super) fn done(path: &str, before: Option<String>, extra: Value) -> (&'static str, Value) {
    let mut v = json!({
        "ok": true, "list": super::tile_id(path), "path": path, "before": before,
    });
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    ("200 OK", v)
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
}

/// One more copy of `want` in the list `list` (a listed `.dmap`'s id), or in
/// a new list in My tiles when none is named; `mode` lays a new list out in
/// that colour mode.
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
        let text = read(&path)?;
        let (new, count) = dt_doc::map::add_or_bump(&text, want).map_err(|e| diags(&e))?;
        write(shared, &path, &new)?;
        return Ok(Put {
            path: PathBuf::from(path),
            made: false,
            count,
            before: Some(text),
        });
    }
    let home = edit::home(shared)?;
    let text = dt_doc::map::new_list(
        &dt_doc::header::stamp(env!("DT_ENGINE_VERSION")),
        "Tiles to print.",
        &[dt_doc::map::piece_line(
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
    Ok(Put {
        path: dest,
        made: true,
        count: 1,
        before: None,
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
            done(
                &p.path.to_string_lossy(),
                p.before,
                json!({"count": p.count, "made": p.made}),
            )
        }
        Err(e) => e,
    }
}

fn index_of(v: &Value) -> Option<usize> {
    v["index"].as_u64().and_then(|n| usize::try_from(n).ok())
}

/// `/api/lists/count`.
pub(super) fn set_count(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(id), Some(i), Some(n)) = (
        v["list"].as_str(),
        index_of(v),
        v["count"].as_u64().and_then(|n| u32::try_from(n).ok()),
    ) else {
        return bad("give the list, the piece and the count");
    };
    edit_text(shared, id, |t| dt_doc::map::set_count(t, i, n))
}

/// `/api/lists/remove`.
pub(super) fn remove(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(id), Some(i)) = (v["list"].as_str(), index_of(v)) else {
        return bad("give the list and the piece");
    };
    edit_text(shared, id, |t| dt_doc::map::remove_piece(t, i))
}

fn edit_text(
    shared: &Shared,
    id: &str,
    f: impl FnOnce(&str) -> Result<String, String>,
) -> (&'static str, Value) {
    let path = match list_path(shared, id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let text = match read(&path) {
        Ok(t) => t,
        Err(e) => return e,
    };
    if let Err(e) = dt_doc::map::decode(&path, &text) {
        return diags(&e);
    }
    let new = match f(&text) {
        Ok(n) => n,
        Err(m) => return bad(&m),
    };
    if let Err(e) = dt_doc::map::decode(&path, &new) {
        return diags(&e);
    }
    match write(shared, &path, &new) {
        Ok(()) => done(&path, Some(text), json!({})),
        Err(e) => e,
    }
}

/// `/api/lists/restore`: Undo of a change, the text it replaced written back
/// (only if it reads as a print list).
pub(super) fn restore(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let (Some(id), Some(text)) = (v["list"].as_str(), v["text"].as_str()) else {
        return bad("give the list and its text");
    };
    let path = match list_path(shared, id) {
        Ok(p) => p,
        Err(e) => return e,
    };
    if let Err(e) = dt_doc::map::decode(&path, text) {
        return diags(&e);
    }
    let now = match read(&path) {
        Ok(t) => t,
        Err(e) => return e,
    };
    match write(shared, &path, text) {
        Ok(()) => done(
            Path::new(&path).to_str().unwrap_or(&path),
            Some(now),
            // a map shows the text it has again at once
            json!({"scene": super::maps::scene_of(shared, &path, text)}),
        ),
        Err(e) => e,
    }
}
