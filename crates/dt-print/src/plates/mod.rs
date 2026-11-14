//! Plates: a print list laid on a printer's bed.
//!
//! [`plan`] reads a print list ([`dt_doc::map`]) and the tiles it names (no
//! geometry): which printer, which settings, which pieces, and whether the
//! printer has the filaments.  [`layout`] puts the pieces on plates once their
//! real footprints are known (from a build: [`Boxes`]).  [`pack`] holds the
//! packer, `export` the files.  Nothing here builds a tile: a front end builds
//! each distinct piece (the CLI for an export, the editor for its view) at the
//! tier it needs, then asks for the layout.

pub mod export;
pub mod pack;

use std::path::{Path, PathBuf};

use dt_doc::inventory::{Colour, Identity, Shelf};
use dt_doc::map::{DEFAULT_PRINTER, Group, Mode, PrintList, PrintMode};
use dt_kit::Material;

use dt_eval::source::{LIB, Source};
use dt_eval::{Diag, Machine, Options, printer_machine};
use pack::{Item, Packing, Placed, Spec};

/// What a caller says beyond the list itself (the command line's words).
#[derive(Clone, Debug, Default)]
pub struct Overrides {
    /// A printer profile in place of the list's.
    pub printer: Option<String>,
    /// A base system in place of the list's.
    pub system: Option<String>,
    /// A print mode in place of the list's.
    pub mode: Option<Mode>,
    /// A grouping in place of the list's.
    pub group: Option<Group>,
    /// Spacing and margin, mm.
    pub gap: Option<f64>,
    /// See [`Overrides::gap`].
    pub margin: Option<f64>,
    /// A turn setting in place of the list's.
    pub turn: Option<bool>,
    /// Look in the inventory (`true`) or print every count (`false`: the
    /// command line's `--all`), in place of the list's.
    pub inventory: Option<bool>,
}

/// Where the list lives and what it can see.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// Reads the tiles.
    pub sources: &'a dyn Source,
    /// The folder the list is in: a tile path with no `lib:` or `home:` is
    /// read from here.
    pub list_dir: Option<&'a Path>,
    /// `home:` and your own printers: `$DT_HOME`, else `~/dharmatiles`.
    pub home: Option<&'a Path>,
    /// Build options to start from: the engine label, and the cells.
    pub base: &'a Options,
    /// What you have printed (the inventory in `home`, read), or `None` where
    /// there is no inventory yet: a list then prints everything it names.
    pub shelf: Option<&'a Shelf>,
}

/// The settings a layout is made with.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The printer.
    pub machine: Machine,
    /// The base system pieces are built for, where a piece names none.
    pub system: String,
    /// Single colour or multicolour.
    pub mode: Mode,
    /// How multicolour pieces are grouped.
    pub group: Group,
    /// Between two pieces, mm.
    pub gap: f64,
    /// Between a piece and the bed's edge, mm.
    pub margin: f64,
    /// May a piece turn a quarter.
    pub turn: bool,
    /// Pieces print only what the inventory lacks (`plates inventory=check`,
    /// the default).
    pub check_inventory: bool,
}

/// The gap between pieces, mm, where none is written.
pub const GAP_MM: f64 = 3.0;
/// The margin to the bed's edge, mm, where none is written.
pub const MARGIN_MM: f64 = 5.0;

