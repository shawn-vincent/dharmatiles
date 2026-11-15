//! The program's disk, behind the editor's [`Storage`] ([`dt_service::storage`]): the paths, the atomic writes and the Trash
//! exactly as they have always been.  The trait, [`Entry`] and the in-memory store are the service's, so a browser host
//! can give its own.

use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

pub use dt_service::storage::{Entry, EntryKind, Storage};
use dt_service::storage::{file_name, trash_name};

/// The real disk.
pub struct DiskStorage;

impl Storage for DiskStorage {
    fn read_text(&self, path: &Path) -> Result<String> {
        std::fs::read_to_string(path)
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn write_text(&self, path: &Path, text: &str) -> Result<()> {
        dt_eval::atomic::write_atomic(path, text)
    }

    fn exists(&self, path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    fn stat(&self, path: &Path) -> Result<Entry> {
        let m = std::fs::metadata(path)?;
        Ok(entry_of(path.to_path_buf(), &m))
    }

    fn list(&self, dir: &Path) -> Result<Vec<Entry>> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let p = e.path();
            // a link that leads nowhere is read as the link itself: a file that cannot be opened, never a folder
            let m = match std::fs::metadata(&p) {
                Ok(m) => m,
                Err(_) => e.metadata()?,
            };
            out.push(entry_of(p, &m));
        }
        Ok(out)
    }

    fn create_dir_all(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)
    }

    fn create_dir(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir(dir)
    }

    fn create_new(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        f.write_all(bytes).inspect_err(|_| {
            // best-effort cleanup of the half-made file; the write error is what is reported
            let _ = std::fs::remove_file(path);
        })
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        std::fs::remove_file(path)
    }

    fn move_no_clobber(&self, from: &Path, to: &Path) -> Result<()> {
        match std::fs::hard_link(from, to) {
            Ok(()) => std::fs::remove_file(from).inspect_err(|_| {
                // best-effort cleanup of the new link; the failure to remove the old name is what is reported
                let _ = std::fs::remove_file(to);
            }),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // the same file under another spelling of its name (a disk that ignores case): renamed in place
                match (std::fs::canonicalize(from), std::fs::canonicalize(to)) {
                    (Ok(a), Ok(b)) if a == b => std::fs::rename(from, to),
                    _ => Err(e),
                }
            }
            Err(_) => {
                // another disk (or one that does not link): copy, created new, and only then the old one removed
                let text = std::fs::read(from)?;
                self.create_new(to, &text)?;
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

#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: a store with no modification times (some mounts) says none, and the editor then reads it as unchanged-unknown"
)]
fn entry_of(path: PathBuf, m: &std::fs::Metadata) -> Entry {
    Entry {
        name: file_name(&path),
        kind: if m.is_dir() {
            EntryKind::Dir
        } else {
            EntryKind::File
        },
        size: m.len(),
        // a store with no times (some mounts) says none
        modified: m.modified().ok(),
        readonly: m.permissions().readonly(),
        path,
    }
}
