//! The calls that change the folders of a place and the tiles in them: `newFolder`, `renameFolder`, `trashFolder`,
//! `trashTile`, `undoTrash`, `moveTile`, `duplicateTile`, `rename` and `deleteTile`.  Their types and the one function
//! each that the program's HTTP handler and a web worker's `Service` both call, written on [`PlaceHost`]: the host says
//! which places there are, where its Trash is, and tells the page what changed.  What a call may touch is decided
//! here, once: only a folder place the user owns (never the library), no `..`, nothing hidden or build output, and
//! the real path still inside its place.  The folder chooser that adds a place (`addPlace`, `removePlace`) is the
//! program's own: a web page has no folder to point at.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::api::TileSummary;
use crate::call::{Refusal, Status, Verb};
use crate::place_calls::{Listed, tile_name, write_new};
use crate::read_calls::Nothing;
use crate::roots::{Root, SKIP_DIRS, kind_of};
use crate::tile::tile_id;
use crate::tile_host::{TileHost, TileRef};

/// A folder of a place: the place by its index, the folder by its path below it (`""` is the place).
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct FolderReq {
    pub root: usize,
    /// The folder, by its path below the place (`""` is the place).
    pub dir: String,
}

/// A folder, and the name a call gives it.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct FolderNamedReq {
    pub root: usize,
    pub dir: String,
    pub name: String,
}

/// A folder made or renamed: its path below the place.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct FolderDone {
    /// The stream's revision once the change was told: the page has the change when its stream reaches it.
    pub rev: u64,
    pub dir: String,
}

/// Something moved to the Trash: what it was and the number its Undo goes by.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct Trashed {
    /// The stream's revision once the change was told: the page has the change when its stream reaches it.
    pub rev: u64,
    pub dir: Option<String>,
    pub path: Option<String>,
    pub undo: usize,
}

/// `undoTrash`: the number a trash call answered.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct TrashUndoReq {
    pub undo: usize,
}

/// A tile (or folder) put back, with the list.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct Restored {
    pub path: String,
    pub id: Option<String>,
    pub tile: Option<TileSummary>,
    /// The stream's revision once the change was told: the page has the change when its stream reaches it.
    pub rev: u64,
}

/// [`Listed`], with the list as it now is.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct ListedIn {
    pub path: String,
    pub id: Option<String>,
    pub tile: Option<TileSummary>,
    /// The stream's revision once the change was told: the page has the change when its stream reaches it.
    pub rev: u64,
}

/// A tile moved into a folder.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct TileMoveReq {
    pub root: usize,
    pub dir: String,
}

/// `rename`: the tile's new name.
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct RenameReq {
    pub name: String,
}

/// A tile deleted: its path.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct Deleted {
    pub path: String,
}

pub const NEW_FOLDER: Verb<FolderNamedReq, FolderDone> = Verb::new("newFolder");
pub const RENAME_FOLDER: Verb<FolderNamedReq, FolderDone> = Verb::new("renameFolder");
pub const TRASH_FOLDER: Verb<FolderReq, Trashed> = Verb::new("trashFolder");
pub const TRASH_TILE: Verb<Nothing, Trashed> = Verb::new("trashTile");
pub const UNDO_TRASH: Verb<TrashUndoReq, Restored> = Verb::new("undoTrash");
pub const MOVE_TILE: Verb<TileMoveReq, ListedIn> = Verb::new("moveTile");
pub const DUPLICATE_TILE: Verb<Nothing, ListedIn> = Verb::new("duplicateTile");
pub const RENAME: Verb<RenameReq, Listed> = Verb::new("rename");
pub const DELETE_TILE: Verb<Nothing, Deleted> = Verb::new("deleteTile");

/// What the place calls ask of whoever runs them, beyond the tile calls' ([`TileHost`]): the places, the Trash, and
/// telling the page what changed.
pub trait PlaceHost: TileHost {
    /// The places the page lists, in order (a place is named by its index here).
    fn roots(&self) -> Vec<Root>;

    /// The Trash a tile or folder is moved to; none where there is none known.
    fn trash_dir(&self) -> Option<PathBuf>;

