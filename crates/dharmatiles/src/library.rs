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

pub(crate) fn options(system: String, seed: Option<u64>) -> Options {
    Options {
        system,
        seed,
        engine: env!("DT_ENGINE_VERSION").to_owned(),
        ..Options::default()
    }
}

/// `--out`, or `out/` beside `dharmatiles.dtile`, else here.
pub(crate) fn out_dir(out: Option<PathBuf>) -> PathBuf {
    out.unwrap_or_else(|| {
        let cwd = std::env::current_dir().unwrap_or_default();
        dt_eval::discover(&cwd).out
    })
}
