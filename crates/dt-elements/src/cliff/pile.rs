//! The cliff's pile engine: from a band, a seed and a foot to the finished
//! stones. It plans the pile ([`plan`]: the form, the zones, the stacking)
//! and dresses it ([`dress`]: columns, faces, the stones cut, fitted and
//! checked against the band, and the core's field). It knows no scene and no
//! stack entry: the body adapter in [`super`] lays what it returns.

use dt_kit::{Band, Field, Frame, Mask, Material, Progress, Rng, Seed, Slots, Support};
use rayon::prelude::*;

use super::stone::{ColState, Env};
use super::{faces, finish, fit, form, stack, stone, util};
use crate::{LookError, Span};

/// What one column of stones came to: each stone's index and piece, and
/// how many cracks it cut in their tops.
struct Column {
    stones: Vec<(usize, dt_kit::Piece)>,
    cracks: usize,
    left_out: usize,
}

type ColResult = Result<Column, dt_kit::KitError>;

/// The most stones a pile may be made of, per square of its area.
const ROCKS_PER_SQUARE: usize = 400;

/// What the author asked of the pile (the cliff's controls).
#[derive(Clone, Debug)]
pub(super) struct Look {
    pub weights: Option<[f64; 5]>,
    /// stone-size multiplier relative to the form's own (massive = 1).
    pub size: f64,
    pub patches: Option<f64>,
    pub patch_width: Option<Span>,
    pub ledges: u32,
    pub ledge_height: Option<f64>,
}

/// Where the pile stands: everything the engine reads of the tile.
pub(super) struct Site<'a> {
    pub frame: &'a Frame,
    pub seed: Seed,
    /// Where the stones may stand.
    pub area: &'a Mask,
    /// The band their tops are held in.
    pub band: &'a Band,
    /// The lowest the land under the pile reaches (world mm): stones are
    /// counted from it.
    pub foot: f64,
    /// A draft carves on coarser grids and is never written.
    pub draft: bool,
}

/// A pile planned: where the stones stand, with no geometry yet.
pub(super) struct Planned {
    pub fm: form::Form,
    pub zone: form::Zones,
    pub st: stack::Stacked,
    rng: Rng,
    pub foot: f64,
}

impl Planned {
    /// The slots the dressed pile takes: one for its plan, one for its
    /// core, one per stone.
    pub fn slots_needed(&self) -> u32 {
        self.st.rocks.len() as u32 + 3
    }
}

/// The finished pile.
pub(super) struct Dressed {
    /// Each stone with its place in the plan, in plan order, in the pile's
    /// own frame (raise it by the foot).
    pub pieces: Vec<(usize, dt_kit::Piece)>,
    /// How many stones the stack drew over again (the same rock, kept once).
    pub repeated: usize,
    /// Cracks cut in the stones' tops.
    pub tops: usize,
    /// Pieces left out because their hull came out degenerate.
    pub left_out: usize,
    /// The core behind the stones, a field in the pile's grid.
    pub core_field: Field,
}

/// Everything that decides where the stones stand, with no geometry.
pub(super) fn plan(look: &Look, site: &Site<'_>) -> Result<Planned, LookError> {
    let (frame, band) = (site.frame, site.band);
    let s = frame.scale();
    let sq = frame.square_mm;
    // the heaved shape in the band: a plane for one height
    let (shape, _) = crate::floor::heave(frame, site.seed, band);
    let foot = site.foot;
    let mut rng = Rng::new(site.seed.key("cliff"));
    let fm = form::make_form(
        &mut rng,
        &form::Ask {
            frame,
            mask: site.area,
            band,
            shape: &shape,
            foot,
            mass_gain: look.size,
            kinds: look.weights,
            ledges: look.ledges,
            ledge_height: look.ledge_height,
            s,
        },
    );
    let zone = form::Zones::new(
        &mut rng,
        fm.w,
        s,
        look.patches,
        look.patch_width.map(Span::pair),
    );
    let max_rocks = ROCKS_PER_SQUARE * ((fm.w * fm.h / (sq * sq)).ceil() as usize).max(1);
    let fit = fit::Fit::new(band, frame, site.area, foot);
    let mut st = stack::stack(&mut rng, &fm, &zone, s, max_rocks, &fit)
        .map_err(|e| LookError::band(&e, "the cliff's stones"))?;
    stack::reconcile(&mut st.rocks, &fm, &mut st.s);
    Ok(Planned {
        fm,
        zone,
        st,
        rng,
        foot,
    })
}