/// One distinct piece: a tile, at a seed, for a system.  Copies share it.
#[derive(Clone, Debug)]
pub struct Kind {
    /// The tile as the list wrote it.
    pub tile: String,
    /// The path the tile is read from.
    pub path: String,
    /// The seed it is built with, if the list gives one.
    pub seed: Option<u64>,
    /// The base system it is built for.
    pub system: String,
    /// How many copies print.
    pub count: u32,
    /// How many copies the list asks for (`print=missing` ones, and the
    /// `print=always` ones).
    pub want: u32,
    /// How many of those the inventory already holds, and so are left off.
    pub have: u32,
    /// How many of this tile and seed you have in the other colour mode (a
    /// piece wanted in multicolour and owned in single colour is `have, single
    /// colour`, and still prints unless you decide otherwise).
    pub other_colour: u32,
    /// The tile as the inventory names it (`lib:tiles/ground/1x1-grass`).
    pub canon: String,
    /// The seed it is built with: the list's, else the tile's own.
    pub seed_used: u64,
    /// The first eight hex digits of the tile document's hash, as the log
    /// writes it.
    pub hash: String,
    /// Its short name (`1x1-grass`).
    pub name: String,
    /// The list's piece each printed copy is (an index into the list's
    /// pieces), so a pin finds its copy.
    pub copies: Vec<usize>,
    /// Its footprint if it were exactly its squares, mm: the layout's
    /// stand-in until a build says what it really covers.
    pub nominal: [f64; 2],
    /// The filaments the tile's stack says it needs, at most (see
    /// [`estimate`]); a build gives the real ones.
    pub estimate: Vec<Material>,
}

/// A print list read and checked, not yet laid out.
#[derive(Clone, Debug)]
pub struct Plan {
    /// What it is laid out with.
    pub settings: Settings,
    /// The distinct pieces, in the order the list first names them.
    pub kinds: Vec<Kind>,
    /// Pieces the list leaves off (`print=skip`), by tile.
    pub skipped: Vec<String>,
    /// Pieces the inventory already holds, left off: (name, how many).
    pub held: Vec<(String, u32)>,
    /// The list's own name (its file's stem).
    pub title: String,
    /// The pins that name a piece that prints.
    pub pins: Vec<PinSpec>,
    /// Pins whose piece does not print (left off or on your shelf): the id
    /// and why, to be said.
    pub loose_pins: Vec<(String, String)>,
}

/// A pin of the list, resolved to the copy it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct PinSpec {
    /// The piece's id.
    pub id: String,
    /// Which distinct piece ([`Plan::kinds`]).
    pub kind: usize,
    /// Which copy of it.
    pub copy: usize,
    /// The plate, from 0.
    pub plate: usize,
    /// Where its footprint's north-west corner stands, across and down from
    /// the bed's north-west corner, mm.
    pub at: [f64; 2],
    /// Turned a quarter.
    pub turn: bool,
}

impl Kind {
    /// What the inventory calls the piece, for `colour`.
    #[must_use]
    pub fn identity(&self, colour: Colour) -> Identity {
        Identity {
            tile: self.canon.clone(),
            seed: self.seed_used,
            system: self.system.clone(),
            colour,
        }
    }
}

/// The colour mode a print mode makes.
#[must_use]
pub const fn colour_of(mode: Mode) -> Colour {
    match mode {
        Mode::Single => Colour::Single,
        Mode::Multicolour => Colour::Multi,
    }
}

impl Plan {
    /// The options piece `k` is built with: the list's system and the
    /// piece's seed, for the printer's floors.
    ///
    /// # Errors
    /// The printer profile cannot be read.
    pub fn options(
        &self,
        k: usize,
        base: &Options,
        home: Option<&Path>,
    ) -> Result<Options, Vec<Diag>> {
        let kind = &self.kinds[k];
        let mut o = Options {
            system: kind.system.clone(),
            seed: kind.seed.or(base.seed),
            ..base.clone()
        };
        o.use_printer(&self.settings.machine.name, home)?;
        Ok(o)
    }

    /// How many pieces in all.
    #[must_use]
    pub fn pieces(&self) -> usize {
        self.kinds.iter().map(|k| k.count as usize).sum()
    }
}

