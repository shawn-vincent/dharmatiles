//! The preview's stack line: each entry named by what it places, with a
//! link to that thing's page on the documentation site.

use dt_eval::doc::{Entry, Tile};
use serde_json::{Value, json};

/// The documentation site (GitHub Pages, served from `docs/` on `main`).
pub const DOCS: &str = "https://shawn-vincent.github.io/dharmatiles/";

/// The page (under [`DOCS`]) that documents an element: every built-in
/// look has its own page under `elements/`.
#[must_use]
pub fn page(look: &str) -> String {
    if dt_doc::schema::look(look).is_some() {
        format!("elements/{look}.html")
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

fn item(label: String, look: &str) -> Value {
    json!({ "label": label, "look": look, "href": format!("{DOCS}{}", page(look)) })
}

/// The tile's stack as `{label, look, href}` items, in written order,
/// ending with the base the system adds.
#[must_use]
pub fn items(tile: &Tile, system: &str) -> Vec<Value> {
    let mut out = Vec::with_capacity(tile.stack.len() + 1);
    for (i, e) in tile.stack.iter().enumerate() {
        out.push(entry(
            e,
            tile.stack[..i].iter().rev().find(|p| inside(e, p)),
        ));
    }
    out.push(base(system));
    out
}

/// The base the evaluator adds to every tile.
#[must_use]
pub fn base(system: &str) -> Value {
    let (label, page) = if system == "ol" {
        ("OpenLOCK base", "openlock")
    } else {
        ("DungeonBlocks base", "dungeonblocks")
    };
    json!({
        "label": label, "look": "connector",
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
    let (mut label, look) = if what == "scatter" && !e.children.is_empty() {
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
    item(label, look)
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
                "cliff",
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
