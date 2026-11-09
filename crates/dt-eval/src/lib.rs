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

mod cache;
pub mod diag;
pub mod doc;
mod hash;
pub mod places;
pub mod plan;
pub mod progress;
pub mod solve;
pub mod source;

use std::path::{Path, PathBuf};

use dt_doc::schema::did_you_mean;
use dt_elements::{Control, Ctx, Element};
use dt_kit::{Audit, BuildRecord, Export, Frame, Mask, Material, Printer, Scene, Seed, Stage};

pub use cache::Cache;
pub use diag::Diag;
pub use dt_kit::Progress;
pub use source::{Files, LIB, Library, Memory, Source};

use crate::doc::Tile;
use crate::plan::{Refusal, Step};

/// A project: the folder holding `dharmatiles.dtile` (or the start folder
/// when there is none) and where builds go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    /// The project root.
    pub root: PathBuf,
    /// Build output: `out/` beside the project file unless it says
    /// `out "…"`.
    pub out: PathBuf,
}

/// **Project discovery**: walk up from `start` to the first folder with a
/// `dharmatiles.dtile`, as Cargo finds `Cargo.toml`.
#[must_use]
pub fn discover(start: &Path) -> Project {
    let found = start
        .ancestors()
        .find(|d| d.join("dharmatiles.dtile").is_file());
    let root = found.unwrap_or(start).to_path_buf();
    let out = found
        .and_then(|d| std::fs::read_to_string(d.join("dharmatiles.dtile")).ok())
        .and_then(|t| kdl::KdlDocument::parse(&t).ok())
        .and_then(|k| {
            k.get("out")
                .and_then(|n| n.entries().first())
                .and_then(|e| e.value().as_string().map(PathBuf::from))
        })
        .unwrap_or_else(|| PathBuf::from("out"));
    Project {
        out: root.join(out),
        root,
    }
}

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

impl Default for Options {
    /// DungeonBlocks, the tile's seed, and the library's default printer
    /// (library/printers/default.dtile).
    ///
    /// # Panics
    /// If the bundled printer profile doesn't decode (a test guards this).
    fn default() -> Self {
        let printer = doc::printer(&Library, doc::PRINTER)
            .unwrap_or_else(|e| panic!("the bundled printer must decode: {e:?}"));
        Self {
            system: "db".to_owned(),
            seed: None,
            printer: Profile {
                name: "default".to_owned(),
                gate: printer,
            },
            engine: concat!("dt-eval-", env!("CARGO_PKG_VERSION")).to_owned(),
            draft: false,
        }
    }
}

/// A finished build.
pub struct Built {
    /// The mesh as written, with each triangle's input triangle.
    pub export: Export,
    /// Material of each written triangle.
    pub materials: Vec<Material>,
    /// The gate's findings.
    pub audit: Audit,
    /// Stage hashes, in build order.
    pub stages: Vec<Stage>,
    /// Where it came from.
    pub record: BuildRecord,
    /// Output path below the project's `out/`, such as
    /// `db/test/1x1-flat-db.stl`.
    pub out: PathBuf,
    /// Tile width and height in mm.
    pub footprint: [f64; 2],
    /// The edge level the ground meets every tile edge at, mm.
    pub edge_mm: Option<f64>,
    /// Numbers the elements noted (counts), in build order.
    pub notes: Vec<(String, f64)>,
    /// The ground surface as built, for painted previews.
    pub surface: Option<(dt_kit::Grid, dt_kit::Mask)>,
    /// The nodes a stamped solid (floor, wall, tree) covers, for painted
    /// previews: the earth's side under them is bedding.
    pub covered: Option<dt_kit::Mask>,
    /// How long each stage took, seconds, in build order (`3:grass`,
    /// `union`, `audit`): the weights the next build's progress bar uses.
    pub stage_secs: Vec<(String, f64)>,
    /// Steps a [`Cache`] gave, not built (the first ones, in plan order);
    /// their stages are missing from `stage_secs`.
    pub reused: usize,
    /// The masonry is brick (a `brick` or `regular-brick` wall): painted
    /// in reds, not stone greys.
    pub brick: bool,
    /// The stack entry each written triangle came from (an index into the
    /// tile's stack; the stack's length is the base, if the tile has one; [`NO_ENTRY`] none).
    /// The lifted ground goes to the surface entry that painted it.
    pub entries: Vec<u16>,
    /// Which of its entry's solids each written triangle came from (the
    /// third rock of a scatter is 2), in build order.
    pub pieces: Vec<u16>,
}

/// No stack entry made this triangle.
pub const NO_ENTRY: u16 = u16::MAX;

