//! Whether a print is one colour or many: one typed value, parsed once where a word comes in (a document's `colour`
//! setting, the command line's `--colour`, a request) and carried as this enum everywhere after.

/// A print's colour: `single` (one filament, no paint) or `multicolour` (each material in its own filament).
#[derive(
    Clone,
    Copy,
    Debug,
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
pub enum Colour {
    /// One filament, no paint.
    Single,
    /// Each material in its own filament.
    Multicolour,
}

impl Colour {
    /// Every colour, in the order the words list them.
    pub const ALL: [Self; 2] = [Self::Single, Self::Multicolour];

    /// The word that names it (`single`, `multicolour`): the command line's, the wire's and a document's.  The one
    /// place the words are written (serde's lowercase names agree, held by a test).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multicolour => "multicolour",
        }
    }

    /// What it is called in a sentence: `single colour`, `multicolour`.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Single => "single colour",
            Self::Multicolour => "multicolour",
        }
    }

    /// The other colour.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Single => Self::Multicolour,
            Self::Multicolour => Self::Single,
        }
    }

    /// The words, as a list for a message (`single or multicolour`).
    #[must_use]
    pub fn words() -> Vec<&'static str> {
        Self::ALL.iter().map(|c| c.word()).collect()
    }
}

impl std::fmt::Display for Colour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.word())
    }
}

impl std::str::FromStr for Colour {
    type Err = String;

    /// The one place a colour's word is read.
    fn from_str(word: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|c| c.word() == word)
            .ok_or_else(|| {
                let hint = if word == "multi" {
                    ": write `multicolour`"
                } else {
                    ""
                };
                format!(
                    "`{word}` is not a colour: {}{hint}",
                    Self::words().join(" or ")
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_reads_back_to_its_colour() {
        for c in Colour::ALL {
            assert_eq!(c.word().parse::<Colour>(), Ok(c));
        }
        assert!(
            "multi"
                .parse::<Colour>()
                .unwrap_err()
                .contains("multicolour")
        );
    }

    #[test]
    fn the_wire_names_are_the_words() {
        for c in Colour::ALL {
            assert_eq!(
                serde_json::to_string(&c).unwrap(),
                format!("\"{}\"", c.word())
            );
        }
    }
}
