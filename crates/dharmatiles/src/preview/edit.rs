//! The editor's calls (`POST`, with this run's token; see `server`):
//!
//! - `/api/edit` `{text, at, name, value}`: set a control on the node whose
//!   name starts at byte `at`, in whichever form it is written, keeping
//!   everything else byte for byte; `value: null` takes it out.  With
//!   `op: insert` `{place: stack|tile, node}` it writes a new node (at the
//!   end of the stack, or before it) and says at which byte; with
//!   `op: remove` it takes out the node at `at`.  Nothing is written to
//!   disk: the page saves the result.
//! - `/api/tiles/N/check` `{text, file?}`: read and plan the tile with
//!   `text` in place of its document (or of `file`, one it uses): the
//!   errors, the stack and the map (see `map`), under a second, for the
//!   text pane and the overlay as you type.
//! - `/api/tiles/N/save` `{text, base, file?}`: write the document (or a
//!   file it uses) when what is on disk is still `base`; else `409` with
//!   what is there.  Library files are never written.  The worker sees the
//!   new file and rebuilds.
//! - `/api/tiles/N/mine`: copy a library tile into My tiles, its `use`s
//!   pointing back at the library, and return the copy's index.
//! - `/api/tiles/N/rename` `{name}`: rename the tile's file in its folder
//!   (a tile's file name is its name), never over another file, and
//!   return the new path and index.  Library tiles are not renamed.
//! - `/api/new` `{name, size}`: make a flat, printable tile called `name`
//!   in My tiles, never over another file.
//! - `/api/tiles/N/draft` `{text, file?, id}`: build a draft of the tile
//!   from `text` (see `dt_eval::Options::draft`), on a thread of its own so
//!   a full build never holds it up.  Only the latest asked for is built;
//!   when it is ready the tile's `draft_id` says so, and
//!   `GET /api/tiles/N/mesh?draft=1` serves it.  Nothing is written.

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};

use dt_eval::Source;

use super::roots::personal_home;
use super::*;

/// 128 random bits, hex: the run's token.
pub(super) fn token() -> String {
    let word = |salt: u64| {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u128(
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        h.write_u32(std::process::id());
        h.finish()
    };
    format!("{:016x}{:016x}", word(1), word(2))
}

/// The text of a file, overlaid on the disk: the document being edited.
struct Overlay(BTreeMap<String, String>);

impl Source for Overlay {
    fn read(&self, path: &str) -> Result<String, String> {
        self.0
            .get(path)
            .cloned()
            .map_or_else(|| dt_eval::Files.read(path), Ok)
    }
}

fn bad(msg: impl Into<String>) -> (&'static str, Value) {
    ("400 Bad Request", json!({ "error": msg.into() }))
}

/// Route one editor call.
pub(super) fn post(shared: &Shared, path: &str, body: &[u8]) -> (&'static str, Value) {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return bad("the body is not JSON");
    };
    if path == "/api/edit" {
        return edit(&v);
    }
    if path == "/api/new" {
        return new_tile(shared, &v);
    }
    if path == "/api/reveal" {
        return reveal(&v);
    }
    let Some((i, what)) = path
        .strip_prefix("/api/tiles/")
        .and_then(|r| r.split_once('/'))
        .and_then(|(n, w)| Some((n.parse::<usize>().ok()?, w)))
    else {
        return ("404 Not Found", json!({ "error": "no such call" }));
    };
    let Some((tile, files)) = lock(&shared.tiles)
        .get(i)
        .filter(|t| !t.gone)
        .map(|t| (t.path.clone(), t.files.clone()))
    else {
        return ("404 Not Found", json!({ "error": "no such tile" }));
    };
    // only the tile's document, or a file its last build read
    let file = match v["file"].as_str() {
        None => tile.clone(),
        Some(f) if f == tile || files.contains_key(f) => f.to_owned(),
        Some(_) => {
            return (
                "403 Forbidden",
                json!({ "error": "not a file of this tile" }),
            );
        }
    };
    match what {
        "check" => check(shared, &tile, &file, &v),
        "save" => save(&file, &v),
        "mine" => mine(shared, &tile, &files),
        "rename" => rename(shared, &tile, &v),
        "export" => export(shared, &tile, &v),
        "draft" => {
            let (Some(text), Some(id)) = (v["text"].as_str(), v["id"].as_u64()) else {
                return bad("give the text and an id");
            };
            *lock(&shared.draft_req) = Some(DraftRequest {
                tile: i,
                path: tile,
                file,
                text: text.to_owned(),
                id,
            });
            shared.draft_cv.notify_one();
            ("202 Accepted", json!({ "ok": true }))
        }
        _ => ("404 Not Found", json!({ "error": "no such call" })),
    }
}