    /// Something was moved to the Trash (where it was, where it is now): the number its Undo goes by.
    fn trash_note(&self, from: &Path, to: &Path) -> usize;

    /// What was put in the Trash under `undo`: (where it was, where it is).
    fn trash_taken(&self, undo: usize) -> Option<(PathBuf, PathBuf)>;

    /// Look at the places again now and tell the page what is new.
    fn rescan(&self);

    /// The stream's revision once everything so far is told.
    fn told(&self) -> u64;

    /// The state of the tile `id` as the list holds it, when it is listed.
    fn summary(&self, id: &str) -> Option<TileSummary>;

    /// A tile (`folder` false) or a folder (`folder` true) moved from `from` to `to`: whatever the host keeps by path
    /// (the program's inventory of prints) follows it.
    fn moved(&self, from: &Path, to: &Path, folder: bool);
}

fn bad(msg: impl Into<String>) -> Refusal {
    Refusal::refused(Status::BadRequest, msg)
}

fn io_failure(what: &str, path: &Path, e: &std::io::Error) -> Refusal {
    Refusal::failure(
        Status::Internal,
        format!("cannot {what} {}: {e}", path.display()),
    )
    .caused_by(e)
}

/// What is at `path`: none when nothing is (the usual answer of a probe); a storage that cannot say is a failure.
fn probe(
    storage: &dyn crate::storage::Storage,
    path: &Path,
) -> Result<Option<crate::storage::Entry>, Refusal> {
    match storage.stat(path) {
        Ok(e) => Ok(Some(e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_failure("look at", path, &e)),
    }
}

/// `dir` as a list of names below a place: no empty, `.`, `..`, hidden, build-output or odd-character names.  `""` is
/// the place itself.
fn components(dir: &str) -> Result<Vec<&str>, Refusal> {
    if dir.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for c in dir.split('/') {
        let odd = c.is_empty()
            || c.starts_with('.')
            || c.contains(['\\', ':'])
            || c.chars().any(char::is_control)
            || SKIP_DIRS.contains(&c);
        if odd {
            return Err(bad(
                "a folder is named by the names below its place, one by one",
            ));
        }
        out.push(c);
    }
    Ok(out)
}

/// Place `i` as a folder place the user owns: its index checked, its folder.
fn place(host: &impl PlaceHost, i: usize) -> Result<PathBuf, Refusal> {
    let roots = host.roots();
    let Some(r) = roots.get(i) else {
        return Err(Refusal::refused(Status::NotFound, "no such place"));
    };
    if !r.writable() {
        return Err(Refusal::refused(
            Status::Forbidden,
            format!("{} is read only: its tiles are not changed here", r.label),
        ));
    }
    let Some(d) = r.dir() else {
        return Err(bad("that place is not a folder"));
    };
    Ok(d.to_path_buf())
}

/// Folder `dir` below place `root`: `(the place's folder, the folder)`.  The real path is checked to stay inside.
fn folder(host: &impl PlaceHost, root: usize, dir: &str) -> Result<(PathBuf, PathBuf), Refusal> {
    let base = place(host, root)?;
    let names = components(dir)?;
    let full = names.iter().fold(base.clone(), |p, n| p.join(n));
    let storage = host.storage();
    if !probe(storage, &full)?.is_some_and(|e| e.is_dir()) {
        return Err(Refusal::refused(Status::NotFound, "no such folder"));
    }
    if !storage.is_inside(&full, &base) {
        return Err(Refusal::refused(
            Status::Forbidden,
            "that folder is outside its place (a link leads out of it)",
        ));
    }
    Ok((base, full))
}

/// Where a new tile goes: folder `dir` of place `root` when `root` is given, else none (My tiles).
///
/// # Errors
/// [`Refusal`]: no such place or folder, a read-only place, or a folder that leads out of its place.
pub fn new_tile_dir(
    host: &impl PlaceHost,
    root: Option<usize>,
    dir: Option<&str>,
) -> Result<Option<PathBuf>, Refusal> {
    let Some(root) = root else {
        return Ok(None);
    };
    folder(host, root, dir.unwrap_or("")).map(|(_, f)| Some(f))
}

/// A tile file of a place the user owns.  Not the library, not a path with `..`, only a kind the actions apply to, and
/// its folder is really inside its place.
fn tile_file(host: &impl PlaceHost, tile: &str, what: &str) -> Result<PathBuf, Refusal> {
    if tile.starts_with(dt_eval::LIB) {
        return Err(bad(format!(
            "a library tile is read only: it is not {what}; make it yours first"
        )));
    }
    let path = PathBuf::from(tile);
    if path.components().any(|c| c == Component::ParentDir) {
        return Err(bad("a path with .. is not a tile"));
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !kind_of(name).is_some_and(|k| k.actions) {
        return Err(bad(format!("only a tile document is {what}")));
    }
    let storage = host.storage();
    let owned = host.roots().iter().any(|r| {
        r.writable()
            && r.dir().is_some_and(|d| {
                path.starts_with(d) && path.parent().is_some_and(|p| storage.is_inside(p, d))
            })
    });
    if owned {
        Ok(path)
    } else {
        Err(bad(format!(
            "only a tile in a folder you own is {what} here"
        )))
    }
}

/// One name of a file or folder from the page: one line of at most 80 characters, with what a file name cannot hold
/// turned into `-`, and no leading dot.
fn leaf_name(name: &str, what: &str) -> Result<String, Refusal> {
    let name = name.trim();
    if name.chars().count() > 80 || name.contains(['\n', '\r']) {
        return Err(bad(format!(
            "a {what} name is one line of at most 80 characters"
        )));
    }
    let out: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let out = out.trim_start_matches('.').trim().to_owned();
    if out.is_empty() {
        return Err(bad(format!("a {what} needs a name")));
    }
    if SKIP_DIRS.contains(&out.as_str()) {
        return Err(bad(format!(
            "“{out}” is not listed (it holds build output): pick another name"
        )));
    }
    Ok(out)
}

fn join_rel(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// A folder made, renamed or trashed is in the list: told, and the answer says the revision it was told at.
fn rescanned(host: &impl PlaceHost) -> u64 {
    host.rescan();
    host.told()
}

/// `newFolder` `{root, dir, name}`: folder `name` in `dir` of place `root`.
///
/// # Errors
/// [`Refusal`]: no such place or folder, a name that is not one, or a name already taken.
pub fn new_folder(host: &impl PlaceHost, req: FolderNamedReq) -> Result<FolderDone, Refusal> {
    let (_, parent) = folder(host, req.root, &req.dir)?;
    let name = leaf_name(&req.name, "folder")?;
    let to = parent.join(&name);
    // `create_dir` never goes over what is there
    match host.storage().create_dir(&to) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(Refusal::refused(
                Status::Conflict,
                format!("“{name}” is already in this folder"),
            ));
        }
        Err(e) => return Err(io_failure("make", &to, &e)),
    }
    Ok(FolderDone {
        rev: rescanned(host),
        dir: join_rel(&req.dir, &name),
    })
}

/// `renameFolder` `{root, dir, name}`: `dir` (not the place itself) gets a new last name.
///
/// # Errors
/// [`Refusal`]: as `newFolder`, and the place itself is not renamed.
pub fn rename_folder(host: &impl PlaceHost, req: FolderNamedReq) -> Result<FolderDone, Refusal> {
    let (base, from) = folder(host, req.root, &req.dir)?;
    if from == base {
        return Err(bad("a place is not renamed here: only a folder in it"));
    }
    let name = leaf_name(&req.name, "folder")?;
    let Some(parent) = from.parent() else {
        return Err(bad("the folder has no parent"));
    };
    let to = parent.join(&name);
    match host.storage().rename_dir(&from, &to) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(Refusal::refused(
                Status::Conflict,
                format!("“{name}” is already in this folder"),
            ));
        }
        Err(e) => return Err(io_failure("rename", &from, &e)),
    }
    // the prints of the tiles in it follow them
    host.moved(&from, &to, true);
    let dir = match req.dir.rsplit_once('/') {
        Some((up, _)) => join_rel(up, &name),
        None => name,
    };
    Ok(FolderDone {
        rev: rescanned(host),
        dir,
    })
}

