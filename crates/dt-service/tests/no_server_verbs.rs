//! A tile is checked, edited, saved and built through `call`, by verb, with no server and no disk: the verb `Table`
//! the program and the worker both answer, a `MemStorage`, and a host of this test's own that runs queued jobs inline
//! (the program's lanes do it on threads, the worker's host in its turn).  `build` is HTTP's verb with its one
//! meaning: it queues the job, and the record comes on the event stream.  `draft` and `step` are answered by the host's
//! draft lane (here a stand-in that answers at once), and the reading verbs (`names`, `tokens`, `checkValue`,
//! `printed`, `offers`, `about`, `elements`, `language`, `text`) need only the options.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use dt_eval::{Options, Source, TileRecord};
use dt_service::build_calls::{BUILD, BuildReq};
use dt_service::call::{CallError, Refusal, Status, Table};
use dt_service::clock::ManualClock;
use dt_service::docs::Docs;
use dt_service::draft_calls::{DRAFT, DraftAsk, DraftReq, Drafted, STEP, StepDraft, StepReq};
use dt_service::edit_calls::{EDIT, EditOp, EditReq, SAVE, SaveReq, Scalar};
use dt_service::folder_calls::{
    self, FolderNamedReq, FolderReq, PlaceHost, RenameReq, TileMoveReq, TrashUndoReq,
};
use dt_service::host::{Alarm, Host};
use dt_service::message::{Message, Scope};
use dt_service::owner::{Command, Core, Setup, Subscribed};
use dt_service::patch::Patch;
use dt_service::queue::{Finished, Job, Taken, Taking};
use dt_service::read_calls::{self, Nothing, Overlay};
use dt_service::roots::{Root, RootKind};
use dt_service::storage::{MemStorage, Storage};
use dt_service::thing_calls::{BUILD_THING, ThingRef, ThingReq};
use dt_service::tile::{Slot, tile_id};
use dt_service::tile_calls::{CHECK, CheckHost, TextReq};
use dt_service::tile_host::{TileHost, TileRef};

const PATH: &str = "/memory/My tiles/b.dtile";
const TILE: &str = "dharmatiles version=\"1.0.0\"\n\ntile size=\"1x1\" seed=1 {\n    stack {\n        dirt\n    }\n}\n";

/// The owner's host: alarms are kept, time is fixed, no side jobs.
#[derive(Default)]
struct TestHost {
    alarms: Mutex<Vec<String>>,
}

impl Alarm for TestHost {
    fn alarm(&self, msg: &str) {
        self.alarms
            .lock()
            .expect("the alarms lock")
            .push(msg.to_owned());
    }

    fn alarm_once(&self, msg: &str) {
        self.alarm(msg);
    }
}

impl Host for TestHost {
    type Job = ();
    type Failure = String;

    fn now_ms(&self) -> u64 {
        1_000
    }
}

/// What a call runs on: the store, the documents, the owner's state and the library from the checkout.
struct World {
    mem: Arc<MemStorage>,
    docs: Docs,
    core: RefCell<Core<TestHost>>,
    options: Options,
    /// What `moved` was told: (from, to, a folder).
    moves: RefCell<Vec<(std::path::PathBuf, std::path::PathBuf, bool)>>,
    tiles: Tiles,
    /// What a map's reading remembers.
    memo: dt_service::scene_read::SceneMemo,
    /// The lists whose plates were asked to be planned again.
    planned: RefCell<Vec<String>>,
    /// The patches the plates' calls made, and the plate the view was put on.
    patched: RefCell<Vec<String>>,
    plate: RefCell<Option<usize>>,
    /// What the inventory's calls said on the stream.
    said: RefCell<Vec<String>>,
    /// The finished builds this host keeps (the worker's memory store): its lane and its exports build through it.
    kept: dt_store::MemStore,
    /// A test's switch: the shelf of things reads every entry as damaged.
    damaged_shelf: std::cell::Cell<bool>,
}

/// A shelf whose entries all read as damaged: what a thing build says of it, loudly.
struct DamagedShelf<'a>(dt_store::Shelved<'a>);

impl dt_kit::Shelf for DamagedShelf<'_> {
    fn read(&self, _id: &dt_kit::ThingId) -> Result<Option<dt_kit::Body>, String> {
        Err("an entry of the shelf is damaged".to_owned())
    }

    fn keep(&self, id: &dt_kit::ThingId, body: &dt_kit::Body) -> Result<(), String> {
        self.0.keep(id, body)
    }

    fn settle(&self) -> Result<(), String> {
        self.0.settle()
    }
}

impl World {
    /// The one build of a tile in this host, the lane's and an export's alike: the tile's own path, the caller's options,
    /// and the kept builds.
    fn build_tile(&self, path: &str, opts: &Options) -> Result<dt_eval::Built, Vec<dt_eval::Diag>> {
        dt_store::build_stored(
            &self.tiles,
            path,
            opts,
            &dt_eval::Progress::none(),
            None,
            Some(&self.kept),
        )
    }
}

/// A map's pieces' tiles: the store, then the library from the checkout.
struct Tiles(Arc<MemStorage>);

impl Source for Tiles {
    fn read(&self, path: &str) -> Result<String, String> {
        Mem(&self.0).read(path)
    }
}

impl dt_service::scene_read::SceneHost for World {
    fn tiles_home(&self) -> Option<std::path::PathBuf> {
        Some("/memory/My tiles".into())
    }

    fn tile_source(&self) -> &dyn Source {
        &self.tiles
    }

    fn tile_paths(&self) -> Vec<String> {
        vec![PATH.to_owned()]
    }

    fn profiles(&self) -> Box<dyn dt_eval::Profiles + '_> {
        Box::new(dt_eval::OnDisk { home: None })
    }

    fn inventory(&self) -> Result<Option<dt_doc::inventory::Inventory>, Vec<dt_eval::Diag>> {
        dt_service::inventory_calls::read_inventory(self)
    }

    fn scene_memo(&self) -> &dt_service::scene_read::SceneMemo {
        &self.memo
    }

    fn entropy(&self) -> u64 {
        self.said.borrow().len() as u64 + 1
    }
}

impl dt_service::inventory_calls::InventoryHost for World {
    fn today(&self) -> String {
        "2026-11-12".to_owned()
    }

    fn say(&self, m: Message) {
        self.said.borrow_mut().push(m.text);
    }

    fn plan_of(&self, _id: &str) -> Option<dt_service::plan::TilePlan> {
        None
    }
}

impl dt_service::plates_calls::PlatesHost for World {
    fn update(&self, _id: &str, patch: Patch) {
        let name = match patch {
            Patch::PlateZero => "PlateZero",
            Patch::MarkStale => "MarkStale",
            _ => "other",
        };
        self.patched.borrow_mut().push(name.to_owned());
    }

    fn seconds(&self) -> f64 {
        0.0
    }

    fn plate_in_view(&self, _id: &str) -> usize {
        self.plate.borrow().unwrap_or(0)
    }

    fn set_plate(&self, _id: &str, plate: usize) {
        *self.plate.borrow_mut() = Some(plate);
    }

    fn palette(&self) -> dt_kit::Palette {
        dt_kit::Palette::default()
    }

    fn inventory_path(&self) -> Option<String> {
        None
    }

    fn modified(&self, _path: &str) -> Option<std::time::SystemTime> {
        None
    }

    fn bambu_template(&self, _machine: &dt_eval::Machine) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn full_kept(&self, _path: &str, _opts: &Options) -> bool {
        false
    }

    fn thing_shelf<'a>(&'a self, opts: &'a Options) -> Option<Box<dyn dt_kit::Shelf + 'a>> {
        let shelf = dt_store::Shelved::new(&self.kept, opts);
        if self.damaged_shelf.get() {
            return Some(Box::new(DamagedShelf(shelf)));
        }
        Some(Box::new(shelf))
    }

