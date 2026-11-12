//! `dharmatiles plates`: a print list laid on plates and written as files.
//!
//! The same steps the editor's Export button takes: read the list, plan it
//! (tiles and printer, the filaments checked before anything builds), build
//! every distinct piece in full, lay the plates out by what the builds really
//! cover, write one Bambu project, or one STL or 3MF per plate.

use std::path::{Path, PathBuf};
use std::time::Instant;

use dt_doc::map::{Group, Mode, PrintList};
use dt_eval::plates::export::{self, Format};
use dt_eval::plates::{self, Boxes, Context, Layout, Overrides, Plan};
use dt_eval::{Built, Diag, Files, Options, Store};
use serde_json::{Value, json};

use crate::{AUDIT_FAILED, ERROR, OK, report};

/// Read a print list.
///
/// # Errors
/// The file cannot be read, or does not decode: every mistake at its text.
pub(crate) fn read_list(path: &Path) -> Result<PrintList, Vec<Diag>> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        vec![Diag::plain(
            "dt::source",
            format!("cannot read {}: {e}", path.display()),
        )]
    })?;
    dt_doc::map::decode(&path.display().to_string(), &text)
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
    Ok(dt_eval::inventory::load(home)?.map(|l| dt_doc::inventory::Shelf::of(&l.inventory)))
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
        dt_kit::project_settings(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    dt_kit::bed_centre(&settings).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(settings))
}

/// What the build of every piece gave.
pub(crate) struct Builds {
    pub(crate) built: Vec<Built>,
    /// Each piece's gate failures (empty when it passed).
    pub(crate) failures: Vec<Vec<String>>,
}

/// Build every distinct piece of a plan in full (through the cache when there
/// is one), calling `step` before each with its number, the count and its
/// name.
///
/// # Errors
/// A piece that does not build.
pub(crate) fn build_all(
    plan: &Plan,
    base: &Options,
    home: Option<&Path>,
    store: Option<&Store>,
    step: &mut dyn FnMut(usize, usize, &str),
) -> Result<Builds, Vec<Diag>> {
    let mut built = Vec::new();
    let mut failures = Vec::new();
    for (k, kind) in plan.kinds.iter().enumerate() {
        step(k + 1, plan.kinds.len(), &kind.name);
        let opts = plan.options(k, base, home)?;
        let b = dt_eval::build_tier(
            &Files,
            &kind.path,
            &opts,
            dt_eval::Tier::Full,
            &dt_eval::Progress::none(),
            None,
            store,
        )?;
        failures.push(if b.checked {
            b.audit.failures(&opts.printer)
        } else {
            Vec::new()
        });
        built.push(b);
    }
    Ok(Builds { built, failures })
}

/// The layout the builds give.
///
/// # Errors
/// A piece that does not fit, or needs too many filaments.
pub(crate) fn layout_of(plan: &Plan, builds: &Builds) -> Result<Layout, Vec<Diag>> {
    let boxes: Vec<Boxes> = builds
        .built
        .iter()
        .map(|b| Boxes::of(&b.export.mesh, &b.materials))
        .collect();
    plates::layout(plan, &boxes)
}

/// The settings words a caller may give, as the command line's.
#[allow(clippy::too_many_arguments)]
pub(crate) fn overrides(
    printer: Option<String>,
    system: Option<String>,
    mode: Option<&str>,
    group: Option<&str>,
    gap: Option<f64>,
    margin: Option<f64>,
    turn: Option<&str>,
    all: bool,
) -> Result<Overrides, String> {
    let mode = match mode {
        None => None,
        Some("single") => Some(Mode::Single),
        Some("multicolour") => Some(Mode::Multicolour),
        Some(m) => {
            return Err(format!(
                "`{m}` is not a mode: use `single` or `multicolour`"
            ));
        }
    };
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
    if let Some(s) = &system
        && !matches!(s.as_str(), "db" | "ol")
    {
        return Err(format!("`{s}` is not a system: use `db` or `ol`"));
    }
    Ok(Overrides {
        printer,
        system,
        mode,
        group,
        gap,
        margin,
        turn,
        inventory: all.then_some(false),
    })
}

/// The format a word names.
pub(crate) fn format_of(word: Option<&str>, machine: &dt_eval::Machine) -> Result<Format, String> {
    match word {
        None => Ok(Format::for_printer(machine)),
        Some("bambu") => Ok(Format::Bambu),
        Some("stl") => Ok(Format::Stl),
        Some("3mf") => Ok(Format::ThreeMf),
        Some(w) => Err(format!(
            "`{w}` is not a format: use `bambu`, `stl` or `3mf`"
        )),
    }
}

