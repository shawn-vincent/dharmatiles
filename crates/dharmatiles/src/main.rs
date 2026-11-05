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
mod build;
mod check;
mod cli;
mod library;
mod looks;
mod mcp;
mod preview;
mod progress;
mod watch;

pub(crate) use audit::{read_stl, stl_footprint};
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
        system: "db".to_owned(),
        port: None,
        open: true,
        palette: None,
    });
    let code = match command {
        Command::Check { paths, json } => check::check(&paths, json),
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
                build::worse(
                    code,
                    build::build(
                        &paths,
                        &options((*sys).to_owned(), seed),
                        &out,
                        Extras {
                            progress: !json && std::io::IsTerminal::is_terminal(&std::io::stderr()),
                            tile_no: None,
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
        Command::Audit {
            path,
            width,
            height,
            json,
        } => audit::audit_stl(&path, width, height, json),
        Command::Preview {
            paths,
            system,
            port,
            open,
            palette,
        } => preview::run(&paths, &options(system, None), port, open, palette),
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
