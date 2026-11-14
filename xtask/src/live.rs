//! The site's live examples: what `cargo xtask docs` writes for them, and `cargo xtask examples`.
//!
//! An example is `<dt-example src="examples/lines/shore.dtile" controls="wobble" overlay="lines points" highlight="line shore">`
//! on a page, naming a `.dtile` under `docs/examples/` (the file the tests build).  So that the page is complete with no script,
//! the generator writes into the element, between markers, the poster's picture and the file's source as real text:
//!
//! ```html
//! <dt-example src="examples/lines/shore.dtile">
//! <!-- gen:example -->…replaced on every run…<!-- /gen:example -->
//! <figcaption>…the author's words…</figcaption>
//! </dt-example>
//! ```
//!
//! The picture is `posters/KEY.jpg`, KEY the build's cache key (`dharmatiles poster`), so an example that changed has a new key
//! and the old poster is simply not found.  Posters are not committed (the site's `posters` job makes them, `cargo xtask
//! examples` makes them here); the markup naming them is, and `docs --check` fails when it is stale.
//!
//! It also writes, on every page that has a card, the head block that loads the card element and the import map its modules
//! use.  The editor's own modules and vendored libraries are not copied: the cards import them at `<site>/editor/js` and `<site>/editor/vendor`,
//! which each place that serves the site provides from `web/preview` (the deploy job, the program's docs route).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const OPEN: &str = "<!-- gen:example -->";
const CLOSE: &str = "<!-- /gen:example -->";
const HEAD_OPEN: &str = "<!-- gen:live-head -->";
const HEAD_CLOSE: &str = "<!-- /gen:live-head -->";

/// The vendored libraries a card's modules name, as the import map says them: (specifier, file in `web/preview/vendor`, served at `<site>/editor/vendor`).
const VENDOR: [(&str, &str); 6] = [
    ("preact", "preact.mjs"),
    ("preact/hooks", "preact-hooks.mjs"),
    ("htm", "htm.mjs"),
    ("@preact/signals", "preact-signals.mjs"),
    ("@preact/signals-core", "preact-signals-core.mjs"),
    ("three", "three.module.min.js"),
];

fn read(p: &Path) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))
}

fn files_under(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            files_under(&p, ext, out)?;
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
    out.sort();
    Ok(())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The value of attribute `name` in the text of one tag.
fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!(" {name}=\"");
    let at = tag.find(&needle)? + needle.len();
    Some(tag[at..at + tag[at..].find('"')?].to_owned())
}

/// `../` as many times as `page` is deep under `docs/`, or `./`.
fn up(page: &Path, docs: &Path) -> String {
    let depth = page
        .strip_prefix(docs)
        .map_or(0, |p| p.components().count().saturating_sub(1));
    if depth == 0 {
        "./".to_owned()
    } else {
        "../".repeat(depth)
    }
}

/// One card found on a page.
struct Card {
    src: String,
    expect: Option<String>,
}

