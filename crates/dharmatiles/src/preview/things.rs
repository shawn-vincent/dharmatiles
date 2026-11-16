//! `/api/tiles/ID/build-thing` `{text, which, seeds, file?}`: one thing of the tile alone, drawn at each seed (see
//! [`dt_service::thing_calls::build_thing`], which holds the types and the one function a web worker calls too).

use super::api::{Out, TileRef, TileRefReply};
use super::*;

pub use dt_service::thing_calls::{ThingReq, ThingRes};

/// The thing, of the tile's text as the page holds it.
pub(super) fn build_thing(shared: &Shared, t: &TileRef, v: ThingReq) -> Out<ThingRes> {
    // the text is of the tile's own file or a part it uses (a file of this tile, or refused): the call lays it over the disk
    t.file(v.file.as_deref())?;
    super::api::out(dt_service::thing_calls::build_thing(
        shared,
        &dt_eval::Files,
        &t.path,
        &v,
    ))
}
