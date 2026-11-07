//! `dharmatiles preview`: a live, painted 3D view of tiles in the browser.
//!
//! One small HTTP server on 127.0.0.1 (never 0.0.0.0) serves the viewer
//! (`web/preview/`, embedded in the program) and the tiles' state:
//!
//! | route | what |
//! |---|---|
//! | `GET /` and assets | the viewer |
//! | `GET /api/tiles` | every tile's state as JSON |
//! | `GET /api/tiles/N/mesh` | the tile's mesh as one binary buffer ([`payload`]) |
//! | `POST /api/tiles/N/build` | build a tile that has not been built yet |
//! | `GET /api/events` | server-sent events: a snapshot, then one event per state change |
//!
//! The tiles come from roots: the files named on the command line (built
//! at once), folders (every tile document under them, built when chosen)
//! and the library.  With no paths the roots are the personal home
//! ([`personal_home`]) and the library.  Folders and the on-disk library
//! are rescanned every second: new tiles join the list, deleted ones leave
//! it.
//!
//! A worker thread builds the tiles one at a time and, as `watch` does,
//! polls the modification time of every file a build read, rebuilding the
//! tile when one changes.  Each build pushes the plan pass first (the
//! tile's size and stack, from `check`, before any geometry), then
//! the finished mesh.  Errors are pushed with their file, line and column.

mod assets;
mod edit;
mod map;
pub mod payload;
pub(crate) mod roots;
mod server;
mod stack;
mod worker;

pub use roots::{Root, RootKind, roots_for};
#[cfg(test)]
use roots::{lib_group, personal_home};
pub use worker::work;

use std::collections::{BTreeMap, VecDeque};
use std::net::TcpListener;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use dt_eval::progress::{StageInfo, Status, Timings, Tracker, plan_stages};
use dt_eval::{Diag, Options};
use serde_json::{Value, json};

use crate::watch::{Recording, mtime};
use crate::{errors_json, tile_json};

/// How often the worker looks for work.
const POLL: Duration = Duration::from_millis(150);
/// The first port tried, when `--port` is not given.
pub const FIRST_PORT: u16 = 7878;
/// The stage after the build: the mesh's colours and binary buffer.
const ENCODE: &str = "encode";
/// How often folders are rescanned for new and deleted tiles.
const RESCAN: Duration = Duration::from_secs(1);
/// Private unwind payload used for cooperative latest-edit-wins cancellation.
/// `resume_unwind` avoids printing an alarming panic for an expected abort.
#[derive(Debug)]
struct Superseded;
/// One served tile.
pub struct Slot {
    path: String,
    /// Its name: the file name, without `.dtile`.
    name: String,
    /// The root it is listed under, and its group there.
    root: usize,
    group: Option<String>,
    /// Its file has gone from its folder: off the list, never built.  The
    /// slot stays so every other tile keeps its index.
    gone: bool,
    /// `idle` (not built yet), `building`, `ready` or `failed`.
    status: &'static str,
    plan: Option<Value>,
    record: Option<Value>,
    errors: Vec<Value>,
    mesh: Option<Arc<Vec<u8>>>,
    mesh_version: u64,
    /// The build's progress (a `progress` event's body) while building.
    progress: Option<Value>,
    /// Files the last build read, with their times.
    files: BTreeMap<String, Option<SystemTime>>,
    /// The last draft (see `edit::drafts`): its mesh, and the page's id
    /// for the text it was built from.
    draft: Option<Arc<Vec<u8>>>,
    draft_id: u64,
    /// While it builds: what each stage has made so far, for the page to
    /// show as it goes (see [`worker`]).  Gone when the build ends.
    sketches: Vec<Sketch>,
}

/// One stage's sketch while a build runs: a later one of the same stage
/// replaces it (the cliff's rough boxes, then its stones).
struct Sketch {
    stage: String,
    id: u64,
    rough: bool,
    mesh: Arc<Vec<u8>>,
}

