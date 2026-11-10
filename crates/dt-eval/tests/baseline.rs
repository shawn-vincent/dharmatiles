//! The full-library baseline: every library tile (tests aside, the set
//! `dharmatiles build --all` builds) on DungeonBlocks, with its output
//! hash, triangles, repairs, overhang and band areas, gate verdict and
//! build time, kept in `tests/baseline/library-db.tsv`.
//!
//! A release check, not a test run (about five minutes, one tile at a time
//! so each time is its own):
//!
//! ```text
//! cargo test --release -p dt-eval --test baseline -- --ignored --nocapture
//! DT_BASELINE=write cargo test --release -p dt-eval --test baseline -- --ignored   # rewrite it
//! ```
//!
//! Every column but `secs` must match the file exactly (a step that is
//! meant to change them rewrites it in the same commit); the times are
//! printed against the file's, to show what got slower.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use dt_eval::{Library, Options, build};

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/baseline/library-db.tsv");
const HEADER: &str = "tile\thash\ttriangles\tvoids_filled\tpinches_moved\tslivers\tcollapsed\tdropped\toverhang_mm2\tband_mm2\tholes\tgate\tsecs";

/// One tile's row; the last column is the build time.
fn row(path: &str) -> String {
    let opts = Options::default();
    let t0 = Instant::now();
    let b = match build(&Library, path, &opts) {
        Ok(b) => b,
        Err(e) => return format!("{path}\trefused: {}\t\t\t\t\t\t\t\t\t\t\t", e.len()),
    };
    let secs = t0.elapsed().as_secs_f64();
    let a = &b.audit;
    let gate = a.failures(&opts.printer);
    // the output hash: the audited mesh as written
    let hash = b.export.mesh.content_hash();
    format!(
        "{path}\t{hash:016x}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{:.3}\t{}\t{}\t{secs:.1}",
        a.triangles,
        a.voids.len(),
        a.pinches_moved,
        a.slivers.len(),
        a.collapsed,
        a.dropped.len(),
        a.overhang_mm2,
        a.band_mm2,
        a.holes,
        if gate.is_empty() { "pass" } else { "fail" },
    )
}

/// The row without its time.
fn fixed(r: &str) -> &str {
    r.rsplit_once('\t').map_or(r, |(a, _)| a)
}

fn secs(r: &str) -> f64 {
    r.rsplit('\t')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0)
}

#[test]
#[ignore = "builds the whole library, about five minutes"]
fn the_library_builds_as_the_baseline_says() {
    let tiles: Vec<String> = Library::paths()
        .expect("the library is listed")
        .into_iter()
        .filter(|p| p.starts_with("lib:tiles/"))
        .collect();
    assert!(tiles.len() > 50, "{} library tiles", tiles.len());
    let rows: Vec<String> = tiles.iter().map(|p| row(p)).collect();
    let mut text = format!("{HEADER}\n");
    for r in &rows {
        writeln!(text, "{r}").unwrap();
    }
    if std::env::var_os("DT_BASELINE").is_some_and(|v| v == "write") {
        std::fs::write(FILE, &text).unwrap();
        eprintln!("wrote {FILE}");
        return;
    }
    let old = std::fs::read_to_string(Path::new(FILE)).expect("run once with DT_BASELINE=write");
    let before: BTreeMap<&str, &str> = old
        .lines()
        .skip(1)
        .map(|l| (l.split('\t').next().unwrap(), l))
        .collect();
    let mut changed = Vec::new();
    let (mut t_old, mut t_new) = (0.0, 0.0);
    for r in &rows {
        let name = r.split('\t').next().unwrap();
        match before.get(name) {
            Some(b) => {
                t_old += secs(b);
                t_new += secs(r);
                if fixed(b) != fixed(r) {
                    changed.push(format!("{name}\n  was {}\n  now {}", fixed(b), fixed(r)));
                }
            }
            None => changed.push(format!("{name}: not in the baseline")),
        }
    }
    eprintln!("build seconds: baseline {t_old:.0}, now {t_new:.0}");
    assert!(changed.is_empty(), "{}", changed.join("\n"));
    assert_eq!(before.len(), rows.len(), "tiles in the baseline");
}