    fn build_piece(
        &self,
        path: &str,
        opts: &Options,
    ) -> Result<dt_eval::Built, Vec<dt_eval::Diag>> {
        self.build_tile(path, opts)
    }

    fn crown_build(&self, _path: &str, _opts: Options) -> Option<dt_eval::Built> {
        None
    }
}

impl dt_service::list_calls::ListHost for World {
    fn path_of(&self, id: &str) -> Option<String> {
        // the lists this test made are listed by their file: the id is the path's
        self.mem
            .list(Path::new("/memory/My tiles"))
            .expect("the folder lists")
            .into_iter()
            .map(|e| e.path.to_string_lossy().into_owned())
            .find(|p| tile_id(p) == id)
    }

    fn list_changed(&self, path: &str) {
        self.planned.borrow_mut().push(path.to_owned());
    }

    fn scene_notes(&self, _: &str) -> Vec<dt_service::scene::Note> {
        Vec::new()
    }
}

/// The tile's own file from the store; everything else (`lib:`) from the library.
struct Mem<'a>(&'a MemStorage);

impl Source for Mem<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        if path.starts_with("lib:") {
            return dt_eval::Files.read(path);
        }
        self.0.read_text(Path::new(path)).map_err(|e| e.to_string())
    }
}

impl CheckHost for World {
    fn options(&self) -> Options {
        self.options.clone()
    }

    fn report(&self, id: &str, scope: Scope, make: impl FnOnce(&str) -> Vec<Message>) {
        let found = make("b");
        self.core.borrow_mut().apply(Command::Report {
            id: id.to_owned(),
            scope,
            found,
        });
    }
}

impl TileHost for World {
    fn docs(&self) -> &Docs {
        &self.docs
    }

    fn storage(&self) -> &dyn Storage {
        self.mem.as_ref()
    }

    fn library(&self) -> &dyn Source {
        &dt_eval::Files
    }

    fn want(&self, id: &str) -> Result<bool, Refusal> {
        let (tx, rx) = channel();
        self.core
            .borrow_mut()
            .apply(Command::Want(id.to_owned(), tx));
        Ok(rx.try_recv().expect("the owner answered in the same step"))
    }

    fn set_marks(&self, id: &str, on: bool) -> Result<bool, Refusal> {
        let (tx, rx) = channel();
        self.core
            .borrow_mut()
            .apply(Command::SetMarks(id.to_owned(), on, tx));
        Ok(rx.try_recv().expect("the owner answered in the same step"))
    }

    /// The test host's draft lane: a text that reads is a mesh (the bytes say which text), one that does not is a mistake.
    fn draft(&self, ask: DraftAsk) -> Result<Drafted, Refusal> {
        let src = Overlay {
            files: BTreeMap::from([(ask.file.clone(), ask.text.clone())]),
            base: &dt_eval::Files,
        };
        Ok(match dt_eval::check(&src, &ask.path, &self.options) {
            Ok(_) => Drafted::Mesh(Arc::new(ask.text.into_bytes())),
            Err(_) => Drafted::Mistake,
        })
    }

    fn version(&self) -> String {
        "test".to_owned()
    }

    fn home(&self) -> Result<std::path::PathBuf, Refusal> {
        Ok(std::path::PathBuf::from("/memory/My tiles"))
    }

    fn listed(&self, dest: &Path) -> dt_service::place_calls::Listed {
        dt_service::place_calls::Listed {
            path: dest.to_string_lossy().into_owned(),
            id: None,
            tile: None,
        }
    }

    fn snapshot(&self) -> dt_service::api::Snapshot {
        let (tx, rx) = channel();
        self.core.borrow_mut().apply(Command::Snapshot(tx));
        rx.try_recv()
            .expect("the owner answered in the same step")
            .with(dt_service::owner::Outside {
                printers: Vec::new(),
                inventory: dt_service::inventory::InventoryState::Unavailable,
            })
    }
}

impl World {
    fn ask<R>(&self, make: impl FnOnce(std::sync::mpsc::Sender<R>) -> Command<TestHost>) -> R {
        let (tx, rx) = channel();
        self.core.borrow_mut().apply(make(tx));
        rx.try_recv().expect("the owner answered in the same step")
    }
}

impl PlaceHost for World {
    fn roots(&self) -> Vec<Root> {
        self.ask(Command::Roots)
    }

    fn trash_dir(&self) -> Option<std::path::PathBuf> {
        Some("/memory/.trash".into())
    }

    fn trash_note(&self, from: &Path, to: &Path) -> usize {
        self.ask(|r| Command::Trash(from.to_path_buf(), to.to_path_buf(), r))
    }

    fn trash_taken(&self, undo: usize) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
        self.ask(|r| Command::Trashed(undo, r))
    }

    fn rescan(&self) {}

    fn told(&self) -> u64 {
        self.ask(Command::Rev)
    }

    fn summary(&self, _: &str) -> Option<dt_service::api::TileSummary> {
        None
    }

    fn moved(&self, from: &Path, to: &Path, folder: bool) {
        self.moves
            .borrow_mut()
            .push((from.to_path_buf(), to.to_path_buf(), folder));
    }
}

impl World {
    /// The test host's lane: take what was queued and build it inline, saying each step as a patch.
    fn run_queued(&self) {
        loop {
            let (tx, rx) = channel();
            self.core.borrow_mut().apply(Command::Take {
                ticket: 1,
                kind: Taking::Main { eager: false },
                reply: tx,
            });
            let Ok(Taken::Job(Job::Full(id, _))) = rx.try_recv() else {
                let (tx, _) = channel();
                self.core.borrow_mut().apply(Command::Withdraw(1, tx));
                return;
            };
            let edit = |p: Patch| {
                self.core
                    .borrow_mut()
                    .apply(Command::Edit(id.clone(), Box::new(p)));
            };
            edit(Patch::Begin);
            let files = BTreeMap::from([(PATH.to_owned(), None)]);
            let (_, rev) = self.docs.read(PATH).expect("the tile reads");
            // the lane's options are the export's (`Options { tier: Full, ..host.options() }`): one key
            let built = self
                .build_tile(
                    PATH,
                    &Options {
                        tier: dt_eval::Tier::Full,
                        ..self.options.clone()
                    },
                )
                .expect("the tile builds");
            edit(Patch::BuildReady {
                problem: Vec::new(),
                record: TileRecord::of(&built, &self.options.printer, 0.0),
                mesh: Arc::new(Vec::new()),
                doc_rev: rev,
                files,
            });
            self.core
                .borrow_mut()
                .apply(Command::Finish(Finished::Full(id)));
        }
    }
}

fn tile_ref() -> TileRef {
    TileRef {
        id: tile_id(PATH),
        path: PATH.to_owned(),
        files: BTreeMap::new(),
    }
}

fn table() -> Table<World, Refusal> {
    Table::default()
        .with(&CHECK, |w: &World, _: TextReq| {
            // the tile's text is the store's: a save wrote it
            dt_service::tile_calls::check(w, &Mem(&w.mem), PATH)
                .map_err(|e| Refusal::failure(Status::Internal, e.0))
        })
        .with(&EDIT, |w: &World, r| dt_service::edit_calls::edit(w, r))
        .with(&SAVE, |w: &World, r| {
            dt_service::edit_calls::save(w, &tile_ref(), r)
        })
        .with(&BUILD, |w: &World, r: BuildReq| {
            dt_service::build_calls::build(w, &tile_id(PATH), r)
        })
        .with(&DRAFT, |w: &World, r| {
            dt_service::draft_calls::draft(w, &tile_ref(), r)
        })
        .with(&STEP, |w: &World, r| {
            dt_service::draft_calls::step(w, &tile_ref(), r)
        })
        .with(&BUILD_THING, |w: &World, r: ThingReq| {
            // the text the page holds goes over the store's
            dt_service::thing_calls::build_thing(w, &Mem(&w.mem), PATH, &r)
        })
}

