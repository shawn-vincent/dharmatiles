//! The editor's service in a web worker: the same verbs the program answers over HTTP, called by name with a JSON
//! request, over a [`MemStorage`] holding the tiles the page hands it.
//!
//! `Service::call(verb, tile, request)` is the whole transport: it reads the request as the verb's type, calls the
//! service's [`Table`], and writes the answer as the verb's response type.  `check`, `edit`, `save` and `build` are the
//! program's own (`dt_service::tile_calls`, `edit_calls`, `build_calls`: one function each for both transports),
//! written on [`TileHost`], which this worker's [`Ctx`] implements.
//!
//! `build` is the HTTP verb with its one meaning: it asks for the tile to be built and answers at once; the worker
//! host runs what the service queued ([`Inner::run_queued`], the same `Take` a lane makes in the program) right after
//! the call returns, and the record arrives through the event stream, as for HTTP.  `events()` hands the page what the
//! stream said since the last pull (JSON stream events: a tile's state, with its record; the messages a check found).
//! One worker is one service; the library is the one [`crate::set_library`] was given.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use dt_eval::{Options, Source, Tier, TileRecord};
use dt_lang::System;
use dt_service::build_calls::{self, Accepted, BUILD, BuildReq};
use dt_service::call::{CallError, Refusal, Status, Table, Verb};
use dt_service::clock::ManualClock;
use dt_service::draft_calls::{
    self, DRAFT, DraftAsk, DraftReq, Drafted, Mesh, STEP, StepReq, Stepped,
};
use dt_service::edit_calls::{self, EDIT, EditReq, EditRes, SAVE, SaveReq, SaveRes};
use dt_service::folder_calls::{
    self, DELETE_TILE, DUPLICATE_TILE, Deleted, FolderDone, FolderNamedReq, FolderReq, ListedIn,
    MOVE_TILE, NEW_FOLDER, PlaceHost, RENAME, RENAME_FOLDER, RenameReq, Restored, TRASH_FOLDER,
    TRASH_TILE, TileMoveReq, TrashUndoReq, Trashed, UNDO_TRASH,
};
use dt_service::host::{Alarm, Host};
use dt_service::list_calls::{
    self, LISTS_ADD, LISTS_COUNT, LISTS_NEW, LISTS_REMOVE, LISTS_RESTORE, ListEdited, ListHost,
    ListsAddReq, ListsAddRes, ListsCountReq, ListsNewReq, ListsRemoveReq, ListsRestoreReq,
};
use dt_service::map_calls::{
    self, MAPS_ADD, MAPS_BATCH, MAPS_DUP, MAPS_PIN, MAPS_REMOVE, MAPS_SET, MAPS_UNPIN,
    MAPS_UNPIN_ALL, MapsAddReq, MapsBatchReq, MapsBatched, MapsDupReq, MapsListReq, MapsPieceReq,
    MapsPieced, MapsPinReq, MapsSetReq,
};
use dt_service::message::{Message, Scope};
use dt_service::owner::{Command, Core, Outside, Setup, Subscribed};
use dt_service::patch::Patch;
use dt_service::payload;
use dt_service::place_calls::{self, Listed, MINE, NEW_TILE, NewReq};
use dt_service::queue::{Finished, Job, Taken, Taking};
use dt_service::read_calls::{
    self, ABOUT, AboutRes, CHECK_VALUE, CheckValueReq, CheckValueRes, ELEMENTS, LANGUAGE, NAMES,
    NamesReq, NamesRes, Nothing, OFFERS, OffersReq, OffersRes, PRINTED, PRINTED_SET, PrintedReq,
    PrintedRes, PrintedSetReq, PrintedSetRes, TEXT, TOKENS, TextReq as FileReq, TextRes, TokensReq,
    TokensRes,
};
use dt_service::roots::{Role, Root, RootKind};
use dt_service::scene_read::{SceneHost, SceneMemo};
use dt_service::storage::{MemStorage, Storage};
use dt_service::target_calls::{self, SET_TARGET, TargetHost, TargetReq, TargetRes};
use dt_service::tile::{Slot, tile_id};
use dt_service::tile_calls::{self, CHECK, CheckHost, TextReq};
use dt_service::tile_host::{TileHost, TileRef};
use dt_value::LockExt;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::{JsError, LIBRARY, MAIN, build_with, stage_total, stl_sha256, system_of};

/// The folder the worker's tiles are listed under.
const FOLDER: &str = "/memory/My tiles";

/// Where the worker's Trash is: a folder of its store, so an Undo puts a tile back.
const TRASH: &str = "/memory/.trash";

/// What a build came to, as the worker keeps it beside the stream's record: the hash of the STL's triangles (empty
/// when the gate stopped the build: only a build that passed has an STL).  `built(path)` answers it.
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BuiltRes {
    pub record: TileRecord,
    pub passed: bool,
    pub stl_bytes: usize,
    pub stl_sha256: String,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    /// The page's worker hears the stream as it happens: `globalThis.dtStream(json)`, the JSON of one `StreamEvent`.  A
    /// build runs inside one call, so its progress cannot wait for the call to end (`events()` is what the stream said
    /// by then).
    #[wasm_bindgen(js_namespace = globalThis, js_name = dtStream)]
    fn js_stream(event: &str);
}

/// Nobody listens off the browser.
#[cfg(not(target_arch = "wasm32"))]
fn js_stream(_event: &str) {}

/// A build's stages, said on the stream as they begin: the program's progress events (`dt_kit::Event`) made into the
/// one [`dt_service::api::Progress`] the page's bar reads.
struct StreamTicks {
    id: String,
    total: usize,
    begun: std::sync::atomic::AtomicUsize,
    label: std::sync::Mutex<String>,
    started_ms: u64,
}

impl StreamTicks {
    fn new(id: &str, total: usize) -> Self {
        Self {
            id: id.to_owned(),
            total,
            begun: std::sync::atomic::AtomicUsize::new(0),
            label: std::sync::Mutex::new(String::new()),
            started_ms: WebHost::default().now_ms(),
        }
    }

