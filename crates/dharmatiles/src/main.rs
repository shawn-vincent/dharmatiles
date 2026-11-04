//! `dharmatiles`: the command-line program.
//!
//! Exit codes (docs/rust/architecture.html, "The command line"):
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

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};
use dt_eval::{Built, Diag, Files, Options, Source};
use dt_kit::{VIEWS, render, stl};
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
        #[arg(required_unless_present = "all")]
        paths: Vec<String>,
        /// Build every tile of the bundled library (not the test tiles).
        #[arg(long)]
        all: bool,
        /// Connector system: `db`, `ol`, or `both`.
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
        /// Also write a 3MF with each triangle's material (earth, rock,
        /// water, grass, …) assigned as a 3MF base material.
        #[arg(long = "3mf")]
        threemf: bool,
        /// A palette document (one `material "#rrggbb"` a line) to paint
        /// the 3MF with, instead of the default colours.
        #[arg(long)]
        palette: Option<PathBuf>,
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
    /// List the built-in looks, or one look's controls.
    Looks {
        /// A look's name: list its controls with units, ranges and defaults.
        name: Option<String>,
        /// One JSON object on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Serve the Model Context Protocol on stdio (check, build, preview and
    /// audit tiles for an agent).
    Mcp {
        /// Serve Streamable HTTP on this address (e.g. 127.0.0.1:8787) instead of stdio.
        #[arg(long)]
        http: Option<String>,
    },
    /// Serve the language server protocol on stdio (diagnostics, hover,
    /// completion for tile documents).
    Lsp,
    /// Run the printability gate on a binary STL file.
    Audit {
        /// The STL file.
        path: String,
        /// The tile's width in mm, for see-through holes (default: the mesh's).
        #[arg(long)]
        width: Option<f64>,
        /// The tile's height in mm (default: the mesh's).
        #[arg(long)]
        height: Option<f64>,
        /// One JSON report on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Open a live, painted 3D view of tiles in the browser (127.0.0.1
    /// only).  Overhangs are red, a tree's 55-65 degree band orange; a tile
    /// rebuilds when you save it or a file it uses.  Ctrl-C stops.
    Preview {
        /// Tile documents: files, or `lib:tiles/…`.  None: the whole bundled
        /// library is listed, each tile built when you choose it.
        paths: Vec<String>,
        /// Connector system: `db` or `ol`.
        #[arg(long, default_value = "db")]
        system: String,
        /// Port on 127.0.0.1 (default: the first free one from 7878).
        #[arg(long)]
        port: Option<u16>,
        /// Open the browser.
        #[arg(long)]
        open: bool,
    },
    /// Build tiles, then rebuild each whenever it or a file it uses changes.
    /// Ctrl-C stops.
    Watch {
        /// Tile documents: files, or `lib:tiles/…` (built once; the library
        /// is inside the program).
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
        /// Stop after this many builds (for tests).
        #[arg(long, hide = true)]
        max_builds: Option<usize>,
    },
}

/// One line to stdout; see [`emit`].
macro_rules! say {
    ($($t:tt)*) => { $crate::emit(&mut std::io::stdout().lock(), format_args!($($t)*)) };
}

/// One line to stderr; see [`emit`].
macro_rules! tell {
    ($($t:tt)*) => { $crate::emit(&mut std::io::stderr().lock(), format_args!($($t)*)) };
}

/// Write one line.  When the reader has gone (`dharmatiles check … | head`)
/// the pipe is closed: stop quietly with 0, as `cat` does, instead of
/// panicking (101).  Any other write error is ignored, as there is
/// nowhere left to report it.
fn emit(w: &mut dyn Write, args: std::fmt::Arguments) {
    if let Err(e) = w.write_fmt(args).and_then(|()| w.write_all(b"\n"))
        && e.kind() == std::io::ErrorKind::BrokenPipe
    {
        std::process::exit(0);
    }
}

mod looks;
mod lsp;
mod mcp;
mod preview;
mod watch;

const OK: u8 = 0;
const ERROR: u8 = 1;
const AUDIT_FAILED: u8 = 3;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Check { paths, json } => check(&paths, json),
        Command::Build {
            paths,
            all,
            system,
            seed,
            out,
            png,
            threemf,
            palette,
            json,
        } => {
            let palette = match palette.as_ref().map(|p| {
                std::fs::read_to_string(p)
                    .map_err(|e| format!("cannot read {}: {e}", p.display()))
                    .and_then(|t| dt_kit::Palette::parse(&t))
            }) {
                None => dt_kit::Palette::default(),
                Some(Ok(p)) => p,
                Some(Err(e)) => {
                    tell!("error: {e}");
                    return ExitCode::from(ERROR);
                }
            };
            let paths = if all { library_tiles() } else { paths };
            let out = out_dir(out);
            let systems: Vec<&str> = if system == "both" {
                vec!["db", "ol"]
            } else {
                vec![system.as_str()]
            };
            systems.iter().fold(OK, |code, sys| {
                worse(
                    code,
                    build(
                        &paths,
                        &options((*sys).to_owned(), seed),
                        &out,
                        Extras {
                            png,
                            threemf,
                            palette: &palette,
                        },
                        json,
                    ),
                )
            })
        }
        Command::Looks { name, json } => looks::run(name.as_deref(), json),
        Command::Mcp { http } => http.map_or_else(mcp::run, |a| mcp::serve_http(&a)),
        Command::Lsp => lsp::run(),
        Command::Audit {
            path,
            width,
            height,
            json,
        } => audit_stl(&path, width, height, json),
        Command::Preview {
            paths,
            system,
            port,
            open,
        } => preview::run(&paths, &options(system, None), port, open),
        Command::Watch {
            paths,
            system,
            seed,
            out,
            png,
            max_builds,
        } => watch::run(
            &paths,
            &options(system, seed),
            &out_dir(out),
            png,
            max_builds,
        ),
    };
    ExitCode::from(code)
}

