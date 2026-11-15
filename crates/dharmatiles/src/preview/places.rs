//! Places: the folders the page lists tiles from, and what it may do in them.
//!
//! A place is a root ([`Root`]): My tiles, a folder named on the command line,
//! a folder the user added with the Mac's folder chooser, the library, the
//! tests.  The page names a place by its index in the list of places and a folder
//! in it by a path below it (`dungeon/combo`), a tile by its id; it never
//! sends a path of its own, and the chooser is shown by this program: the
//! page cannot point the server at a folder.  Every call below checks, on the
//! disk, that what it touches is inside a place the user owns (not the
//! library): no `..`, nothing hidden or build output, and the real path (after
//! symlinks) still under the place's real path.
//!
//! | route | what |
//! |---|---|
//! | `POST /api/places/add` | show the folder chooser; the folder chosen becomes a place, kept in the settings.  A cancel answers `{cancelled: true}` |
//! | `POST /api/places/remove` `{root}` | take an added place off the list (the folder on disk is not touched) |
//! | `POST /api/folders/new` `{root, dir, name}` | make folder `name` in `dir` of place `root` |
//! | `POST /api/folders/rename` `{root, dir, name}` | rename the folder `dir` |
//! | `POST /api/folders/trash` `{root, dir}` | move the folder `dir` to the Trash |
//! | `POST /api/tiles/ID/move` `{root, dir}` | move a tile into folder `dir` of place `root` |
//! | `POST /api/tiles/ID/duplicate` | copy a tile beside itself |
//! | `POST /api/tiles/ID/trash` | move a tile to the Trash |
//! | `POST /api/trash/undo` `{undo}` | put back what a trash call answered `undo` for |
//!
//! The per-user settings are `places.json` in [`settings_dir`]: the folders
//! added, as real paths.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::api::{Out, Reply};
use super::storage::Storage;
use super::*;

// ---- the calls' types ------------------------------------------------------------------

/// A call with nothing to say besides which tile it is for.
pub use dt_service::read_calls::Nothing;

#[derive(Deserialize, Serialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PlacesRemoveReq {
    /// The place, by its index in `roots`.
    pub root: usize,
}

/// What `places/add` answers: the place added, or that the chooser was closed.
#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(untagged)]
pub enum PlacesAddRes {
    Cancelled { cancelled: bool },
    Added(Box<PlaceAdded>),
}

#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PlaceAdded {
    pub added: bool,
    pub root: usize,
    pub label: String,
    pub path: String,
    /// The stream's revision once the change was told: the page has the change when its stream reaches it.
    pub rev: u64,
}

/// A change to the places or their folders: the list as it now is.
#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Changed {
    /// The stream's revision once the change was told: the page has the change when its stream reaches it.
    pub rev: u64,
}

pub use dt_service::folder_calls::{
    FolderDone, FolderNamedReq, FolderReq, ListedIn, Restored, TileMoveReq, TrashUndoReq, Trashed,
};
pub use dt_service::place_calls::Listed;

// ---- the settings ------------------------------------------------------------------

/// The folder the per-user settings are kept in: `$DT_SETTINGS` (the tests and
/// trial runs point it at a scratch folder), else
/// `~/Library/Application Support/dharmatiles` on a Mac, `%APPDATA%\dharmatiles`
/// on Windows, `$XDG_CONFIG_HOME/dharmatiles` or `~/.config/dharmatiles` elsewhere.
///
/// # Errors
/// When no home folder is known to put them in.
pub fn settings_dir() -> Result<PathBuf, String> {
    if let Some(d) = std::env::var_os("DT_SETTINGS").filter(|d| !d.is_empty()) {
        return Ok(d.into());
    }
    let home = || {
        std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                "cannot find your home folder to keep settings in: set DT_SETTINGS to a folder for them".to_owned()
            })
    };
    if cfg!(target_os = "macos") {
        Ok(home()?.join("Library/Application Support/dharmatiles"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .filter(|h| !h.is_empty())
            .map(|a| PathBuf::from(a).join("dharmatiles"))
            .ok_or_else(|| "cannot find %APPDATA% to keep settings in: set DT_SETTINGS".to_owned())
    } else if let Some(x) = std::env::var_os("XDG_CONFIG_HOME").filter(|h| !h.is_empty()) {
        Ok(PathBuf::from(x).join("dharmatiles"))
    } else {
        Ok(home()?.join(".config/dharmatiles"))
    }
}

/// The Trash: `$DT_TRASH` (scratch runs), else `~/.Trash` on a Mac; none elsewhere
/// (the page then says moving to the Trash is not supported, and deletes nothing).
#[must_use]
pub fn trash_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("DT_TRASH").filter(|d| !d.is_empty()) {
        return Some(d.into());
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(|h| PathBuf::from(h).join(".Trash"));
    }
    None
}

