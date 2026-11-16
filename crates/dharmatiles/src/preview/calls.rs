//! The table of the page's calls: one row for each call the editor makes, named by what it does (`save`,
//! `mapsSet`), with the function that answers it and its request and response types.
//! [`super::api`] makes the dispatch and the TypeScript from it, so a call
//! that is not a row here is not a call, and a row has exactly one type for
//! what it takes and one for what it gives.

use super::api::{Reply, TileRef, called, calls, reply, request};
use super::edit::{self, DraftReq};
use super::route;
use super::routes::{resolve, tile_shape};
use super::{Shared, fail, inventory, lists, maps, offers, places, plates, printers, things, tiers};
use dt_service::draft_calls;

calls! {
    post {
        "edit" => edit::edit : edit::EditReq => edit::EditRes;
        "names" => edit::names : edit::NamesReq => edit::NamesRes;
        "tokens" => edit::tokens : edit::TokensReq => edit::TokensRes;
        "checkValue" => edit::check_value : edit::CheckValueReq => edit::CheckValueRes;
        "printedSet" => edit::printed_set : edit::PrintedSetReq => edit::PrintedSetRes;
        "newTile" => edit::new_tile : edit::NewReq => places::Listed;
        "reportMessage" => edit::page_message : edit::PageMessageReq => edit::Done;
        "setTarget" => edit::target : edit::TargetReq => edit::TargetRes;
        "reveal" => edit::reveal : edit::RevealReq => edit::Done;
        "addPlace" => places::add_place : places::Nothing => places::PlacesAddRes;
        "removePlace" => places::remove_place : places::PlacesRemoveReq => places::Changed;
        "newFolder" => places::new_folder : places::FolderNamedReq => places::FolderDone;
        "renameFolder" => places::rename_folder : places::FolderNamedReq => places::FolderDone;
        "trashFolder" => places::trash_folder : places::FolderReq => places::Trashed;
        "undoTrash" => places::undo_trash : places::TrashUndoReq => places::Restored;
        "tiersWant" => tiers::want : tiers::TiersWantReq => tiers::TiersWantRes;
        "printersScan" => printers::scan : printers::PrintersScanReq => printers::PrintersScanRes;
        "printersImport" => printers::import : printers::PrintersImportReq => printers::PrintersImportRes;
        "platesShow" => plates::show : plates::PlatesShowReq => edit::Done;
        "platesSet" => plates::set : plates::PlatesSetReq => lists::ListEdited;
        "platesExport" => plates::export : plates::PlatesExportReq => plates::PlatesExported;
        "inventoryHave" => inventory::have : inventory::InventoryHaveReq => inventory::InventoryWrote;
        "inventoryRecord" => inventory::record : inventory::InventoryRecordReq => inventory::InventoryWrote;
        "inventoryGone" => inventory::gone : inventory::InventoryGoneReq => inventory::InventoryWrote;
        "inventoryRelink" => inventory::relink : inventory::InventoryRelinkReq => inventory::InventoryWrote;
        "inventoryReprint" => inventory::reprint : inventory::InventoryReprintReq => inventory::InventoryReprinted;
        "listsNew" => lists::new_list : lists::ListsNewReq => places::Listed;
        "listsAdd" => lists::add : lists::ListsAddReq => lists::ListsAddRes;
        "listsCount" => lists::set_count : lists::ListsCountReq => lists::ListEdited;
        "listsRemove" => lists::remove : lists::ListsRemoveReq => lists::ListEdited;
        "listsRestore" => lists::restore : lists::ListsRestoreReq => lists::ListEdited;
        "mapsAdd" => maps::add : maps::MapsAddReq => maps::MapsPieced;
        "mapsSet" => maps::set : maps::MapsSetReq => lists::ListEdited;
        "mapsBatch" => maps::batch : maps::MapsBatchReq => maps::MapsBatched;
        "mapsDup" => maps::dup : maps::MapsDupReq => maps::MapsPieced;
        "mapsRemove" => maps::remove : maps::MapsPieceReq => lists::ListEdited;
        "mapsPin" => maps::pin : maps::MapsPinReq => maps::MapsPieced;
        "mapsUnpin" => maps::unpin : maps::MapsPieceReq => lists::ListEdited;
        "mapsUnpinAll" => maps::unpin_all : maps::MapsListReq => lists::ListEdited;
    }
    tile {
        "check" => edit::check : edit::TextReq => edit::CheckRes;
        "printed" => edit::printed : edit::PrintedReq => edit::PrintedRes;
        "save" => edit::save : edit::SaveReq => edit::SaveRes;
        "mine" => edit::mine : places::Nothing => places::Listed;
        "deleteTile" => edit::delete : places::Nothing => edit::Deleted;
        "rename" => edit::rename : edit::RenameReq => places::Listed;
        "moveTile" => places::move_tile : places::TileMoveReq => places::ListedIn;
        "duplicateTile" => places::duplicate_tile : places::Nothing => places::ListedIn;
        "trashTile" => places::trash_tile : places::Nothing => places::Trashed;
        "exportTile" => edit::export : edit::ExportReq => edit::ExportRes;
        "offers" => offers::offers : offers::OffersReq => offers::OffersRes;
        "buildThing" => things::build_thing : things::ThingReq => things::ThingRes;
    }
    get {
        "session" => route::SessionRes;
        "about" => route::AboutRes;
        "elements" => Vec<dt_doc::wire::ElementJson>;
        "language" => dt_doc::wire::LanguageJson;
        "snapshot" => super::api::Snapshot;
        "text" => route::TextRes;
    }
}

