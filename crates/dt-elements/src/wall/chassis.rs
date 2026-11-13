//! Building a masonry run: the spine walked into courses and cells, each
//! cell made a unit, the units joined on a core or over hearting, seated
//! on the ground, cut to the tile and stamped. The wall and the floor
//! both build through [`run`].

use dt_kit::Kind;
use dt_kit::form::{clip, solid};
use dt_kit::pieces::{Pieces, Plane};
use dt_kit::{Field, Mask, Material, Rng, Scene, Seed, Solid, Tag, TriMesh, union};
use rayon::prelude::*;

use super::block::{Block, Finish, MAX_PROUD_MM};
use super::layout::{Cell, Courses, End, Envelope, Seg, Vertex};
use super::{CORE_ROUND_MM, Family, GAP_MM, Height, Mason, block, fieldstone, layout, rubble};
use crate::{Ctx, LookError};

/// Seat over the footprint: this percentile of the surface.
const SEAT_PERCENTILE: f64 = 80.0;
/// Rubble hearting: spacing, chip foot and height, setback.
const RUBBLE_SPACING_MM: f64 = 2.8;
const RUBBLE_FOOT: (f64, f64) = (3.5, 5.5);
const RUBBLE_H: (f64, f64) = (3.0, 4.5);
const RUBBLE_SETBACK_MM: f64 = 1.3;
const RUBBLE_SETBACK_DRY_MM: f64 = 1.3;
/// Shed rubble at a ruin's foot.
const FOOT_RUBBLE_EVERY_MM: f64 = 5.5;
const FOOT_RUBBLE_OFF: (f64, f64) = (0.8, 6.5);
const FOOT_RUBBLE_FOOT: (f64, f64) = (2.2, 4.8);
const FOOT_RUBBLE_H: (f64, f64) = (1.8, 3.4);
/// Battlements: merlon and crenel widths, crenel depth.
const MERLON_MM: (f64, f64) = (8.0, 11.0);
const CRENEL_MM: (f64, f64) = (5.5, 7.5);
const CRENEL_DEPTH_MM: f64 = 11.0;
/// The most a head joint's end is chipped back at its top, mm (a
/// printability floor: two such ends and the joint stay bridgeable).
const HEAD_PULL_MM: f64 = 0.25;

/// What a laid run (a floor) is made to fit: its band, the height field it
/// draws each piece's height and tilt from, and what it is made of.
pub(crate) struct Laid<'a> {
    /// The band every piece is fitted to, and checked against.
    pub fit: Pieces<'a>,
    /// The heaved surface in the band: each piece takes its height and
    /// tilt from it where it stands.
    pub heave: &'a Field,
    /// The steepest a piece may tilt, as a slope.
    pub tilt_max: f64,
    /// What the pieces are made of.
    pub material: Material,
    /// The lattice step, mm.
    pub step: f64,
    /// How far from the tile's boundary a stone's flank is drawn to it,
    /// true mm (0: none).
    pub pull: f64,
}

/// A piece as laid: its top plane and its box in plan.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Placed {
    pub plane: Plane,
    pub lo: [f64; 2],
    pub hi: [f64; 2],
}

impl Laid<'_> {
    /// What a piece in the box `lo..hi` wants: the height field's height
    /// at its middle and its tilt across it, the tilt held to the
    /// steepest.
    fn want(&self, lo: [f64; 2], hi: [f64; 2]) -> Plane {
        let at = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1])];
        let h = |x: f64, y: f64| self.heave.sample(x, y);
        let gx = (h(hi[0], at[1]) - h(lo[0], at[1])) / (hi[0] - lo[0]).max(1e-6);
        let gy = (h(at[0], hi[1]) - h(at[0], lo[1])) / (hi[1] - lo[1]).max(1e-6);
        let size = gx.hypot(gy);
        let k = if size > self.tilt_max {
            self.tilt_max / size
        } else {
            1.0
        };
        Plane {
            at,
            z: h(at[0], at[1]),
            g: [gx * k, gy * k],
        }
    }

    /// The box `lo..hi` fitted: its top plane, inside the band at every
    /// node it stands on.
    fn plane_for(&self, lo: [f64; 2], hi: [f64; 2], what: &str) -> Result<Plane, LookError> {
        let nodes = self.fit.nodes(lo, hi, self.step);
        self.fit
            .plane(&nodes, self.want(lo, hi))
            .map_err(|r| LookError::band(&r, what))
    }
}

/// The build's frame: segments, seat and how wall-local (t, q, z) maps to
/// the world.
pub(crate) struct Run {
    pub segs: Vec<Seg>,
    /// World z of wall-local 0 (standing).
    pub seat: f64,
    /// Wall-local z of the bottom of the bottom course (standing).
    pub base: f64,
    /// Height (standing) or strip depth in plan (laid), mm.
    pub height: f64,
    /// [`Mason::across`] scaled to true mm.
    pub across: f64,
    pub laid: bool,
    pub tile: [f64; 2],
    pub scale: f64,
}

impl Run {
    /// Wall-local (t, q, z) on segment `k` → world.
    pub fn world(&self, k: usize, p: [f64; 3]) -> [f64; 3] {
        let g = &self.segs[k];
        if self.laid {
            let w = g.at(p[0], p[2]);
            [w[0], w[1], self.across - p[1]]
        } else {
            let w = g.at(p[0], p[1]);
            [w[0], w[1], self.seat + p[2]]
        }
    }

    fn place(&self, k: usize, mut m: TriMesh) -> TriMesh {
        for p in &mut m.positions {
            *p = self.world(k, *p);
        }
        m
    }

