//! `docs --check`: a repo path written in code on a page must exist.
//!
//! A page that names `crates/dt-eval/tests/baseline/library-db.tsv` says where a file is; when the file moves, the
//! page is false and nothing else notices.  So every `<code>` on a page under `docs/` (the changelog, its entries and the
//! generated `e/` pages aside) that is written as a path into the repository, a first folder the repository has and a file
//! after it, names a file or folder that is there.  A path with a placeholder in it (`<`, `*`, `…`, `{`) is a pattern
//! and is not checked.

use std::path::{Path, PathBuf};

/// The folders a path written on a page may begin with to be read as a repository path.  `tests` is not a folder of the
/// repository's root: a path written from it is a path with its crate left off, the mistake this check is for.
const FIRST: [&str; 9] = [
    "crates",
    "library",
    "docs",
    "xtask",
    "web",
    "extras",
    "reference",
    "third_party",
    "tests",
];

/// Pages that are not current state: the changelog and its entries say what files were, and `e/` is written by the
/// generator.
fn skipped(root: &Path, page: &Path) -> bool {
    let docs = root.join("docs");
    page == docs.join("changelog.html")
        || page.starts_with(docs.join("e"))
        || page.starts_with(docs.join("changelog.d"))
}

fn pages(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            pages(&p, out)?;
        } else if p.extension().is_some_and(|x| x == "html") {
            out.push(p);
        }
    }
    Ok(())
}

/// The text of every `<code>…</code>` in `html`, with its tags taken out.
fn codes(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("<code") {
        let Some(open) = rest[at..].find('>') else {
            break;
        };
        let start = at + open + 1;
        let Some(len) = rest[start..].find("</code>") else {
            break;
        };
        let mut text = String::new();
        let mut in_tag = false;
        for ch in rest[start..start + len].chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => text.push(c),
                _ => {}
            }
        }
        out.push(
            text.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
        );
        rest = &rest[start + len..];
    }
    out
}

/// Whether `text` is written as a path into the repository, as far as it can be told: no spaces or placeholders, a
/// first folder the repository has, a name after it.
fn is_repo_path(text: &str) -> bool {
    let t = text.trim();
    if t.contains([' ', '<', '>', '*', '…', '{', '}', '"', '=', ':', '~']) || t.starts_with('/') {
        return false;
    }
    let Some((first, rest)) = t.split_once('/') else {
        return false;
    };
    FIRST.contains(&first) && !rest.is_empty()
}

/// The paths the pages write in code that are not in the repository, each as `page: path`.
fn missing(root: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    pages(&root.join("docs"), &mut files)?;
    files.sort();
    let mut out = Vec::new();
    for page in files {
        if skipped(root, &page) {
            continue;
        }
        let html =
            std::fs::read_to_string(&page).map_err(|e| format!("{}: {e}", page.display()))?;
        for text in codes(&html) {
            if is_repo_path(&text) && !root.join(text.trim().trim_end_matches('/')).exists() {
                let shown = page
                    .strip_prefix(root)
                    .map_err(|e| format!("{}: {e}", page.display()))?;
                out.push(format!("{}: {}", shown.display(), text.trim()));
            }
        }
    }
    out.dedup();
    Ok(out)
}

/// Fail when a page names a repository path that is not there.
pub fn check(root: &Path) -> Result<(), String> {
    let missing = missing(root)?;
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "a page names a path that is not in the repository (write the path as it is, or take the sentence out):\n  {}",
        missing.join("\n  ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_told_from_a_pattern_and_from_prose() {
        assert!(is_repo_path("crates/dt-eval/tests/baseline/library-db.tsv"));
        assert!(is_repo_path("tests/baseline/library-db.tsv"));
        assert!(is_repo_path("library/tests/plants/2x2-grove.dtile"));
        assert!(!is_repo_path("library/tiles/*/x.dtile"));
        assert!(!is_repo_path("parts/trees"));
        assert!(!is_repo_path("lib:tiles/ground/1x1.dtile"));
        assert!(!is_repo_path("docs/ then more"));
        assert!(!is_repo_path("crates"));
    }

    #[test]
    fn code_is_read_through_its_tags() {
        let found = codes("a <code>x/y</code> b <code class=\"k\">tests/<b>z</b></code>");
        assert_eq!(found, ["x/y", "tests/z"]);
    }

    #[test]
    fn the_repository_has_every_path_its_pages_write() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let root = root.canonicalize().expect("the repository root");
        if let Err(e) = check(&root) {
            panic!("{e}");
        }
    }
}
