//! The build queue: every build the service does for the page, in one place.  Three kinds, in priority order: a
//! *draft* (the page asked for a quick build of the text it holds), a tile *asked for* (chosen, new, renamed, copied),
//! and a tile whose files *changed* (or, eagerly, a file named on the command line not built yet).
//!
//! A full build is heavy, so one runs at a time; a draft runs beside it.  Both are taken from here by identical
//! worker lanes of the transport.
//!
//! The rest of the building the page asks for has its lanes here too, so no request builds on its own thread: a map
//! piece's *tier* build (and a crown's look at a piece), two at a time, and an *export* (a tile's, a print list's), one
//! at a time.  What a side lane runs is a typed job value of the host ([`Host::Job`]), which the owner only holds and
//! orders; a request for a newer revision of the same thing sets the older one's cancel token (a waiting job is
//! dropped, a running tier build stops at its next progress report), and a tile leaving the list cancels its jobs.
//! Each running build holds a cancel token the build looks at as it reports progress: a newer draft of the same tile
//! sets the older one's, and a tile leaving the list sets its build's.

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use crate::api::DraftTier;
use crate::host::Host;
use crate::owner::Core;
use crate::roots::RootKind;
use crate::tile::{Slot, State, TileId};

/// What a draft's requester gets back.
pub enum Outcome<H: Host> {
    /// The draft's mesh (a [`payload`] buffer).
    Mesh(Arc<Vec<u8>>),
    /// The text has a mistake: the check says where.
    Mistake,
    /// A newer draft of the tile (or the tile leaving) made this one moot.
    Superseded,
    /// The build panicked: a bug in an element, with the panic's words.
    Failed(Box<H::Failure>),
}

/// A draft the page asked for, waiting or running.
pub struct Draft<H: Host> {
    pub tile: TileId,
    pub path: String,
    /// The file the text is of (the tile's own, or one it uses).
    pub file: String,
    pub text: String,
    /// The page has the debug layer on: collect its marks.
    pub marks: bool,
    /// How coarse to build it.
    pub tier: DraftTier,
    pub cancel: Arc<AtomicBool>,
    pub reply: mpsc::Sender<Outcome<H>>,
}

/// What a worker lane takes.
pub enum Job<H: Host> {
    Draft(Draft<H>),
    Full(TileId, Arc<AtomicBool>),
}

/// The lanes of the work that is not a tile's own build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lane {
    /// A piece at a tier: quick, two at a time.
    Tier,
    /// A written file: heavy, one at a time.
    Export,
}

impl Lane {
    pub const fn limit(self) -> usize {
        match self {
            Self::Tier => 2,
            Self::Export => 1,
        }
    }
}

/// A job for a side lane, waiting.
pub struct Side<H: Host> {
    pub lane: Lane,
    /// What it is a build of (the tile, the tier, the seed…): a newer revision of the same key replaces it.
    pub key: String,
    pub rev: u64,
    pub tile: TileId,
    pub cancel: Arc<AtomicBool>,
    /// The typed job the host's lane runs.
    pub job: H::Job,
}

/// A side job running.
pub struct Running {
    lane: Lane,
    key: String,
    rev: u64,
    tile: TileId,
    cancel: Arc<AtomicBool>,
}

/// What a lane asks the owner for.
#[derive(Clone, Copy)]
pub enum Taking {
    /// A tile's own build: a draft, else (when no full build runs) a tile asked for, `eager` the named files not built
    /// yet, a tile whose files changed.
    Main { eager: bool },
    /// A side job of this lane's kind.
    Side(Lane),
}

/// What a lane is given.
pub enum Taken<H: Host> {
    Job(Job<H>),
    Side(Side<H>),
}

/// A lane's request, waiting for work: answered the moment there is some.
pub struct Parked<H: Host> {
    pub ticket: u64,
    pub kind: Taking,
    pub reply: mpsc::Sender<Taken<H>>,
}

/// A lane is done with what it took.
pub enum Finished {
    Full(TileId),
    Draft(Arc<AtomicBool>),
    Side(Arc<AtomicBool>),
}