/// The materials a tile's stack can make, by what its elements write.  An
/// upper bound that never leaves one out (a tree with no leaves still counts
/// its foliage): the printer's slots are checked against it before anything
/// builds, and a build gives the real filaments.  Earth is always there: the
/// connector prints in it.
#[must_use]
pub fn estimate(tile: &dt_eval::Tile) -> Vec<Material> {
    fn of(what: &str, controls: &[dt_eval::doc::Setting], out: &mut Vec<Material>) {
        // a winter tree (`leaves=bare`) has no clumps and no leaves: no leaf material
        let bare = controls.iter().any(|c| {
            c.name == "leaves" && matches!(&c.value, dt_elements::Value::Word(w) if w == "bare")
        });
        match what {
            "ground" | "dirt" => out.push(Material::Earth),
            "rock" | "stone" | "boulder" | "pebble" | "cliff" => out.push(Material::Rock),
            "flower" => out.push(Material::Flower),
            "water" => out.push(Material::Water),
            "grass" | "reeds" => out.push(Material::Grass),
            "wall" | "floor" => out.push(Material::Masonry),
            "tree" if bare => out.push(Material::Wood),
            "tree" => out.extend([Material::Wood, Material::Leaf]),
            _ => {}
        }
    }
    let mut out = vec![Material::Earth];
    for e in &tile.stack {
        of(&e.what.value, &e.controls, &mut out);
        for c in &e.children {
            of(&c.what.value, &c.controls, &mut out);
        }
    }
    crate::filaments_of(&out)
}

/// The path a piece's tile is read from: `lib:tiles/ground/1x1-grass` is the
/// library's, `home:meadows/glade` is under `home`, anything else is beside
/// the list (or absolute).  A missing `.dtile` is added.
///
/// # Errors
/// `home:` with no home, or a relative path with no folder to be relative to.
pub fn tile_path(
    tile: &str,
    list_dir: Option<&Path>,
    home: Option<&Path>,
) -> Result<String, String> {
    let with_ext = |p: String| {
        if p.to_ascii_lowercase().ends_with(".dtile") {
            p
        } else {
            format!("{p}.dtile")
        }
    };
    if tile.starts_with(LIB) {
        return Ok(with_ext(tile.to_owned()));
    }
    if let Some(rest) = tile.strip_prefix("home:") {
        let h = home.ok_or(
            "`home:` needs your tiles folder, and there is none ($DT_HOME, else ~/dharmatiles)",
        )?;
        return Ok(with_ext(
            h.join(rest.trim_start_matches('/'))
                .to_string_lossy()
                .into_owned(),
        ));
    }
    let p = PathBuf::from(tile);
    if p.is_absolute() {
        return Ok(with_ext(tile.to_owned()));
    }
    let d = list_dir.ok_or_else(|| {
        format!("`{tile}` is relative to the list's folder, and this list has none: write `lib:`, `home:` or an absolute path")
    })?;
    Ok(with_ext(d.join(p).to_string_lossy().into_owned()))
}