    fn say(&self, done: usize, part: f64, label: &str) {
        let total = self.total.max(1);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a count of stages and a time in milliseconds, far below 2^52"
        )]
        let (fraction, elapsed_s) = (
            ((done as f64 + part) / total as f64).clamp(0.0, 0.99),
            WebHost::default().now_ms().saturating_sub(self.started_ms) as f64 / 1000.0,
        );
        let event = dt_service::api::StreamEvent::Progress {
            rev: 0,
            id: self.id.clone(),
            progress: dt_service::api::Progress {
                what: "building".to_owned(),
                fraction,
                label: label.to_owned(),
                key: label.to_owned(),
                count: Some(("stages".to_owned(), done, total)),
                elapsed_s,
                eta_s: None,
                estimate_s: None,
            },
        };
        // a reporter cannot return an error, and a tick of numbers and a label cannot fail to write: if it ever does, the
        // console says so (where the worker picks it up) rather than dropping it unseen
        match serde_json::to_string(&event) {
            Ok(json) => js_stream(&json),
            Err(e) => crate::report_console(&format!("cannot write a progress tick: {e}")),
        }
    }
}

impl dt_kit::Reporter for StreamTicks {
    fn report(&self, event: &dt_kit::Event<'_>) {
        use std::sync::atomic::Ordering::Relaxed;
        match event {
            dt_kit::Event::Begin { label, .. } => {
                let n = self.begun.fetch_add(1, Relaxed);
                (*label).clone_into(&mut self.label.lock_unpoisoned());
                self.say(n, 0.0, label);
            }
            dt_kit::Event::Sub { fraction, .. } => {
                let n = self.begun.load(Relaxed).saturating_sub(1);
                let label = self.label.lock_unpoisoned().clone();
                self.say(n, *fraction, &label);
            }
            dt_kit::Event::End { .. } => {}
        }
    }
}

/// The worker as the service's host: alarms are kept for the page, and time is what the page winds.
#[derive(Default)]
struct WebHost {
    alarms: std::sync::Mutex<Vec<String>>,
}

impl Alarm for WebHost {
    fn alarm(&self, msg: &str) {
        self.alarms.lock_unpoisoned().push(msg.to_owned());
    }

    fn alarm_once(&self, msg: &str) {
        let mut a = self.alarms.lock_unpoisoned();
        if !a.iter().any(|m| m == msg) {
            a.push(msg.to_owned());
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    /// `Date.now()`: the worker's wall clock, which wasm has no `SystemTime` for.
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    fn date_now() -> f64;
}

impl Host for WebHost {
    type Job = ();
    type Failure = String;

    #[cfg(target_arch = "wasm32")]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Date.now() is whole milliseconds since 1970, positive"
    )]
    fn now_ms(&self) -> u64 {
        date_now() as u64
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn now_ms(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        #[expect(
            clippy::cast_possible_truncation,
            reason = "milliseconds since 1970 fit a u64 for 500 million years"
        )]
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the clock is after 1970")
            .as_millis() as u64;
        ms
    }
}

/// Everything one worker is: the store, the documents, the owner's state, the stream, and what builds came to.
struct Inner {
    storage: Arc<MemStorage>,
    host: Arc<WebHost>,
    roots: Vec<Root>,
    docs: dt_service::docs::Docs,
    system: Cell<System>,
    core: RefCell<Core<WebHost>>,
    stream: Receiver<String>,
    built: RefCell<BTreeMap<String, BuiltRes>>,
    ticket: Cell<u64>,
    seq: Cell<u64>,
    /// Where a map's pieces' tiles are read from: the store, then the library.
    tiles: StoreAndLib,
    /// What a reading of a map remembers between readings.
    memo: SceneMemo,
}

/// What one call runs on: the worker, the tile it is about and the options it reads it with.
struct Ctx {
    inner: Rc<Inner>,
    tile: String,
    options: Options,
}

/// The library as the page's engine reads it: every `lib:` path of what [`crate::set_library`] was given.
struct Lib;

impl Source for Lib {
    fn read(&self, path: &str) -> Result<String, String> {
        LIBRARY.with(|lib| {
            lib.borrow()
                .get(path)
                .cloned()
                .ok_or_else(|| format!("the page's library has no `{path}`"))
        })
    }
}

/// A path of the store as the page's engine reads it, and every `lib:` path from the library: what a map's pieces name.
struct StoreAndLib(Arc<MemStorage>);

impl Source for StoreAndLib {
    fn read(&self, path: &str) -> Result<String, String> {
        if path.starts_with(dt_eval::LIB) {
            return Lib.read(path);
        }
        self.0
            .read_text(Path::new(path))
            .map_err(|e| format!("cannot read `{path}`: {e}"))
    }
}

/// The library's printer profiles by name (`default`, `bambu/x1c`), as the page handed the library over.
fn library_printers() -> Vec<String> {
    LIBRARY.with(|l| {
        l.borrow()
            .keys()
            .filter_map(|k| k.strip_prefix("lib:printers/")?.strip_suffix(".dtile"))
            .map(str::to_owned)
            .collect()
    })
}

/// The document is `tile.dtile` ([`MAIN`]); every other path is a library file, as the page's engine reads them.
struct Doc<'a> {
    text: &'a str,
}

impl Source for Doc<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        if path == MAIN {
            return Ok(self.text.to_owned());
        }
        Lib.read(path)
    }
}

impl Inner {
    /// List what the places hold now: the tiles that are new join, the ones that are gone leave (the program's rescan, over the
    /// store and the library the page handed over).
    fn rescan(&self) -> Result<bool, Refusal> {
        let found: Vec<(usize, Vec<String>)> = self
            .roots
            .iter()
            .enumerate()
            .filter_map(|(r, root)| {
                Some((r, root.scan(self.storage.as_ref(), self.host.as_ref())?))
            })
            .collect();
        let fresh: Vec<Slot> = found
            .iter()
            .flat_map(|(r, paths)| {
                paths
                    .iter()
                    .map(|p| Slot::in_root(p, *r, self.roots[*r].group(p)))
            })
            .collect();
        // the folders of the places, empty ones too (the program's rescan reads them the same way)
        let folders: BTreeMap<usize, Vec<String>> = self
            .roots
            .iter()
            .enumerate()
            .filter_map(|(r, root)| Some((r, root.scan_folders(self.storage.as_ref())?)))
            .collect();
        let folders_changed = self.ask(|r| Command::SetFolders(folders, r))?;
        let seq = self.seq.get() + 1;
        self.seq.set(seq);
        self.ask(|reply| Command::Reconcile {
            found,
            fresh,
            changed_elsewhere: folders_changed,
            seq,
            read: Vec::new(),
            reply,
        })
    }

