//! `dharmatiles`: the command-line program.
//!
//! Exit codes:
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
use std::process::ExitCode;

use clap::Parser;
use cli::{Cli, Command};

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

mod audit;
mod bambu;
mod build;
mod check;
mod cli;
mod elements;
mod library;
mod mcp;
mod preview;
mod progress;
mod watch;

pub(crate) use audit::{read_stl, stl_corner_round, stl_footprint};
pub(crate) use build::{Extras, build_one, errors_json, report, tile_json};
pub(crate) use library::{all_library_tiles, library_tiles};
use library::{disk_library, options, out_dir};

const OK: u8 = 0;
const ERROR: u8 = 1;
const AUDIT_FAILED: u8 = 3;

fn main() -> ExitCode {
    let cli = Cli::parse();
    disk_library();
    // no command: browse your tiles and the library in the browser
    let command = cli.command.unwrap_or(Command::Preview {
        paths: Vec::new(),
        system: None,
        printer: None,
        port: None,
        open: true,
        palette: None,
    });
    let code = match command {
        Command::Check {
            paths,
            json,
            explain,
        } => check::check(&paths, json, explain),
        Command::Build {
            paths,
            all,
            system,
            printer,
            seed,
            out,
            png,
            threemf,
            bambu_template,
            palette,
            json,
        } => {
            let template = match threemf.then(|| bambu::template(bambu_template.as_deref())) {
                Some(Err(e)) => {
                    tell!("error: {e}");
                    return ExitCode::from(ERROR);
                }
                Some(Ok(t)) => t,
                None => None,
            };
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
            let paths = if all {
                match library_tiles() {
                    Ok(p) => p,
                    Err(e) => {
                        tell!("error: build --all: {e}");
                        return ExitCode::from(ERROR);
                    }
                }
            } else {
                paths
            };
            let out = out_dir(out);
            let opts = match options(&system, &printer, seed) {
                Ok(o) => o,
                Err(e) => {
                    tell!("error: {e}");
                    return ExitCode::from(ERROR);
                }
            };
            build::build(
                &paths,
                &opts,
                &out,
                Extras {
                    progress: !json && std::io::IsTerminal::is_terminal(&std::io::stderr()),
                    tile_no: None,
                    png,
                    threemf,
                    template: template.as_deref(),
                    palette: &palette,
                    cache: None,
                },
                json,
            )
        }
        Command::Migrate { paths } => {
            let mut code = 0;
            for p in &paths {
                let done = std::fs::read_to_string(p)
                    .map_err(|e| e.to_string())
                    .and_then(|text| dt_doc::migrate::heights(&text).map_err(|e| e.to_string()))
                    .and_then(|new| match new {
                        Some(new) => std::fs::write(p, new)
                            .map(|()| "migrated")
                            .map_err(|e| e.to_string()),
                        None => Ok("unchanged"),
                    });
                match done {
                    Ok(what) => tell!("{what} {}", p.display()),
                    Err(e) => {
                        tell!("error: {}: {e}", p.display());
                        code = ERROR;
                    }
                }
            }
            code
        }
        Command::Elements { name, json } => elements::run(name.as_deref(), json),
        Command::Mcp { http } => http.map_or_else(mcp::run, |a| mcp::serve_http(&a)),
        Command::Audit {
            path,
            width,
            height,
            json,
        } => audit::audit_stl(&path, width, height, json),
        Command::Preview {
            paths,
            system,
            printer,
            port,
            open,
            palette,
        } => match library::preview_options(system.as_deref(), printer.as_deref()) {
            Ok(o) => preview::run(&paths, &o, port, open, palette),
            Err(e) => {
                tell!("error: {e}");
                ERROR
            }
        },
        Command::Watch {
            paths,
            system,
            printer,
            seed,
            out,
            png,
            max_builds,
        } => match options(&system, &printer, seed) {
            Ok(o) => watch::run(&paths, &o, &out_dir(out), png, max_builds),
            Err(e) => {
                tell!("error: {e}");
                ERROR
            }
        },
    };
    ExitCode::from(code)
}
