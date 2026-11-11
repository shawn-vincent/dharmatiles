//! Migrating the library from format 0 to format 1, in memory, and the
//! proof that no mesh moves; and a format 0 document read by a program that
//! builds format 1 (migrated as it loads).
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use dt_eval::upgrade::{Moved, Overlay, Upgrading, dependencies, format_of, guard, migrate_files};
use dt_eval::{Library, Memory, Options, Source, build, check, doc};

/// The library as it was written in format 0, kept as test fixtures
/// (`crates/dt-doc/tests/fixtures/library0/`): these tests migrate real
/// format 0 files to format 1 and prove no mesh moves.
#[derive(Clone, Copy)]
struct Library0;

fn library0_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../dt-doc/tests/fixtures/library0")
}

impl Source for Library0 {
    fn read(&self, path: &str) -> Result<String, String> {
        let rel = path
            .strip_prefix("lib:")
            .ok_or_else(|| format!("`{path}` is not in the library"))?;
        std::fs::read_to_string(library0_dir().join(rel)).map_err(|e| format!("{path}: {e}"))
    }
}

fn walk(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            walk(root, &p, out);
        } else if p.extension().is_some_and(|x| x == "dtile") {
            let rel = p.strip_prefix(root).unwrap();
            out.push(format!("lib:{}", rel.to_string_lossy()));
        }
    }
}

fn all_paths() -> Vec<String> {
    let root = library0_dir();
    let mut v = Vec::new();
    walk(&root, &root, &mut v);
    v.sort();
    v
}

fn mem(tile: &str) -> Memory {
    Memory(BTreeMap::from([("t.dtile".to_owned(), tile.to_owned())]))
}

