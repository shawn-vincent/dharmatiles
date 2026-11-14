//! The tile language's guide, `docs/guide/`: what `cargo xtask docs` writes and checks for its concept pages.
//!
//! The map, `docs/language.html`, lists the concept pages in reading order: that list is the one place the order is written.
//! From it, and from the pages themselves, this module derives:
//!
//! * on every concept page, the guide's own list in the margin (`gen:guide-nav`) and the errors the page explains
//!   (`gen:explains`, from the `data-explains` markers on its rules, each linked to its error page and its rule);
//! * the cheat sheet, `docs/guide/cheat-sheet.html`: every word of the language, from the word tables of `dt-lang`, the library's
//!   heights and the edge contract's seams;
//! * on the map, the script that carries an old `language.html#anchor` to the page that now holds it (`gen:redirects`),
//!   from the ids the concept pages have, so it cannot drift;
//! * on every element page, the concept pages that use it (`gen:seen`), from each concept page's `data-elements`.
//!
//! and checks that the pages hold together: every concept page is in the map and every map entry is a page; each page's *Next*
//! link is the next page in the map; an id is on one page only; and every code sample is either a whole document the program
//! checks (`<pre data-doc>`, run through `dharmatiles check`, so it cannot go stale) or marked as what it is instead
//! (`data-fragment`: a piece of a document; `data-output`: what the program prints; `data-grammar`: notation).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use dt_lang::codes;
use serde_json::Value;

/// Ids every concept page has, so they are no anchor of the old guide.
const TEMPLATE_IDS: [&str; 6] = [
    "result",
    "plainly",
    "the-rules",
    "errors-explained",
    "next",
    "the-tiles",
];
const SAMPLE_MARKS: [&str; 4] = ["data-doc", "data-fragment", "data-output", "data-grammar"];

fn read(p: &Path) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unesc(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// `text` with what lies between the markers `<!-- name -->` and `<!-- /name -->` replaced.
fn set_block(text: &str, name: &str, content: &str) -> Result<String, String> {
    let open = format!("<!-- {name} -->");
    let close = format!("<!-- /{name} -->");
    let a = text.find(&open).ok_or_else(|| format!("no {open} block"))?;
    let b = text[a..]
        .find(&close)
        .ok_or_else(|| format!("{open} is never closed"))?
        + a;
    let body = if content.is_empty() {
        "\n".to_owned()
    } else {
        format!("\n{content}\n")
    };
    Ok(format!("{}{open}{body}{}", &text[..a], &text[b..]))
}

fn title(text: &str) -> String {
    text.split_once("<title>")
        .and_then(|(_, r)| r.split_once("</title>"))
        .map(|(t, _)| t.to_owned())
        .unwrap_or_default()
}

/// Every concept page: file stem, in reading order, from the map's `<ol class="guide-map">`.
fn order(map: &str) -> Result<Vec<String>, String> {
    let a = map
        .find("<ol class=\"guide-map\">")
        .ok_or("docs/language.html has no <ol class=\"guide-map\"> list")?;
    let b = map[a..]
        .find("</ol>")
        .ok_or("the guide map's list never ends")?
        + a;
    let mut out = Vec::new();
    let mut rest = &map[a..b];
    while let Some(i) = rest.find("href=\"guide/") {
        rest = &rest[i + "href=\"guide/".len()..];
        let stem = rest
            .split_once(".html")
            .map(|(s, _)| s.to_owned())
            .ok_or("a map link is not a .html page")?;
        if !out.contains(&stem) {
            out.push(stem);
        }
    }
    Ok(out)
}

/// The ids written in a page (`id="…"`), in order.
fn ids(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(" id=\"") {
        rest = &rest[i + 5..];
        if let Some(e) = rest.find('"') {
            out.push(rest[..e].to_owned());
        }
    }
    out
}

/// The codes each `data-explains` marker names, with the id of the element it is on.
fn markers(text: &str) -> Vec<(Vec<String>, Option<String>)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(m) = text[pos..].find("data-explains=\"") {
        let at = pos + m;
        let start = text[..at].rfind('<').unwrap_or(0);
        let end = text[at..].find('>').map_or(text.len(), |e| at + e);
        let tag = &text[start..end];
        let vs = at + "data-explains=\"".len();
        let ve = text[vs..].find('"').map_or(text.len(), |e| vs + e);
        let codes: Vec<String> = text[vs..ve].split_whitespace().map(str::to_owned).collect();
        let id = tag
            .split_once(" id=\"")
            .and_then(|(_, r)| r.split_once('"'))
            .map(|(i, _)| i.to_owned());
        out.push((codes, id));
        pos = ve;
    }
    out
}

