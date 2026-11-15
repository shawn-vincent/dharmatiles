//! A print list in the editor: the program's side.  Planning a list's plates (`rebuild`), the plate in view (`platesShow`) and
//! the plates' settings (`platesSet`) are [`dt_service::plates_calls`], written once on a host the program's [`Shared`] and a
//! web worker's `Service` both are; what is here is the HTTP reply of each, the program's build lanes and disk cache that
//! the plan builds on, and writing the plates out (`platesExport`), which is still the program's.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use dt_eval::Diag;
use dt_print::plates::{Overrides, Plan};
use dt_service::message::{Kind, Message, MessageData, PlateFilesData, Scope, Severity};
use dt_service::patch::Patch;

use super::api::{Out, Reply, out};
use super::edit::Done;
use super::lists::ListEdited;
use super::{Shared, edit, fail};
use crate::plates as cli;
use dt_service::plates_calls as calls;

pub use dt_service::list_calls::is_plates;
pub use dt_service::plates_calls::{PlatesSetReq, PlatesShowReq, again, printer_rows};

pub use dt_service::export_calls::{PlatesExportReq, PlatesExported};

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
    let made =
        match dt_service::export_calls::plate_files(shared, &plan, &path, &mut |n, of, name| {
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
        }) {
            Ok(m) => m,
            Err(errs) => {
                shared.update(&id, Patch::ClearProgress);
                return Err(report_err(&errs));
            }
        };
    shared.update(&id, Patch::ClearProgress);
    let (layout, fmt, files, failing) = (made.layout, made.format, made.files, made.failures);
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
        downloads: Vec::new(),
        format: fmt.word().to_owned(),
        failures: failing,
        plates: layout.plates.len(),
        total_s: t0.elapsed().as_secs_f64(),
        out: out.display().to_string(),
    })
}