/// The file stem for a tile name: the name as written, with only what a
/// file name cannot hold (path separators, `:`, control characters)
/// turned into `-`, and no leading dot.  A tile's file name is its name.
fn file_stem(name: &str) -> String {
    let out: String = name
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let out = out.trim_start_matches('.').trim();
    if out.is_empty() {
        "Untitled tile".to_owned()
    } else {
        out.to_owned()
    }
}

/// A tile name from the page: one line of at most 80 characters.
fn tile_name(v: &Value) -> Result<String, (&'static str, Value)> {
    let name = v["name"].as_str().unwrap_or("").trim();
    if name.chars().count() > 80 || name.contains(['\n', '\r']) {
        return Err(bad("a tile name is one line of at most 80 characters"));
    }
    Ok(file_stem(name))
}

/// Write `text` as `<stem>.dtile` in `dir`, or `<stem> 2.dtile`, … when
/// that is taken: created new, so another file is never overwritten.
fn write_new(dir: &Path, stem: &str, text: &str) -> Result<PathBuf, (&'static str, Value)> {
    use std::io::Write;
    for n in 1..1000 {
        let dest = dir.join(if n == 1 {
            format!("{stem}.dtile")
        } else {
            format!("{stem} {n}.dtile")
        });
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(mut f) => {
                return f
                    .write_all(text.as_bytes())
                    .map(|()| dest.clone())
                    .map_err(|e| {
                        let _ = std::fs::remove_file(&dest);
                        (
                            "500 Internal Server Error",
                            json!({ "error": format!("cannot write {}: {e}", dest.display()) }),
                        )
                    });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err((
                    "500 Internal Server Error",
                    json!({ "error": format!("cannot write {}: {e}", dest.display()) }),
                ));
            }
        }
    }
    Err((
        "500 Internal Server Error",
        json!({ "error": "no free name" }),
    ))
}

/// After a file appears or moves: list it, start its build, say where.
fn listed(shared: &Shared, dest: &Path) -> (&'static str, Value) {
    if shared.rescan() {
        shared.broadcast(&shared.snapshot());
    }
    let path = dest.to_string_lossy().into_owned();
    let index = lock(&shared.tiles)
        .iter()
        .position(|t| t.path == path && !t.gone);
    if let Some(i) = index {
        shared.want(i);
    }
    ("200 OK", json!({ "path": path, "index": index }))
}

