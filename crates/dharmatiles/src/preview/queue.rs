//! The build queue: every build the program does for the page, in one
//! place.  Three kinds, in priority order: a *draft* (the page asked for a
//! quick build of the text it holds), a tile *asked for* (chosen, new,
//! renamed, copied), and a tile whose files *changed* (or, eagerly, a file
//! named on the command line not built yet).
//!
//! A full build is heavy, so one runs at a time; a draft runs beside it.
//! Both are taken from here by identical worker lanes (see [`super::worker`]).
//!
//! The rest of the building the page asks for has its lanes here too, so no
//! request builds on its own thread: a map piece's *tier* build (and a crown's
//! look at a piece), two at a time, and an *export* (a tile's, a print list's),
//! one at a time.  A request waits on its job ([`Shared::on_lane`]); a request
//! for a newer revision of the same thing sets the older one's cancel token (a
//! waiting job is dropped, a running tier build stops at its next progress
//! report), and a tile leaving the list cancels its jobs.
//! Each running build holds a cancel token the build looks at as it
//! reports progress: a newer draft of the same tile sets the older one's,
//! and a tile leaving the list sets its build's.

use std::sync::mpsc::{self, Receiver};

use super::owner::Command;
use super::*;

/// What a draft's requester gets back.
pub(super) enum Outcome {
    /// The draft's mesh (a [`payload`] buffer).
    Mesh(Arc<Vec<u8>>),
    /// The text has a mistake: the check says where.
    Mistake,
    /// A newer draft of the tile (or the tile leaving) made this one moot.
    Superseded,
    /// The build panicked: a bug in an element, with the panic's words.
    Failed(Box<fail::ServerError>),
}

/// A draft the page asked for, waiting or running.
pub(super) struct Draft {
    pub tile: TileId,
    pub path: String,
    /// The file the text is of (the tile's own, or one it uses).
    pub file: String,
    pub text: String,
    /// The page has the debug layer on: collect its marks.
    pub marks: bool,
    /// How coarse to build it.
    pub tier: super::edit::DraftTier,
    pub cancel: Arc<AtomicBool>,
    pub reply: mpsc::Sender<Outcome>,
}

/// What a worker lane takes.
pub(super) enum Job {
    Draft(Draft),
    Full(TileId, Arc<AtomicBool>),
}

/// The lanes of the work that is not a tile's own build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Lane {
    /// A piece at a tier: quick, two at a time.
    Tier,
    /// A written file: heavy, one at a time.
    Export,
}

impl Lane {
    const fn limit(self) -> usize {
        match self {
            Self::Tier => 2,
            Self::Export => 1,
        }
    }
}

/// How a job on a lane ended without an answer.
pub(super) enum Interrupted {
    /// A request for a newer revision of the same thing took its place, or its tile left.
    Superseded,
    /// The job panicked: the requester re-raises it, so it is answered as any panic in a route is.
    Panicked(Box<dyn std::any::Any + Send>),
}

pub(super) type SideRun = Box<dyn FnOnce(&Shared, &Arc<AtomicBool>) + Send>;

/// A job for a side lane, waiting.
pub(super) struct Side {
    lane: Lane,
    /// What it is a build of (the tile, the tier, the seed…): a newer revision of the same key replaces it.
    key: String,
    rev: u64,
    tile: TileId,
    cancel: Arc<AtomicBool>,
    run: SideRun,
}

/// A side job running.
struct Running {
    lane: Lane,
    key: String,
    rev: u64,
    tile: TileId,
    cancel: Arc<AtomicBool>,
}

/// A reporter that does nothing but stop a build whose cancel token is set (see [`Shared::on_lane`]).
pub(super) struct Cancelled(pub Arc<AtomicBool>);

impl dt_kit::Reporter for Cancelled {
    fn report(&self, _: &dt_kit::Event<'_>) {
        if self.0.load(Ordering::Relaxed) {
            std::panic::resume_unwind(Box::new(Superseded));
        }
    }
}

