//! The source trait: every file the evaluator reads goes through it, so the
//! same code reads the disk, the embedded library or an in-memory map (a
//! browser editor, tests).
//!
//! Paths: `lib:parts/grass.kdl` is the embedded library (explicit, never a
//! search path); anything else is a file path. A relative `use` resolves
//! against the importing file's folder, inside the library for library
//! files.

use std::collections::BTreeMap;

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

/// The library compiled into the program.
#[derive(Clone, Copy, Debug, Default)]
pub struct Library;

impl Library {
    /// Every embedded path, as `lib:…`.
    #[must_use]
    pub fn paths() -> Vec<String> {
        embedded::FILES
            .iter()
            .map(|(p, _)| format!("{LIB}{p}"))
            .collect()
    }
}

impl Source for Library {
    fn read(&self, path: &str) -> Result<String, String> {
        let Some(rel) = path.strip_prefix(LIB) else {
            return Err(format!(
                "`{path}` is not in the library (library paths start with `lib:`)"
            ));
        };
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

/// The path a `use "target"` in `from` names, with `.kdl` added if missing.
#[must_use]
pub fn resolve(from: &str, target: &str) -> String {
    let target = if target.ends_with(".kdl") {
        target.to_owned()
    } else {
        format!("{target}.kdl")
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
            resolve("lib:tiles/ground/x.kdl", "../../parts/grass"),
            "lib:parts/grass.kdl"
        );
        assert_eq!(
            resolve("lib:tiles/x.kdl", "lib:parts/grass"),
            "lib:parts/grass.kdl"
        );
        assert_eq!(resolve("tiles/a.kdl", "parts.kdl"), "tiles/parts.kdl");
        assert_eq!(resolve("a.kdl", "b"), "b.kdl");
    }

    #[test]
    fn leading_dot_dots_are_kept() {
        assert_eq!(resolve("../u/sub/t.kdl", "./p"), "../u/sub/p.kdl");
        assert_eq!(resolve("a.kdl", "../shared/x"), "../shared/x.kdl");
        assert_eq!(resolve("../a.kdl", "../../x"), "../../../x.kdl");
        assert_eq!(resolve("t/a.kdl", "../../x"), "../x.kdl");
        assert_eq!(resolve("/p/a.kdl", "../../../x"), "/x.kdl");
        assert_eq!(resolve("lib:a.kdl", "../x"), "lib:../x.kdl");
    }

    #[test]
    fn the_flat_tile_is_embedded() {
        assert!(Library.read("lib:tiles/test/flat-1x1.kdl").is_ok());
        assert!(Library.read("lib:nope.kdl").is_err());
    }
}
