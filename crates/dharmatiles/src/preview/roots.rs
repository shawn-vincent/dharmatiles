//! The roots the preview lists tiles from: files, folders, the library.

pub use dt_service::roots::*;

/// The personal home: `$DT_HOME`, else `~/dharmatiles`.  It need not exist
/// yet; the preview lists its tiles once it does.
///
/// # Errors
/// When neither `$DT_HOME` nor the home folder (`$HOME`, `%USERPROFILE%`) is
/// set: there is nowhere to put your tiles, and a relative folder would put
/// them wherever the program happens to run.
pub fn personal_home() -> Result<std::path::PathBuf, String> {
    if let Some(d) = std::env::var_os("DT_HOME").filter(|d| !d.is_empty()) {
        return Ok(d.into());
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .ok_or_else(|| {
            "cannot find your home folder: neither DT_HOME nor HOME is set; set DT_HOME to the folder for your tiles".to_owned()
        })?;
    Ok(std::path::PathBuf::from(home).join("dharmatiles"))
}

/// What the command line asks for: the roots, and the files named.
/// A folder is a root of its own; `lib:tiles/water` (no `.dtile`) is the
/// library's water group; anything else is a file.  No paths: the personal
/// home, the library and the test tiles.
///
/// # Errors
/// When no paths are named and there is no personal home ([`personal_home`]).
pub fn roots_for(paths: &[String]) -> Result<(Vec<Root>, Vec<String>), String> {
    let lib = Root::new("Library", RootKind::Library("tiles/".to_owned()));
    if paths.is_empty() {
        let home = Root::new("My tiles", RootKind::Dir(personal_home()?)).with_role(Role::Home);
        let tests = Root::new("Tests", RootKind::Library("tests/".to_owned()));
        return Ok((vec![home, lib, tests], Vec::new()));
    }
    let mut roots = Vec::new();
    let mut files = Vec::new();
    for p in paths {
        let lower = p.to_ascii_lowercase();
        if let Some(rest) = p.strip_prefix("lib:")
            && !lower.ends_with(".dtile")
            && !lower.ends_with(".stl")
        {
            roots.push(Root::library_group(&format!(
                "{}/",
                rest.trim_end_matches('/')
            )));
        } else if !p.starts_with("lib:") && std::path::Path::new(p).is_dir() {
            roots.push(Root::new(p, RootKind::Dir(p.into())));
        } else {
            files.push(p.clone());
        }
    }
    if !files.is_empty() {
        roots.insert(0, Root::new("Files", RootKind::Files));
    }
    Ok((roots, files))
}
