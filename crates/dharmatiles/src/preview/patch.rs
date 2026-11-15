//! What a build, a reading or a call does to one tile, as data: the owner applies a [`Patch`] to the tile's [`Slot`]
//! and tells the change in the same step.  The owner holds no function of its callers: a patch is a value, so it can
//! cross a thread, a channel or a worker boundary as it is.

use super::*;

type Files = BTreeMap<String, Option<SystemTime>>;

/// One change to one tile.
pub(super) enum Patch {
    /// A build begins: it is building, nothing stale, nothing shown of the last.
    Begin,
    /// The plan pass's plan.
    Plan(plan::TilePlan),
    /// The build's progress, as the stream says it.
    Progress(api::Progress),
    /// No progress.
    ClearProgress,
    /// No progress and nothing shown of an earlier build.
    ClearProgressAndShown,
    /// A "loading" progress ends (a map's piece loaded).
    EndLoading,
    /// A build was superseded: no progress, and the files it read.
    Superseded { files: Files },
    /// A build failed: its mistakes, the failure if it was a bug, and the files it read.
    BuildFailed {
        errors: Vec<api::Mistake>,
        failure: Option<api::ErrorDetail>,
        files: Files,
    },
    /// A build is done.
    BuildReady {
        problem: Vec<api::Mistake>,
        record: dt_eval::TileRecord,
        mesh: Arc<Vec<u8>>,
        doc_rev: u64,
        files: Files,
    },
    /// An earlier build, shown while the new one runs (only when no mesh is held yet).
    ShowEarlier {
        rev: u64,
        bytes: Vec<u8>,
        shown: api::Shown,
    },
    /// An STL could not be read.
    StlFailed {
        errors: Vec<api::Mistake>,
        files: Files,
    },
    /// An STL was read and audited.
    StlReady {
        plan: plan::TilePlan,
        record: dt_eval::TileRecord,
        bytes: Arc<Vec<u8>>,
        rev: u64,
        files: Files,
    },
    /// A draft failed (a bug): its mistakes and the failure.
    DraftFailed {
        errors: Vec<api::Mistake>,
        failure: api::ErrorDetail,
    },
    /// An inventory document could not be read.
    InventoryFailed {
        errors: Vec<api::Mistake>,
        files: Files,
        plan: plan::TilePlan,
    },
    /// An inventory document was read.
    InventoryReady { files: Files, plan: plan::TilePlan },
    /// A print list's scene was read: a plan already made keeps its panel (`doc`'s scene and revision are taken),
    /// else `doc` is the plan.
    PlatesScene { doc: Box<plan::PlatesDoc> },
    /// What a map's reading said that is not a mistake.
    SceneNotes(Vec<scene::Note>),
    /// A print list could not be laid out (or has nothing to print, `calm`).
    PlatesFailed {
        calm: bool,
        errors: Vec<api::Mistake>,
        files: Files,
        doc: Box<plan::PlatesDoc>,
    },
    /// A print list is laid out and shown.
    PlatesReady {
        plan: plan::TilePlan,
        bytes: Arc<Vec<u8>>,
        rev: u64,
        files: Files,
        shown: Option<api::Shown>,
    },
    /// Built for a target that is not the chosen one any more.
    MarkStale,
    /// The plate in view is the first.
    PlateZero,
    /// A stage's sketch, in place of its last.
    Sketch(Sketch),
    /// Only a state (the tests').
    #[cfg(test)]
    Status(State),
}