fn places_file(dir: &Path) -> PathBuf {
    dir.join("places.json")
}

/// The folders added so far, from the settings (none when there is no file yet).
///
/// # Errors
/// A settings file that cannot be read or is not what it should be: told, never
/// replaced by an empty list (the next save would lose the folders in it).
pub fn load_places(storage: &dyn Storage, dir: &Path) -> Result<Vec<PathBuf>, String> {
    let file = places_file(dir);
    let text = match storage.read_text(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", file.display())),
    };
    let file_of: PlacesFile = serde_json::from_str(&text).map_err(|e| {
        format!(
            "{} is not valid: {e}; fix it or move it aside",
            file.display()
        )
    })?;
    Ok(file_of.places)
}

/// `places.json`: the folders added to the Library and My tiles, the one type of that file, read by [`load_places`] and
/// written by [`save_places`].
#[derive(serde::Serialize, serde::Deserialize)]
struct PlacesFile {
    places: Vec<PathBuf>,
}

/// Keep `places` as the added folders.
///
/// # Errors
/// The settings folder or file cannot be written.
pub fn save_places(storage: &dyn Storage, dir: &Path, places: &[PathBuf]) -> Result<(), String> {
    storage
        .create_dir_all(dir)
        .map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    let file = places_file(dir);
    let body = PlacesFile {
        places: places.to_vec(),
    };
    let text = serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?;
    storage
        .write_text(&file, &format!("{text}\n"))
        .map_err(|e| format!("cannot write {}: {e}", file.display()))
}

/// A folder's name for the Places list.
#[must_use]
pub fn label_of(dir: &Path) -> String {
    dir.file_name().map_or_else(
        || dir.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

// ---- the folder chooser ---------------------------------------------------------------

/// How a folder is chosen for a new place.
#[derive(Clone, Debug)]
pub enum Picker {
    /// The system's folder chooser (the Mac's, through `osascript`).
    Native,
    /// No dialog: this folder, or (`None`) a cancel.  For the tests and for a
    /// trial run (`DT_PICKER=<folder>` or `DT_PICKER=cancel`) that must not
    /// put a dialog on a screen.
    Fixed(Option<PathBuf>),
}

impl Picker {
    /// `Native`, unless `$DT_PICKER` names a folder (or `cancel`).
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("DT_PICKER") {
            Ok(v) if v == "cancel" => Self::Fixed(None),
            Ok(v) if !v.is_empty() => Self::Fixed(Some(v.into())),
            _ => Self::Native,
        }
    }

    /// The folder chosen: `None` when the user cancelled.
    fn choose(&self) -> Result<Option<PathBuf>, String> {
        match self {
            Self::Fixed(p) => Ok(p.clone()),
            Self::Native if cfg!(target_os = "macos") => native_choose(),
            Self::Native => Err(
                "this system has no folder chooser for dharmatiles to show; start it with the folder as an argument: dharmatiles <folder>"
                    .to_owned(),
            ),
        }
    }
}

fn native_choose() -> Result<Option<PathBuf>, String> {
    let out = std::process::Command::new("osascript")
        .args([
            // the chooser belongs to osascript, which is not in front of the browser: bring it forward
            "-e",
            "tell me to activate",
            "-e",
            "POSIX path of (choose folder with prompt \"Choose a folder of tiles to add to dharmatiles\")",
        ])
        .output()
        .map_err(|e| format!("cannot show the folder chooser (osascript): {e}"))?;
    if out.status.success() {
        let p = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let p = if p.len() > 1 {
            p.trim_end_matches('/')
        } else {
            &p
        };
        return Ok(Some(PathBuf::from(p)));
    }
    let err = String::from_utf8_lossy(&out.stderr);
    // "User canceled. (-128)": the user closed the chooser.  A cancel is an answer, not a failure: silent.
    if err.contains("(-128)") {
        return Ok(None);
    }
    Err(format!("the folder chooser failed: {}", err.trim()))
}

// ---- reading a tile's size -------------------------------------------------------------

/// The size a tile document writes (`2x1`), from the `tile` node's `size=`;
/// none for a file that is not read or says none.
#[must_use]
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a best-effort filesystem probe (a listing shows no size for a file it cannot read; opening the tile reports why)"
)]
pub fn size_of(path: &str) -> Option<String> {
    if !path.ends_with(".dtile") {
        return None;
    }
    let text = dt_eval::Source::read(&dt_eval::Files, path).ok()?;
    size_in(&text)
}

