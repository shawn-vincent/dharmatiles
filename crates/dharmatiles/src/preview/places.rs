//! Places: the folders the page lists tiles from, and what it may do in them.
//!
//! A place is a root ([`Root`]): My tiles, a folder named on the command line,
//! a folder the user added with the Mac's folder chooser, the library, the
//! tests.  The page names a place by its index in the snapshot and a folder
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

use std::path::{Component, Path, PathBuf};

use super::edit::bad;
use super::roots::kind_of;
use super::*;

type Reply = (&'static str, Value);

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
pub fn load_places(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let file = places_file(dir);
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", file.display())),
    };
    let v: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "{} is not valid: {e}; fix it or move it aside",
            file.display()
        )
    })?;
    let list = v["places"].as_array().ok_or_else(|| {
        format!(
            "{} has no \"places\" list; fix it or move it aside",
            file.display()
        )
    })?;
    list.iter()
        .map(|p| {
            p.as_str().map(PathBuf::from).ok_or_else(|| {
                format!(
                    "{}: every place must be a path; fix it or move it aside",
                    file.display()
                )
            })
        })
        .collect()
}

/// Keep `places` as the added folders.
///
/// # Errors
/// The settings folder or file cannot be written.
pub fn save_places(dir: &Path, places: &[PathBuf]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    let file = places_file(dir);
    let body = json!({ "places": places.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>() });
    let text = serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?;
    // written beside, then renamed over: a crash leaves the old list, never half of one
    let tmp = dir.join("places.json.tmp");
    std::fs::write(&tmp, format!("{text}\n"))
        .and_then(|()| std::fs::rename(&tmp, &file))
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

/// `dir` as a list of names below a place: no empty, `.`, `..`, hidden, build-output or
/// odd-character names.  `""` is the place itself.
fn components(dir: &str) -> Result<Vec<&str>, Reply> {
    if dir.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for c in dir.split('/') {
        let odd = c.is_empty()
            || c.starts_with('.')
            || c.contains(['\\', ':'])
            || c.chars().any(char::is_control)
            || roots::SKIP_DIRS.contains(&c);
        if odd {
            return Err(refuse(
                "400 Bad Request",
                "a folder is named by the names below its place, one by one",
            ));
        }
        out.push(c);
    }
    Ok(out)
}

/// Is `path` (made of existing folders up to its last name) under `base`, by
/// their real paths: a symlink that leaves the place leaves it here too.
fn really_inside(path: &Path, base: &Path) -> bool {
    match (std::fs::canonicalize(path), std::fs::canonicalize(base)) {
        (Ok(p), Ok(b)) => p.starts_with(b),
        _ => false,
    }
}

/// Place `root` as a folder place the user owns: its index checked, its folder on disk.
fn place(shared: &Shared, root: &Value) -> Result<(usize, PathBuf), Reply> {
    let Some(i) = root.as_u64().and_then(|i| usize::try_from(i).ok()) else {
        return Err(bad("name the place by its number"));
    };
    let roots = shared.roots();
    let Some(r) = roots.get(i) else {
        return Err(refuse("404 Not Found", "no such place"));
    };
    if !r.writable() {
        return Err(refuse(
            "403 Forbidden",
            format!("{} is read only: its tiles are not changed here", r.label),
        ));
    }
    let Some(d) = r.dir() else {
        return Err(bad("that place is not a folder"));
    };
    Ok((i, d.to_path_buf()))
}

/// Folder `dir` below place `root`: `(place index, the place's folder, the folder)`.  `must_exist`
/// is for a folder acted on; a destination is the same.  The real path is checked to stay inside.
fn folder(shared: &Shared, root: &Value, dir: &Value) -> Result<(usize, PathBuf, PathBuf), Reply> {
    let (i, base) = place(shared, root)?;
    let names = components(dir.as_str().ok_or_else(|| bad("give the folder"))?)?;
    let full = names.iter().fold(base.clone(), |p, n| p.join(n));
    if !full.is_dir() {
        return Err(refuse("404 Not Found", "no such folder"));
    }
    if !really_inside(&full, &base) {
        return Err(refuse(
            "403 Forbidden",
            "that folder is outside its place (a link leads out of it)",
        ));
    }
    Ok((i, base, full))
}

/// A tile file of a place the user owns: `(place index, its path)`.  Not the
/// library, not a path with `..`, only a kind the actions apply to, and its
/// folder is really inside its place.
fn tile_file(shared: &Shared, tile: &str, what: &str) -> Result<(usize, PathBuf), Reply> {
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
    let roots = shared.roots();
    let found = roots.iter().enumerate().find(|(_, r)| {
        r.writable()
            && r.dir().is_some_and(|d| {
                path.starts_with(d) && path.parent().is_some_and(|p| really_inside(p, d))
            })
    });
    match found {
        Some((i, _)) => Ok((i, path)),
        None => Err(bad(format!(
            "only a tile in a folder you own is {what} here"
        ))),
    }
}

/// The checks of a tile that is renamed or removed (the calls of `edit`): the place it is in.
pub(super) fn owned_tile(shared: &Shared, tile: &str, what: &str) -> Result<PathBuf, Reply> {
    tile_file(shared, tile, what).map(|(_, p)| p)
}

/// One name of a file or folder from the page: one line of at most 80 characters, with what a
/// file name cannot hold turned into `-`, and no leading dot.
fn leaf_name(v: &Value, what: &str) -> Result<String, Reply> {
    let name = v["name"].as_str().unwrap_or("").trim();
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
    if roots::SKIP_DIRS.contains(&out.as_str()) {
        return Err(bad(format!(
            "“{out}” is not listed (it holds build output): pick another name"
        )));
    }
    Ok(out)
}

// ---- the places -----------------------------------------------------------------------------

/// Tell every listener the list changed, with its snapshot.
pub(super) fn announce(shared: &Shared) {
    let tiles = lock(&shared.tiles);
    let mut ev = lock(&shared.events);
    let rev = ev.bump(Change::List);
    let snap = shared.snapshot_of(&tiles, rev);
    ev.send(&snap);
}

/// `POST /api/places/add`: show the chooser; the folder chosen is a place from now on.
pub(super) fn add_place(shared: &Shared) -> Reply {
    let Some(settings) = shared.settings.clone() else {
        return fail::failure(
            "500 Internal Server Error",
            "no settings folder is set, so an added folder could not be kept",
        );
    };
    let picked = match shared.picker.choose() {
        Ok(Some(p)) => p,
        // the user closed the chooser: nothing to report, and nothing changes
        Ok(None) => return ("200 OK", json!({ "cancelled": true })),
        Err(e) => return fail::failure("500 Internal Server Error", e),
    };
    let _one_at_a_time = lock(&shared.gate);
    let real = match std::fs::canonicalize(&picked) {
        Ok(r) if r.is_dir() => r,
        Ok(r) => {
            return refuse(
                "400 Bad Request",
                format!("{} is not a folder", r.display()),
            );
        }
        Err(e) => return io_failure("read", &picked, &e),
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
            return refuse(
                "409 Conflict",
                format!("{} is already in {label}", real.display()),
            );
        }
        if other.starts_with(&real) {
            return refuse(
                "409 Conflict",
                format!(
                    "{} holds {label}: add a folder that does not",
                    real.display()
                ),
            );
        }
    }
    let mut kept = match load_places(&settings) {
        Ok(k) => k,
        Err(e) => return fail::failure("500 Internal Server Error", e),
    };
    kept.push(real.clone());
    if let Err(e) = save_places(&settings, &kept) {
        return fail::failure("500 Internal Server Error", e);
    }
    let root = Root::new(&label_of(&real), RootKind::Dir(real.clone())).with_role(Role::Added);
    let (index, label) = {
        let mut roots = lock(&shared.roots);
        roots.push(root);
        (roots.len() - 1, label_of(&real))
    };
    shared.rescan_held();
    announce(shared);
    (
        "200 OK",
        json!({ "added": true, "root": index, "label": label, "path": real.to_string_lossy(), "snapshot": shared.snapshot() }),
    )
}