/// Read a print list's tiles and settings.
///
/// # Errors
/// Every problem found, each at the text that caused it: an unknown printer or
/// one with no bed, a tile that is missing or does not check, a printer with
/// fewer filaments than a piece needs in multicolour.
pub fn plan(
    list: &PrintList,
    title: &str,
    ctx: &Context,
    ov: &Overrides,
) -> Result<Plan, Vec<Diag>> {
    let mut errors: Vec<Diag> = Vec::new();
    let pl = &list.plates;
    let printer = ov
        .printer
        .clone()
        .or_else(|| pl.printer.as_ref().map(|p| p.value.clone()))
        .unwrap_or_else(|| DEFAULT_PRINTER.to_owned());
    let machine = match printer_machine(&printer, ctx.home) {
        Ok(m) => Some(m),
        Err(mut e) => {
            // a mistake in the list's own `printer=` points there
            if ov.printer.is_none()
                && let Some(p) = &pl.printer
            {
                e = e
                    .into_iter()
                    .map(|d| {
                        Diag::at(
                            &list.src,
                            p.span,
                            dt_lang::codes::PRINTER,
                            d.message,
                            "this printer",
                        )
                        .maybe_help(d.help)
                    })
                    .collect();
            }
            errors.extend(e);
            None
        }
    };
    if let Some(m) = &machine
        && let Err(d) = m.bed_or_say()
    {
        let d = match (&pl.printer, ov.printer.is_none()) {
            (Some(p), true) => {
                Diag::at(&list.src, p.span, d.code, d.message, "no bed").maybe_help(d.help)
            }
            _ => d,
        };
        errors.push(d);
    }
    let system = ov
        .system
        .clone()
        .or_else(|| pl.system.as_ref().map(|s| s.value.clone()))
        .unwrap_or_else(|| ctx.base.system.clone());
    let slots = machine.as_ref().map_or(1, Machine::slot_count);
    let mode = ov
        .mode
        .or_else(|| pl.mode.as_ref().map(|m| m.value))
        .unwrap_or(if slots > 1 {
            Mode::Multicolour
        } else {
            Mode::Single
        });
    let group = ov
        .group
        .or_else(|| pl.group.as_ref().map(|g| g.value))
        .unwrap_or(Group::Filament);
    let gap = ov
        .gap
        .or_else(|| pl.gap.as_ref().map(|g| g.value))
        .unwrap_or(GAP_MM);
    let margin = ov
        .margin
        .or_else(|| pl.margin.as_ref().map(|g| g.value))
        .unwrap_or(MARGIN_MM);
    let turn = ov
        .turn
        .or_else(|| pl.turn.as_ref().map(|g| g.value))
        .unwrap_or(true);
    let check_inventory = ov
        .inventory
        .or_else(|| pl.inventory.as_ref().map(|g| g.value))
        .unwrap_or(true);
    let colour = colour_of(mode);

    let mut kinds: Vec<Kind> = Vec::new();
    let mut skipped = Vec::new();
    // pieces that ask for the whole count whatever you have, apart from those
    // that print what is missing: one kind may be both
    let mut always: Vec<u32> = Vec::new();
    for (pi, p) in list.pieces.iter().enumerate() {
        if p.print == PrintMode::Skip {
            skipped.push(p.tile.value.clone());
            continue;
        }
        let path = match tile_path(&p.tile.value, ctx.list_dir, ctx.home) {
            Ok(x) => x,
            Err(e) => {
                errors.push(Diag::at(
                    &list.src,
                    p.tile.span,
                    dt_lang::codes::MAP,
                    e,
                    "this tile",
                ));
                continue;
            }
        };
        let sys = p
            .system
            .as_ref()
            .map_or_else(|| system.clone(), |s| s.value.clone());
        let seed = p.seed.as_ref().map(|s| s.value);
        if let Some(i) = kinds
            .iter()
            .position(|k| k.path == path && k.seed == seed && k.system == sys)
        {
            kinds[i].want += p.count.value;
            kinds[i]
                .copies
                .extend(std::iter::repeat_n(pi, p.count.value as usize));
            if p.print == PrintMode::Always {
                always[i] += p.count.value;
            }
            continue;
        }
        let opts = Options {
            system: sys.clone(),
            seed,
            ..ctx.base.clone()
        };
        let checked = match dt_eval::check(ctx.sources, &path, &opts) {
            Ok(c) => c,
            Err(e) => {
                // the tile's own mistakes, where the tile writes them; and where the
                // list names it
                errors.push(
                    Diag::at(
                        &list.src,
                        p.tile.span,
                        dt_lang::codes::MAP,
                        format!("`{}` does not check", p.tile.value),
                        "this tile",
                    )
                    .help(e.first().map_or_else(String::new, |d| d.message.clone())),
                );
                errors.extend(e);
                continue;
            }
        };
        let Some(tile) = checked.tile.as_ref() else {
            errors.push(Diag::at(
                &list.src,
                p.tile.span,
                dt_lang::codes::MAP,
                format!("`{}` is not a tile", p.tile.value),
                "no tile in it",
            ));
            continue;
        };
        let frame = dt_kit::Frame::for_system(&sys, tile.cols, tile.rows);
        let hash =
            crate::inventory::tile_hash(&crate::inventory::canonical(&path, ctx.home), ctx.home)
                .or_else(|| {
                    // a tile that is not under `lib:` or home is read by its path
                    ctx.sources
                        .read(&path)
                        .ok()
                        .map(|t| crate::inventory::short_hash(dt_eval::doc::text_hash(&t)))
                })
                .unwrap_or_default();
        always.push(if p.print == PrintMode::Always {
            p.count.value
        } else {
            0
        });
        kinds.push(Kind {
            want: p.count.value,
            have: 0,
            other_colour: 0,
            canon: crate::inventory::canonical(&path, ctx.home),
            seed_used: checked.seed,
            hash,
            tile: p.tile.value.clone(),
            name: path
                .rsplit(['/', ':'])
                .next()
                .unwrap_or(&path)
                .trim_end_matches(".dtile")
                .to_owned(),
            copies: vec![pi; p.count.value as usize],
            path,
            seed,
            system: sys,
            count: p.count.value,
            nominal: frame.size_mm(),
            estimate: estimate(tile),
        });
    }
    // what you already have is left off: the count, less what the inventory holds
    let mut held: Vec<(String, u32)> = Vec::new();
    if let Some(shelf) = ctx.shelf {
        for (i, k) in kinds.iter_mut().enumerate() {
            let id = k.identity(colour);
            k.other_colour = shelf.other_colour(&id);
            let owned = shelf.have(&id);
            if check_inventory {
                let asked = k.want - always[i];
                k.have = owned.min(asked);
            } else {
                k.have = 0;
            }
            k.count = k.want - k.have;
            k.copies.truncate(k.count as usize);
        }
        let (keep, left): (Vec<Kind>, Vec<Kind>) = kinds.into_iter().partition(|k| k.count > 0);
        held.extend(left.iter().map(|k| (k.name.clone(), k.have)));
        kinds = keep;
    } else {
        for k in &mut kinds {
            k.count = k.want;
        }
    }
    if kinds.is_empty() && errors.is_empty() {
        errors.push(if held.is_empty() {
            Diag::at(
                &list.src,
                list.span,
                dt_lang::codes::MAP,
                "there is nothing to print",
                "no pieces",
            )
            .help("every piece is `print=skip`, or the list has none: add `piece \"lib:tiles/ground/1x1-grass\" count=4`")
        } else {
            Diag::at(
                &list.src,
                list.span,
                dt_lang::codes::INVENTORY,
                "everything in this list is already in your inventory",
                "all of it is on your shelf",
            )
            .help("print it anyway with `--all` (in the editor, turn off \"only what I don't have\"), or mark a piece `print=always`")
        });
    }
    // the printer's filaments, before anything builds
    if mode == Mode::Multicolour {
        for k in &kinds {
            if k.estimate.len() > slots as usize {
                let names: Vec<&str> = k.estimate.iter().map(|m| m.name()).collect();
                let span = list
                    .pieces
                    .iter()
                    .find(|p| p.tile.value == k.tile)
                    .map(|p| p.tile.span);
                let d = format!(
                    "{} needs {} filaments ({}), this printer has {}",
                    k.name,
                    k.estimate.len(),
                    names.join(", "),
                    slots
                );
                let help = if slots == 1 {
                    "print in single colour (`mode=\"single\"`), or choose a printer with an AMS such as bambu/x1c-ams".to_owned()
                } else {
                    "print in single colour (`mode=\"single\"`), or choose a printer with more slots".to_owned()
                };
                errors.push(match span {
                    Some(s) => Diag::at(
                        &list.src,
                        s,
                        dt_lang::codes::SLOTS,
                        d,
                        "needs too many filaments",
                    )
                    .help(help),
                    None => Diag::plain(dt_lang::codes::SLOTS, d).help(help),
                });
            }
        }
    }
    let Some(machine) = machine.filter(|_| errors.is_empty()) else {
        return Err(errors);
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    // pins: each to the copy of the piece it names, if that piece prints
    let mut pins = Vec::new();
    let mut loose_pins = Vec::new();
    for pin in &list.plates.pins {
        let Some(pi) = list
            .pieces
            .iter()
            .position(|p| p.id.as_ref().is_some_and(|i| i.value == pin.id.value))
        else {
            continue;
        };
        let found = kinds.iter().enumerate().find_map(|(k, kind)| {
            kind.copies
                .iter()
                .position(|c| *c == pi)
                .map(|copy| (k, copy))
        });
        let Some((kind, copy)) = found else {
            let why = if list.pieces[pi].print == PrintMode::Skip {
                "it is `print=skip`"
            } else {
                "your inventory already holds it"
            };
            loose_pins.push((pin.id.value.clone(), why.to_owned()));
            continue;
        };
        let Some((at, _)) = pin.at else { continue };
        let sq = dt_kit::Frame::for_system(&kinds[kind].system, 1, 1).square_mm;
        pins.push(PinSpec {
            id: pin.id.value.clone(),
            kind,
            copy,
            plate: (pin.plate.value as usize).saturating_sub(1),
            at: [at.x.mm(sq), at.y.mm(sq)],
            turn: pin.turn,
        });
    }
    Ok(Plan {
        settings: Settings {
            machine,
            system,
            mode,
            group,
            gap,
            margin,
            turn,
            check_inventory,
        },
        kinds,
        skipped,
        held,
        title: title.to_owned(),
        pins,
        loose_pins,
    })
}

/// What a piece really covers, from a build of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Boxes {
    /// Its lowest corner in the piece's own coordinates, mm: the box the
    /// footprint was measured on, kept so a thinned mesh of the piece stands
    /// where the full one would.
    pub lo: [f64; 3],
    /// Its footprint, the top-down box of the whole piece (a crown over the
    /// edge included): width by depth, mm.
    pub size: [f64; 2],
    /// Its height from its lowest point, mm.
    pub height: f64,
    /// The filaments it prints in, in order.
    pub filaments: Vec<Material>,
}

