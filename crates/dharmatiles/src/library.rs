//! Where tiles come from: the library on disk, tile lists, options and
//! the output folder.

use std::path::PathBuf;

use dt_eval::Options;

/// Every tile of the bundled library, as `lib:` paths, tests aside.
/// Inside a checkout (a `dharmatiles.dtile` project whose `library/` holds the
/// printer profile), read `lib:` from that folder, so new and edited
/// library tiles count at once. `$DT_LIBRARY` overrides it; otherwise the
/// copy installed beside the program is used (see `dt_eval::source`).
pub(crate) fn disk_library() {
    if std::env::var_os("DT_LIBRARY").is_some() {
        return;
    }
    let root = dt_eval::discover(&std::env::current_dir().unwrap_or_default()).root;
    let lib = root.join("library");
    if lib.join("printers/default.dtile").is_file() {
        dt_eval::source::use_library_dir(lib);
    }
}

/// Every library tile, test tiles included (the preview's list).
pub(crate) fn all_library_tiles() -> Vec<String> {
    let mut v: Vec<String> = dt_eval::Library::paths()
        .into_iter()
        .filter(|p| p.contains("tiles/"))
        .collect();
    v.sort();
    v
}

pub(crate) fn library_tiles() -> Vec<String> {
    let mut v: Vec<String> = dt_eval::Library::paths()
        .into_iter()
        .filter(|p| p.contains("tiles/") && !p.contains("tiles/test/"))
        .collect();
    v.sort();
    v
}

/// The options for a build target: the base system and the printer
/// profile (the user's own beside their tiles, else the library's).
///
/// # Errors
/// A system that is not `db` or `ol`; a printer that is not there.
pub(crate) fn options(system: &str, printer: &str, seed: Option<u64>) -> Result<Options, String> {
    options_in(
        system,
        printer,
        seed,
        Some(&crate::preview::roots::personal_home()),
    )
}

/// [`options`], with the user's printers read from `home`.
pub(crate) fn options_in(
    system: &str,
    printer: &str,
    seed: Option<u64>,
    home: Option<&std::path::Path>,
) -> Result<Options, String> {
    if !matches!(system, "db" | "ol") {
        return Err(format!("`{system}` is not a system: use `db` or `ol`"));
    }
    let mut o = Options {
        system: system.to_owned(),
        seed,
        engine: env!("DT_ENGINE_VERSION").to_owned(),
        ..Options::default()
    };
    o.use_printer(printer, home).map_err(|e| {
        e.iter()
            .map(|d| match &d.help {
                Some(h) => format!("{}: {h}", d.message),
                None => d.message.clone(),
            })
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    Ok(o)
}

/// `--out`, or `out/` beside `dharmatiles.dtile`, else here.
pub(crate) fn out_dir(out: Option<PathBuf>) -> PathBuf {
    out.unwrap_or_else(|| {
        let cwd = std::env::current_dir().unwrap_or_default();
        dt_eval::discover(&cwd).out
    })
}

/// The file in the user's home that remembers the build target.
const TARGET_FILE: &str = "target.json";

/// The target the title bar's chip last chose: `(system, printer)`.
pub(crate) fn remembered_target(home: &std::path::Path) -> (Option<String>, Option<String>) {
    let v: serde_json::Value = std::fs::read_to_string(home.join(TARGET_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let get = |k: &str| v[k].as_str().map(str::to_owned);
    (get("system"), get("printer"))
}

/// Remember the target in the user's home (made if need be); a failure to
/// write is not worth telling.
pub(crate) fn remember_target(home: &std::path::Path, system: &str, printer: &str) {
    let text = serde_json::json!({"system": system, "printer": printer}).to_string() + "\n";
    let _ =
        std::fs::create_dir_all(home).and_then(|()| std::fs::write(home.join(TARGET_FILE), text));
}

/// The target for the preview: the flags given, else the one remembered
/// in the user's home, else DungeonBlocks and the default printer.
///
/// # Errors
/// A flag that is not a system or a printer.
pub(crate) fn preview_options(
    system: Option<&str>,
    printer: Option<&str>,
) -> Result<Options, String> {
    let home = crate::preview::roots::personal_home();
    let (rs, rp) = remembered_target(&home);
    let sys = system
        .map(str::to_owned)
        .or(rs)
        .unwrap_or_else(|| "db".to_owned());
    let pr = printer
        .map(str::to_owned)
        .or(rp)
        .unwrap_or_else(|| "default".to_owned());
    match options_in(&sys, &pr, None, Some(&home)) {
        // a remembered printer that has gone: the default, not a failure
        Err(_) if printer.is_none() && system.is_none() => {
            options_in("db", "default", None, Some(&home))
        }
        r => r,
    }
}
