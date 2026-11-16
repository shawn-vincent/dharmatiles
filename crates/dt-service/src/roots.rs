//! The roots the preview lists tiles from: files, folders, the library.

use crate::host::Alarm;
use crate::storage::{EntryKind, Storage};

/// Folders a scan never enters (besides hidden ones): build output.
pub const SKIP_DIRS: [&str; 3] = ["out", "target", "node_modules"];

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

/// What a place is to the user: where it stands in the Places list, and
/// whether its tiles may be changed from the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// My tiles: `$DT_HOME`, else `~/dharmatiles`.
    Home,
    /// Files named on the command line.
    Files,
    /// A folder named on the command line.
    Folder,
    /// A folder the user added with the folder chooser (kept in the settings).
    Added,
    /// The library's tiles.
    Library,
    /// The library's test tiles.
    Tests,
}

impl Role {
    /// As the page reads it.
    fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Files => "files",
            Self::Folder => "folder",
            Self::Added => "added",
            Self::Library => "library",
            Self::Tests => "tests",
        }
    }
}

/// One place the list shows, with its name in the switcher.
#[derive(Clone, Debug)]
pub struct Root {
    pub label: String,
    pub kind: RootKind,
    pub role: Role,
}

impl Root {
    #[must_use]
    pub fn new(label: &str, kind: RootKind) -> Self {
        let role = match &kind {
            RootKind::Files => Role::Files,
            RootKind::Dir(_) => Role::Folder,
            RootKind::Library(p) if p.starts_with("tests/") => Role::Tests,
            RootKind::Library(_) => Role::Library,
        };
        Self {
            label: label.to_owned(),
            kind,
            role,
        }
    }

    /// The library's documents under `lib:` + `prefix` (`tiles/water/`, `tests/ground/`, `parts/`: ends in a slash) as a place of
    /// their own, named as the program names it when the command line asks for that group (`lib:parts`): `Library: parts`.
    /// The program's `roots_for` and a web worker's roots both make it here, so a part opens the same wherever it is listed.
    #[must_use]
    pub fn library_group(prefix: &str) -> Self {
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
        Self::new(&label, RootKind::Library(prefix.to_owned()))
    }

    /// The same place, with another role (My tiles, an added folder).
    #[must_use]
    pub fn with_role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    /// May the page change what is in this place (make, rename, move, trash)?
    /// Only a folder on disk that is the user's own; the library never.
    pub fn writable(&self) -> bool {
        matches!(self.role, Role::Home | Role::Folder | Role::Added)
            && matches!(self.kind, RootKind::Dir(_))
    }

    /// The folder on disk, for a place that is one.
    pub fn dir(&self) -> Option<&std::path::Path> {
        match &self.kind {
            RootKind::Dir(d) => Some(d),
            _ => None,
        }
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: a best-effort filesystem probe (a missing folder reads as not there, read again on the next list)"
    )]
    pub fn json(&self, index: usize, storage: &dyn Storage) -> crate::api::RootJson {
        let (kind, path) = match &self.kind {
            RootKind::Files => ("files", None),
            RootKind::Dir(d) => ("dir", Some(d.to_string_lossy().into_owned())),
            RootKind::Library(p) => ("library", Some(format!("lib:{p}"))),
        };
        crate::api::RootJson {
            index,
            label: self.label.clone(),
            kind: kind.to_owned(),
            path,
            role: self.role.as_str().to_owned(),
            writable: self.writable(),
            exists: self
                .dir()
                .is_none_or(|d| storage.stat(d).is_ok_and(|e| e.is_dir())),
        }
    }

    /// Every folder under this place, as paths below it (`a`, `a/b`), sorted:
    /// the folders the page lists, empty ones included.  `None` for a place
    /// that has none to scan (files, the library: its groups are in its
    /// tiles' paths).
    pub fn scan_folders(&self, storage: &dyn Storage) -> Option<Vec<String>> {
        let d = self.dir()?;
        let mut v = Vec::new();
        sub_folders(storage, d, "", 0, &mut v);
        v.sort();
        Some(v)
    }

    /// The tiles in this root now, sorted; `None` for the named files.
    pub fn scan(&self, storage: &dyn Storage, alarm: &dyn Alarm) -> Option<Vec<String>> {
        match &self.kind {
            RootKind::Files => None,
            RootKind::Dir(d) => {
                let mut v = Vec::new();
                tile_files(storage, alarm, d, 0, &mut v);
                v.sort();
                Some(v)
            }
            RootKind::Library(prefix) => {
                let want = format!("lib:{prefix}");
                match storage.library_tiles() {
                    Ok(all) => Some(all.into_iter().filter(|p| p.starts_with(&want)).collect()),
                    Err(e) => {
                        // the list as it was stays: no tile is dropped for a failed scan
                        alarm.alarm_once(&format!("error: preview: cannot list the library: {e}"));
                        None
                    }
                }
            }
        }
    }

    /// The group a tile is listed under: its folder below the root (the
    /// library's `ground`, `water`, …), none at the top.
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: a path outside its root stays as written (display only)"
    )]
    pub fn group(&self, path: &str) -> Option<String> {
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
pub fn lib_group(path: &str) -> Option<String> {
    let rest = path
        .strip_prefix("lib:tiles/")
        .or_else(|| path.strip_prefix("lib:tests/"))?;
    let (g, _) = rest.split_once('/')?;
    Some(g.to_owned())
}

/// Is this `.dtile` a tile (a `tile` node at the top), not a shared part,
/// palette, printer or project file?  A file that can't be read yet counts.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a best-effort filesystem probe (a missing or unreadable file means none yet, read again on the next call)"
)]
pub fn is_tile_document(storage: &dyn Storage, f: &std::path::Path) -> bool {
    storage.read_text(f).map_or(true, |t| {
        t.lines()
            .any(|l| l.starts_with("tile ") || l.starts_with("tile{") || l == "tile")
    })
}

