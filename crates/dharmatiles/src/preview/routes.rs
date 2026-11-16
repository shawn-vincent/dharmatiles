//! The HTTP transport's map from a call's name to its method and route: the one place a route string is written.
//!
//! A call is named by what it does (`check`, `save`, `mapsSet`).  The contract ([`super::calls`], and the
//! TypeScript made from it) knows calls by those names only; this table is how the HTTP transport finds the call a
//! request means, and how the page's HTTP host finds the request a call is (it reads the same table, generated into
//! `web/preview/js/contract.js`).  Another transport (a web worker's messages) maps the same names its own way and
//! needs nothing here.  A test holds that no other code of the program or the page writes a route.

/// The method of a call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Method {
    Get,
    Post,
}

impl Method {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// One call's route.  A `:id` segment is the tile's id.
pub(super) struct Route {
    pub verb: &'static str,
    pub method: Method,
    pub path: &'static str,
}

const fn get(verb: &'static str, path: &'static str) -> Route {
    Route {
        verb,
        method: Method::Get,
        path,
    }
}

const fn post(verb: &'static str, path: &'static str) -> Route {
    Route {
        verb,
        method: Method::Post,
        path,
    }
}

/// Every call, by name.
pub(super) static ROUTES: &[Route] = &[
    // the editor's calls that write or ask, in the order of `calls.rs`
    post("edit", "/api/edit"),
    post("names", "/api/names"),
    post("tokens", "/api/tokens"),
    post("checkValue", "/api/check-value"),
    post("printedSet", "/api/printed-set"),
    post("newTile", "/api/new"),
    post("reportMessage", "/api/messages"),
    post("setTarget", "/api/target"),
    post("reveal", "/api/reveal"),
    post("addPlace", "/api/places/add"),
    post("removePlace", "/api/places/remove"),
    post("newFolder", "/api/folders/new"),
    post("renameFolder", "/api/folders/rename"),
    post("trashFolder", "/api/folders/trash"),
    post("undoTrash", "/api/trash/undo"),
    post("tiersWant", "/api/tiers/want"),
    post("printersScan", "/api/printers/scan"),
    post("printersImport", "/api/printers/import"),
    post("platesShow", "/api/plates/show"),
    post("platesSet", "/api/plates/set"),
    post("platesExport", "/api/plates/export"),
    post("inventoryHave", "/api/inventory/have"),
    post("inventoryRecord", "/api/inventory/record"),
    post("inventoryGone", "/api/inventory/gone"),
    post("inventoryRelink", "/api/inventory/relink"),
    post("inventoryReprint", "/api/inventory/reprint"),
    post("listsNew", "/api/lists/new"),
    post("listsAdd", "/api/lists/add"),
    post("listsCount", "/api/lists/count"),
    post("listsRemove", "/api/lists/remove"),
    post("listsRestore", "/api/lists/restore"),
    post("mapsAdd", "/api/maps/add"),
    post("mapsSet", "/api/maps/set"),
    post("mapsBatch", "/api/maps/batch"),
    post("mapsDup", "/api/maps/dup"),
    post("mapsRemove", "/api/maps/remove"),
    post("mapsPin", "/api/maps/pin"),
    post("mapsUnpin", "/api/maps/unpin"),
    post("mapsUnpinAll", "/api/maps/unpin-all"),
    // a tile's calls
    post("check", "/api/tiles/:id/check"),
    post("printed", "/api/tiles/:id/printed"),
    post("save", "/api/tiles/:id/save"),
    post("mine", "/api/tiles/:id/mine"),
    post("deleteTile", "/api/tiles/:id/delete"),
    post("rename", "/api/tiles/:id/rename"),
    post("moveTile", "/api/tiles/:id/move"),
    post("duplicateTile", "/api/tiles/:id/duplicate"),
    post("trashTile", "/api/tiles/:id/trash"),
    post("exportTile", "/api/tiles/:id/export"),
    post("offers", "/api/tiles/:id/offers"),
    post("buildThing", "/api/tiles/:id/build-thing"),
    // a tile's calls that answer bytes, or only say they were heard
    post("draft", "/api/tiles/:id/draft"),
    post("step", "/api/tiles/:id/step"),
    post("build", "/api/tiles/:id/build"),
    // reads
    get("session", "/api/session"),
    get("about", "/api/about"),
    get("elements", "/api/elements"),
    get("language", "/api/language"),
    get("snapshot", "/api/tiles"),
    get("text", "/api/tiles/:id/text"),
    get("mesh", "/api/tiles/:id/mesh"),
    get("sketch", "/api/tiles/:id/sketch"),
    get("tierResult", "/api/tiers/result"),
    get("events", "/api/events"),
    get("messages", "/api/messages"),
    get("messagesReport", "/api/messages/report"),
    #[cfg(any(test, feature = "test-routes"))]
    get("testPanic", "/api/test-panic"),
    #[cfg(any(test, feature = "test-routes"))]
    get("testIdle", "/api/test-idle"),
    #[cfg(any(test, feature = "test-routes"))]
    get("testFailure", "/api/test-failure"),
];

/// The call a request is, with the tile's id when the route has one.
pub(super) fn resolve<'a>(
    method: &str,
    path: &'a str,
) -> Option<(&'static Route, Option<&'a str>)> {
    ROUTES
        .iter()
        .filter(|r| r.method.as_str() == method)
        .find_map(|r| matches(r.path, path).map(|id| (r, id)))
}

/// `Some(id)` when `path` is `pattern` (the id `None` for a pattern with no `:id`).
fn matches<'a>(pattern: &str, path: &'a str) -> Option<Option<&'a str>> {
    let mut id = None;
    let mut want = pattern.split('/');
    let mut have = path.split('/');
    loop {
        match (want.next(), have.next()) {
            (None, None) => return Some(id),
            (Some(":id"), Some(seg)) if !seg.is_empty() => id = Some(seg),
            (Some(a), Some(b)) if a == b => {}
            _ => return None,
        }
    }
}

/// Whether `path` is one of the program's calls rather than a page file: the calls' own namespace.
pub(super) fn is_api(path: &str) -> bool {
    path.starts_with("/api/")
}

/// `/api/tiles/ID/rest` →`(ID, rest)`: the shape of a call under a tile, whether or not the call exists.
pub(super) fn tile_shape(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/api/tiles/")?;
    let (id, what) = rest.split_once('/')?;
    (!id.is_empty()).then_some((id, what))
}

/// The table as the page's HTTP host reads it: `export const ROUTES = {verb: ['POST', '/api/x'], …}`, in `contract.js`.
#[cfg(test)]
pub(super) fn javascript() -> String {
    let mut out = String::from("export const ROUTES = {\n");
    for r in ROUTES {
        out.push_str(&format!(
            "  {}: ['{}', '{}'],\n",
            r.verb,
            r.method.as_str(),
            r.path
        ));
    }
    out.push_str("};\n");
    out
}