#[test]
fn a_tile_is_checked_edited_saved_and_built_by_verb_with_no_server_and_no_disk() {
    let mem = Arc::new(MemStorage::with(PATH, TILE));
    let core = Core::new(
        Setup {
            tiles: vec![Slot::in_root(PATH, 0, None)],
            roots: vec![Root::new(
                "My tiles",
                RootKind::Dir(Path::new("/memory/My tiles").into()),
            )],
            target: Options::default(),
            home: None,
            epoch: "test".to_owned(),
        },
        mem.clone(),
        Arc::new(TestHost::default()),
        Arc::new(ManualClock::default()),
    );
    let w = World {
        docs: Docs::new(mem.clone(), None),
        tiles: Tiles(mem.clone()),
        mem,
        core: RefCell::new(core),
        options: Options::default(),
        moves: RefCell::new(Vec::new()),
        memo: dt_service::scene_read::SceneMemo::default(),
        planned: RefCell::new(Vec::new()),
        patched: RefCell::new(Vec::new()),
        plate: RefCell::new(None),
        said: RefCell::new(Vec::new()),
        kept: dt_store::MemStore::new("test", 64 << 20),
        damaged_shelf: std::cell::Cell::new(false),
    };
    let table = table();
    let id = tile_id(PATH);
    let (tx, rx) = channel();
    w.core.borrow_mut().apply(Command::Subscribe {
        epoch: None,
        since: None,
        reply: tx,
    });
    let Subscribed { rx: stream, .. } = rx.try_recv().expect("subscribed");

    // check: the text read, its nodes named by a revision an edit must carry back
    let checked = table
        .call(
            &w,
            &CHECK,
            TextReq {
                text: TILE.to_owned(),
                file: None,
            },
        )
        .expect("the check answers");
    assert!(
        checked.ok && !checked.nodes.is_empty(),
        "{:?}",
        checked.errors
    );
    let tile_node = checked
        .nodes
        .iter()
        .find(|n| n.what == "tile")
        .expect("the tile node is listed")
        .id
        .clone();

    // edit: a text edit by node, nothing written
    let edited = table
        .call(
            &w,
            &EDIT,
            EditReq {
                read: checked.nodes_rev.clone(),
                op: EditOp::Set {
                    text: TILE.to_owned(),
                    id: tile_node,
                    name: "seed".to_owned(),
                    value: Some(Scalar::Int(2)),
                },
            },
        )
        .expect("the edit is made");
    assert!(edited.text.contains("seed=2"), "{}", edited.text);
    assert!(
        w.mem
            .read_text(Path::new(PATH))
            .expect("read")
            .contains("seed=1")
    );

    // edit: a node made from its parts (the place tool), the program writing the words and the point
    let made = table
        .call(
            &w,
            &EDIT,
            EditReq {
                read: checked.nodes_rev.clone(),
                op: EditOp::Make {
                    text: TILE.to_owned(),
                    node: dt_doc::ops::NewNode {
                        name: "rock".to_owned(),
                        args: vec![],
                        controls: vec![dt_doc::ops::NewControl {
                            name: "at".to_owned(),
                            value: dt_doc::ops::NewValue::Point([0.5, 0.5]),
                        }],
                        children: vec![],
                        spread: false,
                    },
                    place: None,
                    id: None,
                },
            },
        )
        .expect("the node is made");
    assert!(made.text.contains("rock at=\"(0.5sq,"), "{}", made.text);

    // save: against the revision the page edited from; an older save is a conflict that names the file
    let (_, rev) = w.docs.read(PATH).expect("the tile reads");
    let saved = table
        .call(
            &w,
            &SAVE,
            SaveReq {
                text: edited.text.clone(),
                doc_rev: rev,
                file: None,
            },
        )
        .expect("the save is made");
    assert_eq!(saved.doc_rev, rev + 1);
    assert!(
        w.mem
            .read_text(Path::new(PATH))
            .expect("read")
            .contains("seed=2")
    );
    match table.call(
        &w,
        &SAVE,
        SaveReq {
            text: TILE.to_owned(),
            doc_rev: rev,
            file: None,
        },
    ) {
        Err(CallError::Refused(r)) => {
            assert_eq!(r.status, Status::Conflict);
            let c = r.conflict.expect("a conflict says the file as it is now");
            assert_eq!(c.doc_rev, rev + 1);
            assert!(c.text.contains("seed=2"));
        }
        other => panic!("a stale save is a conflict: {other:?}"),
    }

    // build: the verb queues the job; the host's lane builds it; the record is in the tile and on the stream
    table
        .call(&w, &BUILD, BuildReq::default())
        .expect("asked for");
    w.run_queued();
    let (tx, rx) = channel();
    w.core.borrow_mut().apply(Command::Summary(id, tx));
    let summary = rx
        .try_recv()
        .expect("answered")
        .expect("the tile is listed");
    assert_eq!(summary.status, "ready");
    let record = summary.record.expect("the build's record");
    assert!(record.audit.triangles > 0);
    // the debug layer's switch rides on the verb: on for the tile (its next build collects the marks), off again, and a
    // tile that is not there is a 404 whatever the switch says
    let marks = |w: &World| {
        let (tx, rx) = channel();
        w.core.borrow_mut().apply(Command::Facts(tile_id(PATH), tx));
        rx.try_recv()
            .expect("answered")
            .expect("the tile has facts")
            .marks
    };
    assert!(!marks(&w));
    table
        .call(&w, &BUILD, BuildReq { debug: Some(true) })
        .expect("asked for, the layer on");
    assert!(marks(&w), "build with debug=true turns the layer on");
    table
        .call(&w, &BUILD, BuildReq { debug: Some(false) })
        .expect("asked for, the layer off");
    assert!(!marks(&w));
    assert_eq!(
        dt_service::build_calls::build(&w, "no-such-tile", BuildReq { debug: Some(true) })
            .expect_err("no such tile")
            .status,
        Status::NotFound
    );
    w.run_queued();
    let told: Vec<String> = stream.try_iter().collect();
    assert!(
        told.iter()
            .any(|e| e.contains("\"tile\"") && e.contains("\"ready\"") && e.contains("triangles")),
        "the stream carried the record: {told:?}"
    );
}

/// A world with the tile in the store, for the tests that need no stream.
fn world() -> World {
    let mem = Arc::new(MemStorage::with(PATH, TILE));
    let core = Core::new(
        Setup {
            tiles: vec![Slot::in_root(PATH, 0, None)],
            roots: vec![Root::new(
                "My tiles",
                RootKind::Dir(Path::new("/memory/My tiles").into()),
            )],
            target: Options::default(),
            home: None,
            epoch: "test".to_owned(),
        },
        mem.clone(),
        Arc::new(TestHost::default()),
        Arc::new(ManualClock::default()),
    );
    World {
        docs: Docs::new(mem.clone(), None),
        tiles: Tiles(mem.clone()),
        mem,
        core: RefCell::new(core),
        options: Options::default(),
        moves: RefCell::new(Vec::new()),
        memo: dt_service::scene_read::SceneMemo::default(),
        planned: RefCell::new(Vec::new()),
        patched: RefCell::new(Vec::new()),
        plate: RefCell::new(None),
        said: RefCell::new(Vec::new()),
        kept: dt_store::MemStore::new("test", 64 << 20),
        damaged_shelf: std::cell::Cell::new(false),
    }
}

