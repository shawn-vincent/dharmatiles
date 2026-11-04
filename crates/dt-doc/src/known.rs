//! What a file can see from outside itself: the standard levels (the
//! prelude) and the definitions of the files it `use`s.
//!
//! dt-eval resolves `use` paths, decodes those files first, and registers
//! each with [`Known::add`] under [`crate::model::Use::key`].  The
//! standard levels come from library/parts/levels.dtile and are visible to
//! every file without a `use` line (R6.8); [`Known::standard`] loads them
//! from the copy compiled into this crate.

use std::collections::BTreeMap;

use crate::model::{Definition, Document};
use crate::schema::{Control, Kind};

/// The text of library/parts/levels.dtile, compiled in.
pub const PRELUDE: &str = include_str!("../../../library/parts/levels.dtile");

/// The name the prelude is reported under.
pub const PRELUDE_NAME: &str = "library/parts/levels.dtile";

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

/// Definitions and levels visible to a file.
#[derive(Debug, Default, Clone)]
pub struct Known {
    files: BTreeMap<String, BTreeMap<String, DefInfo>>,
    levels: Vec<(String, f64)>,
}

impl Known {
    /// Nothing known: not even the standard levels.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The standard levels from the compiled-in prelude, and nothing else.
    ///
    /// # Panics
    /// If the compiled-in prelude doesn't decode (a test guards this).
    #[must_use]
    pub fn standard() -> Self {
        let mut k = Self::new();
        let doc = crate::decode(PRELUDE_NAME, PRELUDE, &k)
            .unwrap_or_else(|e| panic!("the prelude must decode: {e:?}"));
        k.add_levels(&doc)
            .unwrap_or_else(|e| panic!("the prelude's levels must resolve: {e}"));
        k
    }

    /// Make a file's file-level levels visible to every file (the
    /// prelude), each resolved to mm against those before it.
    ///
    /// # Errors
    /// The first level that is not one number (a range), naming it.
    pub fn add_levels(&mut self, doc: &Document) -> Result<(), String> {
        for l in &doc.levels {
            let mm = l
                .value
                .value
                .mm(&self.levels)
                .map_err(|e| format!("level `{}`: {e}", l.name.value))?;
            self.levels.retain(|(n, _)| *n != l.name.value);
            self.levels.push((l.name.value.clone(), mm));
        }
        Ok(())
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

    /// The standard levels, in mm at the reference, in the order written.
    #[must_use]
    pub fn levels(&self) -> &[(String, f64)] {
        &self.levels
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
