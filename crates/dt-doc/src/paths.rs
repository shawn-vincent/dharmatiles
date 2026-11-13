//! Where a `use` line points: the library's prefix, the heights file every
//! document sees, and the path a `use` names from the file that writes it.
//! Pure strings: the program reads them from disk and the browser from a map.

use crate::model::{Source, Use};

/// The bundled library's prefix.
pub const LIB: &str = "lib:";

/// The library's heights, which every file but itself sees.
pub const HEIGHTS: &str = "lib:parts/heights.dtile";

/// The path a `use` line in `from` names: the library for a bare or
/// `lib:` path, the project (relative to `from`) for `./` or `../`.
#[must_use]
pub fn use_path(from: &str, u: &Use) -> String {
    match &u.source {
        Source::Library(p) => format!("{LIB}{p}.dtile"),
        Source::Project(p) => resolve(from, p),
    }
}

/// The path a `use "target"` in `from` names, with `.dtile` added if missing.
#[must_use]
pub fn resolve(from: &str, target: &str) -> String {
    let target = if target.ends_with(".dtile") {
        target.to_owned()
    } else {
        format!("{target}.dtile")
    };
    if target.starts_with(LIB) || target.starts_with('/') {
        return target;
    }
    // Windows paths from the command line use `\`; uses are written `/`
    let from = if cfg!(windows) {
        from.replace('\\', "/")
    } else {
        from.to_owned()
    };
    let dir = from.rfind('/').map_or_else(
        || {
            if from.starts_with(LIB) {
                LIB.to_owned()
            } else {
                String::new()
            }
        },
        |i| from[..=i].to_owned(),
    );
    let mut parts: Vec<&str> = Vec::new();
    let (prefix, rest) = dir
        .strip_prefix(LIB)
        .map_or(("", dir.as_str()), |r| (LIB, r));
    let absolute = rest.starts_with('/');
    for p in rest.split('/').chain(target.split('/')) {
        match p {
            "" | "." => {}
            // a relative path keeps the `..` it cannot cancel (`../a`);
            // above `/` there is nothing, so an absolute one drops it
            ".." if parts.last().is_some_and(|l| *l != "..") => {
                parts.pop();
            }
            ".." if absolute => {}
            p => parts.push(p),
        }
    }
    let lead = if absolute { "/" } else { "" };
    format!("{prefix}{lead}{}", parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_to_the_importer() {
        assert_eq!(
            resolve("lib:tiles/ground/x.dtile", "../../parts/grass"),
            "lib:parts/grass.dtile"
        );
        assert_eq!(
            resolve("lib:tiles/x.dtile", "lib:parts/grass"),
            "lib:parts/grass.dtile"
        );
        assert_eq!(resolve("tiles/a.dtile", "parts.dtile"), "tiles/parts.dtile");
        assert_eq!(resolve("a.dtile", "b"), "b.dtile");
    }

    #[test]
    fn leading_dot_dots_are_kept() {
        assert_eq!(resolve("../u/sub/t.dtile", "./p"), "../u/sub/p.dtile");
        assert_eq!(resolve("a.dtile", "../shared/x"), "../shared/x.dtile");
        assert_eq!(resolve("../a.dtile", "../../x"), "../../../x.dtile");
        assert_eq!(resolve("t/a.dtile", "../../x"), "../x.dtile");
        assert_eq!(resolve("/p/a.dtile", "../../../x"), "/x.dtile");
        assert_eq!(resolve("lib:a.dtile", "../x"), "lib:../x.dtile");
    }
}