/// `POST /api/places/remove` `{root}`: an added place leaves the list; its folder is left as it is.
pub(super) fn remove_place(shared: &Shared, v: &Value) -> Reply {
    let Some(settings) = shared.settings.clone() else {
        return fail::failure(
            "500 Internal Server Error",
            "no settings folder is set, so the change could not be kept",
        );
    };
    let _one_at_a_time = lock(&shared.gate);
    let Some(i) = v["root"].as_u64().and_then(|i| usize::try_from(i).ok()) else {
        return bad("name the place by its number");
    };
    let roots = shared.roots();
    let Some(r) = roots.get(i) else {
        return refuse("404 Not Found", "no such place");
    };
    if r.role != Role::Added {
        return refuse(
            "403 Forbidden",
            format!("{} is not a folder you added: it stays", r.label),
        );
    }
    let kept: Vec<PathBuf> = roots
        .iter()
        .enumerate()
        .filter(|(j, r)| *j != i && r.role == Role::Added)
        .filter_map(|(_, r)| r.dir().map(Path::to_path_buf))
        .collect();
    if let Err(e) = save_places(&settings, &kept) {
        return fail::failure("500 Internal Server Error", e);
    }
    let gone: Vec<TileId> = {
        let mut tiles = lock(&shared.tiles);
        let mut gone = Vec::new();
        tiles.retain(|t| {
            let here = t.root == i;
            if here {
                gone.push(t.id.clone());
            }
            !here
        });
        for t in tiles.iter_mut().filter(|t| t.root > i) {
            t.root -= 1;
        }
        lock(&shared.roots).remove(i);
        let mut folders = lock(&shared.folders);
        *folders = std::mem::take(&mut *folders)
            .into_iter()
            .filter(|(r, _)| *r != i)
            .map(|(r, v)| (if r > i { r - 1 } else { r }, v))
            .collect();
        gone
    };
    for id in &gone {
        shared.forget(id);
        lock(&shared.events).messages.forget(id);
    }
    announce(shared);
    (
        "200 OK",
        json!({ "ok": true, "snapshot": shared.snapshot() }),
    )
}

