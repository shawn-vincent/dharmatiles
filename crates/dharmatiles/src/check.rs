//! `dharmatiles check`: read and plan tiles, no geometry.

use crate::cli_wire::{CheckReport, CheckTile, Checked, Failed};
use dt_eval::{Files, Options};

use crate::{ERROR, OK, report};

pub(crate) fn check(paths: &[String], opts: &Options, json: bool, explain: bool) -> u8 {
    let mut code = OK;
    let mut reports = Vec::new();
    for p in paths {
        match dt_eval::check(&Files, p, opts) {
            Ok(c) => {
                let pending = c.pending();
                if !json {
                    match &c.tile {
                        Some(t) if pending.is_empty() => {
                            tell!(
                                "ok {p}: {}x{} tile, {} stack entries",
                                t.cols,
                                t.rows,
                                t.stack.len()
                            );
                        }
                        Some(t) => {
                            tell!(
                                "ok {p}: {}x{} tile; {} part(s) need elements not built yet, so `build` refuses it:",
                                t.cols,
                                t.rows,
                                pending.len()
                            );
                            for d in pending {
                                let at = d
                                    .location()
                                    .map(|(f, l, c)| format!("{f}:{l}:{c}: "))
                                    .unwrap_or_default();
                                tell!("  {at}{}", d.message);
                            }
                        }
                        None => tell!(
                            "ok {p}: {} definitions, no tile",
                            c.file.doc.definitions.len()
                        ),
                    }
                    for d in c.notes() {
                        let at = d
                            .location()
                            .map(|(f, l, c)| format!("{f}:{l}:{c}: "))
                            .unwrap_or_default();
                        tell!("  note [{}] {at}{}", d.code, d.message);
                        if let Some(h) = d.help.as_deref().filter(|h| !h.trim().is_empty()) {
                            tell!("    help: {h}");
                        }
                    }
                    if explain {
                        for line in c.explain() {
                            tell!("  {line}");
                        }
                    }
                }
                reports.push(CheckTile::Checked(Checked {
                    path: p.clone(),
                    ok: true,
                    tile: c.tile.as_ref().map(|t| format!("{}x{}", t.cols, t.rows)),
                    pending: dt_eval::mistakes(pending),
                    notes: dt_eval::mistakes(c.notes()),
                    explain: explain.then(|| c.explain().to_vec()),
                }));
            }
            Err(errs) => {
                code = ERROR;
                reports.push(CheckTile::Failed(Failed::of(p, dt_eval::mistakes(&errs))));
                if !json {
                    report(&errs);
                }
            }
        }
    }
    if json {
        crate::say_json(&CheckReport {
            ok: code == OK,
            tiles: reports,
        });
    }
    code
}
