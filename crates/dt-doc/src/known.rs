//! What a file can see from outside itself: the library's heights (its
//! `let` constants) and the definitions of the files it `use`s.
//!
//! dt-eval resolves `use` paths, decodes those files first, and registers
//! each with [`Known::add`] under [`crate::model::Use::key`].  The library's
//! heights are visible to every file without a `use` line
//! ([`Known::add_heights`]); nothing of the library is compiled in.

use std::collections::BTreeMap;

use crate::model::{Const, ConstKind, Definition, Document};
use crate::schema::{Control, Kind};
use crate::units::Units;

/// What a call needs to know about a definition in another file.
#[derive(Debug, Clone)]
pub struct DefInfo {
    /// Its parameters, in order.
    pub params: Vec<ParamInfo>,
    /// Solid, surface or both.
    pub kind: Kind,
    /// True when it is one thing (placed with `at=`, scatterable).
    pub thing: bool,
}

/// A parameter of a definition.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// Its name.
    pub name: String,
    /// Whether it has a default; without one a caller must give it.
    pub has_default: bool,
    /// The control it fills, which types a caller's value.
    pub control: Option<&'static Control>,
}

impl From<&Definition> for DefInfo {
    fn from(d: &Definition) -> Self {
        Self {
            params: d
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name.value.clone(),
                    has_default: p.default.is_some(),
                    control: p.control,
                })
                .collect(),
            kind: d.summary,
            thing: d.thing,
        }
    }
}

/// Definitions and constants visible to a file.
#[derive(Debug, Default, Clone)]
pub struct Known {
    files: BTreeMap<String, BTreeMap<String, DefInfo>>,
    consts: Vec<Const>,
    units: Units,
}

impl Known {
    /// Nothing known.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What `mm`, `in` and `layer` are worth on the build target (format 1):
    /// its square and the printer's layer.  Without it, DungeonBlocks with
    /// the default printer.
    #[must_use]
    pub fn with_units(mut self, units: Units) -> Self {
        self.units = units;
        self
    }

    /// The units in force.
    #[must_use]
    pub fn units(&self) -> Units {
        self.units
    }

    /// Decode the library's heights (library/parts/heights.dtile, read from
    /// disk by the caller) and make their `let` constants visible to every file.
    ///
    /// # Errors
    /// The heights file does not decode.
    pub fn add_heights(&mut self, name: &str, text: &str) -> Result<(), String> {
        let doc = crate::decode(name, text, self)
            .map_err(|e| format!("the library heights must decode: {e:?}"))?;
        self.add_consts(&doc);
        Ok(())
    }

    /// Make a file's file-level `let` constants visible to every file.
    pub fn add_consts(&mut self, doc: &Document) {
        for c in &doc.consts {
            if c.kind == ConstKind::Let {
                self.consts.push(c.clone());
            }
        }
    }

    /// The library's constants (`groundHeight` …), in the order written.
    #[must_use]
    pub fn consts(&self) -> &[Const] {
        &self.consts
    }

    /// Register a decoded file under `key` (a [`crate::model::Use::key`]).
    pub fn add(&mut self, key: &str, doc: &Document) {
        let defs = doc
            .definitions
            .iter()
            .map(|d| (d.name.value.clone(), DefInfo::from(d)))
            .collect();
        self.files.insert(key.to_owned(), defs);
    }

    /// The definitions in a registered file.
    #[must_use]
    pub fn file(&self, key: &str) -> Option<&BTreeMap<String, DefInfo>> {
        self.files.get(key)
    }

    /// Every registered file key.
    pub fn file_keys(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// The registered file that defines `name`, if any.
    #[must_use]
    pub fn file_defining(&self, name: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|(_, defs)| defs.contains_key(name))
            .map(|(k, _)| k.as_str())
    }
}
