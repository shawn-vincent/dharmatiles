//! `/api/tiles/ID/build-thing` `{text, which, seeds, file?}`: one thing of the tile alone, drawn at each seed (see
//! [`dt_service::thing_calls::build_thing`], which holds the types and the one function a web worker calls too).

use std::collections::BTreeMap;

use super::api::{Out, TileRef, TileRefReply};
use super::edit::Overlay;
use super::*;

pub use dt_service::thing_calls::{ThingReq, ThingRes};

/// The thing, of the tile's text as the page holds it.
pub(super) fn build_thing(shared: &Shared, t: &TileRef, v: ThingReq) -> Out<ThingRes> {
    // the thing is read from the file the text is of: the tile's own, or a part it uses
    let file = t.file(v.file.as_deref())?;
    let src = Overlay(BTreeMap::from([(file.clone(), v.text.clone())]));
    super::api::out(dt_service::thing_calls::build_thing(
        shared, &src, &file, &v,
    ))
}
