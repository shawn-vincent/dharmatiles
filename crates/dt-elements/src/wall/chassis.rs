//! Building a masonry run: the spine walked into courses and cells, each
//! cell made a unit, the units joined on a core or over hearting, seated
//! on the ground, cut to the tile and stamped. The wall and the floor
//! both build through [`run`].

use dt_kit::Kind;
use dt_kit::form::{clip, solid};
use dt_kit::{Mask, Material, Rng, Scene, Seed, Solid, Tag, TriMesh, union};
use rayon::prelude::*;

use super::block::{Block, Finish, MAX_PROUD_MM};
use super::layout::{Cell, Courses, End, Envelope, Seg};
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
    pub thickness: f64,
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
            [w[0], w[1], self.thickness - p[1]]
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
    Block(Block),
    Stone(Box<fieldstone::Stone>),
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
                t >= -1e-9 && t <= g.len + 1e-9 && q >= -1e-9 && q <= depth + 1e-9
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
    let (wall, counts, fp, top) = run(m, scene, ctx, spine, kind, &|_| true)?;
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
            owner: ctx.path.to_owned(),
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
pub(crate) type Built = (Solid, Counts, Mask, f64);

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
) -> Result<Built, LookError> {
    let frame = *scene.frame();
    let s = frame.scale();
    let tile = frame.size_mm();
    let segs = layout::segments(spine).map_err(LookError::build)?;
    let thickness = m.thickness * s;
    let seed = ctx.seed;

    // ---- seat (standing) or the strip (laid)
    let (seat, base, height, fp) = if m.laid {
        let h = match m.height {
            Height::Rel(h) | Height::Top(h) => h,
        };
        let fp = footprint(scene, &segs, h);
        (0.0, 0.0, h, fp)
    } else {
        let fp = footprint(scene, &segs, thickness);
        let mine: Vec<usize> = (0..fp.bits.len())
            .filter(|&k| fp.bits[k] && ctx.area.bits[k])
            .collect();
        if mine.is_empty() {
            return Err(LookError::general(
                format!(
                    "this {kind}'s footprint misses its area: it stands outside its in= region"
                ),
                "put the wall along an edge its region touches, or drop in=",
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
        thickness,
        laid: m.laid,
        tile,
        scale: s,
    };

    // ---- cells
    let mut lrng = Rng::new(seed.key("layout"));
    let courses = Courses {
        thickness,
        height,
        course: (m.course.0 * s, m.course.1 * s),
        bay: (m.bay.0 * s, m.bay.1 * s),
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
        let (c, e) = layout::ruin_cells(cells, &run.segs, thickness, height, m.ruin, s, &mut lrng);
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
    let end_in = rv.min(0.5 * layout::MIN_BAY_FRAC * m.bay.0 * s);
    let low_course = cells
        .iter()
        .map(|c| c.z1 - c.z0)
        .fold(f64::INFINITY, f64::min);
    let top_in = rv.min(0.5 * low_course);
    let round = CORE_ROUND_MM * s;
    if m.family != Family::Fieldstone && !m.laid {
        for (k, g) in run.segs.iter().enumerate() {
            let mut core = |t0: f64, t1: f64, z0: f64, z1: f64| -> Result<(), LookError> {
                if t1 - t0 > 2.0 * round && z1 - z0 > 2.0 * round {
                    let mesh = rubble::rounded_box([t0, rv, z0], [t1, thickness - rv, z1], round)
                        .map_err(LookError::build)?;
                    units.push(Unit::Mesh(run.place(k, mesh)));
                }
                Ok(())
            };
            if let Some(e) = &env {
                core(end_in, g.len - end_in, base, e[k].min(g.len) - top_in)?;
            } else if let Some(b) = &crenels {
                core(end_in, g.len - end_in, base, b.floor_z - top_in)?;
                for &(m0, m1) in &b.merlons[k] {
                    core(
                        (m0 + end_in).max(end_in),
                        (m1 - end_in).min(g.len - end_in),
                        b.floor_z - 2.0 * s,
                        height - top_in,
                    )?;
                }
            } else {
                core(end_in, g.len - end_in, base, height - top_in)?;
            }
        }
    }
    let finish = Finish {
        chip: m.chip * s,
        rounding: m.rounding.0 * s,
        relief: m.relief.unwrap_or(0.0) * s,
        settle: 0.2 * s,
        yaw_deg: if m.laid { 0.0 } else { m.yaw_deg },
        tilt_deg: m.tilt_deg,
        scale: s,
    };
    if let Some(f) = &field {
        let mut stones: Vec<fieldstone::Stone> = cells
            .iter()
            .filter_map(|c| fieldstone::stone(m, &run, f, c, seed))
            .collect();
        if !run.laid {
            fieldstone::hold_proud(&mut stones);
            for (k, pin) in fieldstone::joint_pins(&stones, s).map_err(LookError::build)? {
                units.push(Unit::Mesh(run.place(k, pin)));
            }
        }
        counts.units = stones.len();
        units.extend(stones.into_iter().map(|st| Unit::Stone(Box::new(st))));
    } else {
        let (blocks, extra, missing) = blocks(m, &run, &cells, &finish, seed)?;
        counts.units = blocks.len();
        counts.missing = missing;
        units.extend(blocks.into_iter().map(Unit::Block));
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
    let solid = solidify(scene, &units, &run, m.laid, ctx.edge_mm)?;
    // crumbs the union leaves (a sliver where a tiny closer meets the
    // core's rounding) would be dropped by the gate as lost parts
    let (solid, _) = crate::tree::drop_crumbs(scene, solid, Material::Masonry, true)?;
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
    let top = if m.laid { thickness } else { seat + height };
    Ok((solid, counts, fp, top))
}

/// Every unit a leaf solid (slots reserved in sequence, built in
/// parallel, gathered in index order), unioned and cut to the tile.
pub(crate) fn solidify(
    scene: &mut Scene,
    units: &[Unit],
    run: &Run,
    laid: bool,
    edge: Option<f64>,
) -> Result<Solid, LookError> {
    let n = u32::try_from(units.len()).map_err(LookError::build)?;
    let slots = scene
        .slots(n + 1, Material::Masonry)
        .map_err(LookError::build)?;
    type Piece = Result<(Solid, [f64; 3], [f64; 3]), LookError>;
    let ticks = scene.progress().counter("stones", units.len(), 0.0, 0.9);
    let built: Vec<Piece> = units
        .par_iter()
        .enumerate()
        .map(|(i, u)| {
            ticks.tick();
            let mesh = match u {
                Unit::Block(b) => b.mesh(&run.segs[b.seg]).map_err(LookError::build)?,
                Unit::Stone(st) => run.place(st.seg, st.mesh().map_err(LookError::build)?),
                Unit::Mesh(mm) => mm.clone(),
            };
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
            ))
        })
        .collect();
    let mut solids = Vec::with_capacity(built.len());
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for b in built {
        let (sd, l, h) = b?;
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
    let z_lo = if laid {
        edge.unwrap_or(0.0)
    } else {
        lo[2] - 10.0
    };
    clip(
        &all,
        slots.get(n),
        [0.0, 0.0, z_lo],
        [run.tile[0], run.tile[1], hi[2] + 10.0],
    )
    .map_err(LookError::build)
}

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
) -> Result<(Vec<Block>, Vec<TriMesh>, usize), LookError> {
    let s = finish.scale;
    // the joint scales with everything else
    let jw = m.joint * s;
    let j2 = jw / 2.0;
    let fr = m.face_recess * s;
    let t = run.thickness;
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
            let n = run.segs.len();
            let reach = fr + 0.1 * s;
            if c.seg > 0 && !face0 && (c.t0 - t).abs() < 1e-6 {
                x0 -= reach;
            }
            if c.seg + 1 < n && !face1 && (c.t1 - (run.segs[c.seg].len - t)).abs() < 1e-6 {
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
            (
                ([x0, x1], y, [-1.0, t - q0]),
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
    Ok((blocks, extra, n_missing))
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
    let (t, h) = (run.thickness, run.height);
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
    let t = run.thickness;
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
