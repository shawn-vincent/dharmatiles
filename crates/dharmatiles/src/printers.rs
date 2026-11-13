//! `dharmatiles printers import`: printer profiles from the slicers on this
//! computer, written into your own `printers/` folder (see [`crate::slicers`]).

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

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

/// A candidate as the page and `--json` show it.
pub(crate) fn candidate_json(c: &Candidate, home: Option<&Path>) -> Value {
    let name = slicers::name_of(c);
    let mine = home.is_some_and(|h| h.join("printers").join(format!("{name}.dtile")).exists());
    json!({
        "name": name, "brand": c.brand, "model": c.model, "profile": c.profile,
        "bed": c.bed, "height": c.height, "nozzle": c.nozzle, "slots": c.slots,
        "zones": c.excluded.len(), "have": mine,
    })
}

/// What a scan of one slicer found: what `--json` prints and the page reads.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub(crate) struct ScanJson {
    pub slicer: String,
    pub title: String,
    pub version: String,
    pub label: String,
    pub models: Vec<Value>,
    /// What could not be read, each in words.
    pub problems: Vec<String>,
}

impl ScanJson {
    /// As JSON, for `--json`.
    #[allow(clippy::expect_used)]
    pub(crate) fn value(&self) -> Value {
        serde_json::to_value(self).expect("a scan is plain data")
    }
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
                    say!(
                        "{}",
                        json!({"slicers": f.iter().map(|i| json!({"slicer": i.slicer.word(), "label": i.label()})).collect::<Vec<_>>()})
                    );
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
            say!("{}", scan_json(&scan, Some(&home)).value());
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
                say!(
                    "{}",
                    json!({"written": done.iter().map(|w| json!({"name": w.name, "path": w.path, "kept": w.outcome == Outcome::Kept})).collect::<Vec<_>>(), "problems": scan.problems})
                );
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
