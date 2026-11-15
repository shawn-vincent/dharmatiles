//! Editing a map from the editor: placing, moving, turning, raising, duplicating and removing pieces, new variants, and
//! pins on a plate.  The calls are [`dt_service::map_calls`], written once on a host the program's [`Shared`] and a web
//! worker's `Service` both are; what is here is each answered as the HTTP reply.

use super::Shared;
use super::api::{Out, out};
use dt_service::map_calls as calls;

pub use dt_service::map_calls::{
    MapsAddReq, MapsBatchReq, MapsBatched, MapsDupReq, MapsListReq, MapsPieceReq, MapsPieced,
    MapsPinReq, MapsSetReq,
};
#[cfg(test)]
pub use dt_service::map_calls::{PieceEdit, PieceWords};

use super::lists::ListEdited;

/// `/api/maps/add`.
pub(super) fn add(shared: &Shared, v: MapsAddReq) -> Out<MapsPieced> {
    out(calls::add(shared, v))
}

/// `/api/maps/set`.
pub(super) fn set(shared: &Shared, v: MapsSetReq) -> Out<ListEdited> {
    out(calls::set(shared, v))
}

/// `/api/maps/batch`.
pub(super) fn batch(shared: &Shared, v: MapsBatchReq) -> Out<MapsBatched> {
    out(calls::batch(shared, v))
}

/// `/api/maps/dup`.
pub(super) fn dup(shared: &Shared, v: MapsDupReq) -> Out<MapsPieced> {
    out(calls::dup(shared, v))
}

/// `/api/maps/remove`: the piece, and the pin it had.
pub(super) fn remove(shared: &Shared, v: MapsPieceReq) -> Out<ListEdited> {
    out(calls::remove(shared, v))
}

/// `/api/maps/pin`: the piece stays where it was dropped on its plate.
pub(super) fn pin(shared: &Shared, v: MapsPinReq) -> Out<MapsPieced> {
    out(calls::pin(shared, v))
}

/// `/api/maps/unpin`.
pub(super) fn unpin(shared: &Shared, v: MapsPieceReq) -> Out<ListEdited> {
    out(calls::unpin(shared, v))
}

/// `/api/maps/unpin-all`.
pub(super) fn unpin_all(shared: &Shared, v: MapsListReq) -> Out<ListEdited> {
    out(calls::unpin_all(shared, v))
}
