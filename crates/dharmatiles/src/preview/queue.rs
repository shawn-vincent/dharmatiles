//! The build queue: every build the program does for the page, in one
//! place.  Three kinds, in priority order: a *draft* (the page asked for a
//! quick build of the text it holds), a tile *asked for* (chosen, new,
//! renamed, copied), and a tile whose files *changed* (or, eagerly, a file
//! named on the command line not built yet).
//!
//! A full build is heavy, so one runs at a time; a draft runs beside it.
//! Both are taken from here by identical worker lanes (see [`super::worker`]).
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
    pub cancel: Arc<AtomicBool>,
    pub reply: mpsc::Sender<Outcome>,
}

/// What a worker lane takes.
pub(super) enum Job {
    Draft(Draft),
    Full(TileId, Arc<AtomicBool>),
}

#[derive(Default)]
pub(super) struct Queue {
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
        let named = |t: &Slot| {
            self.roots
                .get(t.root)
                .is_none_or(|r| r.kind == RootKind::Files)
        };
        if eager && let Some(t) = tiles.iter().find(|t| t.status == State::Idle && named(t)) {
            return Some(t.id.clone());
        }
        tiles.iter().find(|t| t.changed()).map(|t| t.id.clone())
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
    }
}
