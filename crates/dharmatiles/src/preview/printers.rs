//! The page's side of `dharmatiles printers import`: what the installed
//! slicers have, and writing the chosen models into the user's `printers/`.

use serde::{Deserialize, Serialize};

use super::api::{Out, Reply};
use super::{Shared, fail};
use crate::printers as cli;
use crate::slicers::{self, Outcome};

fn bad(msg: &str) -> Reply {
    fail::refused("400 Bad Request", msg)
}

/// `/api/printers/scan`.
#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PrintersScanReq {
    pub slicer: Option<String>,
}

/// The slicers installed, or one slicer's models.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(untagged)]
pub enum PrintersScanRes {
    Slicers {
        slicers: Vec<SlicerFound>,
    },
    /// What `dharmatiles printers import --json` prints for a scan.
    Models(crate::printers::ScanJson),
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SlicerFound {
    pub slicer: String,
    pub label: String,
    pub title: String,
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PrintersImportReq {
    pub slicer: String,
    /// The profile names of the models to write.
    pub names: Vec<String>,
    pub force: Option<bool>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PrintersImportRes {
    pub written: Vec<Written>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Written {
    pub name: String,
    /// The user's own profile of that name was kept.
    pub kept: bool,
}

/// `/api/printers/scan` `{slicer?}`: without a slicer, the ones installed;
/// with one, every model it has and what could not be read.
pub(super) fn scan(_: &Shared, v: PrintersScanReq) -> Out<PrintersScanRes> {
    let home = crate::preview::roots::personal_home().ok();
    let Some(word) = v.slicer.as_deref() else {
        let f = cli::found().map_err(|e| fail::failure("500 Internal Server Error", e))?;
        return Ok(PrintersScanRes::Slicers {
            slicers: f
                .iter()
                .map(|i| SlicerFound {
                    slicer: i.slicer.word().to_owned(),
                    label: i.label(),
                    title: i.slicer.title().to_owned(),
                })
                .collect(),
        });
    };
    let i = cli::install_of(word, None).map_err(|e| bad(&e))?;
    Ok(PrintersScanRes::Models(cli::scan_json(
        &slicers::scan(&i),
        home.as_deref(),
    )))
}

/// `/api/printers/import` `{slicer, names: [..], force?}`: write the models
/// whose profile names are given into the user's printers folder, and tell
/// every page the new list.
pub(super) fn import(shared: &Shared, v: PrintersImportReq) -> Out<PrintersImportRes> {
    let names: Vec<&str> = v.names.iter().map(String::as_str).collect();
    let install = cli::install_of(&v.slicer, None).map_err(|e| bad(&e))?;
    let scan = slicers::scan(&install);
    let chosen: Vec<&slicers::Candidate> = scan
        .candidates
        .iter()
        .filter(|c| names.contains(&slicers::name_of(c).as_str()))
        .collect();
    if chosen.is_empty() {
        return Err(bad("none of those models is in the slicer"));
    }
    let home = super::edit::home(shared)?;
    let done = slicers::write(
        &chosen,
        &install,
        &home,
        &cli::library_has,
        v.force == Some(true),
    )
    .map_err(|e| fail::failure("500 Internal Server Error", e))?;
    shared.announce();
    Ok(PrintersImportRes {
        written: done
            .iter()
            .map(|w| Written {
                name: w.name.clone(),
                kept: w.outcome == Outcome::Kept,
            })
            .collect(),
    })
}
