//! The files of one built tile: the STL, its build record and, when asked, the painted 3MF (a Bambu Studio project when
//! a template's settings are given).  The one function that makes their bytes: `dharmatiles build` writes them to
//! `out/`, the editor's Export button writes them in the program, and the browser editor's service hands them to the
//! page to save.  The PNG views are not here (a program's output, drawn by `dt-render`).

use dt_eval::{Built, TileRecord, Verdict};
use dt_kit::Palette;

use crate::plates::export::File;

/// What a tile's files are named: the build's `out` path (`db/ground/1x1-grass-db.stl`) with the extension each file
/// takes.  A name is relative: the caller says where it goes.
fn named(out: &std::path::Path, extension: &str) -> String {
    out.with_extension(extension).display().to_string()
}

/// The files a build gives, by the gate's verdict: only a build that passed is exported (the STL, and the 3MF when
/// `threemf`); a build that was checked also gets its record, `.build.json`.  A low or failed build writes nothing a
/// printer reads.
///
/// # Errors
/// The record cannot be written as JSON, or the Bambu template's settings cannot be read or have no printable area.
pub fn tile_files(
    b: &Built,
    verdict: &Verdict,
    record: &TileRecord,
    threemf: bool,
    template: Option<&str>,
    palette: &Palette,
) -> Result<Vec<File>, String> {
    let mut files = Vec::new();
    if let Verdict::Passed(passed) = verdict {
        files.push((named(&b.out, "stl"), crate::stl(passed)));
    }
    if !matches!(verdict, Verdict::NotChecked) {
        let text = serde_json::to_string_pretty(record).map_err(|e| e.to_string())?;
        files.push((named(&b.out, "build.json"), text.into_bytes()));
    }
    if let (Verdict::Passed(passed), true) = (verdict, threemf) {
        let bytes = match template {
            Some(t) => crate::bambu_project(passed, palette, t)?,
            None => crate::threemf(passed, palette),
        };
        files.push((named(&b.out, "3mf"), bytes));
    }
    Ok(files)
}
