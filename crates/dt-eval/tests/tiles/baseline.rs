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
//! **Counted on the seed the tile writes.**  The repair counts, like the
//! hash, triangles and overhang, are the build on the tile's own seed. The
//! budgets on file were recorded as the worst of four seeds (the written one
//! and three more), which leaves room for the counts' noise: the grass blades
//! of a tile overlap in another arrangement for each move of a line, and the
//! share of near-coincident vertices the gate pushes apart changes two or
//! three times over with it. A rewrite still only lowers a budget, so after an
//! edit that moves things, a budget left at its old height is the room.
//!
//! **Budgets re-recorded (`DT_BASELINE=reset`).**  This writes every tile's
//! budgets as the counts measured now, raising or lowering, and
//! prints each one that rose.  It is used only on Shawn's say-so, never to
//! get a failing build past the budget.  So far once: 14 Nov 2026, when the
//! budgets were re-recorded at the measured counts before the aesthetics pass
//! (which will redo the looks), the grass blades' caps having changed.  Plain
//! `DT_BASELINE=write` still only lowers.

#![expect(
    clippy::disallowed_methods,
    reason = "a test: permitted silence: an environment variable or directory that is not set or not readable is the meaning (the default applies); a failure that panics with its message named (a broken invariant or a failing test): shown loudly, not swallowed"
)]

use std::collections::BTreeMap;
use std::path::Path;

use crate::memo;
use dt_eval::{Library, Options, TileRecord, check};
use dt_kit::Repairs;

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/baseline/library-db.tsv");
/// The second line of the file, saying how its budgets were counted. A file
/// that has budgets but not this line is refused, with a rewrite or without
/// (there is no way to start the budgets afresh but `DT_BASELINE=reset`, on
/// Shawn's say-so, and a budget is raised otherwise only by editing the file by
/// hand).
const MEASURE: &str = "# budgets: each tile's repair counts on its own seed are held to these (recorded as the worst of four seeds; a rewrite only lowers them)";
const HEADER: &str = "tile\thash\ttriangles\tvoids_filled\tpinches_moved\tslivers\tcollapsed\tdropped\toverhang_mm2\tband_mm2\tholes\tgate\tmax_voids\tmax_pinches\tmax_slivers\tmax_collapsed\tmax_dropped\tsecs";

/// What a library tile's build measured, everything the baseline holds it to but its budgets and its time:
/// its output hash and the numbers of its record's audit ([`dt_kit::AuditSummary`]) the file keeps. A tile's
/// repair counts and its budgets are both a [`Repairs`], in the order of [`Repairs::named`], the file's columns.
#[derive(Clone, Debug, PartialEq)]
enum Measured {
    Built {
        /// The output hash: the audited mesh as written.
        hash: u64,
        triangles: usize,
        repairs: Repairs,
        /// Areas to the 3 decimals the file keeps ([`mm2`]).
        overhang_mm2: f64,
        band_mm2: f64,
        holes: usize,
        /// Whether it passes the printability gate.
        passes: bool,
    },
    /// The document was refused, with this many diagnostics.
    Refused(usize),
}

impl Measured {
    fn repairs(&self) -> Option<Repairs> {
        match self {
            Self::Built { repairs, .. } => Some(*repairs),
            Self::Refused(_) => None,
        }
    }
    fn passes(&self) -> bool {
        matches!(self, Self::Built { passes: true, .. })
    }
}

/// A tile's line in the file: what it measured on its own seed, its repair budgets (none for a refused tile),
/// and the seconds its build took.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    tile: String,
    measured: Measured,
    budgets: Option<Repairs>,
    secs: f64,
}

/// An area as the file keeps it: to 3 decimals.
fn mm2(x: f64) -> f64 {
    format!("{x:.3}")
        .parse()
        .expect("a formatted number reads back")
}

/// The file's one way of writing a row and of reading one: nothing else reads its text.
impl Row {
    fn to_tsv(&self) -> String {
        let measured = match &self.measured {
            Measured::Built {
                hash,
                triangles,
                repairs,
                overhang_mm2,
                band_mm2,
                holes,
                passes,
            } => {
                let r = columns(*repairs);
                let gate = if *passes { "pass" } else { "fail" };
                format!(
                    "{hash:016x}\t{triangles}\t{r}\t{overhang_mm2:.3}\t{band_mm2:.3}\t{holes}\t{gate}"
                )
            }
            Measured::Refused(n) => format!("refused: {n}{}", "\t".repeat(10)),
        };
        let budgets = self.budgets.map_or_else(|| "\t".repeat(4), columns);
        format!("{}\t{measured}\t{budgets}\t{:.1}", self.tile, self.secs)
    }