/// What a lane asks the owner for.
#[derive(Clone, Copy)]
pub(super) enum Taking {
    /// A tile's own build: a draft, else (when no full build runs) a tile asked for, `eager` the named files not built
    /// yet, a tile whose files changed.
    Main { eager: bool },
    /// A side job of this lane's kind.
    Side(Lane),
}

/// What a lane is given.
pub(super) enum Taken {
    Job(Job),
    Side(Side),
}

/// A lane's request, waiting for work: answered the moment there is some.
pub(super) struct Parked {
    pub ticket: u64,
    pub kind: Taking,
    pub reply: mpsc::Sender<Taken>,
}

/// A lane is done with what it took.
pub(super) enum Finished {
    Full(TileId),
    Draft(Arc<AtomicBool>),
    Side(Arc<AtomicBool>),
}

/// Every build waiting or running, as the owner holds them.
#[derive(Default)]
pub(super) struct Queue {
    pub(super) side: VecDeque<Side>,
    side_running: Vec<Running>,
    drafts: VecDeque<Draft>,
    /// Tiles asked for, in order.
    wanted: VecDeque<TileId>,
    /// The full build running, with its token.
    full: Option<(TileId, Arc<AtomicBool>)>,
    /// The drafts running.
    drafting: Vec<(TileId, Arc<AtomicBool>)>,
    /// Lanes waiting for work.
    pub(super) parked: Vec<Parked>,
    /// Tiles whose files changed on disk, as the last look at them found (see [`Shared::look_for_changes`]).
    changed: BTreeSet<TileId>,
    /// How many full builds have ended: a look at the files made before the last one ended is out of date.
    finished: u64,
}

impl Queue {
    /// How much work is queued or running: drafts and full builds waiting or running, tiles asked for, side jobs.
    #[cfg(any(test, feature = "test-routes"))]
    pub(super) fn count(&self) -> usize {
        self.drafts.len()
            + self.wanted.len()
            + usize::from(self.full.is_some())
            + self.drafting.len()
            + self.side.len()
            + self.side_running.len()
    }

    pub(super) fn finished(&self) -> u64 {
        self.finished
    }

    /// The files looked at when `finished` builds had ended; out of date when more have ended since.
    pub(super) fn set_changed(&mut self, finished: u64, ids: BTreeSet<TileId>) {
        if finished == self.finished {
            self.changed.extend(ids);
        }
    }

    /// Tiles whose files the program itself changed.
    pub(super) fn add_changed(&mut self, ids: impl IntoIterator<Item = TileId>) {
        self.changed.extend(ids);
    }

    /// Ask for tile `id` to be built next.
    pub(super) fn want(&mut self, id: &str) {
        if !self.wanted.iter().any(|w| w == id) {
            self.wanted.push_back(id.to_owned());
        }
    }