#[test]
fn a_draft_and_a_step_are_answered_by_the_hosts_draft_lane_with_no_server() {
    let w = world();
    let table = table();
    // a draft: the text the page holds, answered as the mesh the lane made
    let mesh = table
        .call(
            &w,
            &DRAFT,
            DraftReq {
                text: TILE.to_owned(),
                file: None,
                debug: None,
            },
        )
        .expect("the draft is made");
    assert_eq!(mesh.0.as_slice(), TILE.as_bytes());
    // a text with a mistake is a 422, a file that is not the tile's a 403
    match table.call(
        &w,
        &DRAFT,
        DraftReq {
            text: "tile {".to_owned(),
            file: None,
            debug: None,
        },
    ) {
        Err(CallError::Refused(r)) => assert_eq!(r.status, Status::Unprocessable),
        other => panic!(
            "a draft of a mistake is refused: {:?}",
            other.map(|m| m.0.len())
        ),
    }
    match table.call(
        &w,
        &DRAFT,
        DraftReq {
            text: TILE.to_owned(),
            file: Some("not-a-file-of-this-tile".to_owned()),
            debug: None,
        },
    ) {
        Err(CallError::Refused(r)) => assert_eq!(r.status, Status::Forbidden),
        other => panic!("a foreign file is refused: {:?}", other.map(|m| m.0.len())),
    }
    // a step: the edit made to the text it carries, the draft of the result, in one answer
    let checked = dt_service::tile_calls::check(&w, &Mem(&w.mem), PATH).expect("checks");
    let tile_node = checked
        .nodes
        .iter()
        .find(|n| n.what == "tile")
        .expect("the tile node")
        .id
        .clone();
    let stepped = table
        .call(
            &w,
            &STEP,
            StepReq {
                edit: EditReq {
                    read: checked.nodes_rev,
                    op: EditOp::Set {
                        text: TILE.to_owned(),
                        id: tile_node,
                        name: "seed".to_owned(),
                        value: Some(Scalar::Int(3)),
                    },
                },
                file: None,
                debug: None,
                tier: None,
            },
        )
        .expect("the step is made");
    assert!(stepped.head.text.contains("seed=3"));
    assert_eq!(stepped.head.draft, StepDraft::Mesh);
    let bytes = stepped.bytes().expect("the answer is written");
    let n = u32::from_be_bytes(bytes[..4].try_into().expect("four bytes")) as usize;
    assert_eq!(
        &bytes[4 + n..],
        stepped.head.text.as_bytes(),
        "the mesh follows the head"
    );
}

#[test]
fn the_reading_verbs_answer_from_the_text_and_the_options_alone() {
    let w = world();
    let names = read_calls::names(&read_calls::NamesReq {
        text: TILE.to_owned(),
    });
    assert!(names.ok);
    let tokens = read_calls::tokens(&read_calls::TokensReq {
        text: TILE.to_owned(),
    })
    .expect("a text colours");
    assert!(!tokens.pieces.is_empty() && tokens.pieces.len().is_multiple_of(3));
    // a typed value the reader refuses, and one it takes
    let bad = read_calls::check_value(
        &w,
        &read_calls::CheckValueReq {
            element: "rock".to_owned(),
            control: "size".to_owned(),
            typed: "250dmm".to_owned(),
        },
    );
    assert!(bad.message.is_some(), "250dmm is too big a rock");
    let good = read_calls::check_value(
        &w,
        &read_calls::CheckValueReq {
            element: "rock".to_owned(),
            control: "size".to_owned(),
            typed: "40dmm".to_owned(),
        },
    );
    assert!(good.message.is_none(), "{:?}", good.message);
    // the Add menu at the top of the stack
    let src = Overlay {
        files: BTreeMap::from([(PATH.to_owned(), TILE.to_owned())]),
        base: &dt_eval::Files,
    };
    let offers = read_calls::offers(&w, &src, PATH, 0).expect("the offers are made");
    assert!(offers.ok && offers.offers.iter().any(|o| o.name == "rock"));
    // what the page reads at startup
    assert_eq!(read_calls::about(&w).version, "test");
    assert!(
        read_calls::elements()
            .iter()
            .any(|e| e.summary.name == "rock")
    );
    assert!(!read_calls::language().nodes.is_empty());
    let text = read_calls::text(&w, &tile_ref(), None)
        .expect("reads")
        .expect("the tile has text");
    assert_eq!(text.text, TILE);
    assert!(text.writable);
    assert!(
        read_calls::text(&w, &tile_ref(), Some("not-a-file-of-this-tile"))
            .expect("reads")
            .is_none()
    );
    // the first message of a stream
    let snapshot = TileHost::snapshot(&w);
    assert_eq!(snapshot.tiles.len(), 1);
}

/// A folder made, renamed and trashed and put back, a tile moved into it, copied, renamed, trashed and deleted: all by
/// the service's own functions over a `MemStorage`, with the refusals the program gives (the library, a path out of a
/// place, a place that is not there).
#[test]
fn the_folder_and_tile_verbs_change_the_places_with_no_server_and_no_disk() {
    let w = world();
    let home = Path::new("/memory/My tiles");
    let named = |dir: &str, name: &str| FolderNamedReq {
        root: 0,
        dir: dir.to_owned(),
        name: name.to_owned(),
    };
    let made = folder_calls::new_folder(&w, named("", "dungeon")).expect("a folder is made");
    assert_eq!(made.dir, "dungeon");
    assert!(
        w.mem
            .stat(&home.join("dungeon"))
            .expect("it is there")
            .is_dir()
    );
    let taken =
        folder_calls::new_folder(&w, named("", "dungeon")).expect_err("a name is not taken twice");
    assert_eq!(taken.status, Status::Conflict);
    for odd in ["", "..", "out"] {
        folder_calls::new_folder(&w, named("", odd))
            .expect_err(&format!("`{odd}` is no folder name"));
    }
    let up = folder_calls::new_folder(&w, named("../x", "y")).expect_err("no way out of the place");
    assert_eq!(up.status, Status::BadRequest);
    let no_place = folder_calls::new_folder(
        &w,
        FolderNamedReq {
            root: 9,
            dir: String::new(),
            name: "x".to_owned(),
        },
    )
    .expect_err("no such place");
    assert_eq!(no_place.status, Status::NotFound);

    // a tile moves in, keeps its name, and the host is told
    let into = TileMoveReq {
        root: 0,
        dir: "dungeon".to_owned(),
    };
    let moved = folder_calls::move_tile(&w, &tile_ref(), into).expect("the tile moves");
    let there = home.join("dungeon/b.dtile");
    assert_eq!(moved.path, there.to_string_lossy());
    assert!(w.mem.exists(&there) && !w.mem.exists(Path::new(PATH)));
    assert_eq!(w.moves.borrow().len(), 1);
    let at = TileRef {
        id: tile_id(&there.to_string_lossy()),
        path: there.to_string_lossy().into_owned(),
        files: BTreeMap::new(),
    };

    // a copy beside it, never over a file; a rename never over a file
    let copy = folder_calls::duplicate_tile(&w, &at, Nothing {}).expect("a copy");
    assert!(copy.path.ends_with("dungeon/b copy.dtile"), "{}", copy.path);
    let twice = folder_calls::duplicate_tile(&w, &at, Nothing {}).expect("another copy");
    assert!(twice.path.ends_with("b copy 2.dtile"), "{}", twice.path);
    let clash = folder_calls::rename(
        &w,
        &at,
        RenameReq {
            name: "b copy".to_owned(),
        },
    )
    .expect_err("the name is taken");
    assert_eq!(clash.status, Status::Conflict);
    let renamed = folder_calls::rename(
        &w,
        &at,
        RenameReq {
            name: "keep".to_owned(),
        },
    )
    .expect("renamed");
    assert!(
        renamed.path.ends_with("dungeon/keep.dtile"),
        "{}",
        renamed.path
    );
    let kept = TileRef {
        id: tile_id(&renamed.path),
        path: renamed.path.clone(),
        files: BTreeMap::new(),
    };

    // a tile to the Trash and back
    let gone = folder_calls::trash_tile(&w, &kept, Nothing {}).expect("trashed");
    assert!(!w.mem.exists(Path::new(&kept.path)));
    assert!(w.mem.exists(Path::new("/memory/.trash/keep.dtile")));
    let back = folder_calls::undo_trash(&w, TrashUndoReq { undo: gone.undo }).expect("put back");
    assert_eq!(back.path, kept.path);
    assert!(w.mem.exists(Path::new(&kept.path)));
    folder_calls::undo_trash(
        &w,
        TrashUndoReq {
            undo: gone.undo + 7,
        },
    )
    .expect_err("no such number");

    // a folder renamed (its tiles come along), trashed with them and put back
    let r = folder_calls::rename_folder(&w, named("dungeon", "cellar")).expect("renamed");
    assert_eq!(r.dir, "cellar");
    assert!(w.mem.exists(&home.join("cellar/keep.dtile")));
    assert!(!w.mem.exists(&home.join("dungeon/keep.dtile")));
    assert!(
        w.moves.borrow().iter().any(|m| m.2),
        "the host was told of the folder"
    );
    let t = folder_calls::trash_folder(
        &w,
        FolderReq {
            root: 0,
            dir: "cellar".to_owned(),
        },
    )
    .expect("trashed");
    assert!(!w.mem.exists(&home.join("cellar")));
    folder_calls::undo_trash(&w, TrashUndoReq { undo: t.undo }).expect("put back");
    assert!(w.mem.exists(&home.join("cellar/keep.dtile")));
    folder_calls::trash_folder(
        &w,
        FolderReq {
            root: 0,
            dir: String::new(),
        },
    )
    .expect_err("a place is not trashed");

    // removed for good; and what is not the user's is refused
    let del = TileRef {
        id: tile_id(&renamed.path),
        path: renamed.path.replace("dungeon", "cellar"),
        files: BTreeMap::new(),
    };
    folder_calls::delete_tile(&w, &del, Nothing {}).expect("removed");
    assert!(!w.mem.exists(&home.join("cellar/keep.dtile")));
    let lib = TileRef {
        id: "x".to_owned(),
        path: "lib:tiles/ground/1x1-grass.dtile".to_owned(),
        files: BTreeMap::new(),
    };
    for r in [
        folder_calls::delete_tile(&w, &lib, Nothing {}).map(drop),
        folder_calls::trash_tile(&w, &lib, Nothing {}).map(drop),
        folder_calls::rename(
            &w,
            &lib,
            RenameReq {
                name: "x".to_owned(),
            },
        )
        .map(drop),
    ] {
        assert_eq!(
            r.expect_err("the library is read only").status,
            Status::BadRequest
        );
    }
}

