//! The source trait: every file the evaluator reads goes through it, so the
//! same code reads the disk or an in-memory map (tests).
//!
//! Paths: `lib:parts/grass.dtile` is the library (explicit, never a search
//! path); anything else is a file path. The library is a folder on disk:
//! a checkout's `library/`, `$DT_LIBRARY`, or the copy installed beside the
//! program. A relative `use` resolves
//! against the importing file's folder, inside the library for library
//! files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub use dt_doc::paths::{LIB, resolve};

/// Where documents come from.
pub trait Source: Sync {
    /// The text at `path`.
    ///
    /// # Errors
    /// Why it cannot be read, in words.
    fn read(&self, path: &str) -> Result<String, String>;
}

/// The `library/` folder `lib:` paths are read from (`None`: no library
/// found).
static DISK: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Read `lib:` from `dir` (a checkout's `library/`) for the rest of the
/// run. Only the first choice counts; returns whether this one did.
pub fn use_library_dir(dir: PathBuf) -> bool {
    DISK.set(Some(dir)).is_ok()
}

/// The `library/` folder `lib:` is read from: the one chosen with
/// [`use_library_dir`], else `$DT_LIBRARY`, else the copy installed beside
/// the program (`library/` next to it, or Homebrew's
/// `../share/dharmatiles/library`), else this source tree's `library/`.
/// `None` when no library is found.
#[must_use]
pub fn library_dir() -> Option<&'static Path> {
    DISK.get_or_init(find_library).as_deref()
}

fn find_library() -> Option<PathBuf> {
    let ok = |d: &Path| d.join("printers/default.dtile").is_file();
    if let Some(d) = std::env::var_os("DT_LIBRARY").map(PathBuf::from) {
        return Some(d);
    }
    if let Some(bin) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        for d in [
            bin.join("library"),
            bin.join("../share/dharmatiles/library"),
        ] {
            if ok(&d) {
                return Some(d);
            }
        }
    }
    let tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library");
    ok(&tree).then_some(tree)
}

/// The `docs/` folder (the site's pages, the error pages under `e/` among them): `$DT_DOCS`, else the copy
/// installed beside the program (`docs/` next to it, or Homebrew's `../share/dharmatiles/docs`), else this source
/// tree's `docs/`.  `None` when no docs are found.
#[must_use]
pub fn docs_dir() -> Option<PathBuf> {
    let ok = |d: &Path| d.join("index.html").is_file();
    if let Some(d) = std::env::var_os("DT_DOCS").map(PathBuf::from) {
        return Some(d);
    }
    if let Some(bin) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        for d in [bin.join("docs"), bin.join("../share/dharmatiles/docs")] {
            if ok(&d) {
                return Some(d);
            }
        }
    }
    let tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    ok(&tree).then_some(tree)
}

/// The disk file behind a `lib:` path, when the library is on disk.
#[must_use]
pub fn library_file(path: &str) -> Option<PathBuf> {
    let rel = path.strip_prefix(LIB)?;
    Some(library_dir()?.join(rel))
}

/// The library folder on disk (see [`library_dir`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct Library;

impl Library {
    /// Every library path, as `lib:…`.
    ///
    /// # Errors
    /// There is no library folder on disk, or a folder in it cannot be read:
    /// a sweep must not quietly skip tiles it could not list.
    pub fn paths() -> Result<Vec<String>, String> {
        let dir = library_dir().ok_or_else(|| {
            "no library folder found: set DT_LIBRARY, or run inside the dharmatiles checkout"
                .to_owned()
        })?;
        let mut v = Vec::new();
        kdl_files(dir, dir, &mut v)?;
        v.sort();
        Ok(v)
    }
}

/// Every `.dtile` under `dir`, as `lib:` paths relative to `root`.
fn kdl_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let list = |e: std::io::Error| format!("cannot read {}: {e}", dir.display());
    for e in std::fs::read_dir(dir).map_err(list)? {
        let p = e.map_err(list)?.path();
        if p.is_dir() {
            kdl_files(root, &p, out)?;
        } else if p.extension().is_some_and(|x| x == "dtile")
            && let Ok(rel) = p.strip_prefix(root)
        {
            out.push(format!("{LIB}{}", rel.to_string_lossy().replace('\\', "/")));
        }
    }
    Ok(())
}

impl Source for Library {
    fn read(&self, path: &str) -> Result<String, String> {
        let Some(rel) = path.strip_prefix(LIB) else {
            return Err(format!(
                "`{path}` is not in the library (library paths start with `lib:`)"
            ));
        };
        if let Some(file) = library_file(path) {
            return std::fs::read_to_string(&file)
                .map_err(|_| format!("the library has no `{rel}` ({})", file.display()));
        }
        Err(format!(
            "no library found for `{rel}`: run inside a checkout, install it beside the program, or set DT_LIBRARY"
        ))
    }
}

/// The disk, with `lib:` paths served by the library folder.
#[derive(Clone, Copy, Debug, Default)]
pub struct Files;

impl Source for Files {
    fn read(&self, path: &str) -> Result<String, String> {
        if path.starts_with(LIB) {
            return Library.read(path);
        }
        std::fs::read_to_string(path).map_err(|e| format!("cannot read `{path}`: {e}"))
    }
}

/// An in-memory map of path → text, with `lib:` paths it does not hold
/// served by the library folder.
#[derive(Clone, Debug, Default)]
pub struct Memory(pub BTreeMap<String, String>);

impl Source for Memory {
    fn read(&self, path: &str) -> Result<String, String> {
        if let Some(t) = self.0.get(path) {
            return Ok(t.clone());
        }
        if path.starts_with(LIB) {
            return Library.read(path);
        }
        Err(format!("no file `{path}`"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_squares_dt_doc_reads_are_the_kits() {
        // dt-doc has no geometry, so it holds its own copy of each square
        let kit = |system| dt_kit::Frame::for_system(system, 1, 1).square_mm.to_bits();
        assert_eq!(dt_doc::schema::SQUARE_MM.to_bits(), kit("db"));
        assert_eq!(dt_doc::schema::OPENLOCK_SQUARE_MM.to_bits(), kit("ol"));
    }

    #[test]
    fn the_flat_tile_is_in_the_library() {
        assert!(Library.read("lib:tests/ground/1x1-flat.dtile").is_ok());
        assert!(Library.read("lib:nope.dtile").is_err());
    }
}
