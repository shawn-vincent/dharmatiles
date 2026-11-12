//! Every file of the library, as shipped, decodes in format 1: the heights
//! first, then the parts the tiles `use`, then the rest.

#![allow(clippy::unwrap_used)]

mod common;

use dt_doc::{Known, decode};

fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|x| x == "dtile") {
            out.push(p);
        }
    }
}

#[test]
fn every_library_file_decodes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library");
    let mut all = Vec::new();
    files(&root, &mut all);
    all.sort();
    assert!(all.len() > 50, "found only {} library files", all.len());

    let mut known = Known::new();
    let read = |p: &std::path::Path| std::fs::read_to_string(p).unwrap();
    known
        .add_heights("lib:parts/heights.dtile", common::HEIGHTS)
        .unwrap();
    // the parts, registered under the keys the `use` lines name
    for part in ["grass", "trees"] {
        let path = root.join(format!("parts/{part}.dtile"));
        let doc = decode(&path.to_string_lossy(), &read(&path), &known)
            .unwrap_or_else(|e| panic!("{part}:\n{}", common::render(&e)));
        known.add(&format!("parts/{part}"), &doc);
    }
    for path in &all {
        let name = path.strip_prefix(&root).unwrap().to_string_lossy();
        if name == "parts/heights.dtile" {
            continue;
        }
        // a palette is read by the 3MF writer, not decoded: its header is all dt-doc reads
        if name.starts_with("palettes/") {
            dt_doc::header::check_text(&name, &read(path)).unwrap();
            continue;
        }
        decode(&name, &read(path), &known)
            .unwrap_or_else(|e| panic!("{name}:\n{}", common::render(&e)));
    }
}

#[test]
fn a_library_tile_with_its_header_taken_off_is_refused_as_format_0() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../library/tiles/ground/1x1-grass.dtile");
    let text = std::fs::read_to_string(path).unwrap();
    let body = text.split_once('\n').unwrap().1;
    let (errs, shown) = common::fails(body);
    assert!(errs.iter().any(|e| e.code == "dt::format-0"), "{shown}");
}