/// `plan` then, unless `check_only`, everything to the files.
#[allow(clippy::too_many_arguments)]
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
    let base = match crate::library::options("db", "default", None) {
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
                "dt::inventory",
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
            match s.mode {
                Mode::Single => "single colour",
                Mode::Multicolour => "multicolour",
            }
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
            say!("{}", json!({"ok": true, "plan": plan_json(&plan)}));
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
                "dt::plates",
                "a Bambu Studio project needs a template for the printer's settings",
            )
            .help("save any project from Bambu Studio for this printer as bambu.3mf in your tiles folder (or name it with --bambu-template), or write one STL per plate with --format stl")],
            json,
        );
    }
    let mut last = Instant::now();
    let builds = build_all(&plan, &base, home.as_deref(), store, &mut |n, of, name| {
        if !json {
            tell!(
                "[{n}/{of}] building {name} ({:.1} s)",
                last.elapsed().as_secs_f64()
            );
        }
        last = Instant::now();
    });
    let builds = match builds {
        Ok(b) => b,
        Err(e) => return fail(&e, json),
    };
    let layout = match layout_of(&plan, &builds) {
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
    let b0 = &builds.built[0];
    let record = dt_kit::BuildRecord {
        engine: b0.record.engine.clone(),
        document: list_path.display().to_string(),
        document_hash: b0.record.document_hash,
        seed: 0,
        system: s.system.clone(),
        printer: s.machine.name.clone(),
    };
    let refs: Vec<&Built> = builds.built.iter().collect();
    let files =
        match export::write_set(&plan, &layout, &refs, fmt, palette, tpl.as_deref(), &record) {
            Ok(f) => f,
            Err(e) => return fail(&[e], json),
        };
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
    let report_json = json!({
        "ok": true,
        "plan": plan_json(&plan),
        "plates": layout_json(&plan, &layout),
        "grouping_cost": layout.grouping_cost,
        "format": fmt.word(),
        "files": written.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
        "gate": builds.failures,
        "total_s": t0.elapsed().as_secs_f64(),
    });
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
    for (k, f) in builds.failures.iter().enumerate() {
        for line in f {
            code = AUDIT_FAILED;
            tell!("audit failed: {}: {line}", plan.kinds[k].name);
        }
    }
    if json {
        say!("{report_json}");
    } else {
        for (i, p) in layout.plates.iter().enumerate() {
            tell!(
                "plate {}: {} piece(s){}",
                i + 1,
                p.len(),
                if s.mode == Mode::Multicolour {
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
        say!(
            "{}",
            json!({"ok": false, "errors": crate::errors_json(errs)})
        );
    } else {
        report(errs);
    }
    ERROR
}

/// The plan as JSON.
pub(crate) fn plan_json(plan: &Plan) -> Value {
    let s = &plan.settings;
    json!({
        "title": plan.title,
        "printer": s.machine.name,
        "label": s.machine.label(),
        "bed": s.machine.bed,
        "excluded": s.machine.excluded,
        "slots": s.machine.slot_count(),
        "verify": s.machine.verify,
        "system": s.system,
        "mode": s.mode.word(),
        "group": s.group.word(),
        "gap": s.gap,
        "margin": s.margin,
        "turn": s.turn,
        "kinds": plan.kinds.iter().map(|k| json!({
            "tile": k.tile, "name": k.name, "path": k.path, "seed": k.seed,
            "system": k.system, "count": k.count, "want": k.want, "have": k.have,
            "other_colour": k.other_colour, "canon": k.canon, "seed_used": k.seed_used,
            "hash": k.hash, "nominal": k.nominal,
            "estimate": k.estimate.iter().map(|m| m.name()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "skipped": plan.skipped,
        "held": plan.held.iter().map(|(n, c)| json!({"name": n, "have": c})).collect::<Vec<_>>(),
        "check_inventory": s.check_inventory,
    })
}

/// A layout as JSON: each plate's pieces with their places.
pub(crate) fn layout_json(plan: &Plan, layout: &Layout) -> Value {
    Value::Array(
        layout
            .plates
            .iter()
            .enumerate()
            .map(|(i, on)| {
                json!({
                    "plate": i + 1,
                    "filaments": layout.filaments[i].iter().map(|m| m.name()).collect::<Vec<_>>(),
                    "pieces": on.iter().map(|p| json!({
                        "kind": p.kind, "copy": p.copy, "name": plan.kinds[p.kind].name,
                        "x": p.at.x, "y": p.at.y, "w": p.at.w, "d": p.at.d, "turned": p.at.turned,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}
