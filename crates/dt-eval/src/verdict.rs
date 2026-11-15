//! **verdict**: the one place a build is judged.  Every front end asks [`Built::verdict`] and reads the answer; none
//! decides "passed" itself.  A [`Passed`] is the proof the gate checked a build and found nothing: only this module can
//! make one, and every writer of a printer's file (STL, 3MF, plates, a Bambu project) takes one, so a low, draft or
//! failed build has no way into a file (`dt_print::stl` shows it in a `compile_fail` doctest).

use dt_kit::{Material, Printer};

use crate::build::Built;

/// The gate checked this build and it passed.  Reads as the build (`passed.export`, `passed.record`).  It cannot be
/// made outside this crate.
#[derive(Clone, Copy)]
pub struct Passed<'a> {
    built: &'a Built,
}

impl<'a> Passed<'a> {
    /// The build that passed.
    #[must_use]
    pub const fn built(&self) -> &'a Built {
        self.built
    }

    /// Each written triangle's material.
    #[must_use]
    pub fn materials(&self) -> &'a [Material] {
        &self.built.materials
    }
}

impl std::ops::Deref for Passed<'_> {
    type Target = Built;
    fn deref(&self) -> &Built {
        self.built
    }
}

impl std::fmt::Debug for Passed<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Passed")
    }
}

/// What the gate says of a build.
#[derive(Debug)]
pub enum Verdict<'a> {
    /// Checked, and nothing failed.
    Passed(Passed<'a>),
    /// Checked, and these failed, in words.
    Failed(Vec<String>),
    /// Not checked: a low, draft or super-low build, with each face flagged by its angle alone.
    NotChecked,
}

impl<'a> Verdict<'a> {
    /// The proof, when the build passed.
    #[must_use]
    pub const fn passed(&self) -> Option<Passed<'a>> {
        match self {
            Verdict::Passed(p) => Some(*p),
            _ => None,
        }
    }

    /// The gate's failures: empty for a pass and for a build that was not checked.
    #[must_use]
    pub fn reasons(&self) -> &[String] {
        match self {
            Verdict::Failed(f) => f,
            _ => &[],
        }
    }
}

impl Built {
    /// The gate's verdict on this build under `printer`: the one decision of "passed".
    #[must_use]
    pub fn verdict(&self, printer: &Printer) -> Verdict<'_> {
        if !self.checked {
            return Verdict::NotChecked;
        }
        let failures = self.audit.failures(printer);
        if failures.is_empty() {
            Verdict::Passed(Passed { built: self })
        } else {
            Verdict::Failed(failures)
        }
    }
}