/// Move `from` into the Trash under a name that is free there, and answer with the number its Undo goes by.
fn trashed(
    host: &impl PlaceHost,
    from: &Path,
    dir: Option<String>,
    path: Option<String>,
) -> Result<Trashed, Refusal> {
    let Some(trash) = host.trash_dir() else {
        return Err(Refusal::failure(
            Status::Internal,
            "there is no Trash known on this system, so nothing was moved (a delete is never a hard delete)",
        ));
    };
    let to = host.storage().trash(from, &trash).map_err(|e| {
        Refusal::failure(
            Status::Internal,
            format!(
                "cannot move {} to the Trash ({}): {e}; nothing was deleted",
                from.display(),
                trash.display()
            ),
        )
        .caused_by(&e)
    })?;
    let undo = host.trash_note(from, &to);
    Ok(Trashed {
        rev: rescanned(host),
        dir,
        path,
        undo,
    })
}

/// `trashFolder` `{root, dir}`.
///
/// # Errors
/// [`Refusal`]: no such place or folder, the place itself, or the Trash cannot take it.
pub fn trash_folder(host: &impl PlaceHost, req: FolderReq) -> Result<Trashed, Refusal> {
    let (base, from) = folder(host, req.root, &req.dir)?;
    if from == base {
        return Err(bad(
            "a place is taken off the list, not trashed: only a folder in it is",
        ));
    }
    trashed(host, &from, Some(req.dir), None)
}

