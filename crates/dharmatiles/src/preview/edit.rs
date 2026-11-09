//! The editor's calls (`POST`, with this run's token; see `server`):
//!
//! - `/api/edit` `{text, at, name, value}`: set a control on the node whose
//!   name starts at byte `at`, in whichever form it is written, keeping
//!   everything else byte for byte; `value: null` takes it out.  With
//!   `op: insert` `{place: stack|tile, node}` it writes a new node (at the
//!   end of the stack, or before it) and says at which byte; with
//!   `op: remove` it takes out the node at `at`.  Nothing is written to
//!   disk: the page saves the result.
//! - `/api/check-value` `{element, control, typed}`: what the reader would
//!   refuse in `typed` for that control (`{message, label, help}`, or
//!   `{message: null}`), by the same check as a written value, so the sheet
//!   draws the program's answer and checks nothing itself.
//! - `/api/tiles/ID/check` `{text, file?}`: read and plan the tile with
//!   `text` in place of its document (or of `file`, one it uses): the
//!   errors, the stack and the map (see `map`), under a second, for the
//!   text pane and the overlay as you type.
//! - `/api/tiles/ID/save` `{text, base, file?}`: write the document (or a
//!   file it uses) when what is on disk is still `base`; else `409` with
//!   what is there.  Library files are never written.  The worker sees the
//!   new file and rebuilds.
//! - `/api/tiles/ID/mine`: copy a library tile into My tiles, its `use`s
//!   pointing back at the library, and return the copy's id and state.
//! - `/api/tiles/ID/rename` `{name}`: rename the tile's file in its folder
//!   (a tile's file name is its name), never over another file, and
//!   return the new path, id and state (a new path is a new id).  Library
//!   tiles are not renamed.
//! - `/api/new` `{name, size}`: make a flat, printable tile called `name`
//!   in My tiles, never over another file.
//! - `/api/tiles/ID/draft` `{text, file?}`: build a draft of the tile from
//!   `text` (see `dt_eval::Options::draft`) ahead of any full build, and
//!   answer with its mesh: `200` and the binary buffer, `422` when the text
//!   has a mistake, `409` when a newer draft of the tile took its place.
//!   Nothing is written.

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};

use dt_eval::Source;

use super::queue::Outcome;
use super::roots::personal_home;
use super::route::{Reply, tile_route};
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
pub(super) struct Overlay(pub BTreeMap<String, String>);

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
pub(super) fn post(shared: &Shared, path: &str, body: &[u8]) -> Reply {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return bad("the body is not JSON").into();
    };
    if path == "/api/edit" {
        return edit(&v).into();
    }
    if path == "/api/check-value" {
        return check_value(&v).into();
    }
    if path == "/api/new" {
        return new_tile(shared, &v).into();
    }
    if path == "/api/reveal" {
        return reveal(&v).into();
    }
    if path == "/api/target" {
        return target(shared, &v).into();
    }
    let Some((id, what)) = tile_route(path) else {
        return ("404 Not Found", json!({ "error": "no such call" })).into();
    };
    let Some((tile, files)) = lock(&shared.tiles)
        .iter()
        .find(|t| t.id == id)
        .map(|t| (t.path.clone(), t.files.clone()))
    else {
        return ("404 Not Found", json!({ "error": "no such tile" })).into();
    };
    // only the tile's document, or a file its last build read
    let file = match v["file"].as_str() {
        None => tile.clone(),
        Some(f) if f == tile || files.contains_key(f) => f.to_owned(),
        Some(_) => {
            return (
                "403 Forbidden",
                json!({ "error": "not a file of this tile" }),
            )
                .into();
        }
    };
    match what {
        "check" => check(shared, &tile, &file, &v).into(),
        "save" => save(&file, &v).into(),
        "mine" => mine(shared, &tile, &files).into(),
        "rename" => rename(shared, &tile, &v).into(),
        "export" => export(shared, &tile, &v).into(),
        "draft" => draft(shared, id, &tile, &file, &v),
        _ => ("404 Not Found", json!({ "error": "no such call" })).into(),
    }
}