fn home(shared: &Shared) -> Result<PathBuf, (&'static str, Value)> {
    let home = shared
        .roots
        .iter()
        .find_map(|r| match &r.kind {
            RootKind::Dir(d) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_else(personal_home);
    std::fs::create_dir_all(&home).map_err(|e| {
        (
            "500 Internal Server Error",
            json!({ "error": format!("cannot make {}: {e}", home.display()) }),
        )
    })?;
    Ok(home)
}

/// Rename a tile's file in its folder.  Never over another file: a link
/// to the new name is made first (which fails if the name is taken), then
/// the old name removed.  Only a change of case, on a disk that ignores
/// case, renames in place.
fn rename(shared: &Shared, tile: &str, v: &Value) -> (&'static str, Value) {
    if tile.starts_with(dt_eval::LIB) {
        return bad("a library tile is not renamed; make it yours first");
    }
    let from = PathBuf::from(tile);
    if from.extension().and_then(|e| e.to_str()) != Some("dtile") {
        return bad("only a tile document is renamed");
    }
    let in_folder = shared.roots.iter().any(|r| match &r.kind {
        RootKind::Dir(d) => from.starts_with(d),
        _ => false,
    });
    if !in_folder {
        return bad("only a tile in a folder the preview lists is renamed here");
    }
    let stem = match tile_name(v) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let Some(dir) = from.parent() else {
        return bad("the tile has no folder");
    };
    let to = dir.join(format!("{stem}.dtile"));
    if to == from {
        return listed(shared, &to);
    }
    match std::fs::hard_link(&from, &to) {
        Ok(()) => {
            if let Err(e) = std::fs::remove_file(&from) {
                let _ = std::fs::remove_file(&to);
                return (
                    "500 Internal Server Error",
                    json!({ "error": format!("cannot rename {}: {e}", from.display()) }),
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let same = matches!(
                (std::fs::canonicalize(&from), std::fs::canonicalize(&to)),
                (Ok(a), Ok(b)) if a == b
            );
            if !same {
                return (
                    "409 Conflict",
                    json!({ "error": format!("a tile called “{stem}” is already in this folder") }),
                );
            }
            if let Err(e) = std::fs::rename(&from, &to) {
                return (
                    "500 Internal Server Error",
                    json!({ "error": format!("cannot rename {}: {e}", from.display()) }),
                );
            }
        }
        Err(e) => {
            return (
                "500 Internal Server Error",
                json!({ "error": format!("cannot rename {}: {e}", from.display()) }),
            );
        }
    }
    listed(shared, &to)
}

/// Make a small, valid tile in My tiles and return its slot. Starting from a
/// printable flat square gives the editor something real to select and change;
/// every later gesture remains ordinary `.dtile` text.
fn new_tile(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let size = v["size"].as_str().unwrap_or("1x1");
    if !matches!(size, "1x1" | "2x1" | "1x2" | "2x2") {
        return bad("size must be 1x1, 2x1, 1x2 or 2x2");
    }
    let stem = match tile_name(v) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let text = format!(
        "tile size=\"{size}\" seed=1 {{\n    about \"A new tile.\"\n    stack {{\n        ground level=edge\n    }}\n}}\n"
    );
    match home(shared).and_then(|h| write_new(&h, &stem, &text)) {
        Ok(dest) => listed(shared, &dest),
        Err(e) => e,
    }
}

/// A draft the page asked for.
pub(super) struct DraftRequest {
    tile: usize,
    path: String,
    file: String,
    text: String,
    id: u64,
}

/// A newer drag request makes the draft currently being computed worthless.
struct LatestDraft(Arc<Shared>);

impl dt_kit::Reporter for LatestDraft {
    fn report(&self, _event: &dt_kit::Event<'_>) {
        if lock(&self.0.draft_req).is_some() {
            std::panic::resume_unwind(Box::new(Superseded));
        }
    }
}

/// The draft thread: builds the latest draft asked for, one at a time.
pub(super) fn drafts(shared: Arc<Shared>, opts: Options) {
    let opts = Options {
        draft: true,
        ..opts
    };
    std::thread::spawn(move || {
        loop {
            let req = {
                let mut slot = lock(&shared.draft_req);
                while slot.is_none() {
                    slot = shared
                        .draft_cv
                        .wait(slot)
                        .unwrap_or_else(PoisonError::into_inner);
                }
                slot.take()
            };
            let Some(r) = req else { continue };
            let src = Overlay(BTreeMap::from([(r.file.clone(), r.text.clone())]));
            let progress = dt_eval::Progress::new(Arc::new(LatestDraft(Arc::clone(&shared))));
            let cache = shared.build_cache(&r.path, true);
            // the build and its encoding together: a panic in either must
            // not end the thread, or drafts stop for the rest of the run
            let made = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let b = dt_eval::build_cached(&src, &r.path, &opts, &progress, Some(&cache))?;
                Ok::<_, Vec<Diag>>(payload::from_draft(
                    &b,
                    opts.printer.overhang_deg,
                    &shared.palette(),
                ))
            }));
            let mesh = match made {
                Err(panic) if panic.is::<Superseded>() => continue,
                Err(_) => {
                    tell!("the draft of {} panicked (a bug in an element)", r.path);
                    continue;
                }
                // a mistake in the text: the check shows it, the last draft stays
                Ok(Err(_)) => continue,
                Ok(Ok(mesh)) => Arc::new(mesh),
            };
            // a newer request already waiting makes this one moot
            if lock(&shared.draft_req).is_some() {
                continue;
            }
            shared.update(r.tile, |t| {
                t.draft = Some(mesh);
                t.draft_id = r.id;
            });
        }
    });
}

/// A KDL value from JSON: whole numbers stay whole.
fn kdl_value(v: &Value) -> Option<kdl::KdlValue> {
    Some(match v {
        Value::Bool(b) => kdl::KdlValue::Bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => kdl::KdlValue::Integer(i128::from(i)),
            None => kdl::KdlValue::Float(n.as_f64()?),
        },
        Value::String(s) => kdl::KdlValue::String(s.clone()),
        _ => return None,
    })
}

