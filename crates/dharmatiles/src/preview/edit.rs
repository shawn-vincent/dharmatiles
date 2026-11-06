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
//! - `/api/tiles/N/draft` `{text, file?, id}`: build a draft of the tile
//!   from `text` (see `dt_eval::Options::draft`), on a thread of its own so
//!   a full build never holds it up.  Only the latest asked for is built;
//!   when it is ready the tile's `draft_id` says so, and
//!   `GET /api/tiles/N/mesh?draft=1` serves it.  Nothing is written.

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};

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

/// A conservative filename for a tile name. The document keeps the name as
/// written; this is only the file's stem, and never contains a path separator.
fn tile_stem(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '+' | '_' | '-') {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !out.is_empty() && !dash {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "untitled".to_owned()
    } else {
        out
    }
}

/// Make a small, valid tile in My tiles and return its slot. Starting from a
/// printable flat square gives the editor something real to select and change;
/// every later gesture remains ordinary `.dtile` text.
fn new_tile(shared: &Shared, v: &Value) -> (&'static str, Value) {
    let name = v["name"].as_str().unwrap_or("Untitled tile").trim();
    let name = if name.is_empty() {
        "Untitled tile"
    } else {
        name
    };
    let size = v["size"].as_str().unwrap_or("1x1");
    if !matches!(size, "1x1" | "2x1" | "1x2" | "2x2") {
        return bad("size must be 1x1, 2x1, 1x2 or 2x2");
    }
    if name.chars().count() > 80 || name.contains(['\n', '\r']) {
        return bad("the tile name must be one line of at most 80 characters");
    }
    let home = shared
        .roots
        .iter()
        .find_map(|r| match &r.kind {
            RootKind::Dir(d) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_else(personal_home);
    if let Err(e) = std::fs::create_dir_all(&home) {
        return (
            "500 Internal Server Error",
            json!({ "error": format!("cannot make {}: {e}", home.display()) }),
        );
    }
    let stem = tile_stem(name);
    let dest = (1..1000)
        .map(|n| {
            home.join(if n == 1 {
                format!("{stem}.dtile")
            } else {
                format!("{stem}-{n}.dtile")
            })
        })
        .find(|p| !p.exists());
    let Some(dest) = dest else {
        return (
            "500 Internal Server Error",
            json!({ "error": "no free name" }),
        );
    };
    let quoted = serde_json::to_string(name).unwrap_or_else(|_| "\"Untitled tile\"".to_owned());
    let text = format!(
        "tile {quoted} size=\"{size}\" seed=1 {{\n    about \"A new tile.\"\n    stack {{\n        ground level=edge\n    }}\n}}\n"
    );
    if let Err(e) = std::fs::write(&dest, text) {
        return (
            "500 Internal Server Error",
            json!({ "error": format!("cannot write {}: {e}", dest.display()) }),
        );
    }
    if shared.rescan() {
        shared.broadcast(&shared.snapshot());
    }
    let path = dest.to_string_lossy().into_owned();
    let index = lock(&shared.tiles).iter().position(|t| t.path == path);
    if let Some(i) = index {
        shared.want(i);
    }
    ("200 OK", json!({ "path": path, "index": index }))
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
            let built = std::panic::catch_unwind(AssertUnwindSafe(|| {
                dt_eval::build_with(&src, &r.path, &opts, &progress)
            }));
            if built.as_ref().is_err_and(|panic| panic.is::<Superseded>()) {
                continue;
            }
            // a newer request already waiting makes this one moot
            if lock(&shared.draft_req).is_some() {
                continue;
            }
            if let Ok(Ok(b)) = built {
                let mesh = Arc::new(payload::from_built(
                    &b,
                    opts.printer.overhang_deg,
                    &shared.palette(),
                ));
                shared.update(r.tile, |t| {
                    t.draft = Some(mesh);
                    t.draft_id = r.id;
                });
            }
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
    let home = shared
        .roots
        .iter()
        .find_map(|r| match &r.kind {
            RootKind::Dir(d) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or_else(personal_home);
    if let Err(e) = std::fs::create_dir_all(&home) {
        return (
            "500 Internal Server Error",
            json!({ "error": format!("cannot make {}: {e}", home.display()) }),
        );
    }
    let stem = tile
        .rsplit('/')
        .next()
        .unwrap_or("tile")
        .trim_end_matches(".dtile")
        .to_owned();
    let dest = (1..1000)
        .map(|n| {
            home.join(if n == 1 {
                format!("{stem}.dtile")
            } else {
                format!("{stem}-{n}.dtile")
            })
        })
        .find(|p| !p.exists());
    let Some(dest) = dest else {
        return (
            "500 Internal Server Error",
            json!({ "error": "no free name" }),
        );
    };
    if let Err(e) = std::fs::write(&dest, pin_uses(&text, files)) {
        return (
            "500 Internal Server Error",
            json!({ "error": format!("cannot write {}: {e}", dest.display()) }),
        );
    }
    if shared.rescan() {
        shared.broadcast(&shared.snapshot());
    }
    let path = dest.to_string_lossy().into_owned();
    let index = lock(&shared.tiles).iter().position(|t| t.path == path);
    if let Some(i) = index {
        shared.want(i);
    }
    ("200 OK", json!({ "path": path, "index": index }))
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
        let text = "// a\nuse \"parts/grass\" lawn\nuse \"lib:parts/trees\" oak\ntile a {}\n";
        assert_eq!(
            pin_uses(text, &files),
            "// a\nuse \"lib:parts/grass\" lawn\nuse \"lib:parts/trees\" oak\ntile a {}\n"
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
        assert!(path.ends_with("shore-first-try.dtile"), "{path}");
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.contains("tile \"Shore / first try\" size=\"2x1\""));
        assert!(text.contains("ground level=edge"));
        dt_eval::check(&dt_eval::Files, path, &Options::default()).unwrap();
        assert!(made["index"].is_number());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
