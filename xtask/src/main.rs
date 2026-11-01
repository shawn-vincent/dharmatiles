//! Repo chores, run as `cargo xtask <command>`.

use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <command>

commands:
  parity   compare Python lab output with Rust output (built in P1)
  help     show this message";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("parity") => parity(&args[1..]),
        Some("help" | "-h" | "--help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command: {other}\n\n{USAGE}");
            ExitCode::FAILURE
        }
        None => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// The parity harness (plan.html, "The parity harness"). Empty until P1.
fn parity(_args: &[String]) -> ExitCode {
    println!("parity: not implemented yet (the harness is built in P1)");
    ExitCode::SUCCESS
}
