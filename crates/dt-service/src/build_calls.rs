//! The build call: one verb, one meaning, on every transport.  `build` asks for tile `id` to be built next and answers
//! at once that it was asked for (HTTP: 202); the build itself runs on whatever lane the host has (threads in the
//! program, the worker host's turn in a web worker), and its record arrives through the event stream
//! ([`crate::api::StreamEvent::Tile`]), the same as for HTTP.

use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::tile_host::TileHost;

/// `build` says whether the page has the debug layer on for the tile (its marks are collected only then); it need not say.
#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct BuildReq {
    /// The debug layer is on (true) or off (false) for this tile; absent leaves it as it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
}

/// The build was asked for.
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct Accepted {}

/// `build`: queue a build of the tile.  The answer comes on the event stream.
pub const BUILD: Verb<BuildReq, Accepted> = Verb::new("build");

/// Ask for tile `id` to be built next (a no-op once it is built).
///
/// # Errors
/// [`Refusal`] (not found): there is no such tile.
pub fn build<H: TileHost>(host: &H, id: &str, req: BuildReq) -> Result<Accepted, Refusal> {
    // the layer's switch first: a tile built before it was turned on builds again, with the marks
    if let Some(on) = req.debug
        && !host.set_marks(id, on)?
    {
        return Err(Refusal::refused(Status::NotFound, "no such tile"));
    }
    if host.want(id)? {
        Ok(Accepted {})
    } else {
        Err(Refusal::refused(Status::NotFound, "no such tile"))
    }
}
