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
//! | `GET /api/events?epoch=E&since=N` | server-sent events: a snapshot (or, resuming, what changed since revision N), then one event per change; every kept [message](dt_service::message) follows it, then each as it is said (`message` events) |
//! | `GET /api/messages?tile=&since=&format=text\|json` | the stream's messages, as the page and an assistant read them: a tile's (id, path or name), after sequence number `since`; also `state=open\|cleared`, `kind=`, `severity=` |
//! | `GET /api/session` | this run's token, which every other `POST` must carry |
//! | `GET /api/about` | the application version and the tile formats it reads, for the About box |
//! | `GET /api/elements` | the elements and their controls |
//! | `GET /api/language` | the tile's nodes with their controls, and the language's units, functions and `$` names |
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
//! One thread, the [`owner`], holds the tiles, the event stream, the places, the target and the queue; the
//! connections and the workers send it typed commands and wait on a reply where they need one, so there is no lock
//! order to keep.  Worker threads wait on the owner for the queue's jobs (one full build at a time, a draft beside
//! it); the owner answers the moment there is one.  A save the program makes tells the owner at once; the one
//! polling there is (`worker::watch`, every [`POLL`]) looks at the modification time of every file a build read, as
//! `watch` does, for a change made outside the program, rebuilding the tile when one changes.  Each build pushes the plan pass first (the
//! tile's size and stack, from `check`, before any geometry), then
//! the finished mesh.  Errors are pushed with their file, line and column.

mod assets;
pub(crate) mod http;
mod queue;
mod route;
mod routes;

pub(crate) mod api;
mod calls;
mod docs;
mod edit;
pub(crate) mod fail;
pub use dt_service::header;
pub(crate) mod inventory;
mod lists;
mod local_docs;
mod maps;
mod messages;
mod offers;
mod owner;
pub use dt_service::payload;
pub(crate) mod places;
mod placing;
pub(crate) mod plan;
mod plates;
pub(crate) mod printers;
mod resources;
pub(crate) mod roots;
mod scene;
mod server;
pub(crate) mod stack;
pub mod storage;
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
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use dt_eval::progress::{StageInfo, Status, Timings, Tracker, plan_stages};
use dt_eval::{Diag, Options};
use dt_service::patch::Patch;

use crate::watch::{Recording, mtime};

/// How often the files of the tiles built, and the folders, are looked at for a change made outside the program (the
/// one polling there is: see [`Shared::look_for_changes`]).
const POLL: Duration = Duration::from_millis(150);
/// How long a lane that has no work waits before it looks at whether it is to stop: a wait on the owner, which
/// answers the moment there is a job, and not a look for work.
const LANE_WAIT: Duration = Duration::from_secs(1);
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

/// A mesh of another revision of the document than the one asked for: 409 with the revision it is of.
fn stale_mesh(rev: u64) -> (&'static str, api::Body) {
    fail::refused_with(
        "409 Conflict",
        "that is not the revision of the document the mesh is of",
        |b| b.doc_rev = Some(rev),
    )
}

/// What a tile's size and seed are remembered by: its path, the seed asked, the system and its file's time.
type SceneCheck = (String, Option<u64>, dt_lang::System, Option<SystemTime>);

#[cfg(test)]
pub(crate) use dt_service::tile::State;
pub(crate) use dt_service::tile::{Facts, Row, Sketch, Slot, TileId, tile_id};

/// What the service asks of the program (a place to say what is wrong, the time): the owner runs with it.
pub(crate) struct Program;

impl dt_service::host::Alarm for Program {
    fn alarm(&self, msg: &str) {
        tell!("{msg}");
    }

    fn alarm_once(&self, msg: &str) {
        tell_once(msg);
    }
}

