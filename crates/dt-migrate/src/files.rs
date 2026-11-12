//! Migrating documents with what they can see: the library's heights and
//! the parts a tile uses, read through a [`Reader`], in memory.
//!
//! - [`migrate_files`] carries documents over (each with what it can see);
//! - [`dependencies`] lists the files a document reads besides itself;
//! - [`Legacy`] reads the library's shared files as they were written in
//!   format 0, which is the library a format 0 document was written for.

use dt_doc::Known;
use dt_doc::error::DocError;
use dt_doc::header;
use dt_doc::model::{Const, ConstKind, Document, Source, Use};

use crate::format1::{self, Outcome};
use crate::old;

/// The library's prefix in a path.
pub const LIB: &str = "lib:";
/// The library's heights.
pub const HEIGHTS: &str = "lib:parts/heights.dtile";
/// The library's edge defaults.
pub const EDGES: &str = "lib:parts/edges.dtile";

/// Where documents come from.
pub trait Reader {
    /// The text at `path`.
    ///
    /// # Errors
    /// Why it cannot be read, in words.
    fn read(&self, path: &str) -> Result<String, String>;
}

/// A problem found while migrating.
#[derive(Debug)]
pub enum Problem {
    /// A mistake in a document, at its text.
    Doc(DocError),
    /// A file that cannot be read, or a result that is not right.
    Plain {
        /// Stable code, such as `dt::source`.
        code: &'static str,
        /// What happened.
        message: String,
    },
}

impl From<DocError> for Problem {
    fn from(e: DocError) -> Self {
        Self::Doc(e)
    }
}

fn docs(es: Vec<DocError>) -> Vec<Problem> {
    es.into_iter().map(Problem::Doc).collect()
}

fn plain(path: &str, e: &str) -> Vec<Problem> {
    vec![Problem::Plain {
        code: "dt::source",
        message: format!("{path}: {e}"),
    }]
}

/// A file as migrated.
#[derive(Debug, Clone)]
pub struct Moved {
    /// Its path, as the program reads it.
    pub path: String,
    /// The text it had.
    pub was: String,
    /// The text it has now.
    pub text: String,
    /// The format it was read as.
    pub from: u32,
}

/// The format a document's header says (0 with none).
///
/// # Errors
/// A header that is malformed.
pub fn format_of(path: &str, text: &str) -> Result<u32, Vec<Problem>> {
    header::check_text(path, text)
        .map(|h| h.map_or(0, |h| h.format))
        .map_err(docs)
}

/// The path a `use` line in `from` names: the library for a bare or
/// `lib:` path, the project (relative to `from`) for `./` or `../`.
#[must_use]
pub fn use_path(from: &str, u: &Use) -> String {
    match &u.source {
        Source::Library(p) => format!("{LIB}{p}.dtile"),
        Source::Project(p) => resolve(from, p),
    }
}

/// The path a `use "target"` in `from` names, with `.dtile` added if missing.
fn resolve(from: &str, target: &str) -> String {
    let target = if target.ends_with(".dtile") {
        target.to_owned()
    } else {
        format!("{target}.dtile")
    };
    if target.starts_with(LIB) || target.starts_with('/') {
        return target;
    }
    // Windows paths from the command line use `\`; uses are written `/`
    let from = if cfg!(windows) {
        from.replace('\\', "/")
    } else {
        from.to_owned()
    };
    let dir = from.rfind('/').map_or_else(
        || {
            if from.starts_with(LIB) {
                LIB.to_owned()
            } else {
                String::new()
            }
        },
        |i| from[..=i].to_owned(),
    );
    let mut parts: Vec<&str> = Vec::new();
    let (prefix, rest) = dir
        .strip_prefix(LIB)
        .map_or(("", dir.as_str()), |r| (LIB, r));
    let absolute = rest.starts_with('/');
    for p in rest.split('/').chain(target.split('/')) {
        match p {
            "" | "." => {}
            ".." if parts.last().is_some_and(|l| *l != "..") => {
                parts.pop();
            }
            ".." if absolute => {}
            p => parts.push(p),
        }
    }
    let lead = if absolute { "/" } else { "" };
    format!("{prefix}{lead}{}", parts.join("/"))
}

/// A document of either format, decoded by the decoder for its format.
fn decoded(path: &str, text: &str, known: &Known) -> Result<Document, Vec<Problem>> {
    match format_of(path, text)? {
        0 => old::decode(path, text, known),
        _ => dt_doc::decode(path, text, known),
    }
    .map_err(docs)
}

/// What a document at `path` can see, for migrating it: the library's
/// heights by their plain names, and each part it uses, read through
/// `sources` (a part of either format: its parameters say the same).
///
/// # Errors
/// A file that cannot be read or decoded.
pub fn known_for(sources: &dyn Reader, path: &str, text: &str) -> Result<Known, Vec<Problem>> {
    let mut stack = Vec::new();
    known_in(sources, path, text, &mut stack)
}

