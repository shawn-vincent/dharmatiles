//! `dharmatiles poster FILE… [--out DIR]`: what the docs site needs of an example, for `cargo xtask docs` and `cargo xtask
//! examples` (a hidden command: it is a chore of the repository, not part of making tiles).
//!
//! Prints one line per file, `FILE<TAB>KEY`: the build cache key ([`dt_store::Store::key`]: a SHA-256 of everything the build
//! read, the document and the library files it used) that names the file's poster, `posters/KEY.jpg`.  The key is the full
//! detail build on DungeonBlocks with the default printer, and the engine is named by the release version, so a poster is
//! made again when the example, the library it reads or the release changes, not on every commit.  A document that does not
//! check prints `FILE<TAB>error:CODE` instead (an example that is meant to fail has no picture).
//!
//! With `--out DIR` it also builds each tile that has no `DIR/KEY.png` yet and writes its top view there (unpainted by the
//! site's overlay: the page draws that over the picture).

use std::path::{Path, PathBuf};

use dt_eval::{Files, Progress, Source, check};
use dt_print::{VIEWS, render};
use dt_store::{Store, Tier, Tracked};

use crate::{ERROR, OK};

/// The key of `path` as the poster step names it.
///
/// # Errors
/// The document does not read or check: the first mistake's code.
pub(crate) fn key_of(path: &str) -> Result<(String, dt_eval::Options), Result<String, String>> {
    let opts = crate::library::options_in("db", "default", None, None).map_err(Err)?;
    let tracked = Tracked::new(&Files);
    tracked.read(path).map_err(Err)?;
    if let Err(errs) = check(&tracked, path, &opts) {
        return Err(Ok(errs.first().map_or("unknown", |e| e.code).to_owned()));
    }
    // only the key is wanted: the folder is never touched
    let store = Store::new(
        std::env::temp_dir().join("dharmatiles-poster-key"),
        env!("CARGO_PKG_VERSION"),
    );
    let key = store.key(&tracked.reads(), path, &opts);
    Ok((key.id().to_owned(), opts))
}

/// Run the command.
pub(crate) fn run(files: &[PathBuf], out: Option<&Path>) -> u8 {
    let mut code = OK;
    for f in files {
        let path = f.to_string_lossy().into_owned();
        let (key, opts) = match key_of(&path) {
            Ok(k) => k,
            Err(Ok(c)) => {
                say!("{path}\terror:{c}");
                continue;
            }
            Err(Err(e)) => {
                tell!("error: {path}: {e}");
                code = ERROR;
                continue;
            }
        };
        say!("{path}\t{key}");
        let Some(dir) = out else { continue };
        // the map of the tile (what the overlay draws), named by the same key: the page draws its marks over the poster
        // before the engine has woken
        let map = dir.join(format!("{key}.map.json"));
        let png = dir.join(format!("{key}.png"));
        let made = write_map(&path, &opts, &map).and_then(|()| {
            if png.is_file() {
                Ok(())
            } else {
                write_png(&path, &opts, &png)
            }
        });
        if let Err(e) = made {
            tell!("error: {path}: {e}");
            code = ERROR;
        }
    }
    code
}

fn write_map(path: &str, opts: &dt_eval::Options, to: &Path) -> Result<(), String> {
    if to.is_file() {
        return Ok(());
    }
    let text = Files.read(path)?;
    let checked = check(&Files, path, opts).map_err(|errs| {
        errs.iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let map = dt_eval::map::json(&checked, &text, opts.units()).map_err(|errs| {
        errs.iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string(&map).map_err(|e| e.to_string())?;
    std::fs::write(to, json).map_err(|e| format!("cannot write {}: {e}", to.display()))
}

fn write_png(path: &str, opts: &dt_eval::Options, png: &Path) -> Result<(), String> {
    let built = dt_store::build_tier(
        &Files,
        path,
        opts,
        Tier::Full,
        &Progress::none(),
        None,
        None,
    )
    .map_err(|errs| {
        errs.iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    let colours = built.colours();
    // the top view: the page draws its marks over the tile's footprint, which this picture is cropped to
    let top = VIEWS
        .iter()
        .find(|v| v.name == "top")
        .ok_or("the renderer has no top view")?;
    let img = render(
        &built.export.mesh,
        &colours,
        top,
        built.footprint,
        (1200, 950),
    );
    if let Some(dir) = png.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    std::fs::write(png, img.png()?).map_err(|e| format!("cannot write {}: {e}", png.display()))
}