#[test]
fn print_lists_and_maps_are_edited_by_verb_with_no_server_and_no_disk() {
    use dt_service::list_calls::{
        self, ListsAddReq, ListsCountReq, ListsNewReq, ListsRemoveReq, ListsRestoreReq,
    };
    use dt_service::map_calls::{
        self, MapsAddReq, MapsPieceReq, MapsPinReq, MapsSetReq, PieceWords,
    };
    let w = world();
    let tile = tile_id(PATH);

    // a new print list is made in My tiles, never over a file
    let made = list_calls::new_list(
        &w,
        ListsNewReq {
            name: Some("Mine".to_owned()),
            root: None,
            dir: None,
            map: None,
        },
    )
    .expect("a list is made");
    assert_eq!(made.path, "/memory/My tiles/Mine.dmap");
    let again = list_calls::new_list(
        &w,
        ListsNewReq {
            name: Some("Mine".to_owned()),
            root: None,
            dir: None,
            map: None,
        },
    )
    .expect("a second list of the name takes another");
    assert_eq!(again.path, "/memory/My tiles/Mine 2.dmap");
    let list = tile_id(&made.path);

    // a tile put in it: one piece of the library's tile, and again raises the count
    let one = list_calls::add(
        &w,
        ListsAddReq {
            tile: tile.clone(),
            list: Some(list.clone()),
            seed: None,
            system: None,
            doc_rev: None,
        },
    )
    .expect("the tile is added");
    assert_eq!((one.count, one.made), (1, false));
    assert!(one.edited.text.contains("home:b"), "{}", one.edited.text);
    let two = list_calls::add(
        &w,
        ListsAddReq {
            tile: tile.clone(),
            list: Some(list.clone()),
            seed: None,
            system: None,
            doc_rev: Some(one.edited.doc_rev),
        },
    )
    .expect("the same piece again");
    assert_eq!(two.count, 2);
    assert!(
        w.planned.borrow().contains(&made.path),
        "the plates are planned again"
    );

    // a count, edited against the revision the page saw; the id the first edit wrote is named
    let counted = list_calls::set_count(
        &w,
        ListsCountReq {
            list: list.clone(),
            id: "p1".to_owned(),
            count: 5,
            doc_rev: two.edited.doc_rev,
        },
    )
    .expect("the count is set");
    assert!(counted.text.contains("count=5"), "{}", counted.text);
    assert!(
        counted
            .before
            .as_deref()
            .is_some_and(|b| b.contains("count=2"))
    );
    // an older revision is a conflict that names the file as it is now, with the scene
    let stale = list_calls::set_count(
        &w,
        ListsCountReq {
            list: list.clone(),
            id: "p1".to_owned(),
            count: 9,
            doc_rev: two.edited.doc_rev,
        },
    )
    .expect_err("the file moved on");
    assert_eq!(stale.status, Status::Conflict);
    let c = stale.conflict.expect("the conflict carries the file");
    assert_eq!(c.doc_rev, counted.doc_rev);
    assert!(c.scene.is_some(), "and the scene as it reads now");
    // Undo: the text written back, if the file is still at the revision
    let undone = list_calls::restore(
        &w,
        ListsRestoreReq {
            list: list.clone(),
            text: counted.before.clone().expect("a before"),
            doc_rev: counted.doc_rev,
        },
    )
    .expect("the change is taken back");
    assert!(undone.text.contains("count=2"));
    let removed = list_calls::remove(
        &w,
        ListsRemoveReq {
            list: list.clone(),
            id: "p1".to_owned(),
            doc_rev: undone.doc_rev,
        },
    )
    .expect("the piece is removed");
    assert!(!removed.text.contains("home:b"), "{}", removed.text);
    let nowhere = list_calls::remove(
        &w,
        ListsRemoveReq {
            list: "nope".to_owned(),
            id: "p1".to_owned(),
            doc_rev: 1,
        },
    )
    .expect_err("no such list");
    assert_eq!(nowhere.status, Status::NotFound);
    // a text that does not read as a list is refused with its mistakes, and nothing is written
    let bad = list_calls::restore(
        &w,
        ListsRestoreReq {
            list: list.clone(),
            text: "not a list".to_owned(),
            doc_rev: removed.doc_rev,
        },
    )
    .expect_err("it does not read");
    assert_eq!(bad.status, Status::Unprocessable);
    assert!(bad.errors.is_some());

    // a map: a grid, and a piece placed on it, read as a scene with no build
    let map = list_calls::new_list(
        &w,
        ListsNewReq {
            name: Some("Map".to_owned()),
            root: None,
            dir: None,
            map: Some(true),
        },
    )
    .expect("a map is made");
    let map_id = tile_id(&map.path);
    let (_, rev) = w.docs.read(&map.path).expect("the map reads");
    let placed = map_calls::add(
        &w,
        MapsAddReq {
            list: map_id.clone(),
            tile: tile.clone(),
            at: Some("(1sq, 0sq)".to_owned()),
            near: None,
            rot: None,
            seed: None,
            seed_word: None,
            system: None,
            doc_rev: rev,
        },
    )
    .expect("the piece is placed");
    let scene = placed.edited.scene.as_ref().expect("the scene is read");
    assert!(scene.map && scene.placed == 1, "{scene:?}");
    assert_eq!(scene.pieces[0].tile, "home:b");
    assert_eq!(scene.pieces[0].at, Some([35.0, 0.0]));
    assert_eq!(scene.pieces[0].error, None);
    // the nearest free square: the wish is taken, so it goes beside
    let (_, rev) = w.docs.read(&map.path).expect("the map reads");
    let near = map_calls::add(
        &w,
        MapsAddReq {
            list: map_id.clone(),
            tile: tile.clone(),
            at: None,
            near: Some(dt_service::placing::Near { x: 1.0, y: 0.0 }),
            rot: None,
            seed: None,
            seed_word: None,
            system: None,
            doc_rev: rev,
        },
    )
    .expect("the second piece is placed near");
    assert_eq!(near.shifted, 1);
    // a piece's words are set, and it is pinned to a plate
    let set = map_calls::set(
        &w,
        MapsSetReq {
            list: map_id.clone(),
            id: placed.id.clone(),
            set: PieceWords {
                rot: Some(Some(90)),
                ..PieceWords::default()
            },
            doc_rev: near.edited.doc_rev,
        },
    )
    .expect("the piece is turned");
    assert!(set.text.contains("rot=90"), "{}", set.text);
    let pinned = map_calls::pin(
        &w,
        MapsPinReq {
            list: map_id.clone(),
            id: placed.id.clone(),
            plate: 1,
            x: 10.04,
            y: 20.0,
            turn: None,
            doc_rev: set.doc_rev,
        },
    )
    .expect("the piece is pinned");
    assert!(pinned.edited.text.contains("pin"), "{}", pinned.edited.text);
    let unpinned = map_calls::unpin(
        &w,
        MapsPieceReq {
            list: map_id.clone(),
            id: placed.id,
            doc_rev: pinned.edited.doc_rev,
        },
    )
    .expect("the pin is taken off");
    assert!(!unpinned.text.contains("pin"), "{}", unpinned.text);
    // a piece that is not there, and a wish that is both places
    let no_piece = map_calls::remove(
        &w,
        MapsPieceReq {
            list: map_id,
            id: "zz".to_owned(),
            doc_rev: unpinned.doc_rev,
        },
    )
    .expect_err("no such piece");
    assert_eq!(no_piece.status, Status::BadRequest);
}

