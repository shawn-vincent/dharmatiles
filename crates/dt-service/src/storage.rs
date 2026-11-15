//! Where the editor's files live: the one seam between the owner and the disk.
//!
//! The owner and the editor's documents read and write a tile's source, move a tile or folder to the Trash and
//! put it back through a [`Storage`], never through `std::fs`.  The program's disk store (`DiskStorage`, in the binary) is the one (the paths, the
//! atomic writes and the Trash exactly as they have always been); [`MemStorage`] holds the same in memory, so a test
//! of a save, a rename or a trash and its undo runs with no disk, and a host with no disk at all (the browser's
//! worker) can be given one of its own.
//!
//! Through it too: the folder scan ([`Storage::list`] and its [`Entry`]), a file's state ([`Storage::stat`]), the new
//! files a copy or a new tile makes ([`Storage::create_new`]), deleting a tile, and the settings and inventory files.
//! A watcher's look at modification times (`watch::mtime`) is the one disk read left beside it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// What a path is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
}

/// What a folder listing, or a look at one path, says of it: the one typed record of a file's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    /// The last part of the path.
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    /// `None` where the store keeps no times.
    pub modified: Option<SystemTime>,
    pub readonly: bool,
}

impl Entry {
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir
    }

    #[must_use]
    pub fn is_file(&self) -> bool {
        self.kind == EntryKind::File
    }
}

/// What the editor needs of a file system.
pub trait Storage: Send + Sync {
    /// A document's text.
    ///
    /// # Errors
    /// The file cannot be read.
    fn read_text(&self, path: &Path) -> Result<String>;

    /// A document's bytes.
    ///
    /// # Errors
    /// The file cannot be read.
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>>;

    /// Write `text` whole or not at all (a reader never sees half of it).
    ///
    /// # Errors
    /// The folder cannot be written.
    fn write_text(&self, path: &Path, text: &str) -> Result<()>;

    /// Whether anything is at `path` (a link is not followed).
    fn exists(&self, path: &Path) -> bool;

    /// What is at `path` (a link is followed).
    ///
    /// # Errors
    /// `NotFound` when nothing is there; any other kind is the system's.
    fn stat(&self, path: &Path) -> Result<Entry>;

    /// What is in the folder `dir`, in no particular order.
    ///
    /// # Errors
    /// `NotFound` when there is no such folder; any other kind is the system's.
    fn list(&self, dir: &Path) -> Result<Vec<Entry>>;

    /// Make the folder `dir` and those above it.
    ///
    /// # Errors
    /// The folder cannot be made.
    fn create_dir_all(&self, dir: &Path) -> Result<()>;

    /// Make the one folder `dir`, never going over what is there.
    ///
    /// # Errors
    /// `AlreadyExists` when `dir` is taken.
    fn create_dir(&self, dir: &Path) -> Result<()>;

    /// Write a new file: created new, so a file already there is never overwritten, and a failed write leaves none.
    ///
    /// # Errors
    /// `AlreadyExists` when `path` is taken.
    fn create_new(&self, path: &Path, bytes: &[u8]) -> Result<()>;

    /// Delete the file at `path`.
    ///
    /// # Errors
    /// There is no such file, or it cannot be removed.
    fn remove_file(&self, path: &Path) -> Result<()>;

    /// Put `from` at `to` without ever going over a file there.  The same file under another spelling of its name
    /// (a disk that ignores case) is renamed in place.
    ///
    /// # Errors
    /// `AlreadyExists` when `to` is taken; any other kind is the system's.
    fn move_no_clobber(&self, from: &Path, to: &Path) -> Result<()>;

    /// Move `from` into the Trash folder `trash` under a name that is free there; where it went.
    ///
    /// # Errors
    /// The Trash cannot be made, or has no free name, or the move fails.
    fn trash(&self, from: &Path, trash: &Path) -> Result<PathBuf>;

    /// Put what was trashed (`trashed`) back at `original`.
    ///
    /// # Errors
    /// The move fails.
    fn restore(&self, trashed: &Path, original: &Path) -> Result<()>;
}

/// The name a trashed `name` takes the `n`th time it is looked for (1 is its own).
pub fn trash_name(name: &str, is_file: bool, n: u32) -> String {
    if n == 1 {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && is_file => (s.to_owned(), format!(".{e}")),
        _ => (name.to_owned(), String::new()),
    };
    format!("{stem} {n}{ext}")
}

/// The last part of a path, as text (`item` when there is none).
pub fn file_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(|| "item".to_owned(), |n| n.to_string_lossy().into_owned())
}

/// Files and folders held in memory: the same behaviour as the disk's, for a test and for a host with no disk (the
/// browser's worker).
#[derive(Default)]
pub struct MemStorage {
    state: Mutex<Mem>,
}

#[derive(Default)]
struct Mem {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
    /// A counter in place of a clock: each write is a later time (a second after the epoch).
    clock: u64,
    times: BTreeMap<PathBuf, u64>,
}

