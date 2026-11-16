//! `dharmatiles plates`: a print list laid on plates and written as files.
//!
//! The same steps the editor's Export button takes: read the list, plan it
//! (tiles and printer, the filaments checked before anything builds), build
//! every distinct piece in full, lay the plates out by what the builds really
//! cover, write one Bambu project, or one STL or 3MF per plate.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cli_wire::{PlatesChecked, PlatesReport, Refused};
use dt_doc::map::{Group, PrintList};
use dt_eval::{Diag, Files};
use dt_lang::Colour;
use dt_print::plates::builds::Builds;
use dt_print::plates::export::Format;
use dt_print::plates::wire::{PlanOnly, PlanPlate};
use dt_print::plates::{self, Context, Overrides};
use dt_store::Store;

use crate::{AUDIT_FAILED, ERROR, OK, report};

/// Read a print list.
///
/// # Errors
/// The file cannot be read, or does not decode: every mistake at its text.
pub(crate) fn read_list(path: &Path) -> Result<PrintList, Vec<Diag>> {
    read_list_text(path, &read_text(path)?)
}

/// A print list's file, read once: whoever hashes the text and decodes it
/// must use this one read, or the hash can label a different text.
///
/// # Errors
/// The file cannot be read.
pub(crate) fn read_text(path: &Path) -> Result<String, Vec<Diag>> {
    std::fs::read_to_string(path).map_err(|e| {
        vec![Diag::plain(
            dt_lang::codes::SOURCE,
            format!("cannot read {}: {e}", path.display()),
        )]
    })
}

/// Decode `text`, which is what `path` holds (read by [`read_text`]).
///
/// # Errors
/// The text does not decode: every mistake at its text.
pub(crate) fn read_list_text(path: &Path, text: &str) -> Result<PrintList, Vec<Diag>> {
    dt_doc::map::decode(&path.display().to_string(), text)
        .map_err(|e| e.iter().map(Diag::from).collect())
}

/// What you have printed, for a list that looks in the inventory (the
/// default; `--all` and `plates inventory=ignore` do not): the shelf of
/// `inventory.dinv` in My tiles, or `None` where there is no inventory yet or
/// the list leaves it alone.
///
/// # Errors
/// An inventory that is there and has mistakes (each at its text): a list is
/// never laid out against a log that is not understood.
pub(crate) fn shelf_for(
    list: &PrintList,
    ov: &Overrides,
    home: Option<&Path>,
) -> Result<Option<dt_doc::inventory::Shelf>, Vec<Diag>> {
    let looks = ov
        .inventory
        .or_else(|| list.plates.inventory.as_ref().map(|i| i.value))
        .unwrap_or(true);
    let Some(home) = home.filter(|_| looks) else {
        return Ok(None);
    };
    let docs = dt_service::docs::Docs::new(
        std::sync::Arc::new(crate::preview::storage::DiskStorage),
        None,
    );
    Ok(dt_service::inventory_calls::load(&docs, home)?
        .map(|l| dt_doc::inventory::Shelf::of(&l.inventory)))
}

/// A list's name: its file's stem.
pub(crate) fn title_of(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "plates".to_owned(), |s| s.to_string_lossy().into_owned())
}

/// The Bambu template's settings for a printer: `--bambu-template`, else the
/// profile's `template` beside your tiles, else `bambu.3mf` there; `None`
/// when there is none.
///
/// # Errors
/// A template that is named or found and cannot be read.
pub(crate) fn template_for(
    machine: &dt_eval::Machine,
    given: Option<&Path>,
    home: Option<&Path>,
) -> Result<Option<String>, String> {
    let found = || -> Option<PathBuf> {
        let h = home?;
        let named = machine.template.as_ref().map(|t| h.join(t));
        named
            .filter(|p| p.is_file())
            .or_else(|| Some(h.join("bambu.3mf")).filter(|p| p.is_file()))
    };
    let Some(path) = given.map(Path::to_path_buf).or_else(found) else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let settings =
        dt_print::project_settings(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    dt_print::bed_centre(&settings).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(settings))
}