fn size_in(text: &str) -> Option<String> {
    let line = text
        .lines()
        .map(str::trim_start)
        .find(|l| l.starts_with("tile ") || l.starts_with("tile{"))?;
    let rest = &line[line.find("size=")? + 5..];
    let rest = rest.trim_start_matches('"');
    let v: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, 'x' | 'X'))
        .collect();
    (v.contains(['x', 'X']) && !v.is_empty()).then_some(v.to_ascii_lowercase())
}

// ---- what a call may touch ----------------------------------------------------------------

fn refuse(status: &'static str, msg: impl Into<String>) -> Reply {
    fail::refused(status, msg)
}

fn io_failure(what: &str, path: &Path, e: &std::io::Error) -> Reply {
    fail::failure_from(
        "500 Internal Server Error",
        format!("cannot {what} {}: {e}", path.display()),
        e,
    )
}

// ---- the places -----------------------------------------------------------------------------

/// Tell every listener what changed among the places, the inventory and the list (only that: see `resources`).
pub(super) fn announce(shared: &Shared) {
    shared.publish(false);
}

/// `POST /api/places/add`: show the chooser; the folder chosen is a place from now on.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a path outside its root stays as written (display only)"
)]
pub(super) fn add_place(shared: &Shared, _: Nothing) -> Out<PlacesAddRes> {
    let Some(settings) = shared.settings.clone() else {
        return Err(fail::failure(
            "500 Internal Server Error",
            "no settings folder is set, so an added folder could not be kept",
        ));
    };
    let picked = match shared.picker.choose() {
        Ok(Some(p)) => p,
        // the user closed the chooser: nothing to report, and nothing changes
        Ok(None) => return Ok(PlacesAddRes::Cancelled { cancelled: true }),
        Err(e) => return Err(fail::failure("500 Internal Server Error", e)),
    };
    let _one_at_a_time = lock(&shared.gate);
    let real = match std::fs::canonicalize(&picked) {
        Ok(r) if r.is_dir() => r,
        Ok(r) => {
            return Err(refuse(
                "400 Bad Request",
                format!("{} is not a folder", r.display()),
            ));
        }
        Err(e) => return Err(io_failure("read", &picked, &e)),
    };
    // a place may not lie in, or hold, another folder (a tile would be listed twice), nor touch the library
    let mut others: Vec<(String, PathBuf)> = shared
        .roots()
        .iter()
        .filter_map(|r| Some((r.label.clone(), r.dir()?.to_path_buf())))
        .collect();
    if let Some(lib) = dt_eval::source::library_dir() {
        others.push(("the library".to_owned(), lib.to_path_buf()));
    }
    for (label, d) in others {
        let other = std::fs::canonicalize(&d).unwrap_or(d);
        if real.starts_with(&other) {
            return Err(refuse(
                "409 Conflict",
                format!("{} is already in {label}", real.display()),
            ));
        }
        if other.starts_with(&real) {
            return Err(refuse(
                "409 Conflict",
                format!(
                    "{} holds {label}: add a folder that does not",
                    real.display()
                ),
            ));
        }
    }
    let mut kept = match load_places(shared.storage.as_ref(), &settings) {
        Ok(k) => k,
        Err(e) => return Err(fail::failure("500 Internal Server Error", e)),
    };
    kept.push(real.clone());
    if let Err(e) = save_places(shared.storage.as_ref(), &settings, &kept) {
        return Err(fail::failure("500 Internal Server Error", e));
    }
    let root = Root::new(&label_of(&real), RootKind::Dir(real.clone())).with_role(Role::Added);
    let index = shared
        .owner
        .ask(|r| super::owner::Command::AddRoot(root, r));
    let label = label_of(&real);
    shared.rescan_held();
    announce(shared);
    Ok(PlacesAddRes::Added(Box::new(PlaceAdded {
        added: true,
        root: index,
        label,
        path: real.to_string_lossy().into_owned(),
        rev: shared.told(),
    })))
}