    /// Ask the owner: `make` is given where the answer goes, and the owner answers in the same step.
    fn ask<R>(&self, make: impl FnOnce(Sender<R>) -> Command<WebHost>) -> Result<R, Refusal> {
        let (tx, rx) = channel();
        self.core.borrow_mut().apply(make(tx));
        rx.try_recv().map_err(|e| {
            Refusal::failure(
                dt_service::call::Status::Internal,
                format!("the owner did not answer in the same step: {e}"),
            )
        })
    }

    /// The worker host's lane: take what the service queued, run it, and say how it went on the stream, until
    /// nothing is queued.  The program's lanes do the same on threads.
    fn run_queued(&self) -> Result<(), Refusal> {
        loop {
            let ticket = self.ticket.get() + 1;
            self.ticket.set(ticket);
            let (tx, rx) = channel();
            self.core.borrow_mut().apply(Command::Take {
                ticket,
                kind: Taking::Main { eager: false },
                reply: tx,
            });
            match rx.try_recv() {
                Ok(Taken::Job(Job::Full(id, _))) => self.build_job(&id)?,
                Ok(_) => {
                    return Err(Refusal::failure(
                        dt_service::call::Status::Internal,
                        "the worker was given a job that is not a tile's build",
                    ));
                }
                Err(TryRecvError::Empty) => {
                    // nothing queued: this lane's request does not wait on
                    self.ask(|r| Command::Withdraw(ticket, r))?;
                    return Ok(());
                }
                Err(TryRecvError::Disconnected) => {
                    return Err(Refusal::failure(
                        dt_service::call::Status::Internal,
                        "the owner's answer was dropped",
                    ));
                }
            }
        }
    }

    /// One full build: what the program's lane does with a tile, said as patches.
    fn build_job(&self, id: &str) -> Result<(), Refusal> {
        let facts = self
            .ask(|r| Command::Facts(id.to_owned(), r))?
            .ok_or_else(|| Refusal::refused(dt_service::call::Status::NotFound, "no such tile"))?;
        let edit = |patch: Patch| {
            self.core
                .borrow_mut()
                .apply(Command::Edit(id.to_owned(), Box::new(patch)));
        };
        edit(Patch::Begin);
        let files = BTreeMap::from([(facts.path.clone(), None)]);
        // a print list or the inventory is not a tile: planning plates is not in the browser editor yet, and the page is told so
        // (never a tile's build of a document that is not one)
        if list_calls::is_plates(&facts.path) || list_calls::is_inventory(&facts.path) {
            let what = if list_calls::is_plates(&facts.path) {
                "a print list's plates are"
            } else {
                "the inventory is"
            };
            edit(Patch::BuildFailed {
                errors: dt_eval::mistakes(&[dt_eval::Diag::plain(
                    dt_lang::codes::PLATES,
                    format!("{what} not planned in the browser editor yet"),
                )
                .help(
                    "the list can be edited here (its tiles, counts and settings); laying it out needs the program (dharmatiles preview)",
                )]),
                failure: None,
                files,
            });
            return Ok(());
        }
        // a library tile is read from the library the page handed over, and has no revision to edit against
        let (text, doc_rev) = if facts.path.starts_with(dt_eval::LIB) {
            (
                Lib.read(&facts.path).map_err(|e| {
                    Refusal::failure(
                        dt_service::call::Status::Internal,
                        format!("cannot read {}: {e}", facts.path),
                    )
                })?,
                0,
            )
        } else {
            let text = self
                .storage
                .read_text(Path::new(&facts.path))
                .map_err(|e| {
                    Refusal::failure(
                        dt_service::call::Status::Internal,
                        format!("cannot read {}: {e}", facts.path),
                    )
                })?;
            let rev = self
                .docs
                .read(&facts.path)
                .map_err(|e| dt_service::docs::read_failed(&facts.path, &e))?
                .1;
            (text, rev)
        };
        // a document that does not check has no stage count: its diagnostics are the build's answer, shown below
        #[expect(
            clippy::disallowed_methods,
            reason = "permitted silence: a document that does not check has no stage count; its diagnostics are what the build answers with"
        )]
        let total = stage_total(&Doc { text: &text }, self.system.get()).unwrap_or(1);
        let progress = dt_eval::Progress::new(Arc::new(StreamTicks::new(id, total)));
        match build_with(
            &Doc { text: &text },
            self.system.get(),
            None,
            0,
            Tier::Full,
            &progress,
            None,
        ) {
            Err(ds) => edit(Patch::BuildFailed {
                errors: dt_eval::mistakes(&ds),
                failure: None,
                files,
            }),
            Ok((built, opts)) => {
                let passed = built.verdict(&opts.printer).passed();
                let (stl_sha256, stl_bytes) = passed
                    .as_ref()
                    .map_or_else(|| (String::new(), 0), stl_sha256);
                let build_s = built.report.stage_secs.iter().map(|s| s.1).sum();
                let record = TileRecord::of(&built, &opts.printer, build_s);
                self.built.borrow_mut().insert(
                    facts.path.clone(),
                    BuiltRes {
                        record: record.clone(),
                        passed: passed.is_some(),
                        stl_bytes,
                        stl_sha256,
                    },
                );
                // the page's mesh, made by the painter the program's lanes use (`dt_service::payload`), kept by the owner
                // and fetched by revision (`Service::mesh`)
                let mesh = payload::from_built(
                    &built,
                    opts.printer.overhang_deg,
                    &dt_kit::Palette::default(),
                )
                .map_err(|e| {
                    Refusal::failure(
                        dt_service::call::Status::Internal,
                        format!("cannot make the view of the build: {e}"),
                    )
                    .doing("build worker", Some(&facts.path))
                })?;
                edit(Patch::BuildReady {
                    problem: Vec::new(),
                    record,
                    mesh: Arc::new(mesh),
                    doc_rev,
                    files,
                });
            }
        }
        self.core
            .borrow_mut()
            .apply(Command::Finish(Finished::Full(id.to_owned())));
        Ok(())
    }
}