/// What the server and the worker share.
pub struct Shared {
    /// Where the user's own printers live and the chosen target is
    /// remembered (`$DT_HOME`); none in the tests that must not touch it.
    home: Option<std::path::PathBuf>,
    /// The tiles and the event stream: owned by one thread, asked by command ([`owner`]).
    owner: owner::Owner,
    /// This run: a stream from another run cannot resume, whatever its
    /// revision.
    epoch: String,
    /// The number of the next lane request ([`Shared::take`]).
    take_seq: std::sync::atomic::AtomicU64,
    /// Where measured stage durations are kept between builds.
    cache: Option<std::path::PathBuf>,
    /// A palette document the paint is read from at each build.
    palette: Option<std::path::PathBuf>,
    /// Held while the places are scanned or changed: one at a time.
    gate: Mutex<()>,
    /// The number of the next reading of the resources the owner cannot read ([`Shared::resources_read`]).
    read_seq: std::sync::atomic::AtomicU64,
    /// Where the per-user settings live (the added places); none in the
    /// tests that must not touch them.
    settings: Option<std::path::PathBuf>,
    /// The Trash a tile or folder is moved to; none where there is none known.
    trash: Option<std::path::PathBuf>,
    /// How a folder is chosen for a new place.
    picker: Picker,
    /// This run's secret: the editor's calls carry it (see `route`).
    token: String,
    /// Each sketch's id, unique across tiles.
    sketch_seq: std::sync::atomic::AtomicU64,
    /// The build caches of the tiles built last, newest first, by path
    /// and whether they are drafts' (see [`dt_eval::Cache`]).
    build_caches: Mutex<VecDeque<(String, bool, Arc<dt_eval::Cache>)>>,
    /// The disk cache of builds ([`dt_store::Store`]): a tile whose inputs are
    /// unchanged opens from it, and an earlier build of one that changed
    /// shows while the new one runs.  None: no disk cache (the tests').
    store: Option<Arc<dt_store::Store>>,
    /// A tile's size and seed as a map's piece reads them, by tile, seed, system and the file's time ([`scene`]).
    scene_checks: Mutex<BTreeMap<SceneCheck, (u32, u32, u64)>>,
    /// The owner of each document file: its revision and its lock ([`docs`]).
    docs: docs::Docs,
    /// Where tiles' files are read, written and trashed ([`storage`]).
    storage: Arc<dyn storage::Storage>,
}

impl dt_service::tile_calls::CheckHost for Shared {
    fn options(&self) -> Options {
        Self::options(self)
    }

    fn report(
        &self,
        id: &str,
        scope: dt_service::message::Scope,
        make: impl FnOnce(&str) -> Vec<dt_service::message::Message>,
    ) {
        Self::report(self, id, scope, make);
    }
}

impl dt_service::tile_host::TileHost for Shared {
    fn docs(&self) -> &dt_service::docs::Docs {
        self.docs.service()
    }

    fn storage(&self) -> &dyn dt_service::storage::Storage {
        self.storage.as_ref()
    }

    fn library(&self) -> &dyn dt_eval::Source {
        &dt_eval::Files
    }

    fn want(&self, id: &str) -> Result<bool, dt_service::call::Refusal> {
        Ok(Self::want(self, id))
    }

    /// The program's draft lane: queued ahead of every full build, replacing an older draft of the tile, and waited for.
    fn version(&self) -> String {
        env!("DT_ENGINE_VERSION").to_owned()
    }

    fn home(&self) -> Result<std::path::PathBuf, dt_service::call::Refusal> {
        self.home_dir()
    }

    fn listed(&self, dest: &std::path::Path) -> dt_service::place_calls::Listed {
        edit::listed(self, dest)
    }

    fn snapshot(&self) -> dt_service::api::Snapshot {
        Self::snapshot(self)
    }

