//! The owner of the editor's state: one thread holds the tiles and the event stream, and everything else asks it.
//!
//! **What it owns.**  The list of tiles (every [`Slot`]) and the event stream ([`Events`]: the listeners, the log
//! of changes a stream resumes from, the messages, and what each resource was last told as).  Nothing else reads or
//! writes either: a transport (an HTTP connection, the worker lanes, a route) holds an [`Owner`], sends it a typed
//! [`Command`] and, when it needs an answer, waits on the reply channel that came with the command.  The commands
//! are applied one at a time, in the order they were sent, so there is no lock order: a change and the event that
//! tells it are one step of the owner, and no listener can fall between them.
//!
//! **What it never does.**  It never blocks on a build, reads the disk or takes a lock another thread holds while it
//! waits on this one.  Long work (a build, a scan of the folders, a read of the inventory) runs on the caller's or a
//! worker's thread and reaches the owner as a command carrying its result ([`Command::Reconcile`],
//! [`Command::Publish`], [`Command::Edit`]); what a command needs from the disk (a resource's value, a new tile's
//! size) is read before it is sent.  So the owner can move, as it is, into a crate that knows neither HTTP nor the
//! disk.
//!
//! **What stays outside, and why.**  The build caches (`Shared::build_caches`) and the disk [`dt_store::Store`] are
//! shared between the build threads for speed: a build reads and fills them from many threads at once, and a
//! command round trip for each stage would cost more than the builds save.  The places (`roots`, `folders`), the
//! target, the queue of builds and the tier book are still behind their own locks in `Shared`; each is a leaf (held
//! alone, never with another), and they move into the owner one at a time.
//!
//! **The one closure.**  [`Command::Edit`] carries a function over one [`Slot`]: the build changes a dozen
//! different fields of a tile, and each is told in the same step as it is made.  Every other command is plain
//! data.  It is the part of this to turn into typed patches before the owner leaves this crate.

use std::sync::mpsc::{Receiver, Sender, channel};

use super::*;
use resources::Resource;

/// Everything the owner holds.
pub(super) struct Core {
    pub(super) tiles: Vec<Slot>,
    pub(super) events: Events,
    /// Where the tiles come from; a slot's `root` indexes this.
    roots: Vec<Root>,
    /// The folders of each folder place, as paths below it (empty ones too): what the last scan found, so a new or
    /// removed folder is told.
    folders: BTreeMap<usize, Vec<String>>,
    /// The build target the page chose: the options every build is made with.
    target: Options,
    /// `$HOME`, as the places say it.
    home: Option<String>,
    epoch: String,
}

/// What a snapshot holds that the owner cannot read itself (it reads the disk): read by the caller, after the owner
/// answered with what it holds ([`Held`]).
pub(super) struct Outside {
    pub printers: Vec<plan::PrinterRow>,
    pub inventory: crate::inventory_wire::InventoryState,
}

/// What the owner holds of a snapshot, at one moment: the tiles, the places, the target and the stream's revision.
pub(super) struct Held {
    rev: u64,
    epoch: String,
    target: resources::Target,
    places: resources::Places,
    tiles: Vec<api::TileSummary>,
}

impl Held {
    /// The whole snapshot: `outside` was read after this, so it is of this moment or later (the events after it, which
    /// the listener has, say anything that changed since).
    pub(super) fn with(self, outside: Outside) -> api::Snapshot {
        api::Snapshot {
            contract: api::contract().to_owned(),
            rev: self.rev,
            epoch: self.epoch,
            system: self.target.system,
            printer: self.target.printer,
            printers: outside.printers,
            overhang_deg: self.target.overhang_deg,
            steepest_deg: self.target.steepest_deg,
            square_mm: self.target.square_mm,
            roots: self.places.roots,
            folders: self.places.folders,
            home: self.places.home,
            inventory: outside.inventory,
            tiles: self.tiles,
        }
    }
}

/// A new listener, registered: the events after this moment come on `rx`, and what it is first sent is here.
pub(super) struct Subscribed {
    pub rx: Receiver<String>,
    /// A stream that resumes is sent what changed since (these events, in order)...
    pub lead: Vec<String>,
    /// ...and one that starts over is sent the snapshot of this moment...
    pub fresh: Option<Held>,
    /// ...and then every kept message, so a page that came (or came back) has the whole record.
    pub messages: Vec<String>,
}