impl CheckHost for Ctx {
    fn options(&self) -> Options {
        self.options.clone()
    }

    fn report(&self, _id: &str, scope: Scope, make: impl FnOnce(&str) -> Vec<Message>) {
        let id = tile_id(&self.tile);
        let name = Path::new(&self.tile)
            .file_stem()
            .map_or_else(|| self.tile.clone(), |s| s.to_string_lossy().into_owned());
        let found = make(&name);
        self.inner
            .core
            .borrow_mut()
            .apply(Command::Report { id, scope, found });
    }
}

impl TileHost for Ctx {
    fn docs(&self) -> &dt_service::docs::Docs {
        &self.inner.docs
    }

    fn storage(&self) -> &dyn Storage {
        self.inner.storage.as_ref()
    }

    fn library(&self) -> &dyn Source {
        &Lib
    }

    fn want(&self, id: &str) -> Result<bool, Refusal> {
        self.inner.ask(|r| Command::Want(id.to_owned(), r))
    }

    fn version(&self) -> String {
        crate::version()
    }

    fn home(&self) -> Result<std::path::PathBuf, Refusal> {
        let home = std::path::PathBuf::from(FOLDER);
        self.inner.storage.create_dir_all(&home).map_err(|e| {
            Refusal::failure(
                dt_service::call::Status::Internal,
                format!("cannot make {}: {e}", home.display()),
            )
        })?;
        Ok(home)
    }

    fn listed(&self, dest: &Path) -> Listed {
        let path = dest.to_string_lossy().into_owned();
        let id = tile_id(&path);
        // a rescan or a summary the owner cannot answer is a bug in the worker: the page is told by the thrown error
        self.inner
            .rescan()
            .expect("the owner answers a reconcile in the same step");
        let tile = self
            .inner
            .ask(|r| Command::Summary(id.clone(), r))
            .expect("the owner answers a summary in the same step");
        if tile.is_some() {
            self.inner
                .ask(|r| Command::Want(id.clone(), r))
                .expect("the owner answers a want in the same step");
        }
        Listed {
            path,
            id: tile.as_ref().map(|_| id),
            tile,
        }
    }

    fn snapshot(&self) -> dt_service::api::Snapshot {
        // the owner answers in the same step: a failure to is a bug in the worker, loudly
        let held = self
            .inner
            .ask(Command::Snapshot)
            .expect("the owner answers a snapshot in the same step");
        held.with(Outside {
            // a worker has no slicers to scan and no home folder to keep an inventory in
            printers: Vec::new(),
            inventory: dt_service::inventory::InventoryState::Unavailable,
        })
    }

    /// The worker's draft lane is its own turn: the draft is built here, now, from the text the page holds (a worker
    /// runs one call at a time, so no draft is ever replaced by a newer one while it waits).
    fn draft(&self, ask: DraftAsk) -> Result<Drafted, Refusal> {
        let opts = Options {
            tier: ask.tier.build(),
            debug: ask.marks,
            clouds: true,
            ..self.options.clone()
        };
        let built = match dt_eval::build_cached(
            &Doc { text: &ask.text },
            MAIN,
            &opts,
            &dt_eval::Progress::none(),
            None,
        ) {
            Ok(b) => b,
            // a mistake in the text: the check says where, the last mesh stays
            Err(_) => return Ok(Drafted::Mistake),
        };
        payload::from_draft(
            &built,
            opts.printer.overhang_deg,
            &dt_kit::Palette::default(),
        )
        .map(|mesh| Drafted::Mesh(Arc::new(mesh)))
        .map_err(|e| {
            Refusal::failure(
                dt_service::call::Status::Internal,
                format!("cannot prepare the draft's view: {e}"),
            )
            .doing("draft worker", Some(&self.tile))
        })
    }
}

impl PlaceHost for Ctx {
    fn roots(&self) -> Vec<Root> {
        self.inner.roots.clone()
    }

    fn trash_dir(&self) -> Option<std::path::PathBuf> {
        Some(std::path::PathBuf::from(TRASH))
    }

    fn trash_note(&self, from: &Path, to: &Path) -> usize {
        self.inner
            .ask(|r| Command::Trash(from.to_path_buf(), to.to_path_buf(), r))
            .expect("the owner answers a trash in the same step")
    }

    fn trash_taken(&self, undo: usize) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        self.inner
            .ask(|r| Command::Trashed(undo, r))
            .expect("the owner answers a trashed in the same step")
    }

    fn rescan(&self) {
        // whether the list changed is on the stream: the page is told by it
        self.inner
            .rescan()
            .expect("the owner answers a reconcile in the same step");
    }

    fn told(&self) -> u64 {
        self.inner.seq.set(self.inner.seq.get() + 1);
        let seq = self.inner.seq.get();
        self.inner.core.borrow_mut().apply(Command::Publish {
            seq,
            read: Vec::new(),
        });
        self.inner
            .ask(Command::Rev)
            .expect("the owner answers a revision in the same step")
    }

    fn summary(&self, id: &str) -> Option<dt_service::api::TileSummary> {
        self.inner
            .ask(|r| Command::Summary(id.to_owned(), r))
            .expect("the owner answers a summary in the same step")
    }

    fn moved(&self, _: &Path, _: &Path, _: bool) {
        // a worker keeps no inventory of prints, so nothing follows a tile that moves
    }
}

impl SceneHost for Ctx {
    fn tiles_home(&self) -> Option<std::path::PathBuf> {
        Some(std::path::PathBuf::from(FOLDER))
    }

    fn tile_source(&self) -> &dyn Source {
        &self.inner.tiles
    }

    fn tile_paths(&self) -> Vec<String> {
        self.inner
            .ask(Command::Paths)
            .expect("the owner answers paths in the same step")
            .into_iter()
            .map(|(_, p)| p)
            .collect()
    }

    fn machine(&self, printer: &str) -> Result<dt_eval::Machine, Vec<dt_eval::Diag>> {
        dt_eval::printer_machine_in(
            &dt_eval::InSource {
                source: &Lib,
                names: &library_printers(),
            },
            printer,
        )
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: a probe where an inventory that is not there or does not read counts as none here; the snapshot says what is wrong with it"
    )]
    fn shelf(&self) -> Option<dt_doc::inventory::Inventory> {
        let path = Path::new(FOLDER).join(dt_doc::inventory::FILE);
        let text = self.inner.storage.read_text(&path).ok()?;
        dt_doc::inventory::decode(&path.to_string_lossy(), &text).ok()
    }

    fn scene_memo(&self) -> &SceneMemo {
        &self.inner.memo
    }
}

