//! `dharmatiles fix`: lay in the fixes the program's own mistakes carry.
//!
//! A mistake that has a mechanical mend (`dt::seam`, `dt::near-edge`, a misspelt word with a near one) carries it as a
//! [`dt_eval::Fix`], made once where the mistake is made, with the help that says the same words.  This reads each
//! document, takes the fixes its mistakes carry, lays them in with `dt_doc::edit::apply_fixes`, checks the mended text
//! and goes round again (a fix can show the next), up to [`ROUNDS`].  Nothing is computed here: no table of fixes,
//! no splicer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dt_eval::{Diag, Files, Options, Source};

use crate::{ERROR, OK, report};

/// The most rounds of check and fix before it stops and says so.
pub(crate) const ROUNDS: usize = 8;

/// The files as they are on disk, except the ones this run has mended in memory.
struct Mended<'a>(&'a BTreeMap<String, String>);

impl Source for Mended<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        match self.0.get(path) {
            Some(t) => Ok(t.clone()),
            None => Files.read(path),
        }
    }
}

/// One change, as shown.
pub(crate) struct Change {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub code: &'static str,
    pub label: String,
    pub was: String,
    pub replacement: String,
    /// How far a point moves, in millimetres on the build target's square.
    pub moved_mm: Option<f64>,
}

/// What a run found.
pub(crate) struct Outcome {
    /// Every change, in the order made.
    pub changes: Vec<Change>,
    /// Each file's text after the fixes, for the files that changed.
    pub texts: BTreeMap<String, String>,
    /// The mistakes still there with no fix (or fixes left when the rounds ran out).
    pub remaining: Vec<Diag>,
    /// The rounds ran out with fixes still to make.
    pub bound: bool,
}

/// The 1-based line and column of byte `offset` in `text`.
fn place(text: &str, offset: usize) -> (usize, usize) {
    let off = offset.min(text.len());
    let before = &text[..off];
    let line = before.matches('\n').count() + 1;
    let col = before.rfind('\n').map_or(off, |i| off - i - 1) + 1;
    (line, col)
}

/// Fix the documents at `paths` (files).
///
/// # Errors
/// A file that cannot be read, or a fix that cannot be laid in (the text is not what it was made for: a bug).
pub(crate) fn run(paths: &[String], opts: &Options) -> Result<Outcome, String> {
    let square = opts.units().square_mm;
    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    let (mut changes, mut remaining, mut bound) = (Vec::new(), Vec::new(), false);
    for path in paths {
        for round in 0..=ROUNDS {
            let errs = match dt_eval::check(&Mended(&texts), path, opts) {
                Ok(_) => break,
                Err(e) => e,
            };
            // the fixes, by the file they mend; a file that is the library's is not ours to write
            let mut by_file: BTreeMap<String, Vec<dt_eval::Fix>> = BTreeMap::new();
            let mut codes: Vec<(String, usize, &'static str)> = Vec::new();
            let mut unmended: Vec<Diag> = Vec::new();
            for d in &errs {
                match (&d.fix, &d.at) {
                    (Some(f), Some((src, _))) if !src.name().starts_with(dt_eval::source::LIB) => {
                        by_file
                            .entry(src.name().to_owned())
                            .or_default()
                            .push((**f).clone());
                        codes.push((src.name().to_owned(), f.offset, d.code));
                    }
                    _ => unmended.push(d.clone()),
                }
            }
            if by_file.is_empty() {
                remaining.extend(unmended);
                break;
            }
            if round == ROUNDS {
                bound = true;
                remaining.extend(errs);
                break;
            }
            for (file, fixes) in by_file {
                let old = Mended(&texts).read(&file)?;
                let done =
                    dt_doc::edit::apply_fixes(&old, &fixes).map_err(|e| format!("{file}: {e}"))?;
                for f in &done.applied {
                    let code = codes
                        .iter()
                        .find(|(n, off, _)| *n == file && *off == f.offset)
                        .map_or("", |c| c.2);
                    let (line, col) = place(&old, f.offset);
                    changes.push(Change {
                        file: file.clone(),
                        line,
                        col,
                        code,
                        label: f.label.clone(),
                        was: f.was.clone(),
                        replacement: f.replacement.clone(),
                        moved_mm: f.moved.map(|sq| sq * square),
                    });
                }
                texts.insert(file, done.text);
            }
            // the mistakes with no fix are said from the last check alone: one may only follow from a mistake just mended
        }
    }
    Ok(Outcome {
        changes,
        texts,
        remaining,
        bound,
    })
}

/// The `.dtile` files at `args`: a file as given, a folder's own and its folders'.
fn gather(args: &[String]) -> Result<Vec<String>, String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        let said = |e: std::io::Error| format!("cannot read folder `{}`: {e}", dir.display());
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(said)?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<_, _>>()
            .map_err(said)?;
        entries.sort();
        for p in entries {
            let hidden = p
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'));
            if hidden {
                continue;
            }
            if p.is_dir() {
                walk(&p, out)?;
            } else if p.extension().is_some_and(|x| x == "dtile") {
                out.push(p);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for a in args {
        if a.starts_with(dt_eval::source::LIB) {
            return Err(format!(
                "`{a}` is in the program's library, which `fix` does not write: copy the tile out first"
            ));
        }
        let p = PathBuf::from(a);
        if p.is_dir() {
            let mut found = Vec::new();
            walk(&p, &mut found)?;
            out.extend(found.into_iter().map(|p| p.to_string_lossy().into_owned()));
        } else {
            out.push(a.clone());
        }
    }
    Ok(out)
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

pub(crate) fn fix(paths: &[String], opts: &Options, write: bool) -> u8 {
    let files = match gather(paths) {
        Ok(f) => f,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    if files.is_empty() {
        tell!("error: no .dtile documents at {}", paths.join(", "));
        return ERROR;
    }
    let out = match run(&files, opts) {
        Ok(o) => o,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let square = opts.units().square_mm;
    for c in &out.changes {
        say!("{}:{}:{}  {}  {}", c.file, c.line, c.col, c.code, c.label);
        say!("  - {}", c.was);
        say!("  + {}", c.replacement);
        if let Some(mm) = c.moved_mm {
            say!(
                "    moves the point {mm:.1} mm (a {square} mm square, {})",
                opts.system
            );
        }
    }
    let mut code = OK;
    if write {
        for (file, text) in &out.texts {
            if let Err(e) = dt_eval::atomic::write_atomic(Path::new(file), text) {
                tell!("error: cannot write `{file}`: {e}");
                return ERROR;
            }
            say!("wrote {file}");
        }
    }
    let n = out.changes.len();
    let (changes, files) = (
        plural(n, "change", "changes"),
        plural(out.texts.len(), "file", "files"),
    );
    if n == 0 {
        say!("no fix applies");
    } else if write {
        say!("{changes} in {files} written");
    } else {
        say!(
            "{changes} in {files}; nothing written: run again with --write to make {}",
            if n == 1 { "it" } else { "them" }
        );
    }
    if out.bound {
        tell!("error: fixes were still being found after {ROUNDS} rounds; run `fix` again");
        code = ERROR;
    }
    if !out.remaining.is_empty() {
        tell!(
            "{} with no fix:",
            plural(out.remaining.len(), "mistake", "mistakes")
        );
        report(&out.remaining);
        code = ERROR;
    }
    code
}
