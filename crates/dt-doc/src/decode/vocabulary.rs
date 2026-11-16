//! The tile language's vocabulary, held to the registry (`dt_lang::words`): every word the schema, the decoder and
//! the printer's settings use is a row, every row is used, a keyword is written in the decoder only by its
//! constant, and a word does one job (one kind of word, one measure), except the collisions `ledger.kdl` lists.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dt_lang::words::{self, WordKind};

use crate::elements::{ELEMENTS, NODES};
use crate::ledger::Ledger;
use crate::schema::{Axis, Control};
use crate::units::Kind;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("a readable directory");
    for e in entries {
        let path = e.expect("a readable directory entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// Every control of the schema, with the measure it takes (`None` for a group) and the owner's path.
fn controls() -> Vec<(String, Option<Axis>, &'static str)> {
    fn walk(
        c: &'static Control,
        owner: &'static str,
        out: &mut Vec<(String, Option<Axis>, &'static str)>,
    ) {
        out.push((c.name.to_owned(), c.value, owner));
        for s in c.subs {
            walk(s, owner, out);
        }
    }
    let mut out = Vec::new();
    for n in NODES {
        for c in n.controls {
            // the one control of `except` is the node's own word
            if c.name != n.name {
                walk(c, n.name, &mut out);
            }
        }
    }
    for e in ELEMENTS {
        for c in e.controls {
            walk(c, e.name, &mut out);
        }
    }
    out
}

fn choices() -> BTreeSet<&'static str> {
    fn walk(c: &'static Control, out: &mut BTreeSet<&'static str>) {
        out.extend(c.words.iter().map(|w| w.word));
        for s in c.subs {
            walk(s, out);
        }
    }
    let mut out = BTreeSet::new();
    for n in NODES {
        for c in n.controls {
            walk(c, &mut out);
        }
    }
    for e in ELEMENTS {
        for c in e.controls {
            walk(c, &mut out);
        }
    }
    // an element named as a choice is the element, not a second word
    out.retain(|w| !ELEMENTS.iter().any(|e| e.name == *w));
    out.extend(Kind::WORDS.iter().map(|(w, _)| *w));
    out
}

/// The text of every Rust file outside dt-lang that could read a word by its constant.
fn sources() -> String {
    let mut files = Vec::new();
    rust_files(&root().join("crates"), &mut files);
    rust_files(&root().join("xtask"), &mut files);
    let mut all = String::new();
    for f in files {
        if f.starts_with(root().join("crates/dt-lang")) || f.ends_with("decode/vocabulary.rs") {
            continue;
        }
        all.push_str(&std::fs::read_to_string(&f).expect("a readable Rust file"));
    }
    all
}

#[test]
fn every_word_the_schema_uses_is_registered_and_every_tile_language_word_is_used() {
    let controls = controls();
    let mut used: BTreeMap<WordKind, BTreeSet<String>> = BTreeMap::new();
    used.entry(WordKind::Element)
        .or_default()
        .extend(ELEMENTS.iter().map(|e| e.name.to_owned()));
    used.entry(WordKind::Control)
        .or_default()
        .extend(controls.iter().map(|(n, _, _)| n.clone()));
    used.entry(WordKind::Control).or_default().extend(
        super::PRINTER_KEYS
            .iter()
            .chain(super::machine::KEYS.iter())
            .map(|s| (*s).to_owned()),
    );
    used.entry(WordKind::Choice)
        .or_default()
        .extend(choices().into_iter().map(str::to_owned));
    // a word that code reads by its constant is used where the constant is
    let text = sources();
    for (name, word, kind) in words::NAMES {
        if text.contains(&format!("words::{name}")) {
            used.entry(*kind).or_default().insert((*word).to_owned());
        }
    }

    let mut problems = Vec::new();
    for (kind, ws) in &used {
        for w in ws {
            if words::find(*kind, w).is_none() {
                problems.push(format!(
                    "`{w}` is used as a {kind:?} and is not in dt-lang/src/words.rs: add a row with its meaning"
                ));
            }
        }
    }
    for w in words::all() {
        let kind_used = matches!(
            w.kind,
            WordKind::Keyword
                | WordKind::Control
                | WordKind::Choice
                | WordKind::Element
                | WordKind::FileKind
        );
        if kind_used && !used.get(&w.kind).is_some_and(|s| s.contains(w.word)) {
            problems.push(format!(
                "`{}` is registered as a {:?} and nothing uses it: remove the row, or use it",
                w.word, w.kind
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// The words the decoder matches by constant: the keywords, and the names of the tile's own nodes' controls.
fn scanned_words() -> BTreeSet<&'static str> {
    let mut out: BTreeSet<&'static str> = words::KEYWORDS.iter().map(|w| w.word).collect();
    for n in NODES {
        for c in n.controls {
            out.insert(c.name);
            for s in c.subs {
                out.insert(s.name);
            }
        }
    }
    out.insert("closed");
    out
}

#[test]
fn no_keyword_is_written_in_the_decoder_outside_the_registry() {
    let mut files = Vec::new();
    rust_files(&root().join("crates/dt-doc/src/decode"), &mut files);
    assert!(files.len() >= 6, "found only {} decoder files", files.len());
    let scanned = scanned_words();
    let mut found = Vec::new();
    for f in files {
        if f.ends_with("vocabulary.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&f).expect("a readable Rust file");
        // the decoder's own tests are documents in strings; only the code before them is the decoder
        let code = text.split("#[cfg(test)]").next().unwrap_or(&text);
        for (n, line) in code.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for w in &scanned {
                let lit = format!("\"{w}\"");
                let mut rest = line;
                while let Some(i) = rest.find(&lit) {
                    let before = rest[..i].chars().last();
                    if !before.is_some_and(|c| c == '\\' || c.is_alphanumeric()) {
                        found.push(format!("{}:{}: {}", f.display(), n + 1, line.trim()));
                    }
                    rest = &rest[i + lit.len()..];
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "a keyword is written as a string in the decoder; use its constant (dt_lang::words::NAME):\n{}",
        found.join("\n")
    );
}

#[test]
fn a_word_does_one_job_except_where_the_ledger_says_it_does_not() {
    let ledger = Ledger::load(&root()).expect("the ledger reads");
    let mut broken: BTreeMap<String, String> = BTreeMap::new();

    // one word, one kind of word
    let mut kinds: BTreeMap<&str, BTreeSet<WordKind>> = BTreeMap::new();
    for w in words::all().filter(|w| w.kind.is_tile_language()) {
        kinds.entry(w.word).or_default().insert(w.kind);
    }
    for (word, ks) in &kinds {
        if ks.len() > 1 {
            broken.insert(format!("collision:{word}"), format!("`{word}` is {ks:?}"));
        }
    }

    // one control name, one measure
    let mut axes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, axis, owner) in controls() {
        axes.entry(name).or_default().insert(axis.map_or_else(
            || format!("a group (on {owner})"),
            |a| format!("{a:?} (on {owner})"),
        ));
    }
    for (name, owners) in axes {
        let measures: BTreeSet<String> = owners
            .iter()
            .map(|s| s.split(" (on ").next().unwrap_or(s).to_owned())
            .collect();
        if measures.len() > 1 {
            broken.insert(
                format!("collision:{name}"),
                format!("control `{name}` measures {measures:?}"),
            );
        }
    }

    let mut ids: BTreeSet<String> = broken.keys().cloned().collect();
    ids.extend(
        ledger
            .ids()
            .into_iter()
            .filter(|i| i.starts_with("collision:"))
            .map(str::to_owned),
    );
    let mut problems = Vec::new();
    for id in ids {
        if let Err(e) = ledger.expect(&id, broken.contains_key(&id)) {
            problems.push(format!(
                "{e}{}",
                broken
                    .get(&id)
                    .map_or(String::new(), |why| format!(" ({why})"))
            ));
        }
    }
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}
