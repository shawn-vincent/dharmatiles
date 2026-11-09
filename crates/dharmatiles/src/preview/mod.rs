//! `dharmatiles preview`: a live, painted 3D view of tiles in the browser.
//!
//! One small HTTP server on 127.0.0.1 (never 0.0.0.0) serves the viewer
//! (`web/preview/`, embedded in the program) and the tiles' state:
//!
//! | route | what |
//! |---|---|
//! | `GET /` and assets | the viewer |
//! | `GET /api/tiles` | every tile's state as JSON |
//! | `GET /api/tiles/ID/mesh` | the tile's mesh as one binary buffer ([`payload`]) |
//! | `GET /api/tiles/ID/sketch?id=K` | one stage's sketch of a running build |
//! | `GET /api/tiles/ID/text` | the tile's document (or `?file=` one it read) for the text pane |
//! | `POST /api/tiles/ID/build` | build a tile that has not been built yet |
//! | `POST /api/tiles/ID/draft` | build the posted text quickly; the reply is its mesh |
//! | `GET /api/events?epoch=E&since=N` | server-sent events: a snapshot (or, resuming, what changed since revision N), then one event per change |
//! | `GET /api/session` | this run's token, which every other `POST` must carry |
//! | `GET /api/elements` | the elements and their controls |
//! | `GET /api/weightings?range=` / `?text=` | the weighting functions, each with its curve ([`crate::weightings`]) |
//! | `POST /api/…` | the editor's calls: check, draft, save, rename, new, export… ([`edit`]) |
//!
//! A tile is named by its id: a hash of its path, the same for as long as
//! the path is listed (and across runs), never a position in the list.
//! Every change to the list or a tile has a revision number; an event
//! stream that drops resumes from its last one.  One queue holds every
//! build: drafts first, then the tiles asked for, then the tiles whose
//! files changed.  A newer draft of a tile supersedes the older.
//!
//! The tiles come from roots: the files named on the command line (built
//! at once), folders (every tile document under them, built when chosen)
//! and the library.  With no paths the roots are the personal home
//! ([`personal_home`]) and the library.  Folders and the on-disk library
//! are rescanned every second: new tiles join the list, deleted ones leave
//! it.
//!
//! Worker threads take the queue's jobs (one full build at a time, a draft
//! beside it) and, as `watch` does, poll the modification time of every file a build read, rebuilding the
//! tile when one changes.  Each build pushes the plan pass first (the
//! tile's size and stack, from `check`, before any geometry), then
//! the finished mesh.  Errors are pushed with their file, line and column.

mod assets;
mod http;
mod queue;
mod route;

/// Where a tile's build stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    /// Not built yet: built when it is chosen.
    Idle,
    Building,
    Ready,
    Failed,
}

impl State {
    /// As the page reads it.
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}
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

use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
/// How many changes the event log remembers for a stream that resumes.
const LOG: usize = 1024;
/// Private unwind payload used for cooperative latest-edit-wins cancellation.
/// `resume_unwind` avoids printing an alarming panic for an expected abort.
#[derive(Debug)]
struct Superseded;

/// A tile's name for the page: 16 hex digits of a hash of its path.  The
/// same path is the same id for as long as it is listed, and across runs.
type TileId = String;

