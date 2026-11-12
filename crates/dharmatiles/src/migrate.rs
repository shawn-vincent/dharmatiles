//! `dharmatiles migrate`: carry documents to the format this program reads,
//! in place.
//!
//! Two steps, in order.  The older language changes (`ground`, `level=`,
//! `bumps=`: [`dt_migrate::heights`]) first; then the move to format 1
//! ([`dt_migrate::format1`]), which writes the header line, the `$` names,
//! the units and the north-west points.
//!
//! A document is migrated against the library as it was written in format 0
//! ([`dt_migrate::files::Legacy`]), with the parts it uses.  Each migrated
//! tile is then checked as the program reads it now, against the library as
//! it is: a tile that does not read as format 1 is not written, and neither
//! is anything it carried along.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dt_eval::{Diag, Options, Source};
use dt_migrate::files::{self, Legacy, Moved, Problem, Reader};

use crate::{ERROR, OK, library, report};

/// Reads each file once, as it was at the start of the run: a part one tile
/// writes is still the old part for the next tile.
struct Frozen {
    cache: Mutex<BTreeMap<String, String>>,
    /// Files the older step already changed.
    legacy: BTreeMap<String, String>,
    /// Every document of the run as it was at the start (older step
    /// applied), by its canonical path: a `lib:` path or a relative one
    /// that names one of them reads this, whatever the run has written
    /// since.
    start: BTreeMap<PathBuf, String>,
}

impl Reader for Frozen {
    fn read(&self, path: &str) -> Result<String, String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "a read panicked".to_owned())?;
        if let Some(t) = cache.get(path) {
            return Ok(t.clone());
        }
        let disk = dt_eval::library_file(path).unwrap_or_else(|| PathBuf::from(path));
        // a file that is not there has no snapshot: the read below says so
        let at_start = disk
            .canonicalize()
            .ok()
            .and_then(|canonical| self.start.get(&canonical));
        let text = match (self.legacy.get(path), at_start) {
            (Some(t), _) | (None, Some(t)) => t.clone(),
            (None, None) => dt_eval::Files.read(path)?,
        };
        cache.insert(path.to_owned(), text.clone());
        Ok(text)
    }
}

/// Every `.dtile` under the given files and folders, in order.
fn expand(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    fn walk(dir: &Path, out: &mut BTreeSet<PathBuf>) -> Result<(), String> {
        let list = |e: std::io::Error| format!("cannot read {}: {e}", dir.display());
        for e in std::fs::read_dir(dir).map_err(list)? {
            let p = e.map_err(list)?.path();
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
            if p.is_dir() {
                // build output and hidden folders are not documents
                if name.is_some_and(|n| n == "out" || n.starts_with('.')) {
                    continue;
                }
                walk(&p, out)?;
            } else if p.extension().is_some_and(|x| x == "dtile") {
                out.insert(p);
            }
        }
        Ok(())
    }
    let mut out = BTreeSet::new();
    for p in paths {
        if p.is_dir() {
            walk(p, &mut out)?;
        } else if p.is_file() {
            out.insert(p.clone());
        } else {
            return Err(format!("no file or folder `{}`", p.display()));
        }
    }
    Ok(out.into_iter().collect())
}

/// What migrating found, as diagnostics to show.
fn diags(problems: &[Problem]) -> Vec<Diag> {
    problems
        .iter()
        .map(|p| match p {
            Problem::Doc(e) => Diag::from(e),
            Problem::Plain { code, message } => Diag::plain(code, message.clone()),
        })
        .collect()
}

