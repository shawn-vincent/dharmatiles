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
    format_preview: bool,
}

impl Known {
    /// Nothing known.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the format in development too (format 1), as the reader's
    /// explicit option: the command line and the preview set it from
    /// `DT_FORMAT_PREVIEW=1`, tests set it directly.
    #[must_use]
    pub fn with_format_preview(mut self, on: bool) -> Self {
        self.format_preview = on;
        self
    }

    /// True when the format in development is read.
    #[must_use]
    pub fn format_preview(&self) -> bool {
        self.format_preview
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
