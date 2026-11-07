//! `cargo xtask docs`: fill each element page's generated reference block
//! from the program's own element schema (`dharmatiles elements --json`), so the
//! site's controls tables cannot drift from what the program accepts.
//!
//! A page opts in with a marked block:
//!
//! ```html
//! <!-- gen:reference cliff -->
//! …replaced on every run…
//! <!-- /gen:reference -->
//! ```

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const OPEN: &str = "<!-- gen:reference ";
const CLOSE: &str = "<!-- /gen:reference -->";
const SOURCE: &str = "https://github.com/shawn-vincent/dharmatiles/blob/main/";

/// Fill every page under `docs/elements/`; with `check`, change nothing
/// and fail if any page is stale.
pub fn run(root: &Path, check: bool) -> Result<(), String> {
    let bin = root.join("target/release/dharmatiles");
    let out = Command::new("cargo")
        .args(["build", "--release", "-q", "-p", "dharmatiles"])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cargo build: {e}"))?;
    if !out.success() {
        return Err("cargo build failed".into());
    }
    let json = Command::new(&bin)
        .args(["elements", "--json"])
        .output()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    let all: Value = serde_json::from_slice(&json.stdout).map_err(|e| e.to_string())?;
    let names: Vec<String> = all["elements"]
        .as_array()
        .ok_or("no elements in `dharmatiles elements --json`")?
        .iter()
        .filter_map(|l| l["name"].as_str().map(str::to_owned))
        .collect();
    let mut elements = Vec::new();
    for n in &names {
        let o = Command::new(&bin)
            .args(["elements", n, "--json"])
            .output()
            .map_err(|e| e.to_string())?;
        elements.push(serde_json::from_slice::<Value>(&o.stdout).map_err(|e| e.to_string())?);
    }
    let tiles = library_tiles(&root.join("library/tiles"))?;

    let dir = root.join("docs/elements");
    // every element the program knows has a page
    let missing: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| !dir.join(format!("{n}.html")).exists())
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "no page in docs/elements/ for: {}",
            missing.join(", ")
        ));
    }
    let mut stale = Vec::new();
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "html"))
        .collect();
    pages.sort();
    for page in pages {
        let text = std::fs::read_to_string(&page).map_err(|e| e.to_string())?;
        let new = fill(&text, &elements, &tiles)?;
        if new != text {
            if check {
                stale.push(page.display().to_string());
            } else {
                std::fs::write(&page, new).map_err(|e| e.to_string())?;
                println!("docs: wrote {}", page.display());
            }
        }
    }
    if stale.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "stale (run `cargo xtask docs`): {}",
            stale.join(", ")
        ))
    }
}

/// `(path from the repo root, text)` of every shipped library tile.
fn library_tiles(dir: &Path) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for g in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let g = g.map_err(|e| e.to_string())?.path();
        if !g.is_dir() || g.file_name().is_some_and(|n| n == "test") {
            continue;
        }
        for f in std::fs::read_dir(&g).map_err(|e| e.to_string())? {
            let f = f.map_err(|e| e.to_string())?.path();
            if f.extension().is_some_and(|x| x == "dtile") {
                let rel = f
                    .strip_prefix(dir.parent().and_then(Path::parent).unwrap_or(dir))
                    .unwrap_or(&f)
                    .display()
                    .to_string();
                out.push((rel, std::fs::read_to_string(&f).map_err(|e| e.to_string())?));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// True if the document writes a node named `name` (the first word of a
/// line, or after `{` or `;`), comments aside.
fn writes(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let line = line.split("//").next().unwrap_or_default();
        line.split(['{', ';'])
            .any(|part| part.split_whitespace().next() == Some(name))
    })
}

fn fill(text: &str, elements: &[Value], tiles: &[(String, String)]) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(OPEN) {
        let after = &rest[i + OPEN.len()..];
        let end_tag = after.find("-->").ok_or("unclosed gen:reference tag")?;
        let name = after[..end_tag].trim();
        let close = after
            .find(CLOSE)
            .ok_or_else(|| format!("gen:reference {name} has no {CLOSE}"))?;
        let element = elements
            .iter()
            .find(|l| l["name"] == name)
            .ok_or_else(|| format!("gen:reference {name}: no such element"))?;
        out.push_str(&rest[..i]);
        out.push_str(OPEN);
        out.push_str(name);
        out.push_str(" -->\n");
        out.push_str(&reference(element, tiles));
        out.push_str(CLOSE);
        rest = &after[close + CLOSE.len()..];
    }
    out.push_str(rest);
    Ok(out)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Code spans for `like this` in schema prose.
fn prose(s: &str) -> String {
    let mut out = String::new();
    for (i, part) in esc(s).split('`').enumerate() {
        if i % 2 == 1 {
            let _ = write!(out, "<code>{part}</code>");
        } else {
            out.push_str(part);
        }
    }
    out
}

fn role_text(role: &str) -> &'static str {
    match role {
        "surface" => "a surface: it paints the ground of its area",
        "thing" => "a thing: one solid, standing on the surface",
        "run" => "a run: built along a tile edge or a drawn line",
        "scatter" => "a scatter: many things, placed; its children say what",
        _ => "an element",
    }
}