fn edit(v: &Value) -> (&'static str, Value) {
    let Some(text) = v["text"].as_str() else {
        return bad("give the text");
    };
    let at = v["at"].as_u64().and_then(|a| usize::try_from(a).ok());
    let done = |r: Result<(String, usize), dt_doc::edit::EditError>| match r {
        Ok((t, at)) => ("200 OK", json!({ "text": t, "at": at })),
        Err(e) => (
            "422 Unprocessable Entity",
            json!({ "error": e.to_string() }),
        ),
    };
    match v["op"].as_str() {
        // a new node: {op: insert, place: stack|tile, node}
        Some("insert") => {
            let Some(node) = v["node"].as_str() else {
                return bad("give the node to insert");
            };
            let place = if v["place"].as_str() == Some("tile") {
                dt_doc::edit::Place::Tile
            } else {
                dt_doc::edit::Place::Stack
            };
            return done(dt_doc::edit::insert_node(text, place, node));
        }
        // a node out: {op: remove, at}
        Some("remove") => {
            let Some(at) = at else {
                return bad("give the node's byte");
            };
            return done(dt_doc::edit::remove_node_at(text, at).map(|t| (t, at)));
        }
        // a line or region renamed, with what names it: {op: rename, mark, old, new}
        Some("rename") => {
            let mark = match v["mark"].as_str() {
                Some("line") => dt_doc::edit::Mark::Line,
                Some("region") => dt_doc::edit::Mark::Region,
                _ => return bad("give mark: line or region"),
            };
            let (Some(old), Some(new)) = (v["old"].as_str(), v["new"].as_str()) else {
                return bad("give old and new");
            };
            return done(dt_doc::edit::rename_mark(text, mark, old, new.trim()).map(|t| (t, 0)));
        }
        Some("set") | None => {}
        Some(op) => return bad(format!("no edit called `{op}`")),
    }
    let (Some(text), Some(at), Some(name)) = (
        v["text"].as_str(),
        v["at"].as_u64().and_then(|a| usize::try_from(a).ok()),
        v["name"].as_str(),
    ) else {
        return bad("give text, at and name");
    };
    let out = if v["value"].is_null() {
        dt_doc::edit::remove_control_at(text, at, name)
    } else {
        match kdl_value(&v["value"]) {
            Some(k) => dt_doc::edit::set_control_at(text, at, name, k),
            None => return bad("a value is a number, a string or a switch"),
        }
    };
    match out {
        Ok(t) => ("200 OK", json!({ "text": t })),
        Err(e) => (
            "422 Unprocessable Entity",
            json!({ "error": e.to_string() }),
        ),
    }
}

fn check(shared: &Shared, tile: &str, file: &str, v: &Value) -> (&'static str, Value) {
    let Some(text) = v["text"].as_str() else {
        return bad("give the text");
    };
    let src = Overlay(BTreeMap::from([(file.to_owned(), text.to_owned())]));
    let opts = Options {
        system: shared.system.clone(),
        ..Options::default()
    };
    match dt_eval::check(&src, tile, &opts) {
        Ok(c) => {
            let stack = c.tile.as_ref().map(|t| stack::items(t, &shared.system));
            let own = src.read(tile).unwrap_or_default();
            (
                "200 OK",
                json!({ "ok": true, "errors": [], "stack": stack, "map": map::json(&c, &own) }),
            )
        }
        Err(errs) => (
            "200 OK",
            json!({ "ok": false, "errors": errors_json(&errs) }),
        ),
    }
}

/// May the editor write `file`?  Never the library, nor a read-only file.
pub(super) fn writable(file: &str) -> bool {
    !file.starts_with(dt_eval::LIB)
        && std::fs::metadata(file).is_ok_and(|m| m.is_file() && !m.permissions().readonly())
}

fn save(file: &str, v: &Value) -> (&'static str, Value) {
    let (Some(text), Some(base)) = (v["text"].as_str(), v["base"].as_str()) else {
        return bad("give the text and the base it was edited from");
    };
    if !writable(file) {
        return (
            "403 Forbidden",
            json!({ "error": "this file is not yours to write: make it yours first" }),
        );
    }
    let now = std::fs::read_to_string(file).unwrap_or_default();
    if now != base {
        return (
            "409 Conflict",
            json!({ "error": "the file changed on disk", "text": now }),
        );
    }
    if now == text {
        return ("200 OK", json!({ "ok": true }));
    }
    // a whole file or none: write beside it, then rename over it
    let tmp = format!("{file}.dt-save");
    match std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, file)) {
        Ok(()) => ("200 OK", json!({ "ok": true })),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            (
                "500 Internal Server Error",
                json!({ "error": format!("cannot write {file}: {e}") }),
            )
        }
    }
}

