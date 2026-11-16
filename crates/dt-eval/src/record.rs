//! **record**: a finished build as one record, the one shape every reader of a build's result takes: what
//! `dharmatiles build --json` prints and writes beside the STL (`.build.json`), what the editor keeps for a
//! tile and sends the page (whose TypeScript type, `TileRecord`, is generated from this one), and what the
//! tests compare.  Nothing builds a build's result by hand.

use std::collections::BTreeMap;

use dt_kit::{AuditSummary, BuildRecord, Printer, Stage};

use crate::build::{Built, CacheUse};

/// A finished build: where it came from, the gate's verdict and numbers, the stage hashes and what the build
/// says about itself.  A tile read from an STL has an audit and nothing else.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub struct TileRecord {
    /// The document's path, or the STL's.
    pub path: String,
    /// Where the build came from; none for an STL.
    pub record: Option<BuildRecord>,
    /// The build target (`db`, `ol`, `db-fine`); none for an STL.
    pub target: Option<String>,
    /// The gate's verdict and the numbers it judged.  A low build is not checked: its failures are empty.
    pub audit: AuditSummary,
    /// Whether the gate checked this build: only a full build is.
    pub checked: bool,
    /// Stage hashes, in build order.
    pub stages: Vec<Stage>,
    /// Seconds the build took.
    pub build_s: f64,
    /// Seconds each stage took, in build order (`3:grass`, `union`, `audit`).
    pub stage_secs: Vec<(String, f64)>,
    /// The highest resident memory each stage reached, MB, under the stage names of `stage_secs` (measured
    /// natively by sampling the process: read it from a build run alone).  Empty for a build from the cache and
    /// for an STL; varies from run to run.
    pub stage_peak_mb: Vec<(String, f64)>,
    /// The edge level the ground meets every tile edge at, mm.
    pub edge_mm: Option<f64>,
    /// Numbers the elements noted (counts), by name; a name noted again is `name#2`, `name#3`.
    pub stats: BTreeMap<String, f64>,
    /// Pieces elements left out because they could not make them, by element.
    pub left_out: BTreeMap<String, usize>,
    /// What the disk cache did.
    pub cache: CacheUse,
    /// What the build did for its things: made, copied, read from the shelf.
    pub things: dt_kit::ThingUse,
}

impl TileRecord {
    /// The record of `b`, built in `build_s` seconds, judged under `printer`.
    #[must_use]
    pub fn of(b: &Built, printer: &Printer, build_s: f64) -> Self {
        let mut audit = b.audit.summary(printer);
        // only a full build was checked: a low one has nothing to fail
        if !b.checked {
            audit.failures.clear();
        }
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        let stats = b
            .report
            .notes
            .iter()
            .map(|(k, v)| {
                let n = seen.entry(k.as_str()).or_default();
                *n += 1;
                let key = if *n == 1 {
                    k.clone()
                } else {
                    format!("{k}#{n}")
                };
                (key, *v)
            })
            .collect();
        Self {
            path: b.record.document.clone(),
            target: Some(b.record.target()),
            record: Some(b.record.clone()),
            audit,
            checked: b.checked,
            stages: b.report.stages.clone(),
            build_s,
            stage_secs: b.report.stage_secs.clone(),
            stage_peak_mb: b.report.stage_peak_mb.clone(),
            edge_mm: b.report.edge_mm,
            stats,
            left_out: b.report.left_out.iter().cloned().collect(),
            cache: b.report.cache.clone(),
            things: b.report.things,
        }
    }

    /// The record of an STL read as a tile: its audit, read in `build_s` seconds.
    #[must_use]
    pub fn of_stl(path: &str, audit: AuditSummary, build_s: f64) -> Self {
        Self {
            path: path.to_owned(),
            record: None,
            target: None,
            audit,
            checked: true,
            stages: Vec::new(),
            build_s,
            stage_secs: Vec::new(),
            stage_peak_mb: Vec::new(),
            edge_mm: None,
            stats: BTreeMap::new(),
            left_out: BTreeMap::new(),
            cache: CacheUse::default(),
            things: dt_kit::ThingUse::default(),
        }
    }

    /// Whether the build passes the gate.
    #[must_use]
    pub fn passes(&self) -> bool {
        self.audit.passes()
    }
}