impl Built {
    /// Per-face paint for previews and PNG renders: the acrylic ramps of
    /// the default palette, overhangs red and the tolerance band orange.
    #[must_use]
    pub fn colours(&self) -> Vec<[f32; 3]> {
        self.colours_with(&dt_kit::Palette::default(), true)
    }

    /// [`Built::colours`] with the ramps of `palette`; without
    /// `overhang`, no red or orange (the viewer's overhang toggle).
    #[must_use]
    pub fn colours_with(&self, palette: &dt_kit::Palette, overhang: bool) -> Vec<[f32; 3]> {
        let surface = self.surface.as_ref().map(|(g, stone)| {
            let p = dt_kit::SurfacePaint::new(g, stone);
            match &self.covered {
                Some(c) => p.covered(c),
                None => p,
            }
        });
        let none = vec![false; self.materials.len()];
        let (over, band) = if overhang {
            (&self.audit.overhang, &self.audit.band)
        } else {
            (&none, &none)
        };
        dt_kit::paint_mesh(
            &self.export.mesh,
            &self.materials,
            over,
            band,
            surface.as_ref(),
            palette,
            self.brick,
        )
    }
}

/// The plan of a document, without geometry.
pub struct Checked {
    /// The file as dt-doc decoded it.
    pub file: std::sync::Arc<doc::File>,
    /// Its tile as read, if it has one (a parts, printer or levels file
    /// has none: it is only decoded).
    pub tile: Option<Tile>,
    /// The tile's build order.
    pub steps: Vec<Step>,
    elements: Vec<Box<dyn Element>>,
    /// What each element writes, from its plan.
    writes: Vec<dt_elements::Write>,
    /// Each step's area: `None` for the whole tile, else a region index.
    /// Masks are only made by `build` (one per distinct area), so `check`
    /// holds no grid however long the stack.
    areas: Vec<Option<usize>>,
    frame: Frame,
    seed: u64,
    /// The Solve phase, for a tile that uses the heights language: its
    /// bodies' bands, and the lints that came with them.
    pub solved: Option<solve::SolveReport>,
}

impl Checked {
    /// Lints and notes from the Solve phase: the tile checks, but look here.
    #[must_use]
    pub fn notes(&self) -> &[Diag] {
        self.solved.as_ref().map_or(&[], |s| s.notes.as_slice())
    }

    /// What `check --explain` says: the defaults applied, each pin's share of
    /// its place, and each override.
    #[must_use]
    pub fn explain(&self) -> &[String] {
        self.solved.as_ref().map_or(&[], |s| s.explain.as_slice())
    }

    /// What the tile asks for that is not built yet (check accepts it,
    /// build refuses it).
    #[must_use]
    pub fn pending(&self) -> &[Diag] {
        self.tile.as_ref().map_or(&[], |t| t.pending.as_slice())
    }

    /// Each of the tile's regions as the build would fill it, on a coarse
    /// lattice of `cells` per square (nodes from the south-west corner,
    /// row by row): for drawing a map of the tile.
    #[must_use]
    pub fn region_masks(&self, cells: u32) -> Vec<Mask> {
        let Some(tile) = &self.tile else {
            return Vec::new();
        };
        let frame = Frame {
            cells_per_square: cells.max(1),
            ..self.frame
        };
        tile.regions
            .iter()
            .map(|r| region_mask(tile, &frame, r))
            .collect()
    }
}

