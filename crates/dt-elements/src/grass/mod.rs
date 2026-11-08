//! Grass: stroke blades, standing on
//! the ground a tile has painted, with a rule for what they
//! meet: no blade roots under a rock, a trunk or
//! a wall, blades press against them and stand lower beside them, and the
//! flow wraps them like the clearings of bare earth.
//!
//! The ground is whatever the stack painted (`ground`, `dirt`): grass adds
//! only blades, so the dirt shows between them and in the clearings.
//! Rocks, trees and walls are built first (the plan reads them), so their
//! footprints are known before a blade is grown: plan, then build.
//!
//! A blade is a printed thing: its layers (0.2 mm) and lines (0.45 mm) are
//! true mm and do not scale with the system's square; the author's
//! `length` and `height` do, and so do the flow's meanders and swaths.
//!
//! Blades are meshed in parallel and gathered in index order. They overlap
//! like shingles, so they are grouped into solids of mutually clear blades
//! (a slot each), and the union takes it from there.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::needless_range_loop,
    clippy::type_complexity,
    clippy::too_many_lines
)]

mod blade;
mod flow;
mod fronds;
mod lat;

pub(crate) use blade::ring_mesh;

/// How far inside the tile's plane a blade that reached it is kept, mm.
const PLANE_CLEAR_MM: f64 = 1e-3;

/// No blade crosses the tile's vertical plane (the rule the side relief
/// follows, so tiles never jam in clips or on a board): a blade whose width
/// or lean reaches past it is moved in until it just clears it.
fn inside_plane(mut m: TriMesh, w: f64, h: f64) -> TriMesh {
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in &m.positions {
        for a in 0..2 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let size = [w, h];
    let shift: [f64; 2] = std::array::from_fn(|a| {
        if lo[a] < PLANE_CLEAR_MM {
            PLANE_CLEAR_MM - lo[a]
        } else if hi[a] > size[a] - PLANE_CLEAR_MM {
            size[a] - PLANE_CLEAR_MM - hi[a]
        } else {
            0.0
        }
    });
    if shift != [0.0; 2] {
        for p in &mut m.positions {
            p[0] += shift[0];
            p[1] += shift[1];
        }
    }
    m
}
use dt_kit::Kind;
use dt_kit::OnFrame;
use dt_kit::{Field, Material, Rng, Scene, Standing, Tag, TriMesh, distance, form};
use rayon::prelude::*;

use crate::read;
use crate::{Control, Ctx, Element, LookError, Plan, Read, Write};
use flow::Swaths;
use fronds::Surface;
use lat::Lat;

/// Field resolution of the flow, mm.
const FIELD_RES_MM: f64 = 0.25;
/// Clearings per square drawn between (× `clearings / 1.5`).
const LENSES_PER_SQ: (f64, f64) = (1.3, 1.8);
/// Seating ground is the painted ground blurred this much.
const SEAT_BLUR_MM: f64 = 0.8;
/// Blades seat on the ground's low envelope: blades stand on the
/// smooth hills under the dirt's texture, not on its crests.
const SEAT_ERODE_MM: f64 = 1.5;
const MUD_MAX_MM: f64 = 4.0;
const MUD_SIGMA_MM: f64 = 9.0;
/// Ground the flow and clearings read is blurred this much (the dirt's
/// own texture would spike the slope).
const FLOW_BLUR_MM: f64 = 2.0;
/// A rock's outline is read this far over its foot.
const FOOT_MM: f64 = 0.45;
const TUCK_MM: f64 = 0.5;
const SIDE_LOW: f64 = 0.5;
const SIDE_MM: f64 = 1.2;
const SIDE_INNER_MM: f64 = 0.8;

/// The grass element.
#[derive(Clone, Debug)]
pub struct Grass {
    length: (f64, f64),
    height: (f64, f64),
    clearings: f64,
    cover: f64,
    bare: f64,
    meander: (f64, f64),
}

impl Grass {
    /// From controls.
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        let mut g = Self {
            length: (3.5, 6.5),
            height: (1.5, 2.0),
            clearings: 1.5,
            cover: 1.0,
            bare: 0.0,
            meander: (10.0, 30.0),
        };
        let (mut lo, mut hi) = (None, None);
        for c in controls {
            match c.name.as_str() {
                "length" => g.length = read::span(c)?,
                "height" => g.height = read::span(c)?,
                "clearings" => g.clearings = read::number(c)?,
                "cover" => g.cover = read::number(c)?,
                "bare" => g.bare = read::number(c)?,
                "flow.meander-min" => lo = Some(read::number(c)?),
                "flow.meander-max" => hi = Some(read::number(c)?),
                _ => {}
            }
        }
        if let (Some(a), Some(b)) = (lo, hi) {
            g.meander = (a, b.max(a));
        }
        Ok(g)
    }
}

