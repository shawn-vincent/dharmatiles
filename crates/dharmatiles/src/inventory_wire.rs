//! The inventory as data: its state and its rows, one definition for every reader.  The editor's page reads them in every
//! snapshot and `dharmatiles inventory --json list` writes the same type, so a command-line script and the page see the
//! same names (`camelCase`) for the same things.  This file depends on nothing of the program's but the document crate, so the
//! command line's own tests take it in whole (`#[path]`) to read the output by deserializing it.

use dt_doc::inventory::Colour;
use serde::{Deserialize, Serialize};

/// The colour mode a print was made in.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(rename_all = "lowercase")]
pub enum ColourWord {
    Single,
    Multi,
}

impl From<ColourWord> for Colour {
    fn from(c: ColourWord) -> Self {
        match c {
            ColourWord::Single => Self::Single,
            ColourWord::Multi => Self::Multi,
        }
    }
}

impl From<Colour> for ColourWord {
    fn from(c: Colour) -> Self {
        match c {
            Colour::Single => Self::Single,
            Colour::Multi => Self::Multi,
        }
    }
}

/// A piece of the inventory: what is held, and whether its tile changed or is gone.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct InventoryRow {
    pub tile: String,
    pub seed: u64,
    pub system: String,
    pub colour: ColourWord,
    pub have: u32,
    pub printed: u32,
    pub failed: u32,
    pub gone: u32,
    pub hash: Option<String>,
    pub last: Option<String>,
    pub edited: bool,
    pub missing: bool,
    pub lines: Vec<usize>,
}

/// What the program knows of the inventory: where it is, the pieces it adds up to, the colour mode last used, and what is
/// wrong with it when it does not read.  A state carries only what it has: no home folder has no path.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InventoryState {
    /// There is no home folder to keep one in.
    Unavailable,
    /// The home folder has no inventory file yet: nothing is made until the first line is written.
    Absent {
        home: String,
        /// Where the file will be.
        path: String,
        id: String,
    },
    /// The file reads.
    Read {
        home: String,
        path: String,
        id: String,
        pieces: Vec<InventoryRow>,
        last_colour: ColourWord,
    },
    /// The file exists and does not read.
    Broken {
        home: String,
        path: String,
        id: String,
        /// What is wrong with it, one line each.
        problems: Vec<String>,
    },
}

impl InventoryState {
    /// The pieces held, none when there is no readable file.
    #[must_use]
    pub fn pieces(&self) -> &[InventoryRow] {
        match self {
            Self::Read { pieces, .. } => pieces,
            _ => &[],
        }
    }
}