    fn from_tsv(line: &str) -> Result<Self, String> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 18 {
            return Err(format!("{} fields, not 18: {line}", f.len()));
        }
        let num = |k: usize| -> Result<usize, String> { field(&f, k, line) };
        let area = |k: usize| -> Result<f64, String> { field(&f, k, line) };
        let measured = if let Some(n) = f[1].strip_prefix("refused: ") {
            Measured::Refused(
                n.parse()
                    .map_err(|e| format!("refused count {n:?}: {e}: {line}"))?,
            )
        } else {
            Measured::Built {
                hash: u64::from_str_radix(f[1], 16)
                    .map_err(|e| format!("hash {:?}: {e}: {line}", f[1]))?,
                triangles: field(&f, 2, line)?,
                repairs: Repairs::from_named([num(3)?, num(4)?, num(5)?, num(6)?, num(7)?]),
                overhang_mm2: area(8)?,
                band_mm2: area(9)?,
                holes: field(&f, 10, line)?,
                passes: match f[11] {
                    "pass" => true,
                    "fail" => false,
                    g => return Err(format!("gate {g:?} is neither pass nor fail: {line}")),
                },
            }
        };
        let budgets = if f[12].is_empty() {
            None
        } else {
            Some(Repairs::from_named([
                num(12)?,
                num(13)?,
                num(14)?,
                num(15)?,
                num(16)?,
            ]))
        };
        Ok(Self {
            tile: f[0].to_owned(),
            measured,
            budgets,
            secs: area(17)?,
        })
    }
}

/// The five counts of `r` as the file's columns.
fn columns(r: Repairs) -> String {
    r.named().map(|(_, n)| n.to_string()).join("\t")
}

/// Column `k` of a row's fields, read as a `T`.
fn field<T: std::str::FromStr>(f: &[&str], k: usize, line: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    f[k].parse()
        .map_err(|e| format!("column {} ({:?}): {e}: {line}", k + 1, f[k]))
}

/// The file's rows, by tile, in its order (none when there is no file yet).
fn read_file() -> Result<Option<Vec<Row>>, String> {
    let text = match std::fs::read_to_string(Path::new(FILE)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{FILE}: {e}")),
    };
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err(format!(
            "{FILE}: the first line is not the header `{HEADER}`"
        ));
    }
    // a file that does not say how its budgets were counted is not read
    if lines.next() != Some(MEASURE) {
        return Err(format!(
            "{FILE} does not say how its budgets were counted (its second line is `{MEASURE}`): the budgets cannot be started afresh; restore the line, or edit a budget by hand where the diff shows it"
        ));
    }
    lines
        .map(|l| Row::from_tsv(l).map_err(|e| format!("{FILE}: {e}")))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn write_file(rows: &[Row]) {
    let mut text = format!("{HEADER}\n{MEASURE}\n");
    for r in rows {
        text.push_str(&r.to_tsv());
        text.push('\n');
    }
    std::fs::write(FILE, text).unwrap_or_else(|e| panic!("{FILE}: {e}"));
}

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

/// One tile's build, measured, and the seconds it took.
fn measure(path: &str) -> (Measured, f64) {
    let opts = Options::default();
    // the memo's build: the release check that passes every library tile through the gate asks for the same
    // one, so the library is built once; the seconds are those of the build, whoever asked first
    let (b, secs) = memo::timed(&Library, path, &opts);
    let b = match b {
        Ok(b) => b,
        Err(e) => return (Measured::Refused(e.len()), secs),
    };
    let a = TileRecord::of(b, &opts.printer, secs).audit;
    let measured = Measured::Built {
        hash: b.export.mesh.content_hash(),
        triangles: a.triangles,
        repairs: a.repairs,
        overhang_mm2: mm2(a.overhang_mm2),
        band_mm2: mm2(a.band_mm2),
        holes: a.holes,
        passes: a.passes(),
    };
    (measured, secs)
}

/// How many builds the sweep runs at once: enough to keep the machine busy, few enough that the biggest
/// builds (8.2 GB each at their peak, the cliffs) fit in memory together. Measured on the cliff tiles,
/// 14 Nov 2026: 300 s on three, 251 on five, 250 on six, the same CPU time each (2,230 s): five fill the
/// machine, and five of the biggest are 41 GB.
const WORKERS: usize = 5;

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

