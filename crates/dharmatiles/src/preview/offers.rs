//! `/api/tiles/ID/offers` `{text, at, file?}`: what the editor's Add menu may offer at place `at` of the stack (see
//! [`dt_service::read_calls::offers`], which holds the types and the one function a web worker calls too).

use std::collections::BTreeMap;

use super::api::{Out, TileRef, TileRefReply};
use super::edit::Overlay;
use super::*;

#[cfg(test)]
pub use dt_service::read_calls::Offer;
pub use dt_service::read_calls::{OffersReq, OffersRes};

/// The offers at place `at`, of the tile's text as the page holds it.
pub(super) fn offers(shared: &Shared, t: &TileRef, v: OffersReq) -> Out<OffersRes> {
    let (tile, file) = (t.path.as_str(), t.file(v.file.as_deref())?);
    let src = Overlay(BTreeMap::from([(file, v.text)]));
    super::api::out(dt_service::read_calls::offers(shared, &src, tile, v.at))
}
