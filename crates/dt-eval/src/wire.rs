//! What the engine says of a mistake, as it travels: one type for the program's messages and checks and for the
//! browser engine, so a fix is written once.  They live here, the lowest crate both the program and dt-web reach.

use crate::Fix;

/// An edit that mends a mistake, on the wire: write `replacement` over the `len` bytes at `offset` of the file the message
/// points into.  The one form of a fix, in a message, a check's mistake and the browser engine's diagnostic alike (and
/// what the browser hands back to apply it).
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct FixJson {
    pub offset: usize,
    pub len: usize,
    /// The text there now: the page applies the fix only where it still reads this.
    pub was: String,
    pub replacement: String,
    /// The button's words: `Change to dirt`.
    pub label: String,
}

impl From<&FixJson> for Fix {
    fn from(f: &FixJson) -> Self {
        Self {
            offset: f.offset,
            len: f.len,
            was: f.was.clone(),
            replacement: f.replacement.clone(),
            label: f.label.clone(),
            moved: None,
        }
    }
}

impl From<&Fix> for FixJson {
    fn from(f: &Fix) -> Self {
        Self {
            offset: f.offset,
            len: f.len,
            was: f.was.clone(),
            replacement: f.replacement.clone(),
            label: f.label.clone(),
        }
    }
}
