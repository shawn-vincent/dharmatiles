//! **build**: a checked document through the plan, the cache, the union and
//! the audit gate to an STL.

use std::path::{Path, PathBuf};

use dt_doc::place::Side;
use dt_elements::Ctx;
use dt_kit::{Audit, BuildRecord, EntryId, Export, Mask, Material, Scene, Seed, Stage};

use crate::cache::Cache;
use crate::check::{Checked, check, masks};
use crate::diag::Diag;
use crate::doc::{self, Tile};
use crate::options::Options;
use crate::plan::Step;
use crate::source::{LIB, Source};
use crate::{cache, hash, regions, solve};
use crate::{coords, crowns, progress};
use dt_kit::Progress;

/// Whether a build used the disk cache, and how.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CacheState {
    /// Not asked (no store).
    #[default]
    Off,
    /// Asked, nothing usable was kept: it was built (and kept).
    Miss,
    /// Read from the cache: nothing was built.
    Hit,
}

/// What the disk cache did for one build ([`Built::cache`]); the cache itself
/// is the `dt-store` crate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CacheUse {
    /// Whether it was used.
    pub state: CacheState,
    /// On a hit, the engine that made the entry.
    pub made_by: Option<String>,
    /// What went wrong, in words, for the front end to show: a damaged entry
    /// that was removed and built again, an entry that could not be written.
    /// The build is real either way.
    pub problems: Vec<String>,
}

/// What prints: the mesh, what each face is made of, the gate's verdict and
/// where the build came from.  Everything the program writes to a printer's
/// file is here, and nothing else is needed to write it.
pub struct Product {
    /// The mesh as written, with each triangle's input triangle.
    pub export: Export,
    /// Material of each written triangle.
    pub materials: Vec<Material>,
    /// The gate's findings.
    pub audit: Audit,
    /// Where it came from.
    pub record: BuildRecord,
    /// Output path below the project's `out/`, such as
    /// `db/tests/ground/1x1-flat-db.stl`.
    pub out: PathBuf,
    /// Tile width and height in mm.
    pub footprint: [f64; 2],
    /// Whether the gate checked this build.  Only a full build is: a low
    /// ([`Options::low`]) or draft build has each face flagged by its angle
    /// alone and its verdict is **not yet checked**.
    pub checked: bool,
}

/// What a build says about itself beside what it makes: the editor's data
/// (the surface it is painted from, the clouds and marks, which entry made
/// which triangle), the determinism record (stage hashes), the timings and
/// what the caches did.  None of it is written to a printer's file.
pub struct Report {
    /// Stage hashes, in build order.
    pub stages: Vec<Stage>,
    /// The edge level the ground meets every tile edge at, mm.
    pub edge_mm: Option<f64>,
    /// Numbers the elements noted (counts), in build order.
    pub notes: Vec<(String, f64)>,
    /// Pieces elements left out because they could not make them (a degenerate
    /// hull or band), by element; empty when nothing was.
    pub left_out: Vec<(String, usize)>,
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
    /// How the build is to be painted beyond its materials.  The document's
    /// words behind it are read by `dt_doc::schema`, not here.
    pub look: dt_kit::Look,
    /// The stack entry each written triangle came from (an index into
    /// [`Report::stack`], which lists the connector last; [`NO_ENTRY`] none).
    /// The lifted ground goes to the surface entry that painted it.
    pub entries: Vec<u16>,
    /// Which of its entry's solids each written triangle came from (the
    /// third rock of a scatter is 2), in build order.
    pub pieces: Vec<u16>,
    /// The stack this mesh was built from: each entry by who it is, the
    /// connector last when the tile has a base.  An entry number in
    /// [`Report::entries`] is a place in this list.
    pub stack: Vec<doc::Named>,
    /// The clouds ([`Options::clouds`]): sheets and columns only.
    pub clouds: Option<dt_kit::debug::Debug>,
    /// The debug layer's marks ([`Options::debug`]): dots and trails only.
    pub debug: Option<dt_kit::debug::Debug>,
    /// Where everything was placed (rocks, trees, flowers), before any
    /// detail was drawn: the same in every tier of a build.
    pub placed: Vec<dt_kit::Placement>,
    /// Whether the disk cache was used for this build, and what
    /// went wrong with it, if anything.
    pub cache: CacheUse,
}

