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

use dt_eval::Files;
pub(super) use dt_service::queue::{Finished, Lane, Taking};

/// What the queue holds, as the program has it: the host's job is a [`SideJob`], a failed draft's carry a server
/// error.
pub(super) type Outcome = dt_service::queue::Outcome<Program>;
pub(super) type Draft = dt_service::queue::Draft<Program>;
pub(super) type Job = dt_service::queue::Job<Program>;
pub(super) type Side = dt_service::queue::Side<Program>;
pub(super) type Taken = dt_service::queue::Taken<Program>;

#[cfg(test)]
type Probe = Box<dyn FnOnce(&Shared, &Arc<AtomicBool>) + Send>;

/// What a side lane runs: one value for each kind of work the program puts on a side lane, with the typed request it
/// is made from and the channel its requester waits on.  The owner holds them and runs none; the lane that takes one
/// calls [`SideJob::run`].
pub(crate) enum SideJob {
    /// A tile's export (see [`edit::export`]).
    ExportTile {
        tile: api::TileRef,
        req: edit::ExportReq,
        reply: mpsc::Sender<Result<api::Out<edit::ExportRes>, Interrupted>>,
    },
    /// A print list's export (see [`plates::export`]).
    ExportPlates {
        id: TileId,
        path: String,
        format: Option<String>,
        reply: mpsc::Sender<Result<api::Out<plates::PlatesExported>, Interrupted>>,
    },
    /// A piece's low build for a crown's look at it ([`scene::crowns`]).
    Crown {
        path: String,
        opts: Options,
        reply: mpsc::Sender<Result<Result<dt_eval::Built, Vec<Diag>>, Interrupted>>,
    },
    /// A wanted tier of a map's piece ([`tiers`]): says what it made itself.
    Tier { ready: tiers::Ready, rev: u64 },
    /// A job a test makes of a function, to see the lanes order, cancel and replace jobs.
    #[cfg(test)]
    Probe(Probe),
}

/// What the program says of itself to the service: the lanes' work and a failed draft's error.
impl dt_service::host::Host for Program {
    type Job = SideJob;
    type Failure = fail::ServerError;

    fn now_ms(&self) -> u64 {
        fail::now_ms()
    }
}

/// `f`, with a panic or a cancel it raises as the [`Interrupted`] it is.
fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, Interrupted> {
    match std::panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => Ok(r),
        Err(p) if p.is::<Superseded>() => Err(Interrupted::Superseded),
        Err(p) => Err(Interrupted::Panicked(p)),
    }
}

/// An answer to a requester that waits on it.  Dropped silently when the requester is gone: it gave up.
fn answer<T>(to: &mpsc::Sender<T>, answer: T) {
    // Allowed (you cancelled): the requester may have gone already
    let _ = to.send(answer);
}

impl SideJob {
    /// Run the job to its end on the lane's thread.
    pub(super) fn run(self, shared: &Shared, cancel: &Arc<AtomicBool>) {
        match self {
            Self::ExportTile { tile, req, reply } => {
                answer(&reply, guarded(|| edit::export_now(shared, &tile, &req)))
            }
            Self::ExportPlates {
                id,
                path,
                format,
                reply,
            } => answer(
                &reply,
                guarded(|| plates::export_now(shared, &id, &path, format.as_deref())),
            ),
            Self::Crown { path, opts, reply } => answer(
                &reply,
                guarded(|| {
                    dt_store::build_stored(
                        &Files,
                        &path,
                        &opts,
                        &dt_eval::Progress::new(Arc::new(Cancelled(Arc::clone(cancel)))),
                        None,
                        shared.store.as_deref(),
                    )
                }),
            ),
            Self::Tier { ready, rev } => shared.build_wanted_tier(&ready, rev, cancel),
            #[cfg(test)]
            Self::Probe(f) => f(shared, cancel),
        }
    }
}

/// How a job on a lane ended without an answer.
pub(crate) enum Interrupted {
    /// A request for a newer revision of the same thing took its place, or its tile left.
    Superseded,
    /// The job panicked: the requester re-raises it, so it is answered as any panic in a route is.
    Panicked(Box<dyn std::any::Any + Send>),
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
                .filter(|t| t.building || t.changed(mtime))
                .count()
    }

    /// Run the job `make` builds around a reply channel on `lane` and wait for its answer.  `key` says what it is a
    /// build of and `rev` which revision of it: a job of the same key and an older revision, waiting or running, is
    /// cancelled by this one.  The job is given the program and its cancel token when its lane runs it (a long build
    /// looks at the token through [`Cancelled`]).
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
        make: impl FnOnce(mpsc::Sender<Result<T, Interrupted>>) -> SideJob,
    ) -> Result<T, Interrupted> {
        let (tx, rx) = mpsc::channel();
        self.submit_lane(lane, key, rev, tile, make(tx));
        rx.recv().unwrap_or(Err(Interrupted::Superseded))
    }

    /// Queue `job` on `lane` and go on: nothing waits for it (its own code says what it made).  The same rule as
    /// [`Shared::on_lane`] for a job of the same key at an older revision.
    pub(super) fn submit_lane(&self, lane: Lane, key: String, rev: u64, tile: &str, job: SideJob) {
        self.owner.send(Command::SubmitSide(Side {
            lane,
            key,
            rev,
            tile: tile.to_owned(),
            cancel: Arc::new(AtomicBool::new(false)),
            job,
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
        job.job.run(self, &cancel);
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
            .filter(|r| r.changed(mtime))
            .map(|r| r.id.clone())
            .collect();
        self.owner.send(Command::Changed { finished, ids });
    }
}

#[cfg(test)]
impl Shared {
    /// [`Shared::on_lane`] for a function a test gives: run on the lane as any job is.
    pub(super) fn on_probe<T: Send + 'static>(
        &self,
        lane: Lane,
        key: String,
        rev: u64,
        tile: &str,
        f: impl FnOnce(&Shared, &Arc<AtomicBool>) -> Result<T, Interrupted> + Send + 'static,
    ) -> Result<T, Interrupted> {
        self.on_lane(lane, key, rev, tile, move |reply| {
            SideJob::Probe(Box::new(move |shared, cancel| {
                let ran = guarded(|| f(shared, cancel)).and_then(|r| r);
                answer(&reply, ran);
            }))
        })
    }
}
