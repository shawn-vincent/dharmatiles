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
//! needs a look not built yet set aside (see its module docs).

pub mod diag;
pub mod doc;
pub mod plan;
pub mod progress;
pub mod source;

use std::path::{Path, PathBuf};

use dt_doc::schema::did_you_mean;
use dt_kit::{Audit, BuildRecord, Export, Frame, Mask, Material, Printer, Scene, Seed, Stage};
use dt_looks::{Control, Ctx, Look};

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
    /// The printer the audit checks against.
    pub printer: Printer,
    /// Engine version for the build record.
    pub engine: String,
}

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
            printer,
            engine: concat!("dt-eval-", env!("CARGO_PKG_VERSION")).to_owned(),
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
    /// Numbers the looks noted (counts), in build order.
    pub notes: Vec<(String, f64)>,
    /// The ground surface as built, for painted previews.
    pub surface: Option<(dt_kit::Grid, dt_kit::Mask)>,
    /// The nodes a stamped solid (floor, wall, tree) covers, for painted
    /// previews: the earth's side under them is bedding.
    pub covered: Option<dt_kit::Mask>,
    /// How long each stage took, seconds, in build order (`3:grass`,
    /// `union`, `audit`): the weights the next build's progress bar uses.
    pub stage_secs: Vec<(String, f64)>,
    /// The masonry is brick (a `brick` or `regular-brick` wall): painted
    /// in reds, not stone greys.
    pub brick: bool,
}

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
    looks: Vec<Box<dyn Look>>,
    /// Each step's area: `None` for the whole tile, else a region index.
    /// Masks are only made by `build` (one per distinct area), so `check`
    /// holds no grid however long the stack.
    areas: Vec<Option<usize>>,
    frame: Frame,
    seed: u64,
}

impl Checked {
    /// What the tile asks for that is not built yet (check accepts it,
    /// build refuses it).
    #[must_use]
    pub fn pending(&self) -> &[Diag] {
        self.tile.as_ref().map_or(&[], |t| t.pending.as_slice())
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
            looks: Vec::new(),
            areas: Vec::new(),
            frame: Frame::dungeonblocks(1, 1),
            seed: 0,
        });
    };
    if let Some(seed) = opts.seed {
        tile.reseed(seed);
    }
    let frame = Frame::for_system(&opts.system, tile.cols, tile.rows);
    let mut errors = Vec::new();
    let mut looks: Vec<Box<dyn Look>> = Vec::new();
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
        let children: Vec<dt_looks::Child> = e
            .children
            .iter()
            .map(|c| dt_looks::Child {
                name: c.what.value.clone(),
                controls: as_controls(&c.controls),
                weight: c.weight,
            })
            .collect();
        match dt_looks::look_with(&e.what.value, &controls, &children) {
            Ok(l) => looks.push(l),
            Err(le) => {
                let at = le
                    .control
                    .as_ref()
                    .and_then(|c| e.controls.iter().find(|s| &s.name == c));
                let d = match at {
                    Some(s) => Diag::at(&s.src, s.span, "dt::look", le.message, "here"),
                    None => Diag::at(&e.src, e.what.span, "dt::look", le.message, "here"),
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
                format!("the tile `{}` has nothing in its stack", tile.name),
                "nothing to build",
            )
            .help("add a look to its stack, such as `stack { ground }`"),
        );
    }
    // the tile is built at the finest cell any entry needs
    let frame = looks
        .iter()
        .filter_map(|l| l.plan().cell_mm)
        .min()
        .map_or(frame, |c| frame.with_cell(c.get()));
    errors.extend(empty_regions(&tile, &frame, &areas));
    match dt_looks::connector(&opts.system) {
        Ok(c) => {
            looks.push(c);
            areas.push(None);
        }
        Err(le) => errors.push(Diag::plain("dt::system", le.message).maybe_help(le.help)),
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let plans: Vec<_> = looks.iter().map(|l| l.plan()).collect();
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
    Ok(Checked {
        file: read.file,
        tile: Some(tile),
        steps,
        looks,
        areas,
        frame,
        seed,
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
                    .or_else(|| Some("without in=, a look covers the whole tile".into())),
            ),
        );
    }
    found
}

