//! The roots the preview lists tiles from: files, folders, the library.

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
    pub(super) label: String,
    pub(super) kind: RootKind,
    pub(super) role: Role,
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

    /// The same place, with another role (My tiles, an added folder).
    #[must_use]
    pub fn with_role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    /// May the page change what is in this place (make, rename, move, trash)?
    /// Only a folder on disk that is the user's own; the library never.
    pub(super) fn writable(&self) -> bool {
        matches!(self.role, Role::Home | Role::Folder | Role::Added)
            && matches!(self.kind, RootKind::Dir(_))
    }

    /// The folder on disk, for a place that is one.
    pub(super) fn dir(&self) -> Option<&std::path::Path> {
        match &self.kind {
            RootKind::Dir(d) => Some(d),
            _ => None,
        }
    }

    pub(super) fn json(&self, index: usize) -> super::api::RootJson {
        let (kind, path) = match &self.kind {
            RootKind::Files => ("files", None),
            RootKind::Dir(d) => ("dir", Some(d.to_string_lossy().into_owned())),
            RootKind::Library(p) => ("library", Some(format!("lib:{p}"))),
        };
        super::api::RootJson {
            index,
            label: self.label.clone(),
            kind: kind.to_owned(),
            path,
            role: self.role.as_str().to_owned(),
            writable: self.writable(),
            exists: self.dir().is_none_or(std::path::Path::is_dir),
        }
    }

    /// Every folder under this place, as paths below it (`a`, `a/b`), sorted:
    /// the folders the page lists, empty ones included.  `None` for a place
    /// that has none to scan (files, the library: its groups are in its
    /// tiles' paths).
    pub(super) fn scan_folders(&self) -> Option<Vec<String>> {
        let d = self.dir()?;
        let mut v = Vec::new();
        sub_folders(d, "", 0, &mut v);
        v.sort();
        Some(v)
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

/// The kinds of document a folder lists.  A new kind (a map, the inventory)
/// is one more entry here: the folders list it, and the page's actions
/// (rename, move, trash, duplicate) apply to the kinds that say `actions`.
pub(super) struct DocKind {
    /// The file extension, lower case, without the dot.
    pub(super) ext: &'static str,
    /// What the page calls the kind (`tile`).
    #[allow(dead_code)]
    pub(super) kind: &'static str,
    /// Whether the folder actions apply (an `.stl` is shown, not managed).
    pub(super) actions: bool,
    /// Whether this file is one of the kind (a `.dtile` that is a tile, not a part).
    pub(super) is_one: fn(&std::path::Path) -> bool,
}

fn always(_: &std::path::Path) -> bool {
    true
}

/// The document kinds, in the order the page lists them.
pub(super) const KINDS: [DocKind; 4] = [
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
fn is_tile_file(f: &std::path::Path) -> bool {
    f.file_name()
        .and_then(|n| n.to_str())
        .is_none_or(|n| !n.eq_ignore_ascii_case("dharmatiles.dtile"))
        && is_tile_document(f)
}

/// The kind a file name is, by its extension.
pub(super) fn kind_of(name: &str) -> Option<&'static DocKind> {
    let ext = std::path::Path::new(name)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    KINDS.iter().find(|k| k.ext == ext)
}

/// The folders under `dir` (below `rel`), hidden ones and build output skipped.
fn sub_folders(dir: &std::path::Path, rel: &str, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // the same folder is read by `tile_files`, which tells a failure (once)
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let p = e.path();
        if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) || !p.is_dir() {
            continue;
        }
        let here = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        out.push(here.clone());
        if depth < 8 {
            sub_folders(&p, &here, depth + 1, out);
        }
    }
}

/// Every document of a kind the folders list (a `.dtile` that is a tile, an
/// `.stl`) under `dir`, skipping hidden folders and build output.
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
        if kind_of(&name).is_some_and(|k| (k.is_one)(&p)) {
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
        let home = Root::new("My tiles", RootKind::Dir(personal_home()?)).with_role(Role::Home);
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