/// `POST /api/places/remove` `{root}`: an added place leaves the list; its folder is left as it is.
pub(super) fn remove_place(shared: &Shared, req: PlacesRemoveReq) -> Out<Changed> {
    let Some(settings) = shared.settings.clone() else {
        return Err(fail::failure(
            "500 Internal Server Error",
            "no settings folder is set, so the change could not be kept",
        ));
    };
    let _one_at_a_time = lock(&shared.gate);
    let i = req.root;
    let roots = shared.roots();
    let Some(r) = roots.get(i) else {
        return Err(refuse("404 Not Found", "no such place"));
    };
    if r.role != Role::Added {
        return Err(refuse(
            "403 Forbidden",
            format!("{} is not a folder you added: it stays", r.label),
        ));
    }
    let kept: Vec<PathBuf> = roots
        .iter()
        .enumerate()
        .filter(|(j, r)| *j != i && r.role == Role::Added)
        .filter_map(|(_, r)| r.dir().map(Path::to_path_buf))
        .collect();
    if let Err(e) = save_places(shared.storage.as_ref(), &settings, &kept) {
        return Err(fail::failure("500 Internal Server Error", e));
    }
    shared
        .owner
        .ask(|r| super::owner::Command::RemoveRoot(i, r));
    announce(shared);
    Ok(Changed { rev: shared.told() })
}

// ---- folders and tiles: the service's calls (dt_service::folder_calls), as this program's replies -------------------

pub(super) fn new_folder(shared: &Shared, req: FolderNamedReq) -> Out<FolderDone> {
    super::api::out(dt_service::folder_calls::new_folder(shared, req))
}

pub(super) fn rename_folder(shared: &Shared, req: FolderNamedReq) -> Out<FolderDone> {
    super::api::out(dt_service::folder_calls::rename_folder(shared, req))
}

pub(super) fn trash_folder(shared: &Shared, req: FolderReq) -> Out<Trashed> {
    super::api::out(dt_service::folder_calls::trash_folder(shared, req))
}

pub(super) fn trash_tile(shared: &Shared, tile: &super::api::TileRef, n: Nothing) -> Out<Trashed> {
    super::api::out(dt_service::folder_calls::trash_tile(shared, tile, n))
}

pub(super) fn undo_trash(shared: &Shared, req: TrashUndoReq) -> Out<Restored> {
    super::api::out(dt_service::folder_calls::undo_trash(shared, req))
}

pub(super) fn move_tile(
    shared: &Shared,
    tile: &super::api::TileRef,
    req: TileMoveReq,
) -> Out<ListedIn> {
    super::api::out(dt_service::folder_calls::move_tile(shared, tile, req))
}

pub(super) fn duplicate_tile(
    shared: &Shared,
    tile: &super::api::TileRef,
    n: Nothing,
) -> Out<ListedIn> {
    super::api::out(dt_service::folder_calls::duplicate_tile(shared, tile, n))
}

#[cfg(test)]
mod tests;
