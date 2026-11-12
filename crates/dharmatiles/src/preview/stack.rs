//! The preview's stack line: each entry named by what it places, with a
//! link to that thing's page on the documentation site, and where it is
//! written (`file`, byte `span`, 1-based `lines`) so the editor can light
//! its text when its geometry is picked.

use dt_eval::doc::{Entry, Named, Tile};
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

fn item(label: String, element: &str) -> Value {
    json!({ "label": label, "element": element, "href": format!("{DOCS}{}", page(element)) })
}

/// The tile's stack as `{key, label, element, href, …}` items, in written
/// order, ending with the base when the stack has a `base` line.  `key` is
/// the entry's id ([`dt_eval::doc::Entry::id`]): what the page keeps to
/// find the entry again whatever the stack looks like later.
///
/// # Errors
/// If a span the plan holds does not lie in its own source (a bug: the
/// page would be sent empty text in its place).
pub fn items(tile: &Tile, system: &str) -> Result<Vec<Value>, String> {
    let names = tile.names(system);
    let mut out = Vec::with_capacity(names.len());
    for (e, named) in tile.stack.iter().zip(&names) {
        out.push(entry(e, named)?);
    }
    if let Some(span) = tile.base_span {
        let mut v = base(system);
        v["key"] = json!(names.last().map(|n| n.id.as_str()));
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
    Ok(out)
}

/// The base line's connector for the build target: its label and page.
#[must_use]
pub fn base(system: &str) -> Value {
    let (label, page) = dt_eval::doc::base_label(system);
    json!({
        "label": label, "element": "connector",
        "href": format!("{DOCS}elements/{page}.html"),
    })
}

fn entry(e: &Entry, named: &Named) -> Result<Value, String> {
    let what = e.what.value.as_str();
    let mut v = item(named.label.clone(), &named.element);
    v["key"] = json!(named.id.as_str());
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
    v["controls"] = json!(settings(&e.controls, &e.what.span, e.src.name())?);
    // where it is put (at=), as written, for the map's handles
    const PLACE: [&str; 1] = ["at"];
    let props = dt_doc::edit::props_at(text, e.what.span.offset()).map_err(|err| {
        format!(
            "cannot read the properties of the `{what}` entry at byte {} of {}: {err}",
            e.what.span.offset(),
            e.src.name()
        )
    })?;
    v["place"] = json!(
        props
            .into_iter()
            .filter(|(n, ..)| PLACE.contains(&n.as_str()))
            .map(|(name, text, span)| json!({ "name": name, "text": text, "span": span }))
            .collect::<Vec<_>>()
    );
    // where a fixed `at=` point is, in squares from the south-west corner,
    // as the program reads it (the page never reads a written point)
    v["at"] =
        json!(e.at.as_ref().and_then(|d| {
            (d[0].is_point() && d[1].is_point()).then(|| [d[0].lo(), d[1].lo()])
        }));
    // a scatter's things, each with its own controls, for the lens; a
    // node is addressed by the offset its name starts at
    let mut things = Vec::with_capacity(e.children.len());
    for c in &e.children {
        things.push(json!({
            "what": c.what.value,
            "at": c.what.span.offset(),
            "controls": settings(&c.controls, &c.what.span, e.src.name())?,
        }));
    }
    v["things"] = json!(things);
    Ok(v)
}

/// The controls written on an entry (a default is not written: its span
/// is the entry's name): name, the text written (`size="6dmm .. 10dmm"`, or a
/// child line `height 35`) and its byte span, and the file it is in (a
/// definition's controls are written in the definition's file).
fn settings(
    controls: &[dt_eval::doc::Setting],
    name: &miette::SourceSpan,
    file: &str,
) -> Result<Vec<Value>, String> {
    controls
        .iter()
        .filter(|c| c.span != *name)
        .map(|c| {
            let (o, l) = (c.span.offset(), c.span.len());
            let text = c.src.inner();
            let written = text.get(o..o + l).ok_or_else(|| {
                format!(
                    "the setting `{}` has a span ({o}, {l}) that is not in {}",
                    c.name,
                    c.src.name()
                )
            })?;
            Ok(json!({
                "name": c.name,
                "text": written,
                "span": [o, l],
                "file": (c.src.name() != file).then(|| c.src.name()),
            }))
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
            .unwrap()
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
    fn every_library_and_test_tile_describes_its_stack_without_a_bad_span() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library");
        let mut paths = Vec::new();
        for group in ["tiles", "tests"] {
            let mut dirs = vec![root.join(group)];
            while let Some(d) = dirs.pop() {
                for e in std::fs::read_dir(d).unwrap() {
                    let p = e.unwrap().path();
                    if p.is_dir() {
                        dirs.push(p);
                    } else if p.extension().is_some_and(|x| x == "dtile") {
                        let rel = p
                            .strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned();
                        paths.push(format!("lib:{rel}"));
                    }
                }
            }
        }
        assert!(paths.len() > 50, "found the library");
        for path in paths {
            // a tile with a mistake has no stack to describe
            if let Ok(c) = dt_eval::check(&Files, &path, &Options::default())
                && let Some(t) = c.tile.as_ref()
            {
                super::items(t, "db").unwrap_or_else(|e| panic!("{path}: {e}"));
            }
        }
    }

    #[test]
    fn every_link_is_on_the_docs_site() {
        let c = dt_eval::check(
            &Files,
            "lib:tiles/cliffs/2x1-cliff.dtile",
            &Options::default(),
        )
        .unwrap();
        for v in super::items(c.tile.as_ref().unwrap(), "ol").unwrap() {
            assert!(v["href"].as_str().unwrap().starts_with(super::DOCS));
        }
    }
}
