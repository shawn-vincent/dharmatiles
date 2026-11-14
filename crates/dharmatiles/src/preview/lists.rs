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
//! a 422 with its mistakes, and nothing is written.
//!
//! - `/api/lists/new` `{name, root?, dir?}`: `<name>.dmap`, an empty map laid
//!   out for the printer last chosen.
//! - `/api/lists/add` `{tile, list?, seed?, system?, docRev?}`: one more copy of a tile
//!   in a list (a new list when none is named); a piece already there at the
//!   same seed and system has its `count` raised.  With a list, `docRev` is the
//!   revision the caller last saw (the page names it when it has the list open);
//!   a caller that has not read the list (the tile's own level) leaves it out.
//! - `/api/lists/count` `{list, id, count, docRev}`, `/api/lists/remove`
//!   `{list, id, docRev}`: a piece's copies, or the piece itself.
//! - `/api/lists/restore` `{list, text, docRev}`: Undo or Redo, a text
//!   written back, if the file is still at `docRev`; else 409.
//!
//! The first edit the editor makes to a file whose pieces have no ids writes
//! an id (`p1`, `p2`, …) on every one, in the same write, and says so once in
//! the answer's `note`.

use std::path::PathBuf;

use dt_doc::edit::EditError;
use dt_doc::map::{DEFAULT_PRINTER, Mode, Want};
use dt_doc::ops::{Kind, Op, apply};
use dt_eval::Diag;
use serde::{Deserialize, Serialize};

use super::api::{Out, Reply};
use super::docs::Edited;
use super::edit::CheckRes;
use super::messages::Scope;
use super::scene::SceneJson;
use super::{Shared, edit, fail, lock, places};

// ---- the calls' types ------------------------------------------------------------------

/// What a call that changed a list answers: the text it replaced (`before`),
/// the text it wrote (`text`), the revision the file has now (`docRev`) and
/// the map's scene as it now reads (cheap: nothing is built), so the page
/// shows the change at once.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ListEdited {
    /// The list's id.
    pub list: String,
    pub path: String,
    pub before: Option<String>,
    pub text: String,
    pub doc_rev: u64,
    pub scene: Option<SceneJson>,
    /// Said once: the pieces were given ids.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct ListsNewReq {
    pub name: Option<String>,
    pub root: Option<usize>,
    pub dir: Option<String>,
    /// A map (with a grid) rather than a print list.
    pub map: Option<bool>,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListsAddReq {
    pub tile: String,
    /// The list's id; none makes a new list.
    pub list: Option<String>,
    pub seed: Option<u64>,
    pub system: Option<String>,
    /// The revision of the list the caller last saw, when it has the list open.
    pub doc_rev: Option<u64>,
}