/// **check**: read `path` and plan it (no geometry).
///
/// # Errors
/// Every problem found, each at its text.
pub fn check(sources: &dyn Source, path: &str, opts: &Options) -> Result<Checked, Vec<Diag>> {
    let read = doc::read(sources, path)?;
    let Some(mut tile) = read.tile else {
        return Ok(Checked {
            file: read.file,
            tile: None,
            steps: Vec::new(),
            elements: Vec::new(),
            writes: Vec::new(),
            areas: Vec::new(),
            frame: Frame::dungeonblocks(1, 1),
            seed: 0,
            solved: None,
        });
    };
    if let Some(seed) = opts.seed {
        tile.reseed(seed);
    }
    let frame = Frame::for_system(&opts.system, tile.cols, tile.rows);
    let mut errors = Vec::new();
    let mut elements: Vec<Box<dyn Element>> = Vec::new();
    let mut areas = Vec::new();
    for e in &tile.stack {
        let as_controls = |settings: &[doc::Setting]| -> Vec<Control> {
            settings
                .iter()
                .map(|c| Control {
                    name: c.name.clone(),
                    value: c.value.clone(),
                })
                .collect()
        };
        let controls = as_controls(&e.controls);
        let children: Vec<dt_elements::Child> = e
            .children
            .iter()
            .map(|c| dt_elements::Child {
                name: c.what.value.clone(),
                controls: as_controls(&c.controls),
                weight: c.weight,
            })
            .collect();
        match dt_elements::element_with(&e.what.value, &controls, &children) {
            Ok(l) => elements.push(l),
            Err(le) => {
                let at = le
                    .control
                    .as_ref()
                    .and_then(|c| e.controls.iter().find(|s| &s.name == c));
                let d = match at {
                    Some(s) => Diag::at(&s.src, s.span, "dt::element", le.message, "here"),
                    None => Diag::at(&e.src, e.what.span, "dt::element", le.message, "here"),
                };
                errors.push(d.maybe_help(le.help));
                continue;
            }
        }
        areas.push(region_of(&tile, e, &mut errors));
    }
    if tile.stack.is_empty() && tile.pending.is_empty() {
        errors.push(
            Diag::at(
                &tile.src,
                tile.span,
                "dt::empty",
                "the tile has nothing in its stack",
                "nothing to build",
            )
            .help("add an element to its stack, such as `stack { ground }`"),
        );
    }
    // the tile is built at the finest cell any entry needs; a draft coarser
    let frame = if opts.draft {
        Frame {
            cells_per_square: DRAFT_CELLS,
            ..frame
        }
    } else {
        elements
            .iter()
            .filter_map(|l| l.plan().cell_mm)
            .min()
            .map_or(frame, |c| frame.with_cell(c.get()))
    };
    errors.extend(empty_regions(&tile, &frame, &areas));
    // the connector comes only from a `base` line; with none the tile has
    // no base, printed flat from zero up
    if tile.has_base {
        match dt_elements::connector(&opts.system) {
            Ok(c) => {
                elements.push(c);
                areas.push(None);
            }
            Err(le) => errors.push(Diag::plain("dt::system", le.message).maybe_help(le.help)),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let plans: Vec<_> = elements.iter().map(|l| l.plan()).collect();
    let steps = plan::schedule(&plans).map_err(|r| {
        let (msg, entry) = match &r {
            Refusal::NoWriter { entry, kind } => (
                format!("this reads every {kind}, and nothing in the tile makes one"),
                *entry,
            ),
            Refusal::SurfaceReadsSolid { entry } => {
                ("a surface cannot read solids yet".to_owned(), *entry)
            }
            Refusal::Cycle { entries } => (
                "these entries read each other's solids".to_owned(),
                entries[0],
            ),
        };
        let d = match tile.stack.get(entry) {
            Some(e) => Diag::at(&e.src, e.span, "dt::plan", msg, "cannot be satisfied"),
            None => Diag::plain("dt::plan", msg),
        };
        vec![d.help("remove the read, or add what it reads to the stack")]
    })?;
    let seed = opts.seed.unwrap_or(tile.seed);
    // the Solve phase: bands and pins, no meshes, on the lattice the build
    // makes its bodies on: `check` and `build` share this one solve
    let writes: Vec<dt_elements::Write> = plans.iter().map(|p| p.writes).collect();
    let solved = Some(solve::solve(&tile, &frame, opts, &writes)?);
    Ok(Checked {
        file: read.file,
        tile: Some(tile),
        steps,
        writes,
        elements,
        areas,
        frame,
        seed,
        solved,
    })
}

/// An entry's area: `None` for the whole tile, or its `in=` region.
fn region_of(tile: &Tile, e: &doc::Entry, errors: &mut Vec<Diag>) -> Option<usize> {
    let name = e.area.as_ref()?;
    let found = tile.regions.iter().position(|r| r.name == name.value);
    if found.is_none() {
        errors.push(
            Diag::at(
                &e.src,
                name.span,
                "dt::unknown-region",
                format!("no region named `{}`", name.value),
                "not a region",
            )
            .maybe_help(
                did_you_mean(&name.value, tile.regions.iter().map(|r| r.name.as_str()))
                    .map(|w| format!("did you mean `{w}`?"))
                    .or_else(|| Some("without in=, an element covers the whole tile".into())),
            ),
        );
    }
    found
}

/// A region's nodes, by paint bucket from its fill points.
fn region_mask(tile: &Tile, frame: &Frame, r: &doc::Region) -> Mask {
    // regions that name each other round a loop cover nothing (the solve
    // reports it)
    places::Places::new(tile, frame)
        .region(r)
        .unwrap_or_else(|_| {
            let (nx, ny) = frame.nodes();
            Mask::new(nx, ny, false)
        })
}

/// One error for each region an element acts in that covers no ground: its
/// fill points all lie on its lines.  One mask at a time, then dropped.
fn empty_regions(tile: &Tile, frame: &Frame, areas: &[Option<usize>]) -> Vec<Diag> {
    let used: std::collections::BTreeSet<usize> = areas.iter().flatten().copied().collect();
    used.into_iter()
        .filter_map(|i| {
            let r = &tile.regions[i];
            if r.fills.is_empty() || region_mask(tile, frame, r).bits.iter().any(|&b| b) {
                return None;
            }
            Some(
                Diag::at(
                    &tile.src,
                    r.span,
                    "dt::empty-region",
                    format!("the region `{}` covers nothing", r.name),
                    "empty",
                )
                .help("its fill points lie on its lines: move each one off the line, into the area it should fill"),
            )
        })
        .collect()
}

/// The masks a build needs, one per distinct area, and each step's index
/// into them.
fn masks(tile: &Tile, frame: &Frame, areas: &[Option<usize>]) -> (Vec<Mask>, Vec<usize>) {
    let (nx, ny) = frame.nodes();
    let mut keys: Vec<Option<usize>> = Vec::new();
    let mut out = Vec::new();
    let index = areas
        .iter()
        .map(|a| {
            keys.iter().position(|k| k == a).unwrap_or_else(|| {
                keys.push(*a);
                out.push(match a {
                    None => Mask::new(nx, ny, true),
                    Some(i) => region_mask(tile, frame, &tile.regions[*i]),
                });
                out.len() - 1
            })
        })
        .collect();
    (out, index)
}

/// **build**: `path` for `opts.system`, through the audit gate.
///
/// # Errors
/// Every problem found; a failed gate is not an error (see
/// [`Audit::failures`]).
pub fn build(sources: &dyn Source, path: &str, opts: &Options) -> Result<Built, Vec<Diag>> {
    build_with(sources, path, opts, &Progress::none())
}

/// [`build`], reporting each stage (and an element's sub-progress) to
/// `progress`.  The report never touches geometry or seeds: the result is
/// the same with any reporter, or none.
///
/// # Errors
/// As [`build`].
pub fn build_with(
    sources: &dyn Source,
    path: &str,
    opts: &Options,
    progress: &Progress,
) -> Result<Built, Vec<Diag>> {
    build_cached(sources, path, opts, progress, None)
}

/// [`build_with`], starting from the scene `cache` kept after the last
/// step this build shares with the build before it, and leaving its own
/// steps there for the next (see [`Cache`]).  The result is the same as
/// without the cache, hash for hash.
///
/// # Errors
/// As [`build`].
pub fn build_cached(
    sources: &dyn Source,
    path: &str,
    opts: &Options,
    progress: &Progress,
    cache: Option<&Cache>,
) -> Result<Built, Vec<Diag>> {
    let text = sources
        .read(path)
        .map_err(|e| vec![Diag::plain("dt::source", e)])?;
    let c = check(sources, path, opts)?;
    let Some(tile) = &c.tile else {
        return Err(vec![
            Diag::plain("dt::no-tile", format!("`{path}` has no tile to build"))
                .help("a tile document holds one `tile size=\"1x1\" { stack { … } }`; parts, printer and level files are only checked"),
        ]);
    };
    if !tile.pending.is_empty() {
        return Err(tile.pending.clone());
    }
    let plan = Plan::new(&c, tile, opts)?;
    let ran = plan.run(progress, cache)?;
    plan.finish(path, &text, ran, progress)
}

fn build_error(e: String) -> Vec<Diag> {
    vec![Diag::plain("dt::build", e)]
}

/// One stack entry (or the connector after the stack), as a build sees it.
pub(crate) struct Job {
    /// Its stack path (`tile/grass/0`), the owner of what it makes.
    pub path: String,
    /// Its seed: `seed=` when pinned, else keyed by its path.
    pub seed: Seed,
    /// Which of the build's masks is its area.
    pub mask: usize,
    /// That mask's content hash.
    pub area_hash: u64,
    /// The hash of the solved band it is built in (0 if it has none), so a
    /// cache never serves a body whose pins changed.
    pub band_hash: u64,
    /// The hash of what lies over it (the water laid after a dirt, see
    /// `Ctx::under`; 0 if nothing), so editing the water never serves a
    /// stale dirt.
    pub under_hash: u64,
}

/// What a build works out before any step runs.
struct Plan<'a> {
    c: &'a Checked,
    tile: &'a Tile,
    opts: &'a Options,
    /// One mask per distinct area.
    masks: Vec<Mask>,
    /// One per element, in stack order, the connector last.
    jobs: Vec<Job>,
    /// The planned feet of every solid that has one.
    foots: Vec<dt_elements::Foot>,
    /// The bodies, solved once on the build's lattice (`check`'s solve).
    solved: &'a solve::SolveReport,
    /// For each dirt: where the water laid after it certainly stands, and
    /// how low (see `Ctx::under`).
    under: Vec<Option<dt_kit::Field>>,
    fills: bool,
    /// Each step's progress key and label.
    stages: Vec<(String, String)>,
    /// The order and areas, hashed: it leads the stages, but is not the
    /// scene's (a scene a cache kept serves a build whose later steps
    /// differ).
    plan_stage: Stage,
}

/// The scene after the last step, and how it got there.
struct Ran {
    scene: Scene,
    stage_secs: Vec<(String, f64)>,
    /// Steps the cache gave.
    reused: usize,
}

impl<'a> Plan<'a> {
    fn new(c: &'a Checked, tile: &'a Tile, opts: &'a Options) -> Result<Self, Vec<Diag>> {
        let root = Seed::new(c.seed);
        let Some(solved) = c.solved.as_ref() else {
            return Err(build_error("the tile was not solved".to_owned()));
        };
        let (masks, mask_of) = masks(tile, &c.frame, &c.areas);
        let hashes: Vec<u64> = masks.iter().map(Mask::content_hash).collect();
        let order = c.steps.iter().fold(hash::OFFSET, |h, s| {
            hash::step(
                h,
                match s {
                    Step::Entry(i) => *i as u64,
                    Step::Lift => u64::MAX,
                },
            )
        });
        let plan_hash = mask_of.iter().fold(order, |h, &m| hash::step(h, hashes[m]));
        let jobs = entry_paths(tile)
            .into_iter()
            .zip(&mask_of)
            .enumerate()
            .map(|(i, (path, &mask))| Job {
                seed: tile
                    .stack
                    .get(i)
                    .and_then(|e| e.seed)
                    .map_or_else(|| root.path(&path), Seed::new),
                path,
                mask,
                area_hash: hashes[mask],
                band_hash: solved.by_entry[i].map_or(0, |b| {
                    let band = &solved.bodies[b].band;
                    band.lo()
                        .z
                        .iter()
                        .chain(&band.hi().z)
                        .fold(hash::OFFSET, |h, v| hash::step(h, v.to_bits()))
                }),
                under_hash: 0,
            })
            .collect();
        let mut plan = Self {
            c,
            tile,
            opts,
            masks,
            jobs,
            foots: Vec::new(),
            under: Vec::new(),
            solved,
            fills: c.writes.contains(&dt_elements::Write::Fill),
            stages: progress::stage_keys(tile, &c.steps),
            plan_stage: Stage {
                name: "plan".to_owned(),
                hash: plan_hash,
            },
        };
        // what stands on the ground says where its foot is before anything
        // is built, so a bank or scatter listed before it (or after) can
        // lean on it
        let mut foots = Vec::new();
        for (i, element) in c.elements.iter().enumerate() {
            let ctx = Ctx {
                against: None,
                ..plan.ctx(i)
            };
            match element.foot(&c.frame, &ctx) {
                Ok(Some(f)) => foots.push(f),
                Ok(None) => {}
                Err(e) => return Err(build_error(format!("{}: {}", plan.jobs[i].path, e.message))),
            }
        }
        plan.foots = foots;
        plan.under = (0..c.elements.len()).map(|i| plan.under_of(i)).collect();
        for (job, under) in plan.jobs.iter_mut().zip(&plan.under) {
            job.under_hash = under.as_ref().map_or(0, |u| {
                u.z.iter()
                    .fold(hash::OFFSET, |h, v| hash::step(h, v.to_bits()))
            });
        }
        Ok(plan)
    }

    /// What water laid after dirt `i` covers: about the height it stands at
    /// (the calm level held to its band), where its region puts it over the dirt.
    fn under_of(&self, i: usize) -> Option<dt_kit::Field> {
        let b = self.solved.by_entry[i]?;
        if self.solved.bodies[b].role != solve::Role::Land {
            return None;
        }
        let calm = self.solved.calm_mm?;
        let first = &self.solved.bodies[b].band;
        let mut out: Option<dt_kit::Field> = None;
        for (j, body) in self.solved.by_entry.iter().enumerate().skip(i + 1) {
            let Some(w) = *body else { continue };
            if self.solved.bodies[w].role != solve::Role::Fluid {
                continue;
            }
            let (lo, hi) = (
                self.solved.bodies[w].band.lo(),
                self.solved.bodies[w].band.hi(),
            );
            let region = &self.masks[self.jobs[j].mask];
            let f =
                out.get_or_insert_with(|| first.lo().with_z(vec![f64::NEG_INFINITY; lo.z.len()]));
            // the calm level held to the band: about where the water stands
            for k in 0..lo.z.len() {
                if region.bits[k] {
                    f.z[k] = f.z[k].max(calm.clamp(lo.z[k], hi.z[k]));
                }
            }
        }
        out
    }

    /// Element `i`'s view of the build.
    fn ctx(&self, i: usize) -> Ctx<'_> {
        let entry = self.tile.stack.get(i);
        let job = &self.jobs[i];
        Ctx {
            area: &self.masks[job.mask],
            path: &job.path,
            seed: job.seed,
            along: entry.and_then(|e| e.along.as_deref()),
            at: entry.and_then(|e| e.at),
            printer: *self.opts.printer,
            against: entry.and_then(|e| e.against.as_deref()),
            foots: &self.foots,
            band: self.solved.by_entry[i].map(|b| &self.solved.bodies[b].band),
            edge_mm: self.solved.edge_mm,
            calm_mm: self.solved.calm_mm,
            under: self.under.get(i).and_then(Option::as_ref),
            draft: self.opts.draft,
        }
    }

    /// Every step, from the last one `cache` kept.
    fn run(&self, progress: &Progress, cache: Option<&Cache>) -> Result<Ran, Vec<Diag>> {
        let (c, tile) = (self.c, self.tile);
        let mut scene = Scene::new(c.frame, Seed::new(c.seed));
        scene.set_progress(progress.clone());
        let mut stage_secs = Vec::new();
        let mut painted = false;
        // sketches start with the lifted ground, which carries what the
        // surfaces made before it: a lone pebble is no picture of a tile
        let mut lifted = !c.steps.contains(&Step::Lift);
        let mut sketched = 0;
        let base = cache::base_key(
            &c.frame,
            c.seed,
            self.opts,
            [self.solved.edge_mm, self.solved.calm_mm],
            self.fills,
            &self.foots,
        );
        let keys = cache::step_keys(base, tile, &c.steps, &self.jobs);
        let mut first = 0;
        if let Some((n, k)) = cache.and_then(|c| c.longest(&keys)) {
            scene = k.scene;
            scene.set_progress(progress.clone());
            painted = k.painted;
            first = n + 1;
            // what the cache gave: each stage done at once, and the scene so
            // far sketched in one piece
            for (key, label) in &self.stages[..first] {
                progress.begin(key, label);
                progress.end(key);
            }
            lifted |= c.steps[..first].contains(&Step::Lift);
            if lifted
                && progress.sketching()
                && !scene.parts().is_empty()
                && let Ok((mesh, mats)) = scene.soup_since(0)
            {
                progress.sketch(&mesh, &mats, false);
                sketched = scene.parts().len();
            }
        }
        for (n, step) in c.steps.iter().enumerate().skip(first) {
            let t_stage = std::time::Instant::now();
            let (key, label) = &self.stages[n];
            progress.begin(key, label);
            match *step {
                Step::Lift => scene
                    .lift("tile/terrain", &self.opts.printer)
                    .map_err(|e| build_error(e.to_string()))?,
                Step::Entry(i) => c.elements[i].build(&mut scene, &self.ctx(i)).map_err(|e| {
                    let msg = format!("{}: {}", self.jobs[i].path, e.message);
                    match tile.stack.get(i) {
                        Some(en) => vec![
                            Diag::at(&en.src, en.span, "dt::build", msg, "while building this")
                                .maybe_help(e.help),
                        ],
                        None => build_error(msg),
                    }
                })?,
            }
            // a front end showing the build as it goes gets what this stage made
            lifted |= *step == Step::Lift;
            if lifted
                && progress.sketching()
                && scene.parts().len() > sketched
                && let Ok((mesh, mats)) = scene.soup_since(sketched)
            {
                progress.sketch(&mesh, &mats, false);
                sketched = scene.parts().len();
            }
            progress.end(key);
            stage_secs.push((key.clone(), t_stage.elapsed().as_secs_f64()));
            if let Some(cache) = cache {
                let mut scene = scene.clone();
                scene.set_progress(Progress::none());
                cache.put(
                    n,
                    cache::Kept {
                        key: keys[n],
                        scene,
                        painted,
                    },
                );
            }
        }
        Ok(Ran {
            scene,
            stage_secs,
            reused: first,
        })
    }

    /// The union, the gate and the result.
    fn finish(
        &self,
        path: &str,
        text: &str,
        ran: Ran,
        progress: &Progress,
    ) -> Result<Built, Vec<Diag>> {
        let (c, tile, opts) = (self.c, self.tile, self.opts);
        let Ran {
            mut scene,
            mut stage_secs,
            reused,
        } = ran;
        let t_stage = std::time::Instant::now();
        progress.begin(progress::UNION, "joining the parts");
        // the masonry is brick when every wall and floor is, and there is some
        let masonry: Vec<bool> = tile
            .stack
            .iter()
            .filter(|e| matches!(e.what.value.as_str(), "wall" | "floor"))
            .map(|e| {
                e.controls.iter().any(|c| {
                    c.name == "style"
                        && matches!(&c.value, dt_elements::Value::Word(w) if w.contains("brick"))
                })
            })
            .collect();
        let brick = !masonry.is_empty() && masonry.iter().all(|&b| b);
        let (mesh, mats) = if opts.draft {
            scene.soup_since(0)
        } else {
            scene.union()
        }
        .map_err(|e| build_error(e.to_string()))?;
        let (entry_in, piece_in) = trace_entries(&scene, &mesh, self);
        progress.end(progress::UNION);
        stage_secs.push((progress::UNION.to_owned(), t_stage.elapsed().as_secs_f64()));
        let t_stage = std::time::Instant::now();
        progress.begin(progress::AUDIT, "checking printability");
        let footprint = c.frame.size_mm();
        // the band every element gets: the printer's overhang to its steepest
        let band = dt_kit::HangBand {
            max_deg: opts.printer.steepest_deg,
            ..dt_kit::HangBand::DEFAULT
        };
        let (export, audit) = if opts.draft {
            // the base is the same in every draft: its sockets are not flagged
            let base = u16::try_from(tile.stack.len()).unwrap_or(NO_ENTRY);
            let skip: Vec<bool> = entry_in.iter().map(|&e| e == base).collect();
            dt_kit::draft(mesh, &opts.printer, &skip)
        } else {
            dt_kit::audit_rounded(
                &mesh,
                footprint,
                c.frame.corner_round_mm(),
                &opts.printer,
                &mats,
                Some(&band),
            )
        };
        progress.end(progress::AUDIT);
        stage_secs.push((progress::AUDIT.to_owned(), t_stage.elapsed().as_secs_f64()));
        let materials = export.source.iter().map(|&t| mats[t as usize]).collect();
        let entries = export
            .source
            .iter()
            .map(|&t| entry_in.get(t as usize).copied().unwrap_or(NO_ENTRY))
            .collect();
        let pieces = export
            .source
            .iter()
            .map(|&t| piece_in.get(t as usize).copied().unwrap_or(0))
            .collect();
        scene.record("export", export.mesh.content_hash());
        let record = BuildRecord {
            engine: opts.engine.clone(),
            document: path.to_owned(),
            document_hash: doc::text_hash(text),
            seed: c.seed,
            system: opts.system.clone(),
            printer: opts.printer.name.clone(),
        };
        Ok(Built {
            export,
            materials,
            audit,
            stages: std::iter::once(self.plan_stage.clone())
                .chain(scene.stages().iter().cloned())
                .collect(),
            record,
            out: out_path(path, &opts.target()),
            footprint,
            edge_mm: self.solved.edge_mm,
            notes: scene.notes().to_vec(),
            surface: scene.surface(),
            covered: scene.support().map(|g| dt_kit::Mask {
                nx: g.nx,
                ny: g.ny,
                bits: g.z.iter().map(|z| z.is_finite()).collect(),
            }),
            stage_secs,
            reused,
            brick,
            entries,
            pieces,
        })
    }
}

