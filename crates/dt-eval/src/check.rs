//! **check**: read a document and plan it, no geometry.

use dt_doc::schema::did_you_mean;
use dt_elements::{Control, Element};
use dt_kit::{Frame, Mask};

use crate::diag::Diag;
use crate::doc::{self, Tile};
use crate::options::{DRAFT_CELLS, Options};
use crate::plan::{self, Refusal, Step};
use crate::source::Source;
use crate::{places, regions, solve};

/// The plan of a document, without geometry.
pub struct Checked {
    /// The file as dt-doc decoded it.
    pub file: std::sync::Arc<doc::File>,
    /// Its tile as read, if it has one (a parts, printer or levels file
    /// has none: it is only decoded).
    pub tile: Option<Tile>,
    /// The tile's build order.
    pub steps: Vec<Step>,
    pub(crate) elements: Vec<Box<dyn Element>>,
    /// What each element writes, from its plan.
    pub(crate) writes: Vec<dt_elements::Write>,
    /// Each step's area: `None` for the whole tile, else a region index.
    /// The masks are `masks`, by region index.
    pub(crate) areas: Vec<Option<usize>>,
    /// Every region of the tile as a mask on the build's lattice, made once
    /// by `check` and shared by the empty-region check, the region tree, the
    /// Solve phase and the build (empty when the tile has no regions or one
    /// cannot be made).
    pub(crate) masks: Vec<Mask>,
    pub(crate) frame: Frame,
    pub(crate) seed: u64,
    /// The Solve phase, for a tile that uses the heights language: its
    /// bodies' bands, and the lints that came with them.
    pub solved: Option<solve::SolveReport>,
    /// The region tree of a format 1 tile (every thing's region, and what
    /// each region's parent is), on the build's lattice.
    pub regions: Option<regions::RegionTree>,
}

impl Checked {
    /// What each element of the plan reads and writes, in stack order, the
    /// connector last when the tile has a `base`: the plan's own input, for
    /// asking whether another entry could stand somewhere in the stack
    /// ([`plan::schedule`]).
    #[must_use]
    pub fn plans(&self) -> Vec<dt_elements::Plan> {
        self.elements.iter().map(|l| l.plan()).collect()
    }

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
    ///
    /// # Errors
    /// A region that cannot be made, at the region (the check has made every
    /// region on its own lattice, so this does not come up for a tile that
    /// passed it).
    pub fn region_masks(&self, cells: u32) -> Result<Vec<Mask>, Vec<Diag>> {
        let Some(tile) = &self.tile else {
            return Ok(Vec::new());
        };
        let frame = Frame {
            cells_per_square: cells.max(1),
            ..self.frame
        };
        places::Places::new(tile, &frame)
            .all_regions()
            .map_err(|flaws| {
                flaws
                    .into_iter()
                    .map(|(i, why)| region_diag(tile, &tile.regions[i], why))
                    .collect()
            })
    }
}