fn known_in(
    sources: &dyn Reader,
    path: &str,
    text: &str,
    stack: &mut Vec<String>,
) -> Result<Known, Vec<Problem>> {
    let mut known = Known::new();
    if path != HEIGHTS {
        let h = sources.read(HEIGHTS).map_err(|e| plain(HEIGHTS, &e))?;
        let doc = decoded(HEIGHTS, &h, &Known::new())?;
        // names as a format 0 document reads them: plain
        let consts: Vec<Const> = doc
            .consts
            .iter()
            .filter(|c| c.kind == ConstKind::Let)
            .map(|c| {
                let mut c = c.clone();
                c.name.value = c.name.value.trim_start_matches('$').to_owned();
                c
            })
            .collect();
        known.set_consts(consts);
    }
    stack.push(path.to_owned());
    let uses = old::uses(path, text).map_err(docs)?;
    for u in &uses {
        let part = use_path(path, u);
        if stack.contains(&part) {
            return Err(plain(&part, "uses itself"));
        }
        let t = sources.read(&part).map_err(|e| plain(&part, &e))?;
        let k = known_in(sources, &part, &t, stack)?;
        let d = decoded(&part, &t, &k)?;
        known.add(u.key(), &d);
    }
    stack.pop();
    Ok(known)
}

/// Carry each of `paths` to format 1, in memory.  A document already at
/// format 1 (or newer) is left out of the result, whatever its minor or
/// build part.  `version` is the program's own, for the header.
///
/// # Errors
/// Each document that cannot be migrated, at its text.
pub fn migrate_files(
    sources: &dyn Reader,
    paths: &[String],
    version: &str,
) -> Result<Vec<Moved>, Vec<Problem>> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for path in paths {
        let text = match sources.read(path) {
            Ok(t) => t,
            Err(e) => {
                errors.extend(plain(path, &e));
                continue;
            }
        };
        match migrate_one(sources, path, &text, version) {
            Ok(Some(m)) => out.push(m),
            Ok(None) => {}
            Err(es) => errors.extend(es),
        }
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

fn migrate_one(
    sources: &dyn Reader,
    path: &str,
    text: &str,
    version: &str,
) -> Result<Option<Moved>, Vec<Problem>> {
    if format_of(path, text)? >= format1::TARGET {
        return Ok(None);
    }
    let known = known_for(sources, path, text)?;
    match format1::migrate(path, text, &known, version) {
        Ok(Outcome::Migrated(m)) => Ok(Some(Moved {
            path: path.to_owned(),
            was: text.to_owned(),
            text: m.text,
            from: m.from,
        })),
        Ok(Outcome::Current(_)) => Ok(None),
        Err(es) => Err(docs(es)),
    }
}

/// The files `path` reads besides itself: the library's heights and edge
/// defaults, and every part it uses, deeply.  `path` itself is not among
/// them.
///
/// # Errors
/// A file that cannot be read, or a `use` line that is malformed.
pub fn dependencies(sources: &dyn Reader, path: &str) -> Result<Vec<String>, Vec<Problem>> {
    let mut out: Vec<String> = Vec::new();
    for lib in [HEIGHTS, EDGES] {
        if path != lib {
            out.push(lib.to_owned());
        }
    }
    let mut todo = vec![path.to_owned()];
    while let Some(p) = todo.pop() {
        let text = sources.read(&p).map_err(|e| plain(&p, &e))?;
        let uses = old::uses(&p, &text).map_err(docs)?;
        for u in &uses {
            let part = use_path(&p, u);
            if part != path && !out.contains(&part) {
                out.push(part.clone());
                todo.push(part);
            }
        }
    }
    Ok(out)
}

/// The library's shared files as they were written in format 0 (heights,
/// edge defaults, the grass and tree parts): the library a format 0
/// document was written for.  Everything else is read from the reader
/// inside.
pub struct Legacy<'s>(pub &'s dyn Reader);

impl Reader for Legacy<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        let text = match path {
            "lib:parts/heights.dtile" => include_str!("legacy/heights.dtile"),
            "lib:parts/edges.dtile" => include_str!("legacy/edges.dtile"),
            "lib:parts/grass.dtile" => include_str!("legacy/grass.dtile"),
            "lib:parts/trees.dtile" => include_str!("legacy/trees.dtile"),
            _ => return self.0.read(path),
        };
        Ok(text.to_owned())
    }
}

/// True when the document has a tile (a parts, printer or palette file has
/// none).
#[must_use]
pub fn has_tile(text: &str) -> bool {
    kdl::KdlDocument::parse(text)
        .is_ok_and(|d| d.nodes().iter().any(|n| n.name().value() == "tile"))
}