impl Element for Grass {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Grass),
            reads: vec![
                Read::Solids(Kind::Terrain),
                Read::IfAny(Kind::Rock),
                Read::IfAny(Kind::Tree),
                Read::IfAny(Kind::Wall),
                Read::IfAny(Kind::Flowers),
            ],
            cell_mm: None,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let frame = *scene.frame();
        let s = frame.scale();
        let [w, h] = frame.size_mm();
        let (nx, ny) = frame.nodes();
        let Some((surface, _)) = scene.surface() else {
            return Err(LookError::general(
                "grass has no ground to grow on",
                "put a `ground` or `dirt` before it in the stack",
            ));
        };
        let raw = Field::from(&surface);
        let seat = raw.erode(SEAT_ERODE_MM * s).blur(SEAT_BLUR_MM);
        let flat = raw.blur(FLOW_BLUR_MM * s);
        let seed = ctx.seed;
        // each stage draws from its own stream, so a draw added to one
        // never moves another
        let draws = |stage: &str| Rng::new(seed.key(stage).key("draws"));

        // 1 the flow, on a lattice that reaches past the tile
        let res = FIELD_RES_MM;
        let lat = flow::lattice(w, h, res);
        let ground: Lat = lat.like(|x, y| flat.sample(x.clamp(0.0, w), y.clamp(0.0, h)));
        let flow0 = flow::make_direction(
            seed.key("flow"),
            &mut draws("flow"),
            &ground,
            s,
            self.meander,
        );
        let sw = Swaths::new(seed.key("swath"), &mut draws("swath"), w, h, &flow0, s);

        // where grass may stand: its area, not under water
        let wet = scene.wet().clone();
        let outside: Vec<bool> = (0..nx * ny)
            .map(|k| !ctx.area.bits[k] || wet.bits[k])
            .collect();
        let area_at = |x: f64, y: f64| {
            let (i, j) = raw_node(&frame, x, y);
            !outside[j * nx + i]
        };
        // what stands on the ground: grass roots nowhere under it
        let mut blocker = scene.standing(&[Kind::Rock, Kind::Tree, Kind::Wall], FOOT_MM);
        // a flower's footprint at its dome, over the column
        let fl = scene.standing(&[Kind::Flowers], 1.0);
        blocker
            .bits
            .iter_mut()
            .zip(&fl.bits)
            .for_each(|(b, f)| *b |= *f);
        let has_blocker = blocker.bits.iter().any(|&b| b);

        // 2 clearings of bare earth, and the flow round them and the rocks
        let n_sq = (w / frame.square_mm) * (h / frame.square_mm);
        let share = ctx.area.share();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = if self.clearings <= 0.0 {
            0
        } else {
            let per =
                draws("clearings").uniform(LENSES_PER_SQ.0, LENSES_PER_SQ.1) * self.clearings / 1.5;
            ((per * n_sq * share).round() as usize).max(1)
        };
        let (count, pool) = if self.bare > 0.0 {
            // a pool of candidate clearings, added best first
            // until the bare earth reaches its target
            (((POOL_PER_SQ * n_sq * share).round() as usize).max(1), true)
        } else {
            (count, false)
        };
        let mut lenses = flow::place_lenses(
            seed.key("lenses"),
            &mut draws("lenses"),
            &|x, y| flat.sample(x.clamp(0.0, w), y.clamp(0.0, h)),
            &area_at,
            &flow0,
            w,
            h,
            count,
            s,
        );
        let things = scene.stands(&[Kind::Rock, Kind::Tree, Kind::Wall]);
        if pool {
            let aprons = apron_shapes(&mut draws("aprons"), &flow0, &things);
            lenses = fill_bare_earth(&lenses, &aprons, &blocker, &outside, &frame, self.bare);
        }
        let wrap = obstacle_circles(&things, &blocker, &frame, &flow0, self.length.0 * s);
        let flow = flow::finish_flow(
            seed.key("rim"),
            &mut draws("rim"),
            &flow0,
            &lenses.combine(usize::MAX, &wrap),
            w,
            h,
            s,
        );

        // grass near the water leans toward it
        let water_dist = (wet.count() > 0).then(|| distance(&frame, &wet.bits).0);
        let flow = if let Some(dist) = &water_dist {
            let to_water = lat.like(|x, y| dist.sample(x.clamp(0.0, w), y.clamp(0.0, h)));
            flow::steer_shore(&flow, &to_water, s)
        } else {
            flow
        };

        // 3 what the fronds read
        // a strip of wet mud along the shore, its width wandering 0 to
        // 4 mm: bare earth where the grass stops
        let mud = water_dist.as_ref().map(|dist| {
            let n = lat.noise(seed.key("mud"), MUD_SIGMA_MM * s);
            Field::from_fn(&frame, |x, y| {
                let width = MUD_MAX_MM * s * (n.sample(x, y) * 0.5).clamp(0.0, 1.0).powf(0.8);
                let (i, j) = dist.node(x, y);
                let d = dist.z[j * dist.nx + i];
                if d > 0.0 && width > 0.2 * s {
                    lat::s01(0.5 + (width - d) / flow::SOIL_EDGE_MM)
                } else {
                    0.0
                }
            })
        });
        let soil = Field::from_fn(&frame, |x, y| {
            let m = mud.as_ref().map_or(0.0, |m| {
                let (i, j) = m.node(x, y);
                m.z[j * m.nx + i]
            });
            lenses.mask(x, y).max(m)
        });
        let bare: Vec<bool> = soil.z.iter().map(|v| *v <= 0.5).collect();
        let on_soil: Vec<bool> = bare.iter().map(|b| !b).collect();
        let soil_depth = if on_soil.iter().any(|&b| b) {
            distance(&frame, &bare).0
        } else {
            Field::new(&frame, 0.0)
        };
        let soil_dist = if on_soil.iter().any(|&b| b) {
            distance(&frame, &on_soil).0
        } else {
            Field::new(&frame, 1e3)
        };
        let (blk_depth, near) = if has_blocker {
            let inside = distance(&frame, &blocker.bits.iter().map(|b| !b).collect::<Vec<_>>()).0;
            let outer = distance(&frame, &blocker.bits).0;
            let n = outer.zip(&inside, |o, i| {
                (SIDE_LOW + (1.0 - SIDE_LOW) * lat::s01(o / SIDE_MM))
                    * (1.0 - lat::s01(i / SIDE_INNER_MM))
            });
            (Some(inside), Some(n))
        } else {
            (None, None)
        };
        let surf = Surface {
            soil,
            soil_depth,
            soil_dist,
            blk_depth,
            near,
            outside,
            tuck_mm: TUCK_MM,
        };

        // 4 fronds
        let len = (self.length.0 * s, self.length.1 * s);
        let hgt = (self.height.0 * s, self.height.1 * s);
        let (frs, strands) = fronds::plan(
            &mut draws("fronds"),
            &surf,
            &sw,
            &flow,
            (w, h),
            len,
            hgt,
            self.cover,
        );

        // 5 blades, in parallel, gathered in order
        let seat_at = |x: f64, y: f64| seat.sample(x, y);
        let raw_at = |x: f64, y: f64| raw.sample(x, y);
        let progress = scene.progress();
        progress.fraction("fronds", 0.1);
        let ticks = progress.counter("blades", frs.len(), 0.1, 0.85);
        let meshes: Vec<Option<TriMesh>> = frs
            .par_iter()
            .enumerate()
            .map(|(i, f)| {
                let m = blade::mesh(f, &seat_at, &raw_at, hair(i)).map(|m| inside_plane(m, w, h));
                ticks.tick();
                m
            })
            .collect();

        // 6 solids of mutually clear blades, a slot each
        let boxes: Vec<Option<([f64; 2], [f64; 2])>> =
            meshes.iter().map(|m| m.as_ref().map(plan_box)).collect();
        let classes = colour(&boxes);
        let n_classes = classes.iter().copied().max().map_or(0, |c| c + 1);
        #[allow(clippy::cast_possible_truncation)]
        let slots = scene
            .slots(n_classes as u32, Material::Grass)
            .map_err(LookError::build)?;
        let owner = ctx.path.to_owned();
        let mut made = 0usize;
        for c in 0..n_classes {
            #[allow(clippy::cast_precision_loss)]
            progress.fraction("solids", 0.85 + 0.15 * c as f64 / n_classes as f64);
            let mut merged = TriMesh::default();
            for (m, _) in meshes
                .iter()
                .zip(&classes)
                .filter(|(m, cl)| m.is_some() && **cl == c)
            {
                if let Some(m) = m {
                    append(&mut merged, m);
                    made += 1;
                }
            }
            #[allow(clippy::cast_possible_truncation)]
            let solid = form::solid(slots.get(c as u32), &merged).map_err(LookError::build)?;
            scene.add(
                Tag {
                    kind: Kind::Grass,
                    owner: owner.clone(),
                },
                solid,
            );
        }
        #[allow(clippy::cast_precision_loss)]
        let soil_share = surf.soil.z.iter().filter(|&&v| v > 0.5).count() as f64 / (nx * ny) as f64;
        // bare earth as counted: clearings and what stands on the
        // ground, over the grass's own area
        let in_area: Vec<usize> = (0..nx * ny).filter(|&k| ctx.area.bits[k]).collect();
        let bare_share = in_area
            .iter()
            .filter(|&&k| surf.soil.z[k] > 0.5 || blocker.bits[k])
            .count() as f64
            / in_area.len().max(1) as f64;
        #[allow(clippy::cast_precision_loss)]
        for (k, v) in [
            ("blades", made as f64),
            ("soil_frac", soil_share),
            ("bare_share", bare_share),
            ("strands", strands as f64),
            ("clearings", lenses.centre.len() as f64),
            ("grass_solids", n_classes as f64),
        ] {
            scene.note(k, v);
        }
        Ok(())
    }
}