// ---- folders ------------------------------------------------------------------------------------

/// A folder made, renamed or trashed is in the list at once: the answer says so.
fn folder_done(shared: &Shared, extra: Value) -> Reply {
    shared.rescan();
    let mut body = json!({ "ok": true, "snapshot": shared.snapshot() });
    if let (Some(b), Value::Object(x)) = (body.as_object_mut(), extra) {
        b.extend(x);
    }
    ("200 OK", body)
}

/// `POST /api/folders/new` `{root, dir, name}`.
pub(super) fn new_folder(shared: &Shared, v: &Value) -> Reply {
    let (_, _, parent) = match folder(shared, &v["root"], &v["dir"]) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let name = match leaf_name(v, "folder") {
        Ok(n) => n,
        Err(e) => return e,
    };
    let to = parent.join(&name);
    // `create_dir` never goes over what is there
    match std::fs::create_dir(&to) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return refuse(
                "409 Conflict",
                format!("“{name}” is already in this folder"),
            );
        }
        Err(e) => return io_failure("make", &to, &e),
    }
    let rel = join_rel(v["dir"].as_str().unwrap_or(""), &name);
    folder_done(shared, json!({ "dir": rel }))
}

fn join_rel(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{dir}/{name}")
    }
}

/// `POST /api/folders/rename` `{root, dir, name}`: `dir` (not the place itself) gets a new last name.
pub(super) fn rename_folder(shared: &Shared, v: &Value) -> Reply {
    let (_, base, from) = match folder(shared, &v["root"], &v["dir"]) {
        Ok(f) => f,
        Err(e) => return e,
    };
    if from == base {
        return bad("a place is not renamed here: only a folder in it");
    }
    let name = match leaf_name(v, "folder") {
        Ok(n) => n,
        Err(e) => return e,
    };
    let Some(parent) = from.parent() else {
        return bad("the folder has no parent");
    };
    let to = parent.join(&name);
    if to != from && std::fs::symlink_metadata(&to).is_ok() {
        // a change of case only, on a disk that ignores case, is the same folder
        let same = matches!((std::fs::canonicalize(&from), std::fs::canonicalize(&to)), (Ok(a), Ok(b)) if a == b);
        if !same {
            return refuse(
                "409 Conflict",
                format!("“{name}” is already in this folder"),
            );
        }
    }
    if let Err(e) = std::fs::rename(&from, &to) {
        return io_failure("rename", &from, &e);
    }
    // the prints of the tiles in it follow them
    super::inventory::note_folder_rename(shared, &from, &to);
    let dir = v["dir"].as_str().unwrap_or("");
    let rel = match dir.rsplit_once('/') {
        Some((up, _)) => join_rel(up, &name),
        None => name,
    };
    folder_done(shared, json!({ "dir": rel }))
}

