//! Printer profiles: where they are kept, how they are named and read, and
//! the machine a profile describes (its bed, nozzle and filament slots).
//!
//! A profile is a file in `library/printers/` (`default.dtile`, or by brand
//! and model, `bambu/x1c.dtile`) or in `printers/` beside your tiles.  Its
//! printability floors feed the build's gate ([`printer_profile`]); its
//! machine feeds plates ([`printer_machine`]).  A profile with a bed may
//! leave the floors out (they are the default profile's), and one with
//! `from` starts from another profile and changes what it names.

use std::path::{Path, PathBuf};

use dt_kit::Printer;

use crate::diag::Diag;
use crate::doc;
use crate::source::{self, Files, Library};

/// Where a printer profile is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileKind {
    /// `library/printers/`, shipped with the program.
    Library,
    /// `printers/` beside the user's own tiles (`$DT_HOME`).
    Mine,
}

/// A printer profile's name is a file name: letters, digits, `-` and `_`,
/// or two of them with a `/` between, `brand/model` (`bambu/x1c`).
fn profile_name_ok(name: &str) -> bool {
    let seg = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    let mut parts = name.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), None, _) => seg(a),
        (Some(a), Some(b), None) => seg(a) && seg(b),
        _ => false,
    }
}

/// Every printer profile there is: the library's and those in
/// `<home>/printers/`, by name (`default`, `fine`, `bambu/x1c`), a profile
/// of yours replacing the library's of the same name.
#[must_use]
pub fn printer_names(home: Option<&Path>) -> Vec<(String, ProfileKind)> {
    let mut found: std::collections::BTreeMap<String, ProfileKind> =
        std::collections::BTreeMap::new();
    let mut scan = |dir: PathBuf, kind: ProfileKind| {
        // a folder that is not there (no printers of your own yet) lists nothing
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut note = |prefix: &str, p: &Path| {
            if p.extension().is_some_and(|x| x == "dtile")
                && let Some(n) = p.file_stem().and_then(|n| n.to_str())
            {
                let name = format!("{prefix}{n}");
                if profile_name_ok(&name) {
                    found.insert(name, kind);
                }
            }
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                let Some(brand) = p.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
                    continue;
                };
                if let Ok(inner) = std::fs::read_dir(&p) {
                    for f in inner.flatten() {
                        note(&format!("{brand}/"), &f.path());
                    }
                }
            } else {
                note("", &p);
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

/// A printer profile as read: the gate's floors and the machine.
struct Loaded {
    floors: [f64; 5],
    machine: Machine,
}

/// The profile called `name`, with `from` followed and the floors and
/// machine words a profile leaves out taken from the one it starts from.
fn load(name: &str, home: Option<&Path>, depth: u8) -> Result<Loaded, Vec<Diag>> {
    if !profile_name_ok(name) {
        return Err(vec![
            Diag::plain("dt::printer", format!("`{name}` is not a printer name")).help(
                "a printer name is letters, digits, `-` and `_`, as `fine`, or brand and model, as `bambu/x1c`",
            ),
        ]);
    }
    let mine = home.map(|h| h.join("printers").join(format!("{name}.dtile")));
    let p = if let Some(f) = mine.filter(|f| f.is_file()) {
        doc::printer_doc(&Files, &f.to_string_lossy())?
    } else {
        let lib = format!("lib:printers/{name}.dtile");
        if source::library_file(&lib).is_some_and(|f| f.is_file()) {
            doc::printer_doc(&Library, &lib)?
        } else {
            let have: Vec<String> = printer_names(home).into_iter().map(|(n, _)| n).collect();
            return Err(vec![
                Diag::plain("dt::printer", format!("no printer profile `{name}`"))
                    .help(format!("the profiles are: {}", have.join(", "))),
            ]);
        }
    };
    let mut floors = [
        p.line.value,
        p.layer.value,
        p.overhang.value,
        p.steepest.value,
        p.gap.value,
    ];
    let m = &p.machine;
    let text = |o: &Option<dt_doc::model::Spanned<String>>| o.as_ref().map(|s| s.value.clone());
    let mut machine = Machine {
        name: name.to_owned(),
        brand: text(&m.brand),
        model: text(&m.model),
        bed: m.bed.as_ref().map(|b| b.value),
        height: m.height.as_ref().map(|h| h.value),
        excluded: m.excluded.iter().map(|z| z.value).collect(),
        nozzle: m.nozzle.as_ref().map(|n| n.value),
        slots: m.slots.as_ref().map(|n| n.value),
        template: text(&m.template),
        verify: text(&m.verify),
        from_slicer: text(&m.from_slicer),
    };
    if let Some(from) = &m.from {
        let parent = from
            .value
            .strip_prefix("lib:printers/")
            .unwrap_or(&from.value)
            .trim_end_matches(".dtile");
        if depth >= 4 || parent == name {
            return Err(vec![
                Diag::plain("dt::printer", format!("`{name}` starts from itself"))
                    .help("`from` names another profile, which may not lead back here"),
            ]);
        }
        let base = load(parent, home, depth + 1)?;
        for (i, f) in floors.iter_mut().enumerate() {
            if !p.floors_given[i] {
                *f = base.floors[i];
            }
        }
        let b = base.machine;
        machine.brand = machine.brand.or(b.brand);
        machine.model = machine.model.or(b.model);
        machine.bed = machine.bed.or(b.bed);
        machine.height = machine.height.or(b.height);
        if machine.excluded.is_empty() {
            machine.excluded = b.excluded;
        }
        machine.nozzle = machine.nozzle.or(b.nozzle);
        machine.slots = machine.slots.or(b.slots);
        machine.template = machine.template.or(b.template);
        machine.verify = machine.verify.or(b.verify);
    }
    Ok(Loaded { floors, machine })
}

/// The printer profile called `name`: `<home>/printers/<name>.dtile` if
/// there is one, else `lib:printers/<name>.dtile`.  Its floors only; the
/// machine is [`printer_machine`].
///
/// # Errors
/// Why there is no such profile, or it does not decode.
pub fn printer_profile(name: &str, home: Option<&Path>) -> Result<Printer, Vec<Diag>> {
    let f = load(name, home, 0)?.floors;
    Ok(Printer::new(f[0], f[1], f[2], f[4]).steepest(f[3]))
}

/// The machine a printer profile describes (see [`Machine`]).
///
/// # Errors
/// Why there is no such profile, or it does not decode.
pub fn printer_machine(name: &str, home: Option<&Path>) -> Result<Machine, Vec<Diag>> {
    Ok(load(name, home, 0)?.machine)
}

/// The machine of a printer profile, as plates see it: what to lay out on
/// and how many filaments it holds.  A word a profile does not say is
/// `None`; [`Machine::nozzle_mm`] and [`Machine::slot_count`] give the
/// defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct Machine {
    /// The profile's name (`bambu/x1c`).
    pub name: String,
    /// The maker, `Bambu Lab`.
    pub brand: Option<String>,
    /// The model, `X1 Carbon`.
    pub model: Option<String>,
    /// The printable area, mm: width by depth.  `None` for a profile that
    /// is only floors (`default`, `fine`): it cannot hold plates.
    pub bed: Option<[f64; 2]>,
    /// The tallest print, mm.
    pub height: Option<f64>,
    /// Zones of the bed nothing can stand on, `[x0, y0, x1, y1]` mm from
    /// the bed's front-left corner.
    pub excluded: Vec<[f64; 4]>,
    /// The nozzle, mm.
    pub nozzle: Option<f64>,
    /// Filaments loaded at once.
    pub slots: Option<u32>,
    /// The slicer template, if the profile names one.
    pub template: Option<String>,
    /// Why the numbers are not yet checked against the maker.
    pub verify: Option<String>,
    /// The slicer profile it was imported from (`from-slicer`); not
    /// inherited.
    pub from_slicer: Option<String>,
}

impl Machine {
    /// The nozzle, mm (0.4 where a profile does not say).
    #[must_use]
    pub fn nozzle_mm(&self) -> f64 {
        self.nozzle.unwrap_or(0.4)
    }

    /// Filaments loaded at once (1 where a profile does not say).
    #[must_use]
    pub fn slot_count(&self) -> u32 {
        self.slots.unwrap_or(1)
    }

    /// Does the profile make Bambu Studio projects (its brand is Bambu Lab)?
    #[must_use]
    pub fn is_bambu(&self) -> bool {
        self.name.starts_with("bambu/")
            || self
                .brand
                .as_deref()
                .is_some_and(|b| b.eq_ignore_ascii_case("bambu lab"))
    }

    /// A human name: `Bambu Lab X1 Carbon`, else the profile's name.
    #[must_use]
    pub fn label(&self) -> String {
        match (&self.brand, &self.model) {
            (Some(b), Some(m)) => format!("{b} {m}"),
            (None, Some(m)) => m.clone(),
            _ => self.name.clone(),
        }
    }

    /// The bed, or the diagnostic that says this profile has none.
    ///
    /// # Errors
    /// A profile with no `bed`.
    pub fn bed_or_say(&self) -> Result<[f64; 2], Diag> {
        self.bed.ok_or_else(|| {
            Diag::plain(
                "dt::printer",
                format!("the printer profile `{}` has no bed", self.name),
            )
            .help("plates need a machine: add `bed \"256x256mm\"` to the profile, or pick one such as `bambu/x1c`")
        })
    }
}