/// A blade's own hair of offset: golden-ratio sequences in index, so
/// neighbours differ, up to ±50 µm across and ±12 µm high.
fn hair(i: usize) -> [f64; 3] {
    let frac = |x: f64| x - libm::floor(x);
    #[allow(clippy::cast_precision_loss)]
    let k = i as f64 + 1.0;
    [
        (frac(k * 0.618_033_988_749_895) - 0.5) * 0.1,
        (frac(k * 0.754_877_666_246_693) - 0.5) * 0.1,
        (frac(k * 0.569_840_290_998_053) - 0.5) * 0.025,
    ]
}

fn raw_node(frame: &dt_kit::Frame, x: f64, y: f64) -> (usize, usize) {
    let (nx, ny) = frame.nodes();
    let st = frame.step_mm();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    (
        ((x / st).round().max(0.0) as usize).min(nx - 1),
        ((y / st).round().max(0.0) as usize).min(ny - 1),
    )
}

/// Each standing thing at least as wide as the shortest blade, as a circle
/// the flow wraps: centre, a ring a little wider than the thing.
fn obstacle_circles(
    things: &[Standing],
    blocker: &dt_kit::Mask,
    frame: &dt_kit::Frame,
    flow: &flow::Flow,
    shortest: f64,
) -> flow::Shapes {
    const FLOW_RING_MM: f64 = 0.6;
    let mut out = flow::Shapes::default();
    for t in things {
        // walls are long: leave them to the rim rule and the blocker
        if t.kind == Kind::Wall || t.r > 12.0 {
            continue;
        }
        // a pebble or a stone smaller than the shortest blade does not
        // turn the grass: the blades run up to it and tuck against it (an
        // eddy round each of many small stones stalls the blades round it)
        if 2.0 * t.r < shortest {
            continue;
        }
        let (i, j) = raw_node(frame, t.at[0], t.at[1]);
        if !blocker.bits[j * blocker.nx + i] && t.r < 0.5 {
            continue;
        }
        let d = flow.at(t.at[0], t.at[1]);
        let n = libm::hypot(d[0], d[1]).max(1e-12);
        out.centre.push(t.at);
        out.axis.push([d[0] / n, d[1] / n]);
        out.half_len.push(t.r + FLOW_RING_MM);
        out.half_w.push(t.r + FLOW_RING_MM);
    }
    out
}

