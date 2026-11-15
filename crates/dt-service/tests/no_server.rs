//! The service runs with no server and no disk: the owner's `Core`, a `MemStorage`, a `ManualClock` and a host of the
//! test's own, driven by `Core::apply` on this thread.  There is no socket, no lane thread and no file system in
//! this test: a tile is listed, asked for, taken by a lane, built (as the patches a build makes), saved to the store,
//! and read back, and every listener's events are the stream's.  This is the proof round 7 (the same code under Node,
//! in wasm) starts from.

use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use dt_eval::Options;
use dt_service::clock::ManualClock;
use dt_service::host::{Alarm, Host};
use dt_service::owner::{Command, Core, Setup};
use dt_service::patch::Patch;
use dt_service::queue::{Job, Taken, Taking};
use dt_service::roots::{Root, RootKind};
use dt_service::storage::{MemStorage, Storage};
use dt_service::tile::{Slot, State, tile_id};

/// A host with no process around it: alarms are kept, the time is fixed, and there are no side jobs.
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

const TILE: &str = "tile size=\"1x1\" seed=1 {\n    stack {\n        dirt\n    }\n}\n";

fn ask<R>(
    core: &mut Core<TestHost>,
    make: impl FnOnce(std::sync::mpsc::Sender<R>) -> Command<TestHost>,
) -> R {
    let (tx, rx) = channel();
    core.apply(make(tx));
    rx.try_recv().expect("the owner answered in the same step")
}

#[test]
fn a_tile_is_listed_asked_for_built_and_saved_with_no_server_and_no_disk() {
    let path = "/memory/My tiles/a.dtile";
    let mem = Arc::new(MemStorage::with(path, TILE));
    let host = Arc::new(TestHost::default());
    let root = Root::new(
        "My tiles",
        RootKind::Dir(Path::new("/memory/My tiles").into()),
    );
    let slot = Slot::in_root(path, 0, None);
    let id = tile_id(path);
    assert_eq!(slot.id, id);
    let mut core = Core::new(
        Setup {
            tiles: vec![slot],
            roots: vec![root],
            target: Options::default(),
            home: None,
            epoch: "test".to_owned(),
        },
        mem.clone(),
        host.clone(),
        Arc::new(ManualClock::default()),
    );

    // a listener is registered at once and is first sent the whole snapshot
    let sub = ask(&mut core, |reply| Command::Subscribe {
        epoch: None,
        since: None,
        reply,
    });
    let held =
        sub.fresh
            .expect("a new stream is sent the snapshot")
            .with(dt_service::owner::Outside {
                printers: Vec::new(),
                inventory: dt_service::inventory::InventoryState::Unavailable,
            });
    assert_eq!(held.tiles.len(), 1);
    assert_eq!(held.tiles[0].status, "idle");
    assert!(
        held.roots[0].exists,
        "the store has the folder the tile is in"
    );

    // the tile is asked for; a lane that waits for work is given it the moment there is some
    assert!(ask(&mut core, |r| Command::Want(id.clone(), r)));
    let (lane, taken) = channel();
    core.apply(Command::Take {
        ticket: 1,
        kind: Taking::Main { eager: false },
        reply: lane,
    });
    match taken.try_recv().expect("the lane was given the build") {
        Taken::Job(Job::Full(got, _)) => assert_eq!(got, id),
        _ => panic!("a lane that waits for a tile's build is given it"),
    }

    // the build's own steps, as patches: the stream says each
    core.apply(Command::Edit(id.clone(), Box::new(Patch::Begin)));
    core.apply(Command::Edit(
        id.clone(),
        Box::new(Patch::Status(State::Ready)),
    ));
    let summary = ask(&mut core, |r| Command::Summary(id.clone(), r)).expect("the tile is listed");
    assert_eq!(summary.status, "ready");
    let told: Vec<String> = sub.rx.try_iter().collect();
    assert!(
        told.iter()
            .any(|e| e.contains("\"tile\"") && e.contains("\"ready\"")),
        "the listener was told the tile is ready: {told:?}"
    );

    // a save: the store holds the new text and a later read sees it, with no disk
    mem.write_text(Path::new(path), &TILE.replace("seed=1", "seed=2"))
        .expect("the store is written");
    assert!(
        mem.read_text(Path::new(path))
            .expect("the store is read")
            .contains("seed=2")
    );
    core.apply(Command::Saved(path.to_owned()));

    assert!(
        host.alarms.lock().expect("the alarms lock").is_empty(),
        "nothing was alarmed"
    );
}
