//! Finding the Bambu Studio template a \`--3mf\` build takes its printer,
//! process and filament settings from.

use std::path::Path;

/// The template's project settings: \`path\`, else \`bambu.3mf\` in the
/// personal home when it exists, else none (a plain painted 3MF).
///
/// # Errors
/// When a template is named or found but cannot be read.
pub(crate) fn template(path: Option<&Path>) -> Result<Option<String>, String> {
    let found = crate::preview::roots::personal_home()?.join("bambu.3mf");
    let path = match path {
        Some(p) => p.to_path_buf(),
        None if found.is_file() => found,
        None => {
            tell!(
                "note: no Bambu Studio template ({}), so the 3MF is a plain one: save any project from Bambu Studio there, with your printer and process, to get named filament slots",
                found.display()
            );
            return Ok(None);
        }
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let settings =
        dt_kit::project_settings(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    // refuse now, naming the template, rather than place the tile on a guessed bed
    dt_kit::bed_centre(&settings).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(settings))
}
