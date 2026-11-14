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
//! Every measured column must match the file exactly (a step that is meant
//! to change them rewrites it in the same commit); the times are printed
//! against the file's, to show what got slower.
//!
//! **The repair budget.**  The gate repairs a mesh before it judges it: it
//! moves pinch vertices, fills sealed voids, removes slivers, collapses
//! edges and drops loose bodies.  Each tile has a budget for each of those
//! five counts, the `max_*` columns, and the test fails when a count is above
//! its budget, whatever else was rewritten.  A rewrite (`DT_BASELINE=write`)
//! can only lower a budget (a count under it becomes the budget) or give a
//! new tile one (its counts); it refuses to write a file in which a count
//! stands above its budget.  To raise a budget on purpose, edit the file by
//! hand, where the diff shows it.
//!
//! **Counted on four seeds.**  A repair count is noisy: the grass blades of a
//! tile overlap in a different arrangement for each seed (and for each move of
//! a line), and the share of near-coincident vertices the gate has to push
//! apart changes two or three times over with it.  So a tile's count, the
//! number its budget is held to, is the worst of its repairs on four seeds:
//! the seed the tile writes, and [`EXTRA_SEEDS`].  The hash, triangles and
//! overhang columns stay on the written seed.  The first rewrite under this
//! measure set the budgets to the worst of four, which for most tiles is above
//! what the single written seed had counted: that redefinition, approved by
//! Shawn, is the one time a budget rose.  From then on the rules above hold,
//! and a file that does not carry the line saying so is refused: there is no
//! second way to start the budgets afresh.
//!
//! **Budgets re-recorded (`DT_BASELINE=reset`).**  This writes every tile's
//! budgets as the worst of four seeds measured now, raising or lowering, and
//! prints each one that rose.  It is used only on Shawn's say-so, never to
//! get a failing build past the budget.  So far once: 14 Nov 2026, when the
//! budgets were re-recorded at the measured counts before the aesthetics pass
//! (which will redo the looks), the grass blades' caps having changed.  Plain
//! `DT_BASELINE=write` still only lowers.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use dt_eval::{Library, Options, build};

/// The three seeds a tile is built on besides the one it writes, to count its
/// repairs: the only place they are set.
const EXTRA_SEEDS: [u64; 3] = [9001, 9002, 9003];

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/baseline/library-db.tsv");
/// The second line of the file, saying how its budgets were counted. A file
/// that has budgets but not this line is refused, with a rewrite or without
/// (there is no way to start the budgets afresh: that was done once, when they
/// became the worst of four seeds, and a budget is raised now only by editing
/// the file by hand).
const MEASURE: &str =
    "# budgets: the worst repair count of each tile on four seeds (its own and three more)";
const HEADER: &str = "tile\thash\ttriangles\tvoids_filled\tpinches_moved\tslivers\tcollapsed\tdropped\toverhang_mm2\tband_mm2\tholes\tgate\tmax_voids\tmax_pinches\tmax_slivers\tmax_collapsed\tmax_dropped\tsecs";

/// The columns that count repairs, and where each one's budget is.
const REPAIRS: [(&str, usize, usize); 5] = [
    ("voids_filled", 3, 12),
    ("pinches_moved", 4, 13),
    ("slivers", 5, 14),
    ("collapsed", 6, 15),
    ("dropped", 7, 16),
];
/// A row's fields: the tile and its measures (12), the budgets (5), the time.
const FIELDS: usize = 18;

/// One tile's measured row, without budgets; the last column is the build time.
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

/// The five repair counts of `path` built on `seed`, in the order of
/// [`REPAIRS`].
fn repairs_on(path: &str, seed: u64) -> [u64; 5] {
    let opts = Options {
        seed: Some(seed),
        ..Options::default()
    };
    let b = build(&Library, path, &opts).unwrap_or_else(|e| panic!("{path} on seed {seed}: {e:?}"));
    let a = &b.audit;
    [
        a.voids.len(),
        a.pinches_moved,
        a.slivers.len(),
        a.collapsed,
        a.dropped.len(),
    ]
    .map(|n| n as u64)
}

/// A measured row (12 fields and the time) with a tile's budgets put in.
fn with_budgets(measured: &[String], budgets: &[String; 5]) -> Vec<String> {
    let mut out: Vec<String> = measured[..12].to_vec();
    out.extend(budgets.iter().cloned());
    out.push(measured[12].clone());
    out
}

/// The measured part of a row: its first twelve fields.
fn fixed(f: &[String]) -> String {
    f[..12].join("\t")
}

fn secs(f: &[String]) -> f64 {
    f.last().and_then(|s| s.parse().ok()).unwrap_or(0.0)
}