impl ListHost for Ctx {
    fn path_of(&self, id: &str) -> Option<String> {
        self.inner
            .ask(|r| Command::Facts(id.to_owned(), r))
            .expect("the owner answers facts in the same step")
            .map(|f| f.path)
    }

    fn list_changed(&self, _path: &str) {
        // the plates are not planned in the browser yet: a list's plan is not on the stream
    }

    fn scene_notes(&self, id: &str) -> Vec<dt_service::scene::Note> {
        self.inner
            .ask(|r| Command::SceneNotes(id.to_owned(), r))
            .expect("the owner answers scene notes in the same step")
    }
}

impl TargetHost for Ctx {
    fn set_target(
        &self,
        system: Option<System>,
        printer: Option<&str>,
    ) -> Result<TargetRes, Refusal> {
        if let Some(p) = printer
            && p != self.options.printer.name
        {
            return Err(Refusal::refused(
                Status::BadRequest,
                format!(
                    "the browser editor builds with the library's printer ({}) only: it has no profile called {p}",
                    self.options.printer.name
                ),
            ));
        }
        let system = system.unwrap_or(self.options.system);
        self.inner.system.set(system);
        let next = options_in(&Lib, system)?;
        let res = TargetRes {
            system,
            printer: next.printer.name.clone(),
        };
        self.inner.core.borrow_mut().apply(Command::SetTarget(next));
        Ok(res)
    }
}

impl Ctx {
    /// The tile this call is about, with the files its last build read (what a copy of a library tile pins its `use`s to).
    fn tile_ref(&self) -> TileRef {
        let id = tile_id(&self.tile);
        let files = self
            .inner
            .ask(|r| Command::Facts(id.clone(), r))
            .expect("the owner answers facts in the same step")
            .map_or_else(BTreeMap::new, |f| f.files);
        TileRef {
            id,
            path: self.tile.clone(),
            files,
        }
    }
}