fn tile_id(path: &str) -> TileId {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// One served tile.
pub struct Slot {
    id: TileId,
    path: String,
    /// Its name: the file name, without `.dtile`.
    name: String,
    /// The root it is listed under, and its group there.
    root: usize,
    group: Option<String>,
    status: State,
    plan: Option<Value>,
    record: Option<Value>,
    errors: Vec<Value>,
    mesh: Option<Arc<Vec<u8>>>,
    mesh_version: u64,
    /// The build's progress (a `progress` event's body) while building.
    progress: Option<Value>,
    /// Files the last build read, with their times.
    files: BTreeMap<String, Option<SystemTime>>,
    /// Built for a target that is not the chosen one any more.
    stale: bool,
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
            id: tile_id(path),
            path: path.to_owned(),
            name,
            root,
            group,
            status: State::Idle,
            plan: None,
            record: None,
            errors: Vec::new(),
            mesh: None,
            mesh_version: 0,
            progress: None,
            files: BTreeMap::new(),
            stale: false,
            sketches: Vec::new(),
        }
    }

    fn summary(&self) -> Value {
        json!({
            "id": self.id,
            "path": self.path,
            "name": self.name,
            "root": self.root,
            "group": self.group,
            "status": self.status.as_str(),
            "plan": self.plan,
            "record": self.record,
            "errors": self.errors,
            "progress": self.progress,
            "mesh_version": self.mesh.as_ref().map(|_| self.mesh_version),
            "mesh_bytes": self.mesh.as_ref().map(|m| m.len()),
            "sketches": self.sketches.iter().map(|k| json!({"id": k.id, "rough": k.rough})).collect::<Vec<_>>(),
        })
    }

    fn changed(&self) -> bool {
        self.status != State::Idle && (self.stale || self.files.iter().any(|(f, t)| mtime(f) != *t))
    }
}

/// What changed, as the event log remembers it.
enum Change {
    Tile(TileId),
    /// Tiles joined or left the list.
    List,
}

/// The listeners and the changes they have been sent.  Every change to
/// the list or a tile takes the next revision; progress events ride on
/// the current one.  Locked after the tiles (never before).
struct Events {
    rev: u64,
    log: VecDeque<(u64, Change)>,
    clients: Vec<Sender<String>>,
}

impl Events {
    fn send(&mut self, msg: &Value) {
        let msg = msg.to_string();
        self.clients.retain(|tx| tx.send(msg.clone()).is_ok());
    }

    /// Note a change, and say which revision it is.
    fn bump(&mut self, change: Change) -> u64 {
        self.rev += 1;
        self.log.push_back((self.rev, change));
        if self.log.len() > LOG {
            self.log.pop_front();
        }
        self.rev
    }
}

/// What the server and the worker share.
pub struct Shared {
    /// The build target the page chose: base system and printer, as the
    /// options every build is made with (see [`Shared::set_target`]).
    target: Mutex<Options>,
    /// Where the user's own printers live and the chosen target is
    /// remembered (`$DT_HOME`); none in the tests that must not touch it.
    home: Option<std::path::PathBuf>,
    tiles: Mutex<Vec<Slot>>,
    events: Mutex<Events>,
    /// This run: a stream from another run cannot resume, whatever its
    /// revision.
    epoch: String,
    /// Every build waiting or running, and the signal that one arrived.
    queue: Mutex<queue::Queue>,
    queue_cv: std::sync::Condvar,
    /// Where measured stage durations are kept between builds.
    cache: Option<std::path::PathBuf>,
    /// A palette document the paint is read from at each build.
    palette: Option<std::path::PathBuf>,
    /// Where the tiles come from; a slot's `root` indexes this.
    roots: Vec<Root>,
    /// When the folders were last rescanned.
    scanned: Mutex<Option<Instant>>,
    /// This run's secret: the editor's calls carry it (see `route`).
    token: String,
    /// Each sketch's id, unique across tiles.
    sketch_seq: std::sync::atomic::AtomicU64,
    /// The build caches of the tiles built last, newest first, by path
    /// and whether they are drafts' (see [`dt_eval::Cache`]).
    build_caches: Mutex<VecDeque<(String, bool, Arc<dt_eval::Cache>)>>,
}

/// How many build caches are kept: a cliff's solids are large, so only
/// the tiles worked on last keep theirs.
const BUILD_CACHES: usize = 4;

/// Lock `m`, also when a thread panicked holding it.  Allowed: every panic
/// in a build or a route is caught, logged and shown to the page (see
/// `worker` and `server`), and the state behind these locks is only ever
/// replaced whole, so a poisoned lock holds nothing half-written.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Log `msg` to stderr the first time it is said, for a failure the periodic
/// rescan would otherwise repeat every few seconds.
pub(crate) fn tell_once(msg: &str) {
    static SAID: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    if lock(&SAID).insert(msg.to_owned()) {
        tell!("{msg}");
    }
}

