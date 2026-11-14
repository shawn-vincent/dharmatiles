//! Plates as files: a Bambu Studio project with every plate, one STL per
//! plate, or one plain 3MF per plate.
//!
//! Only full builds are written: a low or super-low build is not checked and
//! is never exported ([`write_set`] refuses it, by name).

use dt_eval::{Built, Diag};
use dt_kit::{BuildRecord, Palette};

use crate::{PlatePart, PlateSet, PlateStand};

use super::{Layout, Plan};

/// What the files are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// One Bambu Studio project holding every plate (needs a template).
    Bambu,
    /// One binary STL per plate: every slicer reads it.
    Stl,
    /// One plain 3MF per plate.
    ThreeMf,
}

impl Format {
    /// As the command line writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Bambu => "bambu",
            Self::Stl => "stl",
            Self::ThreeMf => "3mf",
        }
    }

    /// What a printer's profile asks for: a Bambu project for a Bambu printer,
    /// else one STL per plate.
    #[must_use]
    pub fn for_printer(machine: &dt_eval::Machine) -> Self {
        if machine.is_bambu() {
            Self::Bambu
        } else {
            Self::Stl
        }
    }
}

/// One file to write: its name and bytes.
pub type File = (String, Vec<u8>);

/// The set of parts and stands a layout makes, for the writers.
fn set_of<'a>(
    plan: &'a Plan,
    layout: &Layout,
    built: &'a [&'a Built],
    record: &'a BuildRecord,
) -> PlateSet<'a> {
    let parts = plan
        .kinds
        .iter()
        .zip(built)
        .map(|(k, b)| PlatePart {
            name: &k.name,
            mesh: &b.export.mesh,
            materials: &b.materials,
        })
        .collect();
    let mut stands = Vec::new();
    for (plate, on) in layout.plates.iter().enumerate() {
        for i in on {
            stands.push(PlateStand {
                part: i.kind,
                plate,
                x: i.at.x,
                y: i.at.y,
                turned: i.at.turned,
            });
        }
    }
    PlateSet {
        title: &plan.title,
        parts,
        stands,
        plates: layout.plates.len(),
        multicolour: plan.settings.mode == dt_doc::map::Mode::Multicolour,
        record,
    }
}

/// The files of a layout.
///
/// `built[k]` is the full build of piece `k`.  A Bambu project needs the
/// template's project settings (`template`).
///
/// # Errors
/// A build that was not checked (only a full build is exported), a Bambu
/// project with no template or with one made for another bed.
pub fn write_set(
    plan: &Plan,
    layout: &Layout,
    built: &[&Built],
    format: Format,
    palette: &Palette,
    template: Option<&str>,
    record: &BuildRecord,
) -> Result<Vec<File>, Diag> {
    for (k, b) in plan.kinds.iter().zip(built) {
        if !b.checked {
            return Err(Diag::plain(
                "dt::plates",
                format!(
                    "{} was not built in full and is not checked: plates are never made from a draft build",
                    k.name
                ),
            ));
        }
    }
    let set = set_of(plan, layout, built, record);
    let stem = &plan.title;
    match format {
        Format::Stl => Ok((0..set.plates)
            .map(|p| {
                (
                    format!("{stem}-plate-{:02}.stl", p + 1),
                    crate::plate_stl(&set, p),
                )
            })
            .collect()),
        Format::ThreeMf => Ok((0..set.plates)
            .map(|p| {
                (
                    format!("{stem}-plate-{:02}.3mf", p + 1),
                    crate::plate_3mf(&set, p, palette),
                )
            })
            .collect()),
        Format::Bambu => {
            let t = template.ok_or_else(|| {
                Diag::plain(
                    "dt::plates",
                    "a Bambu Studio project needs a template for the printer's settings",
                )
                .help("save any project from Bambu Studio for this printer as bambu.3mf in your tiles folder (or name it with --bambu-template), or write one STL per plate with --format stl")
            })?;
            let bed = plan.settings.machine.bed_or_say()?;
            let bytes = crate::bambu_plates(&set, palette, t, bed)
                .map_err(|e| Diag::plain("dt::plates", e))?;
            Ok(vec![(format!("{stem}.3mf"), bytes)])
        }
    }
}
