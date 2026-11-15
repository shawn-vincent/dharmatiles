//! The inventory in the editor: `inventory.dinv` in My tiles, as the page uses it.  The calls and the reading are
//! [`dt_service::inventory_calls`], written once on a host the program's [`Shared`] and a web worker's `Service` both are;
//! what is here is each answered as the HTTP reply.  The page's side is `js/editor/inventory.js` (the view) and the pieces
//! in `plates.js`, `view/places-panel.js` and `levels.js`.  Docs: docs/inventory.html.

use dt_service::inventory::InventoryState;
use dt_service::inventory_calls as calls;

use super::Shared;
use super::api::{Out, out};

pub use dt_service::inventory::InventoryWrote;
pub use dt_service::inventory_calls::{
    InventoryGoneReq, InventoryHaveReq, InventoryRecordReq, InventoryRelinkReq,
    InventoryReprintReq, InventoryReprinted,
};
pub use dt_service::list_calls::is_inventory;

/// The inventory as the page reads it, in every snapshot.
pub(super) fn snapshot(shared: &Shared) -> InventoryState {
    calls::snapshot(shared)
}

/// Open or rebuild an inventory document for the page.
pub(super) fn rebuild(shared: &Shared, id: &str, path: &str) {
    calls::rebuild(shared, id, path);
}

/// `/api/inventory/have`.
pub(super) fn have(shared: &Shared, v: InventoryHaveReq) -> Out<InventoryWrote> {
    out(calls::have(shared, v))
}

/// `/api/inventory/record`.
pub(super) fn record(shared: &Shared, v: InventoryRecordReq) -> Out<InventoryWrote> {
    out(calls::record(shared, v))
}

/// `/api/inventory/gone`.
pub(super) fn gone(shared: &Shared, v: InventoryGoneReq) -> Out<InventoryWrote> {
    out(calls::gone(shared, v))
}

/// `/api/inventory/relink`.
pub(super) fn relink(shared: &Shared, v: InventoryRelinkReq) -> Out<InventoryWrote> {
    out(calls::relink(shared, v))
}

/// `/api/inventory/reprint`.
pub(super) fn reprint(shared: &Shared, v: InventoryReprintReq) -> Out<InventoryReprinted> {
    out(calls::reprint(shared, v))
}

/// A tile or folder moved: the log follows it (see [`calls::note_rename`]).
pub(super) fn note_rename(shared: &Shared, from: &std::path::Path, to: &std::path::Path) {
    calls::note_rename(shared, from, to);
}

/// A folder was renamed: the log follows every tile in it.
pub(super) fn note_folder_rename(shared: &Shared, from: &std::path::Path, to: &std::path::Path) {
    calls::note_folder_rename(shared, from, to);
}
