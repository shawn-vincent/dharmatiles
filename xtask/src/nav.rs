//! The site's top bar, written once: `cargo xtask docs` puts it on every page of `docs/` that has one, from the list below,
//! with the links made relative to the page and the entry the page belongs to marked current.  Regrouping the bar later
//! is a change to this list only.

use std::path::{Path, PathBuf};

/// (label, page from `docs/`, the folders whose pages belong to the entry).
const ENTRIES: &[(&str, &str, &[&str])] = &[
    ("Language", "language.html", &["guide/"]),
    ("Elements", "elements/index.html", &["elements/"]),
    ("Composing", "composing.html", &[]),
    ("Preview", "preview.html", &[]),
    ("Plates", "plates.html", &[]),
    ("Maps", "maps.html", &[]),
    ("Printers", "printers.html", &[]),
    ("Inventory", "inventory.html", &[]),
    ("Tile system", "standards/tile-system.html", &[]),
    ("Printing", "standards/printing.html", &[]),
    ("Rules", "standards/rules.html", &[]),
    ("Roadmap", "roadmap.html", &[]),
    ("Changelog", "changelog.html", &[]),
];

const OPEN: &str = "<nav class=\"top\">";

/// The bar for the page at `rel` (from `docs/`).
pub fn bar(rel: &str) -> String {
    let up = "../".repeat(rel.matches('/').count());
    let mut s = format!(
        "{OPEN}<div class=\"wrap\"><a class=\"brand\" href=\"{up}index.html\">dharmatiles</a>"
    );
    for (label, href, folders) in ENTRIES {
        let cur = rel == *href || folders.iter().any(|f| rel.starts_with(f));
        let mark = if cur { " aria-current=\"page\"" } else { "" };
        s.push_str(&format!("<a href=\"{up}{href}\"{mark}>{label}</a>"));
    }
    s.push_str("</div></nav>");
    s
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

/// Write the bar on every page that has one; with `check`, change nothing and fail on a stale page.
pub fn run(root: &Path, check: bool) -> Result<(), String> {
    let docs = root.join("docs");
    let mut all = Vec::new();
    pages(&docs, &mut all)?;
    all.sort();
    let mut stale = Vec::new();
    for p in all {
        let rel = p
            .strip_prefix(&docs)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let Some(a) = text.find(OPEN) else { continue };
        let Some(len) = text[a..].find("</nav>") else {
            return Err(format!("{rel}: the top bar never ends"));
        };
        let new = format!(
            "{}{}{}",
            &text[..a],
            bar(&rel),
            &text[a + len + "</nav>".len()..]
        );
        if new != text {
            if check {
                stale.push(rel);
            } else {
                std::fs::write(&p, new).map_err(|e| format!("{}: {e}", p.display()))?;
                println!("docs: wrote {}", p.display());
            }
        }
    }
    if stale.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "stale top bar (run `cargo xtask docs`): {}",
            stale.join(", ")
        ))
    }
}