/// The settings words a caller may give, as the command line's.
#[expect(
    clippy::too_many_arguments,
    reason = "the flat inputs of one maths step, named at the call"
)]
pub(crate) fn overrides(
    printer: Option<String>,
    system: Option<dt_lang::System>,
    colour: Option<Colour>,
    group: Option<&str>,
    gap: Option<f64>,
    margin: Option<f64>,
    turn: Option<&str>,
    all: bool,
) -> Result<Overrides, String> {
    let group = match group {
        None => None,
        Some("filament") => Some(Group::Filament),
        Some("size") => Some(Group::Size),
        Some(m) => return Err(format!("`{m}` is not a grouping: use `filament` or `size`")),
    };
    let turn = match turn {
        None => None,
        Some("allowed") => Some(true),
        Some("off") => Some(false),
        Some(m) => {
            return Err(format!(
                "`{m}` is not a turn setting: use `allowed` or `off`"
            ));
        }
    };
    Ok(Overrides {
        printer,
        system,
        colour,
        group,
        gap,
        margin,
        turn,
        inventory: all.then_some(false),
    })
}

/// The format a word names.
pub(crate) fn format_of(word: Option<&str>, machine: &dt_eval::Machine) -> Result<Format, String> {
    word.map_or_else(|| Ok(Format::for_printer(machine)), Format::from_word)
}