fn render(es: &[dt_eval::Diag]) -> String {
    es.iter()
        .map(|e| format!("[{}] {} {:?}", e.code, e.message, e.help))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `path` migrated with everything it reads (the library's heights, edge
/// defaults and parts, in memory), and the guard run on it.
fn same_mesh_after(sources: &dyn Source, path: &str) -> Vec<Moved> {
    let mut group = dependencies(sources, path).unwrap();
    group.push(path.to_owned());
    let moved = migrate_files(sources, &group, "1.0.0")
        .unwrap_or_else(|e| panic!("{path}\n{}", render(&e)));
    let after = Overlay::new(sources, &moved);
    let d = guard(sources, &after, path, &Options::default())
        .unwrap_or_else(|e| panic!("{path}\n{}", render(&e)));
    assert_eq!(d, None, "{path}: the mesh moved");
    moved
}

#[test]
fn water_with_the_tide_builds_the_same_mesh() {
    // the tide reads `$y` the other way up, and the pins read `$waterEdgeHeight`
    let moved = same_mesh_after(&Library0, "lib:tiles/water/1x1-water+soil-angle.dtile");
    let heights = moved.iter().find(|m| m.path == doc::HEIGHTS).unwrap();
    assert!(
        heights.text.contains("cos(720deg * ($x - $y))"),
        "{}",
        heights.text
    );
    // the tile, the library's heights and the edge defaults all moved
    assert!(moved.iter().any(|m| m.path == doc::EDGES));
    assert!(
        moved
            .iter()
            .any(|m| m.path.ends_with("1x1-water+soil-angle.dtile"))
    );
}

#[test]
fn the_mesh_is_the_same_on_openlock_too() {
    // `dmm` and `sq` scale with the system as a written size always did
    let path = "lib:tiles/water/1x1-water+soil-angle.dtile";
    let mut group = dependencies(&Library0, path).unwrap();
    group.push(path.to_owned());
    let moved = migrate_files(&Library0, &group, "1.0.0").unwrap();
    let after = Overlay::new(&Library0, &moved);
    let ol = Options {
        system: "ol".to_owned(),
        ..Options::default()
    };
    assert_eq!(guard(&Library0, &after, path, &ol).unwrap(), None);
}

#[test]
fn a_three_square_tide_builds_the_same_mesh() {
    same_mesh_after(&Library0, "lib:tiles/water/3x3-water.dtile");
}

#[test]
fn pins_at_a_region_and_at_the_tile_build_the_same_mesh() {
    // a literal point in a pin of a body at a region keeps the tile as its
    // frame (`point($tile, …)`); a bare tuple there would be the region's box
    let text = r#"tile size="2x1" seed=9 {
    line l wobble=3 { through "0.3,0" "0.9,1" }
    region bank { boundedBy l; fill "1.5,0.5" }
    stack {
        base
        dirt { pin top=5.5 at="(0.5, 0.25)" }
        dirt top="bottom + [0.6 .. 1.2]" at=bank {
            pin top="bottom + 0.9" at="(1.4, 0.6)"
            pin top="bottom + 1.1" at="(1.8, 0.3)"
        }
    }
}
"#;
    let src = mem(text);
    let moved = same_mesh_after(&src, "t.dtile");
    let new = &moved.iter().find(|m| m.path == "t.dtile").unwrap().text;
    assert!(new.contains(r#"at="point($tile, 1.4sq, 0.4sq)""#), "{new}");
    // the same pin in a body at the whole tile is a plain tuple
    assert!(
        new.contains(r#"pin top="5.5dmm" at="(0.5sq, 0.75sq)""#),
        "{new}"
    );
}

#[test]
fn a_definition_with_parameters_builds_the_same_mesh() {
    // the grass part's parameters are lengths, counts and shares, from their uses
    let moved = same_mesh_after(&Library0, "lib:tests/plants/1x1-grass-flowers.dtile");
    let grass = moved
        .iter()
        .find(|m| m.path == "lib:parts/grass.dtile")
        .unwrap();
    assert!(
        grass.text.contains(r#"param length "3.5dmm .. 6.5dmm""#),
        "{}",
        grass.text
    );
}

#[test]
fn a_weighted_range_and_a_plain_let_build_the_same_mesh() {
    let text = r#"tile size="1x1" seed=4 {
    let footHeight 4
    let n 6
    stack {
        base
        dirt top=footHeight
        scatter perSquare=n { stone size="low(1..6, 11)" }
        scatter count=n { pebble size="1..2.2" buried="20%..50%" }
    }
}
"#;
    let moved = same_mesh_after(&mem(text), "t.dtile");
    let new = &moved.iter().find(|m| m.path == "t.dtile").unwrap().text;
    assert!(new.contains(r#"let footHeight "4dmm""#), "{new}");
    assert!(new.contains("let n 6"), "{new}");
    assert!(new.contains(r#"size="low(1dmm .. 6dmm, 11)""#), "{new}");
}

#[test]
fn a_printer_migrates_to_the_same_numbers() {
    let moved = migrate_files(
        &Library0,
        &["lib:printers/default.dtile".to_owned()],
        "1.0.0",
    )
    .unwrap();
    let after = Overlay::new(&Library0, &moved);
    let new = doc::read(&after, "lib:printers/default.dtile").unwrap();
    let old = doc::read(&Library0, "lib:printers/default.dtile").unwrap();
    let (a, b) = (
        old.file.doc.printer.as_ref().unwrap(),
        new.file.doc.printer.as_ref().unwrap(),
    );
    assert_eq!(
        (
            a.line.value,
            a.layer.value,
            a.overhang.value,
            a.steepest.value,
            a.gap.value
        ),
        (
            b.line.value,
            b.layer.value,
            b.overhang.value,
            b.steepest.value,
            b.gap.value
        )
    );
}

#[test]
fn a_document_at_the_current_format_is_left_alone() {
    let text = "dharmatiles version=\"1.7.2+5.gabc\"\ntile size=\"1x1\" { stack { base } }\n";
    assert_eq!(format_of("t", text).unwrap(), 1);
    let moved = migrate_files(&mem(text), &["t.dtile".to_owned()], "1.0.0").unwrap();
    assert!(moved.is_empty());
}

// ---- migrated as it loads (question 24) ----------------------------------

const OLD: &str = r#"// An old tile.
tile size="1x1" seed=7 {
    line l wobble=2 { through "0,0.4" "1,0.6" }
    region pool { boundedBy l; fill "0.5,0.1" }
    stack {
        base
        dirt top="[groundHeight .. groundHeight + 1]" { pin top=waterBedHeight at=pool }
        water
    }
}
"#;

/// This program: its current format is 1.
fn current_one() -> Options {
    Options::default()
}

/// A program that builds format 0 as it is written (the hash guard's
/// "before"): a format 0 document reads the library as it was.
fn as_written() -> Options {
    Options {
        current_format: 0,
        ..Options::default()
    }
}

#[test]
fn an_older_tile_is_migrated_as_it_loads_with_a_warning_and_the_file_untouched() {
    let src = mem(OLD);
    let old = build(&src, "t.dtile", &as_written()).unwrap();
    let new = build(&src, "t.dtile", &current_one()).unwrap();
    // the same mesh, from the migrated text
    assert_eq!(
        old.export.mesh.content_hash(),
        new.export.mesh.content_hash()
    );
    // loud, never silent
    assert!(
        new.warnings.iter().any(|w| w.code == "dt::migrated"),
        "{:?}",
        new.warnings
    );
    let w = new
        .warnings
        .iter()
        .find(|w| w.code == "dt::migrated")
        .unwrap();
    assert!(
        w.message.contains("format 0") && w.message.contains("format 1"),
        "{}",
        w.message
    );
    assert!(w.help.as_deref().unwrap().contains("dharmatiles migrate"));
    // the build record says so, and its hash is of the file as written
    assert_eq!(new.record.read_from, Some(0));
    assert_eq!(old.record.read_from, None);
    assert_eq!(new.record.document_hash, old.record.document_hash);
    assert!(new.record.origin().contains("read from format 0"));
    // nothing wrote the file
    assert_eq!(src.read("t.dtile").unwrap(), OLD);
}

#[test]
fn check_says_a_document_was_migrated() {
    let c = check(&mem(OLD), "t.dtile", &current_one()).unwrap();
    assert!(
        c.notes().iter().any(|n| n.code == "dt::migrated"),
        "{:?}",
        c.notes()
    );
    assert!(
        c.migrated
            .iter()
            .any(|u| u.path == "t.dtile" && u.from == 0)
    );
    // the library's own files are format 1: nothing of them is migrated
    assert!(!c.migrated.iter().any(|u| u.path == doc::HEIGHTS));
}

#[test]
fn a_tile_at_the_current_format_loads_without_a_warning() {
    let moved = migrate_files(&mem(OLD), &["t.dtile".to_owned()], "1.0.0").unwrap();
    let text = moved
        .into_iter()
        .find(|m| m.path == "t.dtile")
        .unwrap()
        .text;
    let c = check(&mem(&text), "t.dtile", &current_one()).unwrap();
    assert!(c.migrated.is_empty());
    assert!(!c.migrated.iter().any(|u| u.path == "t.dtile"));
    assert!(!c.notes().iter().any(|n| n.message.contains("t.dtile")));
}

#[test]
fn what_migrate_refuses_loading_refuses_with_the_same_error() {
    let text =
        "tile size=\"1x1\" { let a 3\n stack { base\n dirt top=a\n scatter count=a { rock } } }\n";
    let Err(e) = check(&mem(text), "t.dtile", &current_one()) else {
        panic!("a let used both ways loaded");
    };
    assert!(
        e.iter().any(|d| d.code == "dt::migrate-unit"),
        "{}",
        render(&e)
    );
    let e = migrate_files(&mem(text), &["t.dtile".to_owned()], "1.0.0").unwrap_err();
    assert!(
        e.iter().any(|d| d.code == "dt::migrate-unit"),
        "{}",
        render(&e)
    );
}

#[test]
fn a_program_that_builds_format_0_does_not_migrate() {
    // format 0 as current: nothing is read as another format
    let c = check(&mem(OLD), "t.dtile", &as_written()).unwrap();
    assert!(c.migrated.is_empty());
    let up = Upgrading::new(&Library0, 0, "1.0.0");
    assert!(
        up.read(doc::HEIGHTS)
            .unwrap()
            .starts_with("// The standard heights")
    );
    assert!(up.upgraded().is_empty());
}

/// Once the library, its tests and its parts are
/// converted and format 1 is current, no shipped file needs migrating, so no
/// library tile ever shows the `dt::migrated` warning.
#[test]
fn no_library_or_test_file_needs_migrating() {
    let moved = migrate_files(&Library, &Library::paths().unwrap(), "1.0.0").unwrap();
    let stale: Vec<&str> = moved.iter().map(|m| m.path.as_str()).collect();
    assert!(
        stale.is_empty(),
        "{} files are still format 0: {stale:?}",
        stale.len()
    );
}

/// The sweep: every library tile and test tile, built as format 0 and as the
/// format 1 the whole library migrates to, on DungeonBlocks; the mesh hashes
/// must be the same.  About half an hour, one tile at a time:
///
/// ```text
/// cargo test --release -p dt-eval --test migrate -- --ignored --nocapture
/// ```
#[test]
#[ignore = "builds every library and test tile twice, about half an hour"]
fn every_library_tile_builds_the_same_mesh_after_migrating() {
    let paths = all_paths();
    let moved = migrate_files(&Library0, &paths, "1.0.0").unwrap();
    assert_eq!(moved.len(), paths.len(), "every library file migrates");
    let after = Overlay::new(&Library0, &moved);
    let tiles: Vec<&String> = paths
        .iter()
        .filter(|p| p.starts_with("lib:tiles/") || p.starts_with("lib:tests/"))
        .collect();
    assert!(tiles.len() > 90, "{} tiles", tiles.len());
    let (mut same, mut drift, mut failed) = (Vec::new(), Vec::new(), Vec::new());
    for p in tiles {
        let t = std::time::Instant::now();
        match guard(&Library0, &after, p, &Options::default()) {
            Ok(None) => {
                println!("same   {p} ({:.1}s)", t.elapsed().as_secs_f64());
                same.push(p.clone());
            }
            Ok(Some(d)) => {
                println!("MOVED  {p}: {:016x} -> {:016x}", d.before, d.after);
                drift.push(p.clone());
            }
            Err(es) => {
                println!(
                    "FAILED {p}: {}",
                    es.iter()
                        .map(|e| format!("{} {:?}", e.message, e.help))
                        .collect::<Vec<_>>()
                        .join("; ")
                );
                failed.push(p.clone());
            }
        }
    }
    println!(
        "{} same, {} moved, {} failed",
        same.len(),
        drift.len(),
        failed.len()
    );
    assert!(
        drift.is_empty() && failed.is_empty(),
        "moved: {drift:?}; failed: {failed:?}"
    );
}
