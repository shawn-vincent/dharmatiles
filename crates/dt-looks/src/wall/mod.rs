//! `wall`: masonry walls on a tile edge, and the masonry chassis floors
//! share. Port of `src/dharmatiles/walls/` (`masonry.py`, `brick.py`,
//! `fieldstone.py`; openings are not ported: there is no vocabulary for
//! them yet).
//!
//! A wall is a spine (`along=`, the outer face, the body to its left)
//! split into segments; courses × bays × quoin alternation make cells
//! ([`layout`]); battlements and ruin edit the cells; every cell becomes
//! one unit: a [`block`] (cut stone, brick, regular brick, floor slab) or
//! a [`fieldstone`] stone. Mortared families stand on a recessed core
//! (the joints are reveals onto it); drystone has no core, rough
//! [`rubble`] hearting fills it. Every unit is its own leaf solid; the
//! wall is their union, cut to the tile (butt joins), tagged `wall`, and
//! its footprint stamped so what grows after it steers round it.
//!
//! Seed and region (the plan's bug fix): the wall follows its entry's
//! seed, and seats over its footprint within its area; a wall whose
//! footprint misses its area is refused.
//!
//! Scale (D2): every author size scales with the square; the joint width
//! and the most a stone may stand proud of the one under it (0.45 mm) are
//! printability floors and do not.

pub(crate) mod block;
pub(crate) mod fieldstone;
pub(crate) mod layout;
pub(crate) mod rubble;

use dt_kit::form::{clip, solid};
use dt_kit::{Mask, Material, Rng, Scene, Seed, Solid, Tag, TriMesh, union};
use rayon::prelude::*;

use crate::{Control, Ctx, Look, LookError, Plan, Read, Value, Write, only};
use block::{Block, Finish, MAX_PROUD_MM};
use layout::{Bond, Cell, Courses, End, Envelope, Seg};

/// The controls `wall` takes (dt-doc's vocabulary, sub-controls as
/// `group/sub`).
pub const CONTROLS: &[&str] = &[
    "style",
    "height",
    "thickness",
    "stones/height",
    "stones/length",
    "stones/texture",
    "joints/width",
    "joints/depth",
    "edges/rounding",
    "coping",
    "bond",
    "ruin",
    "top",
];

/// The official wall top above the datum, mm at the reference.
const WALL_TOP_MM: f64 = 33.1;
/// `low`: half the wall top (provisional).
const LOW_TOP_MM: f64 = 16.55;
/// Seat over the footprint: this percentile of the surface.
const SEAT_PERCENTILE: f64 = 80.0;
/// Mortar core edge fillet, mm.
const CORE_ROUND_MM: f64 = 0.6;
/// Rubble hearting (E10/E27): spacing, chip foot and height, setback.
const RUBBLE_SPACING_MM: f64 = 2.8;
const RUBBLE_FOOT: (f64, f64) = (3.5, 5.5);
const RUBBLE_H: (f64, f64) = (3.0, 4.5);
const RUBBLE_SETBACK_MM: f64 = 1.3;
/// Shed rubble at a ruin's foot.
const FOOT_RUBBLE_EVERY_MM: f64 = 5.5;
const FOOT_RUBBLE_OFF: (f64, f64) = (0.8, 6.5);
const FOOT_RUBBLE_FOOT: (f64, f64) = (2.2, 4.8);
const FOOT_RUBBLE_H: (f64, f64) = (1.8, 3.4);
/// Battlements: merlon and crenel widths, crenel depth.
const MERLON_MM: (f64, f64) = (8.0, 11.0);
const CRENEL_MM: (f64, f64) = (5.5, 7.5);
const CRENEL_DEPTH_MM: f64 = 11.0;
/// The printer's gap (a joint thinner than this closes in print and
/// needs no pointing), mm.
const GAP_MM: f64 = 0.2;

/// The texture presets (`masonry.py:_TEXTURES`): chip, rounding, relief.
pub(crate) fn texture(word: &str) -> Option<(f64, f64, f64)> {
    match word {
        "chipped" => Some((0.55, 0.22, 0.07)),
        "worn" => Some((0.30, 0.60, 0.10)),
        "hewn" => Some((0.80, 0.15, 0.16)),
        "dressed" => Some((0.15, 0.12, 0.03)),
        _ => None,
    }
}

