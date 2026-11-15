//! The build call: one verb, one meaning, on every transport.  `build` asks for tile `id` to be built next and answers
//! at once that it was asked for (HTTP: 202); the build itself runs on whatever lane the host has (threads in the
//! program, the worker host's turn in a web worker), and its record arrives through the event stream
//! ([`crate::api::StreamEvent::Tile`]), the same as for HTTP.

use serde::{Deserialize, Serialize};

use crate::call::{Refusal, Status, Verb};
use crate::tile_host::TileHost;

/// `build` takes nothing.
#[derive(Deserialize, Serialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct BuildReq {}

/// The build was asked for.
#[derive(Deserialize, Serialize, Debug, Default)]
pub struct Accepted {}

/// `build`: queue a build of the tile.  The answer comes on the event stream.
pub const BUILD: Verb<BuildReq, Accepted> = Verb::new("build");

/// Ask for tile `id` to be built next (a no-op once it is built).
///
/// # Errors
/// [`Refusal`] (not found): there is no such tile.
pub fn build<H: TileHost>(host: &H, id: &str, _: BuildReq) -> Result<Accepted, Refusal> {
    if host.want(id) {
        Ok(Accepted {})
    } else {
        Err(Refusal::refused(Status::NotFound, "no such tile"))
    }
}
