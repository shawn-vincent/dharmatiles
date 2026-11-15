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
use dt_service::host::{Alarm, Host};
use dt_service::message::{Message, Scope};
use dt_service::owner::{Command, Core, Setup, Subscribed};
use dt_service::patch::Patch;
use dt_service::queue::{Finished, Job, Taken, Taking};
use dt_service::read_calls::{self, Overlay};
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
        .with(&DRAFT, |w: &World, r| {
            dt_service::draft_calls::draft(w, &tile_ref(), r)
        })
        .with(&STEP, |w: &World, r| {
            dt_service::draft_calls::step(w, &tile_ref(), r)
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
        mem,
        core: RefCell::new(core),
        options: Options::default(),
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
