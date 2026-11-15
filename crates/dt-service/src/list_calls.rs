//! Starting and filling a print list from the editor: `listsNew`, `listsAdd`, `listsCount`, `listsRemove`,
//! `listsRestore`.  Their types and the one function each that the program's HTTP handler and a web worker's `Service`
//! both call, written on [`ListHost`].
//!
//! Every call is an edit of the list's text, made by `dt_doc::map` and written to the file (the host plans the plates
//! again), and every call that changes a list answers with `before`, the text it replaced, `text`, the text it wrote, and
//! `docRev`, the revision the file has now (see [`crate::docs`]), so the page can offer Undo and Redo (`listsRestore`
//! writes a text back).  An edit names its piece by id and the `docRev` it was made against, and a file that has moved on
//! is refused with a conflict and the current text.  A new list is made through the places' rules like a new tile: in My
//! tiles or a folder of a place the page names, never over a file.  A mistake in a list's text is a refusal with its
//! mistakes, and nothing is written.
//!
//! The first edit the editor makes to a file whose pieces have no ids writes an id (`p1`, `p2`, …) on every one, in the
//! same write, and says so once in the answer's `note`.

use std::path::PathBuf;

use dt_doc::map::{DEFAULT_PRINTER, Mode, Want};
use dt_doc::ops::{EditError, Kind, Op, apply};
use dt_eval::Diag;
use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::docs::{DocFail, Edited};
use crate::folder_calls::PlaceHost;
use crate::message::Scope;
use crate::place_calls::{Listed, tile_name, write_new};
use crate::scene::SceneJson;
use crate::scene_read::{SceneHost, scene_of};
use crate::tile_calls::CheckRes;

// ---- the calls' types ------------------------------------------------------------------

/// What a call that changed a list answers: the text it replaced (`before`), the text it wrote (`text`), the revision the
/// file has now (`docRev`) and the map's scene as it now reads (cheap: nothing is built), so the page shows the change at
/// once.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
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

/// `listsNew`: `<name>.dmap`, an empty list laid out for the printer last chosen.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct ListsNewReq {
    pub name: Option<String>,
    pub root: Option<usize>,
    pub dir: Option<String>,
    /// A map (with a grid) rather than a print list.
    pub map: Option<bool>,
}

/// `listsAdd`: one more copy of a tile in a list (a new list when none is named); a piece already there at the same seed
/// and system has its `count` raised.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListsAddReq {
    pub tile: String,
    /// The list's id; none makes a new list.
    pub list: Option<String>,
    pub seed: Option<u64>,
    pub system: Option<dt_lang::System>,
    /// The revision of the list the caller last saw, when it has the list open.
    pub doc_rev: Option<u64>,
}

/// One more copy of a tile put in a list.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ListsAddRes {
    #[serde(flatten)]
    pub edited: ListEdited,
    /// How many copies of the piece the list now holds.
    pub count: u32,
    /// The list was made by this call.
    pub made: bool,
}

/// `listsCount`: a piece's copies.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListsCountReq {
    pub list: String,
    pub id: String,
    pub count: u32,
    pub doc_rev: u64,
}

/// `listsRemove`: the piece itself.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListsRemoveReq {
    pub list: String,
    pub id: String,
    pub doc_rev: u64,
}

/// `listsRestore`: Undo or Redo, a text written back, if the file is still at `docRev`.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListsRestoreReq {
    pub list: String,
    pub text: String,
    pub doc_rev: u64,
}

pub const LISTS_NEW: Verb<ListsNewReq, Listed> = Verb::new("listsNew");
pub const LISTS_ADD: Verb<ListsAddReq, ListsAddRes> = Verb::new("listsAdd");
pub const LISTS_COUNT: Verb<ListsCountReq, ListEdited> = Verb::new("listsCount");
pub const LISTS_REMOVE: Verb<ListsRemoveReq, ListEdited> = Verb::new("listsRemove");
pub const LISTS_RESTORE: Verb<ListsRestoreReq, ListEdited> = Verb::new("listsRestore");

/// What the list and map calls ask of whoever runs them, beyond what a map is read with ([`SceneHost`]) and the places
/// ([`PlaceHost`]).
pub trait ListHost: SceneHost + PlaceHost {
    /// The path of tile (or list) `id` as the host names it.
    fn path_of(&self, id: &str) -> Option<String>;

