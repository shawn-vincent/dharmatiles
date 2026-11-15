//! The page's wire types: the error body, a tile's summary, the snapshot and the stream's events.  Written once here;
//! the program's HTTP transport (`preview/api.rs`) wraps them in a reply, and `web/preview/js/api.d.ts` is generated
//! from them.

use serde::{Deserialize, Serialize};

/// The error body every route that fails sends (see [`fail`]): the error,
/// and beside it what a refusal also says (the text a conflicting edit lost
/// to, a document's mistakes).
#[derive(Serialize, Deserialize, Default, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub error: ErrorDetail,
    /// A 409: the file as it is now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A 409: the revision the file has now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_rev: Option<u64>,
    /// A 409 for a map: the map as it now reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<crate::scene::SceneJson>,
    /// A 422: the document's mistakes, each at its place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<Mistake>>,
    /// A 409 for a draft: a newer draft took its place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
}

/// The error itself.
#[derive(Serialize, Deserialize, Default, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDetail {
    /// `refused`, `failure` or `bug` (see [`fail`]).
    pub kind: String,
    pub message: String,
    pub help: Option<String>,
    pub causes: Vec<String>,
    pub backtrace: String,
    pub context: ErrorContext,
}

/// What the server was doing.
#[derive(Serialize, Deserialize, Default, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ErrorContext {
    pub route: Option<String>,
    pub tile: Option<String>,
    pub version: String,
    pub at_ms: u64,
}

pub use dt_eval::{Mistake, mistakes};

/// A build's progress while it runs (a `progress` event's body).
#[derive(Serialize, Deserialize, Clone, Default, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// What the work is: `building` (a tile's stages) or `loading` (a map's pieces).  Work in flight for a document
    /// is this one record, whatever the work: the bar, the running row and the status line read nothing else.
    pub what: String,
    pub fraction: f64,
    pub label: String,
    pub key: String,
    /// What the stage is counting: `[what, done, total]`; a total of 0 is just the step.
    pub count: Option<(String, usize, usize)>,
    pub elapsed_s: f64,
    pub eta_s: Option<f64>,
    pub estimate_s: Option<f64>,
}

/// One tile (or print list, or the inventory) as the page reads it.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct TileSummary {
    pub id: String,
    pub path: String,
    pub name: String,
    /// The place it is listed under: an index into `roots`.
    pub root: usize,
    pub group: Option<String>,
    pub size: Option<String>,
    /// `idle`, `building`, `ready` or `failed`.
    pub status: String,
    /// What the plan pass, the plates' layout or the reading of the inventory said.
    pub plan: Option<crate::plan::TilePlan>,
    /// The build's record: audit, stages, timings (snake_case, as `dharmatiles build --json` prints it).
    pub record: Option<dt_eval::TileRecord>,
    pub errors: Vec<Mistake>,
    /// A build that panicked or could not be encoded: an error body's `error`.
    pub failure: Option<ErrorDetail>,
    pub progress: Option<Progress>,
    pub mesh_version: Option<u64>,
    /// The revision of the document the mesh is of: the mesh is fetched by it.
    pub doc_rev: Option<u64>,
    pub mesh_bytes: Option<usize>,
    pub sketches: Vec<SketchRef>,
    /// While the first build runs: the mesh on show is an earlier one (`kind`, `note`).
    pub shown: Option<Shown>,
}

/// The mesh on show while a build runs, and what it is (see [`TileSummary::shown`]).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    /// What the mesh is of: `earlier` (a full build the cache kept), `preview` (a stored low tier, or the plates' drafts).
    pub kind: String,
    /// Words for the status line.
    pub note: String,
    /// Whether the gate has checked the mesh on show: a draft has not been.
    pub checked: bool,
    /// `low` or `full`, for a build on show; a plates preview has no tier.
    pub tier: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
pub struct SketchRef {
    pub id: u64,
    pub rough: bool,
}

/// One place the list shows.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
pub struct RootJson {
    pub index: usize,
    pub label: String,
    /// `files`, `dir` or `library`.
    pub kind: String,
    pub path: Option<String>,
    pub role: String,
    pub writable: bool,
    pub exists: bool,
}

/// A folder of a place, by its path below it.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
pub struct FolderJson {
    pub root: usize,
    pub path: String,
}

/// Every tile's state and what the page lists them with: `GET /api/tiles` and the
/// stream's first event.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// The contract stamp of this program (`contract()`): a page that imported a different one is from another build.
    pub contract: String,
    pub rev: u64,
    pub epoch: String,
    pub system: dt_lang::System,
    pub printer: String,
    pub printers: Vec<crate::plan::PrinterRow>,
    pub overhang_deg: f64,
    pub steepest_deg: f64,
    pub square_mm: f64,
    pub roots: Vec<RootJson>,
    pub folders: Vec<FolderJson>,
    pub home: Option<String>,
    pub inventory: crate::inventory::InventoryState,
    pub tiles: Vec<TileSummary>,
}

/// This program's contract stamp: a hash of the contract (`web/preview/js/api.d.ts`), read at compile time from
/// `web/preview/js/contract.js`, the same file the page imports when it loads.  A page served from files newer
/// or older than the running program (a page read from disk after a merge, or a tab left open across a restart)
/// imported a different stamp, sees it differ in the first snapshot, and shows only that.  The test
/// `the_page_contract_is_current` writes and checks the file with `api.d.ts`.
pub fn contract() -> &'static str {
    const FILE: &str = include_str!("../../../web/preview/js/contract.js");
    // the file is one generated line, `export const CONTRACT = '…';` (checked by the contract test); were it ever
    // malformed, the whole file would stand as the stamp, match no page, and every page would say so
    let value = FILE
        .split("CONTRACT = '")
        .nth(1)
        .and_then(|rest| rest.split('\'').next());
    value.unwrap_or(FILE)
}

/// One event of the stream (`GET /api/events`), tagged by `type`: the page's stream reader is typed by this one
/// type, so a field renamed here fails `tsc` in the page's reader.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StreamEvent {
    /// The whole state: the first event of a stream that cannot resume.
    Snapshot(Box<Snapshot>),
    /// A stream that resumed: what changed since is sent after this.
    Resumed { rev: u64, epoch: String },
    /// One tile's state changed.
    Tile { rev: u64, tile: Box<TileSummary> },
    /// A build's tick (not a revision: it changes nothing a page resumes from).
    Progress {
        rev: u64,
        id: String,
        progress: Progress,
    },
    /// A message was said (or its state changed).
    Message {
        rev: u64,
        message: Box<crate::message::MessageJson>,
    },
    /// A map's tier is built at `doc_rev` (or why it could not be).
    Tier {
        rev: u64,
        key: String,
        doc_rev: u64,
        error: Option<String>,
        /// The error is a mistake in the document's text (not a failure of the program).
        mistake: bool,
        /// The code of the document's mistake (`dt::seam`) when `mistake`.
        code: Option<String>,
    },
    /// A resource changed.
    Resource(crate::resources::ResourceEvent),
}

/// The tiers a draft is built at.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, ts_rs::TS)]
#[serde(rename_all = "kebab-case")]
pub enum DraftTier {
    /// The editor's usual draft ([`dt_eval::Tier::Draft`]).
    #[default]
    Draft,
    /// Coarser, for a drag in progress ([`dt_eval::Tier::Low`]): the same places, less detail.
    Low,
}

impl DraftTier {
    /// The build tier it asks for.
    #[must_use]
    pub const fn build(self) -> dt_eval::Tier {
        match self {
            Self::Draft => dt_eval::Tier::Draft,
            Self::Low => dt_eval::Tier::Low,
        }
    }
}