/// Move `from` into the Trash under a name that is free there; where it went.
fn into_trash(shared: &Shared, from: &Path) -> Result<PathBuf, Reply> {
    let Some(trash) = &shared.trash else {
        return Err(fail::failure(
            "500 Internal Server Error",
            "there is no Trash known on this system, so nothing was moved (a delete is never a hard delete)",
        ));
    };
    if let Err(e) = std::fs::create_dir_all(trash) {
        return Err(io_failure("make", trash, &e));
    }
    let name = from
        .file_name()
        .map_or_else(|| "item".to_owned(), |n| n.to_string_lossy().into_owned());
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && from.is_file() => (s.to_owned(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    for n in 1..1000 {
        let to = trash.join(if n == 1 {
            name.clone()
        } else {
            format!("{stem} {n}{ext}")
        });
        if std::fs::symlink_metadata(&to).is_ok() {
            continue;
        }
        return match std::fs::rename(from, &to) {
            Ok(()) => Ok(to),
            Err(e) => Err(fail::failure_from(
                "500 Internal Server Error",
                format!(
                    "cannot move {} to the Trash ({}): {e}; nothing was deleted",
                    from.display(),
                    trash.display()
                ),
                &e,
            )),
        };
    }
    Err(fail::failure(
        "500 Internal Server Error",
        "no free name in the Trash",
    ))
}

/// Trash `from` and answer with the number its Undo goes by.
fn trashed(shared: &Shared, from: &Path, extra: Value) -> Reply {
    let to = match into_trash(shared, from) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let undo = {
        let mut kept = lock(&shared.trashed);
        kept.push((from.to_path_buf(), to));
        kept.len() - 1
    };
    let mut extra = extra;
    if let Some(o) = extra.as_object_mut() {
        o.insert("undo".to_owned(), json!(undo));
    }
    folder_done(shared, extra)
}

/// `POST /api/folders/trash` `{root, dir}`.
pub(super) fn trash_folder(shared: &Shared, v: &Value) -> Reply {
    let (_, base, from) = match folder(shared, &v["root"], &v["dir"]) {
        Ok(f) => f,
        Err(e) => return e,
    };
    if from == base {
        return bad("a place is taken off the list, not trashed: only a folder in it is");
    }
    trashed(shared, &from, json!({ "dir": v["dir"] }))
}

/// `POST /api/tiles/ID/trash`.
pub(super) fn trash_tile(shared: &Shared, tile: &str) -> Reply {
    let (_, path) = match tile_file(shared, tile, "moved to the Trash") {
        Ok(f) => f,
        Err(e) => return e,
    };
    if !path.is_file() {
        return refuse("404 Not Found", "the tile's file is not there");
    }
    trashed(shared, &path, json!({ "path": tile }))
}

/// `POST /api/trash/undo` `{undo}`: what was trashed goes back where it was, when that place is free.
pub(super) fn undo_trash(shared: &Shared, v: &Value) -> Reply {
    let Some(n) = v["undo"].as_u64().and_then(|n| usize::try_from(n).ok()) else {
        return bad("give the number the Trash answered");
    };
    let entry = lock(&shared.trashed).get(n).cloned();
    let Some((from, to)) = entry else {
        return refuse("404 Not Found", "nothing was trashed under that number");
    };
    // the place it came from must still be one the user owns
    let roots = shared.roots();
    let inside = roots.iter().any(|r| {
        r.writable()
            && r.dir().is_some_and(|d| {
                from.starts_with(d) && from.parent().is_some_and(|p| really_inside(p, d))
            })
    });
    if !inside {
        return refuse(
            "409 Conflict",
            "the folder it came from is not there any more: take it out of the Trash yourself",
        );
    }
    if std::fs::symlink_metadata(&from).is_ok() {
        return refuse(
            "409 Conflict",
            format!("{} is there again: it is left in the Trash", from.display()),
        );
    }
    if let Err(e) = std::fs::rename(&to, &from) {
        return io_failure("put back", &to, &e);
    }
    let path = from.to_string_lossy().into_owned();
    let is_tile = from.is_file();
    shared.rescan();
    let id = tile_id(&path);
    let tile = if is_tile { shared.summary(&id) } else { None };
    (
        "200 OK",
        json!({ "ok": true, "path": path, "id": tile.as_ref().map(|_| id), "tile": tile, "snapshot": shared.snapshot() }),
    )
}

// ---- tiles ----------------------------------------------------------------------------------------

/// A file appears at `dest`: listed, its build asked for, and its state answered with the list.
fn listed_with_snapshot(shared: &Shared, dest: &Path) -> Reply {
    let (status, mut body) = super::edit::listed(shared, dest);
    if let Some(b) = body.as_object_mut() {
        b.insert("snapshot".to_owned(), shared.snapshot());
    }
    (status, body)
}

/// Put `from` at `to` without ever going over a file there: a link first (which refuses a name
/// that is taken), then the old name removed; across disks, a copy created new.
pub(super) fn move_no_clobber(from: &Path, to: &Path, clash: &str) -> Result<(), Reply> {
    use std::io::Write;
    match std::fs::hard_link(from, to) {
        Ok(()) => std::fs::remove_file(from).map_err(|e| {
            // best-effort cleanup of the new link; the failure to remove the old name is what is reported
            let _ = std::fs::remove_file(to);
            io_failure("move", from, &e)
        }),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(refuse("409 Conflict", clash.to_owned()))
        }
        Err(link) => {
            // another disk (or one that does not link): copy, created new, and only then the old one removed
            let text = std::fs::read(from).map_err(|e| io_failure("read", from, &e))?;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(to)
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        refuse("409 Conflict", clash.to_owned())
                    } else {
                        io_failure("write", to, &e)
                    }
                })?;
            if let Err(e) = f.write_all(&text) {
                // best-effort cleanup of the half-made copy; the write error is what is reported
                let _ = std::fs::remove_file(to);
                return Err(io_failure("write", to, &e));
            }
            std::fs::remove_file(from).map_err(|e| {
                io_failure(
                    &format!("remove (after a copy; linking said {link}),"),
                    from,
                    &e,
                )
            })
        }
    }
}

