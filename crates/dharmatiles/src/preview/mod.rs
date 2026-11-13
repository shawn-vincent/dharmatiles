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
//! | `POST /api/tiles/ID/build[?debug=1\|0]` | build a tile that has not been built yet; `debug` says whether the page has the debug layer on for it (its marks are collected only then; turned on for a built tile, it is built again) |
//! | `POST /api/tiles/ID/draft` `{text, file?, debug?}` | build the posted text quickly; the reply is its mesh |
//! | `GET /api/events?epoch=E&since=N` | server-sent events: a snapshot (or, resuming, what changed since revision N), then one event per change; every kept [message](crate::message) follows it, then each as it is said (`message` events) |
//! | `GET /api/messages?tile=&since=&format=text\|json` | the stream's messages, as the page and an assistant read them: a tile's (id, path or name), after sequence number `since`; also `state=open\|cleared`, `kind=`, `severity=` |
//! | `GET /api/session` | this run's token, which every other `POST` must carry |
//! | `GET /api/about` | the application version and the tile formats it reads, for the About box |
//! | `GET /api/elements` | the elements and their controls |
//! | `GET /api/language` | the tile's nodes with their controls, and the language's units, functions and `$` names |
//! | `GET /api/weightings?range=` / `?text=` | the weighting functions, each with its curve ([`crate::weightings`]) |
//! | `GET /api/tiles/ID/tier?tier=super-low\|low\|full[&seed=N&system=db\|ol&printer=NAME]` | a tile's mesh at a tier for a map's piece (from the cache or built; lean: no stack, no clouds) |
//! | `POST /api/maps/…` | a map's edits: add, set, batch, dup, remove, pin, unpin, unpin-all ([`maps`]); each answers with Undo's `before` and the map's new `scene` ([`scene`]) |
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
pub(crate) mod http;
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
pub(crate) mod api;
mod calls;
mod docs;
mod edit;
pub(crate) mod fail;
mod inventory;
mod lists;
mod map;
mod maps;
mod messages;
mod offers;
pub mod payload;
pub(crate) mod places;
mod placing;
mod plates;
mod printers;
pub(crate) mod roots;
mod scene;
mod server;
mod stack;
mod tiers;
mod worker;

pub use places::Picker;
pub use roots::{Role, Root, RootKind, roots_for};
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

use crate::tile_json;
use crate::watch::{Recording, mtime};

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

/// A mesh of another revision of the document than the one asked for: 409 with the revision it is of.
fn stale_mesh(rev: u64) -> (&'static str, Value) {
    fail::refused_with(
        "409 Conflict",
        "that is not the revision of the document the mesh is of",
        |b| b.doc_rev = Some(rev),
    )
}

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
    /// The size the document writes for the tile (`2x1`), read when the tile
    /// is listed: for the folder rows, before any build.
    size: Option<String>,
    status: State,
    /// A print list: the plate in view, from 0.
    plate: usize,
    /// What a map's last reading said that is not a mistake (a crown over a taller neighbour): kept so a
    /// check of the text that finds nothing wrong does not take it from the stream.
    scene_notes: Vec<scene::Note>,
    plan: Option<Value>,
    record: Option<Value>,
    errors: Vec<api::Mistake>,
    /// A build that panicked or could not be encoded, as the page gets it
    /// ([`fail::ServerError`]); the page shows it in the error panel.
    failure: Option<api::ErrorDetail>,
    mesh: Option<Arc<Vec<u8>>>,
    mesh_version: u64,
    /// The revision of the document the mesh is of (the tile's file, or the print list's): a mesh is asked for by it,
    /// and one of another revision is refused, so an older build cannot be shown for a newer text.
    doc_rev: Option<u64>,
    /// The build's progress (a `progress` event's body) while building.
    progress: Option<api::Progress>,
    /// Files the last build read, with their times.
    files: BTreeMap<String, Option<SystemTime>>,
    /// Built for a target that is not the chosen one any more.
    stale: bool,
    /// While the first build runs: the mesh on show is not this tile's
    /// build but one the cache kept (`{"kind": "earlier" | "preview",
    /// "note": …}`), so the page says "rebuilding".  Gone when the build ends.
    shown: Option<Value>,
    /// The page has the debug layer on for this tile: its builds collect the
    /// layer's marks (grass starts, spines) too.  Set by `POST …/build?debug=1`.
    marks: bool,
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
            .trim_end_matches(".dmap")
            .trim_end_matches(".dinv")
            .to_owned();
        Self {
            id: tile_id(path),
            path: path.to_owned(),
            name,
            root,
            group,
            size: None,
            status: State::Idle,
            plate: 0,
            scene_notes: Vec::new(),
            plan: None,
            record: None,
            errors: Vec::new(),
            failure: None,
            mesh: None,
            mesh_version: 0,
            doc_rev: None,
            progress: None,
            files: BTreeMap::new(),
            stale: false,
            shown: None,
            marks: false,
            sketches: Vec::new(),
        }
    }

    fn summary(&self) -> api::TileSummary {
        api::TileSummary {
            id: self.id.clone(),
            path: self.path.clone(),
            name: self.name.clone(),
            root: self.root,
            group: self.group.clone(),
            size: self.size.clone(),
            status: self.status.as_str().to_owned(),
            plan: self.plan.as_ref().map(api::camel_keys),
            record: self.record.as_ref().map(api::camel_keys),
            errors: self.errors.clone(),
            failure: self.failure.clone(),
            progress: self.progress.clone(),
            mesh_version: self.mesh.as_ref().map(|_| self.mesh_version),
            doc_rev: self.mesh.as_ref().and(self.doc_rev),
            mesh_bytes: self.mesh.as_ref().map(|m| m.len()),
            sketches: self
                .sketches
                .iter()
                .map(|k| api::SketchRef {
                    id: k.id,
                    rough: k.rough,
                })
                .collect(),
            shown: self.shown.clone(),
        }
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
    /// What happened to each tile ([`crate::message`]): kept here so it is sent in step with the changes.
    messages: crate::message::Log,
}

