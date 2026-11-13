//! The one way a document of the user's is written: a whole file or none.
//!
//! The bytes go to a temporary file in the same folder, are flushed to the
//! disk, and the temporary file is renamed over the target, so a crash or a
//! full disk leaves the old file, never half of a new one.  Every writer of a
//! `.dtile`, `.dmap`, `.dinv` or settings file goes through [`write_atomic`];
//! a test in the program (`atomic_only`) fails if the preview server calls
//! `std::fs::write` instead.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPS: AtomicU64 = AtomicU64::new(0);

/// Write `bytes` to `path`, replacing any file there all at once.  The
/// target's permissions are kept.
///
/// # Errors
/// The folder cannot be written, or the rename fails; the temporary file is
/// removed (if that fails too, the error says so).
pub fn write_atomic(path: &Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
    let name = path
        .file_name()
        .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
    let tmp_name = format!(
        ".{name}.{}-{}.dt-tmp",
        std::process::id(),
        TEMPS.fetch_add(1, Ordering::Relaxed)
    );
    let tmp = dir.map_or_else(|| tmp_name.clone().into(), |d| d.join(&tmp_name));
    let written = std::fs::File::create_new(&tmp)
        .and_then(|mut f| f.write_all(bytes.as_ref()).and_then(|()| f.sync_all()))
        .and_then(|()| match std::fs::metadata(path) {
            Ok(m) => std::fs::set_permissions(&tmp, m.permissions()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        })
        .and_then(|()| std::fs::rename(&tmp, path));
    if let Err(e) = written {
        return match std::fs::remove_file(&tmp) {
            Ok(()) => Err(e),
            Err(c) if c.kind() == std::io::ErrorKind::NotFound => Err(e),
            Err(c) => Err(std::io::Error::new(
                e.kind(),
                format!("{e} (and {} could not be removed: {c})", tmp.display()),
            )),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_whole_and_leaves_no_temporary() {
        let dir = std::env::temp_dir().join(format!("dt-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.dtile");
        write_atomic(&f, "one").unwrap();
        write_atomic(&f, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        // a folder that is not there: an error, and nothing made
        assert!(write_atomic(&dir.join("no/such/b.dtile"), "x").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