/// The masonry families.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    CutStone,
    Brick,
    RegularBrick,
    Fieldstone,
    /// A floor slab (`StoneFloor`).
    Slab,
}

/// Where the wall's top goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Height {
    /// The top at this level above the datum (mm at the reference).
    Top(f64),
    /// This height above the seat (mm at the reference); laid flat, the
    /// strip's depth in plan (true mm).
    Rel(f64),
}

/// A masonry build, in author mm at the reference (scaled when built).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mason {
    pub family: Family,
    pub laid: bool,
    pub thickness: f64,
    pub height: Height,
    pub course: (f64, f64),
    pub bay: (f64, f64),
    pub joint: f64,
    pub reveal: f64,
    pub min_bond: f64,
    pub chip: f64,
    pub rounding: (f64, f64),
    /// Relief mm (`None`: fieldstone's auto amplitude).
    pub relief: Option<f64>,
    pub face_recess: f64,
    pub yaw_deg: f64,
    pub tilt_deg: f64,
    pub bond: Bond,
    pub ruin: f64,
    pub battlements: bool,
    pub coping_on_edge: bool,
    pub missing: f64,
    pub spall: f64,
    pub crack: f64,
    pub embed: f64,
}

impl Mason {
    /// A family's defaults (the Python class defaults).
    pub fn new(family: Family) -> Self {
        let base = Self {
            family,
            laid: false,
            thickness: 7.0,
            height: Height::Top(WALL_TOP_MM),
            course: (5.5, 8.5),
            bay: (11.0, 20.0),
            joint: 0.11,
            reveal: 1.3,
            min_bond: 3.0,
            chip: 0.55,
            rounding: (0.22, 0.22),
            relief: Some(0.07),
            face_recess: 0.60,
            yaw_deg: 1.2,
            tilt_deg: 0.7,
            bond: Bond::Irregular,
            ruin: 0.0,
            battlements: false,
            coping_on_edge: false,
            missing: 0.0,
            spall: 0.0,
            crack: 0.0,
            embed: 2.5,
        };
        match family {
            Family::CutStone | Family::Slab => base,
            Family::Brick => Self {
                course: (2.5, 3.0),
                bay: (5.0, 6.6),
                joint: 0.125,
                reveal: 1.0,
                min_bond: 2.0,
                chip: 0.25,
                rounding: (0.35, 0.35),
                relief: Some(0.08),
                missing: 0.04,
                spall: 0.10,
                ..base
            },
            Family::RegularBrick => {
                let (chip, r, rel) = texture("dressed").unwrap_or_default();
                Self {
                    course: (3.2, 3.2),
                    bay: (8.5, 8.5),
                    joint: 0.45,
                    reveal: 1.0,
                    bond: Bond::Running,
                    chip,
                    rounding: (r, r),
                    relief: Some(rel),
                    face_recess: 0.12,
                    yaw_deg: 0.25,
                    tilt_deg: 0.15,
                    ..base
                }
            }
            Family::Fieldstone => Self {
                course: (2.2, 5.2),
                bay: (5.0, 16.0),
                joint: 0.0,
                reveal: 2.8,
                min_bond: 1.8,
                rounding: (1.3, 2.6),
                relief: None,
                ..base
            },
        }
    }

    /// Set a texture preset's chip, rounding and relief.
    pub fn texture(&mut self, (chip, r, rel): (f64, f64, f64)) {
        self.chip = chip;
        self.rounding = (r, r);
        self.relief = Some(rel);
    }
}

/// The `wall` look.
#[derive(Clone, Debug, PartialEq)]
pub struct Wall {
    mason: Mason,
}

pub(crate) fn number(c: &Control, lo: f64, hi: f64, what: &str) -> Result<f64, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() && (lo..=hi).contains(&v) => Ok(v),
        ref v => Err(LookError::control(
            &c.name,
            format!("{what} must be {lo} to {hi}, not {v}"),
            format!("write {} as a number, such as {}", c.name, 0.5 * (lo + hi)),
        )),
    }
}

