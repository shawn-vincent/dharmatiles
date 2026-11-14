//! The docs' side of the diagnostic codes (`dt_lang::codes`): the join of two facts, each written once.
//!
//! That a code exists is the registry's.  Where it is explained is the page's: a `data-explains` marker naming it
//! on the section (a heading, a table cell, a list item) that explains it.  This module derives from the two:
//!
//! * the `#dt-type` anchor, written as an empty span at the start of the marked element;
//! * the error page `docs/e/type.html` for every code: what happened, the message as the program prints it (the
//!   program run on the registry's broken document), what to do, the rule quoted from its section and linked at its
//!   anchor, the broken document beside its fix with the change marked, and the codes the same section explains;
//! * the table of every code on `docs/errors.html`.
//!
//! and checks that nothing dangles: a code with no explaining section (or two), a marker for a code that does not
//! exist, an error page or anchor that is missing or stale.  `cargo xtask docs` writes them; `--check` fails.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use dt_lang::codes::{self, Code, Kind};

const MARK: &str = "data-explains=\"";
const ANCHORS_OPEN: &str = "<!--gen:anchors-->";
const ANCHORS_CLOSE: &str = "<!--/gen:anchors-->";
const TABLE_OPEN: &str = "<!-- gen:codes -->";
const TABLE_CLOSE: &str = "<!-- /gen:codes -->";

/// Where a code is explained: a page (from `docs/`) and the words to link it by.
struct Rule {
    page: String,
    /// The page's title.
    title: String,
    /// The marked section's heading, when the marked element is one.
    section: Option<String>,
    /// The rule in the section's own words: the first paragraph or list under a marked heading, or the marked
    /// element's text; HTML with only paragraphs, lists and `<code>` kept.
    quote: Option<String>,
    /// The marked element, the same for every code one marker (or one section) explains.
    place: String,
}

