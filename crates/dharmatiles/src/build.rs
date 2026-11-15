//! `dharmatiles build`: build tiles through the audit gate and write them.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cli_wire::{BuildReport, BuildTile, Failed, Printed};
use dt_eval::{Built, Diag, Files, Options, Source, Verdict};
use dt_print::{VIEWS, render, stl};

use crate::{AUDIT_FAILED, BUG, ERROR, OK, progress};

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
    /// The disk cache of builds: a build whose inputs are all unchanged is
    /// read from it, not made (`--no-cache`: none).
    pub(crate) store: Option<&'a dt_store::Store>,
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
    let mut failed: Vec<&str> = Vec::new();
    for (n, p) in paths.iter().enumerate() {
        let extras = Extras {
            tile_no: (paths.len() > 1).then_some((n + 1, paths.len())),
            ..extras
        };
        // A panic in one tile is a bug, but it must not end the sweep: it is
        // reported against the tile by name, and the next tile builds.
        let (c, r) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dt_crash_switch::panic_in_tile(p);
            build_one(&Files, p, opts, out, extras, json)
        }))
        .unwrap_or_else(|panic| {
            let msg = crate::preview::panic_message(panic.as_ref());
            progress::clear();
            tell!("error: {p}: the build panicked (a bug in dharmatiles, not in the tile): {msg}");
            (
                BUG,
                BuildTile::Failed(Failed::of(
                    p,
                    dt_eval::mistakes(&[Diag::plain(
                        dt_lang::codes::PANIC,
                        format!("the build panicked: {msg}"),
                    )]),
                )),
            )
        });
        if c != OK {
            failed.push(p);
        }
        code = worse(code, c);
        reports.push(r);
    }
    if !json && paths.len() > 1 && !failed.is_empty() {
        tell!(
            "{} of {} tiles failed: {}",
            failed.len(),
            paths.len(),
            failed.join(", ")
        );
    }
    if json {
        crate::say_json(&BuildReport {
            ok: code == OK,
            tiles: reports,
        });
    }
    code
}

/// The worse of two exit codes: a bug (101) beats an error (1) beats a failed
/// audit (3) beats success (0).
pub(crate) fn worse(a: u8, b: u8) -> u8 {
    let rank = |c: u8| match c {
        BUG => 3,
        ERROR => 2,
        AUDIT_FAILED => 1,
        _ => 0,
    };
    if rank(b) > rank(a) { b } else { a }
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
) -> (u8, BuildTile) {
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
                opts.system.word(),
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
    let b = match dt_store::build_stored(sources, p, opts, &reporter, extras.cache, extras.store) {
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
                BuildTile::Failed(Failed::of(p, dt_eval::mistakes(&errs))),
            );
        }
    };
    let built_s = t0.elapsed().as_secs_f64();
    let record = dt_eval::TileRecord::of(&b, &opts.printer, built_s);
    let verdict = b.verdict(&opts.printer);
    reporter.begin(progress::WRITE, "writing files");
    let t_write = Instant::now();
    let written = write(&b, &verdict, &record, out, extras, opts.tier);
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
                BuildTile::Failed(Failed::of(
                    p,
                    dt_eval::mistakes(&[Diag::plain(dt_lang::codes::WRITE, e)]),
                )),
            );
        }
    };
    let total_s = t0.elapsed().as_secs_f64();
    let mut secs = b.report.stage_secs.clone();
    secs.push((progress::WRITE.to_owned(), t_write.elapsed().as_secs_f64()));
    timings.record(p, opts.system.word(), &secs);
    timings.save();
    let a = &record.audit;
    if !json && matches!(verdict, Verdict::NotChecked) {
        tell!(
            "built {p} → {}\n  {} · not yet checked · {} triangles · build {:.2} s, written {:.2} s",
            files[0].display(),
            opts.tier.name(),
            a.triangles,
            built_s,
            total_s
        );
    } else if !json {
        tell!(
            "built {p} → {}\n  {} triangles · watertight {} · overhang (over 65°) {:.2}/{:.2} mm² · band (55–65°) {:.2}/{:.2} mm² · {} dropped · {} voids filled · {} holes · build {:.2} s, written {:.2} s",
            files[0].display(),
            a.triangles,
            a.watertight,
            a.overhang_mm2,
            a.overhang_budget_mm2,
            a.band_mm2,
            a.band_budget_mm2,
            a.repairs.dropped,
            a.repairs.voids_filled,
            a.holes,
            built_s,
            total_s
        );
        if !b.report.left_out.is_empty() {
            tell!("  left out: {}", left_out_text(&b.report.left_out));
        }
        if b.report.reused > 0 {
            tell!("  {} steps kept from the last build", b.report.reused);
        }
        if b.report.cache.state == dt_eval::CacheState::Hit {
            let made: f64 = b.report.stage_secs.iter().map(|s| s.1).sum();
            tell!(
                "  from the cache (made in {made:.2} s by {})",
                b.report
                    .cache
                    .made_by
                    .as_deref()
                    .unwrap_or("an earlier run")
            );
        }
        for f in verdict.reasons() {
            tell!("  audit failed: {f}");
        }
    }
    for problem in &b.report.cache.problems {
        tell!("warning: {p}: {problem}");
    }
    let ok = !matches!(verdict, Verdict::Failed(_));
    let code = if ok { OK } else { AUDIT_FAILED };
    (
        code,
        BuildTile::Printed(Box::new(Printed {
            ok,
            files: files.iter().map(|f| f.display().to_string()).collect(),
            total_s,
            record,
        })),
    )
}

