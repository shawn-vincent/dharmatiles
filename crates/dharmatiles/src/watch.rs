//! `dharmatiles watch`: build each tile, then rebuild it whenever its
//! document or any file it read (its `use` imports) changes.
//!
//! No file-watching crate: modification times are polled every
//! [`POLL`].  The files to watch are the ones the build itself read,
//! recorded by a wrapper [`Source`]; the embedded library (`lib:`) never
//! changes, so it is not watched.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use dt_eval::{Files, LIB, Options, Source};

use crate::{OK, build_one};

/// How often modification times are checked.
const POLL: Duration = Duration::from_millis(300);

/// A file's modification time, `None` when it cannot be read (missing,
/// or mid-save).
pub(crate) fn mtime(path: &str) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The disk, recording every file read and its modification time just
/// before reading, so an edit during a build is caught next poll.
#[derive(Default)]
pub(crate) struct Recording {
    read: Mutex<BTreeMap<String, Option<SystemTime>>>,
}

impl Recording {
    /// Recording that already watches `path` (the document itself), even
    /// if a build never gets to read it.
    pub(crate) fn watching(path: &str) -> Self {
        let rec = Self::default();
        if !path.starts_with(LIB) {
            rec.read
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(path.to_owned(), mtime(path));
        }
        rec
    }

    /// The files read, with their times before reading.
    pub(crate) fn files(&self) -> BTreeMap<String, Option<SystemTime>> {
        self.read
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Source for Recording {
    fn read(&self, path: &str) -> Result<String, String> {
        if !path.starts_with(LIB) {
            self.read
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(path.to_owned())
                .or_insert_with(|| mtime(path));
        }
        Files.read(path)
    }
}

/// One watched tile: the files its last build read, with their times.
struct Tile<'a> {
    path: &'a str,
    files: BTreeMap<String, Option<SystemTime>>,
}

impl Tile<'_> {
    /// Files that changed since the last build.
    fn changed(&self) -> Vec<&str> {
        self.files
            .iter()
            .filter(|(f, t)| mtime(f) != **t)
            .map(|(f, _)| f.as_str())
            .collect()
    }
}

/// Build the tile at `path`, recording what it read.
fn build<'a>(path: &'a str, opts: &Options, out: &Path, png: bool) -> (u8, Tile<'a>) {
    let rec = Recording::default();
    if !path.starts_with(LIB) {
        // watched even if the build never got to read it
        rec.read
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path.to_owned(), mtime(path));
    }
    let (code, _) = build_one(
        &rec,
        path,
        opts,
        out,
        crate::Extras {
            png,
            threemf: false,
            palette: &dt_kit::Palette::default(),
        },
        false,
    );
    let files = rec
        .read
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    (code, Tile { path, files })
}

/// Build every path, then rebuild on change until Ctrl-C (or, for tests,
/// until `max_builds` builds have run).  Returns the last build's code.
pub fn run(
    paths: &[String],
    opts: &Options,
    out: &Path,
    png: bool,
    max_builds: Option<usize>,
) -> u8 {
    let max = max_builds.unwrap_or(usize::MAX);
    let mut builds = 0;
    let mut code = OK;
    let mut tiles = Vec::new();
    for p in paths {
        let (c, t) = build(p, opts, out, png);
        code = c;
        builds += 1;
        tiles.push(t);
        if builds >= max {
            return code;
        }
    }
    let n: usize = tiles.iter().map(|t| t.files.len()).sum();
    if n == 0 {
        tell!("nothing to watch: `lib:` documents are built into the program");
        return code;
    }
    tell!("watching {n} file(s); Ctrl-C to stop");
    loop {
        std::thread::sleep(POLL);
        for t in &mut tiles {
            let changed = t.changed();
            if changed.is_empty() {
                continue;
            }
            tell!("\nchanged: {}", changed.join(", "));
            let (c, fresh) = build(t.path, opts, out, png);
            code = c;
            *t = fresh;
            builds += 1;
            if builds >= max {
                return code;
            }
        }
    }
}