fn reference(element: &Value, tiles: &[(String, String)]) -> String {
    let name = element["name"].as_str().unwrap_or_default();
    let mut s = String::new();
    let place: Vec<String> = element["place"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|w| format!("<code>{w}=</code>"))
                .collect()
        })
        .unwrap_or_default();
    let _ = writeln!(s, "<div class=\"facts\">");
    let _ = writeln!(
        s,
        "  <div><div class=\"k\">Kind</div><div class=\"v\">{}</div></div>",
        role_text(element["role"].as_str().unwrap_or_default())
    );
    let _ = writeln!(
        s,
        "  <div><div class=\"k\">Place words</div><div class=\"v\">{}</div></div>",
        if place.is_empty() {
            "none".to_owned()
        } else {
            place.join(" ")
        }
    );
    let needs: Vec<String> = element["needs"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|w| format!("<code>{w}=</code>"))
                .collect()
        })
        .unwrap_or_default();
    if !needs.is_empty() {
        let _ = writeln!(
            s,
            "  <div><div class=\"k\">Must have</div><div class=\"v\">{}</div></div>",
            needs.join(" ")
        );
    }
    if element["holds_scatter"].as_bool() == Some(true) {
        let _ = writeln!(
            s,
            "  <div><div class=\"k\">Holds</div><div class=\"v\">a <a href=\"scatter.html\"><code>scatter</code></a> inside it lands on it</div></div>"
        );
    }
    let _ = writeln!(
        s,
        "  <div><div class=\"k\">Built</div><div class=\"v\">{}</div></div>",
        if element["built"].as_bool() == Some(true) {
            "yes"
        } else {
            "not yet"
        }
    );
    let _ = writeln!(s, "</div>");

    let controls = element["controls"].as_array().cloned().unwrap_or_default();
    let (plain, expert): (Vec<_>, Vec<_>) = controls
        .iter()
        .partition(|c| c["expert"].as_bool() != Some(true));
    let _ = writeln!(s, "<h3 id=\"controls\">Controls</h3>");
    if plain.is_empty() {
        let _ = writeln!(s, "<p>None: <code>{name}</code> takes no controls.</p>");
    } else {
        table(&mut s, &plain);
    }
    if !expert.is_empty() {
        let _ = writeln!(
            s,
            "<details><summary>Expert controls (parts files only)</summary>"
        );
        table(&mut s, &expert);
        let _ = writeln!(s, "</details>");
    }
    let used: Vec<&(String, String)> = tiles.iter().filter(|(_, t)| writes(t, name)).collect();
    let _ = writeln!(s, "<h3 id=\"in-the-library\">In the library</h3>");
    if used.is_empty() {
        let _ = writeln!(
            s,
            "<p>No shipped library tile writes <code>{name}</code> yet.</p>"
        );
    } else {
        let _ = writeln!(s, "<ul class=\"tiles\">");
        for (path, _) in used {
            let file = path.rsplit('/').next().unwrap_or(path);
            let lib = path.trim_start_matches("library/");
            let _ = writeln!(
                s,
                "  <li><a href=\"{SOURCE}{path}\"><code>{file}</code></a> <span class=\"muted small\">lib:{lib}</span></li>"
            );
        }
        let _ = writeln!(s, "</ul>");
    }
    let _ = writeln!(
        s,
        "<p class=\"muted small\">Generated from <code>dharmatiles elements {name}</code> by <code>cargo xtask docs</code>.</p>"
    );
    s
}

fn takes(c: &Value) -> String {
    let unit = c["unit"].as_str().unwrap_or("group");
    let mut t = esc(unit);
    if let Some(takes) = c["takes"].as_str() {
        let _ = write!(t, ", {}", esc(takes));
    }
    t
}

fn table(s: &mut String, controls: &[&Value]) {
    let _ = writeln!(s, "<div class=\"tbl\"><table class=\"controls\">");
    let _ = writeln!(
        s,
        "<thead><tr><th>Control</th><th>Takes</th><th>Default</th><th>What it does</th></tr></thead>\n<tbody>"
    );
    for c in controls {
        row(s, c, None);
        for sub in c["subs"].as_array().into_iter().flatten() {
            row(s, sub, c["name"].as_str());
        }
    }
    let _ = writeln!(s, "</tbody></table></div>");
}

fn row(s: &mut String, c: &Value, parent: Option<&str>) {
    let name = c["name"].as_str().unwrap_or_default();
    let shown = parent.map_or_else(|| name.to_owned(), |p| format!("{p} {name}"));
    let default = c["default"]
        .as_str()
        .map_or_else(|| "—".to_owned(), |d| format!("<code>{}</code>", esc(d)));
    let mut about = prose(c["about"].as_str().unwrap_or_default());
    if c["required"].as_bool() == Some(true) {
        about.push_str(" <strong>Required.</strong>");
    }
    if c["taken"].as_bool() == Some(false) {
        about.push_str(
            " <span class=\"tag\">not built yet</span> <span class=\"muted small\">\
             <code>check</code> accepts it; <code>build</code> refuses it</span>",
        );
    }
    let words = c["words"].as_array().cloned().unwrap_or_default();
    if !words.is_empty() {
        about.push_str("<dl class=\"words\">");
        for w in &words {
            let _ = write!(
                about,
                "<dt><code>{}</code></dt><dd>{}</dd>",
                esc(w["word"].as_str().unwrap_or_default()),
                prose(w["about"].as_str().unwrap_or_default())
            );
        }
        about.push_str("</dl>");
    }
    let id = format!("c-{}", shown.replace(' ', "-"));
    let _ = writeln!(
        s,
        "<tr id=\"{id}\"><td><code>{shown}</code></td><td>{}</td><td>{default}</td><td>{about}</td></tr>",
        takes(c)
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_node_is_found_at_a_line_start_or_after_a_brace() {
        assert!(super::writes("stack {\n  dirt level=edge\n}", "dirt"));
        assert!(super::writes(
            "region a { bounded-by x; fill \"0,0\" }",
            "fill"
        ));
        assert!(!super::writes("// dirt\nground in=dirt", "dirt"));
    }
}