/// The `<pre>` samples of a page: (attributes, text as written).
fn samples(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("<pre") {
        let tail = &rest[i + 4..];
        let Some(gt) = tail.find('>') else { break };
        let attrs = tail[..gt].to_owned();
        let body = &tail[gt + 1..];
        let Some(end) = body.find("</pre>") else {
            break;
        };
        // a card's source is written by the card's own generator and read by its own test
        if attrs.contains("dtx-code") {
            rest = &body[end..];
            continue;
        }
        let code = body[..end]
            .strip_prefix("<code>")
            .and_then(|c| c.strip_suffix("</code>"))
            .unwrap_or(&body[..end]);
        // the sample as the page shows it: the colour spans `cargo xtask docs` wrote are not part of it (a block with other
        // markup is read as escaped text, and a check of it fails loudly on the markup)
        out.push((
            attrs,
            crate::highlight::text_of(code).unwrap_or_else(|| unesc(code)),
        ));
        rest = &body[end..];
    }
    out
}

/// `cargo xtask docs`'s part for the guide.
pub fn run(root: &Path, check: bool, bin: &Path, seams: &Value) -> Result<(), String> {
    let docs = root.join("docs");
    let guide = docs.join("guide");
    let map_path = docs.join("language.html");
    let map = read(&map_path)?;
    let order = order(&map)?;
    let mut problems = Vec::new();
    let mut files: BTreeMap<PathBuf, String> = BTreeMap::new();

    // the pages: exactly the map's
    let mut on_disk: BTreeSet<String> = BTreeSet::new();
    for e in std::fs::read_dir(&guide).map_err(|e| format!("{}: {e}", guide.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.extension().is_some_and(|x| x == "html") {
            let stem = p
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if stem != "cheat-sheet" {
                on_disk.insert(stem);
            }
        }
    }
    for stem in &order {
        if !on_disk.contains(stem) {
            problems.push(format!(
                "the map lists guide/{stem}.html, which is not a page"
            ));
        }
    }
    for stem in &on_disk {
        if !order.contains(stem) {
            problems.push(format!(
                "guide/{stem}.html is a page the map (language.html) does not list"
            ));
        }
    }
    if problems.is_empty() {
        let mut titles = BTreeMap::new();
        let mut texts = BTreeMap::new();
        for stem in &order {
            let t = read(&guide.join(format!("{stem}.html")))?;
            titles.insert(stem.clone(), title(&t));
            texts.insert(stem.clone(), t);
        }
        let mut elements: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut owners: BTreeMap<String, String> = BTreeMap::new();
        let mut redirects: BTreeMap<String, String> = BTreeMap::new();
        let mut samples_to_check: Vec<(String, usize, String)> = Vec::new();
        for (n, stem) in order.iter().enumerate() {
            let page = format!("guide/{stem}.html");
            let mut text = texts[stem].clone();
            // the guide's own list
            let mut nav = String::from(
                "<ol class=\"guide-pages\">\n<li><a href=\"../language.html\">The map</a></li>\n",
            );
            for s in &order {
                let cur = if s == stem {
                    " aria-current=\"page\""
                } else {
                    ""
                };
                let _ = writeln!(nav, "<li><a href=\"{s}.html\"{cur}>{}</a></li>", titles[s]);
            }
            nav.push_str("<li><a href=\"cheat-sheet.html\">Cheat sheet</a></li>\n</ol>");
            text = set_block(&text, "gen:guide-nav", &nav).map_err(|e| format!("{page}: {e}"))?;
            // the errors this page explains
            if text.contains("<!-- gen:explains -->") {
                let mut list = String::new();
                for (cs, id) in markers(&text) {
                    for c in cs {
                        let Some(code) = codes::lookup(&c) else {
                            problems.push(format!(
                                "{page}: a marker names {c}, which is not in the registry"
                            ));
                            continue;
                        };
                        let here = id
                            .as_ref()
                            .map(|i| format!(" <a class=\"cp-in\" href=\"#{i}\">The rule</a>"))
                            .unwrap_or_default();
                        let _ = writeln!(
                            list,
                            "<li><a class=\"codelink\" href=\"../e/{}.html\"><code>{c}</code></a> {}{here}</li>",
                            codes::slug(&c),
                            crate::codes::inline(code.summary)
                        );
                    }
                }
                let body = if list.is_empty() {
                    "<p>This page explains no error codes.</p>".to_owned()
                } else {
                    format!("<ul class=\"explains\">\n{}</ul>", list.trim_end())
                };
                text =
                    set_block(&text, "gen:explains", &body).map_err(|e| format!("{page}: {e}"))?;
            } else {
                problems.push(format!(
                    "{page}: has no gen:explains block (\"Errors this page explains\")"
                ));
            }
            // Next is the next page in the map
            let expect = order
                .get(n + 1)
                .map_or("cheat-sheet".to_owned(), Clone::clone);
            let next_href = text
                .split_once("id=\"next\"")
                .and_then(|(_, r)| r.split_once("href=\""))
                .and_then(|(_, r)| r.split_once('"'))
                .map(|(h, _)| h.to_owned());
            if next_href.as_deref() != Some(&format!("{expect}.html")) {
                problems.push(format!(
                    "{page}: its Next link is {next_href:?}, and the map's next page is {expect}.html"
                ));
            }
            // the template: result, plainly, rules
            for need in [
                "id=\"result\"",
                "id=\"plainly\"",
                "id=\"errors-explained\"",
                "id=\"next\"",
            ] {
                if stem != "worked" && !text.contains(need) {
                    problems.push(format!("{page}: the page shape has no section {need}"));
                }
            }
            // one id, one page
            for i in ids(&text) {
                if TEMPLATE_IDS.contains(&i.as_str()) {
                    continue;
                }
                if let Some(o) = owners.insert(i.clone(), stem.clone()) {
                    problems.push(format!("the id #{i} is on guide/{o}.html and on {page}"));
                }
                redirects.insert(i, format!("{stem}.html"));
            }
            // the elements the page is seen from
            if let Some(list) = text
                .split_once("data-elements=\"")
                .and_then(|(_, r)| r.split_once('"'))
                .map(|(l, _)| l)
            {
                for e in list.split_whitespace() {
                    if !docs.join(format!("elements/{e}.html")).exists() {
                        problems.push(format!(
                            "{page}: data-elements names {e}, which has no element page"
                        ));
                    }
                    elements.entry(e.to_owned()).or_default().push(stem.clone());
                }
            } else {
                problems.push(format!("{page}: no data-elements on the article"));
            }
            // the samples
            for (i, (attrs, code)) in samples(&text).into_iter().enumerate() {
                let marks: Vec<&str> = SAMPLE_MARKS
                    .iter()
                    .copied()
                    .filter(|m| attrs.contains(m))
                    .collect();
                match marks.as_slice() {
                    [] => problems.push(format!(
                        "{page}: sample {} is neither a whole document (<pre data-doc>) nor marked (data-fragment, data-output, data-grammar): {}",
                        i + 1,
                        code.lines().next().unwrap_or_default()
                    )),
                    ["data-doc"] => samples_to_check.push((page.clone(), i + 1, code)),
                    [_] => {}
                    _ => problems.push(format!("{page}: sample {} has more than one mark", i + 1)),
                }
            }
            files.insert(guide.join(format!("{stem}.html")), text);
        }
        check_samples(root, bin, &samples_to_check, &mut problems)?;

        // the map's redirect script: an old `language.html#anchor` to the page that holds it now
        for id in ids(&map) {
            if owners.contains_key(&id) {
                problems.push(format!(
                    "the map and guide/{}.html both have the id #{id}",
                    owners[&id]
                ));
            }
        }
        let mut js = String::from("<script>\n(() => {\n  const to = {");
        let rows: Vec<String> = redirects
            .iter()
            .map(|(id, page)| format!("\"{id}\":\"{page}\""))
            .collect();
        js.push_str(&rows.join(","));
        js.push_str("};\n  let id = location.hash.slice(1);\n  try { id = decodeURIComponent(id); } catch { /* a hash that is not an id: stay on the map */ }\n  if (Object.hasOwn(to, id)) location.replace(`guide/${to[id]}#${id}`);\n})();\n</script>");
        let map_new =
            set_block(&map, "gen:redirects", &js).map_err(|e| format!("language.html: {e}"))?;
        files.insert(map_path.clone(), map_new);

        // the cheat sheet
        let nav_line = files[&guide.join("tile.html")]
            .lines()
            .find(|l| l.starts_with("<nav class=\"top\">"))
            .ok_or("guide/tile.html has no top bar")?
            .to_owned();
        let sheet = cheat_sheet(root, &nav_line, seams, &order, &titles)?;
        files.insert(guide.join("cheat-sheet.html"), sheet);

        // the element pages: seen from the language
        let el_dir = docs.join("elements");
        for e in std::fs::read_dir(&el_dir).map_err(|e| format!("{}: {e}", el_dir.display()))? {
            let p = e.map_err(|e| e.to_string())?.path();
            if p.extension().is_none_or(|x| x != "html")
                || p.file_stem().is_some_and(|s| s == "index")
            {
                continue;
            }
            let stem = p
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let text = read(&p)?;
            if !text.contains("<!-- gen:seen -->") {
                problems.push(format!(
                    "elements/{stem}.html has no gen:seen block (\"Seen from the language\")"
                ));
                continue;
            }
            let body = match elements.get(&stem) {
                Some(pages) => {
                    let links: Vec<String> = pages
                        .iter()
                        .map(|s| format!("<a href=\"../guide/{s}.html\">{}</a>", titles[s]))
                        .collect();
                    format!(
                        "<section class=\"seen\">\n<h2 id=\"seen\">Seen from the language</h2>\n<p>The guide pages that use <code>{stem}</code>: {}.</p>\n</section>",
                        links.join(", ")
                    )
                }
                None => String::new(),
            };
            files.insert(
                p.clone(),
                set_block(&text, "gen:seen", &body)
                    .map_err(|e| format!("elements/{stem}.html: {e}"))?,
            );
        }
    }

    let mut stale = Vec::new();
    for (path, text) in &files {
        let now = std::fs::read_to_string(path).ok();
        if now.as_deref() != Some(text.as_str()) {
            if check {
                stale.push(
                    path.strip_prefix(&docs)
                        .unwrap_or(path)
                        .display()
                        .to_string(),
                );
            } else {
                std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
                println!("docs: wrote {}", path.display());
            }
        }
    }
    if !stale.is_empty() {
        problems.push(format!(
            "stale (run `cargo xtask docs`): {}",
            stale.join(", ")
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// Run every whole-document sample through `dharmatiles check`: one that the program refuses has gone stale.
fn check_samples(
    root: &Path,
    bin: &Path,
    samples: &[(String, usize, String)],
    problems: &mut Vec<String>,
) -> Result<(), String> {
    if samples.is_empty() {
        return Ok(());
    }
    let scratch = root.join("target/xtask-guide");
    std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    for (page, n, code) in samples {
        let name = format!("{}-{n}.dtile", page.replace(['/', '.'], "-"));
        let file = scratch.join(&name);
        std::fs::write(&file, code).map_err(|e| format!("{}: {e}", file.display()))?;
        let out = Command::new(bin)
            .current_dir(&scratch)
            .env("NO_COLOR", "1")
            .env("COLUMNS", "100")
            .env("DT_CACHE", "off")
            .arg("check")
            .arg(&name)
            .output()
            .map_err(|e| format!("{}: {e}", bin.display()))?;
        if !out.status.success() {
            let said = String::from_utf8_lossy(&out.stderr);
            let said2 = String::from_utf8_lossy(&out.stdout);
            problems.push(format!(
                "{page}: sample {n} is marked as a whole document, and the program refuses it:\n{}{}",
                said, said2
            ));
        }
    }
    Ok(())
}

/// The library's heights, from `library/parts/heights.dtile`: (name, value, what it is).
fn library_heights(root: &Path) -> Result<Vec<(String, String, String)>, String> {
    let text = read(&root.join("library/parts/heights.dtile"))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("let ") else {
            continue;
        };
        let Some((name, rest)) = rest.trim_start().split_once(char::is_whitespace) else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('"') else {
            continue;
        };
        let Some((value, tail)) = rest.split_once('"') else {
            continue;
        };
        let about = tail
            .split_once("//")
            .map(|(_, a)| a.trim().to_owned())
            .unwrap_or_default();
        out.push((name.to_owned(), value.to_owned(), about));
    }
    if out.is_empty() {
        return Err("library/parts/heights.dtile has no heights".to_owned());
    }
    Ok(out)
}

fn cheat_sheet(
    root: &Path,
    nav: &str,
    seams: &Value,
    order: &[String],
    titles: &BTreeMap<String, String>,
) -> Result<String, String> {
    use dt_lang::{
        BODY_SETS, COORD_CALLS, COORDINATES, FUNCTIONS, LAND_SLOPE_DEG, POINT_NAMES, REGION_NAMES,
        REGION_SETS, TREE_SETS, UNITS, WATER_SETTLE_MM, WEIGHTINGS,
    };
    let more = |stem: &str| -> String {
        if order.iter().any(|s| s == stem) {
            format!(
                "<p class=\"muted small\">Read it in <a href=\"{stem}.html\">{}</a>.</p>",
                titles[stem]
            )
        } else {
            String::new()
        }
    };
    let mut s = String::new();
    let _ = write!(
        s,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\n\
<title>Cheat sheet</title>\n<link rel=\"stylesheet\" href=\"../assets/site.css\">\n<script src=\"../assets/site.js\"></script>\n</head>\n<body>\n{nav}\n\
<div class=\"wrap doc-layout\">\n<header class=\"hero\">\n  <div class=\"eyebrow\">The tile language &middot; guide</div>\n  <h1>Cheat sheet</h1>\n  \
<p class=\"lede\">Every word of the language on one page: the units, functions, weightings, coordinates and <code>$</code> names, the library&rsquo;s heights and the seam words. Nothing here is typed: <code>cargo xtask docs</code> writes it from the program&rsquo;s own word tables, so it cannot say what the program does not.</p>\n</header>\n\
<div class=\"margin\">\n<nav class=\"toc\" aria-label=\"On this page\"><details open><summary>On this page</summary><ol>\n\
<li><a href=\"#units\">Units</a></li>\n<li><a href=\"#functions\">Functions</a></li>\n<li><a href=\"#weightings\">Weightings</a></li>\n<li><a href=\"#dollar\"><code>$</code> names</a></li>\n<li><a href=\"#heights\">The library&rsquo;s heights</a></li>\n<li><a href=\"#seams\">Seam words</a></li>\n<li><a href=\"#numbers\">Numbers</a></li>\n</ol>\n\
<p class=\"toc-h\">The guide</p>\n<ol class=\"guide-pages\">\n<li><a href=\"../language.html\">The map</a></li>\n"
    );
    for stem in order {
        let _ = writeln!(s, "<li><a href=\"{stem}.html\">{}</a></li>", titles[stem]);
    }
    s.push_str("<li><a href=\"cheat-sheet.html\" aria-current=\"page\">Cheat sheet</a></li>\n</ol>\n</details></nav>\n</div>\n<article class=\"doc guide cheat\">\n");

    // units
    s.push_str("<section>\n<h2 id=\"units\">Units</h2>\n<div class=\"tbl\"><table>\n<thead><tr><th>Unit</th><th>Is</th></tr></thead>\n<tbody>\n");
    for u in UNITS {
        let about =
            dt_lang::unit_about(u, 35.0, 25.4).ok_or_else(|| format!("unit {u} has no words"))?;
        let _ = writeln!(
            s,
            "<tr><td><code>{u}</code></td><td>{}</td></tr>",
            esc(&about)
        );
    }
    s.push_str("<tr><td><code>%</code></td><td>a plain fraction: <code>50%</code> is 0.5 (a length only inside a point)</td></tr>\n</tbody></table></div>\n");
    s.push_str(&more("units"));
    s.push_str("</section>\n");

    // functions
    s.push_str("<section>\n<h2 id=\"functions\">Functions</h2>\n<div class=\"tbl\"><table>\n<thead><tr><th>Word</th><th>Takes</th><th>Kind</th></tr></thead>\n<tbody>\n");
    for (n, a) in FUNCTIONS {
        let _ = writeln!(
            s,
            "<tr><td><code>{n}</code></td><td>{a}</td><td>of numbers</td></tr>"
        );
    }
    for (n, a) in COORD_CALLS {
        let _ = writeln!(
            s,
            "<tr><td><code>{n}</code></td><td>{a}</td><td>of a region and a unit, or a frame and two numbers</td></tr>"
        );
    }
    let _ = writeln!(
        s,
        "<tr><td><code>parent</code></td><td>1</td><td>the region that holds a region</td></tr>\n</tbody></table></div>\n<p class=\"muted small\">The coordinates are {}.</p>",
        COORDINATES
            .iter()
            .map(|c| format!("<code>{c}</code>"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    s.push_str(&more("squares"));
    s.push_str("</section>\n");

    // weightings
    s.push_str("<section>\n<h2 id=\"weightings\">Weightings</h2>\n<div class=\"tbl\"><table>\n<thead><tr><th>Word</th><th>Numbers after the range</th></tr></thead>\n<tbody>\n");
    for (n, a) in WEIGHTINGS {
        let takes = match (n, a) {
            ("mix", _) => "parts and their weights in pairs".to_owned(),
            (_, usize::MAX) => "the weight of each step".to_owned(),
            (_, a) => a.to_string(),
        };
        let _ = writeln!(s, "<tr><td><code>{n}</code></td><td>{takes}</td></tr>");
    }
    s.push_str("</tbody></table></div>\n");
    s.push_str(&more("weightings"));
    s.push_str("</section>\n");

    // $ names
    s.push_str("<section>\n<h2 id=\"dollar\"><code>$</code> names</h2>\n<p>What an element sets where it works a height out: a body, a tree, any other element.</p>\n<div class=\"tbl\"><table>\n<thead><tr><th>Name</th><th>Body</th><th>Tree</th><th>Other</th></tr></thead>\n<tbody>\n");
    let mark = |set: &[&str], n: &str| if set.contains(&n) { "sets" } else { "" };
    for n in POINT_NAMES.iter().chain(REGION_NAMES.iter()) {
        let _ = writeln!(
            s,
            "<tr><td><code>{n}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            mark(BODY_SETS, n),
            mark(TREE_SETS, n),
            mark(REGION_SETS, n)
        );
    }
    s.push_str("</tbody></table></div>\n");
    s.push_str(&more("names"));
    s.push_str("</section>\n");

    // library heights
    s.push_str("<section>\n<h2 id=\"heights\">The library&rsquo;s heights</h2>\n<div class=\"tbl\"><table>\n<thead><tr><th>Name</th><th>Is</th><th></th></tr></thead>\n<tbody>\n");
    for (n, v, a) in library_heights(root)? {
        let _ = writeln!(
            s,
            "<tr><td><code>{n}</code></td><td><code>{}</code></td><td>{}</td></tr>",
            esc(&v),
            esc(&a)
        );
    }
    s.push_str("</tbody></table></div>\n");
    s.push_str(&more("heights"));
    s.push_str("</section>\n");

    // seams
    s.push_str("<section>\n<h2 id=\"seams\">Seam words</h2>\n");
    if seams["ok"].as_bool() != Some(true) {
        return Err(format!("the edge contract's seams did not read: {seams}"));
    }
    s.push_str("<div class=\"tbl\"><table>\n<thead><tr><th>Along a square&rsquo;s edge</th><th>north</th><th>south</th><th>west</th><th>east</th></tr></thead>\n<tbody>\n");
    for row in seams["seams"]
        .as_array()
        .ok_or("no seams in the edge contract")?
    {
        let at = row["at"].as_f64().ok_or("a seam has no position")?;
        let (frac, what) = crate::docs::seam_fraction(at).ok_or_else(|| {
            format!("seam {at} has no fraction to show it by: add it to xtask/src/docs.rs")
        })?;
        let w = |k: &str| row[k].as_str().unwrap_or_default();
        let _ = writeln!(
            s,
            "<tr><td>{frac} ({what})</td><td><code>{}</code></td><td><code>{}</code></td><td><code>{}</code></td><td><code>{}</code></td></tr>",
            w("north"),
            w("south"),
            w("west"),
            w("east")
        );
    }
    let corners: Vec<String> = seams["corners"]
        .as_array()
        .ok_or("no corners in the edge contract")?
        .iter()
        .map(|c| {
            format!(
                "<code>{}</code> ({})",
                c["word"].as_str().unwrap_or_default(),
                c["corner"].as_str().unwrap_or_default()
            )
        })
        .collect();
    let _ = writeln!(
        s,
        "</tbody></table></div>\n<p>The tile&rsquo;s corners: {}. They take no number.</p>",
        corners.join(", ")
    );
    s.push_str(&more("lines"));
    s.push_str("</section>\n");

    // numbers
    let _ = write!(
        s,
        "<section>\n<h2 id=\"numbers\">Numbers</h2>\n<div class=\"tbl\"><table>\n<tbody>\n<tr><th>Land slope</th><td>{LAND_SLOPE_DEG}&deg;: how steeply dirt eases unless an entry says <code>slope=</code></td></tr>\n<tr><th>Water settle</th><td>{WATER_SETTLE_MM} mm: how far water settles over what is under it</td></tr>\n</tbody></table></div>\n</section>\n"
    );
    s.push_str("</article>\n</div>\n</body>\n</html>\n");
    Ok(s)
}