    /// A draft of a tile replaces any older one of it: waiting ones are answered [`Outcome::Superseded`] at once, a
    /// running one stops at its next progress report.
    pub(super) fn add_draft(&mut self, d: Draft) {
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
    pub(super) fn add_side(&mut self, s: Side) {
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
    pub(super) fn cancel_lane(&mut self, lane: Lane, key: &str) {
        self.side.retain(|s| !(s.lane == lane && s.key == key));
        for r in &self.side_running {
            if r.lane == lane && r.key == key {
                r.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn finish(&mut self, done: Finished) {
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
    pub(super) fn forget(&mut self, id: &str) {
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
    fn give_back(&mut self, taken: Taken) {
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
    #[cfg(test)]
    pub(super) fn side_waiting(&self) -> bool {
        !self.side.is_empty()
    }
}

impl super::owner::Core {
    /// What the next job for `kind` is, taken from the queue.  `tiles` are the tiles listed: a tile asked for that
    /// is not among them is dropped.
    fn next_for(&mut self, kind: Taking) -> Option<Taken> {
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
    pub(super) fn dispatch(&mut self) {
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

impl Shared {
    /// Ask for tile `id` to be built next (a no-op once it has been
    /// built).  False when there is no such tile.
    pub(super) fn want(&self, id: &str) -> bool {
        self.owner.ask(|r| Command::Want(id.to_owned(), r))
    }

    /// Queue a draft of tile `id` from `text`, ahead of every full build,
    /// replacing any older draft of the same tile (waiting ones are
    /// answered [`Outcome::Superseded`] at once, a running one stops at its
    /// next progress report).  The answer comes on the receiver.
    pub(super) fn submit_draft(
        &self,
        id: &str,
        path: &str,
        file: &str,
        text: &str,
        marks: bool,
        tier: super::edit::DraftTier,
    ) -> Receiver<Outcome> {
        let (reply, rx) = mpsc::channel();
        let draft = Draft {
            tile: id.to_owned(),
            path: path.to_owned(),
            file: file.to_owned(),
            text: text.to_owned(),
            marks,
            tier,
            cancel: Arc::new(AtomicBool::new(false)),
            reply,
        };
        self.owner
            .ask(|done| Command::SubmitDraft(Box::new(draft), done));
        rx
    }

    /// A lane's wait for `kind` of work: the answer when there is some, none after `wait`.  The request is parked on
    /// the owner, which answers it the moment there is work; one withdrawn at the end of the wait is answered or not at
    /// all, so no job is sent to a lane that has gone.
    fn take(&self, kind: queue::Taking, wait: Duration) -> Option<queue::Taken> {
        let ticket = self.take_seq.fetch_add(1, Ordering::Relaxed);
        let (reply, rx) = mpsc::channel();
        self.owner.send(Command::Take {
            ticket,
            kind,
            reply,
        });
        match rx.recv_timeout(wait) {
            Ok(t) => Some(t),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.owner.ask(|done| Command::Withdraw(ticket, done));
                // one sent before the owner took the request back is waiting here; the request taken back is
                // dropped, so an empty channel is a disconnected one: nothing came
                rx.try_iter().next()
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!("the state owner stopped while a lane waited for work")
            }
        }
    }

    /// The next job for a lane: a draft; else, when no full build is
    /// running, one asked for, else (when `eager`) the first named file
    /// not built yet, else a tile whose files changed.  Folder and library
    /// tiles are built only when asked for.  With nothing to do it waits
    /// `wait` (the owner answers the moment there is a job) and says `None`.
    pub(super) fn next_job_within(&self, eager: bool, wait: Duration) -> Option<Job> {
        match self.take(queue::Taking::Main { eager }, wait)? {
            queue::Taken::Job(j) => Some(j),
            queue::Taken::Side(_) => unreachable!("a main lane is never given a side job"),
        }
    }

    /// [`Shared::next_job_within`] for one [`POLL`]: the tests' look at what is next.
    #[cfg(test)]
    pub(super) fn next_job(&self, eager: bool) -> Option<Job> {
        self.next_job_within(eager, POLL)
    }

    /// How much work the server has not finished: drafts and full builds waiting or running, tiles asked for, and tiles
    /// building or whose files changed and have not been picked up.  0 is idle.  A test that wants "nothing more will
    /// happen" waits for this (`GET /api/test-idle`), never for a fixed time.
    #[cfg(any(test, feature = "test-routes"))]
    pub(super) fn pending(&self) -> usize {
        let queued = self.owner.ask(Command::Queued);
        queued
            + self
                .rows()
                .iter()
                .filter(|t| t.building || t.changed())
                .count()
    }

    /// Run `f` on `lane` and wait for its answer.  `key` says what it is a build of and `rev` which revision of it: a
    /// job of the same key and an older revision, waiting or running, is cancelled by this one.  `f` is given the
    /// program and its cancel token (a long build looks at it through [`Cancelled`]).
    ///
    /// # Errors
    /// [`Interrupted`]: superseded (also when it was cancelled while running and stopped), or the job panicked.
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: user cancel (a closed channel means the job was dropped by a newer revision of the same key, so it was superseded)"
    )]
    pub(super) fn on_lane<T: Send + 'static>(
        &self,
        lane: Lane,
        key: String,
        rev: u64,
        tile: &str,
        f: impl FnOnce(&Shared, &Arc<AtomicBool>) -> Result<T, Interrupted> + Send + 'static,
    ) -> Result<T, Interrupted> {
        let (tx, rx) = mpsc::channel();
        let run: SideRun = Box::new(move |shared, cancel| {
            let ran = std::panic::catch_unwind(AssertUnwindSafe(|| f(shared, cancel)));
            let answer = match ran {
                Ok(r) => r,
                Err(p) if p.is::<Superseded>() => Err(Interrupted::Superseded),
                Err(p) => Err(Interrupted::Panicked(p)),
            };
            // Allowed (you cancelled): the requester may have gone already
            let _ = tx.send(answer);
        });
        self.submit_lane(lane, key, rev, tile, run);
        rx.recv().unwrap_or(Err(Interrupted::Superseded))
    }

    /// Queue `run` on `lane` and go on: nothing waits for it (its own code says what it made).  The same rule as
    /// [`Shared::on_lane`] for a job of the same key at an older revision.
    pub(super) fn submit_lane(&self, lane: Lane, key: String, rev: u64, tile: &str, run: SideRun) {
        self.owner.send(Command::SubmitSide(Side {
            lane,
            key,
            rev,
            tile: tile.to_owned(),
            cancel: Arc::new(AtomicBool::new(false)),
            run,
        }));
    }

    /// Nobody wants `key` on `lane` any more: a waiting job is dropped, a running one is told to stop.
    pub(super) fn cancel_lane(&self, lane: Lane, key: &str) {
        self.owner.send(Command::CancelLane(lane, key.to_owned()));
    }

    /// The next job for a side lane, when fewer than the lane's limit are running; with nothing to do it waits up to
    /// [`LANE_WAIT`] for one and says `None`.
    pub(super) fn next_side(&self, lane: Lane) -> Option<Side> {
        match self.take(queue::Taking::Side(lane), LANE_WAIT)? {
            queue::Taken::Side(s) => Some(s),
            queue::Taken::Job(_) => unreachable!("a side lane is never given a tile's build"),
        }
    }

    /// Is a side job waiting (not yet taken by its lane)?  For the tests of the lanes.
    #[cfg(test)]
    pub(super) fn pending_has_side_waiting(&self) -> bool {
        self.owner.ask(Command::SideWaiting)
    }

    /// A side lane runs `job`, then is done with it.
    pub(super) fn run_side(&self, job: Side) {
        let cancel = Arc::clone(&job.cancel);
        (job.run)(self, &cancel);
        self.owner.send(Command::Finish(Finished::Side(cancel)));
    }

    /// A lane is done with `job`.
    pub(super) fn finish(&self, job: &Job) {
        self.owner.send(Command::Finish(match job {
            Job::Full(id, _) => Finished::Full(id.clone()),
            Job::Draft(d) => Finished::Draft(Arc::clone(&d.cancel)),
        }));
    }

    /// Look at the files of the tiles built, and say which have changed since: the one place the program polls the
    /// disk for a change it did not make (every [`POLL`], from [`worker::work`]).  A change the program makes itself
    /// (a save, a draft's target) is told to the owner by the call that makes it, and needs no look.
    pub(super) fn look_for_changes(&self) {
        let (finished, rows) = self.owner.ask(Command::Watch);
        let ids = rows
            .iter()
            .filter(|r| r.changed())
            .map(|r| r.id.clone())
            .collect();
        self.owner.send(Command::Changed { finished, ids });
    }
}