    /// Whether the plane through segment `k`'s points `(t, q)` `a` and
    /// `b` lies on the tile boundary.
    pub fn on_edge(&self, k: usize, a: [f64; 2], b: [f64; 2]) -> bool {
        let g = &self.segs[k];
        let (p, q) = (g.at(a[0], a[1]), g.at(b[0], b[1]));
        let on = |v: f64, w: f64, lim: f64| {
            (v.abs() < 1e-6 && w.abs() < 1e-6) || ((v - lim).abs() < 1e-6 && (w - lim).abs() < 1e-6)
        };
        on(p[0], q[0], self.tile[0]) || on(p[1], q[1], self.tile[1])
    }
}

/// `np.percentile` (linear).
pub(crate) fn percentile(mut v: Vec<f64>, pct: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    #[allow(clippy::cast_precision_loss)]
    let r = pct / 100.0 * (v.len() - 1) as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let i = libm::floor(r) as usize;
    let f = r - libm::floor(r);
    if i + 1 < v.len() {
        v[i] + f * (v[i + 1] - v[i])
    } else {
        v[i]
    }
}

/// A unit to build in parallel.
pub(crate) enum Unit {
    /// A block, and for a laid one the top plane it was fitted to.
    Block(Block, Option<Plane>),
    Stone(Box<fieldstone::Stone>, Option<([f64; 2], [f64; 2])>),
    Mesh(TriMesh),
}

/// What a masonry build counted.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Counts {
    pub units: usize,
    pub courses: usize,
    pub chips: usize,
    pub missing: usize,
    pub cracked: usize,
}

/// A keyed stream for a cell.
pub(crate) fn cell_seed(seed: Seed, what: &str, key: &[i64]) -> Seed {
    key.iter()
        .fold(seed.key(what), |s, &k| s.index(k.cast_unsigned()))
}

/// The footprint mask: nodes within each segment's strip `[0, len] ×
/// [0, depth]`.
pub(crate) fn footprint(scene: &Scene, segs: &[Seg], depth: f64) -> Mask {
    // a bend's run goes on past the vertex
    let reach = |v: Vertex| if let Vertex::Bend(e) = v { e } else { 0.0 };
    let f = scene.frame();
    let (nx, ny) = f.nodes();
    let step = f.step_mm();
    let mut m = Mask::new(nx, ny, false);
    for j in 0..ny {
        for i in 0..nx {
            #[allow(clippy::cast_precision_loss)]
            let p = [i as f64 * step, j as f64 * step];
            m.bits[j * nx + i] = segs.iter().any(|g| {
                let (dx, dy) = (p[0] - g.a[0], p[1] - g.a[1]);
                let t = dx * g.d[0] + dy * g.d[1];
                let q = dx * g.n[0] + dy * g.n[1];
                let (lo, hi) = (reach(g.start), reach(g.end));
                t >= -lo - 1e-9 && t <= g.len + hi + 1e-9 && q >= -1e-9 && q <= depth + 1e-9
            });
        }
    }
    m
}

/// Build a masonry run into the scene: a wall or a floor.
pub(crate) fn build(
    m: &Mason,
    scene: &mut Scene,
    ctx: &Ctx<'_>,
    spine: &[[f64; 2]],
    kind: Kind,
) -> Result<(), LookError> {
    let (wall, counts, fp, top, _) = run(m, scene, ctx, spine, kind, &|_| true, None)?;
    finish_build(scene, ctx, kind, wall, counts, &fp, top);
    Ok(())
}

/// Add a built run to the scene, stamp it and note its counts.
pub(crate) fn finish_build(
    scene: &mut Scene,
    ctx: &Ctx<'_>,
    kind: Kind,
    solid: Solid,
    counts: Counts,
    fp: &Mask,
    top: f64,
) {
    scene.add(
        Tag {
            kind,
            owner: ctx.path.clone(),
        },
        solid,
    );
    scene.stamp(fp, top);
    #[allow(clippy::cast_precision_loss)]
    {
        scene.note(&format!("{kind}.units"), counts.units as f64);
        scene.note(&format!("{kind}.courses"), counts.courses as f64);
        scene.note(&format!("{kind}.chips"), counts.chips as f64);
        scene.note(&format!("{kind}.missing"), counts.missing as f64);
        scene.note(&format!("{kind}.cracked"), counts.cracked as f64);
    }
}

/// A built run: the solid, its counts, its footprint and its top.
pub(crate) type Built = (Solid, Counts, Mask, f64, Vec<Placed>);

