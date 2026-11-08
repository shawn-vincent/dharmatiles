//! `dharmatiles build`: build tiles through the audit gate and write them.

use std::path::{Path, PathBuf};
use std::time::Instant;

use dt_eval::{Built, Diag, Files, Options, Source};
use dt_kit::{VIEWS, render, stl};
use serde_json::{Value, json};

use crate::{AUDIT_FAILED, ERROR, OK, progress};

/// What else a build writes beside the STL.
#[derive(Clone, Copy)]
pub(crate) struct Extras<'a> {
    /// Show a progress line on stderr (a terminal, no `--json`).
    pub(crate) progress: bool,
    /// This tile's place in a batch, shown beside the bar.
    pub(crate) tile_no: Option<(usize, usize)>,
    pub(crate) png: bool,
    pub(crate) threemf: bool,
    /// A Bambu Studio template's project settings: the 3MF is a Bambu
    /// project.
    pub(crate) template: Option<&'a str>,
    pub(crate) palette: &'a dt_kit::Palette,
    /// The scenes the tile's last build kept (`watch`), to start from.
    pub(crate) cache: Option<&'a dt_eval::Cache>,
}

pub(crate) fn build(
    paths: &[String],
    opts: &Options,
    out: &Path,
    extras: Extras,
    json: bool,
) -> u8 {
    let mut code = OK;
    let mut reports = Vec::new();
    for (n, p) in paths.iter().enumerate() {
        let extras = Extras {
            tile_no: (paths.len() > 1).then_some((n + 1, paths.len())),
            ..extras
        };
        let (c, r) = build_one(&Files, p, opts, out, extras, json);
        code = worse(code, c);
        reports.push(r);
    }
    if json {
        say!("{}", json!({"ok": code == OK, "tiles": reports}));
    }
    code
}

/// The worse of two exit codes: an error beats a failed audit beats success.
pub(crate) fn worse(a: u8, b: u8) -> u8 {
    if a == ERROR || b == ERROR {
        ERROR
    } else {
        a.max(b)
    }
}

/// Build one tile and write its files; report it on stderr unless `json`.
/// Returns its exit code and its JSON report.
pub(crate) fn build_one(
    sources: &dyn Source,
    p: &str,
    opts: &Options,
    out: &Path,
    extras: Extras,
    json: bool,
) -> (u8, Value) {
    let t0 = Instant::now();
    let mut timings = dt_eval::progress::Timings::load(&dt_eval::progress::Timings::file_in(out));
    let tracker = extras
        .progress
        .then(|| {
            let prefix = extras
                .tile_no
                .map_or_else(String::new, |(n, t)| format!("[{n}/{t}] "));
            let name = p
                .rsplit(['/', ':'])
                .next()
                .unwrap_or(p)
                .trim_end_matches(".dtile");
            let write = dt_eval::progress::StageInfo::new(
                progress::WRITE,
                "writing files",
                0.3,
                p,
                &opts.system,
                &timings,
            );
            progress::tracker_for(
                sources,
                p,
                opts,
                &timings,
                vec![write],
                progress::tty_sink(prefix, name.to_owned()),
            )
        })
        .flatten();
    let reporter = tracker
        .as_ref()
        .map_or_else(dt_eval::Progress::none, |(_, pr)| pr.clone());
    let b = match dt_eval::build_cached(sources, p, opts, &reporter, extras.cache) {
        Ok(b) => b,
        Err(errs) => {
            if tracker.is_some() {
                progress::clear();
            }
            if !json {
                report(&errs);
            }
            return (
                ERROR,
                json!({"path": p, "ok": false, "errors": errors_json(&errs)}),
            );
        }
    };
    let built_s = t0.elapsed().as_secs_f64();
    let failures = b.audit.failures(&opts.printer);
    reporter.begin(progress::WRITE, "writing files");
    let t_write = Instant::now();
    let written = write(&b, &failures, built_s, out, extras);
    reporter.end(progress::WRITE);
    if let Some((t, _)) = &tracker {
        t.finish();
        progress::clear();
    }
    let files = match written {
        Ok(f) => f,
        Err(e) => {
            tell!("error: {p}: {e}");
            return (
                ERROR,
                json!({"path": p, "ok": false, "errors": errors_json(&[Diag::plain("dt::write", e)])}),
            );
        }
    };
    let total_s = t0.elapsed().as_secs_f64();
    let mut secs = b.stage_secs.clone();
    secs.push((progress::WRITE.to_owned(), t_write.elapsed().as_secs_f64()));
    timings.record(p, &opts.system, &secs);
    timings.save();
    let a = &b.audit;
    if !json {
        tell!(
            "built {p} → {}\n  {} triangles · watertight {} · overhang (over 65°) {:.2}/{:.2} mm² · band (55–65°) {:.2}/{:.2} mm² · {} dropped · {} voids filled · {} holes · build {:.2} s, written {:.2} s",
            files[0].display(),
            a.triangles,
            a.watertight,
            a.overhang_mm2,
            a.overhang_budget_mm2,
            a.band_mm2,
            a.band_budget_mm2,
            a.dropped.len(),
            a.voids.len(),
            a.holes,
            built_s,
            total_s
        );
        if b.reused > 0 {
            tell!("  {} steps kept from the last build", b.reused);
        }
        for f in &failures {
            tell!("  audit failed: {f}");
        }
    }
    let mut r = tile_json(&b, &failures, built_s);
    r["files"] = json!(
        files
            .iter()
            .map(|f| f.display().to_string())
            .collect::<Vec<_>>()
    );
    r["total_s"] = json!(total_s);
    let code = if failures.is_empty() {
        OK
    } else {
        AUDIT_FAILED
    };
    (code, r)
}