/// Every build waiting or running, as the owner holds them.
pub struct Queue<H: Host> {
    pub side: VecDeque<Side<H>>,
    pub(crate) side_running: Vec<Running>,
    pub(crate) drafts: VecDeque<Draft<H>>,
    /// Tiles asked for, in order.
    pub(crate) wanted: VecDeque<TileId>,
    /// The full build running, with its token.
    pub(crate) full: Option<(TileId, Arc<AtomicBool>)>,
    /// The drafts running.
    pub(crate) drafting: Vec<(TileId, Arc<AtomicBool>)>,
    /// Lanes waiting for work.
    pub parked: Vec<Parked<H>>,
    /// Tiles whose files changed on disk, as the last look at them found (see [`Shared::look_for_changes`]).
    pub(crate) changed: BTreeSet<TileId>,
    /// How many full builds have ended: a look at the files made before the last one ended is out of date.
    finished: u64,
}

impl<H: Host> Default for Queue<H> {
    fn default() -> Self {
        Self {
            side: VecDeque::new(),
            side_running: Vec::new(),
            drafts: VecDeque::new(),
            wanted: VecDeque::new(),
            full: None,
            drafting: Vec::new(),
            parked: Vec::new(),
            changed: BTreeSet::new(),
            finished: 0,
        }
    }
}

impl<H: Host> Queue<H> {
    /// How much work is queued or running: drafts and full builds waiting or running, tiles asked for, side jobs.
    pub fn count(&self) -> usize {
        self.drafts.len()
            + self.wanted.len()
            + usize::from(self.full.is_some())
            + self.drafting.len()
            + self.side.len()
            + self.side_running.len()
    }

    pub fn finished(&self) -> u64 {
        self.finished
    }

    /// The files looked at when `finished` builds had ended; out of date when more have ended since.
    pub fn set_changed(&mut self, finished: u64, ids: BTreeSet<TileId>) {
        if finished == self.finished {
            self.changed.extend(ids);
        }
    }

    /// Tiles whose files the program itself changed.
    pub fn add_changed(&mut self, ids: impl IntoIterator<Item = TileId>) {
        self.changed.extend(ids);
    }

    /// Ask for tile `id` to be built next.
    pub fn want(&mut self, id: &str) {
        if !self.wanted.iter().any(|w| w == id) {
            self.wanted.push_back(id.to_owned());
        }
    }

