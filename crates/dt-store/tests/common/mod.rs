//! Shared by the tests that build documents from text held in memory.
#![allow(dead_code)]

use std::collections::BTreeMap;

use dt_eval::Memory;

/// The files, by path, as an in-memory file system the program reads.
pub fn memory(files: &[(&str, &str)]) -> Memory {
    Memory(
        files
            .iter()
            .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
            .collect::<BTreeMap<_, _>>(),
    )
}