/// Choose the build target: `system` (`db` or `ol`) and/or `printer` (a
/// profile's name), for every build from now on; the tiles built so far
/// are built again.
fn target(shared: &Shared, v: &Value) -> (&'static str, Value) {
    match shared.set_target(v["system"].as_str(), v["printer"].as_str()) {
        Ok(()) => {
            let o = shared.options();
            (
                "200 OK",
                json!({ "ok": true, "system": o.system, "printer": o.printer.name }),
            )
        }
        Err(e) => bad(&e),
    }
}

/// A draft: queued ahead of the full builds and waited for, so the reply is
/// the mesh itself.
fn draft(shared: &Shared, id: &str, tile: &str, file: &str, v: &Value) -> Reply {
    let Some(text) = v["text"].as_str() else {
        return bad("give the text").into();
    };
    match shared
        .submit_draft(id, tile, file, text)
        .recv_timeout(Duration::from_secs(120))
    {
        Ok(Outcome::Mesh(mesh)) => Reply::Bytes(mesh),
        Ok(Outcome::Mistake) => (
            "422 Unprocessable Entity",
            json!({ "error": "the text has a mistake" }),
        )
            .into(),
        Ok(Outcome::Superseded) => (
            "409 Conflict",
            json!({ "superseded": true, "error": "a newer draft took its place" }),
        )
            .into(),
        Ok(Outcome::Failed(why)) => (
            "500 Internal Server Error",
            json!({ "error": format!("the draft could not be built: {why}") }),
        )
            .into(),
        Err(e) => (
            "500 Internal Server Error",
            json!({ "error": format!("the draft was not answered ({e}): the build worker may have died") }),
        )
            .into(),
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
                        // best-effort cleanup of the half-made file; the write error is what is reported
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

/// After a file appears or moves: list it, start its build, say where and
/// which tile it is (with its state, so the page can show it before the
/// event that lists it arrives).
fn listed(shared: &Shared, dest: &Path) -> (&'static str, Value) {
    shared.rescan();
    let path = dest.to_string_lossy().into_owned();
    let id = tile_id(&path);
    let tile = shared.summary(&id);
    if tile.is_some() {
        shared.want(&id);
    }
    (
        "200 OK",
        json!({ "path": path, "id": tile.as_ref().map(|_| id), "tile": tile }),
    )
}

fn home(shared: &Shared) -> Result<PathBuf, (&'static str, Value)> {
    let home = shared
        .roots
        .iter()
        .find_map(|r| match &r.kind {
            RootKind::Dir(d) => Some(d.clone()),
            _ => None,
        })
        .map_or_else(
            || personal_home().map_err(|e| ("500 Internal Server Error", json!({ "error": e }))),
            Ok,
        )?;
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
                // best-effort cleanup of the new link; the failure to remove the old name is what is reported
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
        "tile size=\"{size}\" seed=1 {{\n    about \"A new tile.\"\n    stack {{\n        base\n        dirt\n    }}\n}}\n"
    );
    match home(shared).and_then(|h| write_new(&h, &stem, &text)) {
        Ok(dest) => listed(shared, &dest),
        Err(e) => e,
    }
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

/// `/api/check-value`: the reader's check of one typed value.
fn check_value(v: &Value) -> (&'static str, Value) {
    let (Some(element), Some(control), Some(typed)) = (
        v["element"].as_str(),
        v["control"].as_str(),
        v["typed"].as_str(),
    ) else {
        return bad("give the element, the control and what was typed");
    };
    // a control the schema does not know (the tile's own, a line's) has
    // nothing to check against here
    let flaw = dt_doc::schema::element(element)
        .and_then(|e| e.control(control))
        .and_then(|c| dt_doc::decode::check_text(c, element, typed));
    let body = match flaw {
        Some(r) => json!({ "message": r.message, "label": r.label, "help": r.help }),
        None => json!({ "message": null }),
    };
    ("200 OK", body)
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
    let opts = shared.options();
    match dt_eval::check(&src, tile, &opts) {
        Ok(c) => {
            let stack = c.tile.as_ref().map(|t| stack::items(t, &opts.system));
            let own = match src.read(tile) {
                Ok(t) => t,
                Err(e) => {
                    return (
                        "500 Internal Server Error",
                        json!({ "error": format!("cannot read {tile}: {e}") }),
                    );
                }
            };
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
    let now = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            return (
                "500 Internal Server Error",
                json!({ "error": format!("cannot read {file} to compare it: {e}") }),
            );
        }
    };
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
            // best-effort cleanup of the temporary file; the write error is what is reported
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
/// the gate), written to [`export_dir`].  With `where`, only says where.
/// It builds for the target the page chose (the title bar's chip);
/// `threemf` adds the painted 3MF.
fn export(shared: &Shared, tile: &str, v: &Value) -> (&'static str, Value) {
    let threemf = v["threemf"].as_bool().unwrap_or(false);
    let template = if threemf {
        match crate::bambu::template(None) {
            Ok(t) => t,
            Err(e) => return ("500 Internal Server Error", json!({ "error": e })),
        }
    } else {
        None
    };
    let out = match export_dir(tile) {
        Ok(o) => o,
        Err(e) => return ("500 Internal Server Error", json!({ "error": e })),
    };
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
    let palette = match shared.palette_checked() {
        Ok(p) => p,
        Err(e) => return ("500 Internal Server Error", json!({ "error": e })),
    };
    let extras = crate::build::Extras {
        progress: false,
        tile_no: None,
        png: false,
        threemf,
        template: template.as_deref(),
        palette: &palette,
        cache: None,
    };
    let opts = shared.options();
    let target = opts.target();
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
    // a file of yours goes to out/<target>/<name>-<target>.stl, not under
    // the folders it sits in (a library tile mirrors the library, as build does)
    if !tile.starts_with("lib:") {
        let stem = Path::new(tile)
            .file_stem()
            .map_or_else(|| "tile".to_owned(), |s| s.to_string_lossy().into_owned());
        b.out = PathBuf::from(&target).join(format!("{stem}-{target}.stl"));
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
///
/// # Errors
/// When the library folder or the tile's folder cannot be resolved to compare
/// them, or there is no personal home: a guess could write into the library.
pub(super) fn export_dir(tile: &str) -> Result<PathBuf, String> {
    let in_library = if tile.starts_with("lib:") {
        true
    } else if let Some(lib) = dt_eval::source::library_dir() {
        let l = lib
            .canonicalize()
            .map_err(|e| format!("cannot resolve the library folder {}: {e}", lib.display()))?;
        let folder = Path::new(tile)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let f = folder
            .canonicalize()
            .map_err(|e| format!("cannot resolve the folder of {tile}: {e}"))?;
        f.starts_with(l)
    } else {
        false
    };
    if in_library {
        return Ok(personal_home()?.join("out"));
    }
    let dir = Path::new(tile).parent().unwrap_or_else(|| Path::new("."));
    let project = dt_eval::discover(dir);
    if let Some(why) = &project.problem {
        super::tell_once(&format!(
            "note: {why}; the project file is ignored, so exports go to out/"
        ));
    }
    Ok(project.out)
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
        let shared = Shared::with_roots(roots, &[], &dt_eval::Options::default());
        let (status, made) = new_tile(
            &shared,
            &json!({ "name": "Shore / first try", "size": "2x1" }),
        );
        assert_eq!(status, "200 OK");
        let path = made["path"].as_str().unwrap();
        assert!(path.ends_with("Shore - first try.dtile"), "{path}");
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("tile size=\"2x1\""));
        assert!(text.contains("base\n        dirt\n"));
        dt_eval::check(&dt_eval::Files, path, &Options::default()).unwrap();
        assert!(made["id"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rename_moves_the_file_and_never_clobbers() {
        let dir = std::env::temp_dir().join(format!("dt-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
        let shared = Shared::with_roots(roots, &[], &dt_eval::Options::default());
        let pond = new_tile(&shared, &json!({ "name": "Pond" })).1;
        let marsh = new_tile(&shared, &json!({ "name": "Marsh" })).1;
        let pond = pond["path"].as_str().unwrap().to_owned();
        let marsh = marsh["path"].as_str().unwrap().to_owned();

        let (status, moved) = rename(&shared, &pond, &json!({ "name": "Still lake" }));
        assert_eq!(status, "200 OK", "{moved}");
        let lake = moved["path"].as_str().unwrap().to_owned();
        assert!(lake.ends_with("Still lake.dtile"), "{lake}");
        assert!(!Path::new(&pond).exists());
        assert!(moved["id"].is_string());

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