/// A region's nodes, by paint bucket from its fill points.
fn region_mask(tile: &Tile, frame: &Frame, r: &doc::Region) -> Mask {
    let lines: Vec<Vec<[f64; 2]>> = r
        .bounded_by
        .iter()
        .filter_map(|l| tile.lines.get(&l.value).cloned())
        .collect();
    dt_kit::area(frame, &lines, &r.fills)
}

/// One error for each region a look acts in that covers no ground: its
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

fn fnv(h: u64, v: u64) -> u64 {
    (h ^ v).wrapping_mul(0x0000_0100_0000_01b3)
}

/// **build**: `path` for `opts.system`, through the audit gate.
///
/// # Errors
/// Every problem found; a failed gate is not an error (see
/// [`Audit::failures`]).
pub fn build(sources: &dyn Source, path: &str, opts: &Options) -> Result<Built, Vec<Diag>> {
    build_with(sources, path, opts, &Progress::none())
}

/// [`build`], reporting each stage (and a look's sub-progress) to
/// `progress`.  The report never touches geometry or seeds: the result is
/// the same with any reporter, or none.
///
/// # Errors
/// As [`build`].
#[allow(clippy::too_many_lines)]
pub fn build_with(
    sources: &dyn Source,
    path: &str,
    opts: &Options,
    progress: &Progress,
) -> Result<Built, Vec<Diag>> {
    let text = sources
        .read(path)
        .map_err(|e| vec![Diag::plain("dt::source", e)])?;
    let c = check(sources, path, opts)?;
    let Some(tile) = &c.tile else {
        return Err(vec![
            Diag::plain("dt::no-tile", format!("`{path}` has no tile to build"))
                .help("a tile document holds one `tile name size=\"1x1\" { stack { … } }`; parts, printer and level files are only checked"),
        ]);
    };
    if !tile.pending.is_empty() {
        return Err(tile.pending.clone());
    }
    let fail = |e: String| vec![Diag::plain("dt::build", e)];
    let root = Seed::new(c.seed);
    let mut scene = Scene::new(c.frame, root);
    scene.set_progress(progress.clone());
    let mut stage_secs: Vec<(String, f64)> = Vec::new();
    let keys = progress::stage_keys(tile, &c.steps);
    let edge_mm = tile
        .edges
        .iter()
        .find(|(surface, _)| surface == "ground")
        .map(|(_, mm)| mm * c.frame.scale());
    scene.set_edge(edge_mm);
    scene.set_water_edge(
        tile.edges
            .iter()
            .find(|(surface, _)| surface == "water")
            .map(|(_, mm)| mm * c.frame.scale()),
    );
    let mut h = c.steps.iter().fold(0xcbf2_9ce4_8422_2325, |h, s| {
        fnv(
            h,
            match s {
                Step::Entry(i) => *i as u64,
                Step::Lift => u64::MAX,
            },
        )
    });
    let (masks, mask_of) = masks(tile, &c.frame, &c.areas);
    let hashes: Vec<u64> = masks.iter().map(Mask::content_hash).collect();
    h = mask_of.iter().fold(h, |h, &m| fnv(h, hashes[m]));
    scene.record("plan", h);
    let paths = entry_paths(tile);
    // with water, the ground's areas are banked down to each other once the
    // surfaces are painted, before any solid reads the ground
    let writes: Vec<dt_looks::Write> = c.looks.iter().map(|l| l.plan().writes).collect();
    let fills = writes.contains(&dt_looks::Write::Fill);
    // plan, then build: what stands on the ground says where its foot is
    // before anything is built, so a bank or scatter listed before it
    // (or after) can lean on it
    let mut foots = Vec::new();
    for (i, look) in c.looks.iter().enumerate() {
        let ctx = Ctx {
            area: &masks[mask_of[i]],
            path: &paths[i],
            seed: tile
                .stack
                .get(i)
                .and_then(|e| e.seed)
                .map_or_else(|| root.path(&paths[i]), Seed::new),
            along: tile.stack.get(i).and_then(|e| e.along.as_deref()),
            at: tile.stack.get(i).and_then(|e| e.at),
            printer: opts.printer,
            against: None,
            foots: &[],
        };
        match look.foot(&c.frame, &ctx) {
            Ok(Some(f)) => foots.push(f),
            Ok(None) => {}
            Err(e) => return Err(fail(format!("{}: {}", paths[i], e.message))),
        }
    }
    let mut painted = false;
    for (n, step) in c.steps.iter().enumerate() {
        let t_stage = std::time::Instant::now();
        progress.begin(&keys[n].0, &keys[n].1);
        let surface = matches!(*step, Step::Entry(i) if writes[i] == dt_looks::Write::Surface);
        if fills && !painted && !surface {
            painted = true;
            scene.bank();
        }
        match *step {
            Step::Lift => scene
                .lift(&format!("tile/{}/ground", tile.name))
                .map_err(|e| fail(e.to_string()))?,
            Step::Entry(i) => {
                let pinned = tile.stack.get(i).and_then(|e| e.seed);
                let ctx = Ctx {
                    area: &masks[mask_of[i]],
                    path: &paths[i],
                    seed: pinned.map_or_else(|| root.path(&paths[i]), Seed::new),
                    along: tile.stack.get(i).and_then(|e| e.along.as_deref()),
                    at: tile.stack.get(i).and_then(|e| e.at),
                    printer: opts.printer,
                    against: tile.stack.get(i).and_then(|e| e.against.as_deref()),
                    foots: &foots,
                };
                c.looks[i].build(&mut scene, &ctx).map_err(|e| {
                    let msg = format!("{}: {}", paths[i], e.message);
                    match tile.stack.get(i) {
                        Some(en) => vec![
                            Diag::at(&en.src, en.span, "dt::build", msg, "while building this")
                                .maybe_help(e.help),
                        ],
                        None => fail(msg),
                    }
                })?;
            }
        }
        progress.end(&keys[n].0);
        stage_secs.push((keys[n].0.clone(), t_stage.elapsed().as_secs_f64()));
    }
    let t_stage = std::time::Instant::now();
    progress.begin(progress::UNION, "joining the parts");
    let styles = |is_brick: bool| {
        tile.stack
            .iter()
            .filter(|e| matches!(e.what.value.as_str(), "wall" | "floor"))
            .filter(|e| {
                let brick = e.controls.iter().any(|c| {
                    c.name == "style"
                        && matches!(&c.value, dt_looks::Value::Word(w) if w.contains("brick"))
                });
                brick == is_brick
            })
            .count()
    };
    let brick = styles(true) > 0 && styles(false) == 0;
    let (mesh, mats) = scene.union().map_err(|e| fail(e.to_string()))?;
    progress.end(progress::UNION);
    stage_secs.push((progress::UNION.to_owned(), t_stage.elapsed().as_secs_f64()));
    let t_stage = std::time::Instant::now();
    progress.begin(progress::AUDIT, "checking printability");
    let footprint = c.frame.size_mm();
    // the band every element gets: the printer's overhang to its steepest
    let band = dt_kit::Band {
        max_deg: opts.printer.steepest_deg,
        ..dt_kit::Band::DEFAULT
    };
    let (export, audit) = dt_kit::audit_banded(&mesh, footprint, &opts.printer, &mats, Some(&band));
    progress.end(progress::AUDIT);
    stage_secs.push((progress::AUDIT.to_owned(), t_stage.elapsed().as_secs_f64()));
    let materials = export.source.iter().map(|&t| mats[t as usize]).collect();
    scene.record("export", export.mesh.content_hash());
    let record = BuildRecord {
        engine: opts.engine.clone(),
        document: path.to_owned(),
        document_hash: doc::text_hash(&text),
        seed: c.seed,
        system: opts.system.clone(),
    };
    Ok(Built {
        export,
        materials,
        audit,
        stages: scene.stages().to_vec(),
        record,
        out: out_path(path, &tile.name, &opts.system),
        footprint,
        edge_mm,
        notes: scene.notes().to_vec(),
        surface: scene.surface(),
        covered: scene.support().map(|g| dt_kit::Mask {
            nx: g.nx,
            ny: g.ny,
            bits: g.z.iter().map(|z| z.is_finite()).collect(),
        }),
        stage_secs,
        brick,
    })
}

