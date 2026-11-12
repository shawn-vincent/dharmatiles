//! How to build: the options every front end passes, the printer profile in
//! use and where profiles come from.

use std::path::{Path, PathBuf};

use dt_kit::{Frame, Printer};

use crate::diag::Diag;
use crate::doc::{self};
use crate::source::{self, Files, Library};

/// How to build.
#[derive(Clone, Debug)]
pub struct Options {
    /// The connector system: `db`.
    pub system: String,
    /// Replaces the tile's seed (the CLI's `--seed`).
    pub seed: Option<u64>,
    /// The printer the audit checks against, with its name.
    pub printer: Profile,
    /// Engine version for the build record.
    pub engine: String,
    /// A draft for the editor's view while a value is dragged: coarser
    /// cells ([`DRAFT_CELLS`] per square), the parts shown as they are
    /// without the union, and the gate's verdict left out (each face is
    /// flagged by its angle alone, see [`dt_kit::draft`]).  Never written.
    pub draft: bool,
    /// A low build, the tier for browsing and the map: the same places,
    /// drawn with less detail (a rock as its hull, grass as a carpet, a
    /// tree as blobs, the ground meshed within a third of a millimetre),
    /// the parts shown as they are without the union, and the gate's
    /// verdict left out (each face is flagged by its angle alone).  A low
    /// build is **not yet checked**: it is never printed or exported.
    pub low: bool,
    /// Cells per square for the whole build, instead of the finest cell any
    /// entry asks for (128 where none does).  What is placed does not
    /// depend on it (see `dt_eval::places`); the surfaces and what is drawn
    /// on them do.  A full build leaves it `None`.
    pub cells: Option<u32>,
    /// Collect the editor's debug layer ([`dt_kit::debug`]): grass starts
    /// and spines.  Changes nothing that is built, and is not part of what
    /// the [`Cache`] keys a scene on (a scene kept without it is not given to
    /// a build that asks for it, one kept with it is given to any).
    pub debug: bool,
    /// Collect the clouds the editor draws for a selected `top=` or pin: each
    /// body's band as low and high sheets, and the span at each tree tip or
    /// stone.  As for [`Options::debug`].
    pub clouds: bool,
}

/// A printer profile in use: the gate's numbers and the name they were read
/// under (`default`, another of `library/printers/`, or one of the user's
/// own, see [`printer_profile`]). Reads as its [`Printer`].
#[derive(Clone, PartialEq)]
pub struct Profile {
    /// The profile's name.
    pub name: String,
    gate: Printer,
}

impl std::ops::Deref for Profile {
    type Target = Printer;
    fn deref(&self) -> &Printer {
        &self.gate
    }
}

impl std::fmt::Debug for Profile {
    /// As the printer alone: the build cache keys on it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.gate.fmt(f)
    }
}

impl Options {
    /// What a unit is worth on this build target: the system's square and
    /// the printer's layer (`mm`, `in` and `layer` in a format 1 document).
    #[must_use]
    pub fn units(&self) -> dt_doc::units::Units {
        dt_doc::units::Units {
            square_mm: Frame::for_system(&self.system, 1, 1).square_mm,
            layer_mm: self.printer.layer_mm,
        }
    }

    /// The build target: the base system, and the printer unless it is the
    /// default one (`db`, `ol`, `db-fine`). It names the output folder and
    /// files and the build record.
    #[must_use]
    pub fn target(&self) -> String {
        dt_kit::target_name(&self.system, &self.printer.name)
    }

    /// Build for the printer profile `name` (see [`printer_profile`]).
    ///
    /// # Errors
    /// Why there is no such profile, or it does not decode.
    pub fn use_printer(&mut self, name: &str, home: Option<&Path>) -> Result<(), Vec<Diag>> {
        self.printer = Profile {
            name: name.to_owned(),
            gate: printer_profile(name, home)?,
        };
        Ok(())
    }
}

/// Where a printer profile is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileKind {
    /// `library/printers/`, shipped with the program.
    Library,
    /// `printers/` beside the user's own tiles (`$DT_HOME`).
    Mine,
}

/// A printer profile's name is a file name: letters, digits, `-` and `_`.
fn profile_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Every printer profile there is: the library's and those in
/// `<home>/printers/`, by name, a profile of yours replacing the
/// library's of the same name.
#[must_use]
pub fn printer_names(home: Option<&Path>) -> Vec<(String, ProfileKind)> {
    let mut found: std::collections::BTreeMap<String, ProfileKind> =
        std::collections::BTreeMap::new();
    let mut scan = |dir: PathBuf, kind: ProfileKind| {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "dtile")
                && let Some(n) = p.file_stem().and_then(|n| n.to_str())
                && profile_name_ok(n)
            {
                found.insert(n.to_owned(), kind);
            }
        }
    };
    if let Some(lib) = source::library_dir() {
        scan(lib.join("printers"), ProfileKind::Library);
    }
    if let Some(h) = home {
        scan(h.join("printers"), ProfileKind::Mine);
    }
    found.into_iter().collect()
}

/// The printer profile called `name`: `<home>/printers/<name>.dtile` if
/// there is one, else `lib:printers/<name>.dtile`.
///
/// # Errors
/// Why there is no such profile, or it does not decode.
pub fn printer_profile(name: &str, home: Option<&Path>) -> Result<Printer, Vec<Diag>> {
    if !profile_name_ok(name) {
        return Err(vec![
            Diag::plain("dt::printer", format!("`{name}` is not a printer name"))
                .help("a printer name is letters, digits, `-` and `_`"),
        ]);
    }
    let mine = home.map(|h| h.join("printers").join(format!("{name}.dtile")));
    if let Some(f) = mine.filter(|f| f.is_file()) {
        return doc::printer(&Files, &f.to_string_lossy());
    }
    let lib = format!("lib:printers/{name}.dtile");
    if source::library_file(&lib).is_some_and(|f| f.is_file()) {
        return doc::printer(&Library, &lib);
    }
    let have: Vec<String> = printer_names(home).into_iter().map(|(n, _)| n).collect();
    Err(vec![
        Diag::plain("dt::printer", format!("no printer profile `{name}`"))
            .help(format!("the profiles are: {}", have.join(", "))),
    ])
}

/// Cells per square in a draft (a full build has 128, or finer where an
/// element asks).
pub const DRAFT_CELLS: u32 = 48;

impl Options {
    /// DungeonBlocks, the tile's seed, and the library's default printer
    /// (library/printers/default.dtile), read from the library on disk (see
    /// [`source::library_dir`]). The program's own paths use this one.
    ///
    /// # Errors
    /// The library's default printer cannot be read or does not decode (a
    /// broken `$DT_LIBRARY`, say), as the diagnostics from the document.
    pub fn new() -> Result<Self, Vec<Diag>> {
        let printer = doc::printer(&Library, doc::PRINTER)?;
        Ok(Self {
            system: "db".to_owned(),
            seed: None,
            printer: Profile {
                name: "default".to_owned(),
                gate: printer,
            },
            engine: concat!("dt-eval-", env!("CARGO_PKG_VERSION")).to_owned(),
            draft: false,
            low: false,
            cells: None,
            debug: false,
            clouds: false,
        })
    }
}

impl Default for Options {
    /// [`Options::new`], for tests and examples, where the bundled library
    /// is known good. This can fail when the library on disk is broken, so
    /// the program's own paths use [`Options::new`] and report the error.
    ///
    /// # Panics
    /// If the library's default printer does not decode.
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| panic!("the bundled printer must decode: {e:?}"))
    }
}
