//! `dharmatiles printers import`: printer profiles from the slicers on this
//! computer, written into your own `printers/` folder (see [`crate::slicers`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cli::PrintersAction;
use crate::slicers::{self, Candidate, Install, Outcome, Scan, Slicer};
use crate::{ERROR, OK};

/// The folders the slicers are looked for in: the user's home and the
/// applications folder (`DT_APPLICATIONS` says another, for tests).
fn places() -> Result<(PathBuf, PathBuf), String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or("cannot find your home folder: HOME is not set")?;
    let apps = std::env::var_os("DT_APPLICATIONS")
        .filter(|d| !d.is_empty())
        .map_or_else(|| PathBuf::from("/Applications"), PathBuf::from);
    Ok((home, apps))
}

/// Every slicer found here.
pub(crate) fn found() -> Result<Vec<Install>, String> {
    let (home, apps) = places()?;
    Ok(slicers::installs(&home, &apps))
}

/// The install of one slicer, or the folder named for it.
pub(crate) fn install_of(word: &str, path: Option<&Path>) -> Result<Install, String> {
    let slicer = Slicer::from_word(word).ok_or_else(|| {
        format!(
            "`{word}` is not a slicer this reads: {}",
            Slicer::ALL.map(Slicer::word).join(", ")
        )
    })?;
    if let Some(p) = path {
        return Ok(slicers::install_at(slicer, p));
    }
    found()?
        .into_iter()
        .find(|i| i.slicer == slicer)
        .ok_or_else(|| {
            format!(
                "{} is not installed here (looked in the applications folder and its settings folder); name its profiles folder with --path",
                slicer.title()
            )
        })
}

/// Does the library already have a profile of this name?
pub(crate) fn library_has(name: &str) -> bool {
    dt_eval::printer_names(None).iter().any(|(n, _)| n == name)
}

/// A model a slicer knows, as the page and `--json` show it.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub(crate) struct CandidateJson {
    /// The profile name it would be written under.
    pub name: String,
    pub brand: String,
    pub model: String,
    /// The slicer's own name for it.
    pub profile: String,
    pub bed: [f64; 2],
    pub height: Option<f64>,
    pub nozzle: Option<f64>,
    pub slots: u32,
    /// Keep-out zones on the bed.
    pub zones: usize,
    /// You already have a profile of that name.
    pub have: bool,
}

pub(crate) fn candidate_json(c: &Candidate, home: Option<&Path>) -> CandidateJson {
    let name = slicers::name_of(c);
    let have = home.is_some_and(|h| h.join("printers").join(format!("{name}.dtile")).exists());
    CandidateJson {
        name,
        brand: c.brand.clone(),
        model: c.model.clone(),
        profile: c.profile.clone(),
        bed: c.bed,
        height: c.height,
        nozzle: c.nozzle,
        slots: c.slots,
        zones: c.excluded.len(),
        have,
    }
}

/// What a scan of one slicer found: what `--json` prints and the page reads.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct ScanJson {
    pub slicer: String,
    pub title: String,
    pub version: String,
    pub label: String,
    pub models: Vec<CandidateJson>,
    /// What could not be read, each in words.
    pub problems: Vec<String>,
}

pub(crate) fn scan_json(s: &Scan, home: Option<&Path>) -> ScanJson {
    ScanJson {
        slicer: s.install.slicer.word().to_owned(),
        title: s.install.slicer.title().to_owned(),
        version: s.install.version.clone(),
        label: s.install.label(),
        models: s
            .candidates
            .iter()
            .map(|c| candidate_json(c, home))
            .collect(),
        problems: s.problems.clone(),
    }
}

/// What `printers import --json` prints after writing: what was written and what could not be read.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) struct PrintersImported {
    pub written: Vec<crate::preview::printers::PrinterWritten>,
    pub problems: Vec<String>,
}