impl Slot {
    #[cfg(test)]
    fn new(path: &str) -> Self {
        Self::in_root(path, 0, lib_group(path))
    }

    fn in_root(path: &str, root: usize, group: Option<String>) -> Self {
        let name = path
            .rsplit(['/', ':'])
            .next()
            .unwrap_or(path)
            .trim_end_matches(".dtile")
            .to_owned();
        Self {
            path: path.to_owned(),
            name,
            root,
            group,
            gone: false,
            status: "idle",
            plan: None,
            record: None,
            errors: Vec::new(),
            mesh: None,
            mesh_version: 0,
            progress: None,
            files: BTreeMap::new(),
            draft: None,
            draft_id: 0,
            sketches: Vec::new(),
        }
    }

    fn summary(&self, index: usize) -> Value {
        json!({
            "index": index,
            "path": self.path,
            "name": self.name,
            "root": self.root,
            "group": self.group,
            "gone": self.gone,
            "status": self.status,
            "plan": self.plan,
            "record": self.record,
            "errors": self.errors,
            "progress": self.progress,
            "mesh_version": self.mesh.as_ref().map(|_| self.mesh_version),
            "mesh_bytes": self.mesh.as_ref().map(|m| m.len()),
            "draft_id": self.draft.as_ref().map(|_| self.draft_id),
            "sketches": self.sketches.iter().map(|k| json!({"id": k.id, "rough": k.rough})).collect::<Vec<_>>(),
        })
    }

    fn changed(&self) -> bool {
        !self.gone && self.status != "idle" && self.files.iter().any(|(f, t)| mtime(f) != *t)
    }
}

/// What the server and the worker share.
pub struct Shared {
    system: String,
    overhang_deg: f64,
    steepest_deg: f64,
    tiles: Mutex<Vec<Slot>>,
    clients: Mutex<Vec<Sender<String>>>,
    wanted: Mutex<VecDeque<usize>>,
    /// Where measured stage durations are kept between builds.
    cache: Option<std::path::PathBuf>,
    /// A palette document the paint is read from at each build.
    palette: Option<std::path::PathBuf>,
    /// Where the tiles come from; a slot's `root` indexes this.
    roots: Vec<Root>,
    /// When the folders were last rescanned.
    scanned: Mutex<Option<Instant>>,
    /// This run's secret: the editor's calls carry it (see `server`).
    token: String,
    /// The draft the page asked for last, not yet built.
    draft_req: Mutex<Option<edit::DraftRequest>>,
    draft_cv: std::sync::Condvar,
    /// Each sketch's id, unique across tiles.
    sketch_seq: std::sync::atomic::AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// Serve `paths` for connector `system` (the tests' server).
    #[cfg(test)]
    #[must_use]
    pub fn new(paths: &[String], system: &str, overhang_deg: f64, steepest_deg: f64) -> Self {
        let roots = if paths.is_empty() {
            Vec::new()
        } else {
            vec![Root::new("Files", RootKind::Files)]
        };
        Self::with_roots(roots, paths, system, overhang_deg, steepest_deg)
    }

    /// Serve `roots` for connector `system`: `files` (listed under the
    /// first [`RootKind::Files`] root) and every tile in the others.
    #[must_use]
    pub fn with_roots(
        roots: Vec<Root>,
        files: &[String],
        system: &str,
        overhang_deg: f64,
        steepest_deg: f64,
    ) -> Self {
        let files_root = roots.iter().position(|r| r.kind == RootKind::Files);
        let tiles = files_root.map_or_else(Vec::new, |r| {
            files
                .iter()
                .map(|p| Slot::in_root(p, r, roots[r].group(p)))
                .collect()
        });
        let me = Self {
            system: system.to_owned(),
            overhang_deg,
            steepest_deg,
            tiles: Mutex::new(tiles),
            clients: Mutex::new(Vec::new()),
            wanted: Mutex::new(VecDeque::new()),
            cache: None,
            palette: None,
            roots,
            scanned: Mutex::new(None),
            token: edit::token(),
            draft_req: Mutex::new(None),
            draft_cv: std::sync::Condvar::new(),
            sketch_seq: std::sync::atomic::AtomicU64::new(0),
        };
        me.rescan();
        me
    }