    fn draft(
        &self,
        ask: dt_service::draft_calls::DraftAsk,
    ) -> Result<dt_service::draft_calls::Drafted, dt_service::call::Refusal> {
        use dt_service::call::{Refusal, Status};
        use dt_service::draft_calls::Drafted;
        match self
            .submit_draft(
                &ask.id, &ask.path, &ask.file, &ask.text, ask.marks, ask.tier,
            )
            .recv_timeout(Duration::from_secs(120))
        {
            Ok(queue::Outcome::Mesh(mesh)) => Ok(Drafted::Mesh(mesh)),
            Ok(queue::Outcome::Mistake) => Ok(Drafted::Mistake),
            Ok(queue::Outcome::Superseded) => Ok(Drafted::Superseded),
            Ok(queue::Outcome::Failed(e)) => {
                let mut r = match e.kind {
                    fail::Kind::Bug => Refusal::bug(Status::Internal, e.message, e.backtrace),
                    fail::Kind::Refused => Refusal::refused(Status::Internal, e.message),
                    fail::Kind::Failure => Refusal::failure(Status::Internal, e.message),
                };
                r.causes = e.causes;
                // what the lane was at (the draft worker, for this tile) is kept, not the request's route
                if let Some(route) = e.doing.route {
                    r = r.doing(route, e.doing.tile.as_deref());
                }
                if let (Some(bug), Some(help)) = (r.bug.as_mut(), e.help) {
                    bug.help = Some(help);
                }
                Err(r)
            }
            Err(e) => Err(Refusal::failure(
                Status::Internal,
                format!("the draft was not answered ({e}): the build worker may have died"),
            )
            .caused_by(&e)
            .doing("draft worker", Some(&ask.path))),
        }
    }
}

impl dt_service::folder_calls::PlaceHost for Shared {
    fn roots(&self) -> Vec<Root> {
        Self::roots(self)
    }

    fn trash_dir(&self) -> Option<std::path::PathBuf> {
        self.trash.clone()
    }

    fn trash_note(&self, from: &std::path::Path, to: &std::path::Path) -> usize {
        self.owner
            .ask(|r| owner::Command::Trash(from.to_path_buf(), to.to_path_buf(), r))
    }

    fn trash_taken(&self, undo: usize) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        self.owner.ask(|r| owner::Command::Trashed(undo, r))
    }

    fn rescan(&self) {
        // whether the list changed is told to every page by the rescan itself
        Self::rescan(self);
    }

    fn told(&self) -> u64 {
        Self::told(self)
    }

    fn summary(&self, id: &str) -> Option<api::TileSummary> {
        Self::summary(self, id)
    }

    fn moved(&self, from: &std::path::Path, to: &std::path::Path, folder: bool) {
        if folder {
            inventory::note_folder_rename(self, from, to);
        } else {
            inventory::note_rename(self, from, to);
        }
    }
}

impl dt_service::target_calls::TargetHost for Shared {
    fn set_target(
        &self,
        system: Option<dt_lang::System>,
        printer: Option<&str>,
    ) -> Result<dt_service::target_calls::TargetRes, dt_service::call::Refusal> {
        Self::set_target(self, system, printer).map_err(|e| {
            dt_service::call::Refusal::refused(dt_service::call::Status::BadRequest, e)
        })?;
        let o = self.options();
        Ok(dt_service::target_calls::TargetRes {
            system: o.system,
            printer: o.printer.name,
        })
    }
}

/// Held while a tile builds: dropping it ends the build's progress message.
struct BuildingGuard {
    shared: Arc<Shared>,
    id: TileId,
}