/// One more copy of a tile put in a list.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ListsAddRes {
    #[serde(flatten)]
    pub edited: ListEdited,
    /// How many copies of the piece the list now holds.
    pub count: u32,
    /// The list was made by this call.
    pub made: bool,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListsCountReq {
    pub list: String,
    pub id: String,
    pub count: u32,
    pub doc_rev: u64,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListsRemoveReq {
    pub list: String,
    pub id: String,
    pub doc_rev: u64,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListsRestoreReq {
    pub list: String,
    pub text: String,
    pub doc_rev: u64,
}

pub(super) fn bad(msg: &str) -> Reply {
    fail::refused("400 Bad Request", msg)
}

/// An edit that could not be made, as the call answers it: a document that
/// does not read answers with its own mistakes, anything else in words.
pub(super) fn edit_err(e: EditError) -> Reply {
    match e {
        EditError::Unread(d) => diags(&d),
        e => bad(&e.to_string()),
    }
}

pub(super) fn diags(e: &[dt_doc::DocError]) -> Reply {
    let d: Vec<Diag> = e.iter().map(Diag::from).collect();
    fail::mistakes(&d)
}

/// `/api/tiles/ID/check` for a print list or the inventory: its decoder's
/// mistakes at their text, as the tile's check gives its own.
pub(super) fn check_text(shared: &Shared, tile: &str, text: &str) -> CheckRes {
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
            CheckRes {
                ok: true,
                errors: Vec::new(),
                stack: None,
                map: None,
            }
        }
        Err(e) => {
            let d: Vec<Diag> = e.iter().map(Diag::from).collect();
            shared.report(&id, Scope::Check, |name| {
                super::messages::of_errors(&d, Scope::Check, &id, name, &[])
            });
            CheckRes {
                ok: false,
                errors: super::api::mistakes(&d),
                stack: None,
                map: None,
            }
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

pub(super) fn list_path(shared: &Shared, id: &str) -> Result<String, Reply> {
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

/// What a call that changed a list answers (see [`ListEdited`]).
pub(super) fn done(shared: &Shared, path: &str, e: &Edited, note: Option<&str>) -> ListEdited {
    ListEdited {
        list: super::tile_id(path),
        path: path.to_owned(),
        before: Some(e.before.clone()),
        text: e.after.clone(),
        doc_rev: e.rev,
        scene: super::maps::scene_of(shared, path, &e.after, e.rev),
        note: note.map(str::to_owned),
    }
}

const GAVE_IDS: &str =
    "Gave every piece an id (p1, p2, …) so edits can name it; the file keeps them.";

/// Edit the list `id` made against the revision `rev` the call names: `f` is given
/// the text (with an id on every piece) and what it reads as, and answers the
/// new text and what the call adds to its answer.  The edit is made under the
/// file's lock, checked to read, written whole, and the list is planned again.
pub(super) fn edit_list<X>(
    shared: &Shared,
    id: &str,
    rev: u64,
    f: impl FnOnce(&str, &dt_doc::map::PrintList) -> Result<(String, X), String>,
) -> Out<(ListEdited, X)> {
    let path = list_path(shared, id)?;
    let mut gave_note = None;
    let r = shared.docs.edit(&path, Some(rev), |text| {
        dt_doc::map::decode(&path, text).map_err(|e| diags(&e))?;
        let named = apply(Kind::Map, text, &Op::GiveIds).map_err(edit_err)?;
        let (named, gave) = (named.text, named.gave_ids);
        let list = dt_doc::map::decode(&path, &named).map_err(|e| diags(&e))?;
        let (new, extra) = f(&named, &list).map_err(|m| bad(&m))?;
        dt_doc::map::decode(&path, &new).map_err(|e| diags(&e))?;
        if gave {
            gave_note = Some(GAVE_IDS);
        }
        Ok((new, extra))
    });
    match r {
        Ok((e, extra)) => {
            changed(shared, &path);
            Ok((done(shared, &path, &e, gave_note), extra))
        }
        Err(e) => Err(with_current(shared, &path, e)),
    }
}

/// A refusal because the file moved on carries the file's scene as it is now,
/// so the page takes the current state at once, not when the worker's next word comes.
pub(super) fn with_current(shared: &Shared, path: &str, reply: Reply) -> Reply {
    if reply.0.starts_with("409")
        && let (Some(text), Some(rev)) = (reply.1["text"].as_str(), reply.1["docRev"].as_u64())
    {
        let mut body = reply.1.clone();
        body["scene"] = super::maps::scene_value(shared, path, text, rev);
        return (reply.0, body);
    }
    reply
}

/// `/api/lists/new`.
pub(super) fn new_list(shared: &Shared, v: ListsNewReq) -> Out<super::places::Listed> {
    let name = v
        .name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or("Print list");
    let stem = edit::tile_name(Some(name))?;
    let into = match places::new_tile_dir(shared, v.root, v.dir.as_deref()) {
        Some(r) => r?,
        None => edit::home(shared)?,
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
    if v.map == Some(true) {
        text = text.replacen(
            "    about \"Tiles to print.\"",
            "    grid cols=8 rows=6\n    about \"A map.\"",
            1,
        );
    }
    let dest = edit::write_new_ext(&into, &stem, "dmap", text.as_bytes())?;
    Ok(edit::listed(shared, &dest))
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
/// that colour mode.  With `rev` the add is made against that revision of the
/// list (a caller looking at the list names it); without, it puts one piece at the
/// end of a list the caller has not read (the tile's own level).
///
/// # Errors
/// The list is not there, does not read (its mistakes), or cannot be written.
pub(super) fn put(
    shared: &Shared,
    want: &Want,
    list: Option<&str>,
    rev: Option<u64>,
    mode: Option<Mode>,
) -> Result<Put, Reply> {
    if let Some(id) = list {
        let path = list_path(shared, id)?;
        let mut count = 0;
        let mut gave = false;
        let (e, ()) = shared.docs.edit(&path, rev, |text| {
            let l = dt_doc::map::decode(&path, text).map_err(|e| diags(&e))?;
            gave = l.pieces.iter().any(|p| p.id.is_none());
            let done = apply(Kind::Map, text, &Op::AddOrBump(want)).map_err(edit_err)?;
            count = done.count.unwrap_or(1);
            Ok((done.text, ()))
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
pub(super) fn add(shared: &Shared, v: ListsAddReq) -> Out<ListsAddRes> {
    let Some(path) = lock(&shared.tiles)
        .iter()
        .find(|t| t.id == v.tile)
        .map(|t| t.path.clone())
        .filter(|p| p.to_ascii_lowercase().ends_with(".dtile"))
    else {
        return Err(fail::refused("404 Not Found", "no such tile"));
    };
    let canon = dt_print::inventory::canonical(&path, shared.home.as_deref());
    let system = match v.system.as_deref() {
        None => None,
        Some(s @ ("db" | "ol")) => Some(s),
        Some(_) => return Err(bad("the system is db or ol")),
    };
    let want = Want {
        tile: &canon,
        seed: v.seed,
        system,
        always: false,
    };
    let p = put(shared, &want, v.list.as_deref(), v.doc_rev, None)?;
    if p.made {
        shared.rescan();
        places::announce(shared);
        shared.want(&super::tile_id(&p.path.to_string_lossy()));
    }
    let path = p.path.to_string_lossy().into_owned();
    Ok(ListsAddRes {
        edited: ListEdited {
            list: super::tile_id(&path),
            path,
            before: p.before,
            text: p.after,
            doc_rev: p.rev,
            scene: None,
            note: p.note.map(str::to_owned),
        },
        count: p.count,
        made: p.made,
    })
}

/// `/api/lists/count`.
pub(super) fn set_count(shared: &Shared, v: ListsCountReq) -> Out<ListEdited> {
    let (e, ()) = edit_list(shared, &v.list, v.doc_rev, |t, _| {
        apply(
            Kind::Map,
            t,
            &Op::SetCount {
                id: &v.id,
                count: v.count,
            },
        )
        .map(|d| (d.text, ()))
        .map_err(|e| e.to_string())
    })?;
    Ok(e)
}

/// `/api/lists/remove`.
pub(super) fn remove(shared: &Shared, v: ListsRemoveReq) -> Out<ListEdited> {
    let (e, ()) = edit_list(shared, &v.list, v.doc_rev, |t, _| {
        apply(Kind::Map, t, &Op::Remove { id: &v.id })
            .map(|d| (d.text, ()))
            .map_err(|e| e.to_string())
    })?;
    Ok(e)
}

/// `/api/lists/restore` `{list, text, docRev}`: Undo or Redo, a text written
/// back (only if it reads as a print list), and only if the file is still at
/// `docRev`, the revision of the change this takes back.  A file that changed
/// since (another tab, a text editor) is refused with a 409 and its current
/// text, and left as it is: this would lose that change.
pub(super) fn restore(shared: &Shared, v: ListsRestoreReq) -> Out<ListEdited> {
    let path = list_path(shared, &v.list)?;
    let r = shared.docs.edit(&path, Some(v.doc_rev), |_| {
        dt_doc::map::decode(&path, &v.text).map_err(|e| diags(&e))?;
        Ok((v.text.clone(), ()))
    });
    match r {
        Ok((e, ())) => {
            changed(shared, &path);
            Ok(done(shared, &path, &e, None))
        }
        Err(e) => Err(with_current(shared, &path, e)),
    }
}
