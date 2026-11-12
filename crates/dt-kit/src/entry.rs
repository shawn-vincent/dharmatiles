//! The identity of a stack entry.

use std::sync::Arc;

/// Who made something: one stack entry of a tile, or the connector after
/// the stack.  It is the entry's seed path, assigned once when the document
/// is read: `tile/<element>/<k>` (the k-th entry of that element, from 0) or
/// `tile/<element>/<id>` for an entry with an `id=`, such as `tile/grass/0`;
/// an entry inlined from a definition call carries the call's id and its own
/// path under it.  Every other place that names an entry (a solid's [`Tag`],
/// a debug mark, a formula, the mesh's trace, the page's selection) carries
/// this value rather than an index into a stack or an offset into the text,
/// so what an entry made is found again whatever the stack looks like now.
///
/// [`Tag`]: crate::Tag
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryId(Arc<str>);

impl EntryId {
    /// The entry whose path is `path`.
    #[must_use]
    pub fn new(path: &str) -> Self {
        Self(Arc::from(path))
    }

    /// The path, `tile/<element>/<k or id>`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for EntryId {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for EntryId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for EntryId {
    fn from(path: &str) -> Self {
        Self::new(path)
    }
}

impl std::fmt::Display for EntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