/// The words of a caught panic (its message), for the log and the page.
pub(crate) fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".to_owned())
}

impl Shared {
    /// Serve `paths` for the default target (the tests' server).
    #[cfg(test)]
    #[must_use]
    pub fn new(paths: &[String]) -> Self {
        let roots = if paths.is_empty() {
            Vec::new()
        } else {
            vec![Root::new("Files", RootKind::Files)]
        };
        Self::with_roots(roots, paths, &Options::default())
    }

    /// Serve `roots` for the build target `opts`: `files` (listed under the
    /// first [`RootKind::Files`] root) and every tile in the others.
    #[must_use]
    pub fn with_roots(roots: Vec<Root>, files: &[String], opts: &Options) -> Self {
        let files_root = roots.iter().position(|r| r.kind == RootKind::Files);
        let tiles = files_root.map_or_else(Vec::new, |r| {
            files
                .iter()
                .map(|p| Slot::in_root(p, r, roots[r].group(p)))
                .collect()
        });
        let me = Self {
            target: Mutex::new(opts.clone()),
            home: None,
            tiles: Mutex::new(tiles),
            events: Mutex::new(Events {
                rev: 1,
                log: VecDeque::new(),
                clients: Vec::new(),
            }),
            epoch: edit::token()[..12].to_owned(),
            queue: Mutex::new(queue::Queue::default()),
            queue_cv: std::sync::Condvar::new(),
            cache: None,
            palette: None,
            roots,
            scanned: Mutex::new(None),
            token: edit::token(),
            sketch_seq: std::sync::atomic::AtomicU64::new(0),
            build_caches: Mutex::new(VecDeque::new()),
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

    /// Read the user's printers from, and remember the chosen target in,
    /// `home` (`$DT_HOME`).
    fn with_home(mut self, home: std::path::PathBuf) -> Self {
        self.home = Some(home);
        self
    }

    /// The options every build is made with: the chosen target.
    fn options(&self) -> Options {
        lock(&self.target).clone()
    }

    /// Choose the build target: the base system and the printer (either
    /// may be left as it is).  Every tile built so far is built again for
    /// it, the page is sent the new state, and the choice is remembered in
    /// the user's home.
    ///
    /// # Errors
    /// In words: a system that is not `db` or `ol`, a printer that is not
    /// there.
    fn set_target(&self, system: Option<&str>, printer: Option<&str>) -> Result<(), String> {
        let now = self.options();
        let system = system.unwrap_or(&now.system);
        let printer = printer.unwrap_or(&now.printer.name);
        let mut next = crate::library::options_in(system, printer, None, self.home.as_deref())?;
        next.engine = now.engine.clone();
        if next.system == now.system && next.printer.name == now.printer.name {
            return Ok(());
        }
        if let Some(h) = &self.home {
            crate::library::remember_target(h, system, printer)?;
        }
        *lock(&self.target) = next;
        let mut tiles = lock(&self.tiles);
        for t in tiles.iter_mut().filter(|t| t.status != State::Idle) {
            t.stale = true;
        }
        let mut ev = lock(&self.events);
        let rev = ev.bump(Change::List);
        let snap = self.snapshot_of(&tiles, rev);
        ev.send(&snap);
        drop(ev);
        drop(tiles);
        self.queue_cv.notify_all();
        Ok(())
    }

    fn with_palette_opt(self, f: Option<std::path::PathBuf>) -> Self {
        match f {
            Some(f) => self.with_palette(f),
            None => self,
        }
    }

    /// The paint now: the palette document, or the default when none was
    /// named.  A palette that cannot be read is an error, in words naming it.
    fn palette_checked(&self) -> Result<dt_kit::Palette, String> {
        let Some(f) = &self.palette else {
            return Ok(dt_kit::Palette::default());
        };
        std::fs::read_to_string(f)
            .map_err(|e| format!("cannot read the palette {}: {e}", f.display()))
            .and_then(|t| {
                dt_kit::Palette::parse(&t).map_err(|e| format!("palette {}: {e}", f.display()))
            })
    }

    /// The paint now, for a view: the palette document, else the default
    /// paint, saying so on stderr; the build that shows a tile tells the page
    /// too ([`Shared::palette_problem`]).
    fn palette(&self) -> dt_kit::Palette {
        self.palette_checked().unwrap_or_else(|e| {
            tell_once(&format!("{e}; using the default paint"));
            dt_kit::Palette::default()
        })
    }

    /// What is wrong with the palette, as the page's unexpected problem
    /// (empty when nothing is): the tile is shown in the default paint.
    fn palette_problem(&self) -> Vec<Value> {
        match self.palette_checked() {
            Ok(_) => Vec::new(),
            Err(e) => errors_json(&[Diag::plain(
                "dt::palette",
                format!("{e}; the tile is shown in the default paint"),
            )]),
        }
    }

    /// Keep stage timings in `file`, so the next build of a tile has an ETA.
    #[must_use]
    pub fn with_cache(mut self, file: std::path::PathBuf) -> Self {
        self.cache = Some(file);
        self
    }

    /// The callback a tracker for tile `id` reports through.
    fn sink(self: &Arc<Self>, id: &str) -> impl Fn(&Status) + Send + Sync + use<> {
        let me = Arc::clone(self);
        let id = id.to_owned();
        move |s| me.progress(&id, s)
    }

    /// A tile's progress: remembered for a browser that connects later,
    /// and sent now.  The tracker calls this about ten times a second.
    fn progress(&self, id: &str, status: &Status) {
        let body = json!({
            "fraction": status.fraction, "label": status.label, "key": status.key,
            "count": status.count, "elapsed_s": status.elapsed_s,
            "eta_s": status.eta_s, "estimate_s": status.estimate_s,
        });
        let mut tiles = lock(&self.tiles);
        if let Some(slot) = tiles.iter_mut().find(|t| t.id == id) {
            slot.progress = Some(body.clone());
            let mut ev = lock(&self.events);
            let rev = ev.rev;
            ev.send(&json!({"type": "progress", "rev": rev, "id": id, "progress": body}));
        }
    }

    /// Every tile's state: the `/api/tiles` body and the first event.
    fn snapshot_of(&self, tiles: &[Slot], rev: u64) -> Value {
        let target = self.options();
        json!({
            "type": "snapshot",
            "rev": rev,
            "epoch": self.epoch,
            "system": target.system,
            "printer": target.printer.name,
            "printers": dt_eval::printer_names(self.home.as_deref())
                .into_iter()
                .map(|(n, k)| json!({"name": n, "mine": k == dt_eval::ProfileKind::Mine}))
                .collect::<Vec<_>>(),
            "overhang_deg": target.printer.overhang_deg,
            "steepest_deg": target.printer.steepest_deg,
            "roots": self.roots.iter().enumerate().map(|(i, r)| r.json(i)).collect::<Vec<_>>(),
            "tiles": tiles.iter().map(Slot::summary).collect::<Vec<_>>(),
        })
    }

    fn snapshot(&self) -> Value {
        let tiles = lock(&self.tiles);
        let rev = lock(&self.events).rev;
        self.snapshot_of(&tiles, rev)
    }

    /// Rescan the folders and the library once [`RESCAN`] has passed:
    /// new tiles join the list, deleted ones leave it (and come back,
    /// under the same id, if the file does).  Every browser is told.
    fn rescan_due(&self) {
        let due = {
            let mut last = lock(&self.scanned);
            let due = last.is_none_or(|t| t.elapsed() >= RESCAN);
            if due {
                *last = Some(Instant::now());
            }
            due
        };
        if due {
            self.rescan();
        }
    }

    /// Rescan every root now, telling every browser the new list when it
    /// changed.  True when it did.
    fn rescan(&self) -> bool {
        let found: Vec<(usize, Vec<String>)> = self
            .roots
            .iter()
            .enumerate()
            .filter_map(|(r, root)| Some((r, root.scan()?)))
            .collect();
        let mut removed = Vec::new();
        let changed = {
            let mut tiles = lock(&self.tiles);
            let before = tiles.len();
            for (r, paths) in found {
                let have: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
                tiles.retain(|t| {
                    let keep = t.root != r || have.contains(t.path.as_str());
                    if !keep {
                        removed.push(t.id.clone());
                    }
                    keep
                });
                for p in &paths {
                    if !tiles.iter().any(|t| t.root == r && &t.path == p) {
                        tiles.push(Slot::in_root(p, r, self.roots[r].group(p)));
                    }
                }
            }
            let changed = tiles.len() != before || !removed.is_empty();
            if changed {
                let mut ev = lock(&self.events);
                let rev = ev.bump(Change::List);
                let snap = self.snapshot_of(&tiles, rev);
                ev.send(&snap);
            }
            changed
        };
        for id in &removed {
            self.forget(id);
        }
        changed
    }

    /// Change tile `id` and tell every listening browser; a tile that is
    /// not listed is left alone.
    fn update(&self, id: &str, f: impl FnOnce(&mut Slot)) {
        let mut tiles = lock(&self.tiles);
        let Some(slot) = tiles.iter_mut().find(|t| t.id == id) else {
            return;
        };
        f(slot);
        let summary = slot.summary();
        let mut ev = lock(&self.events);
        let rev = ev.bump(Change::Tile(id.to_owned()));
        ev.send(&json!({"type": "tile", "rev": rev, "tile": summary}));
    }

    /// The tile `id` as the page reads it, for a call that makes or moves
    /// one (the reply carries it, so the page need not wait for an event).
    fn summary(&self, id: &str) -> Option<Value> {
        lock(&self.tiles)
            .iter()
            .find(|t| t.id == id)
            .map(Slot::summary)
    }

    /// A new listener.  With the epoch and last revision of a stream that
    /// dropped, and the log still holding everything since, it is sent
    /// only what changed (one event per tile, as it is now); otherwise
    /// the snapshot.  Locked so no change falls between the first message
    /// and the rest.
    fn subscribe(&self, from: &route::Resume) -> std::sync::mpsc::Receiver<String> {
        // The sends below ignore their result: `rx` is held here until this
        // returns, so a send to `tx` cannot fail.
        let (tx, rx) = std::sync::mpsc::channel();
        let tiles = lock(&self.tiles);
        let mut ev = lock(&self.events);
        let mut resumed = false;
        if let (Some(e), Some(since)) = (&from.epoch, from.since)
            && *e == self.epoch
            && since <= ev.rev
            && (since == ev.rev || ev.log.front().is_some_and(|(r, _)| *r <= since + 1))
        {
            let mut ids = Vec::new();
            let mut list = false;
            for (_, c) in ev.log.iter().filter(|(r, _)| *r > since) {
                match c {
                    Change::List => list = true,
                    Change::Tile(id) if !ids.contains(id) => ids.push(id.clone()),
                    Change::Tile(_) => {}
                }
            }
            if !list {
                let rev = ev.rev;
                // (the receiver is held here until this returns: a send cannot fail)
                let _ = tx
                    .send(json!({"type": "resumed", "rev": rev, "epoch": self.epoch}).to_string());
                for id in ids {
                    if let Some(t) = tiles.iter().find(|t| t.id == id) {
                        let _ = tx.send(
                            json!({"type": "tile", "rev": rev, "tile": t.summary()}).to_string(),
                        );
                    }
                }
                resumed = true;
            }
        }
        if !resumed {
            let rev = ev.rev;
            let _ = tx.send(self.snapshot_of(&tiles, rev).to_string());
        }
        ev.clients.push(tx);
        rx
    }

    /// A file of tile `id` for the text pane: the tile's own document, or
    /// (`file`) one its last build read, and whether it may be written
    /// (library files never are).  Nothing else on disk is served.
    ///
    /// # Errors
    /// The file is one the tile has, but it could not be read.
    fn text(&self, id: &str, file: Option<&str>) -> Result<Option<Value>, String> {
        let path = {
            let tiles = lock(&self.tiles);
            let Some(t) = tiles.iter().find(|t| t.id == id) else {
                return Ok(None);
            };
            match file {
                None => t.path.clone(),
                Some(f) if f == t.path || t.files.contains_key(f) => f.to_owned(),
                Some(_) => return Ok(None),
            }
        };
        if !path.ends_with(".dtile") {
            return Ok(None);
        }
        let text = dt_eval::Source::read(&dt_eval::Files, &path)?;
        let writable = !path.starts_with(dt_eval::LIB)
            && std::fs::metadata(&path).is_ok_and(|m| !m.permissions().readonly());
        Ok(Some(
            serde_json::json!({ "file": path, "text": text, "writable": writable }),
        ))
    }

    fn mesh(&self, id: &str) -> Option<Arc<Vec<u8>>> {
        lock(&self.tiles).iter().find(|t| t.id == id)?.mesh.clone()
    }

    fn sketch_mesh(&self, id: &str, sketch: u64) -> Option<Arc<Vec<u8>>> {
        lock(&self.tiles)
            .iter()
            .find(|t| t.id == id)?
            .sketches
            .iter()
            .find(|k| k.id == sketch)
            .map(|k| Arc::clone(&k.mesh))
    }

    /// The build cache for `path` (a draft's, or a full build's), made
    /// newest; the oldest beyond [`BUILD_CACHES`] is dropped.
    fn build_cache(&self, path: &str, draft: bool) -> Arc<dt_eval::Cache> {
        let mut caches = lock(&self.build_caches);
        let cache = caches
            .iter()
            .position(|(p, d, _)| p == path && *d == draft)
            .and_then(|k| caches.remove(k))
            .map_or_else(|| Arc::new(dt_eval::Cache::new()), |(_, _, c)| c);
        caches.push_front((path.to_owned(), draft, Arc::clone(&cache)));
        caches.truncate(BUILD_CACHES);
        cache
    }

    /// Keep stage `stage`'s sketch for tile `id`, in place of its last.
    fn add_sketch(&self, id: &str, stage: &str, rough: bool, mesh: Vec<u8>) {
        let sketch = self.sketch_seq.fetch_add(1, Ordering::Relaxed) + 1;
        self.update(id, |t| {
            t.sketches.retain(|k| k.stage != stage);
            t.sketches.push(Sketch {
                stage: stage.to_owned(),
                id: sketch,
                rough,
                mesh: Arc::new(mesh),
            });
        });
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
    let (roots, files) = match roots_for(paths) {
        Ok(r) => r,
        Err(e) => {
            tell!("error: {e}");
            return crate::ERROR;
        }
    };
    let home = match roots::personal_home() {
        Ok(h) => h,
        Err(e) => {
            tell!("error: {e}");
            return crate::ERROR;
        }
    };
    let cache = crate::library::project().out;
    let shared = Arc::new(
        Shared::with_roots(roots, &files, opts)
            .with_home(home)
            .with_cache(Timings::file_in(&cache))
            .with_palette_opt(palette),
    );
    let url = format!("http://{addr}/");
    tell!("preview: {url}  (Ctrl-C to stop)");
    for (i, r) in shared.roots.iter().enumerate() {
        let n = lock(&shared.tiles).iter().filter(|t| t.root == i).count();
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
    if open {
        open_browser(&url);
    }
    work(&shared, true, &AtomicBool::new(false));
    crate::OK
}

#[cfg(test)]
mod tests;