/// `use "parts/grass"` in a library tile names a library file; in a copy
/// it must say so (`use "lib:parts/grass"`).  The build's files say which.
fn pin_uses(text: &str, files: &BTreeMap<String, Option<SystemTime>>) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        let pinned = t.strip_prefix("use ").and_then(|rest| {
            let rest = rest.trim_start();
            let target = rest.strip_prefix('"')?.split('"').next()?;
            if target.starts_with(dt_eval::LIB) || target.starts_with('/') {
                return None;
            }
            let want = format!("/{}", target.trim_end_matches(".dtile"));
            let lib = files.keys().find(|f| {
                f.starts_with(dt_eval::LIB)
                    && (f.trim_end_matches(".dtile").ends_with(&want)
                        || f.trim_end_matches(".dtile")
                            == format!("{}{}", dt_eval::LIB, &want[1..]))
            })?;
            Some(line.replacen(
                &format!("\"{target}\""),
                &format!("\"{}\"", lib.trim_end_matches(".dtile")),
                1,
            ))
        });
        out.push_str(pinned.as_deref().unwrap_or(line));
    }
    out
}

fn mine(
    shared: &Shared,
    tile: &str,
    files: &BTreeMap<String, Option<SystemTime>>,
) -> (&'static str, Value) {
    if !tile.starts_with(dt_eval::LIB) {
        return bad("only a library tile is copied");
    }
    let Ok(text) = dt_eval::Files.read(tile) else {
        return (
            "404 Not Found",
            json!({ "error": format!("cannot read {tile}") }),
        );
    };
    let stem = tile
        .rsplit('/')
        .next()
        .unwrap_or("tile")
        .trim_end_matches(".dtile")
        .to_owned();
    match home(shared).and_then(|h| write_new(&h, &stem, &pin_uses(&text, files))) {
        Ok(dest) => listed(shared, &dest),
        Err(e) => e,
    }
}

/// Export: the same build as `dharmatiles build` (the saved file, through
/// the gate), written to [`export_dir`].  With `where`, only says where.  `system` is `db` or `ol`
/// (default: the preview's); `threemf` adds the painted 3MF.
fn export(shared: &Shared, tile: &str, v: &Value) -> (&'static str, Value) {
    let system = match v["system"].as_str() {
        None => shared.system.clone(),
        Some(s @ ("db" | "ol")) => s.to_owned(),
        Some(_) => return bad("system is db or ol"),
    };
    let threemf = v["threemf"].as_bool().unwrap_or(false);
    let template = if threemf {
        match crate::bambu::template(None) {
            Ok(t) => t,
            Err(e) => return ("500 Internal Server Error", json!({ "error": e })),
        }
    } else {
        None
    };
    let out = export_dir(tile);
    if v["where"].as_bool() == Some(true) {
        // shown with ~ for the home folder
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let shown = home
            .as_deref()
            .and_then(|h| out.strip_prefix(h).ok())
            .map_or_else(
                || out.display().to_string(),
                |r| format!("~/{}", r.display()),
            );
        return (
            "200 OK",
            json!({ "out": out.display().to_string(), "shown": shown }),
        );
    }
    let palette = shared.palette();
    let extras = crate::build::Extras {
        progress: false,
        tile_no: None,
        png: false,
        threemf,
        template: template.as_deref(),
        palette: &palette,
        cache: None,
    };
    let opts = crate::library::options(system.clone(), None);
    let t0 = std::time::Instant::now();
    let mut b = match dt_eval::build_with(&dt_eval::Files, tile, &opts, &dt_eval::Progress::none())
    {
        Ok(b) => b,
        Err(errs) => {
            return (
                "200 OK",
                json!({ "path": tile, "ok": false, "errors": crate::build::errors_json(&errs) }),
            );
        }
    };
    // a file of yours goes to out/<system>/<name>-<system>.stl, not under
    // the folders it sits in (a library tile mirrors the library, as build does)
    if !tile.starts_with("lib:") {
        let stem = Path::new(tile)
            .file_stem()
            .map_or_else(|| "tile".to_owned(), |s| s.to_string_lossy().into_owned());
        b.out = PathBuf::from(&system).join(format!("{stem}-{system}.stl"));
    }
    let built_s = t0.elapsed().as_secs_f64();
    let failures = b.audit.failures(&opts.printer);
    match crate::build::write(&b, &failures, built_s, &out, extras) {
        Ok(files) => {
            let mut r = crate::build::tile_json(&b, &failures, built_s);
            r["failures"] = json!(failures);
            r["files"] = json!(
                files
                    .iter()
                    .map(|f| f.display().to_string())
                    .collect::<Vec<_>>()
            );
            ("200 OK", r)
        }
        Err(e) => ("500 Internal Server Error", json!({ "error": e })),
    }
}

