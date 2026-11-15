//! Starting and filling a print list from the editor: the program's side of `listsNew`, `listsAdd`, `listsCount`,
//! `listsRemove` and `listsRestore`.  The calls are [`dt_service::list_calls`], written once on a host the program's
//! [`Shared`] and a web worker's `Service` both are; what is here is each answered as the HTTP reply.

use super::Shared;
use super::api::{Out, Reply, out};
use super::edit::CheckRes;
use dt_service::list_calls as calls;

pub use dt_service::list_calls::{
    ListEdited, ListsAddReq, ListsAddRes, ListsCountReq, ListsNewReq, ListsRemoveReq,
    ListsRestoreReq, Put,
};

/// One more copy of `want` in a list (see [`dt_service::list_calls::put`]), its refusal as the reply.
pub(super) fn put(
    shared: &Shared,
    want: &dt_doc::map::Want,
    list: Option<&str>,
    rev: Option<u64>,
    mode: Option<dt_doc::map::Mode>,
) -> Result<Put, Reply> {
    calls::put(shared, want, list, rev, mode).map_err(super::api::refusal_reply)
}

/// `/api/tiles/ID/check` for a print list or the inventory (see [`dt_service::list_calls::check_text`]).
pub(super) fn check_text(shared: &Shared, tile: &str, text: &str) -> CheckRes {
    calls::check_text(shared, tile, text)
}

/// `/api/lists/new`.
pub(super) fn new_list(shared: &Shared, v: ListsNewReq) -> Out<super::places::Listed> {
    out(calls::new_list(shared, v))
}

/// `/api/lists/add`.
pub(super) fn add(shared: &Shared, v: ListsAddReq) -> Out<ListsAddRes> {
    out(calls::add(shared, v))
}

/// `/api/lists/count`.
pub(super) fn set_count(shared: &Shared, v: ListsCountReq) -> Out<ListEdited> {
    out(calls::set_count(shared, v))
}

/// `/api/lists/remove`.
pub(super) fn remove(shared: &Shared, v: ListsRemoveReq) -> Out<ListEdited> {
    out(calls::remove(shared, v))
}

/// `/api/lists/restore`.
pub(super) fn restore(shared: &Shared, v: ListsRestoreReq) -> Out<ListEdited> {
    out(calls::restore(shared, v))
}
