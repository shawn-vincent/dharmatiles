//! The source trait: every file the evaluator reads goes through it, so the
//! same code reads the disk, the embedded library or an in-memory map (a
//! browser editor, tests).
//!
//! Paths: `lib:parts/grass.dtile` is the library (explicit, never a search
//! path); anything else is a file path. The library is a folder on disk:
//! a checkout's `library/`, `$DT_LIBRARY`, or the copy installed beside the
//! program. Only builds with the `embedded-library` feature (the browser,
//! phones) carry it compiled in. A relative `use` resolves
//! against the importing file's folder, inside the library for library
//! files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/library.rs"));
}

/// The bundled library's prefix.
pub const LIB: &str = "lib:";

/// Where documents come from.
pub trait Source: Sync {
    /// The text at `path`.
    ///
    /// # Errors
    /// Why it cannot be read, in words.
    fn read(&self, path: &str) -> Result<String, String>;
}

/// The `library/` folder `lib:` paths are read from (`None`: the
/// compiled-in copy, or no library found).
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
/// `None` when the program carries the library compiled in.
#[must_use]
pub fn library_dir() -> Option<&'static Path> {
    DISK.get_or_init(find_library).as_deref()
}

fn find_library() -> Option<PathBuf> {
    if cfg!(feature = "embedded-library") {
        return None;
    }
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

/// The disk file behind a `lib:` path, when the library is on disk.
#[must_use]
pub fn library_file(path: &str) -> Option<PathBuf> {
    let rel = path.strip_prefix(LIB)?;
    Some(library_dir()?.join(rel))
}

/// The library compiled into the program (or, after [`use_library_dir`],
/// the folder on disk).
#[derive(Clone, Copy, Debug, Default)]
pub struct Library;

impl Library {
    /// Every library path, as `lib:…`.
    #[must_use]
    pub fn paths() -> Vec<String> {
        if let Some(dir) = library_dir() {
            let mut v = Vec::new();
            kdl_files(dir, dir, &mut v);
            v.sort();
            return v;
        }
        embedded::FILES
            .iter()
            .map(|(p, _)| format!("{LIB}{p}"))
            .collect()
    }
}

/// Every `.dtile` under `dir`, as `lib:` paths relative to `root`.
fn kdl_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            kdl_files(root, &p, out);
        } else if p.extension().is_some_and(|x| x == "dtile")
            && let Ok(rel) = p.strip_prefix(root)
        {
            out.push(format!("{LIB}{}", rel.to_string_lossy().replace('\\', "/")));
        }
    }
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
        if embedded::FILES.is_empty() {
            return Err(format!(
                "no library found for `{rel}`: run inside a checkout, install it beside the program, or set DT_LIBRARY"
            ));
        }
        embedded::FILES
            .iter()
            .find(|(p, _)| *p == rel)
            .map(|(_, t)| (*t).to_owned())
            .ok_or_else(|| format!("the library has no `{rel}`"))
    }
}

/// The disk, with `lib:` paths served by the embedded library.
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
/// served by the embedded library.
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

/// The path a `use "target"` in `from` names, with `.dtile` added if missing.
#[must_use]
pub fn resolve(from: &str, target: &str) -> String {
    let target = if target.ends_with(".dtile") {
        target.to_owned()
    } else {
        format!("{target}.dtile")
    };
    if target.starts_with(LIB) || target.starts_with('/') {
        return target;
    }
    // Windows paths from the command line use `\`; uses are written `/`
    let from = if cfg!(windows) {
        from.replace('\\', "/")
    } else {
        from.to_owned()
    };
    let dir = from.rfind('/').map_or_else(
        || {
            if from.starts_with(LIB) {
                LIB.to_owned()
            } else {
                String::new()
            }
        },
        |i| from[..=i].to_owned(),
    );
    let mut parts: Vec<&str> = Vec::new();
    let (prefix, rest) = dir
        .strip_prefix(LIB)
        .map_or(("", dir.as_str()), |r| (LIB, r));
    let absolute = rest.starts_with('/');
    for p in rest.split('/').chain(target.split('/')) {
        match p {
            "" | "." => {}
            // a relative path keeps the `..` it cannot cancel (`../a`);
            // above `/` there is nothing, so an absolute one drops it
            ".." if parts.last().is_some_and(|l| *l != "..") => {
                parts.pop();
            }
            ".." if absolute => {}
            p => parts.push(p),
        }
    }
    let lead = if absolute { "/" } else { "" };
    format!("{prefix}{lead}{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_to_the_importer() {
        assert_eq!(
            resolve("lib:tiles/ground/x.dtile", "../../parts/grass"),
            "lib:parts/grass.dtile"
        );
        assert_eq!(
            resolve("lib:tiles/x.dtile", "lib:parts/grass"),
            "lib:parts/grass.dtile"
        );
        assert_eq!(resolve("tiles/a.dtile", "parts.dtile"), "tiles/parts.dtile");
        assert_eq!(resolve("a.dtile", "b"), "b.dtile");
    }

    #[test]
    fn leading_dot_dots_are_kept() {
        assert_eq!(resolve("../u/sub/t.dtile", "./p"), "../u/sub/p.dtile");
        assert_eq!(resolve("a.dtile", "../shared/x"), "../shared/x.dtile");
        assert_eq!(resolve("../a.dtile", "../../x"), "../../../x.dtile");
        assert_eq!(resolve("t/a.dtile", "../../x"), "../x.dtile");
        assert_eq!(resolve("/p/a.dtile", "../../../x"), "/x.dtile");
        assert_eq!(resolve("lib:a.dtile", "../x"), "lib:../x.dtile");
    }

    #[test]
    fn the_flat_tile_is_embedded() {
        assert!(Library.read("lib:tiles/test/1x1-flat.dtile").is_ok());
        assert!(Library.read("lib:nope.dtile").is_err());
    }
}