/// Build a masonry run (not yet added). `keep(cell)` drops cells (a
/// floor outside its area).
#[allow(clippy::too_many_lines)]
pub(crate) fn run(
    m: &Mason,
    scene: &mut Scene,
    ctx: &Ctx<'_>,
    spine: &[[f64; 2]],
    kind: Kind,
    keep: &(dyn Fn(&Cell) -> bool + Sync),
    laid: Option<&Laid<'_>>,
) -> Result<Built, LookError> {
    debug_assert_eq!(m.laid, laid.is_some());
    let frame = *scene.frame();
    let s = frame.scale();
    let tile = frame.size_mm();
    let across = m.across * s;
    let segs = match m.drawn {
        Some(ring) => layout::segments_drawn(spine, ring, across),
        None => layout::segments(spine),
    }
    .map_err(LookError::build)?;
    let seed = ctx.seed;

    // ---- seat (standing) or the strip (laid)
    let (seat, base, height, fp) = if m.laid {
        let h = match m.height {
            Height::Rel(h) | Height::Top(h) => h,
        };
        let fp = footprint(scene, &segs, h);
        (0.0, 0.0, h, fp)
    } else {
        let fp = footprint(scene, &segs, across);
        let mine: Vec<usize> = (0..fp.bits.len())
            .filter(|&k| fp.bits[k] && ctx.area.bits[k])
            .collect();
        if mine.is_empty() {
            return Err(LookError::general(
                format!(
                    "this {kind}'s footprint misses its area: it stands outside its at= region"
                ),
                "put the wall along an edge its region touches, or drop at=",
            ));
        }
        let ter = |k: usize| scene.surface_field().map_or(0.0, |g| f64::from(g.z[k]));
        let surf = |k: usize| {
            let t = ter(k);
            scene
                .support()
                .map(|g| f64::from(g.z[k]))
                .filter(|v| v.is_finite())
                .map_or(t, |v| v.max(t))
        };
        let paved = percentile(mine.iter().map(|&k| surf(k) - ter(k)).collect(), 60.0) > 0.5 * s;
        let embed = if paved { 0.25 * s } else { m.embed * s };
        let seat = percentile(mine.iter().map(|&k| surf(k)).collect(), SEAT_PERCENTILE);
        // the bottom course reaches below the lowest ground under the
        // footprint, so its bottom never shows (walls assume flat ground)
        let lowest = (0..fp.bits.len())
            .filter(|&k| fp.bits[k])
            .map(ter)
            .fold(f64::INFINITY, f64::min);
        let base = (-embed).min(lowest - 0.5 - seat).max(0.3 - seat);
        let height = match m.height {
            Height::Top(z) => (z * s - seat).max(6.0 * s),
            Height::Rel(h) => h * s,
        };
        (seat, base, height, fp)
    };
    let run = Run {
        segs,
        seat,
        base,
        height,
        across,
        laid: m.laid,
        tile,
        scale: s,
    };

    // ---- cells
    let mut lrng = Rng::new(seed.key("layout"));
    let courses = Courses {
        across,
        height,
        course: m.course.scaled(s),
        bay: m.bay.scaled(s),
        min_bond: m.min_bond * s,
        bond: m.bond,
    };
    let mut field = None;
    let mut cells = if m.family == Family::Fieldstone {
        let (c, f) = fieldstone::cells(m, &run, &courses, &mut lrng);
        field = Some(f);
        c
    } else {
        layout::layout(&run.segs, &courses, &mut lrng)
    };
    cells.retain(|c| keep(c));
    let mut crenels = None;
    if m.battlements && !m.laid {
        let (c, b) = layout::crenellate(
            cells,
            &run.segs,
            height,
            (
                (MERLON_MM.0 * s, MERLON_MM.1 * s),
                (CRENEL_MM.0 * s, CRENEL_MM.1 * s),
                CRENEL_DEPTH_MM * s,
            ),
            &mut lrng,
        );
        cells = c;
        crenels = b;
    }
    let mut env: Option<Vec<Envelope>> = None;
    if m.ruin > 0.0 && !m.laid {
        let (c, e) = layout::ruin_cells(cells, &run.segs, across, height, m.ruin, s, &mut lrng);
        cells = c;
        env = Some(e);
    }
    let mut courses_n: Vec<i64> = cells.iter().map(|c| c.key[0]).collect();
    courses_n.sort_unstable();
    courses_n.dedup();

    // ---- units
    let mut units: Vec<Unit> = Vec::new();
    let mut counts = Counts {
        courses: courses_n.len(),
        ..Counts::default()
    };
    let rv = m.reveal * s;
    // at a run's ends and top the core is set in by the joint depth too,
    // but never by half or more of the shortest unit there (a closer can
    // be MIN_BAY_FRAC of the shortest stone, a course as low as the
    // drawn heights make it), or that unit would miss the core and float
    let end_in = rv.min(0.5 * layout::MIN_BAY_FRAC * m.bay.lo() * s);
    let low_course = cells
        .iter()
        .map(|c| c.z1 - c.z0)
        .fold(f64::INFINITY, f64::min);
    let top_in = rv.min(0.5 * low_course);
    let round = CORE_ROUND_MM * s;
    if m.family != Family::Fieldstone && !m.laid {
        for (k, g) in run.segs.iter().enumerate() {
            // a bend's core runs on past the vertex like its stones
            let (b0, b1) = (
                if let Vertex::Bend(e) = g.start {
                    e
                } else {
                    0.0
                },
                if let Vertex::Bend(e) = g.end { e } else { 0.0 },
            );
            let (end_in0, end_in1) = (
                if matches!(g.start, Vertex::Bend(_)) {
                    -b0
                } else {
                    end_in
                },
                if matches!(g.end, Vertex::Bend(_)) {
                    b1
                } else {
                    -end_in
                },
            );
            let mut core = |t0: f64, t1: f64, z0: f64, z1: f64| -> Result<(), LookError> {
                if t1 - t0 > 2.0 * round && z1 - z0 > 2.0 * round {
                    let mesh = rubble::rounded_box([t0, rv, z0], [t1, across - rv, z1], round)
                        .map_err(LookError::build)?;
                    units.push(Unit::Mesh(run.place(k, mesh)));
                }
                Ok(())
            };
            if let Some(e) = &env {
                core(end_in0, g.len + end_in1, base, e[k].min(g.len) - top_in)?;
            } else if let Some(b) = &crenels {
                core(end_in0, g.len + end_in1, base, b.floor_z - top_in)?;
                for &(m0, m1) in &b.merlons[k] {
                    core(
                        (m0 + end_in).max(end_in),
                        (m1 - end_in).min(g.len - end_in),
                        b.floor_z - 2.0 * s,
                        height - top_in,
                    )?;
                }
            } else {
                core(end_in0, g.len + end_in1, base, height - top_in)?;
            }
        }
        // the core across a bend: both runs' core ends and what lies
        // between them
        if env.is_none() && crenels.is_none() {
            let n = run.segs.len();
            for (k, g) in run.segs.iter().enumerate() {
                let Vertex::Bend(_) = g.end else { continue };
                let h = &run.segs[(k + 1) % n];
                let turn = g.d[0] * h.d[1] - g.d[1] * h.d[0];
                if turn.abs() < 0.02 {
                    continue;
                }
                let (z0, z1) = (base, height - top_in);
                let mut pts = Vec::with_capacity(8);
                for q in [rv, across - rv] {
                    for (sg, t) in [(g, g.len), (h, 0.0)] {
                        let p = sg.at(t, q);
                        pts.push([p[0], p[1], run.seat + z0]);
                        pts.push([p[0], p[1], run.seat + z1]);
                    }
                }
                let hull = dt_kit::form::hull(&pts).map_err(LookError::build)?;
                units.push(Unit::Mesh(hull));
            }
        }
    }
    let finish = Finish {
        chip: m.chip * s,
        rounding: m.rounding.lo() * s,
        relief: m.relief.unwrap_or(0.0) * s,
        settle: 0.2 * s,
        yaw_deg: if m.laid { 0.0 } else { m.yaw_deg },
        tilt_deg: m.tilt_deg,
        scale: s,
        grain: m.grain,
        checks: m.checks,
    };
    let mut placed: Vec<Placed> = Vec::new();
    if field.is_some() {
        let mut stones: Vec<fieldstone::Stone> = cells
            .iter()
            .filter_map(|c| fieldstone::stone(m, &run, c, seed))
            .collect();
        if !run.laid {
            fieldstone::hold_proud(&mut stones);
            for (k, pin) in fieldstone::joint_pins(&stones, s).map_err(LookError::build)? {
                units.push(Unit::Mesh(run.place(k, pin)));
            }
        }
        counts.units = stones.len();
        units.extend(stones.into_iter().map(|st| {
            // a laid stone is fitted to its band when it is made (its box
            // held inside the tile: what lies past the plane is cut away)
            let req = laid.map(|_| {
                let (t, z) = st.plan_box();
                (
                    [t[0].max(0.0), z[0].max(0.0)],
                    [t[1].min(tile[0]), z[1].min(tile[1])],
                )
            });
            Unit::Stone(Box::new(st), req)
        }));
    } else {
        let (blocks, extra, missing, put) = blocks(m, &run, &cells, &finish, seed, laid)?;
        placed = put;
        counts.units = blocks.len();
        counts.missing = missing;
        let planes: Vec<Option<Plane>> = if placed.is_empty() {
            vec![None; blocks.len()]
        } else {
            placed.iter().map(|p| Some(p.plane)).collect()
        };
        units.extend(
            blocks
                .into_iter()
                .zip(planes)
                .map(|(b, p)| Unit::Block(b, p)),
        );
        units.extend(extra.into_iter().map(Unit::Mesh));
    }
    // hearting: always for drystone, on ruin for mortared walls
    if m.family == Family::Fieldstone || env.is_some() {
        let cap_z0 = field.as_ref().map_or(height, |f| f.cap_z0);
        let chips = hearting(m, &run, env.as_deref(), cap_z0, seed)?;
        counts.chips = chips.len();
        units.extend(chips.into_iter().map(Unit::Mesh));
    }
    if env.is_some() {
        let feet = foot_rubble(m, &run, scene, seed)?;
        counts.chips += feet.len();
        units.extend(feet.into_iter().map(Unit::Mesh));
    }
    let material = laid.map_or(Material::Masonry, |l| l.material);
    let (solid, laid_stones) = solidify(scene, &units, &run, laid, material)?;
    placed.extend(laid_stones);
    // crumbs the union leaves (a sliver where a tiny closer meets the
    // core's rounding) would be dropped by the gate as lost parts
    let (solid, _) = crate::tree::drop_crumbs(scene, solid, material, true)?;
    // drystone's pebbles leave cracks between their rounded tops that an
    // upper stone's underside can roof by a hair: cut what cannot stand
    // on what is below it (a bevel at the reach angle), as a construction
    // and not a check
    let solid = if m.family == Family::Fieldstone && !m.laid {
        let at = [tile[0] / 2.0, tile[1] / 2.0, seat];
        let r = 0.5 * tile[0].max(tile[1]) + 2.0;
        let footing = crate::tree::ground_box(scene, at, r, seat - 40.0, seat)?;
        let rule = dt_kit::Stand::for_printer(&ctx.printer);
        let (stood, _) = dt_kit::stand(scene, &solid, &footing, Material::Masonry, &rule)
            .map_err(LookError::build)?;
        crate::tree::drop_crumbs(scene, stood, Material::Masonry, true)?.0
    } else {
        solid
    };
    let top = if m.laid { across } else { seat + height };
    Ok((solid, counts, fp, top, placed))
}