/// A change to one tile, made on the owner.
pub(super) type EditFn = Box<dyn FnOnce(&mut Slot) + Send>;

/// What a caller reads of one tile, besides its summary.
pub(super) struct Facts {
    pub path: String,
    pub name: String,
    pub files: BTreeMap<String, Option<SystemTime>>,
    pub plate: usize,
    pub has_mesh: bool,
    pub marks: bool,
    pub idle: bool,
}

/// One tile as the queue sees it: enough to pick what to build, and to see whether its files changed.
#[derive(Clone)]
pub(super) struct Row {
    pub id: TileId,
    pub root: usize,
    pub idle: bool,
    /// Read only by the tests' idle check.
    #[cfg_attr(
        not(any(test, feature = "test-routes")),
        expect(
            dead_code,
            reason = "only `pending` reads it, and that is the tests' idle check"
        )
    )]
    pub building: bool,
    pub stale: bool,
    /// The files its last build read (empty while it is idle).
    pub files: BTreeMap<String, Option<SystemTime>>,
}

impl Row {
    /// Built, and a file it read has changed (or the target did): it is built again.
    pub(super) fn changed(&self) -> bool {
        !self.idle && (self.stale || self.files.iter().any(|(f, t)| mtime(f) != *t))
    }
}

/// How a running build or a map's job says it began, as the message that stays until it ends.
pub(super) struct Begin {
    pub id: TileId,
    pub text: fn(&str) -> String,
    pub code: Option<&'static str>,
}

/// What a tile reports once it is built again: see [`Core::send_messages`].
pub(super) enum Command {
    // ---- reads
    Facts(TileId, Sender<Option<Facts>>),
    Summary(TileId, Sender<Option<api::TileSummary>>),
    Plan(TileId, Sender<Option<plan::TilePlan>>),
    SceneNotes(TileId, Sender<Vec<scene::Note>>),
    Rows(Sender<Vec<Row>>),
    Paths(Sender<Vec<(usize, String)>>),
    Mesh(TileId, u64, Sender<Result<Option<Arc<Vec<u8>>>, u64>>),
    SketchMesh(TileId, u64, Sender<Option<Arc<Vec<u8>>>>),
    Rev(Sender<u64>),
    Messages {
        tile: Option<String>,
        since: u64,
        reply: Sender<Vec<crate::message::Message>>,
    },
    Message(u64, Sender<Option<crate::message::Message>>),
    /// What the owner holds of a snapshot, now.
    Snapshot(Sender<Held>),
    /// A new listener: registered at once, so no event after this reaches the stream without it.
    Subscribe {
        epoch: Option<String>,
        since: Option<u64>,
        reply: Sender<Subscribed>,
    },
    // ---- writes
    Edit(TileId, EditFn),
    /// The debug layer on or off; false when there is no such tile.
    SetMarks(TileId, bool, Sender<bool>),
    SetPlate(TileId, usize),
    Roots(Sender<Vec<Root>>),
    /// A place joins the list, last; answers its index.
    AddRoot(Root, Sender<usize>),
    /// The folders a scan found: answers whether they differ from the last scan's.
    SetFolders(BTreeMap<usize, Vec<String>>, Sender<bool>),
    Target(Sender<Options>),
    /// The build target changed: every built tile is built again, and the pages are told.
    SetTarget(Options),
    Progress {
        id: TileId,
        body: Box<api::Progress>,
        job: bool,
    },
    /// A tier's outcome, said to every listener.
    Tier {
        key: String,
        doc_rev: u64,
        error: Option<String>,
        mistake: bool,
        code: Option<String>,
    },
    BeginMessage(Begin),
    EndProgress(TileId),
    EndJob(TileId),
    Report {
        id: TileId,
        scope: crate::message::Scope,
        found: Vec<crate::message::Message>,
    },
    Say(Box<crate::message::Message>),
    /// Tell the resources that differ from what the pages were last told: the places, the target and the list from the
    /// owner's own state, `read` (the inventory, the printers) as the caller read them at read number `seq` (a value
    /// read before one already told is not told: the pages never go back to an older reading).
    Publish {
        seq: u64,
        read: Vec<Resource>,
    },
    /// The folders were scanned: tiles that are gone leave, new ones join, and when anything changed the
    /// resources are told.  Answers the ids that left.
    Reconcile {
        found: Vec<(usize, Vec<String>)>,
        fresh: Vec<Slot>,
        changed_elsewhere: bool,
        seq: u64,
        read: Vec<Resource>,
        reply: Sender<(bool, Vec<TileId>)>,
    },
    /// A place left the list: it and its tiles go, and the places after it move down.  Answers the ids that left.
    RemoveRoot(usize, Sender<Vec<TileId>>),
    #[cfg(test)]
    Inspect(Box<dyn FnOnce(&mut Core) + Send>),
}

