//! `dharmatiles migrate`: carry documents to the format this program writes,
//! in place.
//!
//! Two steps, in order.  The older language changes (`ground`, `level=`,
//! `bumps=`: [`dt_doc::migrate::heights`]) first; then the move to format 1
//! ([`dt_doc::migrate::format1`]), which writes the header line, the `$`
//! names, the units and the north-west points.
//!
//! The move to format 1 is proved, not assumed: each tile is built as it
//! was and as it is migrated (with the parts it uses, and the library's
//! heights, migrated with it), and the meshes' hashes compared.  A mesh that
//! moved names its file, and nothing of that tile is written.  `--no-guard`
//! skips the builds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use dt_eval::Source;
use dt_eval::upgrade::{self, Moved, Overlay};

use crate::{ERROR, OK, library, report};

/// Reads each file once, as it was at the start of the run: a part one tile
/// writes is still the old part for the next tile's "before" build.
struct Frozen {
    cache: Mutex<BTreeMap<String, String>>,
    /// Files the older step already changed.
    legacy: BTreeMap<String, String>,
}

impl Source for Frozen {
    fn read(&self, path: &str) -> Result<String, String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "a read panicked".to_owned())?;
        if let Some(t) = cache.get(path) {
            return Ok(t.clone());
        }
        let text = match self.legacy.get(path) {
            Some(t) => t.clone(),
            None => dt_eval::Files.read(path)?,
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

fn say_diags(diags: &[dt_eval::Diag]) {
    report(diags);
}

/// `dharmatiles migrate PATHS [--check] [--no-guard]`.
pub(crate) fn run(paths: &[PathBuf], check: bool, guard: bool) -> u8 {
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
        if upgrade::format_of(&key, &text).is_ok_and(|f| f >= 1) {
            continue;
        }
        match dt_doc::migrate::heights(&text) {
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
    // format 1 is read only through the preview switch until 1.0.0 ships it
    if dt_doc::header::max_format(library::format_preview()) < dt_doc::migrate::format1::TARGET {
        tell!(
            "error: migrating to format 1 needs DT_FORMAT_PREVIEW=1 until 1.0.0 ships it (format 1 is in development): run `DT_FORMAT_PREVIEW=1 dharmatiles migrate …`"
        );
        return ERROR;
    }
    let opts = match library::options("db", "default", None) {
        Ok(o) => o,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let version = env!("DT_ENGINE_VERSION");
    let frozen = Frozen {
        cache: Mutex::new(BTreeMap::new()),
        legacy,
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
        match upgrade::format_of(&key, &text) {
            Err(es) => {
                say_diags(&es);
                code = ERROR;
                continue;
            }
            Ok(f) if f >= 1 => {
                tell!("unchanged {key} (format {f}: left alone, whatever its minor or build part)");
                continue;
            }
            Ok(_) => {}
        }
        match one(&frozen, &key, version, guard, &opts) {
            Err(es) => {
                say_diags(&es);
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

/// One document, with the parts it uses: migrated in memory, and for a
/// tile proved by building it before and after.
///
/// # Errors
/// What stops the migration, or a mesh that moved (named).
fn one(
    frozen: &Frozen,
    path: &str,
    version: &str,
    guard: bool,
    opts: &dt_eval::Options,
) -> Result<Vec<Moved>, Vec<dt_eval::Diag>> {
    let mut group = upgrade::dependencies(frozen, path)?;
    group.push(path.to_owned());
    let moved = upgrade::migrate_files(frozen, &group, version)?;
    let text = frozen
        .read(path)
        .map_err(|e| vec![dt_eval::Diag::plain("dt::source", e)])?;
    if guard && upgrade::has_tile(&text) {
        let after = Overlay::new(frozen, &moved);
        if let Some(d) = upgrade::guard(frozen, &after, path, opts)? {
            return Err(vec![
                dt_eval::Diag::plain(
                    "dt::migrate-moved",
                    format!(
                        "{path}: the mesh moved in migrating ({:016x} before, {:016x} after)",
                        d.before, d.after
                    ),
                )
                .help("migrating must not change what a tile builds: nothing was written; report this tile"),
            ]);
        }
    }
    Ok(moved)
}
