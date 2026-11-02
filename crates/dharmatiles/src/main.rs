//! `dharmatiles`: the command-line program.
//!
//! Exit codes (layout.html, "The command line"):
//!
//! | code | meaning |
//! |---|---|
//! | 0 | success |
//! | 1 | error: a bad document or a failed build (nothing written for it) |
//! | 2 | usage error (clap's) |
//! | 3 | built, but the audit gate failed: the STL is still written, so you can look at it (overhangs red with `--png`) |
//!
//! Results go to stdout (`--json` prints one JSON report); progress and
//! errors go to stderr, errors with miette: the line, a caret under the
//! mistake, and a hint.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};
use dt_eval::{Built, Diag, Files, Options};
use dt_kit::{VIEWS, paint, render, stl};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "dharmatiles",
    version = env!("DT_ENGINE_VERSION"),
    about = "Build printable terrain tiles from KDL documents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build tiles to STL, through the printability gate.
    Build {
        /// Tile documents: files, or `lib:tiles/…` in the bundled library.
        #[arg(required = true)]
        paths: Vec<String>,
        /// Connector system.
        #[arg(long, default_value = "db")]
        system: String,
        /// Replace each tile's seed.
        #[arg(long)]
        seed: Option<u64>,
        /// Output folder (default: `out/` beside `dharmatiles.kdl`, else here).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write the four preview views as PNG, overhangs red.
        #[arg(long)]
        png: bool,
        /// One JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Read and plan tiles without building geometry.
    Check {
        /// Tile documents.
        #[arg(required = true)]
        paths: Vec<String>,
        /// One JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
}

const OK: u8 = 0;
const ERROR: u8 = 1;
const AUDIT_FAILED: u8 = 3;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Check { paths, json } => check(&paths, json),
        Command::Build {
            paths,
            system,
            seed,
            out,
            png,
            json,
        } => {
            let opts = Options {
                system,
                seed,
                engine: env!("DT_ENGINE_VERSION").to_owned(),
                ..Options::default()
            };
            let out = out.unwrap_or_else(|| {
                let cwd = std::env::current_dir().unwrap_or_default();
                dt_eval::discover(&cwd).out
            });
            build(&paths, &opts, &out, png, json)
        }
    };
    ExitCode::from(code)
}

fn check(paths: &[String], json: bool) -> u8 {
    let mut code = OK;
    let mut reports = Vec::new();
    for p in paths {
        match dt_eval::check(&Files, p, &Options::default()) {
            Ok(c) => {
                let pending = c.pending();
                if !json {
                    match &c.tile {
                        Some(t) if pending.is_empty() => {
                            eprintln!("ok {p}: tile `{}`, {} stack entries", t.name, t.stack.len());
                        }
                        Some(t) => {
                            eprintln!(
                                "ok {p}: tile `{}`; {} part(s) need looks not built yet, so `build` refuses it:",
                                t.name,
                                pending.len()
                            );
                            for d in pending {
                                let at = d
                                    .location()
                                    .map(|(f, l, c)| format!("{f}:{l}:{c}: "))
                                    .unwrap_or_default();
                                eprintln!("  {at}{}", d.message);
                            }
                        }
                        None => eprintln!(
                            "ok {p}: {} definitions, no tile",
                            c.file.doc.definitions.len()
                        ),
                    }
                }
                reports.push(json!({
                    "path": p,
                    "ok": true,
                    "tile": c.tile.as_ref().map(|t| t.name.clone()),
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
        println!("{}", json!({"ok": code == OK, "tiles": reports}));
    }
    code
}

fn build(paths: &[String], opts: &Options, out: &Path, png: bool, json: bool) -> u8 {
    let mut code = OK;
    let mut reports = Vec::new();
    for p in paths {
        let t0 = Instant::now();
        let b = match dt_eval::build(&Files, p, opts) {
            Ok(b) => b,
            Err(errs) => {
                code = ERROR;
                reports.push(json!({"path": p, "ok": false, "errors": errors_json(&errs)}));
                if !json {
                    report(&errs);
                }
                continue;
            }
        };
        let built_s = t0.elapsed().as_secs_f64();
        let failures = b.audit.failures(&opts.printer);
        let files = match write(&b, &failures, built_s, out, png) {
            Ok(f) => f,
            Err(e) => {
                code = ERROR;
                eprintln!("error: {p}: {e}");
                reports.push(json!({"path": p, "ok": false, "errors": [{"code": "dt::write", "message": e}]}));
                continue;
            }
        };
        let total_s = t0.elapsed().as_secs_f64();
        if !failures.is_empty() && code == OK {
            code = AUDIT_FAILED;
        }
        let a = &b.audit;
        if !json {
            eprintln!(
                "built {p} → {}\n  {} triangles · watertight {} · overhang {:.2} mm² · {} dropped · {} holes · build {:.2} s, written {:.2} s",
                files[0].display(),
                a.triangles,
                a.watertight,
                a.overhang_mm2,
                a.dropped.len(),
                a.holes,
                built_s,
                total_s
            );
            for f in &failures {
                eprintln!("  audit failed: {f}");
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
        reports.push(r);
    }
    if json {
        println!("{}", json!({"ok": code == OK, "tiles": reports}));
    }
    code
}

/// Write the STL, its build record (`.build.json`) and, with `png`, the
/// four views.
fn write(
    b: &Built,
    failures: &[String],
    built_s: f64,
    out: &Path,
    png: bool,
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
    if png {
        let colours = paint(&b.materials, &b.audit.overhang);
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

fn tile_json(b: &Built, failures: &[String], built_s: f64) -> Value {
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
        },
        "audit": {
            "triangles": a.triangles,
            "vertices": a.vertices,
            "volume_mm3": a.volume_mm3,
            "watertight": a.watertight,
            "bodies": a.bodies,
            "dropped": a.dropped.iter().map(|d| json!({"triangles": d.triangles, "volume_mm3": d.volume_mm3})).collect::<Vec<_>>(),
            "pinches_moved": a.pinches_moved,
            "collapsed": a.collapsed,
            "overhang_mm2": a.overhang_mm2,
            "holes": a.holes,
            "failures": failures,
        },
        "stages": b.stages.iter().map(|s| json!({"name": s.name, "hash": format!("{:016x}", s.hash)})).collect::<Vec<_>>(),
        "build_s": built_s,
    })
}

fn errors_json(errs: &[Diag]) -> Vec<Value> {
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

fn report(errs: &[Diag]) {
    for e in errs {
        eprintln!("{:?}", miette::Report::new(e.clone()));
    }
}