/// Every tile of the bundled library, as `lib:` paths, tests aside.
pub(crate) fn library_tiles() -> Vec<String> {
    let mut v: Vec<String> = dt_eval::Library::paths()
        .into_iter()
        .filter(|p| p.contains("tiles/") && !p.contains("tiles/test/"))
        .collect();
    v.sort();
    v
}

fn options(system: String, seed: Option<u64>) -> Options {
    Options {
        system,
        seed,
        engine: env!("DT_ENGINE_VERSION").to_owned(),
        ..Options::default()
    }
}

/// `--out`, or `out/` beside `dharmatiles.kdl`, else here.
fn out_dir(out: Option<PathBuf>) -> PathBuf {
    out.unwrap_or_else(|| {
        let cwd = std::env::current_dir().unwrap_or_default();
        dt_eval::discover(&cwd).out
    })
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
                            tell!("ok {p}: tile `{}`, {} stack entries", t.name, t.stack.len());
                        }
                        Some(t) => {
                            tell!(
                                "ok {p}: tile `{}`; {} part(s) need looks not built yet, so `build` refuses it:",
                                t.name,
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
        say!("{}", json!({"ok": code == OK, "tiles": reports}));
    }
    code
}

/// What else a build writes beside the STL.
#[derive(Clone, Copy)]
struct Extras<'a> {
    png: bool,
    threemf: bool,
    palette: &'a dt_kit::Palette,
}

