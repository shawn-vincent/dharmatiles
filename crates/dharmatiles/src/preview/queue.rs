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

#[derive(Default)]
pub(super) struct Queue {
    side: VecDeque<Side>,
    side_running: Vec<Running>,
    drafts: VecDeque<Draft>,
    /// Tiles asked for, in order.
    wanted: VecDeque<TileId>,
    /// The full build running, with its token.
    full: Option<(TileId, Arc<AtomicBool>)>,
    /// The drafts running.
    drafting: Vec<(TileId, Arc<AtomicBool>)>,
}

impl Shared {
    /// Ask for tile `id` to be built next (a no-op once it has been
    /// built).  False when there is no such tile.
    pub(super) fn want(&self, id: &str) -> bool {
        let idle = lock(&self.tiles)
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.status == State::Idle);
        if idle == Some(true) {
            let mut q = lock(&self.queue);
            if !q.wanted.iter().any(|w| w == id) {
                q.wanted.push_back(id.to_owned());
            }
            self.queue_cv.notify_all();
        }
        idle.is_some()
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
    ) -> Receiver<Outcome> {
        let (reply, rx) = mpsc::channel();
        let mut q = lock(&self.queue);
        q.drafts.retain(|d| {
            let same = d.tile == id;
            if same {
                // Allowed (you cancelled): the requester may have gone already
                let _ = d.reply.send(Outcome::Superseded);
            }
            !same
        });
        for (t, cancel) in &q.drafting {
            if t == id {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        q.drafts.push_back(Draft {
            tile: id.to_owned(),
            path: path.to_owned(),
            file: file.to_owned(),
            text: text.to_owned(),
            marks,
            cancel: Arc::new(AtomicBool::new(false)),
            reply,
        });
        self.queue_cv.notify_all();
        rx
    }

    /// The next job for a lane: a draft; else, when no full build is
    /// running, one asked for, else (when `eager`) the first named file
    /// not built yet, else a tile whose files changed.  Folder and library
    /// tiles are built only when asked for.  With nothing to do it waits
    /// for [`POLL`] (or a new job) and says `None`.
    pub(super) fn next_job(&self, eager: bool) -> Option<Job> {
        let mut q = lock(&self.queue);
        if let Some(d) = q.drafts.pop_front() {
            q.drafting.push((d.tile.clone(), Arc::clone(&d.cancel)));
            return Some(Job::Draft(d));
        }
        if q.full.is_none()
            && let Some(id) = self.pick_full(&mut q.wanted, eager)
        {
            let cancel = Arc::new(AtomicBool::new(false));
            q.full = Some((id.clone(), Arc::clone(&cancel)));
            return Some(Job::Full(id, cancel));
        }
        drop(
            self.queue_cv
                .wait_timeout(q, POLL)
                .unwrap_or_else(PoisonError::into_inner),
        );
        None
    }

    fn pick_full(&self, wanted: &mut VecDeque<TileId>, eager: bool) -> Option<TileId> {
        let tiles = lock(&self.tiles);
        while let Some(id) = wanted.pop_front() {
            if tiles.iter().any(|t| t.id == id) {
                return Some(id);
            }
        }
        let roots = self.roots();
        let named = |t: &Slot| roots.get(t.root).is_none_or(|r| r.kind == RootKind::Files);
        if eager && let Some(t) = tiles.iter().find(|t| t.status == State::Idle && named(t)) {
            return Some(t.id.clone());
        }
        tiles.iter().find(|t| t.changed()).map(|t| t.id.clone())
    }

    /// How much work the server has not finished: drafts and full builds waiting or running, tiles asked for, and tiles
    /// building or whose files changed and have not been picked up.  0 is idle.  A test that wants "nothing more will
    /// happen" waits for this (`GET /api/test-idle`), never for a fixed time.
    #[cfg(any(test, feature = "test-routes"))]
    pub(super) fn pending(&self) -> usize {
        let q = lock(&self.queue);
        let queued = q.drafts.len()
            + q.wanted.len()
            + usize::from(q.full.is_some())
            + q.drafting.len()
            + q.side.len()
            + q.side_running.len();
        drop(q);
        let tiles = lock(&self.tiles);
        queued
            + tiles
                .iter()
                .filter(|t| t.status == State::Building || t.changed())
                .count()
    }

    /// Run `f` on `lane` and wait for its answer.  `key` says what it is a build of and `rev` which revision of it: a
    /// job of the same key and an older revision, waiting or running, is cancelled by this one.  `f` is given the
    /// program and its cancel token (a long build looks at it through [`Cancelled`]).
    ///
    /// # Errors
    /// [`Interrupted`]: superseded (also when it was cancelled while running and stopped), or the job panicked.
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
        {
            let mut q = lock(&self.queue);
            // a waiting job of the same thing at an older revision is dropped: its requester finds its sender gone
            q.side
                .retain(|s| !(s.lane == lane && s.key == key && s.rev < rev));
            for r in &q.side_running {
                if r.lane == lane && r.key == key && r.rev < rev {
                    r.cancel.store(true, Ordering::Relaxed);
                }
            }
            q.side.push_back(Side {
                lane,
                key,
                rev,
                tile: tile.to_owned(),
                cancel: Arc::new(AtomicBool::new(false)),
                run,
            });
        }
        self.queue_cv.notify_all();
    }

    /// Nobody wants `key` on `lane` any more: a waiting job is dropped, a running one is told to stop.
    pub(super) fn cancel_lane(&self, lane: Lane, key: &str) {
        let mut q = lock(&self.queue);
        q.side.retain(|s| !(s.lane == lane && s.key == key));
        for r in &q.side_running {
            if r.lane == lane && r.key == key {
                r.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    /// The next job for a side lane, when fewer than the lane's limit are running; with nothing to do it waits a
    /// moment for one and says `None`.
    pub(super) fn next_side(&self, lane: Lane) -> Option<Side> {
        let mut q = lock(&self.queue);
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
            return Some(s);
        }
        drop(
            self.queue_cv
                .wait_timeout(q, POLL)
                .unwrap_or_else(PoisonError::into_inner),
        );
        None
    }

    /// Is a side job waiting (not yet taken by its lane)?  For the tests of the lanes.
    #[cfg(test)]
    pub(super) fn pending_has_side_waiting(&self) -> bool {
        !lock(&self.queue).side.is_empty()
    }

    /// A side lane runs `job`, then is done with it.
    pub(super) fn run_side(&self, job: Side) {
        let cancel = Arc::clone(&job.cancel);
        (job.run)(self, &cancel);
        let mut q = lock(&self.queue);
        q.side_running.retain(|r| !Arc::ptr_eq(&r.cancel, &cancel));
    }

    /// A lane is done with `job`.
    pub(super) fn finish(&self, job: &Job) {
        let mut q = lock(&self.queue);
        match job {
            Job::Full(..) => q.full = None,
            Job::Draft(d) => q.drafting.retain(|(_, c)| !Arc::ptr_eq(c, &d.cancel)),
        }
    }

    /// Tile `id` has left the list: its builds and drafts stop.
    pub(super) fn forget(&self, id: &str) {
        let mut q = lock(&self.queue);
        q.wanted.retain(|w| w != id);
        q.drafts.retain(|d| {
            let same = d.tile == id;
            if same {
                // Allowed (you cancelled): the requester may have gone already
                let _ = d.reply.send(Outcome::Superseded);
            }
            !same
        });
        for (t, cancel) in q.full.iter().chain(q.drafting.iter()) {
            if t == id {
                cancel.store(true, Ordering::Relaxed);
            }
        }
        q.side.retain(|s| s.tile != id);
        for r in &q.side_running {
            if r.tile == id {
                r.cancel.store(true, Ordering::Relaxed);
            }
        }
    }
}