/// Candidate clearings per square when a bare target asks for them.
const POOL_PER_SQ: f64 = 8.0;
const APRON_PROB: f64 = 0.35;
const APRON_WAKE_PROB: f64 = 0.6;
const APRON_HALF_W: (f64, f64) = (0.5, 0.9);
const APRON_STRETCH: (f64, f64) = (1.2, 1.9);
const APRON_OFFSET: (f64, f64) = (0.6, 1.0);

/// One-sided bare-earth patches (wake or scour) for a minority of rocks.
fn apron_shapes(rng: &mut Rng, flow: &flow::Flow, things: &[Standing]) -> flow::Shapes {
    let mut out = flow::Shapes::default();
    for t in things.iter().filter(|t| t.kind == Kind::Rock) {
        if rng.random() >= APRON_PROB {
            continue;
        }
        let f = flow.at(t.at[0], t.at[1]);
        let n = libm::hypot(f[0], f[1]).max(1e-12);
        let f = [f[0] / n, f[1] / n];
        let half_pi = core::f64::consts::FRAC_PI_2;
        let side = rng.uniform(-half_pi, half_pi)
            + if rng.random() < APRON_WAKE_PROB {
                0.0
            } else {
                core::f64::consts::PI
            };
        let d = [
            libm::cos(side) * f[0] - libm::sin(side) * f[1],
            libm::sin(side) * f[0] + libm::cos(side) * f[1],
        ];
        let bw = t.r * rng.uniform(APRON_HALF_W.0, APRON_HALF_W.1);
        let off = t.r * rng.uniform(APRON_OFFSET.0, APRON_OFFSET.1);
        out.centre
            .push([t.at[0] + d[0] * off, t.at[1] + d[1] * off]);
        out.axis.push(f);
        out.half_w.push(bw);
        out.half_len
            .push(bw * rng.uniform(APRON_STRETCH.0, APRON_STRETCH.1));
    }
    out
}

