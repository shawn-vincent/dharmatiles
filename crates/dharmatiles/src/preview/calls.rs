//! The table of the page's calls: one row for each `POST` the editor makes,
//! with the function that answers it and its request and response types.
//! [`super::api`] makes the dispatch and the TypeScript from it, so a call
//! that is not a row here is not a call, and a row has exactly one type for
//! what it takes and one for what it gives.

use super::api::{Reply, TileRef, calls, reply, request};
use super::edit::{self, DraftReq};
use super::route::{self, tile_route};
use super::{Shared, fail, inventory, lists, lock, maps, offers, places, plates, printers, tiers};

calls! {
    post {
        "/api/edit" => edit::edit : edit::EditReq => edit::EditRes;
        "/api/names" => edit::names : edit::NamesReq => edit::NamesRes;
        "/api/check-value" => edit::check_value : edit::CheckValueReq => edit::CheckValueRes;
        "/api/printed-set" => edit::printed_set : edit::PrintedSetReq => edit::PrintedSetRes;
        "/api/new" => edit::new_tile : edit::NewReq => places::Listed;
        "/api/messages" => edit::page_message : edit::PageMessageReq => edit::Done;
        "/api/target" => edit::target : edit::TargetReq => edit::TargetRes;
        "/api/reveal" => edit::reveal : edit::RevealReq => edit::Done;
        "/api/places/add" => places::add_place : places::Nothing => places::PlacesAddRes;
        "/api/places/remove" => places::remove_place : places::PlacesRemoveReq => places::Changed;
        "/api/folders/new" => places::new_folder : places::FolderNamedReq => places::FolderDone;
        "/api/folders/rename" => places::rename_folder : places::FolderNamedReq => places::FolderDone;
        "/api/folders/trash" => places::trash_folder : places::FolderReq => places::Trashed;
        "/api/trash/undo" => places::undo_trash : places::TrashUndoReq => places::Restored;
        "/api/tiers/want" => tiers::want : tiers::TiersWantReq => tiers::TiersWantRes;
        "/api/printers/scan" => printers::scan : printers::PrintersScanReq => printers::PrintersScanRes;
        "/api/printers/import" => printers::import : printers::PrintersImportReq => printers::PrintersImportRes;
        "/api/plates/show" => plates::show : plates::PlatesShowReq => edit::Done;
        "/api/plates/set" => plates::set : plates::PlatesSetReq => lists::ListEdited;
        "/api/plates/export" => plates::export : plates::PlatesExportReq => plates::PlatesExported;
        "/api/inventory/have" => inventory::have : inventory::InventoryHaveReq => inventory::InventoryWrote;
        "/api/inventory/record" => inventory::record : inventory::InventoryRecordReq => inventory::InventoryWrote;
        "/api/inventory/gone" => inventory::gone : inventory::InventoryGoneReq => inventory::InventoryWrote;
        "/api/inventory/relink" => inventory::relink : inventory::InventoryRelinkReq => inventory::InventoryWrote;
        "/api/inventory/reprint" => inventory::reprint : inventory::InventoryReprintReq => inventory::InventoryReprinted;
        "/api/lists/new" => lists::new_list : lists::ListsNewReq => places::Listed;
        "/api/lists/add" => lists::add : lists::ListsAddReq => lists::ListsAddRes;
        "/api/lists/count" => lists::set_count : lists::ListsCountReq => lists::ListEdited;
        "/api/lists/remove" => lists::remove : lists::ListsRemoveReq => lists::ListEdited;
        "/api/lists/restore" => lists::restore : lists::ListsRestoreReq => lists::ListEdited;
        "/api/maps/add" => maps::add : maps::MapsAddReq => maps::MapsPieced;
        "/api/maps/set" => maps::set : maps::MapsSetReq => lists::ListEdited;
        "/api/maps/batch" => maps::batch : maps::MapsBatchReq => maps::MapsBatched;
        "/api/maps/dup" => maps::dup : maps::MapsDupReq => maps::MapsPieced;
        "/api/maps/remove" => maps::remove : maps::MapsPieceReq => lists::ListEdited;
        "/api/maps/pin" => maps::pin : maps::MapsPinReq => maps::MapsPieced;
        "/api/maps/unpin" => maps::unpin : maps::MapsPieceReq => lists::ListEdited;
        "/api/maps/unpin-all" => maps::unpin_all : maps::MapsListReq => lists::ListEdited;
    }
    tile {
        "check" => edit::check : edit::TextReq => edit::CheckRes;
        "printed" => edit::printed : edit::PrintedReq => edit::PrintedRes;
        "save" => edit::save : edit::SaveReq => edit::SaveRes;
        "mine" => edit::mine : places::Nothing => places::Listed;
        "delete" => edit::delete : places::Nothing => edit::Deleted;
        "rename" => edit::rename : edit::RenameReq => places::Listed;
        "move" => places::move_tile : places::TileMoveReq => places::ListedIn;
        "duplicate" => places::duplicate_tile : places::Nothing => places::ListedIn;
        "trash" => places::trash_tile : places::Nothing => places::Trashed;
        "export" => edit::export : edit::ExportReq => edit::ExportRes;
        "offers" => offers::offers : offers::OffersReq => offers::OffersRes;
    }
    get {
        "/api/session" => route::SessionRes;
        "/api/about" => route::AboutRes;
        "/api/tiles" => super::api::Snapshot;
        "/api/tiles/:id/text" => route::TextRes;
    }
}

/// The reply to a `POST` of the editor, by its path: a call in the table, or a
/// refusal that says there is no such call (404) or no such tile (404).
pub(super) fn dispatch(shared: &Shared, path: &str, body: &[u8]) -> route::Reply {
    if let Some(r) = post(shared, path, body) {
        return r.into();
    }
    let Some((id, what)) = tile_route(path) else {
        return fail::refused("404 Not Found", "no such call").into();
    };
    let Some(tile) = lock(&shared.tiles)
        .iter()
        .find(|t| t.id == id)
        .map(|t| TileRef {
            id: t.id.clone(),
            path: t.path.clone(),
            files: t.files.clone(),
        })
    else {
        return fail::refused("404 Not Found", "no such tile").into();
    };
    // the draft answers a mesh, so it is not a row of the table
    if what == "draft" {
        return match request::<DraftReq>(path, body) {
            Ok(r) => edit::draft(shared, &tile, r),
            Err(e) => e.into(),
        };
    }
    tile_post(shared, &tile, what, path, body)
        .unwrap_or_else(|| fail::refused("404 Not Found", "no such call"))
        .into()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    /// Where the generated types are checked in.
    fn file() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/preview/js/api.d.ts")
    }

    /// `web/preview/js/api.d.ts` is what the Rust types say.  The page's
    /// request layer is checked against that file by `tsc` (`npm test`), so a
    /// field renamed here fails there too.  `DT_CONTRACT=write` rewrites it.
    #[test]
    fn the_page_contract_is_current() {
        let want = super::typescript();
        if std::env::var("DT_CONTRACT").is_ok_and(|v| v == "write") {
            std::fs::write(file(), &want).unwrap();
            return;
        }
        let have = std::fs::read_to_string(file()).unwrap_or_default();
        assert!(
            have == want,
            "web/preview/js/api.d.ts is not what the preview's request and response types say; run\n  DT_CONTRACT=write cargo test -p dharmatiles --bins the_page_contract_is_current\nand then `npm test` in web/preview, which checks the page against it"
        );
    }
}