/// `trashTile`.
///
/// # Errors
/// [`Refusal`]: not a tile of a place the user owns, not there, or the Trash cannot take it.
pub fn trash_tile(host: &impl PlaceHost, tile: &TileRef, _: Nothing) -> Result<Trashed, Refusal> {
    let path = tile_file(host, &tile.path, "moved to the Trash")?;
    if !probe(host.storage(), &path)?.is_some_and(|e| e.is_file()) {
        return Err(Refusal::refused(
            Status::NotFound,
            "the tile's file is not there",
        ));
    }
    trashed(host, &path, None, Some(tile.path.clone()))
}

/// `undoTrash` `{undo}`: what was trashed goes back where it was, when that place is free.
///
/// # Errors
/// [`Refusal`]: nothing under that number, the place it came from is gone or the spot is taken, or the move fails.
pub fn undo_trash(host: &impl PlaceHost, req: TrashUndoReq) -> Result<Restored, Refusal> {
    let Some((from, to)) = host.trash_taken(req.undo) else {
        return Err(Refusal::refused(
            Status::NotFound,
            "nothing was trashed under that number",
        ));
    };
    let storage = host.storage();
    // the place it came from must still be one the user owns
    let inside = host.roots().iter().any(|r| {
        r.writable()
            && r.dir().is_some_and(|d| {
                from.starts_with(d) && from.parent().is_some_and(|p| storage.is_inside(p, d))
            })
    });
    if !inside {
        return Err(Refusal::refused(
            Status::Conflict,
            "the folder it came from is not there any more: take it out of the Trash yourself",
        ));
    }
    if storage.exists(&from) {
        return Err(Refusal::refused(
            Status::Conflict,
            format!("{} is there again: it is left in the Trash", from.display()),
        ));
    }
    storage
        .restore(&to, &from)
        .map_err(|e| io_failure("put back", &to, &e))?;
    let path = from.to_string_lossy().into_owned();
    let is_tile = probe(storage, &from)?.is_some_and(|e| e.is_file());
    host.rescan();
    let id = tile_id(&path);
    let tile = if is_tile { host.summary(&id) } else { None };
    Ok(Restored {
        path,
        id: tile.as_ref().map(|_| id),
        tile,
        rev: host.told(),
    })
}

/// A file appears at `dest`: listed, its build asked for, and its state answered with the list.
fn listed_in(host: &impl PlaceHost, dest: &Path) -> ListedIn {
    let l = host.listed(dest);
    ListedIn {
        path: l.path,
        id: l.id,
        tile: l.tile,
        rev: host.told(),
    }
}

