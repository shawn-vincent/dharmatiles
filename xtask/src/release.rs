//! `cargo xtask release <version> "<summary>"`: number a release.
//!
//! The changes waiting under `docs/changelog.d/` become the version's entry
//! in the changelog, dated today and led by the summary; the workspace version in `Cargo.toml` is bumped;
//! the checks run; then one commit (`<version>`), an annotated tag
//! (`v<version>`), and a push of both.
//!
//! Nothing dirty is released or checked in: the working tree must be
//! clean (commit first), and unless the tests are skipped the checked-in
//! baselines and pictures are rebuilt from that committed tree and the
//! release is refused if any of them differs from what is committed.

use std::path::Path;
use std::process::Command;

use crate::run;

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
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter)"
    )]
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
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: parsing where None is the meaning (a probe of a written word or an optional parameter)"
    )]
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

/// One change not yet in a numbered version: a file under `docs/changelog.d/`,
/// named `YYYY-MM-DD-HHMM-<words>.html`, holding one `<li>…</li>`. Each change
/// is its own file so two branches never edit the same lines; a release folds
/// them, newest first, into its entry and deletes them.
pub struct Fragment {
    pub name: String,
    pub text: String,
}

pub const FRAGMENTS: &str = "docs/changelog.d";

/// Every change waiting for a release, newest first, each checked: its name
/// and that it is one list item.
pub fn fragments(root: &Path) -> Result<Vec<Fragment>, String> {
    let dir = root.join(FRAGMENTS);
    let read = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for e in read {
        let path = e.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == ".keep" {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        check_fragment(&name, &text).map_err(|why| format!("{FRAGMENTS}/{name}: {why}"))?;
        out.push(Fragment {
            name,
            text: text.trim().to_owned(),
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// The changes waiting for a release are each well made, and none is written
/// on the changelog page itself (the page lists numbered versions only).
pub fn check_changes(root: &Path) -> Result<(), String> {
    fragments(root)?;
    let page = root.join("docs/changelog.html");
    let text = std::fs::read_to_string(&page).map_err(|e| format!("{}: {e}", page.display()))?;
    if text.contains(r#"id="unreleased""#) {
        return Err(format!(
            "docs/changelog.html has an Unreleased entry: a change not yet released is its own file under {FRAGMENTS}/ (YYYY-MM-DD-HHMM-some-words.html, one <li>), which a release folds in"
        ));
    }
    Ok(())
}

fn check_fragment(name: &str, text: &str) -> Result<(), String> {
    let b = name.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let words = name.get(16..name.len().saturating_sub(5)).unwrap_or("");
    let named = name.len() > 21
        && digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && b[10] == b'-'
        && digits(11..15)
        && b[15] == b'-'
        && name.ends_with(".html")
        && words
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-');
    if !named {
        return Err(
            "a change is named YYYY-MM-DD-HHMM-some-words.html (lower case, digits and dashes)"
                .into(),
        );
    }
    let t = text.trim();
    if !(t.starts_with("<li>") && t.ends_with("</li>")) || t[4..].contains("<li>") {
        return Err("a change is one list item: <li><strong>What changed:</strong> …</li>".into());
    }
    Ok(())
}

/// The changelog with a new entry numbered `v`, dated `day`, led by
/// `summary` and listing `changes`, above the newest version.
fn number(
    changelog: &str,
    changes: &[Fragment],
    v: Version,
    day: &Day,
    summary: &str,
) -> Result<String, String> {
    if changelog.contains(r#"id="unreleased""#) {
        return Err("docs/changelog.html has an Unreleased entry: changes not yet released are files under docs/changelog.d/".into());
    }
    if changes.is_empty() {
        return Err(format!("{FRAGMENTS}/ lists no changes: nothing to release"));
    }
    const TOC: &str = r##"  <li><a href="#v"##;
    const HEAD: &str = r#"<h2 id="v"#;
    let (Some(toc), Some(head)) = (changelog.find(TOC), changelog.find(HEAD)) else {
        return Err("docs/changelog.html has no version to put the new one above".into());
    };
    let items: String = changes.iter().map(|c| format!("  {}\n", c.text)).collect();
    let entry = format!(
        "<h2 id=\"v{v}\">{v} <small>{}</small></h2>\n<p>{}</p>\n<ul>\n{items}</ul>\n\n",
        day.long(),
        summary.trim()
    );
    let line = format!(r##"  <li><a href="#v{v}">{v} · {}</a></li>"##, day.short());
    let mut text = String::with_capacity(changelog.len() + entry.len() + line.len() + 1);
    text.push_str(&changelog[..toc]);
    text.push_str(&line);
    text.push('\n');
    text.push_str(&changelog[toc..head]);
    text.push_str(&entry);
    text.push_str(&changelog[head..]);
    Ok(text)
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

#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: an environment variable or directory that is not set or not readable is the meaning (the default applies)"
)]
fn cargo(root: &Path, args: &[&str]) -> Result<(), String> {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    run(c.current_dir(root).args(args))
}

/// Rebuild every checked-in baseline (`tests/baseline/library-db.tsv`) and
/// the element pictures from the committed tree, and refuse if any file
/// now differs from what is committed: engine code and the baselines it
/// changes land in one commit, so a baseline may have been built from a
/// dirty tree, and only a rebuild from this one proves it holds.
#[expect(
    clippy::disallowed_methods,
    reason = "permitted silence: an environment variable or directory that is not set or not readable is the meaning (the default applies)"
)]
fn rebuilt_matches_committed(root: &Path) -> Result<(), String> {
    crate::work::heavy_lock(root)?;
    println!("rebuilding the baselines and pictures from the committed tree");
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    run(c.current_dir(root).env("DT_BASELINE", "write").args([
        "test",
        "--release",
        "-p",
        "dt-eval",
        "--test",
        "tiles",
        "--",
        "--ignored",
        crate::work::BASELINE_TEST,
    ]))?;
    crate::pictures::run(root, &[])?;
    let changed = git(root, &["status", "--porcelain"])?;
    if changed.is_empty() {
        return Ok(());
    }
    Err(format!(
        "rebuilt from the committed tree, these differ from what is committed (they are left in the working tree, see git diff):\n{changed}\ncommit the rebuilt files (or fix what makes them differ), then release again"
    ))
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
        return Err(
            "the working tree has changes (git status); commit first: a release is built from committed files"
                .into(),
        );
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
    let changes = fragments(root)?;
    let changelog = number(
        &read(&changelog_path)?,
        &changes,
        v,
        &Day::today()?,
        summary,
    )?;

    // the checks
    cargo(root, &["fmt", "--check"])?;
    cargo(
        root,
        &[
            "clippy",
            "--release",
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
        rebuilt_matches_committed(root)?;
        // the browser's tile is the program's: the wasm bundle against the native build, byte for byte
        crate::work::heavy_lock(root)?;
        crate::parity::run_parity(root, &[])?;
    }

    // the release
    let write =
        |p: &Path, s: &str| std::fs::write(p, s).map_err(|e| format!("{}: {e}", p.display()));
    write(&manifest_path, &manifest)?;
    write(&changelog_path, &changelog)?;
    for c in &changes {
        let p = root.join(FRAGMENTS).join(&c.name);
        std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    cargo(root, &["build", "--release"])?; // Cargo.lock takes the version
    // the examples' posters are named by the build key, which names the engine by its version: the new version
    // renames every poster, so the cards the pages carry are written again, and checked, before they are committed
    crate::docs::run(root, false)?;
    crate::docs::run(root, true)?;
    git(root, &["add", "Cargo.toml", "Cargo.lock", "docs"])?;
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

    fn change(name: &str, text: &str) -> Fragment {
        Fragment {
            name: name.into(),
            text: text.into(),
        }
    }

    #[test]
    fn the_changes_become_the_numbered_entry() {
        let c = "<ol>\n  <li><a href=\"#v0.9.0\">0.9.0</a></li>\n</ol>\n<p>Intro.</p>\n\n<h2 id=\"v0.9.0\">0.9.0</h2>\n";
        let changes = [
            change("2026-11-07-1200-b.html", "<li>Later.</li>"),
            change("2026-11-06-0900-a.html", "<li>Earlier.</li>"),
        ];
        let n = number(c, &changes, Version(0, 10, 0), &DAY, "Better.").unwrap();
        assert_eq!(
            n,
            "<ol>\n  <li><a href=\"#v0.10.0\">0.10.0 · 7 Nov 2026</a></li>\n  <li><a href=\"#v0.9.0\">0.9.0</a></li>\n</ol>\n<p>Intro.</p>\n\n<h2 id=\"v0.10.0\">0.10.0 <small>7 November 2026</small></h2>\n<p>Better.</p>\n<ul>\n  <li>Later.</li>\n  <li>Earlier.</li>\n</ul>\n\n<h2 id=\"v0.9.0\">0.9.0</h2>\n"
        );
        // nothing listed, nothing to release; an old Unreleased entry is refused
        assert!(number(c, &[], Version(0, 10, 0), &DAY, "x").is_err());
        let old = c.replace("<p>Intro.</p>", "<h2 id=\"unreleased\">Unreleased</h2>");
        assert!(number(&old, &changes, Version(0, 10, 0), &DAY, "x").is_err());
    }

    #[test]
    fn a_change_is_named_and_one_item() {
        assert!(
            check_fragment(
                "2026-11-14-1530-drags-keep-up.html",
                "<li><strong>A:</strong> b.</li>\n"
            )
            .is_ok()
        );
        for bad in [
            "2026-11-14-drags.html",
            "2026-11-14-1530-Drags.html",
            "2026-11-14-1530-.html",
            "drags.html",
        ] {
            assert!(check_fragment(bad, "<li>x</li>").is_err(), "{bad}");
        }
        assert!(check_fragment("2026-11-14-1530-two.html", "<li>a</li>\n<li>b</li>").is_err());
        assert!(check_fragment("2026-11-14-1530-para.html", "<p>a</p>").is_err());
    }
}
