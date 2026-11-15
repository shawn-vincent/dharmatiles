//! What a full build of a checked tile will take, before it is made: the page's guard for a device that cannot
//! carry it (a phone's tab is killed at about 1 to 2 GB, and a full grass tile needs 2 GB and 20 single-thread seconds).
//!
//! One function, [`full_cost`], adds up the element schema's weights ([`dt_doc::schema::Cost`]: peak MB and CPU
//! seconds for a whole tile or for each square, and for each thing a scatter places) over the tile's stack.  The
//! weights are fitted to the library as measured (`stage_peak_mb` and a process's CPU seconds, one tile at a time,
//! natively), and the test `the_estimate_holds_to_measurements` holds the sum to those measurements within a
//! stated factor.  An element at a region (`at=`) is taken to cover [`REGION_SHARE`] of the tile.

use crate::doc::Tile;
use crate::doc::{Entry, Setting};
use dt_elements::Value;

/// What a full build of a tile will need, from its checked document.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct FullCost {
    /// The highest memory the build reaches, MB.
    pub peak_mb: f64,
    /// CPU seconds one thread takes (the browser has one).
    pub cpu_secs: f64,
}

/// The connector under the tile (the `base` line): its peak, MB, and CPU seconds.
const BASE_MB: f64 = 100.0;
const BASE_CPU: f64 = 0.25;
/// The share of the tile an element placed `at=` a region is taken to cover.
pub const REGION_SHARE: f64 = 0.4;
/// Things a scatter places when it states no `count` or `perSquare` as a plain number.
const DEFAULT_ITEMS: f64 = 6.0;

/// The number (the middle of a range) a control is set to, if it is one.
fn plain(controls: &[Setting], name: &str) -> Option<f64> {
    match &controls.iter().find(|s| s.name == name)?.value {
        Value::Number(n) => Some(*n),
        Value::Range(a, b) => Some((a + b) / 2.0),
        _ => None,
    }
}

/// How many things a scatter places on a tile of `squares` squares.
fn items(scatter: &Entry, squares: f64) -> f64 {
    plain(&scatter.controls, "count")
        .or_else(|| plain(&scatter.controls, "perSquare").map(|d| d * squares))
        .unwrap_or(DEFAULT_ITEMS)
}

/// The estimate for a build of `tile` at full detail.
#[must_use]
pub fn full_cost(tile: &Tile) -> FullCost {
    let squares = f64::from(tile.cols) * f64::from(tile.rows);
    let mut mb = if tile.has_base { BASE_MB } else { 0.0 };
    let mut cpu = if tile.has_base { BASE_CPU } else { 0.0 };
    for e in &tile.stack {
        let Some(el) = dt_doc::schema::element(&e.what.value) else {
            continue;
        };
        let c = el.cost;
        if el.name == "scatter" {
            // each thing costs what its element does; of several kinds, the average of them
            if e.children.is_empty() {
                continue;
            }
            let n = items(e, squares);
            let kinds = e
                .children
                .iter()
                .filter_map(|c| dt_doc::schema::element(&c.what.value))
                .collect::<Vec<_>>();

            let k = kinds.len().max(1) as f64;
            mb += n * kinds.iter().map(|t| t.cost.mb).sum::<f64>() / k;
            cpu += n * kinds.iter().map(|t| t.cost.cpu).sum::<f64>() / k;
            continue;
        }
        let share = if e.area.is_some() { REGION_SHARE } else { 1.0 };
        mb += c.mb + c.mb_per_square * squares * share;
        cpu += c.cpu + c.cpu_per_square * squares * share;
    }
    FullCost {
        peak_mb: mb,
        cpu_secs: cpu,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Library, Options, check};

    /// Measured on the library, one full build at a time, natively (`dharmatiles build --no-cache --json`, the
    /// build record's highest `stage_peak_mb`; CPU seconds from `/usr/bin/time -l`, user): (tile, peak MB, CPU s).
    /// Remeasure when an element changes much; a tile that builds heavier than its row shows the weights are low.
    const MEASURED: &[(&str, f64, f64)] = &[
        ("lib:tests/ground/1x1-flat.dtile", 104.0, 0.25),
        ("lib:tests/ground/1x1-soil.dtile", 216.0, 0.62),
        ("lib:tests/rocks/1x1-rock-tops.dtile", 222.0, 0.83),
        ("lib:tests/water/1x1-bands-pond.dtile", 174.0, 1.05),
        ("lib:tests/floors/1x1-floor-stone.dtile", 397.0, 1.65),
        ("lib:tests/shapes/3x1-zigzag.dtile", 417.0, 1.64),
        ("lib:tests/water/1x1-reeds.dtile", 654.0, 5.36),
        ("lib:tests/rocks/2x2-rock-clouds.dtile", 983.0, 3.82),
        ("lib:tests/walls/1x1-wall-brick.dtile", 1153.0, 6.99),
        ("lib:tests/cliffs/1x1-cliff-needles.dtile", 1042.0, 11.94),
        ("lib:tests/cliffs/1x1-cliff-plateau.dtile", 1269.0, 12.07),
        ("lib:tests/plants/1x1-tree.dtile", 1393.0, 13.61),
        ("lib:tests/plants/1x1-grass-flowers.dtile", 1448.0, 14.46),
        ("lib:tiles/ground/1x1-grass.dtile", 1885.0, 18.62),
    ];
    /// The factor the estimate is held to, both ways: it may be this much over or under what was measured.
    /// (The extreme is the flat tile, whose dirt has no clods: 2.08 times over.  Every other row is within 1.9.)
    const MB_FACTOR: f64 = 2.1;
    const CPU_FACTOR: f64 = 2.5;

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "a failure that panics with its message named (a broken invariant or a failing test): shown loudly, not swallowed"
    )]
    fn the_estimate_holds_to_measurements() {
        let mut off = Vec::new();
        for (path, mb, cpu) in MEASURED {
            let c = check(&Library, path, &Options::default())
                .unwrap_or_else(|e| panic!("{path}: {e:?}"));
            let est = full_cost(c.tile.as_ref().expect("a tile"));
            let (m, s) = (est.peak_mb / mb, est.cpu_secs / cpu);
            println!(
                "{path}: {:.0} MB vs {mb} ({m:.2}x), {:.1} s vs {cpu} ({s:.2}x)",
                est.peak_mb, est.cpu_secs
            );
            if !(1.0 / MB_FACTOR..=MB_FACTOR).contains(&m) {
                off.push(format!(
                    "{path}: {:.0} MB estimated, {mb} measured",
                    est.peak_mb
                ));
            }
            if !(1.0 / CPU_FACTOR..=CPU_FACTOR).contains(&s) {
                off.push(format!(
                    "{path}: {:.1} s estimated, {cpu} measured",
                    est.cpu_secs
                ));
            }
        }
        assert!(off.is_empty(), "outside the factor:\n{}", off.join("\n"));
    }
}