impl Boxes {
    /// Its highest corner, mm.
    #[must_use]
    pub fn hi(&self) -> [f64; 3] {
        [
            self.lo[0] + self.size[0],
            self.lo[1] + self.size[1],
            self.lo[2] + self.height,
        ]
    }

    /// From a mesh and the material of each of its triangles.
    #[must_use]
    pub fn of(mesh: &dt_kit::TriMesh, materials: &[Material]) -> Self {
        let (lo, hi) = crate::box_of(mesh);
        Self {
            lo,
            size: [hi[0] - lo[0], hi[1] - lo[1]],
            height: hi[2] - lo[2],
            filaments: crate::filaments_of(materials),
        }
    }
}

/// One piece on a plate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    /// Which distinct piece ([`Plan::kinds`]).
    pub kind: usize,
    /// Which copy of it, from 0.
    pub copy: usize,
    /// Where it stands.
    pub at: Placed,
}

/// The plates.
#[derive(Clone, Debug)]
pub struct Layout {
    /// The pieces on each plate, in the order they were put down.
    pub plates: Vec<Vec<Instance>>,
    /// The filaments each plate needs (empty in single colour).
    pub filaments: Vec<Vec<Material>>,
    /// More plates grouping by filament took than size-only packing would
    /// have.
    pub grouping_cost: Option<usize>,
    /// Pins that could not stand where they said: the piece's id and why.
    pub bad_pins: Vec<(String, String)>,
    /// The pinned pieces, as (kind, copy).
    pub pinned: Vec<(usize, usize)>,
}

