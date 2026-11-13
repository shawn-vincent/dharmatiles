//! The application version for the build record and `--version`, drawn
//! from git and never typed.  With `X.Y.Z` the workspace version:
//!
//! - built at the tag `vX.Y.Z`: `X.Y.Z`;
//! - on main (or a detached HEAD), N commits past that tag:
//!   `X.Y.Z+N.gHASH`;
//! - on another branch: `X.Y.Z+<branch>.N.gHASH`, the branch cleaned to
//!   letters, digits and hyphens (all semver build metadata allows);
//! - with uncommitted changes to the engine's inputs, `.dirty` is added
//!   (`X.Y.Z+5.gHASH.dirty`; at the tag itself, `X.Y.Z+dirty`);
//! - without git, plain `X.Y.Z`.
//!
//! Only `v*` tags count, so an unrelated tag never names the engine
//! (`git describe` once printed `walls-fieldstone-e27-125-g…`).  If the tag
//! `vX.Y.Z` doesn't exist yet (the version was bumped by hand, or the
//! checkout has no tags), the count runs from the nearest `v*` tag that is
//! an ancestor, and from the first commit when there is none: some number
//! always shows how far the code is from a release.
//!
//! `DT_SOURCE_HASH` is a hash of the source that decides what a build makes
//! (see `source_hash`): the disk cache keys on it, so a code change, a
//! committed one or not, can never serve geometry the old code made.
//!
//! Only the engine's inputs count for `.dirty` (the Rust crates, the
//! bundled library and the workspace manifests), so a docs edit elsewhere
//! in the repository neither marks the engine dirty nor re-runs this
//! script.

use std::path::Path;
use std::process::Command;

/// The engine's inputs, relative to the workspace root.
const INPUTS: [&str; 6] = [
    "crates",
    "library",
    "xtask",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
];

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The commits past the release: counted from the tag `v{version}`, else
/// from the nearest `v*` tag, else from the first commit.
fn distance(root: &Path, version: &str) -> Option<u64> {
    let tag = format!("refs/tags/v{version}");
    let base = if git(root, &["rev-parse", "-q", "--verify", &tag]).is_some() {
        Some(tag)
    } else {
        git(
            root,
            &["describe", "--tags", "--match", "v*", "--abbrev=0", "HEAD"],
        )
        .filter(|t| !t.is_empty())
    };
    let range = base.map_or_else(|| "HEAD".to_owned(), |b| format!("{b}..HEAD"));
    git(root, &["rev-list", "--count", &range])?.parse().ok()
}

/// The branch, cleaned for build metadata; `None` on main or a detached HEAD.
fn branch(root: &Path) -> Option<String> {
    let name = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if name == "HEAD" || name == "main" {
        return None;
    }
    let clean: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Some(clean)
}

/// FNV-1a over `bytes`, continuing from `h`.
fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Every file under `dir` that is part of what a build makes, path-sorted,
/// into `out`.  Skipped: tests, examples and build output (they decide no
/// geometry).
fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if matches!(name, "tests" | "examples" | "target" | "snapshots") {
                continue;
            }
            collect(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// A hash of the source that decides what a build makes: the crates a build
/// runs through (not the command line, tests or
/// examples: nothing they hold is read by a build, and the cache's own
/// format is in dt-eval), the workspace manifests and lock, the toolchain,
/// the vendored kernel, and the one library file compiled into the program
/// (`parts/heights.dtile`).  The rest of the library is read at run time,
/// and the cache keys on its text.
fn source_hash(root: &Path) -> String {
    let mut files = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root.join("crates")) {
        let mut crates: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        crates.sort();
        for c in crates {
            let name = c.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "dharmatiles" {
                collect(&c, &mut files);
            }
        }
    }
    collect(&root.join("third_party"), &mut files);
    for f in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "library/parts/heights.dtile",
    ] {
        files.push(root.join(f));
    }
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for f in files {
        let rel = f.strip_prefix(root).unwrap_or(&f);
        h = fnv(h, rel.to_string_lossy().replace('\\', "/").as_bytes());
        // an unreadable file is part of the answer: the build script fails
        // loudly rather than hashing less than it should
        match std::fs::read(&f) {
            Ok(b) => h = fnv(fnv(h, &b), &(b.len() as u64).to_le_bytes()),
            Err(e) => panic!(
                "cannot hash {} for the engine's source hash: {e}",
                f.display()
            ),
        }
    }
    format!("{h:016x}")
}

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let root = Path::new(&manifest).join("../..");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    // Re-run on a commit, a checkout or a tag (HEAD, the branch's ref, the
    // index, the tags) and on any edit to the inputs (a directory is
    // scanned whole).
    let mut watch = vec![
        "HEAD".to_owned(),
        "index".to_owned(),
        "packed-refs".to_owned(),
    ];
    if let Some(r) = git(&root, &["symbolic-ref", "-q", "HEAD"]) {
        watch.push(r);
    }
    watch.push("refs/tags".to_owned());
    for p in watch {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", &p]) {
            println!("cargo::rerun-if-changed={}", root.join(path).display());
        }
    }
    for p in INPUTS {
        println!("cargo::rerun-if-changed={}", root.join(p).display());
    }
    println!(
        "cargo::rerun-if-changed={}",
        root.join("third_party").display()
    );
    let engine = match git(&root, &["rev-parse", "--short=7", "HEAD"]).filter(|h| !h.is_empty()) {
        Some(hash) => {
            let mut status = vec!["status", "--porcelain", "--"];
            status.extend(INPUTS);
            let dirty = git(&root, &status).is_some_and(|s| !s.is_empty());
            let n = distance(&root, &version).unwrap_or(0);
            let mut meta: Vec<String> = Vec::new();
            if n > 0 {
                meta.extend(branch(&root));
                meta.push(n.to_string());
                meta.push(format!("g{hash}"));
            }
            if dirty {
                meta.push("dirty".to_owned());
            }
            if meta.is_empty() {
                version
            } else {
                format!("{version}+{}", meta.join("."))
            }
        }
        None => version,
    };
    println!("cargo::rustc-env=DT_ENGINE_VERSION={engine}");
    println!("cargo::rustc-env=DT_SOURCE_HASH={}", source_hash(&root));
}