/// Write the STL, its build record (`.build.json`) and, with `png`, the
/// four views.
pub(crate) fn write(
    b: &Built,
    verdict: &Verdict,
    record: &dt_eval::TileRecord,
    out: &Path,
    extras: Extras,
    tier: dt_store::Tier,
) -> Result<Vec<PathBuf>, String> {
    let path = out.join(&b.out);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let w = |p: &Path, bytes: &[u8]| {
        std::fs::write(p, bytes).map_err(|e| format!("cannot write {}: {e}", p.display()))
    };
    // only a build that passed the gate is exported; a low or failed one writes its views (and, once
    // checked, its record) and nothing a printer reads
    let mut files = Vec::new();
    let beside = path.with_extension("build.json");
    let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
    if let Verdict::Passed(passed) = verdict {
        w(&path, &stl(passed))?;
        files.push(path.clone());
    }
    if !matches!(verdict, Verdict::NotChecked) {
        w(&beside, text.as_bytes())?;
        files.push(beside);
    }
    if let (Verdict::Passed(passed), true) = (verdict, extras.threemf) {
        let p = path.with_extension("3mf");
        let bytes = match extras.template {
            Some(t) => dt_print::bambu_project(passed, extras.palette, t)?,
            None => dt_print::threemf(passed, extras.palette),
        };
        w(&p, &bytes)?;
        files.push(p);
    }
    if extras.png {
        let colours = b.colours_with(extras.palette, true);
        let mut stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if matches!(verdict, Verdict::NotChecked) {
            stem = format!("{stem}-{}", tier.name());
        }
        for v in &VIEWS {
            let img = render(&b.export.mesh, &colours, v, b.footprint, (1200, 950));
            let p = path.with_file_name(format!("{stem}-{}.png", v.name));
            w(&p, &img.png()?)?;
            files.push(p);
        }
    }
    Ok(files)
}

/// `cliff 2, tree 1`: what elements left out, for the summary.
fn left_out_text(left_out: &[(String, usize)]) -> String {
    left_out
        .iter()
        .map(|(e, n)| format!("{e} {n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) use dt_eval::is_document_error;

pub(crate) fn report(errs: &[Diag]) {
    for e in errs {
        tell!("{:?}", miette::Report::new(e.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worst_outcome_wins() {
        assert_eq!(worse(OK, AUDIT_FAILED), AUDIT_FAILED);
        assert_eq!(worse(AUDIT_FAILED, ERROR), ERROR);
        assert_eq!(worse(ERROR, AUDIT_FAILED), ERROR);
        assert_eq!(worse(ERROR, BUG), BUG);
        assert_eq!(worse(BUG, AUDIT_FAILED), BUG);
        assert_eq!(worse(BUG, OK), BUG);
        assert_eq!(worse(OK, OK), OK);
    }
}
