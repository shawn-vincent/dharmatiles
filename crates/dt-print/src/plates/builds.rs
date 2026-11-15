//! The builds a plan's plates are written from: every distinct piece built in full, the layout their real footprints
//! give, and the files.  The command line (`dharmatiles plates`), the editor's Export in the program and the browser
//! editor's service all make their plates here; what differs is only how one piece is built (`build` below: through
//! the disk cache, a lane, or the worker's own turn) and where the files go.

use dt_eval::{Built, Diag, Options, Passed, Verdict};
use dt_kit::{BuildRecord, Palette};

use super::export::{self, File, Format};
use super::{Boxes, Layout, Plan};

/// What the build of every piece gave.
pub struct Builds {
    pub built: Vec<Built>,
    /// The printer each piece was built for, whose gate judges it.
    printers: Vec<dt_eval::Profile>,
}

impl Builds {
    /// Build every distinct piece of a plan in full, calling `step` before each with its number, the count and its
    /// name; `build` builds one tile (its path, and the options it is built with: full tier, the plan's printer).
    ///
    /// # Errors
    /// A piece's printer that does not read, or a piece that does not build.
    pub fn of(
        plan: &Plan,
        base: &Options,
        profiles: &dyn dt_eval::Profiles,
        step: &mut dyn FnMut(usize, usize, &str),
        build: &mut dyn FnMut(&str, &Options) -> Result<Built, Vec<Diag>>,
    ) -> Result<Self, Vec<Diag>> {
        let mut built = Vec::new();
        let mut printers = Vec::new();
        for (k, kind) in plan.kinds.iter().enumerate() {
            step(k + 1, plan.kinds.len(), &kind.name);
            let opts = Options {
                tier: dt_eval::Tier::Full,
                ..plan.options(k, base, profiles)?
            };
            built.push(build(&kind.path, &opts)?);
            printers.push(opts.printer);
        }
        Ok(Self { built, printers })
    }

    /// The gate's verdict on each piece.
    #[must_use]
    pub fn verdicts(&self) -> Vec<Verdict<'_>> {
        self.built
            .iter()
            .zip(&self.printers)
            .map(|(b, p)| b.verdict(p))
            .collect()
    }

    /// Each piece's gate failures (empty when it passed).
    #[must_use]
    pub fn gate(&self) -> Vec<Vec<String>> {
        self.verdicts()
            .iter()
            .map(|v| v.reasons().to_vec())
            .collect()
    }

    /// Every piece's proof, when all of them passed: the only way to write the plates.
    #[must_use]
    pub fn passed(&self) -> Option<Vec<Passed<'_>>> {
        self.verdicts().iter().map(Verdict::passed).collect()
    }

    /// The layout the builds give.
    ///
    /// # Errors
    /// A piece that does not fit, or needs too many filaments.
    pub fn layout(&self, plan: &Plan) -> Result<Layout, Vec<Diag>> {
        let boxes: Vec<Boxes> = self
            .built
            .iter()
            .map(|b| Boxes::of(&b.export.mesh, &b.materials))
            .collect();
        super::layout(plan, &boxes)
    }

    /// The record the plates' files are stamped with: the first piece's engine and document hash, for the list at
    /// `document`.
    #[must_use]
    pub fn record(&self, plan: &Plan, document: &str) -> BuildRecord {
        let b0 = &self.built[0];
        BuildRecord {
            engine: b0.record.engine.clone(),
            document: document.to_owned(),
            document_hash: b0.record.document_hash,
            seed: 0,
            system: plan.settings.system,
            printer: plan.settings.machine.name.clone(),
        }
    }

    /// The plates' files, when every piece passed the gate (none otherwise: a piece that failed is never written).
    ///
    /// # Errors
    /// A Bambu project with no template or one for another bed.
    pub fn files(
        &self,
        plan: &Plan,
        layout: &Layout,
        format: Format,
        palette: &Palette,
        template: Option<&str>,
        document: &str,
    ) -> Result<Option<Vec<File>>, Diag> {
        let Some(passed) = self.passed() else {
            return Ok(None);
        };
        export::write_set(
            plan,
            layout,
            &passed,
            format,
            palette,
            template,
            &self.record(plan, document),
        )
        .map(Some)
    }
}