fn trace_entries(scene: &Scene, mesh: &dt_kit::TriMesh, plan: &Plan<'_>) -> (Vec<u16>, Vec<u16>) {
    let (tile, c, masks) = (plan.tile, plan.c, &plan.masks);
    let writes = &c.writes;
    let to16 = |i: usize| u16::try_from(i).unwrap_or(NO_ENTRY);
    let mut seen: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let parts: Vec<(Option<u16>, u16)> = scene
        .parts()
        .iter()
        .map(|p| {
            let k = seen.entry(p.tag.owner.as_str()).or_default();
            *k += 1;
            (
                plan.jobs
                    .iter()
                    .position(|j| j.path == p.tag.owner)
                    .map(to16),
                to16(*k - 1),
            )
        })
        .collect();
    // the surfaces, in the order they painted
    let surfaces: Vec<usize> = c
        .steps
        .iter()
        .filter_map(|s| match *s {
            Step::Entry(i)
                if i < tile.stack.len() && writes.get(i) == Some(&dt_elements::Write::Surface) =>
            {
                Some(i)
            }
            _ => None,
        })
        .collect();
    // the last entry that filled the surface with water owns the wet nodes
    let filler = c.steps.iter().rev().find_map(|s| match *s {
        Step::Entry(i)
            if i < tile.stack.len() && writes.get(i) == Some(&dt_elements::Write::Fill) =>
        {
            Some(i)
        }
        _ => None,
    });
    let wet = scene.wet();
    let step = c.frame.step_mm();
    let ground = |t: usize| -> u16 {
        let [a, b, d] = mesh.triangles[t].map(|i| mesh.positions[i as usize]);
        let (x, y) = ((a[0] + b[0] + d[0]) / 3.0, (a[1] + b[1] + d[1]) / 3.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (
            (x / step).round().max(0.0) as usize,
            (y / step).round().max(0.0) as usize,
        );
        if let Some(f) = filler
            && wet.nx > 0
            && wet.bits[j.min(wet.ny - 1) * wet.nx + i.min(wet.nx - 1)]
        {
            return to16(f);
        }
        surfaces
            .iter()
            .rev()
            .find(|&&e| {
                let m = &masks[plan.jobs[e].mask];
                m.bits[j.min(m.ny - 1) * m.nx + i.min(m.nx - 1)]
            })
            .or(surfaces.first())
            .map_or(NO_ENTRY, |&e| to16(e))
    };
    let part_of = scene.trace_parts(mesh);
    part_of
        .iter()
        .enumerate()
        .map(|(t, &p)| match parts.get(p as usize) {
            Some((Some(e), k)) => (*e, *k),
            Some((None, _)) => (ground(t), 0),
            None => (NO_ENTRY, 0),
        })
        .unzip()
}

/// Seeds by path: `tile/<element>/<k>`, `k` counting same-named entries,
/// so adding an entry of another element changes no other seed.  The
/// tile's file name is not in it: renaming a tile never reshuffles it.
fn entry_paths(tile: &Tile) -> Vec<String> {
    let mut seen: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut out: Vec<String> = tile
        .stack
        .iter()
        .map(|e| {
            let k = seen.entry(e.what.value.as_str()).or_default();
            *k += 1;
            match &e.id {
                Some(id) => format!("tile/{}/{id}", e.what.value),
                None => format!("tile/{}/{}", e.what.value, *k - 1),
            }
        })
        .collect();
    if tile.has_base {
        out.push("tile/connector".to_owned());
    }
    out
}

/// `lib:tiles/test/1x1-flat.dtile` → `db/test/1x1-flat-db.stl`; a relative
/// disk path keeps its folders below `tiles/` (all of them if it has
/// none), resolved so the result always stays inside `<system>/`: a `..`
/// that would leave it is dropped, and an absolute path keeps none.
fn out_path(path: &str, system: &str) -> PathBuf {
    let p = path.strip_prefix(LIB).unwrap_or(path).replace('\\', "/");
    let rel = p.rsplit_once("tiles/").map_or(p.as_str(), |(_, r)| r);
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    let mut folders: Vec<&str> = Vec::new();
    if !dir.starts_with('/') && !dir.contains(':') {
        for f in dir.split('/') {
            match f {
                "" | "." => {}
                ".." => {
                    folders.pop();
                }
                f => folders.push(f),
            }
        }
    }
    let stem = Path::new(file)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty() && s != "." && s != "..")
        .unwrap_or_else(|| "tile".to_owned());
    let mut out = PathBuf::from(system);
    out.extend(folders);
    out.push(format!("{stem}-{system}.stl"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_the_system_and_a_printer_that_is_not_the_default() {
        let mut o = Options::default();
        assert_eq!(o.target(), "db");
        assert_eq!(dt_kit::target_name("ol", "default"), "ol");
        o.use_printer("fine", None).unwrap();
        assert_eq!(o.target(), "db-fine");
        assert_eq!(o.printer.layer_mm, 0.12);
        assert_eq!(
            out_path("lib:tiles/test/1x1-flat.dtile", &o.target()),
            PathBuf::from("db-fine/test/1x1-flat-db-fine.stl")
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
        let mine =
            "printer fine {\n line 0.6\n layer 0.3\n overhang 50\n steepest 60\n gap 0.3\n}\n";
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
            out_path("lib:tiles/test/1x1-flat.dtile", "db"),
            PathBuf::from("db/test/1x1-flat-db.stl")
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
