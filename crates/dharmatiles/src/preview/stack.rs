//! The preview's stack line: each entry named by what it places, with a
//! link to that thing's page on the documentation site, and where it is
//! written (`file`, byte `span`, 1-based `lines`) so the editor can light
//! its text when its geometry is picked.

use dt_eval::doc::{Entry, Tile};
use serde_json::{Value, json};

/// The documentation site (GitHub Pages, served from `docs/` on `main`).
pub const DOCS: &str = "https://shawn-vincent.github.io/dharmatiles/";

/// The page (under [`DOCS`]) that documents an element: every built-in
/// element has its own page under `elements/`.
#[must_use]
pub fn page(element: &str) -> String {
    if dt_doc::schema::element(element).is_some() {
        format!("elements/{element}.html")
    } else {
        "elements/index.html".to_owned()
    }
}

fn plural(word: &str) -> String {
    if word.ends_with('s') {
        word.to_owned()
    } else {
        format!("{word}s")
    }
}

fn and_list(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn item(label: String, element: &str) -> Value {
    json!({ "label": label, "element": element, "href": format!("{DOCS}{}", page(element)) })
}

/// The tile's stack as `{label, element, href}` items, in written order,
/// ending with the base when the stack has a `base` line.
#[must_use]
pub fn items(tile: &Tile, system: &str) -> Vec<Value> {
    let mut out = Vec::with_capacity(tile.stack.len() + 1);
    for (i, e) in tile.stack.iter().enumerate() {
        out.push(entry(
            e,
            tile.stack[..i].iter().rev().find(|p| inside(e, p)),
        ));
    }
    if let Some(span) = tile.base_span {
        let mut v = base(system);
        v["what"] = json!("base");
        v["file"] = json!(tile.src.name());
        let text = tile.src.inner();
        let line = |o: usize| {
            text.get(..o.min(text.len()))
                .map_or(1, |t| t.matches('\n').count() + 1)
        };
        let (start, len) = (span.offset(), span.len());
        v["span"] = json!([start, len]);
        v["lines"] = json!([line(start), line(start + len.saturating_sub(1))]);
        out.push(v);
    }
    out
}

/// The base line's connector for the build target: its label and page.
#[must_use]
pub fn base(system: &str) -> Value {
    let (label, page) = if system == "ol" {
        ("OpenLOCK base", "openlock")
    } else {
        ("DungeonBlocks base", "dungeonblocks")
    };
    json!({
        "label": label, "element": "connector",
        "href": format!("{DOCS}elements/{page}.html"),
    })
}

/// True when `e` is written inside `outer` (a scatter in a surface,
/// lifted out into its own entry).
fn inside(e: &Entry, outer: &Entry) -> bool {
    let (a, b) = (e.span.offset(), outer.span.offset());
    std::sync::Arc::ptr_eq(&e.src, &outer.src) && a > b && a + e.span.len() <= b + outer.span.len()
}

fn entry(e: &Entry, within: Option<&Entry>) -> Value {
    let what = e.what.value.as_str();
    let (mut label, element) = if what == "scatter" && !e.children.is_empty() {
        let mut names: Vec<String> = Vec::new();
        for c in &e.children {
            let n = plural(&c.what.value);
            if !names.contains(&n) {
                names.push(n);
            }
        }
        // a mixed scatter links to its first thing's page
        (and_list(&names), e.children[0].what.value.as_str())
    } else {
        (what.to_owned(), what)
    };
    if let Some(outer) = within {
        label = format!("{label} in {}", outer.what.value);
    } else if let Some(against) = &e.against {
        label = format!("{label} at the {against}");
    } else if let Some(area) = &e.area {
        label = format!("{label} in {}", area.value);
    }
    let mut v = item(label, element);
    v["what"] = json!(what);
    v["file"] = json!(e.src.name());
    let (start, len) = (e.span.offset(), e.span.len());
    let text = e.src.inner();
    let line = |o: usize| {
        text.get(..o.min(text.len()))
            .map_or(1, |t| t.matches('\n').count() + 1)
    };
    v["span"] = json!([start, len]);
    v["lines"] = json!([line(start), line(start + len.saturating_sub(1))]);
    v["controls"] = json!(settings(&e.controls, &e.what.span, e.src.name()));
    // where it is put (at=, in=, along=), as written, for the map's handles
    const PLACE: [&str; 4] = ["at", "in", "along", "against"];
    v["place"] = json!(
        dt_doc::edit::props_at(text, e.what.span.offset())
            .unwrap_or_default()
            .into_iter()
            .filter(|(n, ..)| PLACE.contains(&n.as_str()))
            .map(|(name, text, span)| json!({ "name": name, "text": text, "span": span }))
            .collect::<Vec<_>>()
    );
    // a scatter's things, each with its own controls, for the lens; a
    // node is addressed by the offset its name starts at
    v["things"] = json!(
        e.children
            .iter()
            .map(|c| json!({
                "what": c.what.value,
                "at": c.what.span.offset(),
                "controls": settings(&c.controls, &c.what.span, e.src.name()),
            }))
            .collect::<Vec<_>>()
    );
    v
}

/// The controls written on an entry (a default is not written: its span
/// is the entry's name): name, the text written (`size="6..10"`, or a
/// child line `height 35`) and its byte span, and the file it is in (a
/// definition's controls are written in the definition's file).
fn settings(
    controls: &[dt_eval::doc::Setting],
    name: &miette::SourceSpan,
    file: &str,
) -> Vec<Value> {
    controls
        .iter()
        .filter(|c| c.span != *name)
        .map(|c| {
            let (o, l) = (c.span.offset(), c.span.len());
            let text = c.src.inner();
            json!({
                "name": c.name,
                "text": text.get(o..o + l).unwrap_or_default(),
                "span": [o, l],
                "file": (c.src.name() != file).then(|| c.src.name()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use dt_eval::{Files, Options};

    fn labels(path: &str, system: &str) -> Vec<String> {
        let c = dt_eval::check(&Files, path, &Options::default()).unwrap();
        super::items(c.tile.as_ref().unwrap(), system)
            .iter()
            .map(|v| v["label"].as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn the_cliff_names_what_each_entry_places() {
        assert_eq!(
            labels("lib:tiles/cliffs/2x1-cliff.dtile", "db"),
            [
                "dirt",
                "pebbles in dirt",
                "cliff in crag",
                "dirt in crag",
                "stones at the cliff",
                "boulders at the cliff",
                "DungeonBlocks base",
            ]
        );
    }

    #[test]
    fn every_link_is_on_the_docs_site() {
        let c = dt_eval::check(
            &Files,
            "lib:tiles/cliffs/2x1-cliff.dtile",
            &Options::default(),
        )
        .unwrap();
        for v in super::items(c.tile.as_ref().unwrap(), "ol") {
            assert!(v["href"].as_str().unwrap().starts_with(super::DOCS));
        }
    }
}