/// `POST /api/tiles/ID/move` `{root, dir}`: the tile's file goes into that folder, under the same name.
pub(super) fn move_tile(shared: &Shared, tile: &str, v: &Value) -> Reply {
    let (_, from) = match tile_file(shared, tile, "moved") {
        Ok(f) => f,
        Err(e) => return e,
    };
    let (_, _, dest_dir) = match folder(shared, &v["root"], &v["dir"]) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let Some(name) = from.file_name() else {
        return bad("the tile has no name");
    };
    let to = dest_dir.join(name);
    if to == from {
        return listed_with_snapshot(shared, &to);
    }
    let clash = format!(
        "a tile called “{}” is already in that folder",
        from.file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
    );
    match move_no_clobber(&from, &to, &clash) {
        Ok(()) => {
            // its prints in the inventory follow it
            super::inventory::note_rename(shared, &from, &to);
            listed_with_snapshot(shared, &to)
        }
        Err(e) => e,
    }
}

/// `POST /api/tiles/ID/duplicate`: a copy beside the tile, “Name copy”, never over a file.
pub(super) fn duplicate_tile(shared: &Shared, tile: &str) -> Reply {
    let (_, from) = match tile_file(shared, tile, "copied") {
        Ok(f) => f,
        Err(e) => return e,
    };
    let text = match std::fs::read(&from) {
        Ok(t) => t,
        Err(e) => return io_failure("read", &from, &e),
    };
    let Some(dir) = from.parent() else {
        return bad("the tile has no folder");
    };
    let stem = from
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    match super::edit::write_new_bytes(dir, &format!("{stem} copy"), &text) {
        Ok(dest) => listed_with_snapshot(shared, &dest),
        Err(e) => e,
    }
}

/// Where a new tile goes: folder `dir` of place `root` when both are given, else My tiles.
pub(super) fn new_tile_dir(shared: &Shared, v: &Value) -> Option<Result<PathBuf, Reply>> {
    if v["root"].is_null() {
        return None;
    }
    Some(folder(shared, &v["root"], &v["dir"]).map(|(_, _, f)| f))
}

#[cfg(test)]
mod tests;