impl Mem {
    fn put(&mut self, path: &Path, bytes: &[u8]) {
        self.clock += 1;
        self.times.insert(path.to_path_buf(), self.clock);
        self.files.insert(path.to_path_buf(), bytes.to_vec());
        self.make_dirs(path.parent());
    }

    fn make_dirs(&mut self, mut up: Option<&Path>) {
        while let Some(d) = up {
            if d.as_os_str().is_empty() {
                break;
            }
            self.dirs.insert(d.to_path_buf());
            up = d.parent();
        }
    }

    fn taken(&self, path: &Path) -> bool {
        self.files.contains_key(path) || self.dirs.contains(path)
    }

    fn entry(&self, path: &Path) -> Option<Entry> {
        let (kind, size) = if let Some(b) = self.files.get(path) {
            (EntryKind::File, b.len() as u64)
        } else if self.dirs.contains(path) {
            (EntryKind::Dir, 0)
        } else {
            return None;
        };
        Some(Entry {
            path: path.to_path_buf(),
            name: file_name(path),
            kind,
            size,
            modified: self
                .times
                .get(path)
                .map(|t| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(*t)),
            readonly: false,
        })
    }
}

fn not_found(path: &Path) -> Error {
    Error::new(
        ErrorKind::NotFound,
        format!("{} is not there", path.display()),
    )
}

impl MemStorage {
    /// A store holding `text` at `path`.
    #[must_use]
    pub fn with(path: &str, text: &str) -> Self {
        let me = Self::default();
        me.state().put(Path::new(path), text.as_bytes());
        me
    }

    fn state(&self) -> std::sync::MutexGuard<'_, Mem> {
        dt_value::LockExt::lock_unpoisoned(&self.state)
    }
}

impl Storage for MemStorage {
    fn read_text(&self, path: &Path) -> Result<String> {
        let bytes = self.read_bytes(path)?;
        String::from_utf8(bytes).map_err(|e| Error::new(ErrorKind::InvalidData, e.to_string()))
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        self.state()
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| not_found(path))
    }

    fn write_text(&self, path: &Path, text: &str) -> Result<()> {
        self.state().put(path, text.as_bytes());
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        self.state().taken(path)
    }

    fn stat(&self, path: &Path) -> Result<Entry> {
        self.state().entry(path).ok_or_else(|| not_found(path))
    }

    fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        let m = self.state();
        if !m.dirs.contains(dir) {
            return Err(not_found(dir));
        }
        let names: BTreeSet<&PathBuf> = m
            .files
            .keys()
            .chain(m.dirs.iter())
            .filter(|p| p.parent() == Some(dir))
            .collect();
        Ok(names.into_iter().filter_map(|p| m.entry(p)).collect())
    }

    fn create_dir_all(&self, dir: &Path) -> Result<()> {
        let mut m = self.state();
        m.dirs.insert(dir.to_path_buf());
        m.make_dirs(dir.parent());
        Ok(())
    }

    fn create_dir(&self, dir: &Path) -> Result<()> {
        let mut m = self.state();
        if m.taken(dir) {
            return Err(Error::new(ErrorKind::AlreadyExists, "taken"));
        }
        m.dirs.insert(dir.to_path_buf());
        m.make_dirs(dir.parent());
        Ok(())
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        let mut m = self.state();
        if m.taken(path) {
            return Err(Error::new(ErrorKind::AlreadyExists, "taken"));
        }
        m.put(path, bytes);
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        let mut m = self.state();
        m.times.remove(path);
        m.files
            .remove(path)
            .map(drop)
            .ok_or_else(|| not_found(path))
    }

    fn move_no_clobber(&self, from: &Path, to: &Path) -> Result<()> {
        let mut m = self.state();
        if m.taken(to) {
            return Err(Error::new(ErrorKind::AlreadyExists, "taken"));
        }
        let bytes = m.files.remove(from).ok_or_else(|| not_found(from))?;
        m.times.remove(from);
        m.put(to, &bytes);
        Ok(())
    }

    fn trash(&self, from: &Path, trash: &Path) -> Result<PathBuf> {
        let mut m = self.state();
        let name = file_name(from);
        for n in 1..1000 {
            let to = trash.join(trash_name(&name, true, n));
            if m.taken(&to) {
                continue;
            }
            let bytes = m.files.remove(from).ok_or_else(|| not_found(from))?;
            m.times.remove(from);
            m.put(&to, &bytes);
            return Ok(to);
        }
        Err(Error::other("no free name in the Trash"))
    }

    fn restore(&self, trashed: &Path, original: &Path) -> Result<()> {
        let mut m = self.state();
        let bytes = m
            .files
            .remove(trashed)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "not in the Trash"))?;
        m.times.remove(trashed);
        m.put(original, &bytes);
        Ok(())
    }
}