    /// A draft of a tile replaces any older one of it: waiting ones are answered [`Outcome::Superseded`] at once, a
    /// running one stops at its next progress report.
    pub fn add_draft(&mut self, d: Draft<H>) {
        self.drafts.retain(|o| {
            let same = o.tile == d.tile;
            if same {
                // Allowed (you cancelled): the requester may have gone already
                let _ = o.reply.send(Outcome::Superseded);
            }
            !same
        });
        for (t, cancel) in &self.drafting {
            if *t == d.tile {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        self.drafts.push_back(d);
    }

    /// A waiting job of the same thing at an older revision is dropped (its requester finds its sender gone), a
    /// running one is told to stop.
    pub fn add_side(&mut self, s: Side<H>) {
        self.side
            .retain(|o| !(o.lane == s.lane && o.key == s.key && o.rev < s.rev));
        for r in &self.side_running {
            if r.lane == s.lane && r.key == s.key && r.rev < s.rev {
                r.cancel.store(true, Ordering::Relaxed);
            }
        }
        self.side.push_back(s);
    }

    /// Nobody wants `key` on `lane` any more: a waiting job is dropped, a running one is told to stop.
    pub fn cancel_lane(&mut self, lane: Lane, key: &str) {
        self.side.retain(|s| !(s.lane == lane && s.key == key));
        for r in &self.side_running {
            if r.lane == lane && r.key == key {
                r.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    pub fn finish(&mut self, done: Finished) {
        match done {
            Finished::Full(id) => {
                self.full = None;
                self.changed.remove(&id);
                self.finished += 1;
            }
            Finished::Draft(c) => self.drafting.retain(|(_, d)| !Arc::ptr_eq(d, &c)),
            Finished::Side(c) => self.side_running.retain(|r| !Arc::ptr_eq(&r.cancel, &c)),
        }
    }

    /// Tile `id` has left the list: its builds and drafts stop.
    pub fn forget(&mut self, id: &str) {
        self.wanted.retain(|w| w != id);
        self.changed.remove(id);
        self.drafts.retain(|d| {
            let same = d.tile == id;
            if same {
                // Allowed (you cancelled): the requester may have gone already
                let _ = d.reply.send(Outcome::Superseded);
            }
            !same
        });
        for (t, cancel) in self.full.iter().chain(self.drafting.iter()) {
            if t == id {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        self.side.retain(|s| s.tile != id);
        for r in &self.side_running {
            if r.tile == id {
                r.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    /// What goes back when a lane that was sent it is gone: the next lane takes it.
    pub fn give_back(&mut self, taken: Taken<H>) {
        match taken {
            Taken::Job(Job::Draft(d)) => {
                self.drafting.retain(|(_, c)| !Arc::ptr_eq(c, &d.cancel));
                self.drafts.push_front(d);
            }
            Taken::Job(Job::Full(id, _)) => {
                self.full = None;
                self.wanted.push_front(id);
            }
            Taken::Side(s) => {
                self.side_running
                    .retain(|r| !Arc::ptr_eq(&r.cancel, &s.cancel));
                self.side.push_front(s);
            }
        }
    }

    /// Whether a side job is waiting (not yet taken by its lane).
    pub fn side_waiting(&self) -> bool {
        !self.side.is_empty()
    }
}

impl<H: Host> Core<H> {
    /// What the next job for `kind` is, taken from the queue.  `tiles` are the tiles listed: a tile asked for that
    /// is not among them is dropped.
    fn next_for(&mut self, kind: Taking) -> Option<Taken<H>> {
        match kind {
            Taking::Main { eager } => {
                if let Some(d) = self.queue.drafts.pop_front() {
                    self.queue
                        .drafting
                        .push((d.tile.clone(), Arc::clone(&d.cancel)));
                    return Some(Taken::Job(Job::Draft(d)));
                }
                if self.queue.full.is_none()
                    && let Some(id) = self.pick_full(eager)
                {
                    let cancel = Arc::new(AtomicBool::new(false));
                    self.queue.full = Some((id.clone(), Arc::clone(&cancel)));
                    return Some(Taken::Job(Job::Full(id, cancel)));
                }
                None
            }
            Taking::Side(lane) => {
                let q = &mut self.queue;
                let running = q.side_running.iter().filter(|r| r.lane == lane).count();
                if running < lane.limit()
                    && let Some(i) = q.side.iter().position(|s| s.lane == lane)
                {
                    let s = q.side.remove(i)?;
                    q.side_running.push(Running {
                        lane,
                        key: s.key.clone(),
                        rev: s.rev,
                        tile: s.tile.clone(),
                        cancel: Arc::clone(&s.cancel),
                    });
                    return Some(Taken::Side(s));
                }
                None
            }
        }
    }

    /// A tile asked for; else (when `eager`) the first named file not built yet; else a tile built for a target
    /// that is not the chosen one, or whose files changed.  Folder and library tiles are built only when asked for.
    fn pick_full(&mut self, eager: bool) -> Option<TileId> {
        while let Some(id) = self.queue.wanted.pop_front() {
            if self.tiles.iter().any(|t| t.id == id) {
                return Some(id);
            }
        }
        let named = |t: &Slot| {
            self.roots
                .get(t.root)
                .is_none_or(|r| r.kind == RootKind::Files)
        };
        if eager
            && let Some(t) = self
                .tiles
                .iter()
                .find(|t| t.status == State::Idle && named(t))
        {
            return Some(t.id.clone());
        }
        let id = self
            .tiles
            .iter()
            .find(|t| t.status != State::Idle && (t.stale || self.queue.changed.contains(&t.id)))
            .map(|t| t.id.clone())?;
        self.queue.changed.remove(&id);
        Some(id)
    }

    /// Give every lane that waits the work there is for it.  Called after every command: any of them may have made
    /// some (a tile asked for, a draft, a file that changed, a lane that finished).
    pub fn dispatch(&mut self) {
        let mut i = 0;
        while i < self.queue.parked.len() {
            let kind = self.queue.parked[i].kind;
            let Some(taken) = self.next_for(kind) else {
                i += 1;
                continue;
            };
            let lane = self.queue.parked.remove(i);
            if let Err(mpsc::SendError(back)) = lane.reply.send(taken) {
                // the lane is gone (it stopped): what it was sent goes back
                self.queue.give_back(back);
                // and the next lane may have it
                i = 0;
            }
        }
    }
}