#[test]
fn the_plates_settings_and_the_plate_in_view_are_set_by_verb_with_no_server_and_no_disk() {
    use dt_service::list_calls::{self, ListsNewReq};
    use dt_service::plates_calls::{self, PlatesSetReq, PlatesShowReq};
    let w = world();
    let made = list_calls::new_list(
        &w,
        ListsNewReq {
            name: Some("Plates".to_owned()),
            root: None,
            dir: None,
            map: None,
        },
    )
    .expect("a list is made");
    let list = tile_id(&made.path);
    let (_, rev) = w.docs.read(&made.path).expect("the list reads");
    let set = |key: &str, value: Option<&str>, doc_rev: u64| {
        plates_calls::set(
            &w,
            PlatesSetReq {
                list: list.clone(),
                key: key.to_owned(),
                value: value.map(str::to_owned),
                doc_rev,
            },
        )
    };
    // a word of the plates line is written, and the list is laid out again (marked stale: building from now)
    let one = set("printer", Some("bambu/x1c"), rev).expect("the printer is set");
    assert!(one.text.contains("bambu/x1c"), "{}", one.text);
    let patches = w.patched.borrow().join(" ");
    assert!(
        patches.contains("PlateZero") && patches.contains("MarkStale"),
        "{patches}"
    );
    // a word that is not a setting, or does not read, is refused and nothing is written
    let nope = set("colour", Some("red"), one.doc_rev).expect_err("not a setting");
    assert_eq!(nope.status, Status::BadRequest);
    let odd = set("mode", Some("rainbow"), one.doc_rev).expect_err("not a mode");
    assert_eq!(odd.status, Status::BadRequest);
    // a file that moved on is a conflict that names it
    let stale = set("gap", Some("3mm"), rev).expect_err("the file moved on");
    assert_eq!(stale.status, Status::Conflict);
    // a word taken off
    let off = set("printer", None, one.doc_rev).expect("the word is taken off");
    assert!(!off.text.contains("bambu/x1c"), "{}", off.text);
    // the plate in view is the owner's, and the list is laid out again
    plates_calls::show(
        &w,
        PlatesShowReq {
            list: list.clone(),
            plate: 2,
        },
    )
    .expect("the plate is shown");
    assert_eq!(*w.plate.borrow(), Some(2));
    plates_calls::show(
        &w,
        PlatesShowReq {
            list: "nope".to_owned(),
            plate: 0,
        },
    )
    .expect_err("no such list");
}

#[test]
fn the_inventory_is_read_and_added_to_by_verb_through_the_store_with_no_server_and_no_disk() {
    use dt_service::inventory::{ColourWord, InventoryState};
    use dt_service::inventory_calls::{
        self, InventoryGoneReq, InventoryHaveReq, InventoryRelinkReq, InventoryReprintReq,
    };
    let w = world();
    let tile = tile_id(PATH);
    let home = Path::new("/memory/My tiles");

    // no log yet: nothing is made until the first line is written
    assert!(matches!(
        inventory_calls::snapshot(&w),
        InventoryState::Absent { .. }
    ));
    assert!(!w.mem.exists(&inventory_calls::path_in(home)));

    // one good print: the file is made with its header, in the store
    let wrote = inventory_calls::have(
        &w,
        InventoryHaveReq {
            tile: tile.clone(),
            colour: ColourWord::Single,
            system: None,
            seed: None,
            count: Some(2),
            on: None,
        },
    )
    .expect("a print is added");
    assert_eq!(wrote.lines.len(), 2);
    assert_eq!(wrote.file, "/memory/My tiles/inventory.dinv");
    let text = w
        .mem
        .read_text(&inventory_calls::path_in(home))
        .expect("the log is in the store");
    assert!(text.contains("2026-11-12"), "{text}");
    assert!(text.contains("home:b"), "{text}");
    let InventoryState::Read { pieces, .. } = inventory_calls::snapshot(&w) else {
        panic!("the log reads");
    };
    assert_eq!(pieces.len(), 1);
    assert_eq!(
        (pieces[0].have, pieces[0].edited, pieces[0].missing),
        (2, false, false)
    );
    assert!(
        w.said.borrow().iter().any(|m| m.contains("Added 2")),
        "{:?}",
        w.said.borrow()
    );

    // the tile changes after it was printed: the piece says so
    w.mem
        .write_text(Path::new(PATH), &format!("{TILE}\n// edited\n"))
        .expect("the tile is written");
    let InventoryState::Read { pieces, .. } = inventory_calls::snapshot(&w) else {
        panic!("the log reads");
    };
    assert!(
        pieces[0].edited,
        "a tile edited since it was printed says so"
    );

    // one is lost; none of a piece you do not have can be
    let p = pieces[0].clone();
    inventory_calls::gone(
        &w,
        InventoryGoneReq {
            tile: p.tile.clone(),
            seed: p.seed,
            system: p.system,
            colour: ColourWord::Single,
            note: Some("dropped".to_owned()),
            on: None,
        },
    )
    .expect("one is marked gone");
    let refused = inventory_calls::gone(
        &w,
        InventoryGoneReq {
            tile: p.tile.clone(),
            seed: p.seed + 1,
            system: p.system,
            colour: ColourWord::Single,
            note: None,
            on: None,
        },
    )
    .expect_err("there is none of that seed");
    assert_eq!(refused.status, Status::Conflict);
    let bad_date = inventory_calls::gone(
        &w,
        InventoryGoneReq {
            tile: p.tile.clone(),
            seed: p.seed,
            system: p.system,
            colour: ColourWord::Single,
            note: None,
            on: Some("yesterday-ish".to_owned()),
        },
    )
    .expect_err("not a date");
    assert_eq!(bad_date.status, Status::BadRequest);

    // a tile that moves: the log follows it with a `renamed` line
    inventory_calls::note_rename(
        &w,
        Path::new(PATH),
        Path::new("/memory/My tiles/glade/b.dtile"),
    );
    let after = w
        .mem
        .read_text(&inventory_calls::path_in(home))
        .expect("the log is in the store");
    assert!(after.contains("home:glade/b"), "{after}");

    // a re-link to a listed tile (the old path is the one the log knows now)
    let relinked = inventory_calls::relink(
        &w,
        InventoryRelinkReq {
            from: "home:glade/b".to_owned(),
            to: tile.clone(),
            on: None,
        },
    )
    .expect("the path is re-linked");
    assert!(relinked.lines[0].contains("home:b"), "{:?}", relinked.lines);

    // a reprint goes into a new list beside the tiles, with `print=always` for a piece still on the shelf
    let reprinted = inventory_calls::reprint(
        &w,
        InventoryReprintReq {
            tile: "home:b".to_owned(),
            seed: p.seed,
            system: p.system,
            colour: ColourWord::Single,
            list: None,
            doc_rev: None,
        },
    )
    .expect("a reprint makes a list");
    assert!(reprinted.made && reprinted.always, "{reprinted:?}");
    assert!(reprinted.text.contains("home:b"), "{}", reprinted.text);

    // a log that does not read is never added to: its mistakes are the answer
    w.mem
        .write_text(&inventory_calls::path_in(home), "not a log {")
        .expect("the log is spoilt");
    let e = inventory_calls::have(
        &w,
        InventoryHaveReq {
            tile,
            colour: ColourWord::Single,
            system: None,
            seed: None,
            count: None,
            on: None,
        },
    )
    .expect_err("the log does not read");
    assert_eq!(e.status, Status::Unprocessable);
    assert!(matches!(
        inventory_calls::snapshot(&w),
        InventoryState::Broken { .. }
    ));
}

