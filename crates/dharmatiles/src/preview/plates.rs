//! A print list in the editor: the program's side.  Planning a list's plates (`rebuild`), the plate in view (`platesShow`) and
//! the plates' settings (`platesSet`) are [`dt_service::plates_calls`], written once on a host the program's [`Shared`] and a
//! web worker's `Service` both are; what is here is the HTTP reply of each, the program's build lanes and disk cache that
//! the plan builds on, and writing the plates out (`platesExport`), which is still the program's.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use dt_eval::Diag;
use dt_print::plates::{self, Overrides, Plan};
use dt_service::message::{Kind, Message, MessageData, PlateFilesData, Scope, Severity};
use dt_service::patch::Patch;
use serde::{Deserialize, Serialize};

use super::api::{Out, Reply, out};
use super::edit::Done;
use super::lists::ListEdited;
use super::{Shared, edit, fail};
use crate::plates as cli;
use dt_service::plates_calls as calls;

pub use dt_service::list_calls::is_plates;
pub use dt_service::plates_calls::{PlatesSetReq, PlatesShowReq, again, printer_rows};

#[derive(Deserialize, Serialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(optional_fields = nullable))]
#[serde(deny_unknown_fields)]
pub struct PlatesExportReq {
    pub list: String,
}

/// The plates written.
#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PlatesExported {
    pub files: Vec<String>,
    pub format: String,
    /// The pieces that fail the printability gate, by tile.
    pub failures: Vec<String>,
    pub plates: usize,
    pub total_s: f64,
    pub out: String,
}

/// Open or rebuild a print list or a map for the page (see [`dt_service::plates_calls::rebuild`]).
pub(super) fn rebuild(shared: &Arc<Shared>, id: &str, path: &str) {
    calls::rebuild(&**shared, id, path);
}

/// A test's hand on the moment between the files' times being taken and the files being read: `path`'s hook runs there,
/// once, so a test can change a file exactly where a real change would be the race.
#[cfg(test)]
pub(super) static AFTER_SEEN: std::sync::Mutex<
    std::collections::BTreeMap<String, Box<dyn FnOnce() + Send>>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());

#[cfg(test)]
pub(super) fn after_seen(path: &str) {
    if let Some(hook) = super::lock(&AFTER_SEEN).remove(path) {
        hook();
    }
}

/// `/api/plates/show` `{list, plate}`: the plate in view.
pub(super) fn show(shared: &Shared, v: PlatesShowReq) -> Out<Done> {
    out(calls::show(shared, v))
}

/// `/api/plates/set` `{list, key, value, docRev}` (see [`dt_service::plates_calls::set`]).
pub(super) fn set(shared: &Shared, v: PlatesSetReq) -> Out<ListEdited> {
    out(calls::set(shared, v))
}

fn say(shared: &Shared, id: &str, scope: Scope, make: impl FnOnce(&str) -> Vec<Message>) {
    shared.report(id, scope, make);
}

type Unplanned = (Option<dt_doc::map::PrintList>, Vec<Diag>);

/// The list's file planned (see [`dt_service::plates_calls::plan_from`]).
fn plan_for(
    shared: &Shared,
    path: &str,
    ov: &Overrides,
) -> Result<(dt_doc::map::PrintList, Plan), Box<Unplanned>> {
    let text = cli::read_text(Path::new(path)).map_err(|e| Box::new((None, e)))?;
    calls::plan_from(shared, path, &text, ov)
}

fn slot_of(shared: &Shared, id: &str) -> Result<(String, String), Reply> {
    shared
        .path_of(id)
        .filter(|p| is_plates(p))
        .map(|p| (id.to_owned(), p))
        .ok_or_else(|| fail::refused("404 Not Found", "no such print list"))
}

/// `/api/plates/export` `{tile}`: build every piece in full, lay the plates
/// out by what they cover and write the files.  This runs the whole build:
/// the page waits, the stream says what is happening.
pub(super) fn export(shared: &Shared, v: PlatesExportReq) -> Out<PlatesExported> {
    let (id, path) = slot_of(shared, &v.list)?;
    let rev = shared.rev_of(&path);
    // on the export lane, one at a time, in its turn
    edit::lane_out(shared.on_lane(
        super::queue::Lane::Export,
        format!("export|{id}"),
        rev,
        &id.clone(),
        move |reply| super::queue::SideJob::ExportPlates { id, path, reply },
    ))
}

