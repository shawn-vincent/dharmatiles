//! The editor's state has one owner ([`super::super::owner`]): a lock appears only where one is meant to.
//!
//! A scan of the preview's source, so a new `Mutex` for a piece of state cannot slip in beside the owner: it has to
//! be one of the shared caches below, or the list is changed here, in the open, with a reason.

use std::path::Path;

/// The files that may hold a `Mutex`, and why.
const ALLOWED: &[(&str, &str)] = &[
    (
        "mod.rs",
        "`Shared`'s shared caches and the pieces not yet moved: the build caches and the scene checks (read and filled \
         from many build threads, so a command round trip each would cost more than they save), and the places gate (held \
         across the disk and the folder chooser, which the owner never waits on); and the log's once-only memory",
    ),
    (
        "docs.rs",
        "one lock per document file: the read-modify-write of a file on disk",
    ),
    (
        "worker.rs",
        "a build's own cell for the stage it is in: one build, never shared",
    ),
    (
        "fail.rs",
        "the last panic's words, noted by a panic hook for the route that catches it",
    ),
    (
        "plates.rs",
        "(tests only) the hooks a test runs between two steps of a build",
    ),
];

/// The fields of `Shared` that are behind a lock: the shared caches, and the places gate (a lock held across disk and the chooser).
/// (A map reading's memory is the service's own type, `SceneMemo`, which keeps its lock inside.)
const SHARED_LOCKS: &[&str] = &["gate", "build_caches"];

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("the preview's source folder is readable");
    for e in entries {
        let p = e.expect("a readable directory entry").path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn a_mutex_is_only_where_the_shared_caches_are() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/preview");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    for f in files {
        let name = f.strip_prefix(&src).unwrap().to_string_lossy().into_owned();
        // the tests may name one to say what they test
        if name == "tests.rs" || name.starts_with("tests/") || name.ends_with("/tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap();
        let has = text.contains("Mutex");
        let allowed = ALLOWED.iter().any(|(n, _)| *n == name);
        assert!(
            !has || allowed,
            "{name} holds a Mutex: the state belongs to the owner (owner.rs); a shared cache is listed in {}",
            file!()
        );
    }
}

#[test]
fn shared_holds_only_the_named_locks() {
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/preview/mod.rs"))
            .unwrap();
    let start = text.find("pub struct Shared {").expect("the Shared struct");
    let body = &text[start..];
    let body = &body[..body.find("\n}\n").expect("the end of Shared")];
    let mut found: Vec<&str> = body
        .lines()
        .filter(|l| l.contains("Mutex<"))
        .map(|l| l.trim().split(':').next().unwrap())
        .collect();
    found.sort_unstable();
    let mut want = SHARED_LOCKS.to_vec();
    want.sort_unstable();
    assert_eq!(
        found, want,
        "a field of Shared behind a lock: the tiles, the events, the places, the target and the queue are the owner's"
    );
}
