//! `dharmatiles check`: read and plan tiles, no geometry.

use dt_eval::{Files, Options};
use serde_json::json;

use crate::{ERROR, OK, errors_json, report};

pub(crate) fn check(paths: &[String], json: bool) -> u8 {
    let mut code = OK;
    let mut reports = Vec::new();
    for p in paths {
        match dt_eval::check(&Files, p, &Options::default()) {
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
                }
                reports.push(json!({
                    "path": p,
                    "ok": true,
                    "tile": c.tile.as_ref().map(|t| format!("{}x{}", t.cols, t.rows)),
                    "pending": errors_json(pending),
                }));
            }
            Err(errs) => {
                code = ERROR;
                reports.push(json!({"path": p, "ok": false, "errors": errors_json(&errs)}));
                if !json {
                    report(&errs);
                }
            }
        }
    }
    if json {
        say!("{}", json!({"ok": code == OK, "tiles": reports}));
    }
    code
}