/// Dress a planned pile: its stones cut, fitted to the band and checked
/// against it (a stone over the band is a bug in the body), and the core's
/// field. `slots` holds [`Planned::slots_needed`].
pub(super) fn dress(
    planned: &Planned,
    site: &Site<'_>,
    slots: &Slots,
    progress: &Progress,
) -> Result<Dressed, LookError> {
    let (fm, zone, st, foot) = (&planned.fm, &planned.zone, &planned.st, planned.foot);
    let (frame, band) = (site.frame, site.band);
    let mut rng = planned.rng.clone();
    let placed = &st.rocks;
    let mut cols = finish::columns(&mut rng, placed, fm, &st.s, zone);
    let plan_slot = slots.get(0);
    // columns that carry another column's stone stand where they are
    for c in &mut cols {
        let cpoly = &c.poly;
        for (ri, r) in placed.iter().enumerate() {
            if c.stones.contains(&ri) || r.z0 <= c.zlo + 1.0 {
                continue;
            }
            let rp = r.poly();
            if shared_area_mm2(&rp, cpoly) > 1.0 {
                c.carries = true;
                c.back.iter_mut().for_each(|b| b.1 = 0.0);
                break;
            }
        }
    }
    for c in &mut cols {
        finish::plan_column(&mut rng, c, placed, fm, plan_slot)
            .map_err(|e| LookError::build(format!("plan column: {e}")))?;
    }
    progress.fraction("dressing columns", 0.03);
    let core_field = super::core::core_field(&st.s, &cols, placed);
    let core_mesh = super::core::core_mesh(&core_field)
        .map_err(|e| LookError::build(format!("core mesh: {e}")))?;
    if progress.sketching() {
        let rough = rough_pile(&core_mesh, placed, foot);
        progress.sketch(&rough, &vec![Material::Rock; rough.triangles.len()], true);
    }
    let mut frng = Rng::new(site.seed.key("cliff-faces"));
    faces::plan_faces(&mut frng, &mut cols, placed);
    // support cones of resting stones, for the cleaving guard; a cone whose
    // hull comes out degenerate is left out, and counted
    let mut cones_left_out = 0;
    let cones: Vec<Option<dt_kit::TriMesh>> = placed
        .iter()
        .map(|r| {
            if r.z0 > 0.0 {
                finish::support_cone(r).unwrap_or_else(|_| {
                    cones_left_out += 1;
                    None
                })
            } else {
                None
            }
        })
        .collect();
    let core_support = Support::new(std::slice::from_ref(&core_mesh)).map_err(LookError::build)?;
    let env = Env {
        form: fm,
        s: &st.s,
        placed,
        seed: site.seed.key("cliff-grain").value(),
        draft: site.draft,
        high: band.hi(),
        foot,
    };
    let col_seed = site.seed.key("cliff-columns");
    progress.fraction("planning faces and cracks", 0.04);
    let ticks = progress.counter("stones", placed.len(), 0.04, 0.85);
    let results: Vec<ColResult> = cols
        .par_iter()
        .enumerate()
        .map(|(ci, c)| {
            let mut rng = Rng::new(col_seed.index(ci as u64));
            let mut crng = Rng::new(col_seed.key("chips").index(ci as u64));
            let b = util::bounds(&c.poly);
            let near: Vec<dt_kit::TriMesh> = cones
                .iter()
                .enumerate()
                .filter_map(|(k, m)| {
                    let m = m.as_ref()?;
                    if c.stones.contains(&k) {
                        return None;
                    }
                    let pb = util::bounds(&placed[k].poly());
                    (pb[0] < b[2] + 3.0
                        && pb[2] > b[0] - 3.0
                        && pb[1] < b[3] + 3.0
                        && pb[3] > b[1] - 3.0)
                        .then(|| m.clone())
                })
                .collect();
            let cone_support = Support::new(&near)
                .map_err(|e| dt_kit::KitError::Shape(format!("cone support: {e}")))?;
            let sup = [&cone_support, &core_support];
            let mut cs = ColState::default();
            let mut out = Vec::new();
            for &ri in c.stones.iter().rev() {
                let slot = slots.get(2 + ri as u32);
                if let Some(p) =
                    stone::rock_piece(&mut rng, &mut crng, ri, c, &mut cs, &env, &sup, slot)?
                {
                    out.push((ri, p));
                }
                ticks.tick();
            }
            Ok(Column {
                stones: out,
                cracks: cs.tops,
                left_out: cs.left_out,
            })
        })
        .collect();
    progress.fraction("stones finished", 0.85);
    let mut pieces: Vec<(usize, dt_kit::Piece)> = Vec::new();
    let mut tops = 0;
    let mut left_out = cones_left_out;
    for r in results {
        let col = r.map_err(LookError::build)?;
        pieces.extend(col.stones);
        tops += col.cracks;
        left_out += col.left_out;
    }
    pieces.sort_by_key(|p| p.0);
    // a stone the band holds down at its site adds no height there, so the
    // stack can draw the same one again and again: the same cut stone,
    // solids that coincide (the same size and place to 0.05 mm), which a union of the pile never
    // finishes. Only the first of each is kept; the rest were the same rock.
    let mut seen = std::collections::HashSet::new();
    let stacked = pieces.len();
    pieces.retain(|(_, p)| {
        let m = p.mesh();
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for v in &m.positions {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let key = |x: f64| (x * 20.0).round() as i64;
        seen.insert((m.triangles.len(), lo.map(key), hi.map(key)))
    });
    let repeated = stacked - pieces.len();
    // a stone that stands over the band is a bug in the body
    let fit = dt_kit::pieces::Pieces::new(band, frame);
    // a draft carves on grids `DRAFT_COARSE` times the print's: its stones
    // may stand that far over the band's high, and are never written
    let band_tol = if site.draft {
        stone::DRAFT_COARSE * faces::RELIEF_GRID_MM
    } else {
        dt_kit::pieces::BAND_TOL_MM
    };
    for (_, p) in &pieces {
        let mut mesh = p.mesh();
        for v in &mut mesh.positions {
            v[2] += foot;
        }
        fit.admits_within(&mesh, band_tol)
            .map_err(|e| LookError::band(&e, "a cliff stone"))?;
    }
    Ok(Dressed {
        pieces,
        repeated,
        tops,
        left_out,
        core_field,
    })
}

/// The node of `frame`'s lattice nearest the cell `(ci, cj)` of the pile's
/// grid, counted by cell centre.
pub(super) fn node_cell(x: f64, y: f64, nx: usize, ny: usize) -> Option<usize> {
    let ci = (x / form::GRID_MM).floor();
    let cj = (y / form::GRID_MM).floor();
    (ci >= 0.0 && cj >= 0.0 && (ci as usize) < nx && (cj as usize) < ny)
        .then(|| cj as usize * nx + ci as usize)
}

/// The pile as boxes: the core and each rock's footprint stood from its
/// underside to its top, raised to stand on `foot`.  A stand-in for the
/// view while the stones are cut, never printed.
fn rough_pile(core: &dt_kit::TriMesh, placed: &[stack::Placed], foot: f64) -> dt_kit::TriMesh {
    let mut out = core.clone();
    out.extra.clear();
    out.num_extra = 0;
    out.merge_from.clear();
    out.merge_to.clear();
    for r in placed {
        let poly = r.poly();
        if poly.len() < 3 {
            continue;
        }
        let top = poly
            .iter()
            .map(|p| r.top_z(p[0], p[1]))
            .fold(f64::NEG_INFINITY, f64::max);
        if top <= r.z0 {
            continue;
        }
        let b = dt_kit::prism_mesh(&[poly], r.z0, top);
        let off = out.positions.len() as u32;
        out.positions.extend(&b.positions);
        out.triangles
            .extend(b.triangles.iter().map(|t| t.map(|v| v + off)));
    }
    for p in &mut out.positions {
        p[2] += foot;
    }
    out
}

/// The area two outlines share, mm², estimated: the points of a 0.5 mm grid
/// over the overlap of their bounds that are inside both, each counted as
/// 0.25 mm².
fn shared_area_mm2(a: &[util::P2], b: &[util::P2]) -> f64 {
    let (ba, bb) = (util::bounds(a), util::bounds(b));
    let (x0, y0, x1, y1) = (
        ba[0].max(bb[0]),
        ba[1].max(bb[1]),
        ba[2].min(bb[2]),
        ba[3].min(bb[3]),
    );
    if x0 >= x1 || y0 >= y1 {
        return 0.0;
    }
    let mut n = 0.0;
    let mut x = x0 + 0.25;
    while x < x1 {
        let mut y = y0 + 0.25;
        while y < y1 {
            if util::point_in(a, [x, y]) && util::point_in(b, [x, y]) {
                n += 0.25;
            }
            y += 0.5;
        }
        x += 0.5;
    }
    n
}