#[test]
fn a_tile_is_exported_as_downloads_by_verb_with_no_server_and_no_disk() {
    use dt_service::export_calls::{self, ExportReq, ExportRes};
    let w = world();
    let t = tile_ref();
    // where the files would go: a browser has downloads, never a folder
    let ExportRes::Where { out, .. } = export_calls::export_tile(
        &w,
        &t,
        &ExportReq {
            threemf: None,
            ask_where: Some(true),
        },
    )
    .expect("asking where is answered") else {
        panic!("asking where answered files")
    };
    assert_eq!(out, "downloads");
    // the tile built in full, through the gate: its STL, its record and the plain painted 3MF, as bytes
    let ExportRes::Built(made) = export_calls::export_tile(
        &w,
        &t,
        &ExportReq {
            threemf: Some(true),
            ask_where: None,
        },
    )
    .expect("the tile exports") else {
        panic!("an export answered where")
    };
    assert!(made.files.is_empty(), "nothing is written: the page saves");
    let names: Vec<&str> = made.downloads.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(names[0] == "b-db.stl", "{names:?}");
    assert!(
        names[1].ends_with(".build.json") && names[2].ends_with(".3mf"),
        "{names:?}"
    );
    let stl = made.downloads[0].bytes().expect("the data is base64");
    // a binary STL: 80 bytes of header, a count, 50 bytes a triangle
    let n = u32::from_le_bytes(stl[80..84].try_into().expect("four bytes")) as usize;
    assert_eq!(stl.len(), 84 + 50 * n);
    assert_eq!(made.downloads[0].media_type, "model/stl");
    assert!(made.build.audit.failures.is_empty());
    // the same bytes the command line writes: one function makes them
    let again = dt_eval::build_cached(
        &w.tiles,
        PATH,
        &dt_eval::Options {
            tier: dt_eval::Tier::Full,
            ..w.options.clone()
        },
        &dt_eval::Progress::none(),
        None,
    )
    .expect("the tile builds");
    let verdict = again.verdict(&w.options.printer);
    assert_eq!(
        stl,
        dt_print::stl(&verdict.passed().expect("it passes")),
        "the download is the STL"
    );
}

#[test]
fn the_lanes_build_and_an_export_of_the_same_tile_are_one_key_and_one_entry() {
    use dt_service::export_calls::{self, ExportReq, ExportRes};
    let w = world();
    let table = table();
    // the lane builds the tile in full: one entry is kept
    assert!(w.kept.is_empty());
    table
        .call(&w, &BUILD, BuildReq::default())
        .expect("the build is queued");
    w.run_queued();
    assert_eq!(w.kept.len(), 1, "the lane's build is kept");
    // an export of the same tile at the same options reads that entry: the same key, a hit, nothing built, nothing more kept
    let ExportRes::Built(made) = export_calls::export_tile(
        &w,
        &tile_ref(),
        &ExportReq {
            threemf: None,
            ask_where: None,
        },
    )
    .expect("the tile exports") else {
        panic!("an export answered where")
    };
    assert_eq!(
        made.build.cache.state,
        dt_eval::CacheState::Hit,
        "the export did not read the lane's build: {:?}",
        made.build.cache
    );
    assert_eq!(
        w.kept.len(),
        1,
        "the export kept a second entry: another key"
    );
}

const THINGS: &str = "dharmatiles version=\"1.0.0\"\n\ndefine bluff {\n    stack {\n        rock size=\"6dmm .. 10dmm\"\n    }\n}\n\ndefine wet {\n    stack {\n        dirt\n    }\n}\n\ntile size=\"1x1\" seed=1 {\n    stack {\n        dirt\n        rock size=\"5dmm .. 8dmm\"\n        scatter count=2 {\n            rock size=\"3dmm .. 4dmm\"\n        }\n    }\n}\n";

fn thing_req(which: ThingRef, seeds: &[u64]) -> ThingReq {
    ThingReq {
        text: THINGS.to_owned(),
        file: None,
        which,
        seeds: seeds.to_vec(),
        tier: None,
    }
}

/// A tile calls the oak a part defines, and the page is on the part (its text is `file`'s): the entry of the tile is the thing,
/// drawn with the part's text as held (an edit of the part is seen), while the tile is read from the store.
#[test]
fn a_called_thing_is_drawn_with_the_text_of_the_part_the_page_is_on() {
    let w = world();
    let tile = "/memory/My tiles/oak.dtile";
    let text = "dharmatiles version=\"1.0.0\"\n\nuse \"parts/trees\" oak\n\ntile size=\"1x1\" seed=7 {\n    stack {\n        base\n        dirt\n        oak\n    }\n}\n";
    w.mem.write_text(Path::new(tile), text).expect("written");
    let part = dt_eval::Files
        .read("lib:parts/trees.dtile")
        .expect("the part");
    let draw = |part_text: &str| {
        dt_service::thing_calls::build_thing(
            &w,
            &Mem(&w.mem),
            tile,
            &ThingReq {
                text: part_text.to_owned(),
                file: Some("lib:parts/trees.dtile".to_owned()),
                which: ThingRef::Entry {
                    key: "tile/tree/0".to_owned(),
                    child: None,
                },
                seeds: vec![1],
                tier: Some(dt_eval::Tier::Low),
            },
        )
        .expect("the called oak is drawn")
    };
    let wide = draw(&part);
    assert_eq!(wide.element, "tree");
    let narrow = draw(&part.replace("width=\"44dmm\"", "width=\"30dmm\""));
    assert!(
        narrow.extent.most.x < wide.extent.most.x,
        "the part's own text is what is drawn: {} then {}",
        wide.extent.most.x,
        narrow.extent.most.x
    );
}