/// Where `tile` exports to: never the library.  A library tile (`lib:`,
/// or a file inside the library folder) goes to `out/` in My tiles; any
/// other file to its project's `out/` (beside the nearest
/// `dharmatiles.dtile` above it), else `out/` beside it.
pub(super) fn export_dir(tile: &str) -> PathBuf {
    let in_library = tile.starts_with("lib:")
        || dt_eval::source::library_dir()
            .and_then(|l| l.canonicalize().ok())
            .zip(Path::new(tile).canonicalize().ok())
            .is_some_and(|(l, t)| t.starts_with(l));
    if in_library {
        return personal_home().join("out");
    }
    let dir = Path::new(tile).parent().unwrap_or_else(|| Path::new("."));
    dt_eval::discover(dir).out
}

/// Show a written file in the Finder (macOS `open -R`; elsewhere the
/// folder is opened).  Only an STL or 3MF that exists.
fn reveal(v: &Value) -> (&'static str, Value) {
    let Some(p) = v["path"].as_str().map(PathBuf::from) else {
        return bad("give the path");
    };
    let kind = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !matches!(kind, "stl" | "3mf") || !p.is_file() {
        return bad("only a written STL or 3MF");
    }
    let r = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&p)
            .status()
    } else {
        let dir = p.parent().unwrap_or_else(|| Path::new("."));
        std::process::Command::new(if cfg!(windows) {
            "explorer"
        } else {
            "xdg-open"
        })
        .arg(dir)
        .status()
    };
    match r {
        Ok(_) => ("200 OK", json!({ "ok": true })),
        Err(e) => (
            "500 Internal Server Error",
            json!({ "error": e.to_string() }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_points_its_uses_at_the_library() {
        let files = BTreeMap::from([
            ("lib:tiles/water/a.dtile".to_owned(), None),
            ("lib:parts/grass.dtile".to_owned(), None),
        ]);
        let text = "// a\nuse \"parts/grass\" lawn\nuse \"lib:parts/trees\" oak\ntile {}\n";
        assert_eq!(
            pin_uses(text, &files),
            "// a\nuse \"lib:parts/grass\" lawn\nuse \"lib:parts/trees\" oak\ntile {}\n"
        );
    }

    #[test]
    fn a_new_tile_is_safe_named_valid_and_listed() {
        let dir = std::env::temp_dir().join(format!("dt-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
        let shared = Shared::with_roots(roots, &[], "db", 55.0, 65.0);
        let (status, made) = new_tile(
            &shared,
            &json!({ "name": "Shore / first try", "size": "2x1" }),
        );
        assert_eq!(status, "200 OK");
        let path = made["path"].as_str().unwrap();
        assert!(path.ends_with("Shore - first try.dtile"), "{path}");
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("tile size=\"2x1\""));
        assert!(text.contains("ground level=edge"));
        dt_eval::check(&dt_eval::Files, path, &Options::default()).unwrap();
        assert!(made["index"].is_number());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rename_moves_the_file_and_never_clobbers() {
        let dir = std::env::temp_dir().join(format!("dt-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
        let shared = Shared::with_roots(roots, &[], "db", 55.0, 65.0);
        let pond = new_tile(&shared, &json!({ "name": "Pond" })).1;
        let marsh = new_tile(&shared, &json!({ "name": "Marsh" })).1;
        let pond = pond["path"].as_str().unwrap().to_owned();
        let marsh = marsh["path"].as_str().unwrap().to_owned();

        let (status, moved) = rename(&shared, &pond, &json!({ "name": "Still lake" }));
        assert_eq!(status, "200 OK", "{moved}");
        let lake = moved["path"].as_str().unwrap().to_owned();
        assert!(lake.ends_with("Still lake.dtile"), "{lake}");
        assert!(!Path::new(&pond).exists());
        assert!(moved["index"].is_number());

        let (status, _) = rename(&shared, &marsh, &json!({ "name": "Still lake" }));
        assert_eq!(status, "409 Conflict");
        assert!(Path::new(&marsh).exists());

        let (status, _) = rename(
            &shared,
            "lib:tiles/test/1x1-flat.dtile",
            &json!({ "name": "x" }),
        );
        assert_eq!(status, "400 Bad Request");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
