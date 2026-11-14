//! `dharmatiles explain dt::CODE`: what an error code means, read from the local copy of its error page (the
//! `docs/e/<code>.html` that `cargo xtask docs` writes), so it works with no network and says what the installed
//! program's docs say.

use dt_lang::codes;

use crate::{ERROR, OK};

/// Print a code's summary, help and where its pages are.  `code` is `dt::type` or just `type`.
pub fn run(code: &str) -> u8 {
    let id = if code.starts_with("dt::") {
        code.to_owned()
    } else {
        format!("dt::{code}")
    };
    let Some(c) = codes::lookup(&id) else {
        tell!("error: `{code}` is not a code of this program");
        tell!(
            "the codes: {}",
            codes::ALL
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ERROR;
    };
    let page = match local_page(&id) {
        Ok(p) => p,
        Err(why) => {
            tell!("error: {why}");
            return ERROR;
        }
    };
    say!("{}", c.id);
    say!("{}", page.summary);
    say!("");
    say!("help: {}", page.help);
    say!("local page: {}", page.path.display());
    say!("online: {}", codes::explain_url(&id));
    OK
}

/// What is read from a code's local error page.
struct Page {
    path: std::path::PathBuf,
    summary: String,
    help: String,
}

/// The local error page of `id`, read and understood.
///
/// # Errors
/// There is no docs folder, no page for the code in it, or the page does not hold what a generated error page does.
fn local_page(id: &str) -> Result<Page, String> {
    let dir = dt_eval::source::docs_dir().ok_or_else(|| {
        "no docs folder found: set $DT_DOCS to one (the checkout's docs/), or install the program with its docs"
            .to_owned()
    })?;
    let path = dir.join("e").join(format!("{}.html", codes::slug(id)));
    let html = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "cannot read {}: {e} (`cargo xtask docs` writes the error pages)",
            path.display()
        )
    })?;
    let part = |class: &str| {
        text_of(&html, class).ok_or_else(|| {
            format!(
                "{} has no `{class}` part: it is stale or not generated",
                path.display()
            )
        })
    };
    let page = Page {
        summary: part("summary")?,
        help: part("help")?,
        // the folder may be written with `..` in it (DT_DOCS, or the program beside its docs): say where the file really is
        path: path
            .canonicalize()
            .map_err(|e| format!("cannot resolve {}: {e}", path.display()))?,
    };
    Ok(page)
}

/// The text of the first paragraph marked `data-part="part"` (as the generator writes an error page), tags removed
/// and entities read.
fn text_of(html: &str, part: &str) -> Option<String> {
    let at = html.find(&format!("data-part=\"{part}\""))?;
    let start = at + html[at..].find('>')? + 1;
    let end = start + html[start..].find("</p>")?;
    let mut out = String::new();
    let mut in_tag = false;
    for ch in html[start..end].chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    Some(
        out.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&"),
    )
}

#[cfg(test)]
mod tests {
    use super::text_of;

    #[test]
    fn the_summary_is_read_without_tags_or_entities() {
        let html = "<p data-part=\"summary\">A <code>&lt;x&gt;</code> &amp; more</p>";
        assert_eq!(text_of(html, "summary").as_deref(), Some("A <x> & more"));
        assert_eq!(text_of(html, "help"), None);
    }
}