    /// Read the paint from this palette document at each build, and
    /// rebuild when it is saved.
    #[must_use]
    pub fn with_palette(mut self, file: std::path::PathBuf) -> Self {
        self.palette = Some(file);
        self
    }

    fn with_palette_opt(self, f: Option<std::path::PathBuf>) -> Self {
        match f {
            Some(f) => self.with_palette(f),
            None => self,
        }
    }

    /// The paint now: the palette document if there is a good one, else
    /// the default.
    fn palette(&self) -> dt_kit::Palette {
        let Some(f) = &self.palette else {
            return dt_kit::Palette::default();
        };
        match std::fs::read_to_string(f)
            .map_err(|e| e.to_string())
            .and_then(|t| dt_kit::Palette::parse(&t))
        {
            Ok(p) => p,
            Err(e) => {
                tell!("palette {}: {e}; using the default paint", f.display());
                dt_kit::Palette::default()
            }
        }
    }

    /// Keep stage timings in `file`, so the next build of a tile has an ETA.
    #[must_use]
    pub fn with_cache(mut self, file: std::path::PathBuf) -> Self {
        self.cache = Some(file);
        self
    }

    /// Tell every listening browser `msg`.
    fn broadcast(&self, msg: &Value) {
        let msg = msg.to_string();
        lock(&self.clients).retain(|tx| tx.send(msg.clone()).is_ok());
    }

    /// The callback a tracker for tile `i` reports through.
    fn sink(self: &Arc<Self>, i: usize) -> impl Fn(&Status) + Send + Sync + use<> {
        let me = Arc::clone(self);
        move |s| me.progress(i, s)
    }

    /// A tile's progress: remembered for a browser that connects later,
    /// and sent now.  The tracker calls this about ten times a second.
    fn progress(&self, i: usize, status: &Status) {
        let body = json!({
            "fraction": status.fraction, "label": status.label, "key": status.key,
            "count": status.count, "elapsed_s": status.elapsed_s,
            "eta_s": status.eta_s, "estimate_s": status.estimate_s,
        });
        if let Some(slot) = lock(&self.tiles).get_mut(i) {
            slot.progress = Some(body.clone());
        }
        self.broadcast(&json!({"type": "progress", "index": i, "progress": body}));
    }

    /// Every tile's state, as the `/api/tiles` body and the first event.
    fn snapshot(&self) -> Value {
        let tiles = lock(&self.tiles);
        json!({
            "type": "snapshot",
            "system": self.system,
            "overhang_deg": self.overhang_deg,
            "steepest_deg": self.steepest_deg,
            "roots": self.roots.iter().enumerate().map(|(i, r)| r.json(i)).collect::<Vec<_>>(),
            "tiles": tiles.iter().enumerate().map(|(i, t)| t.summary(i)).collect::<Vec<_>>(),
        })
    }

    /// Rescan the folders and the library once [`RESCAN`] has passed:
    /// new tiles join the list, deleted ones leave it (and come back if
    /// the file does).  Every browser is told the new list.
    fn rescan_due(&self) {
        let due = {
            let mut last = lock(&self.scanned);
            let due = last.is_none_or(|t| t.elapsed() >= RESCAN);
            if due {
                *last = Some(Instant::now());
            }
            due
        };
        if due && self.rescan() {
            self.broadcast(&self.snapshot());
        }
    }

