//! One served tile: its state ([`Slot`]), where its build stands ([`State`]) and the name the page knows it by.
//! A build or a reading changes it only through a [`crate::patch::Patch`].

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::SystemTime;

use crate::{api, plan, scene};

/// Where a tile's build stands.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Not built yet: built when it is chosen.
    Idle,
    Building,
    Ready,
    Failed,
}

impl State {
    /// As the page reads it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

/// A tile's name for the page: 16 hex digits of a hash of its path.  The
/// same path is the same id for as long as it is listed, and across runs.
pub type TileId = String;

pub fn tile_id(path: &str) -> TileId {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// One served tile.
pub struct Slot {
    pub id: TileId,
    pub path: String,
    /// Its name: the file name, without `.dtile`.
    pub name: String,
    /// The root it is listed under, and its group there.
    pub root: usize,
    pub group: Option<String>,
    /// The size the document writes for the tile (`2x1`), read when the tile
    /// is listed: for the folder rows, before any build.
    pub size: Option<String>,
    pub status: State,
    /// A print list: the plate in view, from 0.
    pub plate: usize,
    /// What a map's last reading said that is not a mistake (a crown over a taller neighbour): kept so a
    /// check of the text that finds nothing wrong does not take it from the stream.
    pub scene_notes: Vec<scene::Note>,
    pub plan: Option<plan::TilePlan>,
    pub record: Option<dt_eval::TileRecord>,
    pub errors: Vec<api::Mistake>,
    /// A build that panicked or could not be encoded, as the page gets it
    /// ([`fail::ServerError`]); the page shows it in the error panel.
    pub failure: Option<api::ErrorDetail>,
    pub mesh: Option<Arc<Vec<u8>>>,
    pub mesh_version: u64,
    /// The revision of the document the mesh is of (the tile's file, or the print list's): a mesh is asked for by it,
    /// and one of another revision is refused, so an older build cannot be shown for a newer text.
    pub doc_rev: Option<u64>,
    /// The build's progress (a `progress` event's body) while building.
    pub progress: Option<api::Progress>,
    /// Files the last build read, with their times.
    pub files: BTreeMap<String, Option<SystemTime>>,
    /// Built for a target that is not the chosen one any more.
    pub stale: bool,
    /// While the first build runs: the mesh on show is not this tile's
    /// build but one the cache kept (see [`api::Shown`]), so the page says
    /// "rebuilding".  Gone when the build ends.
    pub shown: Option<api::Shown>,
    /// The page has the debug layer on for this tile: its builds collect the
    /// layer's marks (grass starts, spines) too.  Set by `POST …/build?debug=1`.
    pub marks: bool,
    /// While it builds: what each stage has made so far, for the page to
    /// show as it goes (see [`worker`]).  Gone when the build ends.
    pub sketches: Vec<Sketch>,
}

/// One stage's sketch while a build runs: a later one of the same stage
/// replaces it (the cliff's rough boxes, then its stones).
pub struct Sketch {
    pub stage: String,
    pub id: u64,
    pub rough: bool,
    pub mesh: Arc<Vec<u8>>,
}

impl Slot {
    pub fn in_root(path: &str, root: usize, group: Option<String>) -> Self {
        let name = path
            .rsplit(['/', ':'])
            .next()
            .unwrap_or(path)
            .trim_end_matches(".dtile")
            .trim_end_matches(".dmap")
            .trim_end_matches(".dinv")
            .to_owned();
        Self {
            id: tile_id(path),
            path: path.to_owned(),
            name,
            root,
            group,
            size: None,
            status: State::Idle,
            plate: 0,
            scene_notes: Vec::new(),
            plan: None,
            record: None,
            errors: Vec::new(),
            failure: None,
            mesh: None,
            mesh_version: 0,
            doc_rev: None,
            progress: None,
            files: BTreeMap::new(),
            stale: false,
            shown: None,
            marks: false,
            sketches: Vec::new(),
        }
    }

    pub fn summary(&self) -> api::TileSummary {
        api::TileSummary {
            id: self.id.clone(),
            path: self.path.clone(),
            name: self.name.clone(),
            root: self.root,
            group: self.group.clone(),
            size: self.size.clone(),
            status: self.status.as_str().to_owned(),
            plan: self.plan.clone(),
            record: self.record.clone(),
            errors: self.errors.clone(),
            failure: self.failure.clone(),
            progress: self.progress.clone(),
            mesh_version: self.mesh.as_ref().map(|_| self.mesh_version),
            doc_rev: self.mesh.as_ref().and(self.doc_rev),
            mesh_bytes: self.mesh.as_ref().map(|m| m.len()),
            sketches: self
                .sketches
                .iter()
                .map(|k| api::SketchRef {
                    id: k.id,
                    rough: k.rough,
                })
                .collect(),
            shown: self.shown.clone(),
        }
    }
}

/// What a caller reads of one tile, besides its summary.
pub struct Facts {
    pub path: String,
    pub name: String,
    pub files: BTreeMap<String, Option<SystemTime>>,
    pub plate: usize,
    pub has_mesh: bool,
    pub marks: bool,
}

/// One tile as the queue sees it: enough to pick what to build, and to see whether its files changed.
#[derive(Clone)]
pub struct Row {
    pub id: TileId,
    pub root: usize,
    pub idle: bool,
    pub building: bool,
    pub stale: bool,
    /// The files its last build read (empty while it is idle).
    pub files: BTreeMap<String, Option<SystemTime>>,
}

impl Row {
    /// Built, and a file it read has changed (or the target did): it is built again.
    pub fn changed(&self, stat: impl Fn(&str) -> Option<SystemTime>) -> bool {
        !self.idle && (self.stale || self.files.iter().any(|(f, t)| stat(f) != *t))
    }
}

impl Slot {
    #[must_use]
    pub fn facts(&self) -> Facts {
        Facts {
            path: self.path.clone(),
            name: self.name.clone(),
            files: self.files.clone(),
            plate: self.plate,
            has_mesh: self.mesh.is_some(),
            marks: self.marks,
        }
    }

    #[must_use]
    pub fn row(&self) -> Row {
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
