//! The service's verbs are words of the registry (`dt_lang::words::VERBS`): a verb made with `Verb::new("name")`
//! anywhere in the program is a row, and a row no verb is made for is a word nothing uses.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use dt_lang::words::{self, WordKind};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("a readable directory") {
        let path = e.expect("a readable directory entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// The names `Verb::new("…")` is called with, outside `call.rs` (whose tests make two of their own).
fn verbs_made() -> BTreeSet<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    let marker = format!("{}::new(\"", "Verb");
    let mut out = BTreeSet::new();
    for f in files {
        if f.ends_with("dt-service/src/call.rs") || f.ends_with("tests/verbs_are_registered.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&f).expect("a readable Rust file");
        let mut rest = text.as_str();
        while let Some(i) = rest.find(&marker) {
            rest = &rest[i + marker.len()..];
            let end = rest.find('"').expect("a closed string");
            out.insert(rest[..end].to_owned());
        }
    }
    out
}

#[test]
fn every_verb_is_a_row_and_every_row_a_verb() {
    let made = verbs_made();
    assert!(made.len() > 40, "found only {} verbs", made.len());
    let rows: BTreeSet<String> = words::all()
        .filter(|w| w.kind == WordKind::Verb)
        .map(|w| w.word.to_owned())
        .collect();
    let new: Vec<_> = made.difference(&rows).collect();
    let unused: Vec<_> = rows.difference(&made).collect();
    assert!(
        new.is_empty(),
        "verbs made and not in dt-lang/src/words.rs (add a row with its meaning): {new:?}"
    );
    assert!(
        unused.is_empty(),
        "verbs registered and never made (remove the row, or make the verb): {unused:?}"
    );
}