/// Every key of the examples under `docs/examples/`: file (from the repo root) to the poster key, or `error:CODE`.
fn keys(root: &Path, files: &[String]) -> Result<BTreeMap<String, String>, String> {
    if files.is_empty() {
        return Ok(BTreeMap::new());
    }
    let bin = crate::program(root, true)?;
    let out = Command::new(&bin)
        .current_dir(root)
        .env("DT_CACHE", "off")
        .arg("poster")
        .args(files)
        .output()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    if !out.status.success() {
        return Err(format!(
            "`dharmatiles poster` failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(f, k)| (f.to_owned(), k.to_owned()))
        .collect())
}

/// The markup of one card: its poster (or, for an example that must fail, a plain note) and its source as text.
fn block(root: &Path, card: &Card, key: &str, rel: &str) -> Result<String, String> {
    let file = root.join("docs").join(&card.src);
    let source = read(&file)?;
    let size = source
        .split_once("size=\"")
        .and_then(|(_, r)| r.split_once('"'))
        .and_then(|(s, _)| s.split_once('x'))
        .and_then(|(c, r)| Some((c.parse::<u32>().ok()?, r.parse::<u32>().ok()?)))
        .ok_or_else(|| format!("{}: no `size=\"CxR\"` in the tile", card.src))?;
    let ratio = format!("aspect-ratio: {} / {}", size.0, size.1);
    let view = match (&card.expect, key.strip_prefix("error:")) {
        (Some(code), Some(got)) if code == got => format!(
            "<figure class=\"dtx-view dtx-nothing\" style=\"{ratio}\"><div class=\"dtx-nobuild\"><b>Nothing builds</b><span>this source gives <a class=\"codelink\" href=\"{rel}e/{}.html\"><code>{}</code></a></span></div></figure>",
            // written as `codes.rs`'s linking writes a mention, so the two generators leave each other's output alone
            esc(code.strip_prefix("dt::").unwrap_or(code)),
            esc(code)
        ),
        (Some(code), got) => {
            return Err(format!(
                "{}: the card says it gives {code}, but the program says {}",
                card.src,
                got.unwrap_or("it builds")
            ));
        }
        (None, Some(code)) => {
            return Err(format!(
                "{}: the example gives {code}: add expect-error=\"{code}\" to its card, or mend it",
                card.src
            ));
        }
        (None, None) => format!(
            "<figure class=\"dtx-view\" style=\"{ratio}\"><img class=\"dtx-poster\" src=\"{rel}posters/{key}.jpg\" alt=\"The tile this source builds, from above\" width=\"800\" height=\"{}\" loading=\"lazy\"></figure>",
            800 * size.1 / size.0
        ),
    };
    // the source coloured as every code block of the site is (`highlight.rs`), so the two generators leave each other's output alone
    Ok(crate::highlight::pre_blocks(&format!(
        "{OPEN}\n<div class=\"dtx-grid\">{view}<pre class=\"dtx-code\"><code>{}</code></pre></div>\n{CLOSE}",
        esc(&source)
    )))
}

/// A page with its cards' markup written; `cards` collects what the page holds.
fn fill_page(
    root: &Path,
    page: &Path,
    text: &str,
    keys: &BTreeMap<String, String>,
    cards: &mut Vec<Card>,
) -> Result<String, String> {
    let docs = root.join("docs");
    let rel = up(page, &docs);
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("<dt-example") {
        let tail = &rest[at..];
        let end = tail
            .find('>')
            .ok_or_else(|| format!("{}: a <dt-example> tag never ends", page.display()))?;
        let tag = &tail[..end];
        let src = attr(tag, "src")
            .ok_or_else(|| format!("{}: a <dt-example> has no src", page.display()))?;
        let card = Card {
            src: src.clone(),
            expect: attr(tag, "expect-error"),
        };
        let file = format!("docs/{src}");
        let key = keys
            .get(&file)
            .ok_or_else(|| format!("{}: no poster key for {file}", page.display()))?;
        let new = block(root, &card, key, &rel)?;
        out.push_str(&rest[..at + end + 1]);
        let after = &rest[at + end + 1..];
        let trimmed = after.trim_start();
        let skip = if trimmed.starts_with(OPEN) {
            let c = trimmed
                .find(CLOSE)
                .ok_or_else(|| format!("{}: {OPEN} is never closed", page.display()))?;
            after.len() - trimmed.len() + c + CLOSE.len()
        } else {
            0
        };
        out.push('\n');
        out.push_str(&new);
        rest = &after[skip..];
        cards.push(card);
    }
    out.push_str(rest);
    head_block(&out, &rel, !cards.is_empty())
}

/// The head block of a page: present with a card on it, absent without.
fn head_block(text: &str, rel: &str, wanted: bool) -> Result<String, String> {
    let mut text = text.to_owned();
    if let Some(a) = text.find(HEAD_OPEN) {
        let b = text[a..]
            .find(HEAD_CLOSE)
            .ok_or("a gen:live-head block is never closed")?
            + a
            + HEAD_CLOSE.len();
        let eat = usize::from(text[b..].starts_with('\n'));
        text.replace_range(a..b + eat, "");
    }
    if !wanted {
        return Ok(text);
    }
    let map: Vec<String> = VENDOR
        .iter()
        .map(|(spec, file)| format!("\"{spec}\":\"{rel}editor/vendor/{file}\""))
        .collect();
    let block = format!(
        "{HEAD_OPEN}\n<script type=\"importmap\">{{\"imports\":{{{}}}}}</script>\n<script type=\"module\" src=\"{rel}assets/live/dt-example.js\"></script>\n{HEAD_CLOSE}\n",
        map.join(",")
    );
    let at = text
        .find("</head>")
        .ok_or("a page with a card has no </head>")?;
    text.insert_str(at, &block);
    Ok(text)
}

/// `cargo xtask docs`'s part: the cards' markup, the head blocks.
pub fn run(root: &Path, check: bool) -> Result<(), String> {
    let docs = root.join("docs");
    let mut pages = Vec::new();
    files_under(&docs, "html", &mut pages)?;
    // every card of every page, to ask the program for the keys once
    let mut srcs = BTreeSet::new();
    for page in &pages {
        let text = read(page)?;
        let mut rest = text.as_str();
        while let Some(at) = rest.find("<dt-example") {
            let tail = &rest[at..];
            let end = tail.find('>').unwrap_or(tail.len());
            if let Some(src) = attr(&tail[..end], "src") {
                srcs.insert(format!("docs/{src}"));
            }
            rest = &tail[end..];
        }
    }
    let keys = keys(root, &srcs.into_iter().collect::<Vec<_>>())?;
    let mut stale = Vec::new();
    for page in &pages {
        let text = read(page)?;
        let mut cards = Vec::new();
        let new = fill_page(root, page, &text, &keys, &mut cards)?;
        if new != text {
            if check {
                stale.push(page.display().to_string());
            } else {
                std::fs::write(page, new).map_err(|e| e.to_string())?;
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

/// `cargo xtask examples [--out DIR]`: the poster of every example, `DIR/KEY.jpg` (docs/posters by default), each made once:
/// a poster that is there is kept, for its name is its build's key.  The tile's top view cropped to its footprint, so the page
/// can lay the overlay over it with no more than the tile's size.
pub fn posters(root: &Path, args: &[String]) -> Result<(), String> {
    let out = match args {
        [] => root.join("docs/posters"),
        [flag, dir] if flag == "--out" => PathBuf::from(dir),
        _ => return Err("usage: cargo xtask examples [--out DIR]".to_owned()),
    };
    let mut files = Vec::new();
    files_under(&root.join("docs/examples"), "dtile", &mut files)?;
    let files: Vec<String> = files
        .iter()
        .filter_map(|f| f.strip_prefix(root).ok())
        .map(|f| f.to_string_lossy().into_owned())
        .collect();
    let tmp = root.join("out/posters-png");
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let bin = crate::program(root, true)?;
    // only the tiles that have no picture yet are built: the key is in the file's name
    let keyed = keys(root, &files)?;
    let todo: Vec<&String> = keyed
        .iter()
        .filter(|(_, k)| {
            !k.starts_with("error:")
                && !(out.join(format!("{k}.jpg")).is_file()
                    && out.join(format!("{k}.map.json")).is_file())
        })
        .map(|(f, _)| f)
        .collect();
    if todo.is_empty() {
        println!("examples: every poster is there ({} examples)", keyed.len());
        return Ok(());
    }
    crate::run(
        Command::new(&bin)
            .current_dir(root)
            .env("DT_CACHE", "off")
            .arg("poster")
            .args(&todo)
            .arg("--out")
            .arg(&tmp)
            .stdout(std::process::Stdio::null()),
    )?;
    for (f, k) in &keyed {
        if k.starts_with("error:") {
            continue;
        }
        // the map beside the poster, by the same key
        let map = out.join(format!("{k}.map.json"));
        if !map.is_file() {
            std::fs::copy(tmp.join(format!("{k}.map.json")), &map)
                .map_err(|e| format!("{f}: no map was made: {e}"))?;
        }
        let jpg = out.join(format!("{k}.jpg"));
        if jpg.is_file() {
            continue;
        }
        let png = tmp.join(format!("{k}.png"));
        let im = crate::image::Rgb::read_png(&png)?;
        let (x0, y0, x1, y1) = im
            .footprint()
            .ok_or_else(|| format!("{f}: no tile in its top view"))?;
        im.crop(x0, y0, x1, y1)
            .fit_width(800)
            .write_jpeg(&jpg, 84)?;
        println!("examples: {}", jpg.display());
    }
    Ok(())
}