/// The reply to a `POST` of the editor, by its path: the path is read as the call it is (the one table of
/// [`super::routes`]) and the call is answered by name; or a refusal that says there is no such call (404) or no
/// such tile (404).
pub(super) fn dispatch(shared: &Shared, path: &str, body: &[u8]) -> route::Reply {
    let resolved = resolve("POST", path);
    if let Some((r, None)) = resolved
        && let Some(reply) = post_verb(shared, r.verb, path, body)
    {
        return reply.into();
    }
    // a call about a tile: the id is in the route, and a tile that is not listed is refused before the call is known
    let (verb, id) = match resolved {
        Some((r, Some(id))) => (r.verb, id),
        _ => match tile_shape(path) {
            Some((id, _)) => ("", id),
            None => return fail::refused("404 Not Found", "no such call").into(),
        },
    };
    let Some(tile) = shared.facts(id).map(|t| TileRef {
        id: id.to_owned(),
        path: t.path,
        files: t.files,
    }) else {
        return fail::refused("404 Not Found", "no such tile").into();
    };
    // the draft answers a mesh, so it is not a row of the table
    if verb == "draft" {
        return match request::<DraftReq>(path, body) {
            Ok(r) => match draft_calls::draft(shared, &tile, r) {
                Ok(dt_service::draft_calls::Mesh(mesh)) => route::Reply::Bytes(mesh),
                Err(refusal) => super::api::refusal_reply(refusal).into(),
            },
            Err(e) => e.into(),
        };
    }
    // a step answers a head and a mesh, so it is not a row of the table either
    if verb == "step" {
        return match request::<edit::StepReq>(path, body) {
            Ok(r) => match draft_calls::step(shared, &tile, r).and_then(|s| s.bytes()) {
                Ok(bytes) => route::Reply::Bytes(std::sync::Arc::new(bytes)),
                Err(refusal) => super::api::refusal_reply(refusal).into(),
            },
            Err(e) => e.into(),
        };
    }
    tile_verb(shared, &tile, verb, path, body)
        .unwrap_or_else(|| fail::refused("404 Not Found", "no such call"))
        .into()
}

#[cfg(test)]
mod tests {