fn options_in(source: &dyn Source, system: System) -> Result<Options, Refusal> {
    let mut o = Options::in_source(source).map_err(|ds| {
        Refusal::failure(
            dt_service::call::Status::Internal,
            format!(
                "the library's default printer does not read: {}",
                ds.iter()
                    .map(|d| d.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        )
    })?;
    o.system = system;
    Ok(o)
}

fn check(ctx: &Ctx, req: TextReq) -> Result<tile_calls::CheckRes, Refusal> {
    // a print list or the inventory is read by its own decoder: its mistakes, at their text
    if req.file.is_none()
        && (list_calls::is_plates(&ctx.tile) || list_calls::is_inventory(&ctx.tile))
    {
        return Ok(list_calls::check_text(ctx, &ctx.tile, &req.text));
    }
    tile_calls::check(ctx, &Doc { text: &req.text }, MAIN)
        .map_err(|e| Refusal::failure(dt_service::call::Status::Internal, e.0))
}

fn edit(ctx: &Ctx, req: EditReq) -> Result<EditRes, Refusal> {
    edit_calls::edit(ctx, req)
}

fn save(ctx: &Ctx, req: SaveReq) -> Result<SaveRes, Refusal> {
    let saved = edit_calls::save(ctx, &ctx.tile_ref(), req)?;
    // the program's documents tell its owner of a write; here the worker does, so the tile builds again
    ctx.inner
        .core
        .borrow_mut()
        .apply(Command::Saved(ctx.tile.clone()));
    Ok(saved)
}

fn build(ctx: &Ctx, req: BuildReq) -> Result<Accepted, Refusal> {
    build_calls::build(ctx, &tile_id(&ctx.tile), req)
}

fn draft(ctx: &Ctx, req: DraftReq) -> Result<Mesh, Refusal> {
    draft_calls::draft(ctx, &ctx.tile_ref(), req)
}

fn step(ctx: &Ctx, req: StepReq) -> Result<Stepped, Refusal> {
    draft_calls::step(ctx, &ctx.tile_ref(), req)
}

fn names(_: &Ctx, req: NamesReq) -> Result<NamesRes, Refusal> {
    Ok(read_calls::names(&req))
}

fn tokens(_: &Ctx, req: TokensReq) -> Result<TokensRes, Refusal> {
    read_calls::tokens(&req)
}

fn check_value(ctx: &Ctx, req: CheckValueReq) -> Result<CheckValueRes, Refusal> {
    Ok(read_calls::check_value(ctx, &req))
}

fn printed(ctx: &Ctx, req: PrintedReq) -> Result<PrintedRes, Refusal> {
    let src = Doc { text: &req.text };
    Ok(read_calls::printed(ctx, &src, MAIN, &req))
}

fn printed_set(ctx: &Ctx, req: PrintedSetReq) -> Result<PrintedSetRes, Refusal> {
    read_calls::printed_set(ctx, &req)
}

fn offers(ctx: &Ctx, req: OffersReq) -> Result<OffersRes, Refusal> {
    read_calls::offers(ctx, &Doc { text: &req.text }, MAIN, req.at)
}

fn about(ctx: &Ctx, _: Nothing) -> Result<AboutRes, Refusal> {
    Ok(read_calls::about(ctx))
}

fn elements(_: &Ctx, _: Nothing) -> Result<Vec<dt_doc::wire::ElementJson>, Refusal> {
    Ok(read_calls::elements())
}

fn language(_: &Ctx, _: Nothing) -> Result<dt_doc::wire::LanguageJson, Refusal> {
    Ok(read_calls::language())
}

fn mine(ctx: &Ctx, req: Nothing) -> Result<Listed, Refusal> {
    place_calls::mine(ctx, &ctx.tile_ref(), req)
}

fn new_tile(ctx: &Ctx, req: NewReq) -> Result<Listed, Refusal> {
    let into = folder_calls::new_tile_dir(ctx, req.root, req.dir.as_deref())?;
    place_calls::new_tile(ctx, &req, into)
}

fn new_folder(ctx: &Ctx, req: FolderNamedReq) -> Result<FolderDone, Refusal> {
    folder_calls::new_folder(ctx, req)
}

fn rename_folder(ctx: &Ctx, req: FolderNamedReq) -> Result<FolderDone, Refusal> {
    folder_calls::rename_folder(ctx, req)
}

fn trash_folder(ctx: &Ctx, req: FolderReq) -> Result<Trashed, Refusal> {
    folder_calls::trash_folder(ctx, req)
}

fn trash_tile(ctx: &Ctx, req: Nothing) -> Result<Trashed, Refusal> {
    folder_calls::trash_tile(ctx, &ctx.tile_ref(), req)
}

fn undo_trash(ctx: &Ctx, req: TrashUndoReq) -> Result<Restored, Refusal> {
    folder_calls::undo_trash(ctx, req)
}

fn move_tile(ctx: &Ctx, req: TileMoveReq) -> Result<ListedIn, Refusal> {
    folder_calls::move_tile(ctx, &ctx.tile_ref(), req)
}

fn duplicate_tile(ctx: &Ctx, req: Nothing) -> Result<ListedIn, Refusal> {
    folder_calls::duplicate_tile(ctx, &ctx.tile_ref(), req)
}

fn rename(ctx: &Ctx, req: RenameReq) -> Result<Listed, Refusal> {
    folder_calls::rename(ctx, &ctx.tile_ref(), req)
}

fn delete_tile(ctx: &Ctx, req: Nothing) -> Result<Deleted, Refusal> {
    folder_calls::delete_tile(ctx, &ctx.tile_ref(), req)
}

fn set_target(ctx: &Ctx, req: TargetReq) -> Result<TargetRes, Refusal> {
    target_calls::set_target(ctx, req)
}

fn lists_new(ctx: &Ctx, req: ListsNewReq) -> Result<Listed, Refusal> {
    list_calls::new_list(ctx, req)
}

fn lists_add(ctx: &Ctx, req: ListsAddReq) -> Result<ListsAddRes, Refusal> {
    list_calls::add(ctx, req)
}

fn lists_count(ctx: &Ctx, req: ListsCountReq) -> Result<ListEdited, Refusal> {
    list_calls::set_count(ctx, req)
}

fn lists_remove(ctx: &Ctx, req: ListsRemoveReq) -> Result<ListEdited, Refusal> {
    list_calls::remove(ctx, req)
}

fn lists_restore(ctx: &Ctx, req: ListsRestoreReq) -> Result<ListEdited, Refusal> {
    list_calls::restore(ctx, req)
}

fn maps_add(ctx: &Ctx, req: MapsAddReq) -> Result<MapsPieced, Refusal> {
    map_calls::add(ctx, req)
}

fn maps_set(ctx: &Ctx, req: MapsSetReq) -> Result<ListEdited, Refusal> {
    map_calls::set(ctx, req)
}

fn maps_batch(ctx: &Ctx, req: MapsBatchReq) -> Result<MapsBatched, Refusal> {
    map_calls::batch(ctx, req)
}

fn maps_dup(ctx: &Ctx, req: MapsDupReq) -> Result<MapsPieced, Refusal> {
    map_calls::dup(ctx, req)
}

fn maps_remove(ctx: &Ctx, req: MapsPieceReq) -> Result<ListEdited, Refusal> {
    map_calls::remove(ctx, req)
}

fn maps_pin(ctx: &Ctx, req: MapsPinReq) -> Result<MapsPieced, Refusal> {
    map_calls::pin(ctx, req)
}

fn maps_unpin(ctx: &Ctx, req: MapsPieceReq) -> Result<ListEdited, Refusal> {
    map_calls::unpin(ctx, req)
}

fn maps_unpin_all(ctx: &Ctx, req: MapsListReq) -> Result<ListEdited, Refusal> {
    map_calls::unpin_all(ctx, req)
}

fn text(ctx: &Ctx, req: FileReq) -> Result<Option<TextRes>, Refusal> {
    read_calls::text(ctx, &ctx.tile_ref(), req.file.as_deref())
}

/// The verbs a worker answers, once.
fn table() -> Table<Ctx, Refusal> {
    Table::default()
        .with(&CHECK, check)
        .with(&EDIT, edit)
        .with(&SAVE, save)
        .with(&BUILD, build)
        .with(&DRAFT, draft)
        .with(&STEP, step)
        .with(&NAMES, names)
        .with(&TOKENS, tokens)
        .with(&CHECK_VALUE, check_value)
        .with(&PRINTED, printed)
        .with(&PRINTED_SET, printed_set)
        .with(&OFFERS, offers)
        .with(&ABOUT, about)
        .with(&ELEMENTS, elements)
        .with(&LANGUAGE, language)
        .with(&TEXT, text)
        .with(&MINE, mine)
        .with(&NEW_TILE, new_tile)
        .with(&NEW_FOLDER, new_folder)
        .with(&RENAME_FOLDER, rename_folder)
        .with(&TRASH_FOLDER, trash_folder)
        .with(&TRASH_TILE, trash_tile)
        .with(&UNDO_TRASH, undo_trash)
        .with(&MOVE_TILE, move_tile)
        .with(&DUPLICATE_TILE, duplicate_tile)
        .with(&RENAME, rename)
        .with(&DELETE_TILE, delete_tile)
        .with(&SET_TARGET, set_target)
        .with(&LISTS_NEW, lists_new)
        .with(&LISTS_ADD, lists_add)
        .with(&LISTS_COUNT, lists_count)
        .with(&LISTS_REMOVE, lists_remove)
        .with(&LISTS_RESTORE, lists_restore)
        .with(&MAPS_ADD, maps_add)
        .with(&MAPS_SET, maps_set)
        .with(&MAPS_BATCH, maps_batch)
        .with(&MAPS_DUP, maps_dup)
        .with(&MAPS_REMOVE, maps_remove)
        .with(&MAPS_PIN, maps_pin)
        .with(&MAPS_UNPIN, maps_unpin)
        .with(&MAPS_UNPIN_ALL, maps_unpin_all)
}

/// What a call answered: JSON, or (a draft, a step) the page's bytes.
enum Answered {
    Json(String),
    Bytes(Vec<u8>),
}

/// Read `request` as the verb's request type and call it.
fn run<Q, S>(
    table: &Table<Ctx, Refusal>,
    ctx: &Ctx,
    verb: &Verb<Q, S>,
    request: &str,
) -> Result<S, Refusal>
where
    Q: serde::de::DeserializeOwned + 'static,
    S: 'static,
{
    let req: Q = serde_json::from_str(request).map_err(|e| {
        Refusal::refused(
            Status::BadRequest,
            format!("{} does not take this: {e}", verb.name),
        )
        .caused_by(&e)
    })?;
    match table.call(ctx, verb, req) {
        Ok(answer) => Ok(answer),
        Err(CallError::Refused(r)) => Err(r),
        Err(e) => Err(Refusal::failure(
            Status::Internal,
            format!("the call table is wrong: {e:?}"),
        )),
    }
}

/// [`run`], with the answer written as JSON.
fn run_json<Q, S>(
    table: &Table<Ctx, Refusal>,
    ctx: &Ctx,
    verb: &Verb<Q, S>,
    request: &str,
) -> Result<Answered, Refusal>
where
    Q: serde::de::DeserializeOwned + 'static,
    S: Serialize + 'static,
{
    let answer = run(table, ctx, verb, request)?;
    serde_json::to_string(&answer)
        .map(Answered::Json)
        .map_err(|e| {
            Refusal::failure(Status::Internal, format!("cannot write the answer: {e}"))
                .caused_by(&e)
        })
}

/// The service in a worker.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Service {
    inner: Rc<Inner>,
    table: Table<Ctx, Refusal>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Service {
    /// An empty service: no tiles, the DungeonBlocks target.  The library must be set ([`crate::set_library`]) first:
    /// the owner's target is the library's default printer.
    ///
    /// # Errors
    /// The library has no default printer that reads, or the owner did not answer its first listener.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Result<Self, JsError> {
        let target = options_in(&Lib, System::Db).map_err(|r| JsError::new(&r.message))?;
        let storage = Arc::new(MemStorage::default());
        // My tiles is there from the start: a folder is made inside a folder that exists
        storage
            .create_dir_all(Path::new(FOLDER))
            .map_err(|e| JsError::new(&format!("cannot make {FOLDER}: {e}")))?;
        // the library the page handed over lists as the Library place: its `lib:tiles/` documents
        let library: Vec<String> = LIBRARY.with(|l| {
            l.borrow()
                .keys()
                .filter(|k| k.starts_with("lib:tiles/"))
                .cloned()
                .collect()
        });
        storage.set_library(&library.iter().map(String::as_str).collect::<Vec<_>>());
        let roots = vec![
            Root::new("My tiles", RootKind::Dir(Path::new(FOLDER).into())).with_role(Role::Home),
            Root::new("Library", RootKind::Library("tiles/".to_owned())),
        ];
        let host = Arc::new(WebHost::default());
        let mut core = Core::new(
            Setup {
                tiles: Vec::new(),
                roots: roots.clone(),
                target,
                home: None,
                epoch: "worker".to_owned(),
            },
            storage.clone(),
            host.clone(),
            Arc::new(ManualClock::default()),
        );
        let (tx, rx) = channel();
        core.apply(Command::Subscribe {
            epoch: None,
            since: None,
            reply: tx,
        });
        let Subscribed { rx: stream, .. } = rx
            .try_recv()
            .map_err(|e| JsError::new(&format!("the owner did not answer a subscription: {e}")))?;
        let docs = dt_service::docs::Docs::new(storage.clone(), None);
        let service = Self {
            inner: Rc::new(Inner {
                tiles: StoreAndLib(storage.clone()),
                memo: SceneMemo::default(),
                storage,
                host,
                roots,
                docs,
                system: Cell::new(System::Db),
                core: RefCell::new(core),
                stream,
                built: RefCell::new(BTreeMap::new()),
                ticket: Cell::new(0),
                seq: Cell::new(0),
            }),
            table: table(),
        };
        // the places as they stand: My tiles is empty, the Library lists the page's tiles
        service
            .inner
            .rescan()
            .map_err(|r| JsError::new(&r.message))?;
        Ok(service)
    }

    /// Put a tile's text in the store at `path`: the tile is listed the first time, and a later `put` is a save
    /// from outside (the tile is built again).
    ///
    /// # Errors
    /// The store cannot be written.
    pub fn put(&self, path: &str, text: &str) -> Result<(), JsError> {
        self.inner
            .storage
            .write_text(Path::new(path), text)
            .map_err(|e| JsError::new(&format!("cannot write {path}: {e}")))?;
        self.inner
            .rescan()
            .map_err(|r| JsError::new(&format!("the owner did not list {path}: {}", r.message)))?;
        self.inner
            .core
            .borrow_mut()
            .apply(Command::Saved(path.to_owned()));
        Ok(())
    }

    /// Choose the build target for the calls that follow (`db` or `ol`).
    ///
    /// # Errors
    /// Not a target.
    pub fn set_target(&self, system: &str) -> Result<(), JsError> {
        self.inner.system.set(system_of(system)?);
        Ok(())
    }

    /// Call the verb `verb` about the tile stored at `tile`, with `request` (JSON), and answer what it gave (JSON, or the
    /// page's bytes for a draft and a step).  What the call queued (`build`) is run before this returns, and its
    /// result is on the stream (`events`).
    fn answer(&self, verb: &str, tile: &str, request: &str) -> Result<Answered, Refusal> {
        let tile = &self.resolve(tile);
        let options = options_in(&Lib, self.inner.system.get())?;
        let ctx = Ctx {
            inner: self.inner.clone(),
            tile: tile.to_owned(),
            options,
        };
        let t = &self.table;
        macro_rules! json {
            ($($v:ident),* $(,)?) => {
                match verb {
                    $( n if n == $v.name => run_json(t, &ctx, &$v, request), )*
                    "draft" => run(t, &ctx, &DRAFT, request).map(|m| Answered::Bytes(m.0.as_ref().clone())),
                    "step" => run(t, &ctx, &STEP, request)
                        .and_then(|s| s.bytes())
                        .map(Answered::Bytes),
                    other => Err(Refusal::refused(Status::NotFound, format!("no such call: {other}"))),
                }
            };
        }
        let answer = json![
            CHECK,
            EDIT,
            SAVE,
            BUILD,
            NAMES,
            TOKENS,
            CHECK_VALUE,
            PRINTED,
            PRINTED_SET,
            OFFERS,
            ABOUT,
            ELEMENTS,
            LANGUAGE,
            TEXT,
            MINE,
            NEW_TILE,
            NEW_FOLDER,
            RENAME_FOLDER,
            TRASH_FOLDER,
            TRASH_TILE,
            UNDO_TRASH,
            MOVE_TILE,
            DUPLICATE_TILE,
            RENAME,
            DELETE_TILE,
            SET_TARGET,
            LISTS_NEW,
            LISTS_ADD,
            LISTS_COUNT,
            LISTS_REMOVE,
            LISTS_RESTORE,
            MAPS_ADD,
            MAPS_SET,
            MAPS_BATCH,
            MAPS_DUP,
            MAPS_REMOVE,
            MAPS_PIN,
            MAPS_UNPIN,
            MAPS_UNPIN_ALL,
        ]?;
        // the worker host's turn: what the call queued is built now, and the stream says how it went
        self.inner.run_queued()?;
        Ok(answer)
    }

    /// The path of the tile `tile` names: it is a tile's id (what the page calls it by) when the owner lists one, else it is the
    /// path itself (a call from outside, a tile not listed yet).
    fn resolve(&self, tile: &str) -> String {
        self.inner
            .ask(|r| Command::Facts(tile.to_owned(), r))
            .expect("the owner answers facts in the same step")
            .map_or_else(|| tile.to_owned(), |f| f.path)
    }

    /// The refusal as the page reads it: the status and the error body, written as JSON for the thrown error.
    fn refused(&self, verb: &str, tile: &str, r: Refusal) -> JsError {
        let reply = r.reply(
            verb,
            Some(tile),
            &crate::version(),
            WebHost::default().now_ms(),
        );
        match serde_json::to_string(&reply) {
            Ok(json) => JsError::new(&json),
            Err(e) => JsError::new(&format!("cannot write a refusal: {e}")),
        }
    }

    /// Call the verb `verb` about the tile stored at `tile`, with `request` (JSON), and answer JSON.  What the call
    /// queued (`build`) is run before this returns, and its result is on the stream (`events`).
    ///
    /// # Errors
    /// No such verb, a request that is not the verb's, or the call refused or failed: the error's message is the JSON of a
    /// `RefusalReply` (the status number and the same error body HTTP sends).
    pub fn call(&self, verb: &str, tile: &str, request: &str) -> Result<String, JsError> {
        match self.answer(verb, tile, request) {
            Ok(Answered::Json(json)) => Ok(json),
            Ok(Answered::Bytes(_)) => Err(self.refused(
                verb,
                tile,
                Refusal::refused(
                    Status::BadRequest,
                    format!("{verb} answers bytes: ask with call_bytes"),
                ),
            )),
            Err(r) => Err(self.refused(verb, tile, r)),
        }
    }

    /// [`Service::call`] for the verbs that answer bytes: `draft` (the mesh) and `step` (the head's length, the head, the
    /// mesh), as the HTTP routes answer them.
    ///
    /// # Errors
    /// As [`Service::call`].
    pub fn call_bytes(&self, verb: &str, tile: &str, request: &str) -> Result<Vec<u8>, JsError> {
        match self.answer(verb, tile, request) {
            Ok(Answered::Bytes(b)) => Ok(b),
            Ok(Answered::Json(_)) => Err(self.refused(
                verb,
                tile,
                Refusal::refused(
                    Status::BadRequest,
                    format!("{verb} answers JSON: ask with call"),
                ),
            )),
            Err(r) => Err(self.refused(verb, tile, r)),
        }
    }

    /// The first message of a stream: every tile's state and the rest of what a page is told, as JSON.
    ///
    /// # Errors
    /// The answer cannot be written.
    pub fn snapshot(&self) -> Result<String, JsError> {
        // the snapshot holds everything the stream said so far: a page that takes it is told only what comes after
        self.inner.stream.try_iter().for_each(drop);
        let ctx = Ctx {
            inner: self.inner.clone(),
            tile: String::new(),
            options: options_in(&Lib, self.inner.system.get())
                .map_err(|r| JsError::new(&r.message))?,
        };
        serde_json::to_string(&TileHost::snapshot(&ctx))
            .map_err(|e| JsError::new(&format!("cannot write the snapshot: {e}")))
    }

    /// What a finished build came to, for the tile at `path` (its record, whether it passed, the STL's hash): the
    /// worker's own note beside the stream's record.
    ///
    /// # Errors
    /// The tile has not been built here, or the answer cannot be written.
    pub fn built(&self, path: &str) -> Result<String, JsError> {
        let built = self.inner.built.borrow();
        let b = built
            .get(path)
            .ok_or_else(|| JsError::new(&format!("{path} has not been built here")))?;
        serde_json::to_string(b).map_err(|e| JsError::new(&format!("cannot write the answer: {e}")))
    }

    /// The mesh the page reads (`dt_service::payload`'s bytes) of tile `tile` (an id or a path), made by the build of revision `doc_rev` of its
    /// document: none when the tile's mesh is of another revision (a build is on its way) or the tile has none.
    ///
    /// # Errors
    /// The owner did not answer.
    pub fn mesh(&self, tile: &str, doc_rev: f64) -> Result<Option<Vec<u8>>, JsError> {
        let tile = &self.resolve(tile);
        // a revision is a whole number the page counts up from 1: it travels as a JS number
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a document revision is a small whole number"
        )]
        let rev = doc_rev as u64;
        let answer = self
            .inner
            .ask(|r| Command::Mesh(tile_id(tile), rev, r))
            .map_err(|r| JsError::new(&r.message))?;
        // `Err(rev)`: the mesh is of another revision, which the next summary says
        Ok(match answer {
            Ok(mesh) => mesh.map(|m| m.as_ref().clone()),
            Err(_other_revision) => None,
        })
    }

    /// What the stream said since the last pull, as a JSON array of stream events (a tile's state with its record, the
    /// messages a check found); they are handed over once.
    #[must_use]
    pub fn events(&self) -> String {
        let said: Vec<String> = self.inner.stream.try_iter().collect();
        format!("[{}]", said.join(","))
    }
}
