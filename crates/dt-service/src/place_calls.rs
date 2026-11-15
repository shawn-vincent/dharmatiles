//! The calls that put a new tile in a folder: `newTile` (a flat, printable tile of a size) and `mine` (a copy of a
//! library tile in My tiles, its `use`s pinned to the library).  Their types and the one function each that the
//! program's HTTP handler and a web worker's `Service` both call, written on [`TileHost`]: the host says where My
//! tiles is ([`TileHost::home`]) and lists what was written ([`TileHost::listed`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::api::TileSummary;
use crate::call::{Refusal, Status, Verb};
use crate::edit_calls::{EDIT_PATH, refusal};
use crate::read_calls::Nothing;
use crate::tile_host::{TileHost, TileRef};

/// A file that appeared or moved: where, and which tile it is.
#[derive(Serialize, Deserialize, Debug, ts_rs::TS)]
pub struct Listed {
    pub path: String,
    pub id: Option<String>,
    pub tile: Option<TileSummary>,
}

/// `newTile`: a new tile in My tiles (or a folder of a place).
#[derive(Deserialize, Serialize, Debug, ts_rs::TS)]
#[ts(optional_fields = nullable)]
#[serde(deny_unknown_fields)]
pub struct NewReq {
    pub name: Option<String>,
    /// Squares across and up, like `2x1`.
    pub size: Option<String>,
    /// The folder place and the folder in it, when not My tiles.
    pub root: Option<usize>,
    pub dir: Option<String>,
}

pub const NEW_TILE: Verb<NewReq, Listed> = Verb::new("newTile");
pub const MINE: Verb<Nothing, Listed> = Verb::new("mine");

fn bad(msg: impl Into<String>) -> Refusal {
    Refusal::refused(Status::BadRequest, msg)
}

/// The file stem for a tile name: the name as written, with only what a file name cannot hold (path separators, `:`,
/// control characters) turned into `-`, and no leading dot.  A tile's file name is its name.
#[must_use]
pub fn file_stem(name: &str) -> String {
    let out: String = name
        .trim()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let out = out.trim_start_matches('.').trim();
    if out.is_empty() {
        "Untitled tile".to_owned()
    } else {
        out.to_owned()
    }
}

/// The stem a tile named `name` is filed under.
///
/// # Errors
/// [`Refusal`] (bad request): more than one line, or over 80 characters.
pub fn tile_name(name: Option<&str>) -> Result<String, Refusal> {
    let name = name.unwrap_or("").trim();
    if name.chars().count() > 80 || name.contains(['\n', '\r']) {
        return Err(bad("a tile name is one line of at most 80 characters"));
    }
    Ok(file_stem(name))
}

/// Write `text` as `<stem>.<ext>` in `dir`, or `<stem> 2.<ext>`, … when that is taken: created new, so another file is
/// never overwritten.
///
/// # Errors
/// [`Refusal`] (internal): a file cannot be written, or no name is free.
pub fn write_new(
    storage: &dyn crate::storage::Storage,
    dir: &Path,
    stem: &str,
    ext: &str,
    text: &[u8],
) -> Result<PathBuf, Refusal> {
    for n in 1..1000 {
        let dest = dir.join(if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem} {n}.{ext}")
        });
        match storage.create_new(&dest, text) {
            Ok(()) => return Ok(dest),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(Refusal::failure(
                    Status::Internal,
                    format!("cannot write {}: {e}", dest.display()),
                )
                .caused_by(&e));
            }
        }
    }
    Err(Refusal::failure(Status::Internal, "no free name"))
}

/// `use "parts/grass"` in a library tile names a library file; in a copy it must say so (`use "lib:parts/grass"`).
/// The build's files say which.  Each `use` line is one [`Op::SetArg`](dt_doc::ops::Op::SetArg) through the checked
/// `apply`, at the node the decoder says the line is: a `use` inside a string or a comment is not one.
///
/// # Errors
/// [`Refusal`]: the text does not read, or an edit is refused.
pub fn pin_uses(
    library: &dyn dt_eval::Source,
    opts: &dt_eval::Options,
    text: &str,
    files: &BTreeMap<String, Option<SystemTime>>,
) -> Result<String, Refusal> {
    use dt_doc::ops::{Kind, Op, apply_with};
    let reader = dt_eval::Evaluated {
        base: library,
        path: EDIT_PATH,
        opts,
    };
    let uses = dt_doc::uses("tile", text).map_err(|errs| {
        Refusal::of_mistakes(&errs.iter().map(dt_eval::Diag::from).collect::<Vec<_>>())
    })?;
    // every id is read from the text as it is now: a SetArg changes the bytes of its own value only, and the nodes keep
    // their ids, so the table is read once
    let table = dt_doc::nodeid::table(text).map_err(|e| refusal(&e))?;
    let mut out = text.to_owned();
    for u in &uses {
        let target = u.path.value.as_str();
        if target.starts_with(dt_eval::LIB) || target.starts_with('/') {
            continue;
        }
        let want = format!("/{}", target.trim_end_matches(".dtile"));
        let Some(lib) = files.keys().find(|f| {
            f.starts_with(dt_eval::LIB)
                && (f.trim_end_matches(".dtile").ends_with(&want)
                    || f.trim_end_matches(".dtile") == format!("{}{}", dt_eval::LIB, &want[1..]))
        }) else {
            continue;
        };
        let Some(node) = table.iter().find(|n| n.at == u.span.offset()) else {
            return Err(Refusal::failure(
                Status::Internal,
                format!("the decoder's `use \"{target}\"` is at a byte no node starts at"),
            ));
        };
        let value = kdl::KdlValue::String(lib.trim_end_matches(".dtile").to_owned());
        out = apply_with(
            &reader,
            Kind::Tile,
            &out,
            &Op::SetArg {
                id: &node.id,
                index: 0,
                value,
            },
        )
        .map_err(|e| refusal(&e))?
        .text;
    }
    Ok(out)
}

