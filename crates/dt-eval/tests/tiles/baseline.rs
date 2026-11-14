//! The full-library baseline: every library tile (tests aside, the set
//! `dharmatiles build --all` builds) on DungeonBlocks, with its output
//! hash, triangles, repairs, overhang and band areas, gate verdict and
//! build time, kept in `tests/baseline/library-db.tsv`.
//!
//! A release check, not a test run (about five minutes, one tile at a time
//! so each time is its own):
//!
//! ```text
//! cargo test --release -p dt-eval --test tiles -- --ignored the_library_builds_as_the_baseline_says --nocapture
//! DT_BASELINE=write cargo test --release -p dt-eval --test tiles -- --ignored the_library_builds_as_the_baseline_says   # rewrite it
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

use crate::memo;
use dt_eval::{Library, Options, check};

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

/// The elements and tiles `DT_BASELINE_ONLY` names (`grass,cliff` or
/// `lib:tiles/ground/1x1-grass.dtile`), if it is set: only those tiles and the
/// library tiles whose stack uses one of the elements (a scatter's things
/// count) are built, and every other tile's row is kept as the file has it, a rewrite
/// included. `cargo xtask check` sets it for a change to one element; a
/// release builds the whole library.
fn only() -> Option<Vec<String>> {
    let v = std::env::var("DT_BASELINE_ONLY").ok()?;
    let names: Vec<String> = v
        .split(',')
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .collect();
    assert!(!names.is_empty(), "DT_BASELINE_ONLY={v:?} names no element");
    Some(names)
}

/// The elements a tile's stack uses, its scatters' things included.
fn uses(path: &str) -> Vec<String> {
    let c = check(&Library, path, &Options::default())
        .unwrap_or_else(|e| panic!("{path} does not check: {e:?}"));
    let tile = c.tile.unwrap_or_else(|| panic!("{path} is not a tile"));
    tile.stack
        .iter()
        .flat_map(|e| {
            std::iter::once(e.what.value.clone())
                .chain(e.children.iter().map(|c| c.what.value.clone()))
        })
        .collect()
}

/// One tile's measured row, without budgets; the last column is the build time.
fn row(path: &str) -> String {
    let opts = Options::default();
    // the memo's build: the release check that passes every library tile through the gate asks for the same
    // one, so the library is built once; the seconds are those of the build, whoever asked first
    let (b, secs) = memo::timed(&Library, path, &opts);
    let b = match b {
        Ok(b) => b,
        Err(e) => return format!("{path}\trefused: {}\t\t\t\t\t\t\t\t\t\t\t", e.len()),
    };
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
/// [`REPAIRS`], or why it did not build.
fn repairs_on(path: &str, seed: u64) -> Result<[u64; 5], String> {
    let opts = Options {
        seed: Some(seed),
        ..Options::default()
    };
    // Each extra seed is read once, for its repair counts; kept, three more copies of the library (several GB)
    // would stand for the whole run.
    // fresh: built once and dropped, so the memo does not keep it
    let b = dt_eval::build(&Library, path, &opts).map_err(|e| format!("on seed {seed}: {e:?}"))?;
    let a = &b.audit;
    Ok([
        a.voids.len(),
        a.pinches_moved,
        a.slivers.len(),
        a.collapsed,
        a.dropped.len(),
    ]
    .map(|n| n as u64))
}

/// What one of the sweep's builds gave.
enum Done {
    /// A tile's row on its written seed.
    Row(Vec<String>),
    /// Its repair counts on an extra seed.
    Repairs(Result<[u64; 5], String>),
}

/// How many builds the sweep runs at once: enough to keep the machine busy, few enough that the biggest
/// builds (near 8.5 GB each at their peak) fit in memory together.
const WORKERS: usize = 3;

/// `work` on each job, in the order given, on [`WORKERS`] threads; each build still spreads over the whole
/// machine inside. Each result comes back with its job.
fn in_parallel<T: Send>(order: &[usize], work: impl Fn(usize) -> T + Sync) -> Vec<(usize, T)> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(Vec::with_capacity(order.len()));
    std::thread::scope(|s| {
        for _ in 0..WORKERS {
            s.spawn(|| {
                loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&j) = order.get(k) else { break };
                    let r = work(j);
                    out.lock().expect("a sweep worker panicked").push((j, r));
                }
            });
        }
    });
    out.into_inner().expect("a sweep worker panicked")
}