    /// Rescan every root now.  True when the list changed.
    fn rescan(&self) -> bool {
        let found: Vec<(usize, Vec<String>)> = self
            .roots
            .iter()
            .enumerate()
            .filter_map(|(r, root)| Some((r, root.scan()?)))
            .collect();
        let mut tiles = lock(&self.tiles);
        let mut changed = false;
        for (r, paths) in found {
            let have: std::collections::BTreeSet<&str> = paths.iter().map(String::as_str).collect();
            for t in tiles.iter_mut().filter(|t| t.root == r) {
                let gone = !have.contains(t.path.as_str());
                changed |= t.gone != gone;
                t.gone = gone;
            }
            for p in &paths {
                if !tiles.iter().any(|t| t.root == r && &t.path == p) {
                    tiles.push(Slot::in_root(p, r, self.roots[r].group(p)));
                    changed = true;
                }
            }
        }
        changed
    }

    /// Change tile `i` and tell every listening browser.
    fn update(&self, i: usize, f: impl FnOnce(&mut Slot)) {
        let summary = {
            let mut tiles = lock(&self.tiles);
            let Some(slot) = tiles.get_mut(i) else { return };
            f(slot);
            slot.summary(i)
        };
        self.broadcast(&json!({"type": "tile", "tile": summary}));
    }

    /// A new listener, whose first message is the snapshot.  The clients
    /// lock is held across both steps so no update falls between them.
    fn subscribe(&self) -> std::sync::mpsc::Receiver<String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut clients = lock(&self.clients);
        let _ = tx.send(self.snapshot().to_string());
        clients.push(tx);
        rx
    }

    /// Ask for tile `i` to be built next (a no-op once it has been built).
    fn want(&self, i: usize) -> bool {
        let idle = lock(&self.tiles)
            .get(i)
            .filter(|t| !t.gone)
            .map(|t| t.status == "idle");
        if idle == Some(true) {
            let mut w = lock(&self.wanted);
            if !w.contains(&i) {
                w.push_back(i);
            }
        }
        idle.is_some()
    }

    /// A file of tile `i` for the text pane: the tile's own document, or
    /// (`file`) one its last build read, and whether it may be written
    /// (library files never are).  Nothing else on disk is served.
    fn text(&self, i: usize, file: Option<&str>) -> Option<Value> {
        let path = {
            let tiles = lock(&self.tiles);
            let t = tiles.get(i)?;
            match file {
                None => t.path.clone(),
                Some(f) if f == t.path || t.files.contains_key(f) => f.to_owned(),
                Some(_) => return None,
            }
        };
        if !path.ends_with(".dtile") {
            return None;
        }
        let text = dt_eval::Source::read(&dt_eval::Files, &path).ok()?;
        let writable = !path.starts_with(dt_eval::LIB)
            && std::fs::metadata(&path).is_ok_and(|m| !m.permissions().readonly());
        Some(serde_json::json!({ "file": path, "text": text, "writable": writable }))
    }

    fn mesh(&self, i: usize) -> Option<Arc<Vec<u8>>> {
        lock(&self.tiles).get(i)?.mesh.clone()
    }

    fn draft_mesh(&self, i: usize) -> Option<Arc<Vec<u8>>> {
        lock(&self.tiles).get(i)?.draft.clone()
    }

    fn sketch_mesh(&self, i: usize, id: u64) -> Option<Arc<Vec<u8>>> {
        lock(&self.tiles)
            .get(i)?
            .sketches
            .iter()
            .find(|k| k.id == id)
            .map(|k| Arc::clone(&k.mesh))
    }

    /// Keep stage `stage`'s sketch for tile `i`, in place of its last.
    fn add_sketch(&self, i: usize, stage: &str, rough: bool, mesh: Vec<u8>) {
        let id = self.sketch_seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.update(i, |t| {
            t.sketches.retain(|k| k.stage != stage);
            t.sketches.push(Sketch {
                stage: stage.to_owned(),
                id,
                rough,
                mesh: Arc::new(mesh),
            });
        });
    }

    /// The next tile to build: one asked for, else (when `eager`) the first
    /// named file not built yet, else one whose files changed.  Folder and
    /// library tiles are built only when asked for.
    fn pick(&self, eager: bool) -> Option<usize> {
        if let Some(i) = lock(&self.wanted).pop_front() {
            return Some(i);
        }
        let tiles = lock(&self.tiles);
        let named = |t: &Slot| {
            self.roots
                .get(t.root)
                .is_none_or(|r| r.kind == RootKind::Files)
        };
        if eager
            && let Some(i) = tiles
                .iter()
                .position(|t| t.status == "idle" && !t.gone && named(t))
        {
            return Some(i);
        }
        tiles.iter().position(Slot::changed)
    }
}