/// `buildThing`: a rock define at seeds 1 to 4, a rock entry and a scatter's child, and the refusals, by verb with no
/// server and no disk.
#[test]
fn a_thing_is_drawn_alone_at_each_seed_by_verb_with_no_server() {
    let w = world();
    let table = table();
    let define = || ThingRef::Define {
        name: "bluff".to_owned(),
    };
    let res = table
        .call(&w, &BUILD_THING, thing_req(define(), &[1, 2, 3, 4]))
        .expect("a rock define is drawn");
    assert_eq!(res.element, "rock");
    assert_eq!(res.draws.len(), 4);
    let e = res.extent;
    assert!(e.least.x <= e.most.x && e.least.z <= e.most.z, "{e:?}");
    assert!(
        res.shown
            .iter()
            .any(|c| c.control == "buried" && c.from == "default"),
        "the controls as built: {:?}",
        res.shown
    );
    let bytes = |d: &dt_service::thing_calls::ThingDraw| {
        dt_service::export_calls::unbase64(&d.mesh).expect("base64")
    };
    for d in &res.draws {
        assert!(
            d.triangles > 20,
            "seed {} has {} triangles",
            d.seed,
            d.triangles
        );
        assert_eq!(&bytes(d)[..4], dt_service::payload::MAGIC);
        // frame z up: the thing's own box lies within its extent
        assert!(d.size.z > 0.0 && d.size.z <= e.most.z + 1e-6, "{d:?} {e:?}");
    }
    assert_ne!(
        bytes(&res.draws[0]),
        bytes(&res.draws[1]),
        "two seeds, two rocks"
    );
    // a draw is the same wherever it is asked: one seed alone is the strip's draw of that seed
    let again = table
        .call(&w, &BUILD_THING, thing_req(define(), &[3]))
        .expect("one seed");
    assert_eq!(bytes(&again.draws[0]), bytes(&res.draws[2]));
    // the bodies are kept, as in a tile: the first call made its four rocks, and asking for a seed again reads its body
    assert_eq!(
        (res.used.built, res.used.kept),
        (4, 0),
        "four rocks made: {:?}",
        res.used
    );
    assert_eq!(
        (again.used.built, again.used.kept),
        (0, 1),
        "a seed asked again is read, not made: {:?}",
        again.used
    );
    assert!(res.problems.is_empty() && again.problems.is_empty());
    // a shelf that cannot be read is told in the answer, and the draw is made all the same, the same bytes
    w.damaged_shelf.set(true);
    let hurt = table
        .call(&w, &BUILD_THING, thing_req(define(), &[3]))
        .expect("a draw is made past a damaged shelf");
    w.damaged_shelf.set(false);
    assert_eq!(bytes(&hurt.draws[0]), bytes(&res.draws[2]));
    assert_eq!((hurt.used.built, hurt.used.kept), (1, 0));
    assert_eq!(
        hurt.problems,
        vec!["an entry of the shelf is damaged".to_owned()]
    );

    // a low draw is a quick look, not the gate's: it says so
    assert!(
        res.draws.iter().all(|d| d.checked),
        "a full draw is checked"
    );
    let mut quick = thing_req(define(), &[3]);
    quick.tier = Some(dt_eval::Tier::Low);
    let quick = table.call(&w, &BUILD_THING, quick).expect("a low draw");
    assert!(!quick.draws[0].checked);

    // an entry of the tile, and one of a scatter's things, by the id the stack gives them
    let read = dt_eval::doc::read_with(
        &dt_eval::Memory(BTreeMap::from([(PATH.to_owned(), THINGS.to_owned())])),
        PATH,
        Options::default().units(),
    )
    .expect("reads");
    let keys: Vec<String> = read
        .tile
        .expect("a tile")
        .stack
        .iter()
        .map(|e| e.id.as_str().to_owned())
        .collect();
    let rock = keys
        .iter()
        .find(|k| k.contains("rock"))
        .expect("a rock entry");
    let one = table
        .call(
            &w,
            &BUILD_THING,
            thing_req(
                ThingRef::Entry {
                    key: rock.clone(),
                    child: None,
                },
                &[1],
            ),
        )
        .expect("an entry is drawn");
    assert_eq!(one.element, "rock");
    let scatter = keys
        .iter()
        .find(|k| k.contains("scatter"))
        .expect("a scatter");
    let child = table
        .call(
            &w,
            &BUILD_THING,
            thing_req(
                ThingRef::Entry {
                    key: scatter.clone(),
                    child: Some(0),
                },
                &[1],
            ),
        )
        .expect("a scatter's thing is drawn");
    assert!(
        child.extent.most.x < one.extent.most.x,
        "the child is the smaller rock"
    );
    let refused = |which: ThingRef, seeds: &[u64]| match table.call(
        &w,
        &BUILD_THING,
        thing_req(which, seeds),
    ) {
        Err(CallError::Refused(r)) => r,
        other => panic!("refused: {:?}", other.map(|r| r.element)),
    };
    // the scatter itself, and a define of a layer, are layers
    let r = refused(
        ThingRef::Entry {
            key: scatter.clone(),
            child: None,
        },
        &[1],
    );
    assert_eq!(r.status, Status::Unprocessable);
    assert!(r.message.contains("layer"), "{}", r.message);
    let r = refused(
        ThingRef::Define {
            name: "wet".to_owned(),
        },
        &[1],
    );
    assert_eq!(r.status, Status::Unprocessable);
    assert!(r.message.contains("`wet` is a layer"), "{}", r.message);
    // a name that is no define, and no seed
    let r = refused(
        ThingRef::Define {
            name: "nope".to_owned(),
        },
        &[1],
    );
    assert_eq!(r.status, Status::NotFound);
    let r = refused(define(), &[]);
    assert_eq!(r.status, Status::BadRequest);
}

/// What `cargo xtask parity --service` holds the worker's `buildThing` to: the program's draws of a rock define at seeds 1 to 4
/// (this test, native), as the triangles, the SHA-256 of each mesh payload and the extent.  The file is the one definition of
/// the document and of the answer; `DT_THING_GOLDEN=write` rewrites the answer from this run.
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
struct ThingGolden {
    text: String,
    name: String,
    seeds: Vec<u64>,
    triangles: Vec<usize>,
    mesh_sha256: Vec<String>,
    extent: Option<dt_service::thing_calls::ThingExtent>,
}

#[test]
fn the_programs_thing_draws_are_the_golden_the_worker_is_held_to() {
    use sha2::{Digest, Sha256};
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/thing-parity.json");
    let golden: ThingGolden =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("the golden file"))
            .expect("the golden file reads");
    let w = world();
    let table = table();
    let res = table
        .call(
            &w,
            &BUILD_THING,
            ThingReq {
                text: golden.text.clone(),
                file: None,
                which: ThingRef::Define {
                    name: golden.name.clone(),
                },
                seeds: golden.seeds.clone(),
                tier: None,
            },
        )
        .expect("the golden's thing is drawn");
    let made = ThingGolden {
        text: golden.text.clone(),
        name: golden.name.clone(),
        seeds: golden.seeds.clone(),
        triangles: res.draws.iter().map(|d| d.triangles).collect(),
        mesh_sha256: res
            .draws
            .iter()
            .map(|d| {
                let bytes = dt_service::export_calls::unbase64(&d.mesh).expect("base64");
                Sha256::digest(bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect()
            })
            .collect(),
        extent: Some(res.extent),
    };
    if std::env::var_os("DT_THING_GOLDEN").is_some_and(|v| v == "write") {
        std::fs::write(
            &file,
            serde_json::to_string_pretty(&made).expect("writes") + "\n",
        )
        .expect("the golden file is written");
        return;
    }
    assert_eq!(
        made, golden,
        "the program's draws are not the golden's: if the engine changed on purpose, run DT_THING_GOLDEN=write cargo test -p dt-service --test no_server_verbs the_programs_thing"
    );
}
