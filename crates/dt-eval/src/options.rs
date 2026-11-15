//! How to build: the options every front end passes, the printer profile in
//! use and where profiles come from.

use std::path::Path;

use dt_kit::{Frame, Printer};

use crate::diag::Diag;
use crate::doc;

use crate::source::{Library, Source};

/// How to build.
#[derive(Clone, Debug)]
pub struct Options {
    /// The connector system.
    pub system: dt_lang::System,
    /// Replaces the tile's seed (the CLI's `--seed`).
    pub seed: Option<u64>,
    /// The printer the audit checks against, with its name.
    pub printer: Profile,
    /// Engine version for the build record.
    pub engine: String,
    /// How finely to build, one of [`Tier`]: a full build is the only one
    /// that is checked and may be written out.
    pub tier: Tier,
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

/// How finely a build is made.  One field of [`Options`], so no illegal pair
/// of flags can be written, and part of the key of every kept build: each tier
/// has its own entries (a full build is never answered with a preview).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum Tier {
    /// A finished build: the union and the gate.  What the printer is given.
    #[default]
    Full,
    /// The editor's draft: coarser cells ([`DRAFT_CELLS`] per square), the
    /// parts shown as they are without the union, and the gate's verdict
    /// left out (each face is flagged by its angle alone, see
    /// [`dt_kit::draft`]).  Shown while a value is dragged; never kept or
    /// written.
    Draft,
    /// The tier for browsing and the map: the same places, drawn with less
    /// detail (a rock as its hull, grass as a carpet, a tree as blobs, the
    /// ground meshed within a third of a millimetre), the parts as they are
    /// without the union, no gate verdict.  **Not yet checked**: never
    /// printed or exported.
    Low,
    /// A low build cut down to a few thousand triangles
    /// ([`crate::super_low`]): the library's previews, the map when far.  It
    /// is built as a low build; the name keeps its entries apart.
    SuperLow,
}

impl Tier {
    /// Its name, as the key, the wire and `dharmatiles cache` say it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Draft => "draft",
            Self::Low => "low",
            Self::SuperLow => "super-low",
        }
    }

    /// Whether the gate checks a build of this tier: only a full one.
    #[must_use]
    pub const fn checked(self) -> bool {
        matches!(self, Self::Full)
    }

    /// Whether the elements draw with less detail (a low build, or a super-low's source).
    #[must_use]
    pub const fn is_low(self) -> bool {
        matches!(self, Self::Low | Self::SuperLow)
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for Tier {
    type Err = String;
    fn from_str(word: &str) -> Result<Self, String> {
        match word {
            "full" => Ok(Self::Full),
            "draft" => Ok(Self::Draft),
            "low" => Ok(Self::Low),
            "super-low" => Ok(Self::SuperLow),
            other => Err(format!(
                "`{other}` is not a tier: full, draft, low or super-low"
            )),
        }
    }
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
            square_mm: Frame::for_system(self.system, 1, 1).square_mm,
            layer_mm: self.printer.layer_mm,
        }
    }

    /// The build target: the base system, and the printer unless it is the
    /// default one (`db`, `ol`, `db-fine`). It names the output folder and
    /// files and the build record.
    #[must_use]
    pub fn target(&self) -> String {
        dt_kit::target_name(self.system, &self.printer.name)
    }

    /// Build for the printer profile `name` (see [`printer_profile`]).
    ///
    /// # Errors
    /// Why there is no such profile, or it does not decode.
    pub fn use_printer(&mut self, name: &str, home: Option<&Path>) -> Result<(), Vec<Diag>> {
        self.use_printer_in(name, &crate::machine::OnDisk { home })
    }

    /// [`Options::use_printer`], with the profiles found where `profiles` says.
    ///
    /// # Errors
    /// Why there is no such profile, or it does not decode.
    pub fn use_printer_in(
        &mut self,
        name: &str,
        profiles: &dyn crate::machine::Profiles,
    ) -> Result<(), Vec<Diag>> {
        self.printer = Profile {
            name: name.to_owned(),
            gate: crate::machine::printer_profile_in(profiles, name)?,
        };
        Ok(())
    }
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
        Self::in_source(&Library)
    }

    /// [`Options::new`] with the default printer read through `sources`
    /// (the browser build has no disk: its library is a map).
    ///
    /// # Errors
    /// As [`Options::new`].
    pub fn in_source(sources: &dyn Source) -> Result<Self, Vec<Diag>> {
        let printer = doc::printer(sources, doc::PRINTER)?;
        Ok(Self {
            system: dt_lang::System::Db,
            seed: None,
            printer: Profile {
                name: "default".to_owned(),
                gate: printer,
            },
            engine: concat!("dt-eval-", env!("CARGO_PKG_VERSION")).to_owned(),
            tier: Tier::Full,
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
    #[expect(
        clippy::disallowed_methods,
        reason = "a failure that panics with its message named (a broken invariant or a failing test): shown loudly, not swallowed"
    )]
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| panic!("the bundled printer must decode: {e:?}"))
    }
}