/// A handle on the owner: what a transport holds.  Cheap to share; the owner stops when the last is dropped.
pub(super) struct Owner {
    tx: Sender<Command>,
}

impl Owner {
    pub(super) fn start(
        tiles: Vec<Slot>,
        roots: Vec<Root>,
        target: Options,
        home: Option<String>,
        epoch: String,
    ) -> Self {
        let (tx, rx) = channel::<Command>();
        let mut core = Core {
            tiles,
            events: Events {
                rev: 1,
                log: VecDeque::new(),
                clients: Vec::new(),
                messages: crate::message::Log::default(),
                told: resources::Told::default(),
            },
            roots,
            folders: BTreeMap::new(),
            target,
            home,
            epoch,
        };
        // the thread ends with the last handle: `recv` fails once every sender is gone
        let started = std::thread::Builder::new()
            .name("owner".to_owned())
            .spawn(move || {
                while let Ok(c) = rx.recv() {
                    // a closure that panics (a bug in a build's edit) is shown, and the state goes on:
                    // its asker, if any, finds its reply gone and says so
                    if let Err(p) = std::panic::catch_unwind(AssertUnwindSafe(|| core.apply(c))) {
                        tell!(
                            "error: preview: a change to the state panicked: {}",
                            panic_message(p.as_ref())
                        );
                    }
                }
            })
            .map(drop);
        if let Err(e) = started {
            panic!("the state owner's thread could not start: {e}");
        }
        Self { tx }
    }

    /// Send `c`: it is applied after every command this thread sent before it.
    pub(super) fn send(&self, c: Command) {
        if self.tx.send(c).is_err() {
            panic!("the state owner has stopped: a command could not be sent");
        }
    }

    /// Send the command `make` builds around a reply channel, and wait for its answer.
    pub(super) fn ask<R>(&self, make: impl FnOnce(Sender<R>) -> Command) -> R {
        let (tx, rx) = channel();
        self.send(make(tx));
        answer(&rx)
    }

    pub(super) fn edit(&self, id: &str, f: impl FnOnce(&mut Slot) + Send + 'static) {
        self.send(Command::Edit(id.to_owned(), Box::new(f)));
    }

    #[cfg(test)]
    pub(super) fn inspect<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Core) -> R + Send + 'static,
    ) -> R {
        let (tx, rx) = channel();
        self.send(Command::Inspect(Box::new(move |c| {
            // the asker is waiting on `rx`: a send cannot fail
            let _ = tx.send(f(c));
        })));
        answer(&rx)
    }
}

impl Core {
    fn slot(&self, id: &str) -> Option<&Slot> {
        self.tiles.iter().find(|t| t.id == id)
    }

