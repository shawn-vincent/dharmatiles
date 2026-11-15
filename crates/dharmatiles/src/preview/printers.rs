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
#[derive(Deserialize, Serialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PrintersScanReq {
    pub slicer: Option<String>,
}

/// The slicers installed, or one slicer's models.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(untagged)]
pub enum PrintersScanRes {
    Slicers {
        slicers: Vec<SlicerFound>,
    },
    /// What `dharmatiles printers import --json` prints for a scan.
    Models(crate::printers::ScanJson),
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SlicerFound {
    pub slicer: String,
    pub label: String,
    pub title: String,
}

impl SlicerFound {
    /// A slicer found here, as the page and `printers import --json` say it.
    pub(crate) fn of(i: &slicers::Install) -> Self {
        Self {
            slicer: i.slicer.word().to_owned(),
            label: i.label(),
            title: i.slicer.title().to_owned(),
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PrintersImportReq {
    pub slicer: String,
    /// The profile names of the models to write.
    pub names: Vec<String>,
    pub force: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PrintersImportRes {
    pub written: Vec<PrinterWritten>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PrinterWritten {
    pub name: String,
    /// Where the profile is.
    pub path: String,
    /// The user's own profile of that name was kept.
    pub kept: bool,
}

impl PrinterWritten {
    /// A profile `slicers::write` wrote or kept.
    pub(crate) fn of(w: &slicers::Written) -> Self {
        Self {
            name: w.name.clone(),
            path: w.path.display().to_string(),
            kept: w.outcome == Outcome::Kept,
        }
    }
}

/// `/api/printers/scan` `{slicer?}`: without a slicer, the ones installed;
/// with one, every model it has and what could not be read.
pub(super) fn scan(_: &Shared, v: PrintersScanReq) -> Out<PrintersScanRes> {
    let Some(word) = v.slicer.as_deref() else {
        let f = cli::found().map_err(|e| fail::failure("500 Internal Server Error", e))?;
        return Ok(PrintersScanRes::Slicers {
            slicers: f.iter().map(SlicerFound::of).collect(),
        });
    };
    let home = crate::preview::roots::personal_home().map_err(|e| bad(&e))?;
    let i = cli::install_of(word, None).map_err(|e| bad(&e))?;
    Ok(PrintersScanRes::Models(cli::scan_json(
        &slicers::scan(&i),
        Some(home.as_path()),
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
        written: done.iter().map(PrinterWritten::of).collect(),
    })
}
