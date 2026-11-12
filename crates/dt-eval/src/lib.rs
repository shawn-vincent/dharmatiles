//! dt-eval: from a document path to an audited mesh.
//!
//! Finding the project ([`discover`]), reading documents and their imports
//! through the [`Source`] trait (`lib:` is the embedded library), the
//! plan-then-build scheduler ([`plan::schedule`]), seeds by path, and the
//! one public call every front end uses: [`build`] this document for this
//! system. [`check`] does the plan pass only (no geometry).
//!
//! Documents are read by dt-doc, through [`doc`]: imports, levels,
//! definitions and defaults resolved, the edge contract read, and what
//! needs an element not built yet set aside (see its module docs).

mod build;
mod cache;
mod check;
pub mod coords;
pub mod crash_switch;
mod crowns;
pub mod diag;
pub mod doc;
mod hash;
pub mod inventory;
mod lower;
pub mod machine;
mod options;
pub mod places;
pub mod plan;
pub mod plates;
pub mod progress;
pub mod regions;
pub mod solve;
pub mod source;
pub mod store;
pub mod tiers;

use std::path::{Path, PathBuf};

pub use build::{Built, NO_ENTRY, build, build_cached, build_stored, build_tier, build_with};
pub use cache::Cache;
pub use check::{Checked, check};
pub use diag::Diag;
pub use doc::{Region, Tile, Within};
pub use dt_kit::Progress;
pub use lower::{RangeRefusal, Ranged, ranged, weighting_flaw_in};
pub use machine::{Machine, ProfileKind, printer_machine, printer_names, printer_profile};
pub use options::{DRAFT_CELLS, Options, Profile};
pub use source::{Files, LIB, Library, Memory, Source, library_file};
pub use store::{CacheState, CacheUse, Store, Tier, Tracked};

/// A project: the folder holding `dharmatiles.dtile` (or the start folder
/// when there is none) and where builds go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    /// The project root.
    pub root: PathBuf,
    /// Build output: `out/` beside the project file unless it says
    /// `out "…"`.
    pub out: PathBuf,
    /// Why the project file could not be used, when it was found but not
    /// read (output then goes to `out/`): callers say so.
    pub problem: Option<String>,
}

/// **Project discovery**: walk up from `start` to the first folder with a
/// `dharmatiles.dtile`, as Cargo finds `Cargo.toml`.
#[must_use]
pub fn discover(start: &Path) -> Project {
    let found = start
        .ancestors()
        .find(|d| d.join("dharmatiles.dtile").is_file());
    let root = found.unwrap_or(start).to_path_buf();
    let mut problem = None;
    let out = found.and_then(|d| {
        let file = d.join("dharmatiles.dtile");
        let read = std::fs::read_to_string(&file)
            .map_err(|e| format!("cannot read {}: {e}", file.display()))
            .and_then(|t| {
                kdl::KdlDocument::parse(&t)
                    .map_err(|e| format!("{} is not valid KDL: {e}", file.display()))
            })
            .map(|k| {
                k.get("out")
                    .and_then(|n| n.entries().first())
                    .and_then(|e| e.value().as_string().map(PathBuf::from))
            });
        read.unwrap_or_else(|e| {
            problem = Some(e);
            None
        })
    });
    Project {
        out: root.join(out.unwrap_or_else(|| PathBuf::from("out"))),
        root,
        problem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::out_path;

    #[test]
    fn a_target_is_the_system_and_a_printer_that_is_not_the_default() {
        let mut o = Options::default();
        assert_eq!(o.target(), "db");
        assert_eq!(dt_kit::target_name("ol", "default"), "ol");
        o.use_printer("fine", None).unwrap();
        assert_eq!(o.target(), "db-fine");
        assert_eq!(o.printer.layer_mm, 0.12);
        assert_eq!(
            out_path("lib:tests/ground/1x1-flat.dtile", &o.target()),
            PathBuf::from("db-fine/tests/ground/1x1-flat-db-fine.stl")
        );
        let e = o.use_printer("nope", None).unwrap_err();
        assert!(e[0].message.contains("no printer profile"), "{e:?}");
        assert!(o.use_printer("../x", None).is_err());
        assert!(o.use_printer("", None).is_err());
        // a failed choice changes nothing
        assert_eq!(o.target(), "db-fine");
    }

    #[test]
    fn your_printers_come_first_and_the_library_ones_are_listed() {
        let home = std::env::temp_dir().join(format!("dt-printers-{}", std::process::id()));
        std::fs::create_dir_all(home.join("printers")).unwrap();
        let mine = "dharmatiles version=1\nprinter fine {\n line \"0.6mm\"\n layer \"0.3mm\"\n overhang \"50deg\"\n steepest \"60deg\"\n gap \"0.3mm\"\n}\n";
        std::fs::write(home.join("printers/fine.dtile"), mine).unwrap();
        std::fs::write(home.join("printers/other.dtile"), mine).unwrap();
        std::fs::write(home.join("printers/not a name.dtile"), mine).unwrap();
        let names = printer_names(Some(&home));
        let get = |n: &str| names.iter().find(|(m, _)| m == n).map(|(_, k)| *k);
        assert_eq!(get("default"), Some(ProfileKind::Library));
        assert_eq!(get("fine"), Some(ProfileKind::Mine));
        assert_eq!(get("other"), Some(ProfileKind::Mine));
        assert_eq!(get("not a name"), None);
        assert_eq!(printer_profile("fine", Some(&home)).unwrap().layer_mm, 0.3);
        assert_eq!(printer_profile("fine", None).unwrap().layer_mm, 0.12);
    }

    #[test]
    fn out_paths_mirror_the_library() {
        assert_eq!(
            out_path("lib:tests/ground/1x1-flat.dtile", "db"),
            PathBuf::from("db/tests/ground/1x1-flat-db.stl")
        );
        assert_eq!(out_path("my.dtile", "db"), PathBuf::from("db/my-db.stl"));
    }

    #[test]
    fn out_paths_stay_inside_the_system_folder() {
        // was: db/../../t/evil/f-db.stl, outside out/
        assert_eq!(
            out_path("tiles/../../t/evil/f.dtile", "db"),
            PathBuf::from("db/t/evil/f-db.stl")
        );
        assert_eq!(
            out_path("../t/evil/f.dtile", "db"),
            PathBuf::from("db/t/evil/f-db.stl")
        );
        assert_eq!(
            out_path("a/../../../f.dtile", "db"),
            PathBuf::from("db/f-db.stl")
        );
        assert_eq!(
            out_path("/abs/proj/f.dtile", "db"),
            PathBuf::from("db/f-db.stl")
        );
        assert_eq!(
            out_path("C:\\proj\\f.dtile", "db"),
            PathBuf::from("db/f-db.stl")
        );
        assert_eq!(out_path("dir/", "db"), PathBuf::from("db/dir/tile-db.stl"));
        for p in [
            "tiles/../../t/evil/f.dtile",
            "../../x.dtile",
            "/x/../../y.dtile",
        ] {
            let o = out_path(p, "db");
            assert!(
                o.components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))),
                "{p} → {o:?}"
            );
        }
    }
}
