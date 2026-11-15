//! What the page is told besides a tile's own state, as resources: the places, the build target, the printers, the
//! inventory and the list of tiles (the wire types; what the program tells when is `preview/resources.rs`).

use serde::{Deserialize, Serialize};

use crate::api::{FolderJson, RootJson, TileSummary};
use crate::inventory::InventoryState;
use crate::plan::PrinterRow;

/// The places: where tiles come from and the folders in them.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
pub struct Places {
    pub roots: Vec<RootJson>,
    pub folders: Vec<FolderJson>,
    pub home: Option<String>,
}

/// The build target the page chose.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub system: dt_lang::System,
    pub printer: String,
    pub overhang_deg: f64,
    pub steepest_deg: f64,
    /// One square's side on the chosen system, mm (35 DungeonBlocks, 25.4 OpenLOCK): the page's only source for it.
    pub square_mm: f64,
}

/// The tiles listed, in order, with the state of the ones that joined since the pages were last told.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
pub struct List {
    pub ids: Vec<String>,
    pub added: Vec<TileSummary>,
}

/// One resource and its value.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(tag = "name", content = "value", rename_all = "camelCase")]
pub enum Resource {
    Places(Places),
    Target(Target),
    Printers(Vec<PrinterRow>),
    Inventory(InventoryState),
    List(List),
}

/// A resource on the event stream.
#[derive(Serialize, Deserialize, Clone, Debug, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceEvent {
    /// The stream's revision (for resuming).
    pub rev: u64,
    /// The resource's own revision: it rises when this resource changes and not otherwise.
    pub res_rev: u64,
    #[serde(flatten)]
    pub resource: Resource,
}

impl Resource {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Places(_) => "places",
            Self::Target(_) => "target",
            Self::Printers(_) => "printers",
            Self::Inventory(_) => "inventory",
            Self::List(_) => "list",
        }
    }
}
