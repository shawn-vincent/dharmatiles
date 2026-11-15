//! The program's half of `cargo xtask parity`: `parity_line` for each `tile@system` on the command line, one JSON
//! line each, with the library read from the bundle's `library.json` (the same text the browser is given).
//! `cargo run --release -p dt-web --example parity -- <library.json> <tile>@<system> ...`

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let lib = args
        .next()
        .ok_or("usage: parity <library.json> <tile>@<system> ...")?;
    let json = std::fs::read_to_string(&lib).map_err(|e| format!("{lib}: {e}"))?;
    dt_web::set_library(&json).map_err(|e| e.to_string())?;
    for job in args {
        let (tile, system) = job
            .split_once('@')
            .ok_or_else(|| format!("{job}: expected <tile>@<system>"))?;
        println!(
            "{}",
            dt_web::parity_line(tile, system).map_err(|e| e.to_string())?
        );
    }
    Ok(())
}