fn size_words(c: &Candidate) -> String {
    format!(
        "{} x {} mm{}{}",
        c.bed[0],
        c.bed[1],
        c.height
            .map_or_else(String::new, |h| format!(", {h} mm tall")),
        if c.slots > 1 {
            format!(", {} filaments", c.slots)
        } else {
            String::new()
        }
    )
}

/// `dharmatiles printers …`.
pub(crate) fn run(action: PrintersAction) -> u8 {
    match action {
        PrintersAction::Import {
            slicer,
            models,
            all,
            path,
            list,
            force,
            json,
        } => import(
            slicer.as_deref(),
            &models,
            all,
            path.as_deref(),
            list,
            force,
            json,
        ),
    }
}

fn import(
    slicer: Option<&str>,
    models: &[String],
    all: bool,
    path: Option<&Path>,
    list: bool,
    force: bool,
    json: bool,
) -> u8 {
    let home = match crate::preview::roots::personal_home() {
        Ok(h) => h,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let Some(word) = slicer else {
        // no slicer named: say which are here
        return match found() {
            Ok(f) => {
                if json {
                    crate::say_json(&crate::preview::printers::PrintersScanRes::Slicers {
                        slicers: f
                            .iter()
                            .map(crate::preview::printers::SlicerFound::of)
                            .collect(),
                    });
                } else if f.is_empty() {
                    say!(
                        "No slicer found here. Name a profiles folder with `printers import <bambu|orca|prusa|cura> --path DIR`."
                    );
                } else {
                    say!("Slicers found:");
                    for i in &f {
                        say!("  {:<7} {}", i.slicer.word(), i.label());
                    }
                    say!(
                        "Then: dharmatiles printers import <slicer> --list, or name models, or --all."
                    );
                }
                OK
            }
            Err(e) => {
                tell!("error: {e}");
                ERROR
            }
        };
    };
    let install = match install_of(word, path) {
        Ok(i) => i,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let scan = slicers::scan(&install);
    let want: Vec<String> = models.iter().map(|m| m.to_lowercase()).collect();
    let chosen: Vec<&Candidate> = scan
        .candidates
        .iter()
        .filter(|c| {
            all || {
                let n = format!("{} {}", c.brand, c.model).to_lowercase();
                want.iter().any(|w| n.contains(w))
            }
        })
        .collect();
    let writing = !list && (all || !models.is_empty());
    if !writing {
        if json {
            crate::say_json(&scan_json(&scan, Some(&home)));
        } else {
            say!("{}: {} models", install.label(), scan.candidates.len());
            for c in &scan.candidates {
                say!(
                    "  {:<34} {}",
                    format!("{} {}", c.brand, c.model),
                    size_words(c)
                );
            }
            report_problems(&scan.problems);
            if !list {
                say!("Name the models to import (a part of the name is enough), or use --all.");
            }
        }
        return OK;
    }
    if chosen.is_empty() {
        tell!(
            "error: no model of {} matches {}",
            install.label(),
            models.join(", ")
        );
        return ERROR;
    }
    match slicers::write(&chosen, &install, &home, &library_has, force) {
        Ok(done) => {
            if json {
                crate::say_json(&PrintersImported {
                    written: done
                        .iter()
                        .map(crate::preview::printers::PrinterWritten::of)
                        .collect(),
                    problems: scan.problems.clone(),
                });
            } else {
                for w in &done {
                    match w.outcome {
                        Outcome::Written => say!("wrote {}  ({})", w.name, w.path.display()),
                        Outcome::Kept => say!(
                            "kept {}  (already in your printers folder; --force replaces it)",
                            w.name
                        ),
                    }
                }
                report_problems(&scan.problems);
            }
            OK
        }
        Err(e) => {
            tell!("error: {e}");
            ERROR
        }
    }
}

fn report_problems(p: &[String]) {
    if !p.is_empty() {
        tell!("{} profile(s) could not be used:", p.len());
        for x in p.iter().take(20) {
            tell!("  {x}");
        }
        if p.len() > 20 {
            tell!("  ... and {} more", p.len() - 20);
        }
    }
}
