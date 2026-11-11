//! Migrating documents from format 0 to format 1 where the evaluator reads
//! them: the library's files, a tile and the parts it uses, in memory or to
//! be written, with the proof that the mesh did not move.
//!
//! - [`migrate_files`] carries documents over (each with what it can see:
//!   the library's heights and the parts it uses);
//! - [`Overlay`] reads those new texts in place of the old, so a build can
//!   run on them without writing anything;
//! - [`mesh_hash`] builds a tile and gives the hash of its mesh, so the
//!   mesh before and after can be compared (the hash guard: STL bytes carry
//!   the version, the mesh does not);
//! - [`Upgrading`] is a [`Source`] that migrates what it reads as it reads
//!   it, for a program whose current format is 1 and a document of format 0.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dt_doc::Known;
use dt_doc::header;
use dt_doc::migrate::format1::{self, Outcome};
use dt_doc::model::Document;

use crate::diag::Diag;
use crate::doc::{EDGES, HEIGHTS, use_path};
use crate::source::Source;
use crate::{Options, build};

/// A file as migrated.
#[derive(Debug, Clone)]
pub struct Moved {
    /// Its path, as the evaluator reads it.
    pub path: String,
    /// The text it had.
    pub was: String,
    /// The text it has now.
    pub text: String,
    /// The format it was read as.
    pub from: u32,
}

/// True when the document has a tile (a parts, printer or palette file has
/// none, and has nothing to build).
#[must_use]
pub fn has_tile(text: &str) -> bool {
    kdl::KdlDocument::parse(text)
        .is_ok_and(|d| d.nodes().iter().any(|n| n.name().value() == "tile"))
}

/// The format a document's header says (0 with none).
///
/// # Errors
/// A header that is malformed.
pub fn format_of(path: &str, text: &str) -> Result<u32, Vec<Diag>> {
    header::check_text_with(path, text, true)
        .map(|h| h.map_or(0, |h| h.format))
        .map_err(|es| es.iter().map(Diag::from).collect())
}

/// `source`'s text over `inner`'s: the files migrated, read where the old
/// ones were.
pub struct Overlay<'s> {
    inner: &'s dyn Source,
    top: BTreeMap<String, String>,
}

impl<'s> Overlay<'s> {
    /// `moved` over `inner`.
    #[must_use]
    pub fn new(inner: &'s dyn Source, moved: &[Moved]) -> Self {
        Self {
            inner,
            top: moved
                .iter()
                .map(|m| (m.path.clone(), m.text.clone()))
                .collect(),
        }
    }

    /// Every path held.
    pub fn held(&self) -> impl Iterator<Item = &str> {
        self.top.keys().map(String::as_str)
    }
}

impl Source for Overlay<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        match self.top.get(path) {
            Some(t) => Ok(t.clone()),
            None => self.inner.read(path),
        }
    }
}

/// What a document at `path` can see, for migrating it: the library's
/// heights by their plain names, and each part it uses, read through
/// `sources` (a part of either format: its parameters say the same).
///
/// # Errors
/// A file that cannot be read or decoded.
pub fn known_for(sources: &dyn Source, path: &str, text: &str) -> Result<Known, Vec<Diag>> {
    let mut stack = Vec::new();
    known_in(sources, path, text, &mut stack)
}

fn plain(path: &str, e: &str) -> Vec<Diag> {
    vec![Diag::plain("dt::source", format!("{path}: {e}"))]
}

fn decoded(path: &str, text: &str, known: &Known) -> Result<Document, Vec<Diag>> {
    dt_doc::decode(path, text, known).map_err(|es| es.iter().map(Diag::from).collect())
}

fn known_in(
    sources: &dyn Source,
    path: &str,
    text: &str,
    stack: &mut Vec<String>,
) -> Result<Known, Vec<Diag>> {
    let mut known = Known::new().with_format_preview(true);
    if path != HEIGHTS {
        let h = sources.read(HEIGHTS).map_err(|e| plain(HEIGHTS, &e))?;
        let doc = decoded(HEIGHTS, &h, &Known::new().with_format_preview(true))?;
        // names as a format 0 document reads them: plain
        let consts: Vec<dt_doc::model::Const> = doc
            .consts
            .iter()
            .filter(|c| c.kind == dt_doc::model::ConstKind::Let)
            .map(|c| {
                let mut c = c.clone();
                c.name.value = c.name.value.trim_start_matches('$').to_owned();
                c
            })
            .collect();
        known.set_consts(consts);
    }
    stack.push(path.to_owned());
    let uses =
        dt_doc::uses(path, text).map_err(|es| es.iter().map(Diag::from).collect::<Vec<_>>())?;
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
    sources: &dyn Source,
    paths: &[String],
    version: &str,
) -> Result<Vec<Moved>, Vec<Diag>> {
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
    sources: &dyn Source,
    path: &str,
    text: &str,
    version: &str,
) -> Result<Option<Moved>, Vec<Diag>> {
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
        Err(es) => Err(es.iter().map(Diag::from).collect()),
    }
}

