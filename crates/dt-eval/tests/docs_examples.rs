//! The site's examples are files the tests build.  Every `<dt-example src="examples/…">` card in `docs/` names a `.dtile` under
//! `docs/examples/`, which the page reads for its source and this test reads to build it: an example cannot drift from the
//! language.  A card with `expect-error="dt::code"` must fail with exactly that code; every other card must build.  Every file
//! under `docs/examples/` must be used by a card, and none is a library tile (the library is `library/`).
//!
//! The build is the draft tier (a coarse lattice), which is what the page builds while it is being edited; the gate's own
//! sweep is the library's.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dt_eval::{Files, Options, build, check};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

fn files_under(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files_under(&p, ext, out);
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
}

/// The value of attribute `name` in the text of one tag, if it has it.
fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!(" {name}=\"");
    let at = tag.find(&needle)? + needle.len();
    let end = tag[at..].find('"')?;
    Some(tag[at..at + end].to_owned())
}

/// Every card in the docs: `src` (relative to `docs/`) to the page it is on and the code it must fail with, if any.
fn cards() -> Vec<(String, String, Option<String>)> {
    let mut pages = Vec::new();
    files_under(&root(), "html", &mut pages);
    let mut out = Vec::new();
    for page in pages {
        let text = std::fs::read_to_string(&page).unwrap();
        let name = page.strip_prefix(root()).unwrap().display().to_string();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("<dt-example") {
            let tail = &rest[at..];
            let end = tail
                .find('>')
                .unwrap_or_else(|| panic!("{name}: a <dt-example> tag never ends"));
            let tag = &tail[..end];
            let src =
                attr(tag, "src").unwrap_or_else(|| panic!("{name}: a <dt-example> has no src"));
            out.push((src, name.clone(), attr(tag, "expect-error")));
            rest = &tail[end..];
        }
    }
    out
}

#[test]
fn every_card_builds_or_gives_the_code_it_says() {
    let cards = cards();
    assert!(
        cards.len() >= 8,
        "the docs have only {} examples",
        cards.len()
    );
    // one file, one expectation: two cards on the same file must agree
    let mut expect: BTreeMap<String, (Option<String>, String)> = BTreeMap::new();
    for (src, page, code) in &cards {
        if let Some((prev, on)) = expect.get(src) {
            assert_eq!(
                prev, code,
                "{src} is a card on {on} and on {page} with different expect-error"
            );
        }
        expect.insert(src.clone(), (code.clone(), page.clone()));
    }
    let mut wrong = Vec::new();
    for (src, (code, page)) in &expect {
        assert!(
            src.starts_with("examples/") && src.ends_with(".dtile"),
            "{page}: {src} is not an examples/*.dtile"
        );
        let path = root().join(src);
        assert!(path.is_file(), "{page}: {src} does not exist");
        let path = path.display().to_string();
        let mut opts = Options::in_source(&Files).unwrap();
        opts.draft = true;
        let built = build(&Files, &path, &opts);
        match (code, built) {
            (None, Ok(b)) => assert!(
                b.export.mesh.triangles.len() > 100,
                "{src} built an empty tile"
            ),
            (None, Err(errs)) => wrong.push(format!(
                "{src} (on {page}) does not build: {:?}",
                errs.iter()
                    .map(|e| (e.code, &e.message))
                    .collect::<Vec<_>>()
            )),
            (Some(code), Ok(_)) => wrong.push(format!(
                "{src} (on {page}) built, but the card says it gives {code}"
            )),
            (Some(code), Err(errs)) => {
                let got: Vec<&str> = errs.iter().map(|e| e.code).collect();
                // the check alone gives the same mistakes: the page shows them before it builds
                let checked: Vec<&str> = match check(&Files, &path, &opts) {
                    Ok(_) => Vec::new(),
                    Err(e) => e.iter().map(|e| e.code).collect(),
                };
                if !got.contains(&code.as_str()) || !checked.contains(&code.as_str()) {
                    wrong.push(format!("{src} (on {page}) should give {code}: the build gives {got:?}, the check {checked:?}"));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_example_file_is_used_by_a_card_and_none_is_a_library_tile() {
    let used: Vec<String> = cards().into_iter().map(|c| c.0).collect();
    let mut files = Vec::new();
    files_under(&root().join("examples"), "dtile", &mut files);
    assert!(files.len() >= 8, "only {} example files", files.len());
    for f in files {
        let rel = f
            .strip_prefix(root())
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            used.contains(&rel),
            "{rel} is used by no card: add one or delete the file"
        );
    }
    // the library lists its tiles from library/tiles and library/tests: nothing under docs/ may be reachable from it
    let lib = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library");
    let mut tiles = Vec::new();
    files_under(&lib, "dtile", &mut tiles);
    assert!(
        tiles.iter().all(|t| !t.starts_with(root())),
        "an example is inside the library"
    );
}
