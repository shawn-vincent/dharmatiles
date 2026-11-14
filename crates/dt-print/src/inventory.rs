//! The inventory on disk: `inventory.dinv` in My tiles, read and added to.
//!
//! The document and its counting are `dt_doc::inventory`.  This is what needs
//! the disk and the library: where the file is, which path names a tile (the
//! same tile has one name in the log however a list wrote it), whether a tile
//! changed since it was printed, and a write that is never half done.

use std::path::{Path, PathBuf};

use dt_doc::inventory::{self as doc, Inventory};

use dt_eval::source::{LIB, Source};
use dt_eval::{Diag, Files};

/// Where the inventory is: `inventory.dinv` in `home` (My tiles).
#[must_use]
pub fn path_in(home: &Path) -> PathBuf {
    home.join(doc::FILE)
}

/// A tile's name in the log: `lib:tiles/ground/1x1-grass` for the library's,
/// `home:meadows/glade` for one in My tiles, else its path, the `.dtile` left
/// off.  `path` may be any way of writing it that a print list accepts.
#[must_use]
pub fn canonical(path: &str, home: Option<&Path>) -> String {
    let trimmed = path.strip_suffix(".dtile").unwrap_or(path);
    if trimmed.starts_with(LIB) {
        return trimmed.to_owned();
    }
    if let Some(rest) = trimmed.strip_prefix("home:") {
        return format!("home:{}", rest.trim_start_matches('/'));
    }
    if let Some(h) = home
        && let Ok(rel) = Path::new(trimmed).strip_prefix(h)
    {
        return format!("home:{}", rel.to_string_lossy().replace('\\', "/"));
    }
    trimmed.to_owned()
}

/// The path a tile of the log is read from (`.dtile` put back); `None` for a
/// `home:` name when there is no home.
#[must_use]
pub fn readable(canon: &str, home: Option<&Path>) -> Option<String> {
    if canon.starts_with(LIB) {
        return Some(format!("{canon}.dtile"));
    }
    if let Some(rest) = canon.strip_prefix("home:") {
        return Some(format!("{}.dtile", home?.join(rest).to_string_lossy()));
    }
    Some(format!("{canon}.dtile"))
}

/// A document hash as the log writes it: the first eight hex digits of the
/// text's hash.
#[must_use]
pub fn short_hash(h: u64) -> String {
    format!("{h:016x}")[..8].to_owned()
}

/// The hash of the tile `canon` as it is now, as the log writes it; `None`
/// when the tile cannot be read (it is not there: renamed, moved or deleted).
#[must_use]
pub fn tile_hash(canon: &str, home: Option<&Path>) -> Option<String> {
    let path = readable(canon, home)?;
    let text = Files.read(&path).ok()?;
    Some(short_hash(dt_eval::doc::text_hash(&text)))
}

/// Is the tile different from when it was printed?  False when the print
/// recorded no hash or the tile cannot be read (the latter is its own
/// question: a path that is gone).
#[must_use]
pub fn edited_since(canon: &str, recorded: Option<&str>, home: Option<&Path>) -> bool {
    match (recorded, tile_hash(canon, home)) {
        (Some(r), Some(now)) => !now.starts_with(r) && !r.starts_with(&now),
        _ => false,
    }
}

/// Is the tile still where the log says?
#[must_use]
pub fn tile_exists(canon: &str, home: Option<&Path>) -> bool {
    tile_hash(canon, home).is_some()
}

/// Today's date, `2026-11-12`, in UTC (the editor sends its own local day;
/// the command line has no zone to ask for).
#[must_use]
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date_of(i64::try_from(secs / 86_400).unwrap_or(0))
}

/// The civil date of `days` since 1970-01-01 (Howard Hinnant's algorithm).
#[must_use]
pub fn date_of(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// An inventory read from disk, with its text.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The file.
    pub path: PathBuf,
    /// Its text.
    pub text: String,
    /// What it holds.
    pub inventory: Inventory,
}