/// A finished build: the [`Product`] and the [`Report`] beside it.  A build
/// reads as its product (`built.export`, `built.audit`: the product's fields
/// are reached through it); the report is `built.report`.
pub struct Built {
    /// What prints.
    pub product: Product,
    /// What the build says about itself.
    pub report: Report,
}

impl std::ops::Deref for Built {
    type Target = Product;
    fn deref(&self) -> &Product {
        &self.product
    }
}

impl std::ops::DerefMut for Built {
    fn deref_mut(&mut self) -> &mut Product {
        &mut self.product
    }
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

    /// The painted colours (overhang red and orange) and the clean ones
    /// (none), as [`Built::colours_with`] gives them, from one painting:
    /// the overhang paint lies over the clean colour, so the second set
    /// is the first with those faces replaced and costs no second pass
    /// through the painter (and its ground surface).
    #[must_use]
    pub fn colours_pair(&self, palette: &dt_kit::Palette) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
        let clean = self.colours_with(palette, false);
        let mut painted = clean.clone();
        let (over, band) = (&self.audit.overhang, &self.audit.band);
        for (k, c) in painted.iter_mut().enumerate() {
            // as the painter lays it: orange over red over the paint
            if band.get(k).copied().unwrap_or(false) {
                *c = dt_kit::ORANGE;
            } else if over.get(k).copied().unwrap_or(false) {
                *c = dt_kit::RED;
            }
        }
        (painted, clean)
    }

    /// [`Built::colours`] with the ramps of `palette`; without
    /// `overhang`, no red or orange (the viewer's overhang toggle).
    #[must_use]
    pub fn colours_with(&self, palette: &dt_kit::Palette, overhang: bool) -> Vec<[f32; 3]> {
        let surface = self.report.surface.as_ref().map(|(g, stone)| {
            let p = dt_kit::SurfacePaint::new(g, stone);
            match &self.report.covered {
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
            self.report.look.brick,
        )
    }
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
    // the file as it is: its hash is the document's, whatever it is read as
    let text = sources
        .read(path)
        .map_err(|e| vec![Diag::plain(dt_lang::codes::SOURCE, e)])?;
    let c = check(sources, path, opts)?;
    build_checked(&c, path, &text, opts, progress, cache)
}

/// A checked document through the plan, the union and the gate: `text` is the
/// file as read (its hash is the document's).  [`build_cached`] is this after
/// reading and checking; a front end with its own reads (the disk cache) calls
/// this one.
///
/// # Errors
/// As [`build`].
pub fn build_checked(
    c: &Checked,
    path: &str,
    text: &str,
    opts: &Options,
    progress: &Progress,
    cache: Option<&Cache>,
) -> Result<Built, Vec<Diag>> {
    let Some(tile) = &c.tile else {
        return Err(vec![
            Diag::plain(dt_lang::codes::NO_TILE, format!("`{path}` has no tile to build"))
                .help("a tile document holds one `tile size=\"1x1\" { stack { … } }`; parts, printer and level files are only checked"),
        ]);
    };
    if !tile.pending.is_empty() {
        return Err(tile.pending.clone());
    }
    let plan = Plan::new(c, tile, opts)?;
    let ran = plan.run(progress, cache)?;
    let built = plan.finish(path, text, ran, progress)?;
    Ok(built)
}

fn build_error(e: String) -> Vec<Diag> {
    vec![Diag::plain(dt_lang::codes::BUILD, e)]
}

/// One stack entry (or the connector after the stack), as a build sees it.
pub(crate) struct Job {
    /// Who it is (`tile/grass/0`), the owner of what it makes.
    pub id: EntryId,
    /// Its seed: `seed=` when pinned, else keyed by its id.
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
    /// The regions' hash when its controls hold a formula that reads them
    /// (a tree's `top=`), else 0, so editing a region it reads never
    /// serves a stale tree.
    pub regions_hash: u64,
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
    /// Works trees' `top=` formulas out at their branch tips.
    crowns: crowns::Crowns<'a>,
}

/// The scene after the last step, and how it got there.
struct Ran {
    scene: Scene,
    stage_secs: Vec<(String, f64)>,
    /// Steps the cache gave.
    reused: usize,
}

impl<'a> Plan<'a> {
    /// Where the entry `id` stands among the jobs: its place in the stack,
    /// or the connector's, after it.  `None` for the terrain, which no
    /// entry owns.
    fn job_of(&self, id: &EntryId) -> Option<usize> {
        self.tile.index_of(id).or_else(|| {
            (self.tile.has_base && *id == connector_id()).then_some(self.tile.stack.len())
        })
    }

    fn new(c: &'a Checked, tile: &'a Tile, opts: &'a Options) -> Result<Self, Vec<Diag>> {
        let root = Seed::new(c.seed);
        let Some(solved) = c.solved.as_ref() else {
            return Err(build_error("the tile was not solved".to_owned()));
        };
        let (masks, mask_of) = masks(&c.masks, &c.frame, &c.areas);
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
        // each entry's id, then the connector's
        let ids = tile
            .stack
            .iter()
            .map(|e| e.id.clone())
            .chain(tile.has_base.then(connector_id));
        let jobs = ids
            .zip(&mask_of)
            .enumerate()
            .map(|(i, (id, &mask))| Job {
                seed: tile
                    .stack
                    .get(i)
                    .and_then(|e| e.seed)
                    .map_or_else(|| root.path(id.as_str()), Seed::new),
                id,
                mask,
                area_hash: hashes[mask],
                band_hash: solved.by_entry[i].map_or(0, |b| {
                    let band = &solved.bodies[b].band;
                    let plain = band
                        .lo()
                        .z
                        .iter()
                        .chain(&band.hi().z)
                        .fold(hash::OFFSET, |h, v| hash::step(h, v.to_bits()));
                    // a cloud's odds are part of what its body is built on
                    band.odds().map_or(plain, |o| hash::step(plain, o.key()))
                }),
                under_hash: 0,
                regions_hash: tile
                    .stack
                    .get(i)
                    .filter(|e| {
                        // a tree's or rock's `top=`, its own or a scatter child's
                        e.controls
                            .iter()
                            .chain(e.children.iter().flat_map(|c| c.controls.iter()))
                            .any(|c| matches!(c.value, dt_elements::Value::Formula(_)))
                    })
                    .and(c.regions.as_ref())
                    .map_or(0, regions::RegionTree::content_hash),
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
            crowns: crowns::Crowns {
                tile,
                regions: c.regions.as_ref(),
                coords: c.regions.as_ref().map(|t| {
                    let u = opts.units();
                    coords::Coords::new(t, u.square_mm, u.layer_mm)
                }),
                frame: c.frame,
            },
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
                Err(e) => return Err(build_error(format!("{}: {}", plan.jobs[i].id, e.message))),
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
            path: &job.id,
            seed: job.seed,
            along: entry.and_then(|e| {
                e.along.as_deref().or_else(|| {
                    e.line
                        .as_ref()
                        .and_then(|n| self.tile.lines.get(n))
                        .map(Vec::as_slice)
                })
            }),
            line_side: entry.and_then(|e| {
                e.line.as_ref().map(|_| match e.side {
                    Some(Side::Left) => dt_elements::LineSide::Left,
                    Some(Side::Right) => dt_elements::LineSide::Right,
                    Some(Side::Centre) | None => dt_elements::LineSide::Centre,
                })
            }),
            at: entry.and_then(|e| e.at.as_ref()),
            printer: *self.opts.printer,
            against: entry.and_then(|e| e.against.as_deref()),
            foots: &self.foots,
            band: self.solved.by_entry[i].map(|b| &self.solved.bodies[b].band),
            below: self.solved.by_entry[i].map(|b| &self.solved.bodies[b].below),
            land_mm: self.solved.by_entry[i].map_or(0.0, |b| self.solved.bodies[b].land_mm),
            calm_mm: self.solved.calm_mm,
            under: self.under.get(i).and_then(Option::as_ref),
            draft: self.opts.draft,
            crowns: Some(&self.crowns),
        }
    }

    /// Entry `i`'s band, low and high, as debug sheets over its region,
    /// when the layer is on, the entry writes its band (`top=` or a pin) and
    /// the band has any depth there.
    fn band_sheets(&self, scene: &mut Scene, i: usize) {
        let Some(b) = self.solved.by_entry[i].filter(|&b| self.solved.bodies[b].written) else {
            return;
        };
        let body = &self.solved.bodies[b];
        let area = &self.masks[self.jobs[i].mask];
        let owner = &self.jobs[i].id;
        let Some(d) = scene.clouds() else {
            return;
        };
        // on the region's lattice, the frame's
        let on_area = |f: &dt_kit::Field| {
            if f.nx == area.nx && f.ny == area.ny {
                f.clone()
            } else {
                <dt_kit::Field as dt_kit::OnFrame>::from_fn(&self.c.frame, |x, y| f.sample(x, y))
            }
        };
        let (lo, hi) = (on_area(body.band.lo()), on_area(body.band.hi()));
        let deep = area
            .bits
            .iter()
            .zip(lo.z.iter().zip(&hi.z))
            .any(|(&a, (l, h))| a && h - l > 0.05);
        if deep {
            // how often each sheet's height comes up at each node: the
            // weighting's density at the low end and at the high end
            let band = &body.band;
            let (bx, by) = (band.lo().nx, band.lo().ny);
            let node = |n: usize| {
                if (bx, by) == (area.nx, area.ny) {
                    return n;
                }
                // another lattice: the band's node nearest this one
                let (i, j) = (n % area.nx, n / area.nx);
                let k = self.c.frame.step_mm() / band.lo().step;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (i, j) = (
                    ((i as f64 * k).round() as usize).min(bx - 1),
                    ((j as f64 * k).round() as usize).min(by - 1),
                );
                j * bx + i
            };
            let shades = |u: f64| -> Vec<f32> {
                // one weighting is read at many nodes: ask once each
                let mut seen: Vec<(*const dt_value::Weighting, f32)> = Vec::new();
                (0..area.bits.len())
                    .map(|n| match band.weighting(node(n)) {
                        None => dt_kit::debug::shade_of(None, u),
                        Some(w) => {
                            let p: *const dt_value::Weighting = w;
                            if let Some(&(_, v)) = seen.iter().find(|(q, _)| *q == p) {
                                v
                            } else {
                                let v = dt_kit::debug::shade_of(Some(w), u);
                                seen.push((p, v));
                                v
                            }
                        }
                    })
                    .collect()
            };
            d.sheet(owner, "band-lo", lo, area.clone(), shades(0.0));
            d.sheet(owner, "band-hi", hi, area.clone(), shades(1.0));
        }
    }

    /// Every step, from the last one `cache` kept.
    fn run(&self, progress: &Progress, cache: Option<&Cache>) -> Result<Ran, Vec<Diag>> {
        let (c, tile) = (self.c, self.tile);
        let mut scene = Scene::new(c.frame, Seed::new(c.seed));
        scene.set_progress(progress.clone());
        scene.set_low(self.opts.low);
        let want = dt_kit::debug::Collect {
            clouds: self.opts.clouds,
            marks: self.opts.debug,
        };
        scene.collect(want);
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
        if let Some((n, k)) = cache.and_then(|c| c.longest(&keys, want)) {
            scene = k.scene;
            scene.collect(want);
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
            // a sketch failing (a triangle of unknown material) is a bug in
            // the scene that the final build would meet too: it travels up
            if lifted && progress.sketching() && !scene.parts().is_empty() {
                let (mesh, mats) = scene
                    .soup_since(0)
                    .map_err(|e| build_error(format!("sketching the build: {e}")))?;
                progress.sketch(&mesh, &mats, false);
                sketched = scene.parts().len();
            }
        }
        for (n, step) in c.steps.iter().enumerate().skip(first) {
            let t_stage = crate::clock::Stopwatch::start();
            let (key, label) = &self.stages[n];
            progress.begin(key, label);
            match *step {
                Step::Lift => scene
                    .lift(&EntryId::new(TERRAIN), &self.opts.printer)
                    .map_err(|e| build_error(e.to_string()))?,
                Step::Entry(i) => {
                    self.band_sheets(&mut scene, i);
                    c.elements[i].build(&mut scene, &self.ctx(i)).map_err(|e| {
                        let msg = format!("{}: {}", self.jobs[i].id, e.message);
                        match tile.stack.get(i) {
                            Some(en) => vec![
                                Diag::at(
                                    &en.src,
                                    en.span,
                                    e.code.unwrap_or(dt_lang::codes::BUILD),
                                    msg,
                                    "while building this",
                                )
                                .maybe_help(e.help),
                            ],
                            None => build_error(msg),
                        }
                    })?;
                }
            }
            // a front end showing the build as it goes gets what this stage made
            lifted |= *step == Step::Lift;
            if lifted && progress.sketching() && scene.parts().len() > sketched {
                let (mesh, mats) = scene
                    .soup_since(sketched)
                    .map_err(|e| build_error(format!("sketching the build: {e}")))?;
                progress.sketch(&mesh, &mats, false);
                sketched = scene.parts().len();
            }
            progress.end(key);
            stage_secs.push((key.clone(), t_stage.elapsed_secs()));
            if let Some(cache) = cache {
                let mut scene = scene.clone();
                scene.set_progress(Progress::none());
                cache.put(
                    n,
                    cache::Kept {
                        key: keys[n],
                        scene,
                        painted,
                        collected: want,
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
        let t_stage = crate::clock::Stopwatch::start();
        progress.begin(progress::UNION, "joining the parts");
        // how the build is painted beyond its materials: the schema reads the
        // document's words (brick masonry), this only carries the answer
        let look = dt_kit::Look {
            brick: dt_doc::schema::masonry_is_brick(tile.stack.iter().map(|e| {
                let style = e.controls.iter().find_map(|c| match &c.value {
                    dt_elements::Value::Word(w) if c.name == "style" => Some(w.as_str()),
                    _ => None,
                });
                (e.what.value.as_str(), style)
            })),
        };
        let (mesh, mats) = if opts.draft || opts.low {
            scene.soup_since(0)
        } else {
            scene.union()
        }
        .map_err(|e| build_error(e.to_string()))?;
        let (entry_in, piece_in) = trace_entries(&scene, &mesh, self);
        progress.end(progress::UNION);
        stage_secs.push((progress::UNION.to_owned(), t_stage.elapsed_secs()));
        let t_stage = crate::clock::Stopwatch::start();
        progress.begin(progress::AUDIT, "checking printability");
        let footprint = c.frame.size_mm();
        // the band every element gets: the printer's overhang to its steepest
        let band = dt_kit::HangBand {
            max_deg: opts.printer.steepest_deg,
            ..dt_kit::HangBand::DEFAULT
        };
        let (export, audit) = if opts.draft || opts.low {
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
        stage_secs.push((progress::AUDIT.to_owned(), t_stage.elapsed_secs()));
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
        let built = Built {
            product: Product {
                export,
                materials,
                audit,
                record,
                out: out_path(path, &opts.target()),
                footprint,
                checked: !(opts.draft || opts.low),
            },
            report: Report {
                stages: std::iter::once(self.plan_stage.clone())
                    .chain(scene.stages().iter().cloned())
                    .collect(),
                edge_mm: self.solved.edge_mm,
                notes: scene.notes().to_vec(),
                left_out: {
                    let mut all = scene.left_outs().to_vec();
                    for b in self.solved.bodies.iter().filter(|b| b.left_out > 0) {
                        match all.iter_mut().find(|(e, _)| *e == b.element) {
                            Some((_, n)) => *n += b.left_out,
                            None => all.push((b.element.clone(), b.left_out)),
                        }
                    }
                    all
                },
                surface: scene.surface(),
                covered: scene
                    .support()
                    .zip(scene.bedded())
                    .map(|(g, bits)| dt_kit::Mask {
                        nx: g.nx,
                        ny: g.ny,
                        bits,
                    }),
                stage_secs,
                reused,
                look,
                entries,
                pieces,
                stack: tile.names(&opts.system),
                clouds: opts.clouds.then(|| cloud_part(scene.collected())).flatten(),
                debug: opts.debug.then(|| mark_part(scene.collected())).flatten(),
                placed: scene.placed().to_vec(),
                cache: CacheUse::default(),
            },
        };
        // a low build keeps to its budget: a cliff's carving is more than a
        // map can show
        Ok(if opts.low {
            crate::tiers::thin_built(built, crate::tiers::LOW_BUDGET)
        } else {
            built
        })
    }
}

fn trace_entries(scene: &Scene, mesh: &dt_kit::TriMesh, plan: &Plan<'_>) -> (Vec<u16>, Vec<u16>) {
    let (tile, c, masks) = (plan.tile, plan.c, &plan.masks);
    let writes = &c.writes;
    let to16 = |i: usize| u16::try_from(i).unwrap_or(NO_ENTRY);
    let mut seen: std::collections::BTreeMap<&EntryId, usize> = std::collections::BTreeMap::new();
    let parts: Vec<(Option<u16>, u16)> = scene
        .parts()
        .iter()
        .map(|p| {
            let k = seen.entry(&p.tag.owner).or_default();
            *k += 1;
            (plan.job_of(&p.tag.owner).map(to16), to16(*k - 1))
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

/// What a scene collected, as the clouds alone (sheets and columns).
fn cloud_part(d: Option<&dt_kit::debug::Debug>) -> Option<dt_kit::debug::Debug> {
    d.map(|d| dt_kit::debug::Debug {
        sheets: d.sheets.clone(),
        columns: d.columns.clone(),
        ..Default::default()
    })
}

/// What a scene collected, as the debug layer's marks alone (dots and trails).
fn mark_part(d: Option<&dt_kit::debug::Debug>) -> Option<dt_kit::debug::Debug> {
    d.map(|d| dt_kit::debug::Debug {
        dots: d.dots.clone(),
        trails: d.trails.clone(),
        ..Default::default()
    })
}

/// The connector's id: the entry the stack's `base` line makes, after the
/// stack's own (its index is the stack's length).
fn connector_id() -> EntryId {
    EntryId::new("tile/connector")
}

/// The id of the ground the terrain lifts into, which no stack entry owns.
const TERRAIN: &str = "tile/terrain";

/// `lib:tests/ground/1x1-flat.dtile` → `db/tests/ground/1x1-flat-db.stl`; a relative
/// disk path keeps its folders below `tiles/` (all of them if it has
/// none), resolved so the result always stays inside `<system>/`: a `..`
/// that would leave it is dropped, and an absolute path keeps none.
pub(crate) fn out_path(path: &str, system: &str) -> PathBuf {
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
