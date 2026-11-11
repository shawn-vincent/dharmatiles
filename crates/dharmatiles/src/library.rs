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
    let root = project().root;
    let lib = root.join("library");
    if lib.join("printers/default.dtile").is_file() {
        dt_eval::source::use_library_dir(lib);
    }
}

/// The folder the program was run in; when it cannot be found, say so and
/// use `.` (never silently).
pub(crate) fn current_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|e| {
        tell!("note: cannot find the current folder ({e}); using `.`");
        PathBuf::from(".")
    })
}

/// The project around the current folder, saying so when its project file
/// could not be used.
pub(crate) fn project() -> dt_eval::Project {
    let p = dt_eval::discover(&current_dir());
    if let Some(why) = &p.problem {
        crate::preview::tell_once(&format!(
            "note: {why}; the project file is ignored, so builds go to out/"
        ));
    }
    p
}

/// Every library tile and test tile (the preview's Library and Tests).
///
/// # Errors
/// The library cannot be found or listed.
pub(crate) fn all_library_tiles() -> Result<Vec<String>, String> {
    let mut v: Vec<String> = dt_eval::Library::paths()?
        .into_iter()
        .filter(|p| p.starts_with("lib:tiles/") || p.starts_with("lib:tests/"))
        .collect();
    v.sort();
    Ok(v)
}

/// The shipped tiles (`build --all`, the MCP's `list_tiles`), test tiles left out.
///
/// # Errors
/// The library cannot be found or listed, or holds no tiles: a sweep of
/// nothing is not a success.
pub(crate) fn library_tiles() -> Result<Vec<String>, String> {
    let mut v: Vec<String> = dt_eval::Library::paths()?
        .into_iter()
        .filter(|p| p.starts_with("lib:tiles/"))
        .collect();
    v.sort();
    if v.is_empty() {
        return Err("the library holds no tiles to build".to_owned());
    }
    Ok(v)
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
        Some(&crate::preview::roots::personal_home()?),
    )
}

/// True when `DT_FORMAT_PREVIEW=1`: read the format in development
/// (format 1, until 1.0.0 ships it).  The environment is read here, at
/// the boundary, and passed on as an option.
pub(crate) fn format_preview() -> bool {
    std::env::var("DT_FORMAT_PREVIEW").is_ok_and(|v| v == "1")
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
        format_preview: format_preview(),
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
    out.unwrap_or_else(|| project().out)
}

/// The file in the user's home that remembers the build target.
const TARGET_FILE: &str = "target.json";

/// The target the title bar's chip last chose: `(system, printer)`; both
/// `None` before any choice (no file yet).
///
/// # Errors
/// The file is there but cannot be read or is not the JSON this program wrote.
pub(crate) fn remembered_target(
    home: &std::path::Path,
) -> Result<(Option<String>, Option<String>), String> {
    let file = home.join(TARGET_FILE);
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((None, None)),
        Err(e) => return Err(format!("cannot read {}: {e}", file.display())),
    };
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not valid JSON: {e}", file.display()))?;
    let get = |k: &str| v[k].as_str().map(str::to_owned);
    Ok((get("system"), get("printer")))
}

/// Remember the target in the user's home (made if need be).
///
/// # Errors
/// The folder cannot be made or the file written.
pub(crate) fn remember_target(
    home: &std::path::Path,
    system: &str,
    printer: &str,
) -> Result<(), String> {
    let text = serde_json::json!({"system": system, "printer": printer}).to_string() + "\n";
    let file = home.join(TARGET_FILE);
    std::fs::create_dir_all(home)
        .and_then(|()| std::fs::write(&file, text))
        .map_err(|e| {
            format!(
                "cannot remember the build target in {}: {e}",
                file.display()
            )
        })
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
    let home = crate::preview::roots::personal_home()?;
    let (rs, rp) = remembered_target(&home).unwrap_or_else(|e| {
        // the remembered target is a convenience: say it is not used, and
        // build for the default (never silently)
        tell!("note: {e}; using the default build target");
        (None, None)
    });
    let sys = system
        .map(str::to_owned)
        .or(rs)
        .unwrap_or_else(|| "db".to_owned());
    let pr = printer
        .map(str::to_owned)
        .or(rp)
        .unwrap_or_else(|| "default".to_owned());
    match options_in(&sys, &pr, None, Some(&home)) {
        // a remembered printer that has gone: the default, not a failure, and said
        Err(e) if printer.is_none() && system.is_none() => {
            tell!(
                "note: the remembered build target {sys}/{pr} is not available ({e}); using db/default"
            );
            options_in("db", "default", None, Some(&home))
        }
        r => r,
    }
}

/// Read a palette document: its header line is checked as in every kind of
/// document, then its colours and ramps.
///
/// # Errors
/// In words naming the file: unreadable, a bad header (shown as a
/// diagnostic), or a bad line.
pub(crate) fn read_palette(path: &std::path::Path) -> Result<dt_kit::Palette, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read the palette {}: {e}", path.display()))?;
    dt_doc::header::check_text(&path.display().to_string(), &text).map_err(|errs| {
        errs.into_iter()
            .map(|e| format!("{:?}", miette::Report::new(e)))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    dt_kit::Palette::parse(&text).map_err(|e| format!("palette {}: {e}", path.display()))
}
