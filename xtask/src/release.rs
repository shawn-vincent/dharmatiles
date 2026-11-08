//! `cargo xtask release <version> "<summary>"`: number a release.
//!
//! The changelog's Unreleased entry becomes the version, dated today and
//! led by the summary; the workspace version in `Cargo.toml` is bumped;
//! the checks run; then one commit (`<version>`), an annotated tag
//! (`v<version>`), and a push of both.

use std::path::Path;
use std::process::Command;

use crate::run;

const UNRELEASED_TOC: &str = r##"  <li><a href="#unreleased">Unreleased</a></li>"##;
const UNRELEASED_HEAD: &str = r#"<h2 id="unreleased">Unreleased</h2>"#;
const UNRELEASED_NOTE: &str = "<p>In the source, not yet in a numbered version.</p>";

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A version `major.minor.patch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u32, u32, u32);

impl Version {
    fn parse(s: &str) -> Option<Self> {
        let mut it = s.trim().split('.').map(|p| p.parse::<u32>().ok());
        let v = Self(it.next()??, it.next()??, it.next()??);
        it.next().is_none().then_some(v)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// A calendar day.
struct Day {
    year: i32,
    month: usize,
    day: u32,
}

impl Day {
    /// Today, local time (from `date`).
    fn today() -> Result<Self, String> {
        let out = Command::new("date")
            .arg("+%Y %m %d")
            .output()
            .map_err(|e| format!("cannot run date: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut it = text.split_whitespace();
        let bad = || format!("date said {text:?}");
        let year = it.next().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
        let month: usize = it.next().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
        let day = it.next().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
        if !(1..=12).contains(&month) {
            return Err(bad());
        }
        Ok(Self { year, month, day })
    }

    /// `7 Nov 2026`, for the contents list.
    fn short(&self) -> String {
        format!(
            "{} {} {}",
            self.day,
            &MONTHS[self.month - 1][..3],
            self.year
        )
    }

    /// `7 November 2026`, for the heading.
    fn long(&self) -> String {
        format!("{} {} {}", self.day, MONTHS[self.month - 1], self.year)
    }
}

/// The workspace version in `Cargo.toml`, and the text with it set to `to`.
fn bump(manifest: &str, to: Version) -> Result<(Version, String), String> {
    let mut in_package = false;
    let mut found = None;
    let mut out = Vec::new();
    for line in manifest.lines() {
        if line.starts_with('[') {
            in_package = line.trim() == "[workspace.package]";
        }
        if in_package
            && found.is_none()
            && let Some(rest) = line.strip_prefix("version = \"")
        {
            let old = rest.trim_end_matches('"');
            found = Some(Version::parse(old).ok_or_else(|| format!("version {old:?}"))?);
            out.push(format!("version = \"{to}\""));
            continue;
        }
        out.push(line.to_owned());
    }
    let old = found.ok_or("no version under [workspace.package] in Cargo.toml")?;
    let mut text = out.join("\n");
    if manifest.ends_with('\n') {
        text.push('\n');
    }
    Ok((old, text))
}

/// The changelog with its Unreleased entry numbered `v`, dated `day` and
/// led by `summary`.
fn number(changelog: &str, v: Version, day: &Day, summary: &str) -> Result<String, String> {
    let count = |s: &str| changelog.matches(s).count();
    if count(UNRELEASED_TOC) != 1 || count(UNRELEASED_HEAD) != 1 {
        return Err(
            "docs/changelog.html has no Unreleased entry (or more than one): nothing to release"
                .into(),
        );
    }
    let at = changelog.find(UNRELEASED_HEAD).unwrap_or(0);
    let entry = &changelog[at..];
    let end = entry[UNRELEASED_HEAD.len()..]
        .find("<h2")
        .map_or(entry.len(), |e| e + UNRELEASED_HEAD.len());
    if !entry[..end].contains("<li>") {
        return Err("the Unreleased entry lists no changes".into());
    }
    let head = format!(r#"<h2 id="v{v}">{v} <small>{}</small></h2>"#, day.long());
    let lead = format!("<p>{}</p>", summary.trim());
    let text = changelog
        .replacen(
            UNRELEASED_TOC,
            &format!(r##"  <li><a href="#v{v}">{v} · {}</a></li>"##, day.short()),
            1,
        )
        .replacen(
            &format!("{UNRELEASED_HEAD}\n{UNRELEASED_NOTE}"),
            &format!("{head}\n{lead}"),
            1,
        );
    // an entry written without the standard note
    Ok(if text.contains(UNRELEASED_HEAD) {
        text.replacen(UNRELEASED_HEAD, &format!("{head}\n{lead}"), 1)
    } else {
        text
    })
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn cargo(root: &Path, args: &[&str]) -> Result<(), String> {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    run(c.current_dir(root).args(args))
}

pub fn run_release(root: &Path, args: &[String]) -> Result<(), String> {
    let flag = |f: &str| args.iter().any(|a| a == f);
    let words: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [version, summary] = words[..] else {
        return Err(
            "usage: cargo xtask release <version> \"<summary>\" [--no-tests] [--no-push]".into(),
        );
    };
    let v = Version::parse(version).ok_or_else(|| format!("not a version: {version}"))?;
    let tag = format!("v{v}");

    // where we stand
    if git(root, &["rev-parse", "--abbrev-ref", "HEAD"])? != "main" {
        return Err("release from main".into());
    }
    if !git(root, &["status", "--porcelain"])?.is_empty() {
        return Err("the working tree has changes; commit them first".into());
    }
    if !git(root, &["tag", "-l", &tag])?.is_empty() {
        return Err(format!("{tag} is already tagged"));
    }
    let manifest_path = root.join("Cargo.toml");
    let changelog_path = root.join("docs/changelog.html");
    let read = |p: &Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let (old, manifest) = bump(&read(&manifest_path)?, v)?;
    if v <= old {
        return Err(format!("{v} is not past the current version, {old}"));
    }
    let changelog = number(&read(&changelog_path)?, v, &Day::today()?, summary)?;

    // the checks
    cargo(root, &["fmt", "--check"])?;
    cargo(
        root,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    crate::docs::run(root, true)?;
    if !flag("--no-tests") {
        cargo(root, &["test", "--release", "--workspace"])?;
    }

    // the release
    let write =
        |p: &Path, s: &str| std::fs::write(p, s).map_err(|e| format!("{}: {e}", p.display()));
    write(&manifest_path, &manifest)?;
    write(&changelog_path, &changelog)?;
    cargo(root, &["build", "--release"])?; // Cargo.lock takes the version
    git(
        root,
        &["add", "Cargo.toml", "Cargo.lock", "docs/changelog.html"],
    )?;
    git(root, &["commit", "-q", "-m", &v.to_string()])?;
    git(
        root,
        &["tag", "-a", &tag, "-m", &format!("dharmatiles {v}")],
    )?;
    if flag("--no-push") {
        println!("released {v} ({tag}); not pushed: git push origin main {tag}");
    } else {
        git(root, &["push", "-q", "origin", "main", &tag])?;
        println!("released {v} ({tag}), pushed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Day = Day {
        year: 2026,
        month: 11,
        day: 7,
    };

    #[test]
    fn versions_order_and_parse() {
        assert_eq!(Version::parse("0.10.2"), Some(Version(0, 10, 2)));
        assert!(Version::parse("1.2").is_none() && Version::parse("1.2.3.4").is_none());
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
    }

    #[test]
    fn the_workspace_version_is_bumped_and_nothing_else() {
        let m = "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"0.9.0\"\nedition = \"2024\"\n\n[workspace.dependencies]\nfoo = { version = \"1.0.0\" }\n";
        let (old, new) = bump(m, Version(0, 10, 0)).unwrap();
        assert_eq!(old, Version(0, 9, 0));
        assert_eq!(new, m.replace("0.9.0", "0.10.0"));
    }

    #[test]
    fn the_unreleased_entry_is_numbered() {
        let c = format!(
            "<ul>\n{UNRELEASED_TOC}\n  <li><a href=\"#v0.9.0\">0.9.0</a></li>\n</ul>\n{UNRELEASED_HEAD}\n{UNRELEASED_NOTE}\n<ul>\n  <li>A change.</li>\n</ul>\n<h2 id=\"v0.9.0\">0.9.0</h2>\n"
        );
        let n = number(&c, Version(0, 10, 0), &DAY, "Better.").unwrap();
        assert!(n.contains(r##"<li><a href="#v0.10.0">0.10.0 · 7 Nov 2026</a></li>"##));
        assert!(n.contains(
            "<h2 id=\"v0.10.0\">0.10.0 <small>7 November 2026</small></h2>\n<p>Better.</p>"
        ));
        assert!(!n.contains("Unreleased") && !n.contains(UNRELEASED_NOTE));
        // nothing listed, nothing to release
        let empty = c.replace("  <li>A change.</li>\n", "");
        assert!(number(&empty, Version(0, 10, 0), &DAY, "x").is_err());
        assert!(number("<h2>0.9.0</h2>", Version(0, 10, 0), &DAY, "x").is_err());
    }
}
