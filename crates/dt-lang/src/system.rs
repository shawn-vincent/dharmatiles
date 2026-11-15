//! The connector systems a tile can be built for: one typed value, parsed once where a word comes in (the command line,
//! a request, the page's arguments, a document's `system` node) and carried as this enum everywhere after.

/// A connector system: DungeonBlocks (`db`, the default) or OpenLOCK (`ol`).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum System {
    /// DungeonBlocks: the 35 mm square with socket pegs underneath.
    #[default]
    Db,
    /// OpenLOCK: the 25.4 mm square with clip sockets underneath.
    Ol,
}

/// Each system and the word that names it: the one table the words are read from and written by.
const WORDS: [(System, &str); 2] = [(System::Db, "db"), (System::Ol, "ol")];

impl System {
    /// Every system, in the order the words list them.
    pub const ALL: [Self; 2] = [Self::Db, Self::Ol];

    /// The word that names it (`db`, `ol`): the command line's, the wire's and a document's.
    #[must_use]
    pub fn word(self) -> &'static str {
        WORDS
            .iter()
            .find(|(s, _)| *s == self)
            .map_or("", |(_, w)| w)
    }

    /// What the system is called in a sentence: `DungeonBlocks`, `OpenLOCK`.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Db => "DungeonBlocks",
            Self::Ol => "OpenLOCK",
        }
    }

    /// The words, as a list for a message (`db or ol`).
    #[must_use]
    pub fn words() -> Vec<&'static str> {
        WORDS.iter().map(|(_, w)| *w).collect()
    }
}

impl std::fmt::Display for System {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

impl std::str::FromStr for System {
    type Err = String;

    /// The one place a system's word is read.
    fn from_str(word: &str) -> Result<Self, String> {
        WORDS
            .iter()
            .find(|(_, w)| *w == word)
            .map(|(s, _)| *s)
            .ok_or_else(|| format!("`{word}` is not a system: {}", Self::words().join(" or ")))
    }
}