/// `mine`: copy the library tile into My tiles, its `use`s pointing back at the library, and say which tile the copy is.
///
/// # Errors
/// [`Refusal`]: the tile is not a library tile, cannot be read, or the copy cannot be written.
pub fn mine<H: TileHost>(host: &H, t: &TileRef, _: Nothing) -> Result<Listed, Refusal> {
    let (tile, files) = (t.path.as_str(), &t.files);
    if !tile.starts_with(dt_eval::LIB) {
        return Err(bad("only a library tile is copied"));
    }
    let Ok(text) = host.library().read(tile) else {
        return Err(Refusal::refused(
            Status::NotFound,
            format!("cannot read {tile}"),
        ));
    };
    let stem = tile
        .rsplit('/')
        .next()
        .unwrap_or("tile")
        .trim_end_matches(".dtile")
        .to_owned();
    let dest = write_new(
        host.storage(),
        &host.home()?,
        &stem,
        "dtile",
        pin_uses(host.library(), &host.options(), &text, files)?.as_bytes(),
    )?;
    Ok(host.listed(&dest))
}

/// `newTile`: a small, valid tile in `into` (My tiles when none): a printable flat square gives the editor something real
/// to select and change; every later gesture remains ordinary `.dtile` text.
///
/// # Errors
/// [`Refusal`]: the size or the name is not one, or the tile cannot be written.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter)"
)]
pub fn new_tile<H: TileHost>(
    host: &H,
    v: &NewReq,
    into: Option<PathBuf>,
) -> Result<Listed, Refusal> {
    // squares across, then up, as the size picker offers: 1 to 8 each way
    let size = v.size.as_deref().unwrap_or("1x1").trim();
    let valid = size.split_once('x').is_some_and(|(c, r)| {
        let ok =
            |n: &str| n.parse::<u32>().is_ok_and(|n| (1..=8).contains(&n)) && !n.starts_with('0');
        ok(c) && ok(r)
    });
    if !valid {
        return Err(bad(
            "size must be squares across and up, like 2x1 (1 to 8 each way)",
        ));
    }
    let stem = tile_name(v.name.as_deref())?;
    // a new document is written in the current format, with its header
    let version = dt_doc::header::stamp(&host.version());
    let text = format!(
        "dharmatiles version=\"{version}\"\n\ntile size=\"{size}\" seed=1 {{\n    about \"A new tile.\"\n    stack {{\n        base\n        dirt\n    }}\n}}\n"
    );
    let into = match into {
        Some(d) => d,
        None => host.home()?,
    };
    let dest = write_new(host.storage(), &into, &stem, "dtile", text.as_bytes())?;
    Ok(host.listed(&dest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_points_its_uses_at_the_library() {
        let files = BTreeMap::from([
            ("lib:tiles/water/a.dtile".to_owned(), None),
            ("lib:parts/grass.dtile".to_owned(), None),
        ]);
        let text = "// a\nuse \"parts/grass\" lawn\nuse \"lib:parts/trees\" oak\ntile {}\n";
        let opts = dt_eval::Options::default();
        let pin = |t: &str| pin_uses(&dt_eval::Files, &opts, t, &files).unwrap();
        assert_eq!(
            pin(text),
            "// a\nuse \"lib:parts/grass\" lawn\nuse \"lib:parts/trees\" oak\ntile {}\n"
        );
        // a `use` in a comment or a string, or written with other spacing, is judged by the decoder
        let odd = "about \"use \\\"parts/grass\\\" x\"\n// use \"parts/grass\" y\nuse   \"parts/grass\"   lawn\ntile {}\n";
        let got = pin(odd);
        assert!(got.contains("use   \"lib:parts/grass\"   lawn"), "{got}");
        assert!(got.contains("// use \"parts/grass\" y"), "{got}");
        assert!(got.contains("about \"use \\\"parts/grass\\\" x\""), "{got}");
    }
}