/// The kinds of document a folder lists.  A new kind (a map, the inventory)
/// is one more entry here: the folders list it, and the page's actions
/// (rename, move, trash, duplicate) apply to the kinds that say `actions`.
pub struct DocKind {
    /// The file extension, lower case, without the dot.
    pub ext: &'static str,
    /// What the page calls the kind (`tile`).
    #[allow(dead_code, reason = "used by one build configuration only")]
    pub kind: &'static str,
    /// Whether the folder actions apply (an `.stl` is shown, not managed).
    pub actions: bool,
    /// Whether this file is one of the kind (a `.dtile` that is a tile, not a part).
    pub is_one: fn(&dyn Storage, &std::path::Path) -> bool,
}

fn always(_: &dyn Storage, _: &std::path::Path) -> bool {
    true
}

/// The document kinds, in the order the page lists them.
pub const KINDS: [DocKind; 4] = [
    DocKind {
        ext: "dtile",
        kind: "tile",
        actions: true,
        is_one: is_tile_file,
    },
    // a print list: shown in its folder and opened on the plates; renaming,
    // moving and the rest of the folder actions are for tiles until a list has its own
    DocKind {
        ext: "dmap",
        kind: "map",
        actions: false,
        is_one: always,
    },
    // the inventory: one log in My tiles, opened as a view of what you have printed
    // (docs/inventory.html); not renamed, moved or trashed from the page
    DocKind {
        ext: "dinv",
        kind: "inventory",
        actions: false,
        is_one: always,
    },
    DocKind {
        ext: "stl",
        kind: "stl",
        actions: false,
        is_one: always,
    },
];

/// A `.dtile` that is a tile: not the project file, not a part or palette.
fn is_tile_file(storage: &dyn Storage, f: &std::path::Path) -> bool {
    f.file_name()
        .and_then(|n| n.to_str())
        .is_none_or(|n| !n.eq_ignore_ascii_case("dharmatiles.dtile"))
        && is_tile_document(storage, f)
}

/// The kind a file name is, by its extension.
pub fn kind_of(name: &str) -> Option<&'static DocKind> {
    let ext = std::path::Path::new(name)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    KINDS.iter().find(|k| k.ext == ext)
}

/// The folders under `dir` (below `rel`), hidden ones and build output skipped.
fn sub_folders(
    storage: &dyn Storage,
    dir: &std::path::Path,
    rel: &str,
    depth: usize,
    out: &mut Vec<String>,
) {
    let Ok(entries) = storage.list(dir) else {
        // the same folder is read by `tile_files`, which tells a failure (once)
        return;
    };
    for e in entries {
        let name = e.name;
        let p = e.path;
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) || e.kind != EntryKind::Dir {
            continue;
        }
        let here = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        out.push(here.clone());
        if depth < 8 {
            sub_folders(storage, &p, &here, depth + 1, out);
        }
    }
}

/// Every document of a kind the folders list (a `.dtile` that is a tile, an
/// `.stl`) under `dir`, skipping hidden folders and build output.
pub fn tile_files(
    storage: &dyn Storage,
    alarm: &dyn Alarm,
    dir: &std::path::Path,
    depth: usize,
    out: &mut Vec<String>,
) {
    let entries = match storage.list(dir) {
        Ok(e) => e,
        // a folder not made yet (the personal home) simply has no tiles
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            alarm.alarm_once(&format!(
                "error: preview: cannot list {}: {e}; its tiles are missing from the list",
                dir.display()
            ));
            return;
        }
    };
    for e in entries {
        let name = e.name;
        if name.starts_with('.') {
            continue;
        }
        let p = e.path;
        if e.kind == EntryKind::Dir {
            if depth < 8 && !SKIP_DIRS.contains(&name.as_str()) {
                tile_files(storage, alarm, &p, depth + 1, out);
            }
            continue;
        }
        if kind_of(&name).is_some_and(|k| (k.is_one)(storage, &p)) {
            out.push(p.to_string_lossy().into_owned());
        }
    }
}