/// Open `url` in the default browser.
fn open_browser(url: &str) {
    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/c", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    if std::process::Command::new(cmd).args(args).spawn().is_err() {
        tell!("could not open a browser; open {url} yourself");
    }
}

/// Bind 127.0.0.1 on `port`, or, with none given, the first free port from
/// [`FIRST_PORT`].
fn bind(port: Option<u16>) -> Result<TcpListener, String> {
    match port {
        Some(p) => TcpListener::bind(("127.0.0.1", p))
            .map_err(|e| format!("cannot listen on 127.0.0.1:{p}: {e}")),
        None => (FIRST_PORT..FIRST_PORT + 20)
            .find_map(|p| TcpListener::bind(("127.0.0.1", p)).ok())
            .ok_or_else(|| format!("no free port from {FIRST_PORT}; give one with --port")),
    }
}

/// `dharmatiles preview`.  Returns an exit code (it runs until Ctrl-C).
pub fn run(
    paths: &[String],
    opts: &Options,
    port: Option<u16>,
    open: bool,
    palette: Option<std::path::PathBuf>,
) -> u8 {
    let listener = match bind(port) {
        Ok(l) => l,
        Err(e) => {
            tell!("error: {e}");
            return crate::ERROR;
        }
    };
    let addr = listener
        .local_addr()
        .map_or_else(|_| "127.0.0.1".to_owned(), |a| a.to_string());
    // files named are built at once; folder and library tiles when chosen
    let (roots, files) = roots_for(paths);
    let cache = dt_eval::discover(&std::env::current_dir().unwrap_or_default()).out;
    let shared = Arc::new(
        Shared::with_roots(
            roots,
            &files,
            &opts.system,
            opts.printer.overhang_deg,
            opts.printer.steepest_deg,
        )
        .with_cache(Timings::file_in(&cache))
        .with_palette_opt(palette),
    );
    let url = format!("http://{addr}/");
    tell!("preview: {url}  (Ctrl-C to stop)");
    for (i, r) in shared.roots.iter().enumerate() {
        let n = lock(&shared.tiles)
            .iter()
            .filter(|t| !t.gone && t.root == i)
            .count();
        match &r.kind {
            RootKind::Dir(d) if !d.is_dir() => tell!(
                "{}: {} (not there yet; tiles saved there will show up)",
                r.label,
                d.display()
            ),
            RootKind::Dir(d) => tell!("{}: {} ({n} tile(s))", r.label, d.display()),
            RootKind::Files => tell!("{}: {n} file(s)", r.label),
            RootKind::Library(_) => tell!("{}: {n} tile(s)", r.label),
        }
    }
    match dt_eval::source::library_dir() {
        Some(d) => tell!(
            "library: {} (read from disk; edits and new tiles show up live)",
            d.display()
        ),
        None => tell!("library: the copy built into this program"),
    }
    server::spawn(listener, Arc::clone(&shared));
    edit::drafts(Arc::clone(&shared), opts.clone());
    if open {
        open_browser(&url);
    }
    work(&shared, opts, true, &AtomicBool::new(false));
    crate::OK
}

#[cfg(test)]
mod tests;