    fn apply(&mut self, c: Command) {
        match c {
            Command::Facts(id, r) => reply(&r, self.slot(&id).map(Slot::facts)),
            Command::Summary(id, r) => reply(&r, self.slot(&id).map(Slot::summary)),
            Command::Plan(id, r) => reply(&r, self.slot(&id).and_then(|t| t.plan.clone())),
            Command::SceneNotes(id, r) => reply(
                &r,
                self.slot(&id)
                    .map(|t| t.scene_notes.clone())
                    .unwrap_or_default(),
            ),
            Command::Rows(r) => reply(&r, self.tiles.iter().map(Slot::row).collect()),
            Command::Paths(r) => reply(
                &r,
                self.tiles
                    .iter()
                    .map(|t| (t.root, t.path.clone()))
                    .collect(),
            ),
            Command::Mesh(id, doc_rev, r) => reply(&r, self.mesh(&id, doc_rev)),
            Command::SketchMesh(id, k, r) => reply(
                &r,
                self.slot(&id).and_then(|t| {
                    t.sketches
                        .iter()
                        .find(|s| s.id == k)
                        .map(|s| Arc::clone(&s.mesh))
                }),
            ),
            Command::Rev(r) => reply(&r, self.events.rev),
            Command::Messages {
                tile,
                since,
                reply: r,
            } => reply(
                &r,
                self.events
                    .messages
                    .query(tile.as_deref(), since, |_| true)
                    .into_iter()
                    .cloned()
                    .collect(),
            ),
            Command::Message(seq, r) => reply(
                &r,
                self.events
                    .messages
                    .all()
                    .iter()
                    .find(|m| m.seq == seq)
                    .cloned(),
            ),
            Command::Snapshot(r) => reply(&r, self.held()),
            Command::Subscribe {
                epoch,
                since,
                reply: r,
            } => {
                let sub = self.subscribe(epoch.as_deref(), since);
                reply(&r, sub);
            }
            Command::Edit(id, f) => self.update(&id, f),
            Command::SetMarks(id, on, r) => {
                let found = self.tiles.iter_mut().find(|t| t.id == id).map(|t| {
                    let was = std::mem::replace(&mut t.marks, on);
                    if on && !was && t.status != State::Idle {
                        t.stale = true;
                    }
                });
                reply(&r, found.is_some());
            }
            Command::SetPlate(id, n) => {
                if let Some(t) = self.tiles.iter_mut().find(|t| t.id == id) {
                    t.plate = n;
                }
            }
            Command::Roots(r) => reply(&r, self.roots.clone()),
            Command::AddRoot(root, r) => {
                self.roots.push(root);
                reply(&r, self.roots.len() - 1);
            }
            Command::SetFolders(folders, r) => {
                let differs = self.folders != folders;
                if differs {
                    self.folders = folders;
                }
                reply(&r, differs);
            }
            Command::Target(r) => reply(&r, self.target.clone()),
            Command::SetTarget(next) => {
                self.target = next;
                for t in self.tiles.iter_mut().filter(|t| t.status != State::Idle) {
                    t.stale = true;
                }
                self.publish(0, Vec::new());
            }
            Command::Progress { id, body, job } => self.progress(&id, *body, job),
            Command::Tier {
                key,
                doc_rev,
                error,
                mistake,
                code,
            } => {
                let rev = self.events.rev;
                self.events.send(&api::StreamEvent::Tier {
                    rev,
                    key,
                    doc_rev,
                    error,
                    mistake,
                    code,
                });
            }
            Command::BeginMessage(b) => self.begin_message(&b),
            Command::EndProgress(id) => {
                let gone = self.events.messages.end_progress(&id);
                self.send_messages(&gone);
            }
            Command::EndJob(id) => {
                let gone = self.events.messages.end_job(&id);
                self.send_messages(&gone);
            }
            Command::Report { id, scope, found } => {
                // a tile that left the list meanwhile reports nothing
                if self.slot(&id).is_some() {
                    let mut found = found;
                    for m in &mut found {
                        self.name_file(m);
                    }
                    let changed = self
                        .events
                        .messages
                        .report(fail::now_ms(), &id, scope, found);
                    self.send_messages(&changed);
                }
            }
            Command::Say(m) => {
                let mut m = *m;
                self.name_file(&mut m);
                let m = self.events.messages.say(fail::now_ms(), m);
                self.send_messages(&[m]);
            }
            Command::Publish { seq, read } => self.publish(seq, read),
            Command::Reconcile {
                found,
                fresh,
                changed_elsewhere,
                seq,
                read,
                reply: r,
            } => {
                reply(
                    &r,
                    self.reconcile(found, fresh, changed_elsewhere, seq, read),
                );
            }
            Command::RemoveRoot(i, r) => reply(&r, self.remove_root(i)),
            #[cfg(test)]
            Command::Inspect(f) => f(self),
        }
    }