/// `dharmatiles migrate PATHS [--check]`.
pub(crate) fn run(paths: &[PathBuf], check: bool) -> u8 {
    let files = match expand(paths) {
        Ok(f) if f.is_empty() => {
            tell!("error: no .dtile documents under the paths given");
            return ERROR;
        }
        Ok(f) => f,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let mut code = OK;
    let mut would = false;

    // the older language changes, in memory
    let mut legacy: BTreeMap<String, String> = BTreeMap::new();
    for p in &files {
        let key = p.to_string_lossy().into_owned();
        let text = match std::fs::read_to_string(p) {
            Ok(t) => t,
            Err(e) => {
                tell!("error: {key}: {e}");
                code = ERROR;
                continue;
            }
        };
        // a document of format 1 has none of the old forms
        if files::format_of(&key, &text).is_ok_and(|f| f >= 1) {
            continue;
        }
        match dt_migrate::heights::heights(&text) {
            Ok(Some(new)) => {
                if check {
                    tell!("would migrate {key} (the heights language)");
                    would = true;
                } else if let Err(e) = std::fs::write(p, &new) {
                    tell!("error: {key}: {e}");
                    code = ERROR;
                    continue;
                } else {
                    tell!("migrated {key} (the heights language)");
                }
                legacy.insert(key, new);
            }
            Ok(None) => {}
            Err(e) => {
                tell!("error: {key}: {e}");
                code = ERROR;
            }
        }
    }

    // format 1
    let opts = match library::options("db", "default", None) {
        Ok(o) => o,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let version = env!("DT_ENGINE_VERSION");
    // every document as it is now, before this run writes any of them
    let mut start: BTreeMap<PathBuf, String> = BTreeMap::new();
    for p in &files {
        let key = p.to_string_lossy().into_owned();
        let text = match legacy.get(&key) {
            Some(t) => t.clone(),
            None => match std::fs::read_to_string(p) {
                Ok(t) => t,
                Err(e) => {
                    tell!("error: {key}: {e}");
                    return ERROR;
                }
            },
        };
        match p.canonicalize() {
            Ok(c) => {
                start.insert(c, text);
            }
            Err(e) => {
                tell!("error: {key}: {e}");
                return ERROR;
            }
        }
    }
    let frozen = Frozen {
        cache: Mutex::new(BTreeMap::new()),
        legacy,
        start,
    };
    let mut done: BTreeSet<String> = BTreeSet::new();
    for p in &files {
        let key = p.to_string_lossy().into_owned();
        if done.contains(&key) {
            continue;
        }
        let text = match frozen.read(&key) {
            Ok(t) => t,
            Err(e) => {
                tell!("error: {e}");
                code = ERROR;
                continue;
            }
        };
        match files::format_of(&key, &text) {
            Err(es) => {
                report(&diags(&es));
                code = ERROR;
                continue;
            }
            Ok(f) if f >= 1 => {
                tell!("unchanged {key} (format {f}: left alone, whatever its minor or build part)");
                continue;
            }
            Ok(_) => {}
        }
        match one(&frozen, &key, version, &opts) {
            Err(es) => {
                report(&es);
                tell!("error: {key} not migrated: nothing of it was written");
                code = ERROR;
            }
            Ok(moved) => {
                for m in &moved {
                    if m.path.starts_with(dt_eval::LIB) {
                        tell!(
                            "checked {} (the library's own copy migrates with the library; used in memory)",
                            m.path
                        );
                    } else if done.insert(m.path.clone()) {
                        if check {
                            tell!("would migrate {} (format {} to 1)", m.path, m.from);
                            would = true;
                        } else if let Err(e) = std::fs::write(&m.path, &m.text) {
                            tell!("error: {}: {e}", m.path);
                            code = ERROR;
                        } else {
                            tell!("migrated {} (format {} to 1)", m.path, m.from);
                        }
                    }
                }
                done.insert(key);
            }
        }
    }
    if check && would && code == OK {
        // as `cargo fmt --check`: something would change
        return ERROR;
    }
    code
}

/// A [`Source`] reading the migrated texts in place of the old, and the rest
/// as the program reads it.
struct Moved1<'a>(&'a [Moved]);

impl Source for Moved1<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        match self.0.iter().find(|m| m.path == path) {
            Some(m) => Ok(m.text.clone()),
            None => dt_eval::Files.read(path),
        }
    }
}

/// One document, with the parts it uses: migrated in memory, and for a tile
/// checked as the program now reads it (the library's own files are the
/// library's: only what is in the project is read from the migration).
///
/// # Errors
/// What stops the migration, or a migrated tile that does not read as
/// format 1.
fn one(
    frozen: &Frozen,
    path: &str,
    version: &str,
    opts: &Options,
) -> Result<Vec<Moved>, Vec<Diag>> {
    // the document was written against the library as it was in format 0:
    // that is what it is migrated against
    let old = Legacy(frozen);
    let mut group = files::dependencies(&old, path).map_err(|e| diags(&e))?;
    group.push(path.to_owned());
    let moved = files::migrate_files(&old, &group, version).map_err(|e| diags(&e))?;
    let text = old
        .read(path)
        .map_err(|e| vec![Diag::plain("dt::source", e)])?;
    if files::has_tile(&text) {
        // the library's copies are read as they are, so the migrated parts
        // under `lib:` are left out of what the check reads
        let own: Vec<Moved> = moved
            .iter()
            .filter(|m| !m.path.starts_with(dt_eval::LIB))
            .cloned()
            .collect();
        dt_eval::check(&Moved1(&own), path, opts).map_err(|es| {
            let mut out = vec![
                Diag::plain(
                    "dt::migrate-unread",
                    format!("{path}: the migrated tile does not read as format 1"),
                )
                .help("migrating must give a tile this program reads: nothing was written; report this tile"),
            ];
            out.extend(es);
            out
        })?;
    }
    Ok(moved)
}