impl Patch {
    /// Do the change to `t`.
    pub(super) fn apply(self, t: &mut Slot) {
        match self {
            Self::Begin => {
                t.status = State::Building;
                t.stale = false;
                t.errors.clear();
                t.failure = None;
                t.progress = None;
                t.sketches.clear();
            }
            Self::Plan(p) => t.plan = Some(p),
            Self::Progress(p) => t.progress = Some(p),
            Self::ClearProgress => t.progress = None,
            Self::ClearProgressAndShown => {
                t.progress = None;
                t.shown = None;
            }
            Self::EndLoading => {
                if t.progress.as_ref().is_some_and(|p| p.what == "loading") {
                    t.progress = None;
                }
            }
            Self::Superseded { files } => {
                t.progress = None;
                t.files = files;
            }
            Self::BuildFailed {
                errors,
                failure,
                files,
            } => {
                t.status = State::Failed;
                t.progress = None;
                t.sketches.clear();
                t.errors = errors;
                t.failure = failure;
                t.files = files;
                // a build the cache kept was only on show until this one landed
                if t.shown.take().is_some() {
                    t.mesh = None;
                }
            }
            Self::BuildReady {
                problem,
                record,
                mesh,
                doc_rev,
                files,
            } => {
                t.status = State::Ready;
                // what to say beside a good build: the palette
                t.errors = problem;
                t.progress = None;
                t.sketches.clear();
                t.record = Some(record);
                t.mesh = Some(mesh);
                t.mesh_version += 1;
                t.doc_rev = Some(doc_rev);
                t.shown = None;
                t.files = files;
            }
            Self::ShowEarlier { rev, bytes, shown } => {
                if t.mesh.is_none() {
                    // labelled with the revision now, so the page can fetch it; it is never mistaken for the
                    // build, which replaces it
                    t.doc_rev = Some(rev);
                    t.mesh = Some(Arc::new(bytes));
                    t.mesh_version += 1;
                    t.shown = Some(shown);
                }
            }
            Self::StlFailed { errors, files } => {
                t.status = State::Failed;
                t.errors = errors;
                t.files = files;
            }
            Self::StlReady {
                plan,
                record,
                bytes,
                rev,
                files,
            } => {
                t.status = State::Ready;
                t.plan = Some(plan);
                t.record = Some(record);
                t.mesh = Some(bytes);
                t.mesh_version += 1;
                t.doc_rev = Some(rev);
                t.files = files;
            }
            Self::DraftFailed { errors, failure } => {
                t.status = State::Failed;
                t.progress = None;
                t.sketches.clear();
                t.errors = errors;
                t.failure = Some(failure);
            }
            Self::InventoryFailed {
                errors,
                files,
                plan,
            } => {
                t.status = State::Failed;
                t.progress = None;
                t.shown = None;
                t.errors = errors;
                t.files = files;
                t.mesh = None;
                t.plan = Some(plan);
            }
            Self::InventoryReady { files, plan } => {
                t.status = State::Ready;
                t.progress = None;
                t.shown = None;
                t.errors.clear();
                t.failure = None;
                t.files = files;
                t.mesh = None;
                t.plan = Some(plan);
            }
            Self::PlatesScene { doc } => {
                // a plan already made keeps its panel: the scene and its revision are what this reading adds
                if let Some(plan::TilePlan::Plates(held)) = &mut t.plan {
                    held.scene = doc.scene;
                    held.doc_rev = doc.doc_rev;
                } else {
                    t.plan = Some(plan::TilePlan::Plates(doc));
                }
            }
            Self::SceneNotes(kept) => t.scene_notes = kept,
            Self::PlatesFailed {
                calm,
                errors,
                files,
                doc,
            } => {
                t.status = if calm { State::Ready } else { State::Failed };
                t.progress = None;
                t.shown = None;
                t.errors = if calm { Vec::new() } else { errors };
                t.files = files;
                t.mesh = None;
                t.plan = Some(plan::TilePlan::Plates(doc));
            }
            Self::PlatesReady {
                plan,
                bytes,
                rev,
                files,
                shown,
            } => {
                t.status = State::Ready;
                t.plan = Some(plan);
                t.mesh = Some(bytes);
                t.mesh_version += 1;
                t.doc_rev = Some(rev);
                t.files = files;
                t.errors.clear();
                t.shown = shown;
            }
            Self::MarkStale => {
                t.stale = true;
                if t.status != State::Idle {
                    t.status = State::Building;
                }
            }
            Self::PlateZero => t.plate = 0,
            #[cfg(test)]
            Self::Status(s) => t.status = s,
            Self::Sketch(s) => {
                t.sketches.retain(|k| k.stage != s.stage);
                t.sketches.push(s);
            }
        }
    }
}