/// Each tile's build time on the file, to start the longest first; none for a tile not on it.
fn last_secs() -> BTreeMap<String, f64> {
    let Ok(text) = std::fs::read_to_string(Path::new(FILE)) else {
        return BTreeMap::new(); // no file yet: any order
    };
    text.lines()
        .skip(2)
        .filter_map(|l| {
            let f: Vec<String> = l.split('\t').map(str::to_owned).collect();
            (f.len() == FIELDS).then(|| (f[0].clone(), secs(&f)))
        })
        .collect()
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
    let only = only();
    let tiles: Vec<String> = match &only {
        Some(names) => tiles
            .into_iter()
            .filter(|p| names.contains(p) || uses(p).iter().any(|u| names.contains(u)))
            .collect(),
        None => tiles,
    };
    if let Some(names) = &only {
        assert!(
            !tiles.is_empty(),
            "no library tile uses {}",
            names.join(" or ")
        );
        eprintln!("{} library tiles use {}", tiles.len(), names.join(" or "));
    }
    // every build the sweep makes, a tile on its written seed or an extra one, runs on a few workers at once:
    // one build keeps about half the machine busy, so three together finish in about half the time (measured
    // 14 Nov 2026: six tiles 41 s together, 90 s one by one); the biggest builds peak near 8.5 GB each
    let jobs: Vec<(usize, Option<u64>)> = (0..tiles.len())
        .flat_map(|t| std::iter::once((t, None)).chain(EXTRA_SEEDS.map(|s| (t, Some(s)))))
        .collect();
    let took = last_secs();
    let mut order: Vec<usize> = (0..jobs.len()).collect();
    // the longest tiles first, so the last to finish are short
    order.sort_by(|&a, &b| {
        let t = |j: usize| took.get(&tiles[jobs[j].0]).copied().unwrap_or(f64::MAX);
        t(b).total_cmp(&t(a))
    });
    let done = in_parallel(&order, |j| match jobs[j] {
        (t, None) => Done::Row(row(&tiles[t]).split('\t').map(str::to_owned).collect()),
        (t, Some(seed)) => Done::Repairs(repairs_on(&tiles[t], seed)),
    });
    let mut measured: Vec<Vec<String>> = vec![Vec::new(); tiles.len()];
    let mut extra: Vec<Vec<Result<[u64; 5], String>>> = vec![Vec::new(); tiles.len()];
    for (j, d) in done {
        match d {
            Done::Row(r) => measured[jobs[j].0] = r,
            Done::Repairs(r) => extra[jobs[j].0].push(r),
        }
    }
    // each tile's repair counts: the worst over the written seed and the extra ones (a tile refused on its own
    // seed has none, whatever the extra seeds did)
    let worst: Vec<Option<[u64; 5]>> = measured
        .iter()
        .zip(&extra)
        .map(|(m, more)| {
            let mut w = [0; 5];
            for (k, (_, at, _)) in REPAIRS.iter().enumerate() {
                w[k] = count(m, *at)?;
            }
            for on in more {
                let on = on.as_ref().unwrap_or_else(|e| panic!("{}: {e}", m[0]));
                for (a, b) in w.iter_mut().zip(on) {
                    *a = (*a).max(*b);
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

    // the tiles not built keep their rows; the file stays in the library's order
    let built: BTreeMap<&str, &Vec<String>> = rows.iter().map(|r| (r[0].as_str(), r)).collect();
    let all: Vec<Vec<String>> = if only.is_some() {
        before
            .values()
            .map(|f| {
                built
                    .get(f[0].as_str())
                    .map_or_else(|| f.clone(), |r| (*r).clone())
            })
            .chain(rows.iter().filter(|r| !before.contains_key(&r[0])).cloned())
            .collect()
    } else {
        rows.clone()
    };
    let mut text = format!("{HEADER}\n{MEASURE}\n");
    for r in &all {
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
    if only.is_none() {
        assert_eq!(before.len(), rows.len(), "tiles in the baseline");
    }
}
