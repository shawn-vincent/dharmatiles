//! The engine version for the build record, as `src/dharmatiles/build_info.py`
//! formats it from `git describe --tags --long --always`:
//! `<tag>` on a tag, `<tag>-<n>-g<hash>` after one, `g<hash>` with no tag,
//! each with `-dirty` when the tree has changes; `unknown` without git.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn version() -> String {
    let Some(describe) =
        git(&["describe", "--tags", "--long", "--always"]).filter(|d| !d.is_empty())
    else {
        return "unknown".to_owned();
    };
    let dirty = git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
    let suffix = if dirty { "-dirty" } else { "" };
    // "v1.0.0-12-gabcdef7", or a bare hash
    let mut parts = describe.rsplitn(3, '-');
    if let (Some(hash), Some(n), Some(tag)) = (parts.next(), parts.next(), parts.next())
        && let (Some(short), Ok(n)) = (hash.strip_prefix('g'), n.parse::<u32>())
    {
        return if n == 0 {
            format!("{tag}{suffix}")
        } else {
            format!("{tag}-{n}-g{short}{suffix}")
        };
    }
    format!("g{describe}{suffix}")
}

fn main() {
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo::rerun-if-changed={dir}/HEAD");
        println!("cargo::rerun-if-changed={dir}/index");
    }
    println!("cargo::rustc-env=DT_ENGINE_VERSION={}", version());
}