    fn mesh(&self, id: &str, doc_rev: u64) -> Result<Option<Arc<Vec<u8>>>, u64> {
        let Some(t) = self.slot(id) else {
            return Ok(None);
        };
        match (&t.mesh, t.doc_rev) {
            (Some(m), Some(r)) if r == doc_rev => Ok(Some(Arc::clone(m))),
            (Some(_), r) => Err(r.unwrap_or(0)),
            (None, _) => Ok(None),
        }
    }

    /// Change tile `id` and tell every listening browser; a tile that is not listed is left alone.
    fn update(&mut self, id: &str, f: EditFn) {
        let Some(slot) = self.tiles.iter_mut().find(|t| t.id == id) else {
            return;
        };
        f(slot);
        let summary = slot.summary();
        let ev = &mut self.events;
        let rev = ev.bump(Change::Tile(id.to_owned()));
        ev.send(&api::StreamEvent::Tile {
            rev,
            tile: Box::new(summary),
        });
    }

    /// A tile's progress: remembered for a browser that connects later, and sent now.
    fn progress(&mut self, id: &str, body: api::Progress, job: bool) {
        let Some(slot) = self.tiles.iter_mut().find(|t| t.id == id) else {
            return;
        };
        // the document's own build, once it has a stage to say, is the work in flight until it ends
        if job
            && slot.status == State::Building
            && slot.progress.as_ref().is_some_and(|p| p.what == "building")
        {
            return;
        }
        slot.progress = Some(body.clone());
        let ev = &mut self.events;
        let rev = ev.rev;
        ev.send(&api::StreamEvent::Progress {
            rev,
            id: id.to_owned(),
            progress: body,
        });
    }

    /// A message about a tile names the tile's file when it names none of its own: a copy of it says which file.
    fn name_file(&self, m: &mut crate::message::Message) {
        if m.file.is_none() {
            m.file = self.slot(&m.tile).map(|t| t.path.clone());
        }
    }

    fn begin_message(&mut self, b: &Begin) {
        use crate::message::{Kind, Message, Scope, Severity};
        let Some(name) = self.slot(&b.id).map(|t| t.name.clone()) else {
            return;
        };
        let mut m = Message::new(
            Kind::Progress,
            Severity::Info,
            Scope::Build,
            &b.id,
            &name,
            (b.text)(&name),
        );
        if let Some(code) = b.code {
            m = m.code(code);
        }
        let m = self.events.messages.begin_progress(fail::now_ms(), m);
        self.send_messages(&[m]);
    }

    /// Tell every listener of `changed` messages.
    fn send_messages(&mut self, changed: &[crate::message::Message]) {
        let rev = self.events.rev;
        for m in changed {
            self.events.send(&api::StreamEvent::Message {
                rev,
                message: Box::new(m.wire()),
            });
        }
    }

    /// Every tile's state and what else the owner holds of the snapshot: the `/api/tiles` body and the first event.
    fn held(&self) -> Held {
        Held {
            rev: self.events.rev,
            epoch: self.epoch.clone(),
            target: self.target_value(),
            places: self.places(),
            tiles: self.tiles.iter().map(Slot::summary).collect(),
        }
    }

    /// The places: where tiles come from and the folders in them.
    fn places(&self) -> resources::Places {
        resources::Places {
            roots: self
                .roots
                .iter()
                .enumerate()
                .map(|(i, r)| r.json(i))
                .collect(),
            folders: self
                .folders
                .iter()
                .flat_map(|(r, v)| {
                    v.iter().map(move |p| api::FolderJson {
                        root: *r,
                        path: p.clone(),
                    })
                })
                .collect(),
            home: self.home.clone(),
        }
    }

    /// The build target the page chose.
    fn target_value(&self) -> resources::Target {
        let t = &self.target;
        resources::Target {
            system: t.system,
            overhang_deg: t.printer.overhang_deg,
            steepest_deg: t.printer.steepest_deg,
            square_mm: dt_kit::Frame::for_system(t.system, 1, 1).square_mm,
            printer: t.printer.name.clone(),
        }
    }