/// **check**: read `path` and plan it (no geometry).
///
/// # Errors
/// Every problem found, each at its text.
pub fn check(sources: &dyn Source, path: &str, opts: &Options) -> Result<Checked, Vec<Diag>> {
    let units = opts.units();
    let read = doc::read_with(sources, path, units)?;
    let Some(mut tile) = read.tile else {
        return Ok(Checked {
            file: read.file,
            tile: None,
            steps: Vec::new(),
            elements: Vec::new(),
            writes: Vec::new(),
            areas: Vec::new(),
            masks: Vec::new(),
            frame: Frame::dungeonblocks(1, 1),
            seed: 0,
            solved: None,
            regions: None,
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
                let code = le.code.unwrap_or("dt::element");
                let d = match at {
                    Some(s) => Diag::at(&s.src, s.span, code, le.message, "here"),
                    None => Diag::at(&e.src, e.what.span, code, le.message, "here"),
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
    let frame = if let Some(cells) = opts.cells {
        Frame {
            cells_per_square: cells,
            ..frame
        }
    } else if opts.draft {
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
    // a region that cannot be made is said here, where it is written; only
    // regions that can be made are looked at for being empty
    let masks = match places::Places::new(&tile, &frame).all_regions() {
        Ok(masks) => {
            errors.extend(empty_regions(&tile, &masks, &areas));
            masks
        }
        Err(flaws) => {
            errors.extend(
                flaws
                    .into_iter()
                    .map(|(i, why)| region_diag(&tile, &tile.regions[i], why)),
            );
            Vec::new()
        }
    };
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
    // format 1: every thing's region, and what each formula can reach
    let region_tree = if errors.is_empty() {
        match regions::RegionTree::build(&tile, &frame, &masks) {
            Ok(tree) => {
                errors.extend(tree.parent_flaws(&tile));
                errors.extend(tree.hole_flaws(&tile));
                Some(tree)
            }
            Err(d) => {
                errors.push(d);
                None
            }
        }
    } else {
        None
    };
    // format 1: a pin's point is worked out in the frame it was written in,
    // and must lie in the region its body acts in
    if let Some(tree) = &region_tree {
        errors.extend(tree.resolve_pins(&mut tile, units));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let plans: Vec<_> = elements.iter().map(|l| l.plan()).collect();
    let steps = plan::schedule(&plans).map_err(|r| {
        // the entry's element and line, for a help that names another entry
        let named = |i: usize| match tile.stack.get(i) {
            Some(e) => {
                let line = Diag::at(&e.src, e.span, "", "", "")
                    .location()
                    .map_or(String::new(), |(_, l, _)| format!(" (line {l})"));
                format!("`{}`{line}", e.what.value)
            }
            None => "an entry".to_owned(),
        };
        let (msg, entry, help, code) = match &r {
            Refusal::NoWriter { entry, kind } => (
                format!("this reads every {kind}, and nothing in the tile makes one"),
                *entry,
                "remove the read, or add what it reads to the stack".to_owned(),
                "dt::plan",
            ),
            Refusal::SurfaceReadsSolid { entry } => (
                "a surface cannot read solids yet".to_owned(),
                *entry,
                "remove the read, or add what it reads to the stack".to_owned(),
                "dt::plan",
            ),
            Refusal::Early {
                entry,
                later,
                kind,
            } => {
                let (what, fix) = match kind {
                    Some(dt_elements::Kind::Terrain) => (
                        "stands on the ground, and the ground is painted after it in the stack"
                            .to_owned(),
                        "the stack is built in the order it is listed: list a `ground` or `dirt` before it".to_owned(),
                    ),
                    Some(k) => (
                        format!("reads every {k}, and the first one is listed after it in the stack"),
                        "the stack is built in the order it is listed: list it after what it reads".to_owned(),
                    ),
                    None => (
                        "lies on the ground, and a surface is painted after it in the stack"
                            .to_owned(),
                        "the stack is built in the order it is listed: list it after every `ground` or `dirt` it lies on".to_owned(),
                    ),
                };
                (
                    format!("this {what}"),
                    *entry,
                    format!("{fix}; {} comes after it", named(*later)),
                    "dt::order",
                )
            }
        };
        let d = match tile.stack.get(entry) {
            Some(e) => Diag::at(&e.src, e.span, code, msg, "listed too early"),
            None => Diag::plain(code, msg),
        };
        vec![d.help(help)]
    })?;
    let seed = opts.seed.unwrap_or(tile.seed);
    // the Solve phase: bands and pins, no meshes, on the lattice the build
    // makes its bodies on: `check` and `build` share this one solve
    let writes: Vec<dt_elements::Write> = plans.iter().map(|p| p.writes).collect();
    let mut solved = solve::solve(&tile, &frame, opts, region_tree.as_ref(), &masks)?;
    solved.notes.extend(tile.notes.iter().cloned());
    if let Some(tree) = &region_tree {
        solved.explain.extend(tree.explain(&tile));
    }
    let solved = Some(solved);
    Ok(Checked {
        file: read.file,
        tile: Some(tile),
        steps,
        writes,
        elements,
        areas,
        masks,
        frame,
        seed,
        solved,
        regions: region_tree,
    })
}

/// An entry's area: `None` for the whole tile, or its `at=` region.
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
                    .or_else(|| Some("without at=, an element covers the whole tile".into())),
            ),
        );
    }
    found
}

/// A region that cannot be made (regions that name each other round a loop,
/// a name that is no place), as an error where the region is written.
fn region_diag(tile: &Tile, r: &doc::Region, msg: String) -> Diag {
    Diag::at(&tile.src, r.span, "dt::region", msg, "this region").help(
        "a region is made of lines, fill points, `within` and `except` of other regions, none of which may lead back to it",
    )
}

/// One error for each region an element acts in that covers no ground: its
/// fill points all lie on its lines.
fn empty_regions(tile: &Tile, masks: &[Mask], areas: &[Option<usize>]) -> Vec<Diag> {
    let used: std::collections::BTreeSet<usize> = areas.iter().flatten().copied().collect();
    used.into_iter()
        .filter_map(|i| {
            let r = &tile.regions[i];
            if r.fills.is_empty() {
                return None;
            }
            if masks[i].bits.iter().any(|&b| b) {
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
/// into them: the tile's regions' masks, made once by `check`, copied.
pub(crate) fn masks(
    regions: &[Mask],
    frame: &Frame,
    areas: &[Option<usize>],
) -> (Vec<Mask>, Vec<usize>) {
    let (nx, ny) = frame.nodes();
    let mut keys: Vec<Option<usize>> = Vec::new();
    let mut out = Vec::new();
    let mut index = Vec::new();
    for a in areas {
        let at = if let Some(at) = keys.iter().position(|k| k == a) {
            at
        } else {
            keys.push(*a);
            out.push(match a {
                None => Mask::new(nx, ny, true),
                Some(i) => regions[*i].clone(),
            });
            out.len() - 1
        };
        index.push(at);
    }
    (out, index)
}