/// Seeds by path: `tile/<name>/<look>/<k>`, `k` counting same-named
/// entries, so adding an entry of another look changes no other seed.
fn entry_paths(tile: &Tile) -> Vec<String> {
    let mut seen: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut out: Vec<String> = tile
        .stack
        .iter()
        .map(|e| {
            let k = seen.entry(e.what.value.as_str()).or_default();
            *k += 1;
            match &e.id {
                Some(id) => format!("tile/{}/{}/{id}", tile.name, e.what.value),
                None => format!("tile/{}/{}/{}", tile.name, e.what.value, *k - 1),
            }
        })
        .collect();
    out.push(format!("tile/{}/connector", tile.name));
    out
}

/// `lib:tiles/test/1x1-flat.dtile` → `db/test/1x1-flat-db.stl`; a relative
/// disk path keeps its folders below `tiles/` (all of them if it has
/// none), resolved so the result always stays inside `<system>/`: a `..`
/// that would leave it is dropped, and an absolute path keeps none.
fn out_path(path: &str, name: &str, system: &str) -> PathBuf {
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
        .unwrap_or_else(|| name.replace(['/', '\\', ':'], "-"));
    let mut out = PathBuf::from(system);
    out.extend(folders);
    out.push(format!("{stem}-{system}.stl"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_paths_mirror_the_library() {
        assert_eq!(
            out_path("lib:tiles/test/1x1-flat.dtile", "flat", "db"),
            PathBuf::from("db/test/1x1-flat-db.stl")
        );
        assert_eq!(
            out_path("my.dtile", "x", "db"),
            PathBuf::from("db/my-db.stl")
        );
    }

    #[test]
    fn out_paths_stay_inside_the_system_folder() {
        // was: db/../../t/evil/f-db.stl, outside out/
        assert_eq!(
            out_path("tiles/../../t/evil/f.dtile", "a", "db"),
            PathBuf::from("db/t/evil/f-db.stl")
        );
        assert_eq!(
            out_path("../t/evil/f.dtile", "a", "db"),
            PathBuf::from("db/t/evil/f-db.stl")
        );
        assert_eq!(
            out_path("a/../../../f.dtile", "a", "db"),
            PathBuf::from("db/f-db.stl")
        );
        assert_eq!(
            out_path("/abs/proj/f.dtile", "a", "db"),
            PathBuf::from("db/f-db.stl")
        );
        assert_eq!(
            out_path("C:\\proj\\f.dtile", "a", "db"),
            PathBuf::from("db/f-db.stl")
        );
        assert_eq!(
            out_path("dir/", "ü/../../x", "db"),
            PathBuf::from("db/dir/ü-..-..-x-db.stl")
        );
        for p in [
            "tiles/../../t/evil/f.dtile",
            "../../x.dtile",
            "/x/../../y.dtile",
        ] {
            let o = out_path(p, "a", "db");
            assert!(
                o.components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))),
                "{p} → {o:?}"
            );
        }
    }
}