fn span(c: &Control, lo: f64, hi: f64, what: &str) -> Result<(f64, f64), LookError> {
    match c.value {
        Value::Range(a, b) if lo <= a && a <= b && b <= hi => Ok((a, b)),
        Value::Number(_) => number(c, lo, hi, what).map(|v| (v, v)),
        ref v => Err(LookError::control(
            &c.name,
            format!("{what} must lie in {lo} to {hi} mm, not {v}"),
            format!(
                "write a size or a range in mm, such as {}=\"{lo}..{hi}\"",
                c.name
            ),
        )),
    }
}

pub(crate) fn word<'a>(c: &'a Control, words: &[&str]) -> Result<&'a str, LookError> {
    match &c.value {
        Value::Word(w) if words.contains(&w.as_str()) => Ok(w),
        v => Err(LookError::control(
            &c.name,
            format!("`{}` is not one of the choices: {v}", c.name),
            format!("write one of {}", words.join(", ")),
        )),
    }
}

impl Wall {
    /// From its controls.
    ///
    /// # Errors
    /// [`LookError`] for a control it does not take or a value out of
    /// range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("wall", controls, CONTROLS)?;
        let get = |n: &str| controls.iter().find(|c| c.name == n);
        let family = match get("style") {
            Some(c) => match word(c, &["cut-stone", "fieldstone", "brick", "regular-brick"])? {
                "fieldstone" => Family::Fieldstone,
                "brick" => Family::Brick,
                "regular-brick" => Family::RegularBrick,
                _ => Family::CutStone,
            },
            None => Family::CutStone,
        };
        let mut m = Mason::new(family);
        for c in controls {
            match c.name.as_str() {
                "height" => {
                    m.height = match &c.value {
                        Value::Word(w) if w == "full" => Height::Top(WALL_TOP_MM),
                        Value::Word(w) if w == "low" => Height::Top(LOW_TOP_MM),
                        _ => Height::Rel(number(c, 6.0, 200.0, "wall height (mm)")?),
                    }
                }
                "thickness" => m.thickness = number(c, 2.0, 40.0, "wall thickness (mm)")?,
                "stones/height" => m.course = span(c, 1.0, 40.0, "course height")?,
                "stones/length" => m.bay = span(c, 2.0, 80.0, "stone length")?,
                "stones/texture" => {
                    let w = word(c, &["chipped", "worn", "hewn", "dressed"])?;
                    if let Some(t) = texture(w) {
                        if family == Family::Fieldstone {
                            // fieldstone keeps its pebble roundings; the
                            // preset sets the relief only
                            m.relief = Some(t.2);
                        } else {
                            m.texture(t);
                        }
                    }
                }
                "joints/width" => m.joint = number(c, 0.0, 3.0, "joint width (mm)")?,
                "joints/depth" => m.reveal = number(c, 0.0, 5.0, "joint depth (mm)")?,
                "edges/rounding" => m.rounding = span(c, 0.0, 5.0, "rounding")?,
                "coping" => m.coping_on_edge = word(c, &["loose", "on-edge"])? == "on-edge",
                "bond" => {
                    m.bond = if word(c, &["irregular", "running"])? == "running" {
                        Bond::Running
                    } else {
                        Bond::Irregular
                    }
                }
                "ruin" => {
                    let v = match c.value {
                        Value::Number(v) if v > 1.0 => v / 100.0,
                        Value::Number(v) => v,
                        _ => -1.0,
                    };
                    if !(0.0..=1.0).contains(&v) {
                        return Err(LookError::control(
                            "ruin",
                            format!("ruin must be 0 to 100%, not {}", c.value),
                            "write how much has fallen, such as ruin=\"40%\"".into(),
                        ));
                    }
                    m.ruin = v;
                }
                "top" => m.battlements = word(c, &["flat", "battlements"])? == "battlements",
                _ => {}
            }
        }
        if m.bond == Bond::Running && family != Family::RegularBrick {
            // a running bond on any family: uniform courses of the mean
            let mid = 0.5 * (m.course.0 + m.course.1);
            m.course = (mid, mid);
        }
        Ok(Self { mason: m })
    }
}

