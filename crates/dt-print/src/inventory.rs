//! The names the inventory uses: `inventory.dinv` in My tiles, and which path names a tile.
//!
//! The document and its counting are `dt_doc::inventory`.  Reading and adding to the file, and whether a tile changed since
//! it was printed, need the host's files: they are `dt_service::inventory_calls`'.  This is the part that is only words and
//! paths: where the file is, which path names a tile (the same tile has one name in the log however a list wrote it).

use std::path::{Path, PathBuf};

use dt_doc::inventory as doc;

use dt_eval::source::LIB;

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

/// Today's date, `2026-11-12`, in UTC (the editor sends its own local day;
/// the command line has no zone to ask for).
#[must_use]
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter); permitted silence: a clock before 1970 or a missing mtime reads as zero (a stamp, never data)"
)]
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
