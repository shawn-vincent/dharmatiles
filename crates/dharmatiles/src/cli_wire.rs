//! What the command line prints with `--json`, each as one type: the program makes it by constructing the type and the tests
//! read it by deserializing the same type (the integration tests take this file in whole with `#[path]`, so it depends on
//! nothing of the program's own, only on the crates beneath it).
//!
//! Names are `camelCase`, like the page's.  A mistake is [`dt_eval::Mistake`], the type the page shows and a message carries,
//! and a plan and its plates are `dt_print::plates::wire`'s, the ones the page's panel reads.

use dt_eval::{Mistake, TileRecord};
use dt_print::plates::wire::{PlanOnly, PlanPlate};
use serde::{Deserialize, Serialize};

/// A command that stopped on mistakes of the document (`check`, `plates`, `elements`, `inventory`).
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Refused {
    pub ok: bool,
    pub errors: Vec<Mistake>,
}

impl Refused {
    #[must_use]
    pub fn of(errors: Vec<Mistake>) -> Self {
        Self { ok: false, errors }
    }
}

/// One tile that did not build, check or preview: it is named, and so are its mistakes.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Failed {
    pub path: String,
    pub ok: bool,
    pub errors: Vec<Mistake>,
}

impl Failed {
    #[must_use]
    pub fn of(path: &str, errors: Vec<Mistake>) -> Self {
        Self {
            path: path.to_owned(),
            ok: false,
            errors,
        }
    }
}

// ---- build ---------------------------------------------------------------------------------------------------------

/// A tile `build --json` built: the build's record, whether it passes the gate, the files written and how long it all took.
///
/// Its names are the record's (`snake_case`, as `TileRecord` is everywhere it is written), so a tile's report is one
/// convention, not two.
#[derive(Serialize, Deserialize, Debug)]
pub struct Printed {
    pub ok: bool,
    #[serde(flatten)]
    pub record: TileRecord,
    pub files: Vec<String>,
    pub total_s: f64,
}

/// What `build --json` says of one tile.
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum BuildTile {
    Printed(Box<Printed>),
    Failed(Failed),
}

/// `build --json`: every tile, and whether all passed.
#[derive(Serialize, Deserialize, Debug)]
pub struct BuildReport {
    pub ok: bool,
    pub tiles: Vec<BuildTile>,
}

// ---- check ---------------------------------------------------------------------------------------------------------

/// A tile `check --json` read and planned.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Checked {
    pub path: String,
    pub ok: bool,
    /// `1x1`: its squares, when the file has a tile.
    pub tile: Option<String>,
    /// Parts that need elements not built yet.
    pub pending: Vec<Mistake>,
    pub notes: Vec<Mistake>,
    /// The plan in words, when `--explain` asked for it.
    pub explain: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum CheckTile {
    Checked(Checked),
    Failed(Failed),
}

/// `check --json`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CheckReport {
    pub ok: bool,
    pub tiles: Vec<CheckTile>,
}

// ---- cache ---------------------------------------------------------------------------------------------------------

/// `cache --json` with the cache switched off.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CacheOff {
    pub enabled: bool,
}

/// `cache --json`: where the cache is and what is in it.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CacheShown {
    pub enabled: bool,
    pub folder: String,
    pub engine: String,
    pub entries: usize,
    pub bytes: u64,
    pub strays: usize,
    pub limit_bytes: u64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum CacheStatus {
    Shown(CacheShown),
    Off(CacheOff),
}

/// One entry of `cache list --json`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CacheEntryRow {
    pub path: String,
    pub target: String,
    pub tier: String,
    pub engine: String,
    pub triangles: u64,
    pub bytes: u64,
    pub file: String,
}

/// `cache clear --json`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct CacheCleared {
    pub removed: usize,
    pub bytes: u64,
}

/// A preview `cache previews --json` has: found already, or built.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct PreviewKept {
    pub path: String,
    pub ok: bool,
    /// `kept already` or `built`.
    pub how: String,
    pub triangles: usize,
    /// What went wrong with the cache on the way.
    pub problems: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum PreviewTile {
    Kept(PreviewKept),
    Failed(Failed),
}

/// `cache previews --json`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewsReport {
    pub ok: bool,
    pub folder: String,
    pub tiles: Vec<PreviewTile>,
    pub total_s: f64,
}

// ---- plates --------------------------------------------------------------------------------------------------------

/// `plates --check --json`: the plan, laid out nowhere yet.
#[derive(Serialize, Deserialize, Debug)]
pub struct PlatesChecked {
    pub ok: bool,
    pub plan: PlanOnly,
}

/// `plates --json`, and the `.plates.json` written beside the files: the plan, the plates, the files and the gate.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlatesReport {
    pub ok: bool,
    pub plan: PlanOnly,
    pub plates: Vec<PlanPlate>,
    pub grouping_cost: Option<usize>,
    /// How the files are written: `bambu`, `stl` or `3mf`.
    pub format: String,
    pub files: Vec<String>,
    /// Each distinct piece's gate failures (empty when it passed), in the plan's order.
    pub gate: Vec<Vec<String>>,
    pub total_s: f64,
}

// ---- inventory -----------------------------------------------------------------------------------------------------

/// `inventory add --json`: the file written and the lines added to it.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct InventoryAdded {
    pub ok: bool,
    pub file: String,
    pub lines: Vec<String>,
}