impl Events {
    fn send<T: serde::Serialize>(&mut self, msg: &T) {
        let msg = match serde_json::to_string(msg) {
            Ok(m) => m,
            Err(e) => {
                tell!("error: preview: an event could not be written: {e}");
                return;
            }
        };
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
    /// Where the tiles come from; a slot's `root` indexes this.  A place the
    /// user adds is appended, one they remove is taken out (and the slots'
    /// indexes after it move down: see [`places`]).  Locked after the tiles.
    roots: Mutex<Vec<Root>>,
    /// The folders of each folder place, as paths below it (empty ones too):
    /// what the last scan found, so a new or removed folder is told.
    folders: Mutex<BTreeMap<usize, Vec<String>>>,
    /// Held while the places are scanned or changed: one at a time.
    gate: Mutex<()>,
    /// When the folders were last rescanned.
    scanned: Mutex<Option<Instant>>,
    /// Where the per-user settings live (the added places); none in the
    /// tests that must not touch them.
    settings: Option<std::path::PathBuf>,
    /// The Trash a tile or folder is moved to; none where there is none known.
    trash: Option<std::path::PathBuf>,
    /// What is put in the Trash, for an Undo that names it by number: (where it was, where it is).
    trashed: Mutex<Vec<(std::path::PathBuf, std::path::PathBuf)>>,
    /// How a folder is chosen for a new place.
    picker: Picker,
    /// This run's secret: the editor's calls carry it (see `route`).
    token: String,
    /// Each sketch's id, unique across tiles.
    sketch_seq: std::sync::atomic::AtomicU64,
    /// The build caches of the tiles built last, newest first, by path
    /// and whether they are drafts' (see [`dt_eval::Cache`]).
    build_caches: Mutex<VecDeque<(String, bool, Arc<dt_eval::Cache>)>>,
    /// The disk cache of builds ([`dt_eval::Store`]): a tile whose inputs are
    /// unchanged opens from it, and an earlier build of one that changed
    /// shows while the new one runs.  None: no disk cache (the tests').
    store: Option<Arc<dt_eval::Store>>,
    /// The inventory as the last snapshot told it (text), so a change made
    /// outside the program (the file edited by hand) is told too.
    inventory_seen: Mutex<String>,
    /// The tiers the page wants of a map's pieces, what is building and what is built ([`tiers`]).
    tier_book: Mutex<tiers::TierBook>,
    /// The owner of each document file: its revision and its lock ([`docs`]).
    docs: docs::Docs,
}

/// Held while a tile builds: dropping it ends the build's progress message.
struct BuildingGuard {
    shared: Arc<Shared>,
    id: TileId,
}

impl Drop for BuildingGuard {
    fn drop(&mut self) {
        let mut ev = lock(&self.shared.events);
        let gone = ev.messages.end_progress(&self.id);
        Shared::send_messages(&mut ev, &gone);
    }
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
                messages: crate::message::Log::default(),
            }),
            epoch: edit::token()[..12].to_owned(),
            queue: Mutex::new(queue::Queue::default()),
            queue_cv: std::sync::Condvar::new(),
            cache: None,
            palette: None,
            roots: Mutex::new(roots),
            folders: Mutex::new(BTreeMap::new()),
            gate: Mutex::new(()),
            scanned: Mutex::new(None),
            settings: None,
            trash: None,
            trashed: Mutex::new(Vec::new()),
            picker: Picker::Native,
            token: edit::token(),
            sketch_seq: std::sync::atomic::AtomicU64::new(0),
            build_caches: Mutex::new(VecDeque::new()),
            store: None,
            inventory_seen: Mutex::new(String::new()),
            tier_book: Mutex::new(tiers::TierBook::default()),
            docs: docs::Docs::default(),
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

    /// Keep the added places in the settings folder `dir`.
    #[must_use]
    pub fn with_settings(mut self, dir: std::path::PathBuf) -> Self {
        self.settings = Some(dir);
        self
    }

    /// Keep builds in the disk cache `store`, and open from it.
    #[must_use]
    pub fn with_store(mut self, store: Option<dt_eval::Store>) -> Self {
        self.store = store.map(Arc::new);
        self
    }

    /// Move what is trashed into `dir`.
    #[must_use]
    pub fn with_trash(mut self, dir: std::path::PathBuf) -> Self {
        self.trash = Some(dir);
        self
    }

    /// Choose a folder for a new place with `picker`.
    #[must_use]
    pub fn with_picker(mut self, picker: Picker) -> Self {
        self.picker = picker;
        self
    }

    /// The places now (a copy: they can change while it is read).
    fn roots(&self) -> Vec<Root> {
        lock(&self.roots).clone()
    }

    /// Whether the page has the debug layer on for tile `id`: its builds
    /// collect the layer's marks.
    fn marks_of(&self, id: &str) -> bool {
        lock(&self.tiles).iter().any(|t| t.id == id && t.marks)
    }

    /// The page turned the debug layer on or off for tile `id`.  Turned on
    /// for a tile already built, the tile is built again (the scenes its last
    /// build kept hold no marks); turned off, nothing is rebuilt: the page
    /// hides the layer and the builds after this one leave the marks out.
    /// False when there is no such tile.
    fn set_marks(&self, id: &str, on: bool) -> bool {
        let mut tiles = lock(&self.tiles);
        let Some(t) = tiles.iter_mut().find(|t| t.id == id) else {
            return false;
        };
        let was = std::mem::replace(&mut t.marks, on);
        if on && !was && t.status != State::Idle {
            t.stale = true;
        }
        drop(tiles);
        self.queue_cv.notify_all();
        true
    }

    /// The options every build is made with: the chosen target.
    fn options(&self) -> Options {
        lock(&self.target).clone()
    }

    /// Tell every page the state again (the printers there are changed on disk).
    fn announce(&self) {
        let tiles = lock(&self.tiles);
        let mut ev = lock(&self.events);
        let rev = ev.bump(Change::List);
        let snap = self.snapshot_of(&tiles, rev);
        ev.send(&snap);
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

    fn with_trash_opt(self, dir: Option<std::path::PathBuf>) -> Self {
        match dir {
            Some(d) => self.with_trash(d),
            None => self,
        }
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
        crate::library::read_palette(f)
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
    fn palette_problem(&self) -> Vec<api::Mistake> {
        api::mistakes(&self.palette_diags())
    }

    /// [`Shared::palette_problem`] as diagnostics.
    fn palette_diags(&self) -> Vec<Diag> {
        match self.palette_checked() {
            Ok(_) => Vec::new(),
            Err(e) => vec![Diag::plain(
                "dt::palette",
                format!("{e}; the tile is shown in the default paint"),
            )],
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
        let body = api::Progress {
            fraction: status.fraction,
            label: status.label.clone(),
            key: status.key.clone(),
            count: status.count.clone(),
            elapsed_s: status.elapsed_s,
            eta_s: Some(status.eta_s),
            estimate_s: Some(status.estimate_s),
        };
        let mut tiles = lock(&self.tiles);
        if let Some(slot) = tiles.iter_mut().find(|t| t.id == id) {
            slot.progress = Some(body.clone());
            let mut ev = lock(&self.events);
            let rev = ev.rev;
            ev.send(&json!({"type": "progress", "rev": rev, "id": id, "progress": body}));
        }
    }

    /// A tile's name (for a message), or none when it is not listed.
    fn name_of(&self, id: &str) -> Option<String> {
        lock(&self.tiles)
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.name.clone())
    }

    /// Tell every listener of `changed` messages.
    fn send_messages(ev: &mut Events, changed: &[crate::message::Message]) {
        let rev = ev.rev;
        for m in changed {
            ev.send(&json!({"type": "message", "rev": rev, "message": m.json()}));
        }
    }

    /// What tile `id` reports for `scope` now (`make` is given its name): what
    /// it no longer reports is cleared, what it says again stays, the rest is
    /// new; every listener is told what changed.  A tile that is not listed
    /// reports nothing.
    fn report(
        &self,
        id: &str,
        scope: crate::message::Scope,
        make: impl FnOnce(&str) -> Vec<crate::message::Message>,
    ) {
        let Some(name) = self.name_of(id) else { return };
        let found = make(&name);
        let mut ev = lock(&self.events);
        let changed = ev.messages.report(fail::now_ms(), id, scope, found);
        Self::send_messages(&mut ev, &changed);
    }

    /// One message with nothing to clear (a failure of the server's or the page's), said now.
    fn say(&self, m: crate::message::Message) {
        let mut ev = lock(&self.events);
        let m = ev.messages.say(fail::now_ms(), m);
        Self::send_messages(&mut ev, &[m]);
    }

    /// A build of tile `id` begins: a message says so until the guard drops
    /// (whatever way the build ends); it is never kept.
    fn building(self: &Arc<Self>, id: &str) -> Option<BuildingGuard> {
        use crate::message::{Kind, Message, Scope, Severity};
        let name = self.name_of(id)?;
        let mut ev = lock(&self.events);
        let m = Message::new(
            Kind::Progress,
            Severity::Info,
            Scope::Build,
            id,
            &name,
            format!("Building {name}"),
        );
        let m = ev.messages.begin_progress(fail::now_ms(), m);
        Self::send_messages(&mut ev, &[m]);
        Some(BuildingGuard {
            shared: Arc::clone(self),
            id: id.to_owned(),
        })
    }

    /// Every tile's state: the `/api/tiles` body and the first event.
    fn snapshot_of(&self, tiles: &[Slot], rev: u64) -> api::Snapshot {
        let target = self.options();
        api::Snapshot {
            kind: "snapshot".to_owned(),
            rev,
            epoch: self.epoch.clone(),
            system: target.system,
            printer: target.printer.name.clone(),
            printers: plates::printers_json(self.home.as_deref())
                .iter()
                .map(api::camel_keys)
                .collect(),
            overhang_deg: target.printer.overhang_deg,
            steepest_deg: target.printer.steepest_deg,
            roots: lock(&self.roots)
                .iter()
                .enumerate()
                .map(|(i, r)| r.json(i))
                .collect(),
            folders: lock(&self.folders)
                .iter()
                .flat_map(|(r, v)| {
                    v.iter().map(move |p| api::FolderJson {
                        root: *r,
                        path: p.clone(),
                    })
                })
                .collect(),
            home: std::env::var("HOME").ok(),
            inventory: api::camel_keys(&inventory::snapshot(self.home.as_deref())),
            tiles: tiles.iter().map(Slot::summary).collect(),
        }
    }

    /// The snapshot as JSON, the way a test reads it.
    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    fn snapshot_json(&self) -> Value {
        serde_json::to_value(self.snapshot()).unwrap()
    }

    fn snapshot(&self) -> api::Snapshot {
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
        let _one_at_a_time = lock(&self.gate);
        self.rescan_held()
    }

    /// [`Shared::rescan`], with the gate held by the caller.
    fn rescan_held(&self) -> bool {
        let roots = self.roots();
        let found: Vec<(usize, Vec<String>)> = roots
            .iter()
            .enumerate()
            .filter_map(|(r, root)| Some((r, root.scan()?)))
            .collect();
        let folders: BTreeMap<usize, Vec<String>> = roots
            .iter()
            .enumerate()
            .filter_map(|(r, root)| Some((r, root.scan_folders()?)))
            .collect();
        let folders_changed = {
            let mut now = lock(&self.folders);
            let differs = *now != folders;
            if differs {
                *now = folders;
            }
            differs
        };
        let mut removed = Vec::new();
        // the inventory is the page's too: edited by hand, or by the command line, it is told
        let inventory_changed = {
            let now = inventory::snapshot(self.home.as_deref()).to_string();
            let mut seen = lock(&self.inventory_seen);
            let differs = *seen != now;
            if differs {
                *seen = now;
            }
            differs
        };
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
                        let mut slot = Slot::in_root(p, r, roots[r].group(p));
                        slot.size = places::size_of(p);
                        tiles.push(slot);
                    }
                }
            }
            let changed = tiles.len() != before
                || !removed.is_empty()
                || folders_changed
                || inventory_changed;
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
            lock(&self.events).messages.forget(id);
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
    fn summary(&self, id: &str) -> Option<api::TileSummary> {
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
            match serde_json::to_string(&self.snapshot_of(&tiles, rev)) {
                Ok(s) => {
                    // a page that went away between its connection and its snapshot
                    let _ = tx.send(s);
                }
                Err(e) => tell!("error: preview: the snapshot could not be written: {e}"),
            }
        }
        // every kept message follows, so a page that came (or came back) has the whole record
        for m in ev.messages.all() {
            let _ =
                tx.send(json!({"type": "message", "rev": ev.rev, "message": m.json()}).to_string());
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
    fn text(&self, id: &str, file: Option<&str>) -> Result<Option<route::TextRes>, String> {
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
        if !path.ends_with(".dtile") && !path.ends_with(".dmap") {
            return Ok(None);
        }
        // a library file is read only, and has no revision to edit against
        let (text, doc_rev) = if path.starts_with(dt_eval::LIB) {
            (dt_eval::Source::read(&dt_eval::Files, &path)?, 0)
        } else {
            self.docs
                .read(&path)
                .map_err(|e| format!("cannot read {path}: {e}"))?
        };
        let writable = !path.starts_with(dt_eval::LIB)
            && std::fs::metadata(&path).is_ok_and(|m| !m.permissions().readonly());
        Ok(Some(route::TextRes {
            file: path,
            text,
            writable,
            doc_rev,
        }))
    }

    /// The tile's mesh, if it is of document revision `doc_rev`: `Ok(None)` when there is none yet, `Err(now)` (the
    /// revision it is of) when it is of another, which is never sent as if it were this one's.
    fn mesh(&self, id: &str, doc_rev: u64) -> Result<Option<Arc<Vec<u8>>>, u64> {
        let tiles = lock(&self.tiles);
        let Some(t) = tiles.iter().find(|t| t.id == id) else {
            return Ok(None);
        };
        match (&t.mesh, t.doc_rev) {
            (Some(m), Some(r)) if r == doc_rev => Ok(Some(Arc::clone(m))),
            (Some(_), r) => Err(r.unwrap_or(0)),
            (None, _) => Ok(None),
        }
    }

    /// The revision of the document at `path` now: a library file has none (0).  A file that cannot be read is 0 too:
    /// whatever reads it next says so, with the words.
    fn rev_of(&self, path: &str) -> u64 {
        if path.starts_with(dt_eval::LIB) {
            return 0;
        }
        self.docs.read(path).map_or(0, |(_, r)| r)
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

/// The most triangles one piece of a map has at full detail in the page.
const MAP_FULL_TRIANGLES: usize = 600_000;

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
    let store = match crate::cache::open() {
        Ok(s) => s,
        Err(e) => {
            tell!("error: {e}");
            return crate::ERROR;
        }
    };
    let settings = match places::settings_dir() {
        Ok(d) => d,
        Err(e) => {
            tell!("error: {e}");
            return crate::ERROR;
        }
    };
    // the folders added with the chooser: listed after the rest, in the order they were added
    let mut roots = roots;
    match places::load_places(&settings) {
        Ok(added) => roots.extend(
            added
                .into_iter()
                .map(|d| Root::new(&places::label_of(&d), RootKind::Dir(d)).with_role(Role::Added)),
        ),
        Err(e) => {
            tell!("error: {e}");
            return crate::ERROR;
        }
    }
    let shared = Arc::new(
        Shared::with_roots(roots, &files, opts)
            .with_home(home)
            .with_settings(settings)
            .with_trash_opt(places::trash_dir())
            .with_picker(Picker::from_env())
            .with_cache(Timings::file_in(&cache))
            .with_store(store)
            .with_palette_opt(palette),
    );
    let url = format!("http://{addr}/");
    tell!("preview: {url}  (Ctrl-C to stop)");
    for (i, r) in shared.roots().iter().enumerate() {
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
    match shared.store.as_deref() {
        Some(s) => tell!(
            "build cache: {} (builds are kept there; `dharmatiles cache` says how much)",
            s.dir().display()
        ),
        None => tell!("build cache: off (DT_CACHE=off): every open builds"),
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
