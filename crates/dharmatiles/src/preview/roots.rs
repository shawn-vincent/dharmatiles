//! The roots the preview lists tiles from: files, folders, the library.

use super::*;

/// Folders a scan never enters (besides hidden ones): build output.
pub(super) const SKIP_DIRS: [&str; 3] = ["out", "target", "node_modules"];

/// Where the tiles come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootKind {
    /// Files named on the command line: built at once, never rescanned.
    Files,
    /// A folder: every tile document under it.
    Dir(std::path::PathBuf),
    /// The library's tiles under `lib:` + this prefix (`tiles/`, the test
    /// tiles' `tests/`, or `tiles/water/` for one group).
    Library(String),
}

/// One place the list shows, with its name in the switcher.
#[derive(Clone, Debug)]
pub struct Root {
    pub(super) label: String,
    pub(super) kind: RootKind,
}

impl Root {
    #[must_use]
    pub fn new(label: &str, kind: RootKind) -> Self {
        Self {
            label: label.to_owned(),
            kind,
        }
    }

    pub(super) fn json(&self, index: usize) -> Value {
        let (kind, path) = match &self.kind {
            RootKind::Files => ("files", Value::Null),
            RootKind::Dir(d) => ("dir", json!(d.to_string_lossy())),
            RootKind::Library(p) => ("library", json!(format!("lib:{p}"))),
        };
        json!({"index": index, "label": self.label, "kind": kind, "path": path})
    }

    /// The tiles in this root now, sorted; `None` for the named files.
    pub(super) fn scan(&self) -> Option<Vec<String>> {
        match &self.kind {
            RootKind::Files => None,
            RootKind::Dir(d) => {
                let mut v = Vec::new();
                tile_files(d, 0, &mut v);
                v.sort();
                Some(v)
            }
            RootKind::Library(prefix) => {
                let want = format!("lib:{prefix}");
                match crate::all_library_tiles() {
                    Ok(all) => Some(all.into_iter().filter(|p| p.starts_with(&want)).collect()),
                    Err(e) => {
                        // the list as it was stays: no tile is dropped for a failed scan
                        super::tell_once(&format!("error: preview: cannot list the library: {e}"));
                        None
                    }
                }
            }
        }
    }

    /// The group a tile is listed under: its folder below the root (the
    /// library's `ground`, `water`, …), none at the top.
    pub(super) fn group(&self, path: &str) -> Option<String> {
        match &self.kind {
            RootKind::Dir(d) => {
                let rel = std::path::Path::new(path).strip_prefix(d).ok()?;
                let parent = rel.parent()?.to_string_lossy().replace('\\', "/");
                (!parent.is_empty()).then_some(parent)
            }
            RootKind::Files | RootKind::Library(_) => lib_group(path),
        }
    }
}

/// `lib:tiles/water/x.dtile` → `water`; `lib:tests/shapes/x.dtile` → `shapes`.
pub(super) fn lib_group(path: &str) -> Option<String> {
    let rest = path
        .strip_prefix("lib:tiles/")
        .or_else(|| path.strip_prefix("lib:tests/"))?;
    let (g, _) = rest.split_once('/')?;
    Some(g.to_owned())
}

/// Is this `.dtile` a tile (a `tile` node at the top), not a shared part,
/// palette, printer or project file?  A file that can't be read yet counts.
pub(super) fn is_tile_document(f: &std::path::Path) -> bool {
    std::fs::read_to_string(f).map_or(true, |t| {
        t.lines()
            .any(|l| l.starts_with("tile ") || l.starts_with("tile{") || l == "tile")
    })
}

/// Every tile document (`.dtile` with a `tile` node) and `.stl` under
/// `dir`, skipping hidden folders and build output.
pub(super) fn tile_files(dir: &std::path::Path, depth: usize, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // a folder not made yet (the personal home) simply has no tiles
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            super::tell_once(&format!(
                "error: preview: cannot list {}: {e}; its tiles are missing from the list",
                dir.display()
            ));
            return;
        }
    };
    for e in entries {
        let e = match e {
            Ok(e) => e,
            Err(e) => {
                super::tell_once(&format!(
                    "error: preview: cannot list an entry of {}: {e}",
                    dir.display()
                ));
                continue;
            }
        };
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            if depth < 8 && !SKIP_DIRS.contains(&name.as_str()) {
                tile_files(&p, depth + 1, out);
            }
            continue;
        }
        let lower = name.to_ascii_lowercase();
        let tile =
            (lower.ends_with(".dtile") && lower != "dharmatiles.dtile" && is_tile_document(&p))
                || lower.ends_with(".stl");
        if tile {
            out.push(p.to_string_lossy().into_owned());
        }
    }
}

/// The personal home: `$DT_HOME`, else `~/dharmatiles`.  It need not exist
/// yet; the preview lists its tiles once it does.
///
/// # Errors
/// When neither `$DT_HOME` nor the home folder (`$HOME`, `%USERPROFILE%`) is
/// set: there is nowhere to put your tiles, and a relative folder would put
/// them wherever the program happens to run.
pub fn personal_home() -> Result<std::path::PathBuf, String> {
    if let Some(d) = std::env::var_os("DT_HOME").filter(|d| !d.is_empty()) {
        return Ok(d.into());
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .ok_or_else(|| {
            "cannot find your home folder: neither DT_HOME nor HOME is set; set DT_HOME to the folder for your tiles".to_owned()
        })?;
    Ok(std::path::PathBuf::from(home).join("dharmatiles"))
}

/// What the command line asks for: the roots, and the files named.
/// A folder is a root of its own; `lib:tiles/water` (no `.dtile`) is the
/// library's water group; anything else is a file.  No paths: the personal
/// home, the library and the test tiles.
///
/// # Errors
/// When no paths are named and there is no personal home ([`personal_home`]).
pub fn roots_for(paths: &[String]) -> Result<(Vec<Root>, Vec<String>), String> {
    let lib = Root::new("Library", RootKind::Library("tiles/".to_owned()));
    if paths.is_empty() {
        let home = Root::new("My tiles", RootKind::Dir(personal_home()?));
        let tests = Root::new("Tests", RootKind::Library("tests/".to_owned()));
        return Ok((vec![home, lib, tests], Vec::new()));
    }
    let mut roots = Vec::new();
    let mut files = Vec::new();
    for p in paths {
        let lower = p.to_ascii_lowercase();
        if let Some(rest) = p.strip_prefix("lib:")
            && !lower.ends_with(".dtile")
            && !lower.ends_with(".stl")
        {
            let prefix = format!("{}/", rest.trim_end_matches('/'));
            let (name, group) = match prefix.strip_prefix("tests/") {
                Some(g) => ("Tests", g),
                None => ("Library", prefix.trim_start_matches("tiles/")),
            };
            let group = group.trim_end_matches('/');
            let label = if group.is_empty() {
                name.to_owned()
            } else {
                format!("{name}: {group}")
            };
            roots.push(Root::new(&label, RootKind::Library(prefix)));
        } else if !p.starts_with("lib:") && std::path::Path::new(p).is_dir() {
            roots.push(Root::new(p, RootKind::Dir(p.into())));
        } else {
            files.push(p.clone());
        }
    }
    if !files.is_empty() {
        roots.insert(0, Root::new("Files", RootKind::Files));
    }
    Ok((roots, files))
}