/// Write the anchors and error pages, or with `check` change nothing and fail if any is missing or stale.
/// `bin` is the program, run on each broken example for the message the page shows.
pub fn run(root: &Path, check: bool, bin: &Path) -> Result<(), String> {
    let docs = root.join("docs");
    let mut files: BTreeMap<PathBuf, String> = BTreeMap::new(); // every file this module owns, as it should be
    let mut rules: BTreeMap<String, Vec<Rule>> = BTreeMap::new();
    let mut problems = Vec::new();

    for page in html_pages(&docs)? {
        let rel = page
            .strip_prefix(&docs)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with("e/") {
            continue;
        }
        let text =
            std::fs::read_to_string(&page).map_err(|e| format!("{}: {e}", page.display()))?;
        let (text, found) = anchor(&rel, &text, &mut problems);
        for (code, rule) in found {
            rules.entry(code).or_default().push(rule);
        }
        files.insert(page, text);
    }

    for (code, rs) in &rules {
        if codes::lookup(code).is_none() {
            problems.push(format!(
                "a marker names {code}, which is not in the registry (dt-lang/src/codes.rs): {}",
                rs.iter()
                    .map(|r| r.page.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if rs.len() > 1 {
            problems.push(format!(
                "{code} is explained in {} places ({}): one section explains a code",
                rs.len(),
                rs.iter()
                    .map(|r| r.page.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    for c in codes::ALL {
        if !rules.contains_key(c.id) {
            problems.push(format!(
                "{} has no explaining section: put data-explains=\"{}\" on the section of a docs page that explains it",
                c.id, c.id
            ));
        }
    }

    // the error pages
    let e_dir = docs.join("e");
    let scratch = root.join("target/xtask-codes");
    std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let nav = nav(&docs)?;
    for c in codes::ALL {
        if let Some(rule) = rules.get(c.id).and_then(|r| r.first()) {
            // the codes the same section explains, in the registry's order
            let related: Vec<&str> = codes::ALL
                .iter()
                .filter(|o| {
                    o.id != c.id
                        && rules
                            .get(o.id)
                            .and_then(|r| r.first())
                            .is_some_and(|r| r.place == rule.place)
                })
                .map(|o| o.id)
                .collect();
            let said = match c.example {
                Some(ex) => Some(message(bin, &scratch, c.id, ex.broken)?),
                None => None,
            };
            files.insert(
                e_dir.join(format!("{}.html", codes::slug(c.id))),
                error_page(&nav, c, rule, said.as_deref(), &related),
            );
        }
    }
    // the table of every code
    let index = docs.join("errors.html");
    match files.get_mut(&index) {
        Some(text) => *text = fill_table(text)?,
        None => problems.push("docs/errors.html is missing".to_owned()),
    }

    // an error page for a code the registry no longer has is stale too
    if e_dir.is_dir() {
        for p in html_pages(&e_dir)? {
            if !files.contains_key(&p) {
                if check {
                    problems.push(format!(
                        "{} is an error page for no code (run `cargo xtask docs`)",
                        p.display()
                    ));
                } else {
                    std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                    println!("docs: removed {}", p.display());
                }
            }
        }
    }

    let mut stale = Vec::new();
    for (path, text) in &files {
        let now = std::fs::read_to_string(path).ok();
        if now.as_deref() != Some(text.as_str()) {
            if check {
                stale.push(path.clone());
            } else {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                }
                std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
                println!("docs: wrote {}", path.display());
            }
        }
    }
    if !stale.is_empty() {
        let shown: Vec<String> = stale
            .iter()
            .take(8)
            .map(|p| p.strip_prefix(&docs).unwrap_or(p).display().to_string())
            .collect();
        let more = stale.len().saturating_sub(shown.len());
        problems.push(format!(
            "stale (run `cargo xtask docs`): {}{}",
            shown.join(", "),
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// Every `.html` file under `dir`, sorted.
fn html_pages(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if p.is_dir() {
            out.extend(html_pages(&p)?);
        } else if p.extension().is_some_and(|x| x == "html") {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

/// `text` with the code anchors of every marker written (replacing the ones there), and what each marker explains.
fn anchor(rel: &str, text: &str, problems: &mut Vec<String>) -> (String, Vec<(String, Rule)>) {
    let title = tag_text(text, "<title>", "</title>").unwrap_or_else(|| rel.to_owned());
    let mut out = String::with_capacity(text.len() + 256);
    let mut found = Vec::new();
    let mut pos = 0;
    while let Some(m) = text[pos..].find(MARK) {
        let at = pos + m;
        let Some(tag_start) = text[..at].rfind('<') else {
            problems.push(format!("{rel}: a marker outside a tag"));
            break;
        };
        let Some(tag_len) = text[at..].find('>') else {
            problems.push(format!("{rel}: a marker in a tag that does not close"));
            break;
        };
        let tag_end = at + tag_len + 1;
        let value_start = at + MARK.len();
        let Some(vlen) = text[value_start..].find('"') else {
            problems.push(format!("{rel}: a marker with no closing quote"));
            break;
        };
        let ids: Vec<&str> = text[value_start..value_start + vlen]
            .split_whitespace()
            .collect();
        let tag = &text[tag_start..tag_end];
        let section = ["h2", "h3", "h4"]
            .iter()
            .find(|h| tag.starts_with(&format!("<{h} ")) || tag.starts_with(&format!("<{h}>")))
            .and_then(|h| tag_text(&text[tag_start..], &format!("<{h}"), &format!("</{h}>")))
            .map(|t| plain(&t));
        let quote = quote(text, tag, tag_end, section.is_some());
        for id in &ids {
            found.push((
                (*id).to_owned(),
                Rule {
                    page: rel.to_owned(),
                    title: title.clone(),
                    section: section.clone(),
                    quote: quote.clone(),
                    place: format!("{rel}@{tag_start}"),
                },
            ));
        }
        out.push_str(&text[pos..tag_end]);
        out.push_str(ANCHORS_OPEN);
        for id in &ids {
            let _ = write!(out, "<span id=\"{}\"></span>", codes::anchor(id));
        }
        out.push_str(ANCHORS_CLOSE);
        pos = tag_end;
        if text[pos..].starts_with(ANCHORS_OPEN) {
            match text[pos..].find(ANCHORS_CLOSE) {
                Some(i) => pos += i + ANCHORS_CLOSE.len(),
                None => {
                    problems.push(format!("{rel}: an anchors block that does not close"));
                    break;
                }
            }
        }
    }
    out.push_str(&text[pos..]);
    (out, found)
}

/// The rule in its section's words: under a marked heading its first paragraph or list (before the next heading);
/// on any other marked element (a table cell, a list item) that element's own text, as a paragraph.
fn quote(text: &str, tag: &str, tag_end: usize, heading: bool) -> Option<String> {
    let rest = &text[tag_end..];
    let html = if heading {
        let end = rest.find("<h").unwrap_or(rest.len());
        let (at, name) = ["<p", "<ul", "<ol"]
            .iter()
            .filter_map(|o| {
                rest[..end]
                    .match_indices(o)
                    .find(|(i, _)| rest[i + o.len()..].starts_with(['>', ' ']))
                    .map(|(i, _)| (i, &o[1..]))
            })
            .min_by_key(|(i, _)| *i)?;
        let close = format!("</{name}>");
        let stop = at + rest[at..].find(&close)? + close.len();
        rest[at..stop].to_owned()
    } else {
        let name: String = tag[1..]
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        format!("<p>{}</p>", &rest[..rest.find(&format!("</{name}>"))?])
    };
    let q = kept(&html);
    (q.len() > "<p></p>".len()).then_some(q)
}

/// HTML with every tag but paragraphs, lists and `<code>` removed (links, emphasis, generated anchors) and those
/// kept bare of attributes, whitespace squeezed; entities are kept as written.
fn kept(html: &str) -> String {
    const KEEP: [&str; 5] = ["p", "ul", "ol", "li", "code"];
    let mut out = String::new();
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let Some(gt) = rest[lt..].find('>') else {
            rest = "";
            break;
        };
        let tag = &rest[lt + 1..lt + gt];
        let (close, body) = tag.strip_prefix('/').map_or((false, tag), |b| (true, b));
        let name: String = body
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if KEEP.contains(&name.as_str()) {
            out.push_str(&format!("<{}{name}>", if close { "/" } else { "" }));
        }
        rest = &rest[lt + gt + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What the program prints for `broken`: `dharmatiles check example.dtile`, run where the file is, so the message
/// names it as a reader's own would.  The program must refuse it; a clean read is an error here.
fn message(bin: &Path, scratch: &Path, id: &str, broken: &str) -> Result<String, String> {
    let file = scratch.join("example.dtile");
    std::fs::write(&file, broken).map_err(|e| format!("{}: {e}", file.display()))?;
    let out = Command::new(bin)
        .current_dir(scratch)
        .args(["check", "example.dtile"])
        .env("NO_COLOR", "1")
        .env_remove("CLICOLOR_FORCE")
        .env("COLUMNS", "80")
        .output()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    if out.status.success() {
        return Err(format!(
            "{id}: the program read the broken example without an error"
        ));
    }
    let mut said = String::from_utf8_lossy(&out.stderr).into_owned();
    said.push_str(&String::from_utf8_lossy(&out.stdout));
    if !said.contains(id) {
        return Err(format!(
            "{id}: the program's message for the broken example does not name it:\n{said}"
        ));
    }
    Ok(said
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches('\n')
        .to_owned())
}

/// `broken` and `fixed` as HTML, each with what the other does not have marked: a line changed in place has only
/// its changed middle marked; a line with no partner is marked whole.
fn marked(broken: &str, fixed: &str) -> (String, String) {
    let a: Vec<&str> = broken.trim_end().lines().collect();
    let b: Vec<&str> = fixed.trim_end().lines().collect();
    let mut out_a = Vec::new();
    let mut out_b = Vec::new();
    if a.len() == b.len() {
        for (x, y) in a.iter().zip(&b) {
            let (mx, my) = mark_line(x, y);
            out_a.push(mx);
            out_b.push(my);
        }
    } else {
        for x in &a {
            out_a.push(if b.contains(x) {
                escape(x)
            } else {
                format!("<mark class=\"del\">{}</mark>", escape(x))
            });
        }
        for y in &b {
            out_b.push(if a.contains(y) {
                escape(y)
            } else {
                format!("<mark class=\"ins\">{}</mark>", escape(y))
            });
        }
    }
    (out_a.join("\n"), out_b.join("\n"))
}

/// Two versions of one line, each with its changed middle marked (the common start and end left plain).
fn mark_line(x: &str, y: &str) -> (String, String) {
    if x == y {
        return (escape(x), escape(y));
    }
    let xc: Vec<char> = x.chars().collect();
    let yc: Vec<char> = y.chars().collect();
    let mut pre = 0;
    while pre < xc.len() && pre < yc.len() && xc[pre] == yc[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < xc.len() - pre
        && suf < yc.len() - pre
        && xc[xc.len() - 1 - suf] == yc[yc.len() - 1 - suf]
    {
        suf += 1;
    }
    let part = |c: &[char], class: &str| {
        let s: String = c[pre..c.len() - suf].iter().collect();
        let head: String = c[..pre].iter().collect();
        let tail: String = c[c.len() - suf..].iter().collect();
        if s.is_empty() {
            format!(
                "{}<mark class=\"{class} at\"></mark>{}",
                escape(&head),
                escape(&tail)
            )
        } else {
            format!(
                "{}<mark class=\"{class}\">{}</mark>{}",
                escape(&head),
                escape(&s),
                escape(&tail)
            )
        }
    };
    (part(&xc, "del"), part(&yc, "ins"))
}

/// The text between `open` (the start of a tag, whatever its attributes) and `close`, tags and all.
fn tag_text(text: &str, open: &str, close: &str) -> Option<String> {
    let s = text.find(open)?;
    let body = s + text[s..].find('>')? + 1;
    let e = body + text[body..].find(close)?;
    Some(text[body..e].to_owned())
}

/// `html` as plain text: tags removed (generated anchors included), whitespace squeezed.
fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Text made safe for HTML, with each `code` in backticks set as code.
fn inline(s: &str) -> String {
    let mut out = String::new();
    for (i, part) in s.split('`').enumerate() {
        let esc = part
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");
        if i % 2 == 1 {
            let _ = write!(out, "<code>{esc}</code>");
        } else {
            out.push_str(&esc);
        }
    }
    out
}

/// Text made safe for HTML, nothing else.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn kind_words(k: Kind) -> &'static str {
    match k {
        Kind::Mistake => "a mistake in the document",
        Kind::Note => "a note: nothing is refused",
        Kind::Failure => "the program failed, not the document",
    }
}

fn kind_class(k: Kind) -> &'static str {
    match k {
        Kind::Mistake => "mistake",
        Kind::Note => "note",
        Kind::Failure => "failure",
    }
}

/// The site's top bar, as the element pages (one level down) write it, with no page marked current.
fn nav(docs: &Path) -> Result<String, String> {
    let index = docs.join("elements/index.html");
    let text = std::fs::read_to_string(&index).map_err(|e| format!("{}: {e}", index.display()))?;
    let line = text
        .lines()
        .find(|l| l.starts_with("<nav class=\"top\">"))
        .ok_or_else(|| format!("{} has no top bar", index.display()))?;
    Ok(line.replace(" aria-current=\"page\"", ""))
}

/// A terminal's lower-case advice as a sentence: its first letter capitalised, a full stop at the end.
fn sentence(s: &str) -> String {
    let mut chars = s.chars();
    let mut out: String = chars
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    out.push_str(chars.as_str());
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// The error page of one code: the code and what happened; the message as printed; what to do beside the rule;
/// the broken document beside its fix; and the codes the same section explains.
fn error_page(nav: &str, c: &Code, rule: &Rule, said: Option<&str>, related: &[&str]) -> String {
    let id = c.id;
    let kind = kind_class(c.kind);
    let mut s = String::new();
    let _ = write!(
        s,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\n\
<title>{id}</title>\n<link rel=\"stylesheet\" href=\"../assets/site.css\">\n\
<script src=\"../assets/site.js\"></script>\n</head>\n<body>\n{nav}\n\
<main class=\"wrap codepage {kind}\">\n<header class=\"hero\">\n  <div class=\"eyebrow\"><a href=\"../errors.html\">Error code</a> · {}</div>\n  <h1>{id}</h1>\n  \
<p class=\"lede\" data-part=\"summary\">{}</p>\n</header>\n",
        kind_words(c.kind),
        inline(&sentence(c.summary))
    );
    if let Some(said) = said {
        let _ = write!(
            s,
            "<section class=\"said\" aria-labelledby=\"said\">\n<h2 id=\"said\">What the program says</h2>\n\
<pre class=\"said no-hl\"><code>{}</code></pre>\n</section>\n",
            escape(said)
        );
    }
    let link = format!("../{}#{}", rule.page, codes::anchor(id));
    let place = match &rule.section {
        Some(sec) => format!("{} · {}", escape(&rule.title), escape(sec)),
        None => escape(&rule.title),
    };
    // what to do and the example on the left, the rule (often the longer) on the right
    let _ = write!(
        s,
        "<div class=\"cp-grid\">\n<div>\n<section>\n<h2 id=\"help\">What to do</h2>\n<p data-part=\"help\">{}</p>\n</section>\n",
        inline(&sentence(c.help))
    );
    if let Some(ex) = c.example {
        let (broken, fixed) = marked(ex.broken, ex.fixed);
        let _ = write!(
            s,
            "<section>\n<h2 id=\"example\">Broken, then fixed</h2>\n<div class=\"cp-pair\">\n\
<figure class=\"broken\"><figcaption>Gives <code>{id}</code></figcaption>\n<pre><code>{broken}</code></pre></figure>\n\
<figure class=\"fixed\"><figcaption>Mended, it reads clean</figcaption>\n<pre><code>{fixed}</code></pre></figure>\n</div>\n</section>\n"
        );
    }
    let _ = write!(
        s,
        "</div>\n<section>\n<h2 id=\"rule\">The rule</h2>\n{}<p class=\"cp-in\"><a href=\"{link}\">Read it in context: {place}</a></p>\n</section>\n</div>\n",
        rule.quote
            .as_ref()
            .map(|q| format!("<blockquote class=\"rule\">{q}</blockquote>\n"))
            .unwrap_or_default()
    );
    s.push_str("<footer class=\"cp-foot\">\n");
    if !related.is_empty() {
        let _ = writeln!(
            s,
            "<span>Explained with it: {}</span>",
            related
                .iter()
                .map(|r| format!("<a href=\"{}.html\"><code>{r}</code></a>", codes::slug(r)))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    let _ = write!(
        s,
        "<span><a href=\"../errors.html\">Every error code</a></span>\n\
<span>Offline: <code>dharmatiles explain {id}</code></span>\n</footer>\n</main>\n</body>\n</html>\n"
    );
    s
}

/// `errors.html` with the table of every code written between its markers.
fn fill_table(text: &str) -> Result<String, String> {
    let (Some(a), Some(b)) = (text.find(TABLE_OPEN), text.find(TABLE_CLOSE)) else {
        return Err(format!(
            "docs/errors.html has no {TABLE_OPEN} … {TABLE_CLOSE} block"
        ));
    };
    let mut t = String::from(
        "\n<div class=\"tbl\"><table>\n<thead><tr><th>Code</th><th>Whose</th><th>What happened</th></tr></thead>\n<tbody>\n",
    );
    for c in codes::ALL {
        let _ = writeln!(
            t,
            "<tr><td><a href=\"e/{}.html\"><code>{}</code></a></td><td>{}</td><td>{}</td></tr>",
            codes::slug(c.id),
            c.id,
            kind_words(c.kind),
            inline(c.summary)
        );
    }
    t.push_str("</tbody></table></div>\n");
    Ok(format!("{}{TABLE_OPEN}{t}{}", &text[..a], &text[b..]))
}
