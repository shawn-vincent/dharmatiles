//! Documents the tests write in the old form (format 0), carried to format
//! 1 the way `dharmatiles migrate` carries them, so a test can say its tile
//! in a few plain lines and still run it through the program that reads
//! format 1 only.
#![allow(dead_code)]

use std::collections::BTreeMap;

use dt_eval::Memory;
use dt_migrate::files::{Legacy, Reader, migrate_files};

struct Files<'a>(&'a BTreeMap<String, String>);

impl Reader for Files<'_> {
    fn read(&self, path: &str) -> Result<String, String> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| format!("no file `{path}`"))
    }
}

/// The files, each migrated against the library as it was written in
/// format 0 (a file that is already format 1, or that cannot be migrated,
/// stays as written: the test says what it makes of it).
pub fn modern(files: &[(&str, &str)]) -> Memory {
    let map: BTreeMap<String, String> = files
        .iter()
        .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
        .collect();
    let mut out = map.clone();
    let reader = Files(&map);
    let old = Legacy(&reader);
    for path in map.keys() {
        if let Ok(moved) = migrate_files(&old, std::slice::from_ref(path), "1.0.0")
            && let Some(m) = moved.into_iter().next()
        {
            // the header shares the document's first line, so a mistake is
            // on the line the test wrote it on
            let text = match m.text.split_once('\n') {
                Some((header, rest)) if header.starts_with("dharmatiles ") => {
                    format!("{header}; {}", rest.trim_start_matches('\n'))
                }
                _ => m.text,
            };
            out.insert(path.clone(), text);
        }
    }
    Memory(out)
}