/// The files `path` reads besides itself: the library's heights and edge
/// defaults, and every part it uses, deeply.  Paths as the evaluator reads
/// them; `path` itself is not among them.
///
/// # Errors
/// A file that cannot be read, or a `use` line that is malformed.
pub fn dependencies(sources: &dyn Source, path: &str) -> Result<Vec<String>, Vec<Diag>> {
    let mut out: Vec<String> = Vec::new();
    for lib in [HEIGHTS, EDGES] {
        if path != lib {
            out.push(lib.to_owned());
        }
    }
    let mut todo = vec![path.to_owned()];
    while let Some(p) = todo.pop() {
        let text = sources.read(&p).map_err(|e| plain(&p, &e))?;
        let uses =
            dt_doc::uses(&p, &text).map_err(|es| es.iter().map(Diag::from).collect::<Vec<_>>())?;
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

/// The hash of the mesh `path` builds into: what the hash guard compares
/// (the STL's bytes carry the version of the program, the mesh does not).
///
/// # Errors
/// Whatever stops the build.
pub fn mesh_hash(sources: &dyn Source, path: &str, opts: &Options) -> Result<u64, Vec<Diag>> {
    Ok(build(sources, path, opts)?.export.mesh.content_hash())
}

/// A mesh that moved in migrating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    /// The tile.
    pub path: String,
    /// The mesh hash read as format 0.
    pub before: u64,
    /// The mesh hash read as format 1.
    pub after: u64,
}

/// The hash guard: build `path` before (`old`) and after (`new`) migrating
/// and compare the meshes.  `Ok(None)` when they are the same.
///
/// # Errors
/// Either build fails.
pub fn guard(
    old: &dyn Source,
    new: &dyn Source,
    path: &str,
    opts: &Options,
) -> Result<Option<Drift>, Vec<Diag>> {
    // both read in the same way: format 1 is read in its development
    let opts = Options {
        format_preview: true,
        ..opts.clone()
    };
    let before = mesh_hash(old, path, &opts)?;
    let after = mesh_hash(new, path, &opts)?;
    Ok((before != after).then(|| Drift {
        path: path.to_owned(),
        before,
        after,
    }))
}

/// One file read as an older format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upgrade {
    /// Its path.
    pub path: String,
    /// The format it was written in.
    pub from: u32,
}

/// A [`Source`] that migrates a document older than `current` as it reads
/// it, leaving the file as it is: the program builds what it reads and says
/// so (`dt::migrated`).  What it could not migrate it reports (see
/// [`Upgrading::failures`]) with the same errors `migrate` gives.
pub struct Upgrading<'s> {
    inner: &'s dyn Source,
    current: u32,
    version: String,
    cache: Mutex<BTreeMap<String, Arc<String>>>,
    done: Mutex<Vec<Upgrade>>,
    failed: Mutex<Vec<Diag>>,
}

impl<'s> Upgrading<'s> {
    /// Read `inner`, migrating whatever is older than `current`.
    #[must_use]
    pub fn new(inner: &'s dyn Source, current: u32, version: &str) -> Self {
        Self {
            inner,
            current,
            version: version.to_owned(),
            cache: Mutex::new(BTreeMap::new()),
            done: Mutex::new(Vec::new()),
            failed: Mutex::new(Vec::new()),
        }
    }

    /// The files migrated so far, in the order read.
    #[must_use]
    pub fn upgraded(&self) -> Vec<Upgrade> {
        self.done
            .lock()
            .map_or_else(|p| p.into_inner().clone(), |d| d.clone())
    }

    /// What could not be migrated.
    #[must_use]
    pub fn failures(&self) -> Vec<Diag> {
        self.failed
            .lock()
            .map_or_else(|p| p.into_inner().clone(), |d| d.clone())
    }
}

impl Source for Upgrading<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        let text = self.inner.read(path)?;
        let format = match format_of(path, &text) {
            Ok(f) => f,
            // a bad header is the decoder's to report, where it reads it
            Err(_) => return Ok(text),
        };
        if format >= self.current.min(format1::TARGET) {
            return Ok(text);
        }
        let cached = self
            .cache
            .lock()
            .map_err(|_| "a migration panicked".to_owned())?
            .get(path)
            .cloned();
        if let Some(t) = cached {
            return Ok((*t).clone());
        }
        let moved = migrate_one(self, path, &text, &self.version);
        match moved {
            Ok(Some(m)) => {
                self.cache
                    .lock()
                    .map_err(|_| "a migration panicked".to_owned())?
                    .insert(path.to_owned(), Arc::new(m.text.clone()));
                self.done
                    .lock()
                    .map_err(|_| "a migration panicked".to_owned())?
                    .push(Upgrade {
                        path: path.to_owned(),
                        from: m.from,
                    });
                Ok(m.text)
            }
            Ok(None) => Ok(text),
            Err(es) => {
                let n = es.len();
                self.failed
                    .lock()
                    .map_err(|_| "a migration panicked".to_owned())?
                    .extend(es);
                Err(format!(
                    "`{path}` is format {format}, and migrating it to format {} found {n} problem(s)",
                    format1::TARGET
                ))
            }
        }
    }
}

/// The warning every place that shows a migrated document gives.
#[must_use]
pub fn migrated_note(path: &str, from: u32) -> Diag {
    let message = format!(
        "this document is format {from}, read as format {} in memory",
        format1::TARGET
    );
    let help =
        "run `dharmatiles migrate` to update the file (the editor offers \"Update this file\")";
    Diag::plain("dt::migrated", format!("{path}: {message}")).help(help)
}
