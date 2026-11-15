//! The rule of `dt_doc::ops::apply`: an edit never adds a mistake the old text did not have.  Random valid
//! operations (from a small seeded generator) are made to every test tile and every library tile; whatever an
//! edit answers, the text it leaves decodes no worse than the text it was made to, read here by a decoder
//! that knows the library's heights and parts, not by the check `apply` makes itself.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use dt_doc::nodeid;
use dt_doc::ops::{Kind, Flaw, Op, Place, Reader, apply_with};
use dt_doc::{Known, decode};
use kdl::KdlValue;

/// SplitMix64: a seeded stream, the same on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, of: &'a [T]) -> &'a T {
        &of[self.below(of.len())]
    }
}

/// The decoder with the library's heights and parts known.
struct Library(Known);

impl Reader for Library {
    fn mistakes(&self, _: Kind, text: &str) -> Vec<Flaw> {
        decode("tile", text, &self.0)
            .err()
            .map(|e| e.iter().map(Flaw::from).collect())
            .unwrap_or_default()
    }
}

fn library() -> Library {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library");
    let mut known = common::known();
    for part in ["grass", "trees"] {
        let path = root.join(format!("parts/{part}.dtile"));
        let doc = decode(
            &path.to_string_lossy(),
            &std::fs::read_to_string(&path).unwrap(),
            &known,
        )
        .unwrap_or_else(|e| panic!("{part}:\n{}", common::render(&e)));
        known.add(&format!("parts/{part}"), &doc);
    }
    Library(known)
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|x| x == "dtile") {
            out.push(p);
        }
    }
}

const WORDS: [&str; 6] = ["rock", "pool", "x", "6..10", "$nothing", "0.5sq"];
const CONTROLS: [&str; 6] = ["seed", "size", "count", "height", "at", "level"];
const NODES: [&str; 6] = [
    "rock",
    "dirt",
    "rock size=\"6..10\"",
    "grass at=nowhere",
    "scatter count=2 { rock }",
    "not an element",
];

fn value(r: &mut Rng) -> KdlValue {
    match r.below(4) {
        0 => KdlValue::Integer(r.below(200) as i128 - 20),
        1 => KdlValue::Float(r.below(1000) as f64 / 8.0),
        2 => KdlValue::Bool(r.below(2) == 0),
        _ => KdlValue::String((*r.pick(&WORDS)).to_owned()),
    }
}

/// One random operation on the nodes of `text`, made and its text answered (the refusals too).
fn one(reader: &Library, text: &str, r: &mut Rng) -> Option<Result<String, String>> {
    let table = nodeid::table(text).ok()?;
    let n = r.pick(&table);
    let id = n.id.as_str();
    let other = r.pick(&table).id.clone();
    let word = (*r.pick(&WORDS)).to_owned();
    let control = *r.pick(&CONTROLS);
    let value = value(r);
    let node = *r.pick(&NODES);
    let op = match r.below(9) {
        0 => Op::Hide { id },
        1 => Op::Take { id },
        2 => Op::Move {
            id,
            before: Some(&other),
        },
        3 => Op::Move { id, before: None },
        4 => Op::SetControl {
            id,
            name: control,
            value: Some(value),
        },
        5 => Op::SetControl {
            id,
            name: control,
            value: None,
        },
        6 => Op::SetArg {
            id,
            index: 0,
            value,
        },
        7 => Op::Rename { id, new: &word },
        _ => Op::Insert {
            place: if r.below(2) == 0 {
                Place::Stack
            } else {
                Place::Before(&other)
            },
            node,
        },
    };
    Some(match apply_with(reader, Kind::Tile, text, &op) {
        Ok(d) => Ok(d.text),
        Err(e) => Err(e.to_string()),
    })
}

fn set(reader: &Library, text: &str) -> BTreeSet<Flaw> {
    reader.mistakes(Kind::Tile, text).into_iter().collect()
}

#[test]
fn no_edit_leaves_text_that_decodes_worse() {
    let started = std::time::Instant::now();
    let reader = library();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library");
    let mut all = Vec::new();
    files(&root.join("tests"), &mut all);
    files(&root.join("tiles"), &mut all);
    all.sort();
    assert!(all.len() > 50, "found only {} tiles", all.len());
    let (mut made, mut refused, mut other) = (0, 0, 0);
    let mut r = Rng(0xD4A7_0001);
    for path in &all {
        let name = path.strip_prefix(&root).unwrap().to_string_lossy();
        let mut text = std::fs::read_to_string(path).unwrap();
        for step in 0..8 {
            let before = set(&reader, &text);
            let Some(answer) = one(&reader, &text, &mut r) else {
                break;
            };
            match answer {
                Ok(after) => {
                    made += 1;
                    let now = set(&reader, &after);
                    let added: Vec<_> = now.difference(&before).collect();
                    assert!(
                        added.is_empty(),
                        "{name} step {step}: the edit added {added:?}\n{after}"
                    );
                    text = after;
                }
                Err(e) if e.starts_with("this edit would add a mistake") => refused += 1,
                // a name that is not there, an entry that cannot be moved: refused in words, not by the check
                Err(_) => other += 1,
            }
        }
    }
    eprintln!(
        "{} tiles: {made} edits made, {refused} refused for adding a mistake, {other} refused otherwise, {:?}",
        all.len(),
        started.elapsed()
    );
    assert!(made > 100, "only {made} edits were made");
    assert!(refused > 20, "only {refused} edits were refused: the generator is too tame");
    assert!(started.elapsed().as_secs() < 5, "the property test is over its 5 s");
}