    /// Say what changed among `now` (the resources that may have) and the list of tiles: only what differs from
    /// what the pages were last told is sent.
    fn publish(&mut self, seq: u64, read: Vec<Resource>) {
        let mut now = vec![
            Resource::Places(self.places()),
            Resource::Target(self.target_value()),
        ];
        for r in read {
            let name = r.name();
            let told = self.events.told.read_at.entry(name).or_insert(0);
            // a reading older than one told already is dropped
            if *told <= seq {
                *told = seq;
                now.push(r);
            }
        }
        let ev = &mut self.events;
        let ids: Vec<TileId> = self.tiles.iter().map(|t| t.id.clone()).collect();
        if ids != ev.told.ids {
            let added = self
                .tiles
                .iter()
                .filter(|t| !ev.told.ids.contains(&t.id))
                .map(Slot::summary)
                .collect();
            now.push(Resource::List(resources::List {
                ids: ids.clone(),
                added,
            }));
            ev.told.ids = ids;
        }
        for r in now {
            let name = r.name();
            // a list is told when its ids changed (decided above); the rest when their value did
            let text = match &r {
                Resource::List(l) => Ok(l.ids.join(",")),
                other => serde_json::to_string(other),
            };
            match text {
                Ok(text) => {
                    if ev.told.values.get(name) == Some(&text) {
                        continue;
                    }
                    ev.told.values.insert(name, text);
                }
                // said, and sent all the same: a value that cannot be written is never taken for one unchanged
                Err(e) => {
                    tell!(
                        "error: preview: the {name} resource could not be written to compare: {e}"
                    );
                    ev.told.values.remove(name);
                }
            }
            let res_rev = {
                let n = ev.told.revs.entry(name).or_insert(0);
                *n += 1;
                *n
            };
            let rev = ev.bump(Change::Resource(name));
            let event = api::StreamEvent::Resource(resources::ResourceEvent {
                rev,
                res_rev,
                resource: r,
            });
            match serde_json::to_string(&event) {
                Ok(s) => {
                    ev.told.events.insert(name, s);
                }
                Err(e) => tell!("error: preview: a resource could not be written: {e}"),
            }
            ev.send(&event);
        }
    }

    fn reconcile(
        &mut self,
        found: Vec<(usize, Vec<String>)>,
        fresh: Vec<Slot>,
        changed_elsewhere: bool,
        seq: u64,
        read: Vec<Resource>,
    ) -> (bool, Vec<TileId>) {
        let before = self.tiles.len();
        let mut removed = Vec::new();
        for (r, paths) in found {
            let have: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
            self.tiles.retain(|t| {
                let keep = t.root != r || have.contains(t.path.as_str());
                if !keep {
                    removed.push(t.id.clone());
                }
                keep
            });
        }
        for slot in fresh {
            if !self
                .tiles
                .iter()
                .any(|t| t.root == slot.root && t.path == slot.path)
            {
                self.tiles.push(slot);
            }
        }
        let changed = self.tiles.len() != before || !removed.is_empty() || changed_elsewhere;
        if changed {
            self.publish(seq, read);
        }
        for id in &removed {
            self.events.messages.forget(id);
        }
        (changed, removed)
    }

    fn remove_root(&mut self, i: usize) -> Vec<TileId> {
        self.roots.remove(i);
        self.folders = std::mem::take(&mut self.folders)
            .into_iter()
            .filter(|(r, _)| *r != i)
            .map(|(r, v)| (if r > i { r - 1 } else { r }, v))
            .collect();
        let mut gone = Vec::new();
        self.tiles.retain(|t| {
            let here = t.root == i;
            if here {
                gone.push(t.id.clone());
            }
            !here
        });
        for t in self.tiles.iter_mut().filter(|t| t.root > i) {
            t.root -= 1;
        }
        for id in &gone {
            self.events.messages.forget(id);
        }
        gone
    }