fn build(paths: &[String], opts: &Options, out: &Path, extras: Extras, json: bool) -> u8 {
    let mut code = OK;
    let mut reports = Vec::new();
    for p in paths {
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
fn worse(a: u8, b: u8) -> u8 {
    if a == ERROR || b == ERROR {
        ERROR
    } else {
        a.max(b)
    }
}

/// Build one tile and write its files; report it on stderr unless `json`.
/// Returns its exit code and its JSON report.
fn build_one(
    sources: &dyn Source,
    p: &str,
    opts: &Options,
    out: &Path,
    extras: Extras,
    json: bool,
) -> (u8, Value) {
    let t0 = Instant::now();
    let b = match dt_eval::build(sources, p, opts) {
        Ok(b) => b,
        Err(errs) => {
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
    let files = match write(&b, &failures, built_s, out, extras) {
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
    let a = &b.audit;
    if !json {
        tell!(
            "built {p} → {}\n  {} triangles · watertight {} · overhang {:.2} mm² · band {:.2}/{:.2} mm² · {} dropped · {} voids filled · {} holes · build {:.2} s, written {:.2} s",
            files[0].display(),
            a.triangles,
            a.watertight,
            a.overhang_mm2,
            a.band_mm2,
            a.band_budget_mm2,
            a.dropped.len(),
            a.voids.len(),
            a.holes,
            built_s,
            total_s
        );
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
fn write(
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
        w(
            &p,
            &dt_kit::threemf(&b.export.mesh, &b.materials, &b.record, extras.palette),
        )?;
        files.push(p);
    }
    if extras.png {
        let colours = b.colours();
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
            "holes": a.holes,
            "failures": failures,
        },
        "stages": b.stages.iter().map(|s| json!({"name": s.name, "hash": format!("{:016x}", s.hash)})).collect::<Vec<_>>(),
        "build_s": built_s,
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

fn report(errs: &[Diag]) {
    for e in errs {
        tell!("{:?}", miette::Report::new(e.clone()));
    }
}

/// The looks' notes as one JSON object. A look used twice notes the same
/// names twice: the second is keyed `name#2`, the third `name#3`, so no
/// entry's numbers overwrite another's.
fn stats(notes: &[(String, f64)]) -> serde_json::Map<String, Value> {
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

/// `audit FILE.stl`: the gate on a binary STL as a slicer would read it.
fn audit_stl(path: &str, width: Option<f64>, height: Option<f64>, json: bool) -> u8 {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tell!("error: cannot read {path}: {e}");
            return ERROR;
        }
    };
    let Some(mesh) = read_stl(&bytes) else {
        tell!("error: {path} is not a binary STL");
        return ERROR;
    };
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &mesh.positions {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    // the tile's size: whole squares (DungeonBlocks 35 mm, OpenLOCK 25.4 mm)
    // nearest the mesh's extent, which the base's flare overshoots a little
    let tile = |extent: f64| -> f64 {
        [35.0, 25.4]
            .iter()
            .filter_map(|sq| {
                let n = libm::round(extent / sq).max(1.0);
                let err = (extent - n * sq).abs();
                (err < 1.5).then_some((err, n * sq))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(extent, |(_, v)| v)
    };
    let footprint = [
        width.unwrap_or_else(|| tile(hi[0] - lo[0])),
        height.unwrap_or_else(|| tile(hi[1] - lo[1])),
    ];
    let printer = Options::default().printer;
    let (_, a) = dt_kit::audit(&mesh, footprint, &printer);
    let failures = a.failures(&printer);
    if json {
        say!(
            "{}",
            json!({"ok": failures.is_empty(), "path": path, "triangles": a.triangles,
                "volume_mm3": a.volume_mm3, "watertight": a.watertight, "bodies": a.bodies,
                "overhang_mm2": a.overhang_mm2, "holes": a.holes,
                "dropped": a.dropped.len(), "failures": failures})
        );
    } else {
        tell!(
            "{path}: {} triangles, watertight {}, overhang {:.2} mm², {} holes, {} loose bodies dropped",
            a.triangles,
            a.watertight,
            a.overhang_mm2,
            a.holes,
            a.dropped.len()
        );
        for f in &failures {
            tell!("  audit failed: {f}");
        }
    }
    if failures.is_empty() {
        OK
    } else {
        AUDIT_FAILED
    }
}

/// A binary STL's triangles, vertices merged by position (as a slicer
/// reads it).
fn read_stl(b: &[u8]) -> Option<dt_kit::TriMesh> {
    let n = usize::try_from(u32::from_le_bytes(b.get(80..84)?.try_into().ok()?)).ok()?;
    if b.len() < 84 + 50 * n {
        return None;
    }
    let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let mut index: std::collections::HashMap<[u32; 3], u32> = std::collections::HashMap::new();
    let mut m = dt_kit::TriMesh::default();
    for t in 0..n {
        let base = 84 + 50 * t + 12;
        let mut tri = [0u32; 3];
        for (v, slot) in tri.iter_mut().enumerate() {
            let o = base + 12 * v;
            let p = [f(o), f(o + 4), f(o + 8)];
            *slot = *index.entry(p.map(f32::to_bits)).or_insert_with(|| {
                m.positions.push(p.map(f64::from));
                u32::try_from(m.positions.len() - 1).unwrap_or(u32::MAX)
            });
        }
        m.triangles.push(tri);
    }
    Some(m)
}