/// `plan` then, unless `check_only`, everything to the files.
#[expect(
    clippy::too_many_arguments,
    reason = "the flat inputs of one maths step, named at the call"
)]
pub(crate) fn run(
    list_path: &Path,
    ov: &Overrides,
    format: Option<&str>,
    out: &Path,
    template: Option<&Path>,
    palette: &dt_kit::Palette,
    store: Option<&Store>,
    check_only: bool,
    json: bool,
) -> u8 {
    let t0 = Instant::now();
    let home = match crate::preview::roots::personal_home() {
        Ok(h) => Some(h),
        Err(e) => {
            // only `home:` and your own printers need it; the plan says so where it does
            tell!("note: {e}");
            None
        }
    };
    let list = match read_list(list_path) {
        Ok(l) => l,
        Err(e) => return fail(&e, json),
    };
    let base = match crate::library::options(dt_lang::System::Db, "default", None) {
        Ok(o) => o,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    let dir = list_path.parent().filter(|p| !p.as_os_str().is_empty());
    let shelf = match shelf_for(&list, ov, home.as_deref()) {
        Ok(s) => s,
        Err(mut e) => {
            e.push(Diag::plain(
                dt_lang::codes::INVENTORY,
                "this list prints only what your inventory lacks, and the inventory does not read",
            )
            .help("mend inventory.dinv, or lay the list out without it: --all"));
            return fail(&e, json);
        }
    };
    let ctx = Context {
        sources: &Files,
        list_dir: dir.or(Some(Path::new("."))),
        home: home.as_deref(),
        profiles: &dt_eval::OnDisk {
            home: home.as_deref(),
        },
        base: &base,
        shelf: shelf.as_ref(),
    };
    let title = title_of(list_path);
    let plan = match plates::plan(&list, &title, &ctx, ov) {
        Ok(p) => p,
        Err(e) => return fail(&e, json),
    };
    let s = &plan.settings;
    let fmt = match format_of(format, &s.machine) {
        Ok(f) => f,
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    };
    if !json {
        tell!(
            "{title}: {} piece(s) of {} tile(s) for the {} ({} x {} mm, {} filament(s)), {}",
            plan.pieces(),
            plan.kinds.len(),
            s.machine.label(),
            s.machine.bed.map_or(0.0, |b| b[0]),
            s.machine.bed.map_or(0.0, |b| b[1]),
            s.machine.slot_count(),
            s.colour.title()
        );
        if let Some(v) = &s.machine.verify {
            tell!("note: this printer's numbers are not yet checked against the maker ({v})");
        }
        let held: u32 = plan.held.iter().map(|h| h.1).sum::<u32>()
            + plan.kinds.iter().map(|k| k.have).sum::<u32>();
        if held > 0 {
            tell!(
                "note: {held} piece{} already in your inventory left off ({} to print): --all prints them too",
                if held == 1 { " is" } else { "s are" },
                plan.pieces()
            );
        }
    }
    if check_only {
        if json {
            crate::say_json(&PlatesChecked {
                ok: true,
                plan: PlanOnly::of(&plan),
            });
        }
        return OK;
    }
    let tpl = if fmt == Format::Bambu {
        match template_for(&s.machine, template, home.as_deref()) {
            Ok(t) => t,
            Err(e) => {
                tell!("error: {e}");
                return ERROR;
            }
        }
    } else {
        None
    };
    if fmt == Format::Bambu && tpl.is_none() {
        // before the builds, not after
        return fail(
            &[Diag::plain(
                dt_lang::codes::PLATES,
                "a Bambu Studio project needs a template for the printer's settings",
            )
            .help("save any project from Bambu Studio for this printer as bambu.3mf in your tiles folder (or name it with --bambu-template), or write one STL per plate with --format stl")],
            json,
        );
    }
    let mut last = Instant::now();
    // every distinct piece in full, through the disk cache when there is one: the one stored-build call the program's lanes use
    let builds = Builds::of(
        &plan,
        &base,
        &dt_eval::OnDisk {
            home: home.as_deref(),
        },
        &mut |n, of, name| {
            if !json {
                tell!(
                    "[{n}/{of}] building {name} ({:.1} s)",
                    last.elapsed().as_secs_f64()
                );
            }
            last = Instant::now();
        },
        &mut |path, opts| {
            dt_store::build_stored(
                &Files,
                path,
                opts,
                &dt_eval::Progress::none(),
                None,
                store.map(dt_store::Store::kept),
            )
        },
    );
    let builds = match builds {
        Ok(b) => b,
        Err(e) => return fail(&e, json),
    };
    let layout = match builds.layout(&plan) {
        Ok(l) => l,
        Err(e) => return fail(&e, json),
    };
    if let Some(n) = layout.grouping_cost
        && !json
    {
        tell!(
            "note: grouping by filament set takes {n} more plate{} than packing by size: run with --group size to pack by size only",
            if n == 1 { "" } else { "s" }
        );
    }
    let gate = builds.gate();
    // a piece that failed the gate is never written: the plates wait for it to pass
    let files = match builds.files(
        &plan,
        &layout,
        fmt,
        palette,
        tpl.as_deref(),
        &list_path.display().to_string(),
    ) {
        Ok(f) => f,
        Err(e) => return fail(&[e], json),
    };
    let passed = files.is_some();
    let files = files.unwrap_or_default();
    let dest = out.join("plates").join(&title);
    if let Err(e) = std::fs::create_dir_all(&dest) {
        tell!("error: cannot create {}: {e}", dest.display());
        return ERROR;
    }
    let mut written = Vec::new();
    for (name, bytes) in &files {
        let p = dest.join(name);
        if let Err(e) = std::fs::write(&p, bytes) {
            tell!("error: cannot write {}: {e}", p.display());
            return ERROR;
        }
        written.push(p);
    }
    let report_json = PlatesReport {
        ok: passed,
        plan: PlanOnly::of(&plan),
        plates: PlanPlate::of_layout(&plan, &layout, &dt_doc::map::piece_ids(&list)),
        grouping_cost: layout.grouping_cost,
        format: fmt.word().to_owned(),
        files: written.iter().map(|f| f.display().to_string()).collect(),
        gate: gate.clone(),
        total_s: t0.elapsed().as_secs_f64(),
    };
    let rec = dest.join(format!("{title}.plates.json"));
    match serde_json::to_string_pretty(&report_json)
        .map_err(|e| e.to_string())
        .and_then(|t| {
            std::fs::write(&rec, t).map_err(|e| format!("cannot write {}: {e}", rec.display()))
        }) {
        Ok(()) => {}
        Err(e) => {
            tell!("error: {e}");
            return ERROR;
        }
    }
    let mut code = OK;
    for (k, f) in gate.iter().enumerate() {
        for line in f {
            code = AUDIT_FAILED;
            tell!("audit failed: {}: {line}", plan.kinds[k].name);
        }
    }
    if json {
        crate::say_json(&report_json);
    } else {
        for (i, p) in layout.plates.iter().enumerate() {
            tell!(
                "plate {}: {} piece(s){}",
                i + 1,
                p.len(),
                if s.colour == Colour::Multicolour {
                    format!(
                        ", filaments {}",
                        layout.filaments[i]
                            .iter()
                            .map(|m| m.name())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                } else {
                    String::new()
                }
            );
        }
        for f in &written {
            tell!("wrote {}", f.display());
        }
        tell!(
            "{} plate(s) in {:.1} s",
            layout.plates.len(),
            t0.elapsed().as_secs_f64()
        );
    }
    code
}

fn fail(errs: &[Diag], json: bool) -> u8 {
    if json {
        crate::say_json(&Refused::of(dt_eval::mistakes(errs)));
    } else {
        report(errs);
    }
    ERROR
}
