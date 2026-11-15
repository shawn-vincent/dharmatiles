//! What the tile calls ask of whoever runs them: the tile a call is about ([`TileRef`]) and the one host trait the tile
//! families are written on ([`TileHost`]).  The program's `Shared` is one host; a web worker's is another.

use dt_eval::Source;

use crate::call::{Refusal, Status};
use crate::tile_calls::CheckHost;

/// The tile a call about one tile is for: its id, its own file and the files its last build read.
#[derive(Clone)]
pub struct TileRef {
    pub id: String,
    pub path: String,
    pub files: std::collections::BTreeMap<String, Option<std::time::SystemTime>>,
}

impl TileRef {
    /// The file a call is for: the tile's own document, or a file its last build read.
    ///
    /// # Errors
    /// [`Refusal`] (forbidden): not a file of this tile.
    pub fn resolve_file(&self, asked: Option<&str>) -> Result<String, Refusal> {
        match asked {
            None => Ok(self.path.clone()),
            Some(f) if f == self.path || self.files.contains_key(f) => Ok(f.to_owned()),
            Some(_) => Err(Refusal::refused(
                Status::Forbidden,
                "not a file of this tile",
            )),
        }
    }
}

/// May the editor write `file`?  Never the library, nor a read-only file.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a best-effort filesystem probe (a missing or unreadable file means none yet, read again on the next call)"
)]
#[must_use]
pub fn writable(storage: &dyn crate::storage::Storage, file: &str) -> bool {
    !file.starts_with(dt_eval::LIB)
        && storage
            .stat(std::path::Path::new(file))
            .is_ok_and(|m| m.is_file() && !m.readonly)
}

/// What the tile calls ask of whoever runs them, beyond a check's ([`CheckHost`]): the documents (with their
/// revisions), the files, and the library the tile's `use`s are read from.  The tile families (edit and save, then
/// draft, step, build, places and plates) are written on this one trait.
pub trait TileHost: CheckHost {
    /// The documents the host has read or written.
    fn docs(&self) -> &crate::docs::Docs;
    /// Where the files are read and written.
    fn storage(&self) -> &dyn crate::storage::Storage;
    /// What a tile's other files (`lib:` paths) are read from.
    fn library(&self) -> &dyn Source;
    /// Ask for tile `id` to be built next: false when there is no such tile.  The build runs on the host's lane and
    /// its record comes on the event stream.
    ///
    /// # Errors
    /// [`Refusal`]: the owner could not be asked.
    fn want(&self, id: &str) -> Result<bool, Refusal>;

    /// Build a draft for the page on the host's draft lane and wait for it: the program queues it ahead of its full
    /// builds (a newer draft of the tile replaces an older), a worker builds it in its own turn.
    ///
    /// # Errors
    /// [`Refusal`]: the draft panicked or its view could not be made (a bug or a failure, not a mistake in the text).
    fn draft(
        &self,
        ask: crate::draft_calls::DraftAsk,
    ) -> Result<crate::draft_calls::Drafted, Refusal>;

    /// The program's version, as the page's About and an error's context say it.
    fn version(&self) -> String;

    /// Every tile's state and the rest of what a page is told, as a stream's first message holds it.
    fn snapshot(&self) -> crate::api::Snapshot;
}