/// Put `from` at `to` without ever going over a file there.
fn move_no_clobber(
    host: &impl PlaceHost,
    from: &Path,
    to: &Path,
    clash: &str,
) -> Result<(), Refusal> {
    host.storage().move_no_clobber(from, to).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            Refusal::refused(Status::Conflict, clash.to_owned())
        } else {
            io_failure("move", from, &e)
        }
    })
}

/// `moveTile` `{root, dir}`: the tile's file goes into that folder, under the same name.
///
/// # Errors
/// [`Refusal`]: not a tile of a place the user owns, no such folder, or the name is taken there.
pub fn move_tile(
    host: &impl PlaceHost,
    tile: &TileRef,
    req: TileMoveReq,
) -> Result<ListedIn, Refusal> {
    let from = tile_file(host, &tile.path, "moved")?;
    let (_, dest_dir) = folder(host, req.root, &req.dir)?;
    let Some(name) = from.file_name() else {
        return Err(bad("the tile has no name"));
    };
    let to = dest_dir.join(name);
    if to == from {
        return Ok(listed_in(host, &to));
    }
    let clash = format!(
        "a tile called “{}” is already in that folder",
        from.file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
    );
    move_no_clobber(host, &from, &to, &clash)?;
    // its prints in the inventory follow it
    host.moved(&from, &to, false);
    Ok(listed_in(host, &to))
}

/// `duplicateTile`: a copy beside the tile, “Name copy”, never over a file.
///
/// # Errors
/// [`Refusal`]: not a tile of a place the user owns, or it cannot be read or the copy written.
pub fn duplicate_tile(
    host: &impl PlaceHost,
    tile: &TileRef,
    _: Nothing,
) -> Result<ListedIn, Refusal> {
    let from = tile_file(host, &tile.path, "copied")?;
    let text = host
        .storage()
        .read_bytes(&from)
        .map_err(|e| io_failure("read", &from, &e))?;
    let Some(dir) = from.parent() else {
        return Err(bad("the tile has no folder"));
    };
    let stem = from
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let dest = write_new(host.storage(), dir, &format!("{stem} copy"), "dtile", &text)?;
    Ok(listed_in(host, &dest))
}

/// `rename`: a tile's file gets another name in its folder.  Never over another file; only a change of case, on a
/// disk that ignores case, renames in place.
///
/// # Errors
/// [`Refusal`]: not a tile of a place the user owns, a name that is not one, or the name is taken.
pub fn rename(host: &impl PlaceHost, tile: &TileRef, v: RenameReq) -> Result<Listed, Refusal> {
    let from = tile_file(host, &tile.path, "renamed")?;
    let stem = tile_name(Some(&v.name))?;
    let Some(dir) = from.parent() else {
        return Err(bad("the tile has no folder"));
    };
    let to = dir.join(format!("{stem}.dtile"));
    if to == from {
        return Ok(host.listed(&to));
    }
    match host.storage().move_no_clobber(&from, &to) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(Refusal::refused(
                Status::Conflict,
                format!("a tile called “{stem}” is already in this folder"),
            ));
        }
        Err(e) => return Err(io_failure("rename", &from, &e)),
    }
    // its prints in the inventory follow it
    host.moved(&from, &to, false);
    Ok(host.listed(&to))
}

/// `deleteTile`: a tile's file is deleted from a folder the page lists: what the editor's Undo does to the copy it made
/// of a library tile.  Never the library, never a file outside the folders, never anything but a tile document.
///
/// # Errors
/// [`Refusal`]: not a tile of a place the user owns, or the file cannot be deleted.
pub fn delete_tile(host: &impl PlaceHost, tile: &TileRef, _: Nothing) -> Result<Deleted, Refusal> {
    let path = tile_file(host, &tile.path, "deleted")?;
    host.storage()
        .remove_file(&path)
        .map_err(|e| io_failure("delete", &path, &e))?;
    host.rescan();
    Ok(Deleted {
        path: tile.path.clone(),
    })
}