impl Layout {
    /// How many pieces in all.
    #[must_use]
    pub fn pieces(&self) -> usize {
        self.plates.iter().map(Vec::len).sum()
    }
}

/// The packer's spec for a plan.
///
/// # Errors
/// A printer with no bed.
pub fn spec_of(settings: &Settings) -> Result<Spec, Diag> {
    Ok(Spec {
        bed: settings.machine.bed_or_say()?,
        excluded: settings.machine.excluded.clone(),
        gap: settings.gap,
        margin: settings.margin,
        turn: settings.turn,
        slots: settings.machine.slot_count(),
        mode: settings.mode,
        group: settings.group,
    })
}

/// Put every piece on a plate.  `boxes[k]` is what piece `k` really covers.
///
/// # Errors
/// A piece bigger than the bed or the printer's height, or needing more
/// filaments than the printer has: each named, by tile.
pub fn layout(plan: &Plan, boxes: &[Boxes]) -> Result<Layout, Vec<Diag>> {
    let spec = spec_of(&plan.settings).map_err(|d| vec![d])?;
    let mut items: Vec<Item> = Vec::new();
    let mut who: Vec<(usize, usize)> = Vec::new();
    for (k, kind) in plan.kinds.iter().enumerate() {
        for copy in 0..kind.count as usize {
            items.push(Item {
                id: items.len(),
                size: boxes[k].size,
                filaments: boxes[k].filaments.clone(),
            });
            who.push((k, copy));
        }
    }
    let mut errors = Vec::new();
    if let Some(h) = plan.settings.machine.height {
        for (k, kind) in plan.kinds.iter().enumerate() {
            if boxes[k].height > h + 1e-6 {
                errors.push(
                    Diag::plain(
                        dt_lang::codes::PLATES,
                        format!(
                            "{} stands {:.1} mm tall and the {} prints up to {h} mm",
                            kind.name,
                            boxes[k].height,
                            plan.settings.machine.label()
                        ),
                    )
                    .help("choose a taller printer, or a shorter tile"),
                );
            }
        }
    }
    // pins: the footprint where it was dropped, from the bed's north-west corner
    let mut starts = Vec::new();
    let mut base = 0;
    for kind in &plan.kinds {
        starts.push(base);
        base += kind.count as usize;
    }
    let mut pins = Vec::new();
    for p in &plan.pins {
        if p.copy >= plan.kinds[p.kind].count as usize {
            continue;
        }
        let [w, d] = boxes[p.kind].size;
        let (pw, pd) = if p.turn { (d, w) } else { (w, d) };
        pins.push(pack::Pin {
            id: starts[p.kind] + p.copy,
            plate: p.plate,
            at: Placed {
                id: starts[p.kind] + p.copy,
                x: p.at[0],
                y: spec.bed[1] - p.at[1] - pd,
                turned: p.turn,
                w: pw,
                d: pd,
            },
        });
    }
    let packing: Packing = pack::pack_with(&items, &pins, &spec);
    for &id in &packing.too_big {
        let (k, _) = who[id];
        let s = boxes[k].size;
        errors.push(
            Diag::plain(
                dt_lang::codes::PLATES,
                format!(
                    "{} covers {:.1} x {:.1} mm and does not fit the {} x {} mm bed ({} mm margin, zones left out)",
                    plan.kinds[k].name, s[0], s[1], spec.bed[0], spec.bed[1], spec.margin
                ),
            )
            .help("choose a bigger bed, or a smaller margin"),
        );
    }
    for &id in &packing.too_colourful {
        let (k, _) = who[id];
        errors.push(
            Diag::plain(
                dt_lang::codes::SLOTS,
                format!(
                    "{} needs {} filaments, this printer has {}",
                    plan.kinds[k].name,
                    boxes[k].filaments.len(),
                    spec.slots
                ),
            )
            .help("print in single colour, or choose a printer with more slots"),
        );
    }
    if !errors.is_empty() {
        errors.dedup_by(|a, b| a.message == b.message);
        return Err(errors);
    }
    let plates = packing
        .plates
        .iter()
        .map(|p| {
            p.placed
                .iter()
                .map(|at| {
                    let (kind, copy) = who[at.id];
                    Instance {
                        kind,
                        copy,
                        at: *at,
                    }
                })
                .collect()
        })
        .collect();
    let id_of = |item: usize| {
        let (k, c) = who[item];
        plan.pins
            .iter()
            .find(|p| p.kind == k && p.copy == c)
            .map_or_else(String::new, |p| p.id.clone())
    };
    let pinned_items: Vec<usize> = pins
        .iter()
        .map(|p| p.id)
        .filter(|id| !packing.bad_pins.iter().any(|(b, _)| b == id))
        .collect();
    Ok(Layout {
        plates,
        filaments: packing.plates.iter().map(|p| p.filaments.clone()).collect(),
        grouping_cost: packing.grouping_cost,
        bad_pins: packing
            .bad_pins
            .iter()
            .map(|(i, why)| (id_of(*i), (*why).to_owned()))
            .collect(),
        pinned: pinned_items.iter().map(|i| who[*i]).collect(),
    })
}