/// Write the STL, its build record (`.build.json`) and, with `png`, the
/// four views.
pub(crate) fn write(
    b: &Built,
    failures: &[String],
    built_s: f64,
    out: &Path,
    extras: Extras,
) -> Result<Vec<PathBuf>, String> {
    let path = out.join(&b.out);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let w = |p: &Path, bytes: &[u8]| {
        std::fs::write(p, bytes).map_err(|e| format!("cannot write {}: {e}", p.display()))
    };
    w(&path, &stl(&b.export.mesh, &b.record))?;
    let mut files = vec![path.clone()];
    let record = path.with_extension("build.json");
    let text = serde_json::to_string_pretty(&tile_json(b, failures, built_s))
        .map_err(|e| e.to_string())?;
    w(&record, text.as_bytes())?;
    files.push(record);
    if extras.threemf {
        let p = path.with_extension("3mf");
        let bytes = match extras.template {
            Some(t) => {
                dt_kit::bambu_project(&b.export.mesh, &b.materials, &b.record, extras.palette, t)?
            }
            None => dt_kit::threemf(&b.export.mesh, &b.materials, &b.record, extras.palette),
        };
        w(&p, &bytes)?;
        files.push(p);
    }
    if extras.png {
        let colours = b.colours_with(extras.palette, true);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        for v in &VIEWS {
            let img = render(&b.export.mesh, &colours, v, b.footprint, (1200, 950));
            let p = path.with_file_name(format!("{stem}-{}.png", v.name));
            w(&p, &img.png()?)?;
            files.push(p);
        }
    }
    Ok(files)
}

pub(crate) fn tile_json(b: &Built, failures: &[String], built_s: f64) -> Value {
    let a = &b.audit;
    json!({
        "path": b.record.document,
        "ok": failures.is_empty(),
        "record": {
            "engine": b.record.engine,
            "document": b.record.document,
            "document_hash": format!("{:016x}", b.record.document_hash),
            "seed": b.record.seed,
            "system": b.record.system,
            "printer": b.record.printer,
            "target": b.record.target(),
        },
        "audit": {
            "triangles": a.triangles,
            "vertices": a.vertices,
            "volume_mm3": a.volume_mm3,
            "watertight": a.watertight,
            "bodies": a.bodies,
            "dropped": a.dropped.iter().map(|d| json!({"triangles": d.triangles, "volume_mm3": d.volume_mm3, "bounds": d.bounds})).collect::<Vec<_>>(),
            "voids_filled": a.voids.len(),
            "slivers": a.slivers.len(),
            "band_mm2": a.band_mm2,
            "band_budget_mm2": a.band_budget_mm2,
            "pinches_moved": a.pinches_moved,
            "collapsed": a.collapsed,
            "overhang_mm2": a.overhang_mm2,
            "overhang_budget_mm2": a.overhang_budget_mm2,
            "holes": a.holes,
            "failures": failures,
        },
        "stages": b.stages.iter().map(|s| json!({"name": s.name, "hash": format!("{:016x}", s.hash)})).collect::<Vec<_>>(),
        "build_s": built_s,
        "stage_secs": b.stage_secs.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>(),
        "edge_mm": b.edge_mm,
        "stats": stats(&b.notes),
    })
}

pub(crate) fn errors_json(errs: &[Diag]) -> Vec<Value> {
    errs.iter()
        .map(|d| {
            let (file, line, column) = d.location().map_or(
                (Value::Null, Value::Null, Value::Null),
                |(f, l, c)| (json!(f), json!(l), json!(c)),
            );
            json!({"code": d.code, "message": d.message, "file": file, "line": line, "column": column, "help": d.help})
        })
        .collect()
}

pub(crate) fn report(errs: &[Diag]) {
    for e in errs {
        tell!("{:?}", miette::Report::new(e.clone()));
    }
}

/// The elements' notes as one JSON object. An element used twice notes the same
/// names twice: the second is keyed `name#2`, the third `name#3`, so no
/// entry's numbers overwrite another's.
pub(crate) fn stats(notes: &[(String, f64)]) -> serde_json::Map<String, Value> {
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    notes
        .iter()
        .map(|(k, v)| {
            let n = seen.entry(k.as_str()).or_default();
            *n += 1;
            let key = if *n == 1 {
                k.clone()
            } else {
                format!("{k}#{n}")
            };
            (key, json!(v))
        })
        .collect()
}
