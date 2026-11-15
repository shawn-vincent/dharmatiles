//! The reader an edit is held to: the whole check of a tile (its `use`s, the library's heights and seams, the plan of
//! its stack), so that [`dt_doc::ops::Checked`] compares what the page or `fix` would show of the old text and of the
//! new.  One type for the editor's routes and for `dharmatiles fix`.

use crate::{Options, Source};
use dt_doc::ops::{Flaw, Kind, Plain, Reader};

/// The check of a tile, read from `base` with `path` holding the text asked about.
pub struct Evaluated<'a> {
    /// What the tile's other files are read from.
    pub base: &'a dyn Source,
    /// The tile whose text is asked about; a path to resolve its `use`s from.
    pub path: &'a str,
    /// The build target.
    pub opts: &'a Options,
}

/// `base`, except that `path` reads as `text`.
struct OneFile<'a> {
    base: &'a dyn Source,
    path: &'a str,
    text: &'a str,
}

impl Source for OneFile<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        if path == self.path {
            Ok(self.text.to_owned())
        } else {
            self.base.read(path)
        }
    }
}

impl Reader for Evaluated<'_> {
    fn mistakes(&self, kind: Kind, text: &str) -> Vec<Flaw> {
        if kind != Kind::Tile {
            return Plain.mistakes(kind, text);
        }
        let src = OneFile {
            base: self.base,
            path: self.path,
            text,
        };
        match crate::check(&src, self.path, self.opts) {
            Ok(_) => Vec::new(),
            Err(errs) => errs
                .iter()
                .map(|d| Flaw {
                    code: d.code.to_owned(),
                    message: d.message.clone(),
                })
                .collect(),
        }
    }
}