    /// A new listener.  With the epoch and last revision of a stream that dropped, and the log still holding
    /// everything since, it is sent only what changed (one event per tile, as it is now); otherwise the snapshot.
    /// Registered in the same step the first events are made, so no change falls between them and the rest.
    fn subscribe(&mut self, epoch: Option<&str>, since: Option<u64>) -> Subscribed {
        let (tx, rx) = channel();
        let ev = &self.events;
        let mut lead = Vec::new();
        let mut fresh = None;
        let mut resumed = false;
        if let (Some(e), Some(since)) = (epoch, since)
            && *e == self.epoch
            && since <= ev.rev
            && (since == ev.rev || ev.log.front().is_some_and(|(r, _)| *r <= since + 1))
        {
            let mut ids = Vec::new();
            let mut list = false;
            let mut names: Vec<&'static str> = Vec::new();
            for (_, c) in ev.log.iter().filter(|(r, _)| *r > since) {
                match c {
                    // tiles that joined or left: the page starts over from the whole
                    Change::Resource("list") => list = true,
                    Change::Resource(n) if !names.contains(n) => names.push(n),
                    Change::Resource(_) => {}
                    Change::Tile(id) if !ids.contains(id) => ids.push(id.clone()),
                    Change::Tile(_) => {}
                }
            }
            if !list {
                let rev = ev.rev;
                lead.extend(encode(&api::StreamEvent::Resumed {
                    rev,
                    epoch: self.epoch.clone(),
                }));
                // the last of each resource that changed since, not the whole
                for n in &names {
                    if let Some(event) = ev.told.events.get(n) {
                        lead.push(event.clone());
                    }
                }
                for id in ids {
                    if let Some(t) = self.tiles.iter().find(|t| t.id == id) {
                        lead.extend(encode(&api::StreamEvent::Tile {
                            rev,
                            tile: Box::new(t.summary()),
                        }));
                    }
                }
                resumed = true;
            }
        }
        if !resumed {
            fresh = Some(self.held());
        }
        let messages = self
            .events
            .messages
            .all()
            .iter()
            .filter_map(|m| {
                encode(&api::StreamEvent::Message {
                    rev: self.events.rev,
                    message: Box::new(m.wire()),
                })
            })
            .collect();
        self.events.clients.push(tx);
        Subscribed {
            rx,
            lead,
            fresh,
            messages,
        }
    }
}

/// The answer to a command, waited for.  The owner not answering means it has stopped: a failure of the program, not
/// something to go on without.
fn answer<R>(rx: &Receiver<R>) -> R {
    match rx.recv() {
        Ok(r) => r,
        Err(_) => panic!("the state owner stopped before it answered"),
    }
}

/// An answer to a command.  Dropped silently when its asker is gone: a request that gave up has no use for it.
fn reply<T>(to: &Sender<T>, answer: T) {
    // Allowed (you cancelled): the asker may have gone already
    let _ = to.send(answer);
}

/// An event as the stream sends it.  One that cannot be written is said, loudly, and not sent.
pub(super) fn encode(event: &api::StreamEvent) -> Option<String> {
    match serde_json::to_string(event) {
        Ok(s) => Some(s),
        Err(e) => {
            tell!("error: preview: an event could not be written: {e}");
            None
        }
    }
}

/// A listener's stream: what it is sent first, then every event after, as the owner sends them.
pub(super) struct Subscription {
    head: std::cell::RefCell<VecDeque<String>>,
    rx: Receiver<String>,
}

impl Subscription {
    pub(super) fn new(head: VecDeque<String>, rx: Receiver<String>) -> Self {
        Self {
            head: std::cell::RefCell::new(head),
            rx,
        }
    }

    /// The next event, waiting up to `wait` for one.
    pub(super) fn recv_timeout(
        &self,
        wait: Duration,
    ) -> Result<String, std::sync::mpsc::RecvTimeoutError> {
        if let Some(m) = self.head.borrow_mut().pop_front() {
            return Ok(m);
        }
        self.rx.recv_timeout(wait)
    }

    /// What is there now, without waiting.
    #[cfg(test)]
    pub(super) fn try_iter(&self) -> impl Iterator<Item = String> + '_ {
        let head: Vec<String> = self.head.borrow_mut().drain(..).collect();
        head.into_iter().chain(self.rx.try_iter())
    }
}

impl Slot {
    fn facts(&self) -> Facts {
        Facts {
            path: self.path.clone(),
            name: self.name.clone(),
            files: self.files.clone(),
            plate: self.plate,
            has_mesh: self.mesh.is_some(),
            marks: self.marks,
            idle: self.status == State::Idle,
        }
    }

    pub(super) fn row(&self) -> Row {
        let idle = self.status == State::Idle;
        Row {
            id: self.id.clone(),
            root: self.root,
            idle,
            building: self.status == State::Building,
            stale: self.stale,
            files: if idle {
                BTreeMap::new()
            } else {
                self.files.clone()
            },
        }
    }
}