/// A repair count in a row, if the row has one (a refused tile has none).
fn count(f: &[String], at: usize) -> Option<u64> {
    f.get(at).and_then(|s| s.parse().ok())
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
    let measured: Vec<Vec<String>> = tiles
        .iter()
        .map(|p| row(p).split('\t').map(str::to_owned).collect())
        .collect();
    // each tile's repair counts: the worst over the written seed and the extra ones
    let worst: Vec<Option<[u64; 5]>> = measured
        .iter()
        .map(|m| {
            let mut w = [0; 5];
            for (k, (_, at, _)) in REPAIRS.iter().enumerate() {
                w[k] = count(m, *at)?;
            }
            for seed in EXTRA_SEEDS {
                let on = repairs_on(&m[0], seed);
                for (a, b) in w.iter_mut().zip(on) {
                    *a = (*a).max(b);
                }
            }
            Some(w)
        })
        .collect();
    let old = std::fs::read_to_string(Path::new(FILE)).ok();
    // the file's rows, by tile; a file from before the budgets has 13 fields
    let mut before: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for l in old
        .iter()
        .flat_map(|o| o.lines().skip(1))
        .filter(|l| !l.starts_with('#'))
    {
        let f: Vec<String> = l.split('\t').map(str::to_owned).collect();
        before.insert(f[0].clone(), f);
    }
    // a file that does not say how its budgets were counted is not read: there is no reset
    assert!(
        old.as_ref()
            .is_none_or(|o| o.lines().nth(1) == Some(MEASURE)),
        "{FILE} does not say how its budgets were counted (its second line is `{MEASURE}`): the budgets cannot be started afresh; restore the line, or edit a budget by hand where the diff shows it"
    );
    let has_budgets = before.values().all(|f| f.len() == FIELDS);
    let mode = std::env::var("DT_BASELINE").unwrap_or_default();
    let reset = mode == "reset";
    let writing = reset || mode == "write";

    // the repair budget: a count above its budget fails, and a rewrite cannot raise one
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut over = Vec::new();
    let mut under = 0;
    for (m, w) in measured.iter().zip(&worst) {
        let name = &m[0];
        let on_file = before.get(name).filter(|f| f.len() == FIELDS);
        let mut budgets: [String; 5] = Default::default();
        for (k, (word, _, bat)) in REPAIRS.iter().enumerate() {
            let Some(n) = w.map(|w| w[k]) else { continue };
            let budget = match on_file {
                Some(f) => count(f, *bat).unwrap_or(n),
                // a new tile, or the first write of the budgets: what it repairs now
                None => n,
            };
            if reset && n > budget {
                eprintln!("budget rose: {name} {word} {budget} -> {n}");
            }
            if n > budget && !reset {
                over.push(format!("{name}: {word} {n} is over its budget of {budget}"));
            }
            if n < budget {
                under += 1;
            }
            budgets[k] = if reset { n } else { n.min(budget) }.to_string();
        }
        rows.push(with_budgets(m, &budgets));
    }
    assert!(
        over.is_empty(),
        "the gate repairs more than the budget allows (lower the cause; a budget is raised only by editing {FILE} by hand):\n{}",
        over.join("\n")
    );
    if under > 0 {
        eprintln!(
            "{under} repair counts are under their budgets: DT_BASELINE=write lowers the budgets to them"
        );
    }

    let mut text = format!("{HEADER}\n{MEASURE}\n");
    for r in &rows {
        writeln!(text, "{}", r.join("\t")).unwrap();
    }
    if writing {
        std::fs::write(FILE, &text).unwrap();
        eprintln!("wrote {FILE}");
        return;
    }
    assert!(
        old.is_some(),
        "run once with DT_BASELINE=write: there is no baseline"
    );
    assert!(
        has_budgets,
        "the baseline has no repair budgets yet: run once with DT_BASELINE=write to add them"
    );
    let mut changed = Vec::new();
    let (mut t_old, mut t_new) = (0.0, 0.0);
    for r in &rows {
        match before.get(&r[0]) {
            Some(b) => {
                t_old += secs(b);
                t_new += secs(r);
                if fixed(b) != fixed(r) {
                    changed.push(format!("{}\n  was {}\n  now {}", r[0], fixed(b), fixed(r)));
                }
            }
            None => changed.push(format!("{}: not in the baseline", r[0])),
        }
    }
    eprintln!("build seconds: baseline {t_old:.0}, now {t_new:.0}");
    assert!(changed.is_empty(), "{}", changed.join("\n"));
    assert_eq!(before.len(), rows.len(), "tiles in the baseline");
}
