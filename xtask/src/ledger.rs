//! The ledger's two rules, on the branch's history: a change can only shrink `ledger.kdl`, except an audit
//! commit (one that touches `ledger.kdl` alone), and a release wants an audit newer than the last release.

use std::collections::BTreeSet;
use std::path::Path;

use dt_doc::ledger::{FILE, Ledger};

use crate::work::git;

/// The ledger as it stood at `rev`: empty when the file did not exist yet.
fn at(root: &Path, rev: &str) -> Result<Ledger, String> {
    let spec = format!("{rev}:{FILE}");
    if git(root, &["cat-file", "-e", &spec]).is_err() {
        return Ok(Ledger::default());
    }
    let text = git(root, &["show", &spec])?;
    Ledger::parse(&text).map_err(|e| format!("at {rev}: {e}"))
}

/// The files a commit changes.
fn files(root: &Path, rev: &str) -> Result<Vec<String>, String> {
    Ok(git(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-m",
            rev,
        ],
    )?
    .lines()
    .map(str::to_owned)
    .collect())
}

/// Whether a commit is an audit commit: it changes `ledger.kdl` and nothing else.
fn is_audit(root: &Path, rev: &str) -> Result<bool, String> {
    Ok(files(root, rev)?.iter().all(|f| f == FILE))
}

/// The ratchet: every id added since the merge-base with main arrives in an audit commit.
///
/// # Errors
/// The ledger cannot be read at some commit, or a commit adds an id and changes another file.
pub fn ratchet(root: &Path) -> Result<(), String> {
    Ledger::load(root).map_err(|e| e.to_string())?;
    let Ok(head) = git(root, &["rev-parse", "HEAD"]) else {
        println!("ledger ratchet skipped: no HEAD");
        return Ok(());
    };
    if git(root, &["rev-parse", "--verify", "-q", "main"]).is_err() {
        println!("ledger ratchet skipped: there is no `main` to compare with");
        return Ok(());
    }
    let mut base = git(root, &["merge-base", "HEAD", "main"])?;
    if base == head {
        // on main itself, the commits not yet pushed
        match git(root, &["merge-base", "HEAD", "main@{upstream}"]) {
            Ok(b) => base = b,
            Err(_) => {
                println!("ledger ratchet skipped: on main, which has no upstream to compare with");
                return Ok(());
            }
        }
    }
    let range = format!("{base}..HEAD");
    let commits = git(root, &["rev-list", "--reverse", "--no-merges", &range])?;
    let mut checked = 0;
    for c in commits.lines() {
        checked += 1;
        let before = at(root, &format!("{c}^"))?;
        let after = at(root, c)?;
        let added: BTreeSet<&str> = after.ids().difference(&before.ids()).copied().collect();
        if added.is_empty() || is_audit(root, c)? {
            continue;
        }
        let short = &c[..c.len().min(8)];
        return Err(format!(
            "commit {short} adds {} to {FILE} and changes other files: a violation is added only by an audit commit, which touches {FILE} alone (fix the case instead, or split the commit)",
            added
                .iter()
                .map(|i| format!("`{i}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    println!("ledger ratchet: {checked} commits since the merge-base add nothing but by audit");
    Ok(())
}

/// The release's audit gate: a commit touching `ledger.kdl` alone, newer than the last `v*` tag.
///
/// # Errors
/// There is none.
pub fn audited_since_last_release(root: &Path) -> Result<(), String> {
    let range = match git(root, &["describe", "--tags", "--abbrev=0", "--match", "v*"]) {
        Ok(tag) => format!("{tag}..HEAD"),
        Err(_) => "HEAD".to_owned(),
    };
    let commits = git(root, &["rev-list", &range, "--", FILE])?;
    for c in commits.lines() {
        if is_audit(root, c)? {
            return Ok(());
        }
    }
    Err(format!(
        "no audit commit (one touching {FILE} alone) since the last release ({range}): run the language-audit skill and commit its ledger, or give --skip audit \"<reason>\""
    ))
}