    /// A list's file changed: plan its plates again.
    fn list_changed(&self, path: &str);

    /// What the last reading of the map `id` noted (its crowns), which is still true of a text that has not been built
    /// again.
    fn scene_notes(&self, id: &str) -> Vec<crate::scene::Note>;
}

fn bad(msg: impl Into<String>) -> Refusal {
    Refusal::refused(Status::BadRequest, msg)
}

/// Is this document a print list?
#[must_use]
pub fn is_plates(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".dmap")
}

/// Is this document an inventory?
#[must_use]
pub fn is_inventory(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".dinv")
}

/// An edit that could not be made, as the call answers it: a document that does not read answers with its own mistakes,
/// anything else in words.
#[must_use]
pub fn edit_err(e: EditError) -> Refusal {
    match e {
        EditError::Unread(d) => diags(&d),
        // an edit the checked `apply` refuses for the mistake it would add is told as a mistake
        e @ EditError::Adds(_) => crate::edit_calls::refusal(&e),
        e => bad(e.to_string()),
    }
}

/// Why a list's edit was not made.  A closure of [`edit_list`] gives one by `?` from words (a 400) or from an
/// [`EditError`] (as [`edit_err`] tells it).
pub struct ListErr(pub Refusal);

impl From<String> for ListErr {
    fn from(m: String) -> Self {
        Self(bad(m))
    }
}

impl From<&str> for ListErr {
    fn from(m: &str) -> Self {
        Self(bad(m))
    }
}

impl From<EditError> for ListErr {
    fn from(e: EditError) -> Self {
        Self(edit_err(e))
    }
}

/// A document's mistakes as the refusal that carries them.
#[must_use]
pub fn diags(e: &[dt_doc::DocError]) -> Refusal {
    let d: Vec<Diag> = e.iter().map(Diag::from).collect();
    Refusal::of_mistakes(&d)
}

