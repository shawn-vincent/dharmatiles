//! A document's plan, as the page reads it (`TileSummary.plan`): one type, [`TilePlan`], with a variant for each kind
//! of document that has one (a tile, an STL file, a print list or map, the inventory).  Every writer constructs it and
//! every reader (Rust or the page, through the generated TypeScript) reads its fields; nothing patches or looks up by
//! key.  The wire names are the page's, `camelCase`.

use serde::Serialize;

use super::api::Mistake;
use super::scene::SceneJson;
use super::stack::StackItem;
pub use crate::inventory_wire::InventoryRow;
pub use dt_print::plates::wire::{PlanPlate, PlanSettings, PlannedKind};

/// A tile's squares, across and up: what a tile and an STL file's footprint have, and a print list does not.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlanSquares {
    pub cols: u32,
    pub rows: u32,
}

/// The ground a document stands on, which the viewer rules: its square, its size and the height of its zero.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlanGround {
    pub square_mm: f64,
    /// Width by depth, mm: a tile's, or a print list's printer's bed; none when its printer names no bed (nothing is
    /// made up for it).
    pub size_mm: Option<[f64; 2]>,
    /// The zero height: where a tile's base meets its body, or a bed's top.  None for an STL file, which has no zero of
    /// its own: the mesh's lowest point stands in.
    pub datum_z: Option<f64>,
}

/// What the plan pass (or the layout, or the reading of the inventory) said about a document.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TilePlan {
    /// A tile: its frame and its stack.
    Tile {
        #[serde(flatten)]
        squares: PlanSquares,
        #[serde(flatten)]
        ground: PlanGround,
        stack: Vec<StackItem>,
    },
    /// An STL file: the frame its footprint says, and the one line a file's stack is.
    Stl {
        /// The file's name.
        tile: String,
        #[serde(flatten)]
        squares: PlanSquares,
        #[serde(flatten)]
        ground: PlanGround,
        stack: Vec<StackItem>,
    },
    /// A print list or a map.
    Plates(Box<PlatesDoc>),
    /// The inventory.
    Inventory(InventoryDoc),
}

/// A print list or map's plan: the panel, and the scene once it is read.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlatesDoc {
    #[serde(flatten)]
    pub ground: PlanGround,
    /// The file can be edited.
    pub writable: bool,
    pub plates: PlatesPanel,
    /// The map as read; none while the file does not read as a list.
    pub scene: Option<SceneJson>,
    /// The revision of the file the scene (and the layout) is of.
    pub doc_rev: u64,
}

/// What the plates' panel reads: a list that plans, one that does not (its settings as written), or one that did not
/// read at all.  The page tells them by `planned` and by `printer`.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(untagged)]
pub enum PlatesPanel {
    Planned(Box<PlatesPlanned>),
    Written(PlatesWritten),
    Unread(PlatesUnread),
}

/// A list that does not plan: the settings as written, so the panel still offers the printer and the mode and the
/// mistake can be mended there.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlatesWritten {
    #[cfg_attr(test, ts(type = "false"))]
    pub planned: bool,
    pub writable: bool,
    pub printer: String,
    pub label: Option<String>,
    pub bed: Option<[f64; 2]>,
    pub slots: Option<u32>,
    pub mode: Option<String>,
    pub group: Option<String>,
    pub gap: Option<f64>,
    pub margin: Option<f64>,
    pub printers: Vec<PrinterRow>,
    pub check_inventory: bool,
    pub pieces: Vec<ListPiece>,
}

/// A file that did not read as a list: only the printers to choose from.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlatesUnread {
    #[cfg_attr(test, ts(type = "false"))]
    pub planned: bool,
    pub writable: bool,
    pub printers: Vec<PrinterRow>,
}

/// A list that plans: the plan's settings and kinds, the layout and the checks on it.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlatesPlanned {
    #[cfg_attr(test, ts(type = "true"))]
    pub planned: bool,
    /// The plan: its settings, and what it leaves off.
    #[serde(flatten)]
    pub settings: PlanSettings,
    pub kinds: Vec<PlanKind>,
    pub layout: Vec<PlanPlate>,
    /// The plate on show, from 0.
    pub view: usize,
    /// Every piece's mesh is its full build.
    pub exact: bool,
    pub grouping_cost: Option<usize>,
    pub slot_check: SlotCheck,
    pub writable: bool,
    /// How an export is written: `bambu`, `stl` or `3mf`.
    pub format: String,
    pub printers: Vec<PrinterRow>,
    /// The list's own pieces, in the order written.
    pub pieces: Vec<ListPiece>,
}

/// One distinct piece of a planned list.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlanKind {
    #[serde(flatten)]
    pub planned: PlannedKind,
    /// Its footprint, mm.
    pub size: [f64; 2],
    pub height: f64,
    /// Its filaments are the build's (not the stack's estimate).
    pub exact: bool,
    pub filaments: Vec<String>,
}

/// Do the filaments the plates need fit the printer's slots.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SlotCheck {
    pub need: usize,
    pub have: u32,
    pub ok: bool,
}

/// A piece of a list, in the order written, for the panel to edit.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ListPiece {
    pub tile: String,
    pub count: u32,
    /// The id the piece has, or the one the first edit of the file will write on it.
    pub id: String,
    pub pinned: bool,
    pub seed: Option<u64>,
    pub system: Option<String>,
    pub print: String,
}

/// A printer profile, for the choosers: brand, then model.  One with no bed (`default`, `fine`) is only floors.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(untagged)]
pub enum PrinterRow {
    /// A profile that reads.
    Read(PrinterRead),
    /// A profile that does not read, still listed with what is wrong with it.
    Broken(PrinterBroken),
}

/// A printer profile that reads.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PrinterRead {
    pub name: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub label: String,
    pub bed: Option<[f64; 2]>,
    pub height: Option<f64>,
    pub slots: u32,
    pub verify: Option<String>,
    pub zones: usize,
    pub from_slicer: Option<String>,
    pub mine: bool,
}

/// A printer profile that does not read.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PrinterBroken {
    pub name: String,
    pub label: String,
    pub slots: u32,
    pub mine: bool,
    /// What is wrong with it.
    pub broken: String,
}

/// The inventory's plan: its source and the pieces it adds up to.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct InventoryDoc {
    pub writable: bool,
    pub inventory: InventoryRead,
}

/// What reading the inventory made.
#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct InventoryRead {
    pub text: String,
    pub pieces: Vec<InventoryRow>,
    /// What is wrong with the file; empty when it reads.
    pub problems: Vec<Mistake>,
}