    /// Where the generated types are checked in.
    fn file() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/preview/js/api.d.ts")
    }

    /// `web/preview/js/api.d.ts` is what the Rust types say.  The page's
    /// request layer is checked against that file by `tsc` (`npm test`), so a
    /// field renamed here fails there too.  `DT_CONTRACT=write` rewrites it.
    /// The page's copy of the contract stamp, beside `api.d.ts`.
    fn stamp_file() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/preview/js/contract.js")
    }

    /// The stamp of a contract: FNV-1a, 64 bits, of its TypeScript, as 16 hex digits.
    fn stamp_of(ts: &str) -> String {
        let h = ts.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
        format!("{h:016x}")
    }

    /// What the stamp is of: the types, and the HTTP transport's table of routes (which the page reads from the same file).
    fn contract_text() -> String {
        super::typescript() + &super::super::routes::javascript()
    }

    fn stamp_js(stamp: &str) -> String {
        format!(
            "// Generated by the Rust test the_page_contract_is_current from js/api.d.ts and the HTTP routes: do not edit.  The program reads this\n// file when it is compiled and the page when it loads; a page whose stamp is not the program's is from another build.\nexport const CONTRACT = '{stamp}';\n{}",
            super::super::routes::javascript()
        )
    }

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: a best-effort filesystem probe (a missing or unreadable file means none yet, read again on the next call)"
    )]
    fn the_page_contract_is_current() {
        let want = super::typescript();
        let want_js = stamp_js(&stamp_of(&contract_text()));
        if std::env::var_os("DT_CONTRACT").is_some_and(|v| v == "write") {
            std::fs::write(file(), &want).unwrap();
            std::fs::write(stamp_file(), &want_js).unwrap();
            return;
        }
        let have = std::fs::read_to_string(file()).unwrap_or_default();
        let have_js = std::fs::read_to_string(stamp_file()).unwrap_or_default();
        assert!(
            have == want && have_js == want_js,
            "web/preview/js/api.d.ts (or its stamp, js/contract.js) is not what the preview's request and response types say; run\n  DT_CONTRACT=write cargo test -p dharmatiles --bins the_page_contract_is_current\nand then `npm test` in web/preview, which checks the page against it"
        );
    }

    /// The program's stamp is the one the page imports (both read js/contract.js), and the first snapshot a page
    /// gets carries it.
    #[test]
    fn the_program_tells_its_contract_stamp() {
        let stamp = super::super::api::contract();
        assert_eq!(stamp.len(), 16, "the stamp is 16 hex digits: {stamp:?}");
        assert!(stamp.bytes().all(|b| b.is_ascii_hexdigit()), "{stamp:?}");
        assert_eq!(
            stamp,
            stamp_of(&contract_text()),
            "the compiled-in stamp is the contract's"
        );
    }

    /// Every call of the table has one route, every route one call of the table (or is one of the calls that
    /// answer bytes or a stream, which are not typed), and no two calls share a name or a method and route.
    #[test]
    fn every_call_has_one_route_and_every_route_a_call() {
        use super::super::routes::ROUTES;
        let untyped = [
            "draft",
            "step",
            "build",
            "mesh",
            "sketch",
            "tierResult",
            "events",
            "messages",
            "messagesReport",
            "testPanic",
            "testIdle",
            "testFailure",
        ];
        for verb in super::VERBS {
            let n = ROUTES.iter().filter(|r| r.verb == *verb).count();
            assert_eq!(n, 1, "the call {verb} has {n} routes");
        }
        for r in ROUTES {
            assert!(
                super::VERBS.contains(&r.verb) || untyped.contains(&r.verb),
                "the route {} {} names a call ({}) that is not in the table",
                r.method.as_str(),
                r.path,
                r.verb
            );
        }
        let mut seen = std::collections::BTreeSet::new();
        for r in ROUTES {
            assert!(
                seen.insert((r.method.as_str(), r.path)),
                "{} {} is twice",
                r.method.as_str(),
                r.path
            );
            assert!(
                r.verb.chars().all(|c| c.is_ascii_alphanumeric())
                    && r.verb.starts_with(|c: char| c.is_ascii_lowercase()),
                "{}: a call is named in camelCase",
                r.verb
            );
        }
    }

    /// A route string is written in the HTTP transport's table, nowhere else: not in the program's code (comments and
    /// the tests, which speak HTTP, aside) and not in the page's, whose HTTP host reads the table from `contract.js`.
    #[test]
    fn the_routes_are_written_once() {
        fn code_of(text: &str) -> String {
            // a file's tests are what follows its `#[cfg(test)] mod tests`
            let text = text.split("#[cfg(test)]\nmod tests").next().unwrap_or(text);
            text.lines()
                .filter(|l| {
                    let t = l.trim_start();
                    !(t.starts_with("//") || t.starts_with('*') || t.starts_with("/*"))
                })
                .map(|l| l.split(" // ").next().unwrap_or(l))
                .collect::<Vec<_>>()
                .join("\n")
        }
        fn files(dir: &std::path::Path, ext: &str, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    if !(p.ends_with("node_modules") || p.ends_with("tests")) {
                        files(&p, ext, out);
                    }
                } else if p.extension().is_some_and(|x| x == ext) {
                    out.push(p);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut rs = Vec::new();
        files(&root.join("src"), "rs", &mut rs);
        let mut js = Vec::new();
        files(&root.join("../../web/preview/js"), "js", &mut js);
        let mut bad = Vec::new();
        for p in rs.iter().chain(&js) {
            let name = p.file_name().unwrap().to_string_lossy();
            // the table; the generated contract; the tests' files
            if name == "routes.rs"
                || name == "contract.js"
                || name == "tests.rs"
                || name == "host.js"
            {
                continue;
            }
            if code_of(&std::fs::read_to_string(p).unwrap()).contains("/api/") {
                bad.push(p.display().to_string());
            }
        }
        // the page's HTTP host reads the table; it writes no route of its own
        let host = std::fs::read_to_string(root.join("../../web/preview/js/host.js")).unwrap();
        if code_of(&host).contains("/api/") {
            bad.push("web/preview/js/host.js".to_owned());
        }
        assert!(
            bad.is_empty(),
            "a route is written outside src/preview/routes.rs: {bad:?}"
        );
    }
}
