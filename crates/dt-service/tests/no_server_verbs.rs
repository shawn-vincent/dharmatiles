//! A tile is checked, edited, saved and built through `call`, by verb, with no server and no disk: the verb `Table`
//! the program and the worker both answer, a `MemStorage`, and a host of this test's own that runs queued jobs inline
//! (the program's lanes do it on threads, the worker's host in its turn).  `build` is HTTP's verb with its one
//! meaning: it queues the job, and the record comes on the event stream.  (`draft` and `step` are not service verbs
//! yet; they are the binary's.)

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
use dt_service::edit_calls::{EDIT, EditOp, EditReq, SAVE, SaveReq, Scalar};
use dt_service::host::{Alarm, Host};
use dt_service::message::{Message, Scope};
use dt_service::owner::{Command, Core, Setup, Subscribed};
use dt_service::patch::Patch;
use dt_service::queue::{Finished, Job, Taken, Taking};
use dt_service::roots::{Root, RootKind};
use dt_service::storage::{MemStorage, Storage};
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
            let built = dt_eval::build_cached(
                &Mem(&self.mem),
                PATH,
                &self.options,
                &dt_eval::Progress::none(),
                None,
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
        mem,
        core: RefCell::new(core),
        options: Options::default(),
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
            assert_eq!(r.doc_rev, Some(rev + 1));
            assert!(r.text.is_some_and(|t| t.contains("seed=2")));
        }
        other => panic!("a stale save is a conflict: {other:?}"),
    }

    // build: the verb queues the job; the host's lane builds it; the record is in the tile and on the stream
    table.call(&w, &BUILD, BuildReq {}).expect("asked for");
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
    let told: Vec<String> = stream.try_iter().collect();
    assert!(
        told.iter()
            .any(|e| e.contains("\"tile\"") && e.contains("\"ready\"") && e.contains("triangles")),
        "the stream carried the record: {told:?}"
    );
}