impl Drop for BuildingGuard {
    fn drop(&mut self) {
        self.shared
            .owner
            .send(owner::Command::EndProgress(self.id.clone()));
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
    dt_value::LockExt::lock_unpoisoned(m)
}

/// `$HOME`, as the places say it; none when it is not set.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: an environment variable or directory that is not set or not readable is the meaning (the default applies)"
)]
fn home_env() -> Option<String> {
    std::env::var("HOME").ok()
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
pub(crate) use dt_service::host::panic_message;

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
        let epoch = edit::token()[..12].to_owned();
        let storage: Arc<dyn storage::Storage> = Arc::new(storage::DiskStorage);
        let owner = owner::start(
            tiles,
            roots,
            opts.clone(),
            home_env(),
            epoch.clone(),
            Arc::clone(&storage),
        );
        let me = Self {
            home: None,
            owner: owner.clone(),
            epoch,
            take_seq: std::sync::atomic::AtomicU64::new(1),
            cache: None,
            palette: None,
            gate: Mutex::new(()),
            read_seq: std::sync::atomic::AtomicU64::new(1),
            settings: None,
            trash: None,
            picker: Picker::Native,
            token: edit::token(),
            sketch_seq: std::sync::atomic::AtomicU64::new(0),
            build_caches: Mutex::new(VecDeque::new()),
            store: None,
            scene_checks: Mutex::new(BTreeMap::new()),
            docs: docs::Docs::telling(owner, Arc::clone(&storage)),
            storage,
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
    pub fn with_store(mut self, store: Option<dt_store::Store>) -> Self {
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
        self.owner.ask(owner::Command::Roots)
    }

    /// Whether the page has the debug layer on for tile `id`: its builds
    /// collect the layer's marks.
    fn marks_of(&self, id: &str) -> bool {
        self.facts(id).is_some_and(|f| f.marks)
    }

    /// The page turned the debug layer on or off for tile `id`.  Turned on
    /// for a tile already built, the tile is built again (the scenes its last
    /// build kept hold no marks); turned off, nothing is rebuilt: the page
    /// hides the layer and the builds after this one leave the marks out.
    /// False when there is no such tile.
    fn set_marks(&self, id: &str, on: bool) -> bool {
        self.owner
            .ask(|r| owner::Command::SetMarks(id.to_owned(), on, r))
    }

    /// The tiles, for a test to read or arrange: run on the owner.
    #[cfg(test)]
    fn tiles_with<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Vec<Slot>) -> R + Send + 'static,
    ) -> R {
        self.owner.inspect(move |c| f(&mut c.tiles))
    }

    /// The options every build is made with: the chosen target.
    fn options(&self) -> Options {
        self.owner.ask(owner::Command::Target)
    }

    /// Tell every page the state again (the printers there are changed on disk).
    fn announce(&self) {
        self.publish(true);
    }

    /// Choose the build target: the base system and the printer (either
    /// may be left as it is).  Every tile built so far is built again for
    /// it, the page is sent the new state, and the choice is remembered in
    /// the user's home.
    ///
    /// # Errors
    /// In words: a system that is not `db` or `ol`, a printer that is not
    /// there.
    fn set_target(
        &self,
        system: Option<dt_lang::System>,
        printer: Option<&str>,
    ) -> Result<(), String> {
        let now = self.options();
        let system = system.unwrap_or(now.system);
        let printer = printer.unwrap_or(&now.printer.name);
        let mut next = crate::library::options_in(system, printer, None, self.home.as_deref())?;
        next.engine = now.engine.clone();
        if next.system == now.system && next.printer.name == now.printer.name {
            return Ok(());
        }
        if let Some(h) = &self.home {
            crate::library::remember_target(h, system, printer)?;
        }
        self.owner.send(owner::Command::SetTarget(next));
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
        match self.palette_checked() {
            Ok(p) => p,
            Err(e) => {
                tell_once(&format!("{e}; using the default paint"));
                dt_kit::Palette::default()
            }
        }
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
                dt_lang::codes::PALETTE,
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
            what: "building".to_owned(),
            fraction: status.fraction,
            label: status.label.clone(),
            key: status.key.clone(),
            count: status.count.clone(),
            elapsed_s: status.elapsed_s,
            eta_s: Some(status.eta_s),
            estimate_s: Some(status.estimate_s),
        };
        self.tell_progress(id, body, false);
    }

    /// Work in flight for document `id`, said: remembered for a browser that connects later, and sent now.  A job (a
    /// map's pieces loading) does not take the place of the document's own build once that has a stage to say.
    fn tell_progress(&self, id: &str, body: api::Progress, job: bool) {
        self.owner.send(owner::Command::Progress {
            id: id.to_owned(),
            body: Box::new(body),
            job,
        });
    }

    /// What the owner knows of tile `id` besides its summary; none when it is not listed.
    fn facts(&self, id: &str) -> Option<Facts> {
        self.owner.ask(|r| owner::Command::Facts(id.to_owned(), r))
    }

    /// A tile's name (for a message), or none when it is not listed.
    fn name_of(&self, id: &str) -> Option<String> {
        self.facts(id).map(|f| f.name)
    }

    /// The path of tile `id` as the program names it (a path in My tiles, or `lib:` in the library).
    fn path_of(&self, id: &str) -> Option<String> {
        self.facts(id).map(|f| f.path)
    }

    /// What tile `id` reports for `scope` now (`make` is given its name): what
    /// it no longer reports is cleared, what it says again stays, the rest is
    /// new; every listener is told what changed.  A tile that is not listed
    /// reports nothing.
    fn report(
        &self,
        id: &str,
        scope: dt_service::message::Scope,
        make: impl FnOnce(&str) -> Vec<dt_service::message::Message>,
    ) {
        let Some(name) = self.name_of(id) else { return };
        self.owner.send(owner::Command::Report {
            id: id.to_owned(),
            scope,
            found: make(&name),
        });
    }

    /// One message with nothing to clear (a failure of the server's or the page's), said now.
    fn say(&self, m: dt_service::message::Message) {
        self.owner.send(owner::Command::Say(Box::new(m)));
    }

    /// A build of tile `id` begins: a message says so until the guard drops
    /// (whatever way the build ends); it is never kept.
    fn building(self: &Arc<Self>, id: &str) -> Option<BuildingGuard> {
        self.name_of(id)?;
        self.owner.send(owner::Command::BeginMessage(owner::Begin {
            id: id.to_owned(),
            text: |name| format!("Building {name}"),
            code: None,
        }));
        Some(BuildingGuard {
            shared: Arc::clone(self),
            id: id.to_owned(),
        })
    }

    /// What a snapshot holds that is read from the disk (the owner makes the rest): the printers and the inventory.
    fn snapshot_outside(&self) -> owner::Outside {
        owner::Outside {
            printers: self.printers_value(),
            inventory: self.inventory_value(),
        }
    }

    /// Tell what changed (the resources that differ from what the pages were last told) and say the stream's
    /// revision after it: an answer carries this, never the state itself, so the page takes the change from the
    /// stream, the one path every change reaches it by.
    fn told(&self) -> u64 {
        self.publish(false);
        self.owner.ask(owner::Command::Rev)
    }

    /// Every tile's state and the rest of what the page is told: what the owner holds, then what is read from disk.
    fn snapshot(&self) -> api::Snapshot {
        let held = self.owner.ask(owner::Command::Snapshot);
        held.with(self.snapshot_outside())
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
            .filter_map(|(r, root)| Some((r, root.scan(self.storage.as_ref(), &Program)?)))
            .collect();
        let folders: BTreeMap<usize, Vec<String>> = roots
            .iter()
            .enumerate()
            .filter_map(|(r, root)| Some((r, root.scan_folders(self.storage.as_ref())?)))
            .collect();
        let folders_changed = self.owner.ask(|r| owner::Command::SetFolders(folders, r));
        // the inventory is the page's too: edited by hand, or by the command line, it is told
        let inventory_changed = {
            let now = inventory::snapshot(self.home.as_deref());
            self.owner
                .ask(|r| owner::Command::SeeInventory(Box::new(now), r))
        };
        // the tiles the owner has, so only a new one is read for its size (the disk is read here, not there)
        let have: BTreeSet<(usize, String)> =
            self.owner.ask(owner::Command::Paths).into_iter().collect();
        let mut fresh = Vec::new();
        for (r, paths) in &found {
            for p in paths {
                if !have.contains(&(*r, p.clone())) {
                    let mut slot = Slot::in_root(p, *r, roots[*r].group(p));
                    slot.size = places::size_of(p);
                    fresh.push(slot);
                }
            }
        }
        let (seq, read) = self.resources_read(false);
        self.owner.ask(|reply| owner::Command::Reconcile {
            found,
            fresh,
            changed_elsewhere: folders_changed || inventory_changed,
            seq,
            read,
            reply,
        })
    }

    /// Change tile `id` and tell every listening browser; a tile that is
    /// not listed is left alone.
    fn update(&self, id: &str, patch: Patch) {
        self.owner.edit(id, patch);
    }

    /// The tile `id` as the page reads it, for a call that makes or moves
    /// one (the reply carries it, so the page need not wait for an event).
    fn summary(&self, id: &str) -> Option<api::TileSummary> {
        self.owner
            .ask(|r| owner::Command::Summary(id.to_owned(), r))
    }

    /// A new listener.  With the epoch and last revision of a stream that
    /// dropped, and the owner still holding everything since, it is sent
    /// only what changed; otherwise the snapshot.
    fn subscribe(&self, from: &route::Resume) -> owner::Subscription {
        let sub = self.owner.ask(|reply| owner::Command::Subscribe {
            epoch: from.epoch.clone(),
            since: from.since,
            reply,
        });
        let mut head: VecDeque<String> = sub.lead.into();
        if let Some(held) = sub.fresh {
            // read after the listener was registered: what it says is of that moment or later
            let whole = held.with(self.snapshot_outside());
            head.extend(owner::encode(&api::StreamEvent::Snapshot(Box::new(whole))));
        }
        head.extend(sub.messages);
        owner::Subscription::new(head, sub.rx)
    }

    /// A file of tile `id` for the text pane: the tile's own document, or
    /// (`file`) one its last build read, and whether it may be written
    /// (library files never are).  Nothing else on disk is served.
    ///
    /// # Errors
    /// The file is one the tile has, but it could not be read.
    fn text(
        &self,
        id: &str,
        file: Option<&str>,
    ) -> Result<Option<route::TextRes>, dt_service::call::Refusal> {
        let Some(t) = self.facts(id) else {
            return Ok(None);
        };
        let tile = api::TileRef {
            id: id.to_owned(),
            path: t.path,
            files: t.files,
        };
        dt_service::read_calls::text(self, &tile, file)
    }

    /// The tile's mesh, if it is of document revision `doc_rev`: `Ok(None)` when there is none yet, `Err(now)` (the
    /// revision it is of) when it is of another, which is never sent as if it were this one's.
    fn mesh(&self, id: &str, doc_rev: u64) -> Result<Option<Arc<Vec<u8>>>, u64> {
        self.owner
            .ask(|r| owner::Command::Mesh(id.to_owned(), doc_rev, r))
    }

    /// The revision of the document at `path` now: a library file has none (0).  A file that cannot be read is 0 too:
    /// whatever reads it next says so, with the words.
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: a best-effort filesystem probe (a file that cannot be read has revision 0; whatever reads it next reports it with the words)"
    )]
    fn rev_of(&self, path: &str) -> u64 {
        if path.starts_with(dt_eval::LIB) {
            return 0;
        }
        self.docs.read(path).map_or(0, |(_, r)| r)
    }

    fn sketch_mesh(&self, id: &str, sketch: u64) -> Option<Arc<Vec<u8>>> {
        self.owner
            .ask(|r| owner::Command::SketchMesh(id.to_owned(), sketch, r))
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
        let stage = stage.to_owned();
        self.update(
            id,
            Patch::Sketch(Sketch {
                stage,
                id: sketch,
                rough,
                mesh: Arc::new(mesh),
            }),
        );
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
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a best-effort filesystem probe (a missing or unreadable file means none yet, read again on the next call)"
)]
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
    let addr = match listener.local_addr() {
        Ok(a) => a.to_string(),
        Err(e) => {
            tell!("error: cannot read the address the preview is listening on: {e}");
            return crate::ERROR;
        }
    };
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
    match places::load_places(&storage::DiskStorage, &settings) {
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
        let n = shared.rows().iter().filter(|t| t.root == i).count();
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