/// The first `k` free lenses and the aprons, `k` the least that brings the
/// bare earth (clearings and what stands on the ground) to `target` of the
/// area; all of them if none does.
fn fill_bare_earth(
    free: &flow::Lenses,
    aprons: &flow::Shapes,
    blocker: &dt_kit::Mask,
    outside: &[bool],
    frame: &dt_kit::Frame,
    target: f64,
) -> flow::Lenses {
    const STRIDE: usize = 4;
    let (nx, ny) = frame.nodes();
    let st = frame.step_mm();
    let nodes: Vec<(usize, f64, f64)> = (0..ny)
        .step_by(STRIDE)
        .flat_map(|j| (0..nx).step_by(STRIDE).map(move |i| (j * nx + i, i, j)))
        .filter(|(k, _, _)| !outside[*k])
        .map(|(k, i, j)| {
            #[allow(clippy::cast_precision_loss)]
            (k, i as f64 * st, j as f64 * st)
        })
        .collect();
    let mut lens = free.combine(0, aprons);
    for k in 0..=free.centre.len() {
        lens = free.combine(k, aprons);
        #[allow(clippy::cast_precision_loss)]
        let bare = nodes
            .iter()
            .filter(|(i, x, y)| blocker.bits[*i] || lens.mask(*x, *y) > 0.5)
            .count() as f64
            / nodes.len().max(1) as f64;
        if bare >= target {
            break;
        }
    }
    lens
}

/// A blade's footprint box.
fn plan_box(m: &TriMesh) -> ([f64; 2], [f64; 2]) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in &m.positions {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    ([lo[0] - 0.05, lo[1] - 0.05], [hi[0] + 0.05, hi[1] + 0.05])
}

/// First-fit colouring: blades whose footprint boxes overlap get different
/// classes, so each class is a set of disjoint shells (one valid solid).
fn colour(boxes: &[Option<([f64; 2], [f64; 2])>]) -> Vec<usize> {
    const CELL: f64 = 4.0;
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    let mut class = vec![0usize; boxes.len()];
    let cells = |b: &([f64; 2], [f64; 2])| {
        #[allow(clippy::cast_possible_truncation)]
        let (i0, i1) = (
            libm::floor(b.0[0] / CELL) as i64,
            libm::floor(b.1[0] / CELL) as i64,
        );
        #[allow(clippy::cast_possible_truncation)]
        let (j0, j1) = (
            libm::floor(b.0[1] / CELL) as i64,
            libm::floor(b.1[1] / CELL) as i64,
        );
        (i0..=i1).flat_map(move |i| (j0..=j1).map(move |j| (i, j)))
    };
    for (k, b) in boxes.iter().enumerate() {
        let Some(b) = b else { continue };
        let mut taken = std::collections::BTreeSet::new();
        for c in cells(b) {
            for &o in grid.get(&c).into_iter().flatten() {
                let Some(ob) = &boxes[o] else { continue };
                if ob.0[0] <= b.1[0] && b.0[0] <= ob.1[0] && ob.0[1] <= b.1[1] && b.0[1] <= ob.1[1]
                {
                    taken.insert(class[o]);
                }
            }
        }
        class[k] = (0..).find(|c| !taken.contains(c)).unwrap_or(0);
        for c in cells(b) {
            grid.entry(c).or_default().push(k);
        }
    }
    class
}

/// Append `b` to `a` (disjoint shells).
fn append(a: &mut TriMesh, b: &TriMesh) {
    let base = u32::try_from(a.positions.len()).unwrap_or(u32::MAX);
    a.positions.extend_from_slice(&b.positions);
    a.triangles.extend(
        b.triangles
            .iter()
            .map(|t| [t[0] + base, t[1] + base, t[2] + base]),
    );
}