/// Every unit a leaf solid (slots reserved in sequence, built in
/// parallel, gathered in index order), unioned and cut to the tile.
pub(crate) fn solidify(
    scene: &mut Scene,
    units: &[Unit],
    run: &Run,
    laid: Option<&Laid<'_>>,
    material: Material,
) -> Result<(Solid, Vec<Placed>), LookError> {
    let n = u32::try_from(units.len()).map_err(LookError::build)?;
    let slots = scene.slots(n + 1, material).map_err(LookError::build)?;
    type Piece = Result<(Solid, [f64; 3], [f64; 3], Option<Placed>), LookError>;
    let ticks = scene.progress().counter("stones", units.len(), 0.0, 0.9);
    let low = scene.low();
    let built: Vec<Piece> = units
        .par_iter()
        .enumerate()
        .map(|(i, u)| {
            ticks.tick();
            let mut put = None;
            let mesh = match u {
                Unit::Block(b, plane) => {
                    let mut mesh = if low {
                        b.hull(&run.segs[b.seg])
                    } else {
                        b.mesh(&run.segs[b.seg])
                    }
                    .map_err(LookError::build)?;
                    if let Some(plane) = plane {
                        // the relief's high points may stand over the fitted
                        // plane: the block is seated down to it (its tilt
                        // kept), so the grain's highest reaches the band's
                        // top and no further
                        let over = mesh
                            .positions
                            .iter()
                            .map(|p| p[2] - plane.height([p[0], p[1]]))
                            .fold(0.0_f64, f64::max);
                        if over > 0.0 {
                            for p in &mut mesh.positions {
                                p[2] -= over;
                            }
                        }
                    }
                    mesh
                }
                Unit::Stone(st, req) => {
                    let mut mesh = run.place(st.seg, st.mesh().map_err(LookError::build)?);
                    if let Some(l) = laid.filter(|l| l.pull > 0.0) {
                        pull_flanks(&mut mesh, run.tile, l.pull);
                    }
                    if let (Some(l), Some((lo, hi))) = (laid, req) {
                        put = Some(fit_stone(l, &mut mesh, *lo, *hi)?);
                    }
                    mesh
                }
                Unit::Mesh(mm) => mm.clone(),
            };
            if let (Some(l), true) = (laid, !matches!(u, Unit::Mesh(_))) {
                l.fit
                    .admits(&mesh)
                    .map_err(|r| LookError::band(&r, "a piece of the floor"))?;
            }
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for p in &mesh.positions {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            Ok((
                solid(slots.get(i), &mesh).map_err(LookError::build)?,
                lo,
                hi,
                put,
            ))
        })
        .collect();
    let mut solids = Vec::with_capacity(built.len());
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let mut put = Vec::new();
    for b in built {
        let (sd, l, h, p) = b?;
        put.extend(p);
        for k in 0..3 {
            lo[k] = lo[k].min(l[k]);
            hi[k] = hi[k].max(h[k]);
        }
        solids.push(sd);
    }
    let all = union(&solids).map_err(LookError::build)?;
    // butt joins: cut flush at the tile boundary; a floor is cut flat at
    // the datum (the connector owns z < 0), or with an edge contract at
    // the edge level: the ground never dips below it, so the cut is
    // always buried, and the tile's side reads earth below one straight
    // line and stone above it. Cut at the datum, the slabs' clipped sides
    // and the earth's side shared a plane from 0 to the edge level and
    // the union handed its faces to either material. (Standing walls
    // stand inside the tile and never reach its side.)
    let z_lo = if laid.is_some() {
        // the slabs stand on the land under them: they reach down to its
        // lowest, so none lies over a hollow (zero, the base, where there
        // is no land)
        scene.land().map_or(0.0, |l| {
            l.z.iter().copied().fold(f64::INFINITY, f64::min) - 0.05
        })
    } else {
        lo[2] - 10.0
    };
    let cut = clip(
        &all,
        slots.get(n),
        [0.0, 0.0, z_lo],
        [run.tile[0], run.tile[1], hi[2] + 10.0],
    )
    .map_err(LookError::build)?;
    Ok((cut, put))
}

/// Draw a laid stone's upper flank to the tile's plane where it stands near
/// it: a vertex within `reach` of a side moves toward that side, the more the
/// higher it is on the stone and the nearer the plane, so the pillow's
/// top edge meets the plane at about the stone's top and its face keeps
/// the stone's own relief (scaled down) a little behind the plane. Nothing
/// moves past the plane, and a vertex only moves toward it, so the flank
/// never leans out.
fn pull_flanks(mesh: &mut TriMesh, tile: [f64; 2], reach: f64) {
    let foot = mesh
        .positions
        .iter()
        .map(|p| p[2])
        .fold(f64::INFINITY, f64::min);
    let apex = mesh
        .positions
        .iter()
        .map(|p| p[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (apex - foot).max(1e-9);
    let smooth = |t: f64| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    for p in &mut mesh.positions {
        let up = smooth(1.3 * (p[2] - foot) / span);
        for axis in 0..2 {
            let (lo, hi) = (p[axis], tile[axis] - p[axis]);
            let (d, from_hi) = if lo <= hi { (lo, false) } else { (hi, true) };
            let w = 1.0 - smooth(d / reach).powi(2);
            let moved = d * (1.0 - w * up);
            p[axis] = if from_hi { tile[axis] - moved } else { moved };
        }
    }
}

/// Fit a laid stone's finished mesh to its band: its highest point is taken
/// to a plane drawn from the height field, inside the band over the box
/// it stands in, and the stone is stretched up or down from its foot (the
/// foot stays where it is, so nothing is left hanging over the land).
fn fit_stone(
    l: &Laid<'_>,
    mesh: &mut TriMesh,
    lo: [f64; 2],
    hi: [f64; 2],
) -> Result<Placed, LookError> {
    let plane = l.plane_for(lo, hi, "a flagstone")?;
    let foot = mesh
        .positions
        .iter()
        .map(|p| p[2])
        .fold(f64::INFINITY, f64::min);
    let apex = mesh
        .positions
        .iter()
        .map(|p| p[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let span = (apex - foot).max(1e-9);
    for p in &mut mesh.positions {
        let top = plane.height([p[0], p[1]]);
        p[2] = foot + (p[2] - foot) * (top - foot) / span;
    }
    Ok(Placed { plane, lo, hi })
}

/// The blocks of a run, the extra meshes (mortar, pointing), how many cells
/// are missing, and where each block was laid.
type BlocksMade = (Vec<Block>, Vec<TriMesh>, usize, Vec<Placed>);

/// Blocks for every cell (mortared families and slabs), held printable,
/// plus the mortar under a missing brick and the pointing of joints wider
/// than the printer's gap. Returns (blocks, extra meshes, missing count).
#[allow(clippy::too_many_lines)]
pub(crate) fn blocks(
    m: &Mason,
    run: &Run,
    cells: &[Cell],
    finish: &Finish,
    seed: Seed,
    laid: Option<&Laid<'_>>,
) -> Result<BlocksMade, LookError> {
    let s = finish.scale;
    // the joint scales with everything else
    let jw = m.joint * s;
    let j2 = jw / 2.0;
    let fr = m.face_recess * s;
    let t = run.across;
    // missing units: bricks (never bottom, top, quoins or ends) and slabs,
    // never two stacked
    let mut order: Vec<usize> = (0..cells.len()).collect();
    order.sort_by(|&a, &b| cells[a].z0.total_cmp(&cells[b].z0).then(a.cmp(&b)));
    let mut missing = vec![false; cells.len()];
    if m.missing > 0.0 {
        for &i in &order {
            let c = &cells[i];
            let eligible = m.laid
                || (!c.is_bottom
                    && !c.is_top
                    && !c.is_quoin
                    && c.end0 == End::Joint
                    && c.end1 == End::Joint);
            let stacked = !m.laid
                && order.iter().any(|&o| {
                    let b = &cells[o];
                    missing[o]
                        && b.seg == c.seg
                        && (b.z1 - c.z0).abs() < 0.6
                        && b.t0.max(c.t0) < b.t1.min(c.t1)
                });
            let mut r = Rng::new(cell_seed(seed, "missing", &c.key));
            if eligible && !stacked && r.random() < m.missing {
                missing[i] = true;
            }
        }
    }
    let mut out: Vec<(usize, Block)> = Vec::new();
    let mut placed: Vec<Placed> = Vec::new();
    for (i, c) in cells.iter().enumerate() {
        if missing[i] {
            continue;
        }
        let bs = cell_seed(seed, "unit", &c.key);
        let mut rng = Rng::new(bs.key("draw"));
        let face0 = c.end0 == End::Face;
        let face1 = c.end1 == End::Face;
        let mut x0 = c.t0 + if face0 { rng.uniform(0.0, fr) } else { j2 };
        let mut x1 = c.t1 - if face1 { rng.uniform(0.0, fr) } else { j2 };
        // at an inside corner a block butts the other run's inner face,
        // which sits back by its face recess: run on into it, so the
        // corner joint never opens wider than the block above can bridge
        if !run.laid {
            let g = &run.segs[c.seg];
            let reach = fr + 0.1 * s;
            if g.start == Vertex::Quoin && !face0 && (c.t0 - t).abs() < 1e-6 {
                x0 -= reach;
            }
            if g.end == Vertex::Quoin && !face1 && (c.t1 - (g.len - t)).abs() < 1e-6 {
                x1 += reach;
            }
        }
        let q0 = rng.uniform(0.0, fr);
        let q1 = t - rng.uniform(0.0, fr);
        let mut f = *finish;
        // spalled: a broken unit with a big chip budget
        let spall = m.spall > 0.0 && !c.is_quoin && rng.random() < m.spall;
        let (bx, mask, exposed_top) = if run.laid {
            let y = [
                c.z0 + if c.is_bottom { 0.0 } else { j2 },
                c.z1 - if c.is_top { 0.0 } else { j2 },
            ];
            if spall {
                f.chip += (0.30 * (x1 - x0).min(y[1] - y[0]) * 0.25).min(0.30 * s);
            }
            // a laid piece takes its top from the band: a plane fitted to
            // it over the nodes it stands on, held under the steepest tilt
            let (xs, ys, zt) = if let Some(l) = laid {
                let plane = l.plane_for([x0, y[0]], [x1, y[1]], "a slab")?;
                // on the tile's boundary the piece runs past the plane, so
                // the cut there is square and its top is whole
                let pad = m.edge_pad;
                let xs = [
                    if c.t0 < 1e-6 && pad > 0.0 {
                        x0 - pad
                    } else {
                        x0
                    },
                    if c.t1 > run.tile[0] - 1e-6 && pad > 0.0 {
                        x1 + pad
                    } else {
                        x1
                    },
                ];
                let ys = [
                    if y[0] < 1e-6 && pad > 0.0 {
                        y[0] - pad
                    } else {
                        y[0]
                    },
                    if y[1] > run.tile[1] - 1e-6 && pad > 0.0 {
                        y[1] + pad
                    } else {
                        y[1]
                    },
                ];
                let mid = [0.5 * (xs[0] + xs[1]), 0.5 * (ys[0] + ys[1])];
                let zt = plane.height(mid);
                placed.push(Placed {
                    plane,
                    lo: [x0, y[0]],
                    hi: [x1, y[1]],
                });
                (xs, ys, zt)
            } else {
                ([x0, x1], y, t - q0)
            };
            (
                (xs, ys, [-1.0, zt]),
                [!face0, !face1, !c.is_bottom, !c.is_top],
                true,
            )
        } else {
            let z = [
                run.seat + if c.is_bottom { run.base } else { c.z0 + j2 },
                run.seat + c.z1 - if c.is_top { 0.0 } else { j2 },
            ];
            if spall {
                f.chip = (0.35 * (x1 - x0).min(z[1] - z[0])).min(1.2 * s);
            }
            (([x0, x1], [q0, q1], z), [true; 4], c.is_top)
        };
        let mut b = block::draw(&mut rng, c.seg, bx, &f, exposed_top, exposed_top, mask, bs);
        if run.laid {
            b.exposed = [false; 4];
            if let Some(p) = placed.last() {
                // the tilt and height are the fitted plane's, and nothing
                // settles under it
                b.tilt = p.plane.g;
                b.drop = 0.0;
                if f.checks > 0.0 && rng.random() < f.checks {
                    let end = if rng.random() < 0.5 { 0.0 } else { 1.0 };
                    let across = rng.uniform(0.2 * (b.y[1] - b.y[0]), 0.8 * (b.y[1] - b.y[0]));
                    b.check = Some([end, across, 0.3 * s]);
                }
            }
        } else {
            b.exposed = [face0, face1, true, true];
            // a head joint opens no wider at its top than the unit above
            // can bridge (its two chipped ends lean apart)
            if !face0 {
                b.top[0] = b.top[0].min(HEAD_PULL_MM);
            }
            if !face1 {
                b.top[1] = b.top[1].min(HEAD_PULL_MM);
            }
            // a face on the tile boundary keeps its relief and its yaw
            // inside the tile (no stone planed at the boundary)
            let tooth = if b.carve > 0.0 { 0.1 } else { 0.0 };
            let lx = b.x[1] - b.x[0];
            if run.on_edge(c.seg, [c.t0, 0.0], [c.t1, 0.0]) {
                b.y[0] += b.yaw.abs() * lx / 2.0 + tooth;
            }
            if run.on_edge(c.seg, [c.t0, t], [c.t1, t]) {
                b.y[1] -= b.yaw.abs() * lx / 2.0 + tooth;
            }
            if face0 && run.on_edge(c.seg, [c.t0, 0.0], [c.t0, t]) {
                b.x[0] += tooth;
            }
            if face1 && run.on_edge(c.seg, [c.t1, 0.0], [c.t1, t]) {
                b.x[1] -= tooth;
            }
        }
        out.push((i, b));
    }
    let cell_of: Vec<usize> = out.iter().map(|(i, _)| *i).collect();
    let mut blocks: Vec<Block> = out.into_iter().map(|(_, b)| b).collect();
    if !run.laid {
        block::hold_proud(&mut blocks, &run.segs);
    }
    let mut extra = Vec::new();
    if !run.laid {
        // mortar under each missing brick's roof: sloped back at 45° from
        // within MAX_PROUD_MM of the bricks above (the socket's roof prints)
        for (i, c) in cells.iter().enumerate() {
            if !missing[i] {
                continue;
            }
            let above: Vec<&Block> = blocks
                .iter()
                .zip(&cell_of)
                .filter(|(_, ci)| {
                    let o = &cells[**ci];
                    o.seg == c.seg && (o.z0 - c.z1).abs() < 0.6 && o.t0.max(c.t0) < o.t1.min(c.t1)
                })
                .map(|(b, _)| b)
                .collect();
            for outer in [true, false] {
                let face = above
                    .iter()
                    .map(|b| {
                        let swing = b.yaw.abs() * (b.x[1] - b.x[0]) / 2.0;
                        if outer {
                            b.y[0] + b.bot[2] - swing
                        } else {
                            t - (b.y[1] - b.bot[3] + swing)
                        }
                    })
                    .fold(f64::INFINITY, f64::min);
                let qw = if face.is_finite() { face + 0.35 } else { 0.35 };
                let back = (m.reveal * s + 0.3).max(qw + 0.1);
                let zr = c.z1 + j2 + 0.05;
                let drop = back - qw;
                let q = |v: f64| if outer { v } else { t - v };
                let mut pts = Vec::new();
                for tt in [c.t0 - j2, c.t1 + j2] {
                    pts.push([tt, q(qw), zr]);
                    pts.push([tt, q(back), zr]);
                    pts.push([tt, q(back), zr - drop]);
                }
                let mesh = dt_kit::form::hull(&pts).map_err(LookError::build)?;
                extra.push(run.place(c.seg, mesh));
            }
        }
        // pointing: a bed joint wider than the gap is filled to within
        // MAX_PROUD_MM of the faces, so the unit above never bridges it
        if jw > GAP_MM {
            for (b, &ci) in blocks.iter().zip(&cell_of) {
                let c = &cells[ci];
                if c.is_bottom {
                    continue;
                }
                let g = MAX_PROUD_MM;
                let t0 = if c.end0 == End::Face {
                    c.t0 + g
                } else {
                    c.t0 - j2
                };
                let t1 = if c.end1 == End::Face {
                    c.t1 - g
                } else {
                    c.t1 + j2
                };
                let z = b.z[0] - run.seat;
                let mesh = rubble::boxed([t0, g, z - jw - 0.02], [t1, t - g, z + 0.02])
                    .map_err(LookError::build)?;
                extra.push(run.place(c.seg, mesh));
            }
        }
    }
    let n_missing = missing.iter().filter(|&&x| x).count();
    Ok((blocks, extra, n_missing, placed))
}

/// `_hearting_parts`: a sealed sheet of small rough chips through the
/// body, two layers half-pitch staggered, set back from every face and
/// end; under the coping or top course for drystone, under the break for
/// a ruin.
fn hearting(
    m: &Mason,
    run: &Run,
    env: Option<&[Envelope]>,
    cap_z0: f64,
    seed: Seed,
) -> Result<Vec<TriMesh>, LookError> {
    let s = run.scale;
    let (t, h) = (run.across, run.height);
    // drystone sets the chips deeper: its stones' faces curl back toward
    // the cracks, and a chip must never show past them
    let sb = if m.family == Family::Fieldstone {
        RUBBLE_SETBACK_DRY_MM * s
    } else {
        RUBBLE_SETBACK_MM * s
    };
    let sp = RUBBLE_SPACING_MM * s;
    let bands = [(sb, 0.55 * t), (0.45 * t, t - sb)];
    let mut rng = Rng::new(seed.key("hearting"));
    let mut out = Vec::new();
    for (k, g) in run.segs.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let nt = (libm::round(g.len / sp) as usize + 1).max(2);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let nz = (libm::round(h / sp) as usize + 1).max(2);
        for (layer, &(y0, y1)) in bands.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let off = 0.5 * layer as f64;
            for i in 0..nt {
                for j in 0..nz {
                    #[allow(clippy::cast_precision_loss)]
                    let (fi, fj, fnt, fnz) = (i as f64, j as f64, nt as f64, nz as f64);
                    let stagger = if j % 2 == 1 { 0.5 } else { 0.0 };
                    let tc = (fi + stagger + off + rng.uniform(-0.25, 0.25)) * g.len / (fnt - 1.0);
                    let zc = (fj + off + rng.uniform(-0.25, 0.25)) * h / (fnz - 1.0);
                    let w = rng.uniform(RUBBLE_FOOT.0, RUBBLE_FOOT.1) * s;
                    let hh = rng.uniform(RUBBLE_H.0, RUBBLE_H.1) * s;
                    let t0 = (tc - w / 2.0).max(sb).min(g.len - sb - w);
                    let top = env.map_or(h, |e| e[k].at(tc)).min(cap_z0);
                    let z0 = (zc - hh / 2.0).max(0.2 * s).min(top - 0.3 * s - hh);
                    if z0 < 0.2 * s - 1e-9 || t0 < sb - 1e-9 {
                        continue;
                    }
                    let mesh = rubble::rubble([t0, y0, z0], [t0 + w, y1, z0 + hh], &mut rng)
                        .map_err(LookError::build)?;
                    out.push(run.place(k, mesh));
                }
            }
        }
    }
    Ok(out)
}

/// `_foot_rubble`: shards shed at a ruin's foot, both sides, sunk past
/// their widest so no underside shows; only shards wholly inside the
/// tile (none is planed at the boundary).
fn foot_rubble(m: &Mason, run: &Run, scene: &Scene, seed: Seed) -> Result<Vec<TriMesh>, LookError> {
    let s = run.scale;
    let t = run.across;
    let mut rng = Rng::new(seed.key("foot"));
    let mut out = Vec::new();
    let grid = scene.surface_field();
    let step = scene.frame().step_mm();
    let ground = |p: [f64; 2]| -> f64 {
        let Some(g) = grid else { return run.seat };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (
            (libm::round(p[0] / step).max(0.0) as usize).min(g.nx - 1),
            (libm::round(p[1] / step).max(0.0) as usize).min(g.ny - 1),
        );
        f64::from(g.z[j * g.nx + i])
    };
    for (k, g) in run.segs.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (m.ruin * g.len / (FOOT_RUBBLE_EVERY_MM * s)) as usize;
        for _ in 0..n {
            let tt = rng.uniform(2.0 * s, g.len - 2.0 * s);
            let off = rng.uniform(FOOT_RUBBLE_OFF.0, FOOT_RUBBLE_OFF.1) * s;
            let q = if rng.random() < 0.5 { -off } else { t + off };
            let w = rng.uniform(FOOT_RUBBLE_FOOT.0, FOOT_RUBBLE_FOOT.1) * s;
            let d = w * rng.uniform(0.7, 1.0);
            let h = rng.uniform(FOOT_RUBBLE_H.0, FOOT_RUBBLE_H.1) * s;
            let mut chip = Rng::new(seed.key("foot").index(rng.next_u64()));
            let corners = [
                g.at(tt - w / 2.0, q - d / 2.0),
                g.at(tt + w / 2.0, q - d / 2.0),
                g.at(tt - w / 2.0, q + d / 2.0),
                g.at(tt + w / 2.0, q + d / 2.0),
            ];
            let inside = corners.iter().all(|c| {
                c[0] > 0.05 && c[1] > 0.05 && c[0] < run.tile[0] - 0.05 && c[1] < run.tile[1] - 0.05
            });
            if !inside {
                continue;
            }
            let zg = corners
                .iter()
                .map(|c| ground(*c))
                .fold(ground(g.at(tt, q)), f64::min);
            let z0 = zg - 0.55 * h - run.seat;
            let mesh = rubble::rubble(
                [tt - w / 2.0, q - d / 2.0, z0],
                [tt + w / 2.0, q + d / 2.0, z0 + h],
                &mut chip,
            )
            .map_err(LookError::build)?;
            out.push(run.place(k, mesh));
        }
    }
    Ok(out)
}
