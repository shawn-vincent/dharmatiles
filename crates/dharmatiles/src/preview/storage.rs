//! Where the editor's files live: the one seam between the owner and the disk.
//!
//! The owner and [`super::docs::Docs`] read and write a tile's source, move a tile or folder to the Trash and
//! put it back through a [`Storage`], never through `std::fs`.  [`DiskStorage`] is the program's (the paths, the
//! atomic writes and the Trash exactly as they have always been); [`MemStorage`] holds the same in memory, so a test
//! of a save, a rename or a trash and its undo runs with no disk, and a host with no disk at all (the browser's
//! worker) can be given one of its own.
//!
//! Not yet through it (the round after this): the folder scan (`Root::scan`), the inventory file, the printers and
//! the settings, which still read the disk where they are used.

#[cfg(test)]
use std::collections::BTreeMap;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::Mutex;

/// What the editor needs of a file system.
pub trait Storage: Send + Sync {
    /// A document's text.
    ///
    /// # Errors
    /// The file cannot be read.
    fn read_text(&self, path: &Path) -> Result<String>;

    /// Write `text` whole or not at all (a reader never sees half of it).
    ///
    /// # Errors
    /// The folder cannot be written.
    fn write_text(&self, path: &Path, text: &str) -> Result<()>;

    /// Whether anything is at `path` (a link is not followed).
    fn exists(&self, path: &Path) -> bool;

    /// Put `from` at `to` without ever going over a file there.
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
fn trash_name(name: &str, is_file: bool, n: u32) -> String {
    if n == 1 {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && is_file => (s.to_owned(), format!(".{e}")),
        _ => (name.to_owned(), String::new()),
    };
    format!("{stem} {n}{ext}")
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(|| "item".to_owned(), |n| n.to_string_lossy().into_owned())
}

/// The real disk.
pub struct DiskStorage;

impl Storage for DiskStorage {
    fn read_text(&self, path: &Path) -> Result<String> {
        std::fs::read_to_string(path)
    }

    fn write_text(&self, path: &Path, text: &str) -> Result<()> {
        dt_eval::atomic::write_atomic(path, text)
    }

    fn exists(&self, path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    fn move_no_clobber(&self, from: &Path, to: &Path) -> Result<()> {
        use std::io::Write;
        match std::fs::hard_link(from, to) {
            Ok(()) => std::fs::remove_file(from).inspect_err(|_| {
                // best-effort cleanup of the new link; the failure to remove the old name is what is reported
                let _ = std::fs::remove_file(to);
            }),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Err(e),
            Err(_) => {
                // another disk (or one that does not link): copy, created new, and only then the old one removed
                let text = std::fs::read(from)?;
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(to)?;
                if let Err(e) = f.write_all(&text) {
                    // best-effort cleanup of the half-made copy; the write error is what is reported
                    let _ = std::fs::remove_file(to);
                    return Err(e);
                }
                std::fs::remove_file(from)
            }
        }
    }

    fn trash(&self, from: &Path, trash: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(trash)?;
        let name = file_name(from);
        for n in 1..1000 {
            let to = trash.join(trash_name(&name, from.is_file(), n));
            if std::fs::symlink_metadata(&to).is_ok() {
                continue;
            }
            std::fs::rename(from, &to)?;
            return Ok(to);
        }
        Err(Error::other("no free name in the Trash"))
    }

    fn restore(&self, trashed: &Path, original: &Path) -> Result<()> {
        std::fs::rename(trashed, original)
    }
}

/// Files held in memory: the same behaviour as the disk's, for a test (the web worker's host takes it in round 4).
#[cfg(test)]
#[derive(Default)]
pub struct MemStorage {
    files: Mutex<BTreeMap<PathBuf, String>>,
}

#[cfg(test)]
impl MemStorage {
    /// A store holding `text` at `path`.
    #[must_use]
    pub fn with(path: &str, text: &str) -> Self {
        let me = Self::default();
        me.put(Path::new(path), text);
        me
    }

    fn put(&self, path: &Path, text: &str) {
        dt_value::LockExt::lock_unpoisoned(&self.files).insert(path.to_path_buf(), text.to_owned());
    }
}

#[cfg(test)]
impl Storage for MemStorage {
    fn read_text(&self, path: &Path) -> Result<String> {
        dt_value::LockExt::lock_unpoisoned(&self.files)
            .get(path)
            .cloned()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("{} is not there", path.display()),
                )
            })
    }

    fn write_text(&self, path: &Path, text: &str) -> Result<()> {
        self.put(path, text);
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        dt_value::LockExt::lock_unpoisoned(&self.files).contains_key(path)
    }

    fn move_no_clobber(&self, from: &Path, to: &Path) -> Result<()> {
        let mut files = dt_value::LockExt::lock_unpoisoned(&self.files);
        if files.contains_key(to) {
            return Err(Error::new(ErrorKind::AlreadyExists, "taken"));
        }
        let text = files
            .remove(from)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "not there"))?;
        files.insert(to.to_path_buf(), text);
        Ok(())
    }

    fn trash(&self, from: &Path, trash: &Path) -> Result<PathBuf> {
        let mut files = dt_value::LockExt::lock_unpoisoned(&self.files);
        let name = file_name(from);
        for n in 1..1000 {
            let to = trash.join(trash_name(&name, true, n));
            if files.contains_key(&to) {
                continue;
            }
            let text = files
                .remove(from)
                .ok_or_else(|| Error::new(ErrorKind::NotFound, "not there"))?;
            files.insert(to.clone(), text);
            return Ok(to);
        }
        Err(Error::other("no free name in the Trash"))
    }

    fn restore(&self, trashed: &Path, original: &Path) -> Result<()> {
        let mut files = dt_value::LockExt::lock_unpoisoned(&self.files);
        let text = files
            .remove(trashed)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "not in the Trash"))?;
        files.insert(original.to_path_buf(), text);
        Ok(())
    }
}