#[test]
#[ignore = "builds the whole library, about three minutes"]
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
    let file = read_file().unwrap_or_else(|e| panic!("{e}"));
    let before: BTreeMap<String, Row> = file
        .iter()
        .flatten()
        .map(|r| (r.tile.clone(), r.clone()))
        .collect();
    let mode = std::env::var("DT_BASELINE").unwrap_or_default();
    let reset = mode == "reset";
    let writing = reset || mode == "write";
    // at another seed the file's hashes, triangles and areas are not this run's to compare or to write
    let other_seed = dt_testkit::test_seed();
    assert!(
        !(writing && other_seed.is_some()),
        "DT_BASELINE={mode} with DT_TEST_SEED={}: the baseline is each tile at its own seed; write it without DT_TEST_SEED",
        other_seed.unwrap_or_default()
    );

    // the tiles build on a few workers at once: one build keeps about a third of the machine busy (the cliff
    // corner: 4.6 of 12 cores), so [`WORKERS`] of them fill it; the longest first, so the last to finish are
    // short
    let mut order: Vec<usize> = (0..tiles.len()).collect();
    order.sort_by(|&a, &b| {
        let t = |j: usize| before.get(&tiles[j]).map_or(f64::MAX, |r| r.secs);
        t(b).total_cmp(&t(a))
    });
    let mut measured: Vec<Option<(Measured, f64)>> = vec![None; tiles.len()];
    for (t, m) in in_parallel(&order, |t| measure(&tiles[t])) {
        measured[t] = Some(m);
    }

    // the repair budget: a count above its budget fails, and a rewrite cannot raise one
    let mut rows: Vec<Row> = Vec::new();
    let mut over = Vec::new();
    let mut under = 0;
    for (tile, m) in tiles.iter().zip(measured) {
        let (measured, secs) = m.expect("every tile was measured");
        let budgets = measured.repairs().map(|counts| {
            // a new tile's budgets are what it repairs now
            let on_file = before.get(tile).and_then(|r| r.budgets).unwrap_or(counts);
            let mut budgets = on_file.named().map(|(_, n)| n);
            for (k, ((word, n), (_, budget))) in
                counts.named().into_iter().zip(on_file.named()).enumerate()
            {
                if reset && n > budget {
                    eprintln!("budget rose: {tile} {word} {budget} -> {n}");
                }
                if n > budget && !reset {
                    over.push(format!("{tile}: {word} {n} is over its budget of {budget}"));
                }
                if n < budget {
                    under += 1;
                }
                budgets[k] = if reset { n } else { n.min(budget) };
            }
            Repairs::from_named(budgets)
        });
        rows.push(Row {
            tile: tile.clone(),
            measured,
            budgets,
            secs,
        });
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

    // at another seed: the repair budgets above, and each tile passing or failing the gate as it does at its own
    if let Some(seed) = other_seed {
        let word = |m: &Measured| if m.passes() { "pass" } else { "fail" };
        let flipped: Vec<String> = rows
            .iter()
            .filter_map(|r| {
                let b = before.get(&r.tile)?;
                (b.measured.passes() != r.measured.passes()).then(|| {
                    format!(
                        "{}: the gate says {} at seed {seed}, {} at its own",
                        r.tile,
                        word(&r.measured),
                        word(&b.measured)
                    )
                })
            })
            .collect();
        assert!(flipped.is_empty(), "{}", flipped.join("\n"));
        return;
    }

    if writing {
        // the tiles not built keep their rows; the file stays in the library's order
        let built: BTreeMap<&str, &Row> = rows.iter().map(|r| (r.tile.as_str(), r)).collect();
        let all: Vec<Row> = if only.is_some() {
            file.iter()
                .flatten()
                .map(|f| {
                    built
                        .get(f.tile.as_str())
                        .map_or_else(|| f.clone(), |r| (*r).clone())
                })
                .chain(
                    rows.iter()
                        .filter(|r| !before.contains_key(&r.tile))
                        .cloned(),
                )
                .collect()
        } else {
            rows.clone()
        };
        write_file(&all);
        eprintln!("wrote {FILE}");
        return;
    }
    assert!(
        file.is_some(),
        "run once with DT_BASELINE=write: there is no baseline"
    );
    let mut changed = Vec::new();
    let (mut t_old, mut t_new) = (0.0, 0.0);
    for r in &rows {
        match before.get(&r.tile) {
            Some(b) => {
                t_old += b.secs;
                t_new += r.secs;
                if b.measured != r.measured {
                    changed.push(format!(
                        "{}\n  was {:?}\n  now {:?}",
                        r.tile, b.measured, r.measured
                    ));
                }
            }
            None => changed.push(format!("{}: not in the baseline", r.tile)),
        }
    }
    eprintln!("build seconds: baseline {t_old:.0}, now {t_new:.0}");
    assert!(changed.is_empty(), "{}", changed.join("\n"));
    if only.is_none() {
        assert_eq!(before.len(), rows.len(), "tiles in the baseline");
    }
}

/// The file reads back as it was written: every row on file, to its text.
#[test]
fn the_baseline_file_reads_back_as_written() {
    let text = std::fs::read_to_string(FILE).unwrap();
    let rows = read_file().unwrap().unwrap();
    for (line, row) in text.lines().skip(2).zip(&rows) {
        assert_eq!(row.to_tsv(), line);
    }
    assert_eq!(rows.len(), text.lines().count() - 2);
    let refused = Row {
        tile: "lib:tiles/x.dtile".into(),
        measured: Measured::Refused(2),
        budgets: None,
        secs: 0.5,
    };
    assert_eq!(Row::from_tsv(&refused.to_tsv()).unwrap(), refused);
}