/// A print list or the inventory read by its own decoder: its mistakes, at their text, as the tile's check gives its own.
pub fn check_text(host: &impl ListHost, tile: &str, text: &str) -> CheckRes {
    let result = if is_inventory(tile) {
        dt_doc::inventory::decode(tile, text).map(|_| ())
    } else {
        dt_doc::map::decode(tile, text).map(|_| ())
    };
    let id = crate::tile::tile_id(tile);
    let ok = |errors: Vec<dt_eval::Mistake>| CheckRes {
        ok: errors.is_empty(),
        errors,
        stack: None,
        map: None,
        nodes: Vec::new(),
        nodes_rev: None,
    };
    match result {
        Ok(()) => {
            // what the last reading of a map noted (its crowns) is still true of this text
            let notes = host.scene_notes(&id);
            host.report(&id, Scope::Check, |name| {
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
            ok(Vec::new())
        }
        Err(e) => {
            let d: Vec<Diag> = e.iter().map(Diag::from).collect();
            host.report(&id, Scope::Check, |name| {
                crate::tile_calls::of_errors(&d, Scope::Check, &id, name, &[])
            });
            ok(dt_eval::mistakes(&d))
        }
    }
}

/// The printer last chosen (the title bar's), where it holds a bed.
fn printer_for(host: &impl ListHost) -> String {
    let name = host.options().printer.name;
    match crate::scene_read::machine(host, &name) {
        Ok(m) if m.bed.is_some() => name,
        _ => DEFAULT_PRINTER.to_owned(),
    }
}

/// The path of the print list `id`.
///
/// # Errors
/// [`Refusal`] (not found): there is no such print list.
pub fn list_path(host: &impl ListHost, id: &str) -> Result<String, Refusal> {
    host.path_of(id)
        .filter(|p| is_plates(p))
        .ok_or_else(|| Refusal::refused(Status::NotFound, "no such print list"))
}

/// A refusal because the file moved on carries the file's scene as it is now, so the page takes the current state at
/// once, not when the next word comes.
#[must_use]
pub fn with_current(host: &impl SceneHost, path: &str, mut r: Refusal) -> Refusal {
    if r.status == Status::Conflict
        && let Some(c) = r.conflict.as_mut()
    {
        c.scene = scene_of(host, path, &c.text, c.doc_rev).map(Box::new);
    }
    r
}

/// What a call that changed a list answers (see [`ListEdited`]).
#[must_use]
pub fn done(host: &impl ListHost, path: &str, e: &Edited, note: Option<&str>) -> ListEdited {
    ListEdited {
        list: crate::tile::tile_id(path),
        path: path.to_owned(),
        before: Some(e.before.clone()),
        text: e.after.clone(),
        doc_rev: e.rev,
        scene: scene_of(host, path, &e.after, e.rev),
        note: note.map(str::to_owned),
    }
}

const GAVE_IDS: &str =
    "Gave every piece an id (p1, p2, …) so edits can name it; the file keeps them.";

/// Edit the list `id` made against the revision `rev` the call names: `f` is given the text (with an id on every piece)
/// and what it reads as, and answers the new text and what the call adds to its answer.  The edit is made under the
/// file's lock, checked to read, written whole, and the list is planned again.
///
/// # Errors
/// [`Refusal`]: no such list, a file that moved on (a conflict with the current text and scene), a text that does not
/// read, or what `f` refused.
pub fn edit_list<X>(
    host: &impl ListHost,
    id: &str,
    rev: u64,
    f: impl FnOnce(&str, &dt_doc::map::PrintList) -> Result<(String, X), ListErr>,
) -> Result<(ListEdited, X), Refusal> {
    let path = list_path(host, id)?;
    let mut gave_note = None;
    let r = host.docs().edit(&path, Some(rev), |text| {
        dt_doc::map::decode(&path, text).map_err(|e| diags(&e))?;
        let named = apply(Kind::Map, text, &Op::GiveIds).map_err(edit_err)?;
        let (named, gave) = (named.text, named.gave_ids);
        let list = dt_doc::map::decode(&path, &named).map_err(|e| diags(&e))?;
        let (new, extra) = f(&named, &list).map_err(|e| e.0)?;
        dt_doc::map::decode(&path, &new).map_err(|e| diags(&e))?;
        if gave {
            gave_note = Some(GAVE_IDS);
        }
        Ok((new, extra))
    });
    match r {
        Ok((e, extra)) => {
            host.list_changed(&path);
            Ok((done(host, &path, &e, gave_note), extra))
        }
        Err(e) => Err(with_current(host, &path, e.into_refusal())),
    }
}

/// `listsNew`.
///
/// # Errors
/// [`Refusal`]: a bad name, a folder that is not the user's, or a file that cannot be written.
pub fn new_list(host: &impl ListHost, v: ListsNewReq) -> Result<Listed, Refusal> {
    let name = v
        .name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or("Print list");
    let stem = tile_name(Some(name))?;
    let into = match crate::folder_calls::new_tile_dir(host, v.root, v.dir.as_deref())? {
        Some(d) => d,
        None => host.home()?,
    };
    let mut text = dt_doc::map::new_list(
        &dt_doc::header::stamp(&host.version()),
        "Tiles to print.",
        &[],
        Some(&printer_for(host)),
        host.options().system,
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
    let dest = write_new(host.storage(), &into, &stem, "dmap", text.as_bytes())?;
    Ok(host.listed(&dest))
}

/// What a piece added to a list did.
pub struct Put {
    pub path: PathBuf,
    pub made: bool,
    pub count: u32,
    pub before: Option<String>,
    /// The text the list holds now.
    pub after: String,
    /// The revision it has now.
    pub rev: u64,
    /// Said once: the pieces were given ids.
    pub note: Option<&'static str>,
}

/// One more copy of `want` in the list `list` (a listed `.dmap`'s id), or in a new list in My tiles when none is named;
/// `mode` lays a new list out in that colour mode.  With `rev` the add is made against that revision of the list (a
/// caller looking at the list names it); without, it puts one piece at the end of a list the caller has not read (the
/// tile's own level).
///
/// # Errors
/// The list is not there, does not read (its mistakes), or cannot be written.
pub fn put(
    host: &impl ListHost,
    want: &Want,
    list: Option<&str>,
    rev: Option<u64>,
    mode: Option<Mode>,
) -> Result<Put, Refusal> {
    if let Some(id) = list {
        let path = list_path(host, id)?;
        let mut count = 0;
        let mut gave = false;
        let (e, ()) = host
            .docs()
            .edit(&path, rev, |text| {
                let l = dt_doc::map::decode(&path, text).map_err(|e| diags(&e))?;
                gave = l.pieces.iter().any(|p| p.id.is_none());
                let done = apply(Kind::Map, text, &Op::AddOrBump(want)).map_err(edit_err)?;
                count = done.count.unwrap_or(1);
                Ok((done.text, ()))
            })
            .map_err(DocFail::into_refusal)?;
        host.list_changed(&path);
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
    let home = host.home()?;
    let text = dt_doc::map::new_list(
        &dt_doc::header::stamp(&host.version()),
        "Tiles to print.",
        &[dt_doc::map::piece_line(
            Some("p1"),
            want.tile,
            1,
            want.seed,
            want.system,
            want.always,
        )],
        Some(&printer_for(host)),
        host.options().system,
        mode,
    );
    let dest = write_new(host.storage(), &home, "Print list", "dmap", text.as_bytes())?;
    let rev = host.docs().rev(&dest.to_string_lossy())?;
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

/// `listsAdd`.
///
/// # Errors
/// [`Refusal`]: no such tile or list, a list that does not read or moved on, or a file that cannot be written.
pub fn add(host: &impl ListHost, v: ListsAddReq) -> Result<ListsAddRes, Refusal> {
    let Some(path) = host
        .path_of(&v.tile)
        .filter(|p| p.to_ascii_lowercase().ends_with(".dtile"))
    else {
        return Err(Refusal::refused(Status::NotFound, "no such tile"));
    };
    let home = host.tiles_home();
    let canon = dt_print::inventory::canonical(&path, home.as_deref());
    let want = Want {
        tile: &canon,
        seed: v.seed,
        system: v.system,
        always: false,
    };
    let p = put(host, &want, v.list.as_deref(), v.doc_rev, None)?;
    if p.made {
        // list it, and plan it
        host.listed(&p.path);
    }
    let path = p.path.to_string_lossy().into_owned();
    Ok(ListsAddRes {
        edited: ListEdited {
            list: crate::tile::tile_id(&path),
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

/// `listsCount`.
///
/// # Errors
/// As [`edit_list`].
pub fn set_count(host: &impl ListHost, v: ListsCountReq) -> Result<ListEdited, Refusal> {
    let (e, ()) = edit_list(host, &v.list, v.doc_rev, |t, _| {
        apply(
            Kind::Map,
            t,
            &Op::SetCount {
                id: &v.id,
                count: v.count,
            },
        )
        .map(|d| (d.text, ()))
        .map_err(ListErr::from)
    })?;
    Ok(e)
}

/// `listsRemove`.
///
/// # Errors
/// As [`edit_list`].
pub fn remove(host: &impl ListHost, v: ListsRemoveReq) -> Result<ListEdited, Refusal> {
    let (e, ()) = edit_list(host, &v.list, v.doc_rev, |t, _| {
        apply(Kind::Map, t, &Op::Remove { id: &v.id })
            .map(|d| (d.text, ()))
            .map_err(ListErr::from)
    })?;
    Ok(e)
}

/// `listsRestore`: Undo or Redo, a text written back (only if it reads as a print list), and only if the file is still at
/// `docRev`, the revision of the change this takes back.  A file that changed since (another tab, a text editor) is
/// refused with its current text, and left as it is: this would lose that change.
///
/// # Errors
/// [`Refusal`]: no such list, a file that moved on, or a text that does not read.
pub fn restore(host: &impl ListHost, v: ListsRestoreReq) -> Result<ListEdited, Refusal> {
    let path = list_path(host, &v.list)?;
    let r = host.docs().edit(&path, Some(v.doc_rev), |_| {
        dt_doc::map::decode(&path, &v.text).map_err(|e| diags(&e))?;
        Ok((v.text.clone(), ()))
    });
    match r {
        Ok((e, ())) => {
            host.list_changed(&path);
            Ok(done(host, &path, &e, None))
        }
        Err(e) => Err(with_current(host, &path, e.into_refusal())),
    }
}