pub(super) fn export_now(shared: &Shared, id: &str, path: &str) -> Out<PlatesExported> {
    let id = id.to_owned();
    let path = path.to_owned();
    let t0 = Instant::now();
    let report_err = |errs: &[Diag]| {
        say(shared, &id, Scope::Export, |name| {
            dt_service::tile_calls::of_errors(errs, Scope::Export, &id, name, &[])
        });
        fail::mistakes(errs)
    };
    let (_, plan) = match plan_for(shared, &path, &Overrides::default()) {
        Ok(x) => x,
        Err(b) => return Err(report_err(&b.1)),
    };
    let s = &plan.settings;
    let fmt = plates::export::Format::for_printer(&s.machine);
    let tpl = if fmt == plates::export::Format::Bambu {
        match cli::template_for(&s.machine, None, shared.home.as_deref()) {
            Ok(Some(t)) => Some(t),
            Ok(None) => {
                return Err(report_err(&[Diag::plain(
                    dt_lang::codes::PLATES,
                    "a Bambu Studio project needs a template for the printer's settings",
                )
                .help("save any project from Bambu Studio for this printer as bambu.3mf in your tiles folder")]));
            }
            Err(e) => return Err(report_err(&[Diag::plain(dt_lang::codes::PLATES, e)])),
        }
    } else {
        None
    };
    let palette = shared
        .palette_checked()
        .map_err(|e| fail::failure("500 Internal Server Error", e))?;
    let base = shared.options();
    let builds = match cli::build_all(
        &plan,
        &base,
        shared.home.as_deref(),
        shared.store.as_deref(),
        &mut |n, of, name| {
            let progress = super::api::Progress {
                what: "building".to_owned(),
                fraction: (n - 1) as f64 / of as f64,
                label: format!("building {name} in full for the export"),
                key: "plates".to_owned(),
                count: Some((format!("{n} of {of}"), 0, 0)),
                elapsed_s: t0.elapsed().as_secs_f64(),
                eta_s: None,
                estimate_s: None,
            };
            shared.update(&id, Patch::Progress(progress));
        },
    ) {
        Ok(b) => b,
        Err(e) => {
            shared.update(&id, Patch::ClearProgress);
            return Err(report_err(&e));
        }
    };
    shared.update(&id, Patch::ClearProgress);
    let layout = match cli::layout_of(&plan, &builds) {
        Ok(l) => l,
        Err(e) => return Err(report_err(&e)),
    };
    let b0 = &builds.built[0];
    let record = dt_kit::BuildRecord {
        engine: b0.record.engine.clone(),
        document: path.clone(),
        document_hash: b0.record.document_hash,
        seed: 0,
        system: s.system,
        printer: s.machine.name.clone(),
    };
    // a piece that failed the gate is never written: the plates wait for it to pass
    let files = match builds.passed() {
        Some(passed) => match plates::export::write_set(
            &plan,
            &layout,
            &passed,
            fmt,
            &palette,
            tpl.as_deref(),
            &record,
        ) {
            Ok(f) => f,
            Err(e) => return Err(report_err(&[e])),
        },
        None => Vec::new(),
    };
    let out = edit::export_dir(&path)
        .map(|o| o.join("plates").join(&plan.title))
        .map_err(|e| fail::failure("500 Internal Server Error", e))?;
    std::fs::create_dir_all(&out).map_err(|e| {
        fail::failure(
            "500 Internal Server Error",
            format!("cannot create {}: {e}", out.display()),
        )
    })?;
    let mut written = Vec::new();
    for (name, bytes) in &files {
        let p = out.join(name);
        dt_eval::atomic::write_atomic(&p, bytes).map_err(|e| {
            fail::failure(
                "500 Internal Server Error",
                format!("cannot write {}: {e}", p.display()),
            )
        })?;
        written.push(p.display().to_string());
    }
    let failing: Vec<String> = builds
        .gate()
        .iter()
        .enumerate()
        .flat_map(|(k, f)| {
            let name = plan.kinds[k].name.clone();
            f.iter()
                .map(move |l| format!("{name}: {l}"))
                .collect::<Vec<_>>()
        })
        .collect();
    say(shared, &id, Scope::Export, |name| {
        let mut out = Vec::new();
        if !written.is_empty() {
            out.push(
                Message::new(
                    Kind::Export,
                    Severity::Info,
                    Scope::Export,
                    &id,
                    name,
                    format!(
                        "Exported {} plate{} ({} piece{}) for the {}: {}",
                        layout.plates.len(),
                        if layout.plates.len() == 1 { "" } else { "s" },
                        layout.pieces(),
                        if layout.pieces() == 1 { "" } else { "s" },
                        s.machine.label(),
                        written
                            .iter()
                            .map(|f| f.rsplit('/').next().unwrap_or(f))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
                .data(MessageData::PlateFiles(PlateFilesData {
                    files: written.clone(),
                    format: fmt.word().to_owned(),
                    build_s: t0.elapsed().as_secs_f64(),
                })),
            );
        }
        for f in &failing {
            out.push(
                Message::new(
                    Kind::Gate,
                    Severity::Error,
                    Scope::Export,
                    &id,
                    name,
                    format!("{f}: the printability gate fails"),
                )
                .code(dt_lang::codes::GATE),
            );
        }
        out
    });
    // the pieces are full builds now: the view may show them
    again(shared, &id);
    Ok(PlatesExported {
        files: written,
        format: fmt.word().to_owned(),
        failures: failing,
        plates: layout.plates.len(),
        total_s: t0.elapsed().as_secs_f64(),
        out: out.display().to_string(),
    })
}