/// Read the inventory in `home`.  `Ok(None)` when there is no file: no
/// inventory yet, and nothing is made until the first write.
///
/// # Errors
/// The file is there and cannot be read, or has mistakes: each at its text.
pub fn load(home: &Path) -> Result<Option<Loaded>, Vec<Diag>> {
    let path = path_in(home);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(vec![Diag::plain(
                "dt::inventory",
                format!("cannot read {}: {e}", path.display()),
            )]);
        }
    };
    let inventory = doc::decode(&path.display().to_string(), &text)
        .map_err(|e| e.iter().map(Diag::from).collect::<Vec<_>>())?;
    Ok(Some(Loaded {
        path,
        text,
        inventory,
    }))
}

/// Add `lines` to the end of the log in `home`, making the file (with its
/// header) when this is the first write.  Written whole to a new file and
/// moved into place, so the log is never half a line.
///
/// # Errors
/// The file has mistakes (nothing is added to a file that is not understood,
/// and the mistakes are the answer), or cannot be written.
pub fn append(home: &Path, lines: &[String]) -> Result<PathBuf, Vec<Diag>> {
    let path = path_in(home);
    let text = match load(home)? {
        Some(l) => {
            match dt_doc::ops::apply(
                dt_doc::ops::Kind::Inventory,
                &l.text,
                &dt_doc::ops::Op::Append(lines),
            ) {
                Ok(d) => d.text,
                Err(dt_doc::edit::EditError::Unread(e)) => {
                    return Err(e.iter().map(Diag::from).collect::<Vec<_>>());
                }
                Err(e) => return Err(vec![Diag::plain("dt::inventory", e.to_string())]),
            }
        }
        None => {
            std::fs::create_dir_all(home).map_err(|e| {
                vec![Diag::plain(
                    "dt::inventory",
                    format!("cannot make {}: {e}", home.display()),
                )]
            })?;
            doc::new_text(&dt_doc::header::stamp(env!("CARGO_PKG_VERSION")), lines)
        }
    };
    dt_eval::atomic::write_atomic(&path, text).map_err(|e| {
        vec![Diag::plain(
            "dt::inventory",
            format!("cannot write {}: {e}", path.display()),
        )]
    })?;
    Ok(path)
}

/// A tile named for the log: how the log names it, its seed and its hash.
#[derive(Debug, Clone)]
pub struct Named {
    /// `lib:tiles/ground/1x1-grass`, `home:meadows/glade`.
    pub canon: String,
    /// The seed (`seed`, else the tile's own).
    pub seed: u64,
    /// The first eight hex digits of the document's hash.
    pub hash: String,
}

/// Read a tile the way a print list does (`lib:`, `home:`, or a path), check
/// it for `system` and say how the log names it.
///
/// # Errors
/// The path does not resolve, the tile is not there or does not check.
pub fn name_tile(
    tile: &str,
    dir: Option<&Path>,
    home: Option<&Path>,
    system: &str,
    seed: Option<u64>,
) -> Result<Named, Vec<Diag>> {
    let path = crate::plates::tile_path(tile, dir, home)
        .map_err(|e| vec![Diag::plain("dt::inventory", e)])?;
    let text = Files
        .read(&path)
        .map_err(|e| vec![Diag::plain("dt::inventory", e)])?;
    let opts = dt_eval::Options {
        system: system.to_owned(),
        seed,
        ..dt_eval::Options::default()
    };
    let checked = dt_eval::check(&Files, &path, &opts)?;
    if checked.tile.is_none() {
        return Err(vec![Diag::plain(
            "dt::inventory",
            format!("`{tile}` is not a tile"),
        )]);
    }
    Ok(Named {
        canon: canonical(&path, home),
        seed: checked.seed,
        hash: short_hash(dt_eval::doc::text_hash(&text)),
    })
}