impl Look for Wall {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid("wall"),
            reads: vec![Read::Solids("ground"), Read::IfAny("floor")],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let Some(along) = ctx.along else {
            return Err(LookError {
                control: None,
                message: "a wall needs along= the tile edges it stands on".into(),
                help: Some("write along=\"north\" or along=\"north west\"".into()),
            });
        };
        let sq = scene.frame().square_mm;
        let spine: Vec<[f64; 2]> = along.iter().map(|p| [p[0] * sq, p[1] * sq]).collect();
        build(&self.mason, scene, ctx, &spine, "wall")
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

/// Build a masonry run into the scene: a wall (`kind` "wall") or a floor.
pub(crate) fn build(
    m: &Mason,
    scene: &mut Scene,
    ctx: &Ctx<'_>,
    spine: &[[f64; 2]],
    kind: &'static str,
) -> Result<(), LookError> {
    let (wall, counts, fp, top) = run(m, scene, ctx, spine, kind, &|_| true)?;
    finish_build(scene, ctx, kind, wall, counts, &fp, top);
    Ok(())
}

/// Add a built run to the scene, stamp it and note its counts.
pub(crate) fn finish_build(
    scene: &mut Scene,
    ctx: &Ctx<'_>,
    kind: &'static str,
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
    kind: &str,
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
            return Err(LookError {
                control: None,
                message: format!(
                    "this {kind}'s footprint misses its area: it stands outside its in= region"
                ),
                help: Some("put the wall along an edge its region touches, or drop in=".into()),
            });
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
        let (c, e) = layout::ruin_cells(cells, &run.segs, height, m.ruin, s, &mut lrng);
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
                core(rv, g.len - rv, base, e[k].min(g.len) - rv)?;
            } else if let Some(b) = &crenels {
                core(rv, g.len - rv, base, b.floor_z - rv)?;
                for &(m0, m1) in &b.merlons[k] {
                    core(
                        (m0 + rv).max(rv),
                        (m1 - rv).min(g.len - rv),
                        b.floor_z - 2.0 * s,
                        height - rv,
                    )?;
                }
            } else {
                core(rv, g.len - rv, base, height - rv)?;
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
    let solid = solidify(scene, &units, &run, m.laid)?;
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
) -> Result<Solid, LookError> {
    let n = u32::try_from(units.len()).map_err(LookError::build)?;
    let slots = scene
        .slots(n + 1, Material::Masonry)
        .map_err(LookError::build)?;
    type Piece = Result<(Solid, [f64; 3], [f64; 3]), LookError>;
    let built: Vec<Piece> = units
        .par_iter()
        .enumerate()
        .map(|(i, u)| {
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
    // the datum (the connector owns z < 0)
    let z_lo = if laid { 0.0 } else { lo[2] - 10.0 };
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
    let j2 = m.joint / 2.0;
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
        let x0 = c.t0 + if face0 { rng.uniform(0.0, fr) } else { j2 };
        let x1 = c.t1 - if face1 { rng.uniform(0.0, fr) } else { j2 };
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
        if m.joint > GAP_MM {
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
                let mesh = rubble::boxed([t0, g, z - m.joint - 0.02], [t1, t - g, z + 0.02])
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
    let sb = RUBBLE_SETBACK_MM * s;
    let sp = RUBBLE_SPACING_MM * s;
    let bands = [(sb, 0.55 * t), (0.45 * t, t - sb)];
    let _ = m;
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn c(name: &str, v: Value) -> Control {
        Control {
            name: name.into(),
            value: v,
        }
    }

    #[test]
    fn controls_set_the_family_and_its_defaults() {
        let w = Wall::new(&[c("style", Value::Word("brick".into()))]).unwrap();
        assert_eq!(w.mason.family, Family::Brick);
        assert!((w.mason.course.0 - 2.5).abs() < 1e-12);
        let w = Wall::new(&[
            c("style", Value::Word("cut-stone".into())),
            c("stones/texture", Value::Word("hewn".into())),
            c("ruin", Value::Number(40.0)),
        ])
        .unwrap();
        assert!((w.mason.chip - 0.80).abs() < 1e-12);
        assert!((w.mason.ruin - 0.4).abs() < 1e-12);
        assert!(Wall::new(&[c("bond", Value::Word("herringbone".into()))]).is_err());
    }

    #[test]
    fn percentiles_interpolate() {
        assert!((percentile(vec![1.0, 2.0, 3.0, 4.0, 5.0], 80.0) - 4.2).abs() < 1e-12);
    }
}
