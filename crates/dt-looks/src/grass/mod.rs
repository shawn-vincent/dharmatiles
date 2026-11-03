//! Grass: grasslab v3's stroke blades (grasslab/grasslab.py), standing on
//! the ground a tile has painted, with meadowlab's rule for what they
//! meet (meadowlab/meadowlab.py): no blade roots under a rock, a trunk or
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
use dt_kit::{Field, Material, Rng, Scene, Tag, TriMesh, distance, form};
use rayon::prelude::*;

use crate::{Control, Ctx, Look, LookError, Plan, Read, Value, Write, only};
use flow::Swaths;
use fronds::Surface;
use lat::Lat;

/// The controls `grass` takes.
pub const CONTROLS: &[&str] = &["length", "height", "clearings", "cover", "bare", "flow"];

/// Field resolution of the flow, mm.
const FIELD_RES_MM: f64 = 0.25;
/// Clearings per square the lab draws between (× `clearings / 1.5`).
const LENSES_PER_SQ: (f64, f64) = (1.3, 1.8);
/// Seating ground is the painted ground blurred this much.
const SEAT_BLUR_MM: f64 = 0.8;
/// Ground the flow and clearings read is blurred this much (the dirt's
/// own texture would spike the slope).
const FLOW_BLUR_MM: f64 = 2.0;
/// A rock's outline is read this far over its foot.
const FOOT_MM: f64 = 0.45;
const TUCK_MM: f64 = 0.5;
const SIDE_LOW: f64 = 0.5;
const SIDE_MM: f64 = 1.2;
const SIDE_INNER_MM: f64 = 0.8;

/// The grass look.
#[derive(Clone, Debug)]
pub struct Grass {
    length: (f64, f64),
    height: (f64, f64),
    clearings: f64,
    cover: f64,
    bare: f64,
    meander: (f64, f64),
}

fn range(c: &Control) -> Result<(f64, f64), LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() && v > 0.0 => Ok((v, v)),
        Value::Range(a, b) if a.is_finite() && a > 0.0 && b >= a => Ok((a, b)),
        ref v => Err(LookError {
            control: Some(c.name.clone()),
            message: format!("`{}` is a size in mm over 0, or a range, not {v}", c.name),
            help: Some(format!("write {}=\"3..5\"", c.name)),
        }),
    }
}

impl Grass {
    /// From controls.
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("grass", controls, CONTROLS)?;
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
                "length" => g.length = range(c)?,
                "height" => g.height = range(c)?,
                "clearings" => match c.value {
                    Value::Number(v) if (0.0..=20.0).contains(&v) => g.clearings = v,
                    ref v => {
                        return Err(LookError {
                            control: Some("clearings".into()),
                            message: format!("clearings are 0 to 20 a square, not {v}"),
                            help: Some("write clearings=1.5".into()),
                        });
                    }
                },
                "cover" => match c.value {
                    Value::Number(v) if (0.0..=1.0).contains(&v) => g.cover = v,
                    ref v => {
                        return Err(LookError {
                            control: Some("cover".into()),
                            message: format!("cover is 0% to 100%, not {v}"),
                            help: Some("write cover=\"80%\"".into()),
                        });
                    }
                },
                "bare" => match c.value {
                    Value::Number(v) if (0.0..=0.9).contains(&v) => g.bare = v,
                    ref v => {
                        return Err(LookError {
                            control: Some("bare".into()),
                            message: format!("bare is 0% to 90%, not {v}"),
                            help: Some("write bare=\"20%\"".into()),
                        });
                    }
                },
                "flow.meander-min" => lo = Some(num(c)?),
                "flow.meander-max" => hi = Some(num(c)?),
                _ => {}
            }
        }
        if let (Some(a), Some(b)) = (lo, hi) {
            g.meander = (a, b.max(a));
        }
        Ok(g)
    }
}

fn num(c: &Control) -> Result<f64, LookError> {
    match c.value {
        Value::Number(v) if v.is_finite() => Ok(v),
        ref v => Err(LookError {
            control: Some(c.name.clone()),
            message: format!("`{}` is a number, not {v}", c.name),
            help: None,
        }),
    }
}

fn as_field(g: &dt_kit::Grid) -> Field {
    Field {
        nx: g.nx,
        ny: g.ny,
        step: g.step,
        z: g.z.iter().map(|&v| f64::from(v)).collect(),
    }
}

impl Look for Grass {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid("grass"),
            reads: vec![
                Read::Solids("ground"),
                Read::IfAny("rock"),
                Read::IfAny("tree"),
                Read::IfAny("wall"),
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
            return Err(LookError {
                control: None,
                message: "grass has no ground to grow on".into(),
                help: Some("put a `ground` or `dirt` before it in the stack".into()),
            });
        };
        let raw = as_field(&surface);
        let seat = raw.blur(SEAT_BLUR_MM);
        let flat = raw.blur(FLOW_BLUR_MM * s);
        let seed = ctx.seed;
        let mut rng = Rng::new(seed.key("grass"));

        // 1 the flow, on a lattice that reaches past the tile
        let res = FIELD_RES_MM;
        let lat = flow::lattice(w, h, res);
        let ground: Lat = lat.like(|x, y| flat.sample(x.clamp(0.0, w), y.clamp(0.0, h)));
        let flow0 = flow::make_direction(seed.key("flow"), &mut rng, &ground, s, self.meander);
        let sw = Swaths::new(seed.key("swath"), &mut rng, w, h, &flow0, s);

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
        let blocker = scene.standing(&["rock", "tree", "wall"], FOOT_MM);
        let has_blocker = blocker.bits.iter().any(|&b| b);

        // 2 clearings of bare earth, and the flow round them and the rocks
        let n_sq = (w / frame.square_mm) * (h / frame.square_mm);
        let share = ctx.area.bits.iter().filter(|&&b| b).count() as f64 / (nx * ny) as f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = if self.clearings <= 0.0 {
            0
        } else {
            let per = rng.uniform(LENSES_PER_SQ.0, LENSES_PER_SQ.1) * self.clearings / 1.5;
            ((per * n_sq * share).round() as usize).max(1)
        };
        let (count, pool) = if self.bare > 0.0 {
            // meadowlab: a pool of candidate clearings, added best first
            // until the bare earth reaches its target
            (((POOL_PER_SQ * n_sq * share).round() as usize).max(1), true)
        } else {
            (count, false)
        };
        let mut lenses = flow::place_lenses(
            seed.key("lenses"),
            &mut rng,
            &|x, y| flat.sample(x.clamp(0.0, w), y.clamp(0.0, h)),
            &area_at,
            &flow0,
            w,
            h,
            count,
            s,
        );
        let things = standing_things(scene);
        if pool {
            let aprons = apron_shapes(&mut rng, &flow0, &things);
            lenses = fill_bare_earth(&lenses, &aprons, &blocker, &outside, &frame, self.bare);
        }
        let wrap = obstacle_circles(&things, &blocker, &frame, &flow0);
        let flow = flow::finish_flow(
            seed.key("rim"),
            &mut rng,
            &flow0,
            &lenses.combine(usize::MAX, &wrap),
            w,
            h,
            s,
        );

        // 3 what the fronds read
        let soil = Field::from_fn(&frame, |x, y| lenses.mask(x, y));
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
        let (frs, strands) =
            fronds::plan(&mut rng, &surf, &sw, &flow, (w, h), len, hgt, self.cover);

        // 5 blades, in parallel, gathered in order
        let seat_at = |x: f64, y: f64| seat.sample(x, y);
        let raw_at = |x: f64, y: f64| raw.sample(x, y);
        let meshes: Vec<Option<TriMesh>> = frs
            .par_iter()
            .enumerate()
            .map(|(i, f)| blade::mesh(f, &seat_at, &raw_at, hair(i)))
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
                    kind: "grass",
                    owner: owner.clone(),
                },
                solid,
            );
        }
        #[allow(clippy::cast_precision_loss)]
        let soil_share = surf.soil.z.iter().filter(|&&v| v > 0.5).count() as f64 / (nx * ny) as f64;
        #[allow(clippy::cast_precision_loss)]
        for (k, v) in [
            ("blades", made as f64),
            ("soil_frac", soil_share),
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

/// Each standing thing as a circle the flow wraps (meadowlab's
/// `rock_circles`): centre, a ring a little wider than the thing.
fn obstacle_circles(
    things: &[Thing],
    blocker: &dt_kit::Mask,
    frame: &dt_kit::Frame,
    flow: &flow::Flow,
) -> flow::Shapes {
    const FLOW_RING_MM: f64 = 0.6;
    let mut out = flow::Shapes::default();
    for t in things {
        // walls are long: leave them to the rim rule and the blocker
        if t.kind == "wall" || t.r > 12.0 {
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

/// Something standing on the ground: where, how wide.
struct Thing {
    kind: &'static str,
    at: [f64; 2],
    r: f64,
}

fn standing_things(scene: &Scene) -> Vec<Thing> {
    scene
        .parts()
        .iter()
        .filter(|p| ["rock", "tree", "wall"].contains(&p.tag.kind))
        .filter_map(|p| {
            let (lo, hi) = p.solid.bounds()?;
            Some(Thing {
                kind: p.tag.kind,
                at: [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0],
                r: 0.5 * (hi[0] - lo[0]).max(hi[1] - lo[1]),
            })
        })
        .collect()
}

/// Candidate clearings per square when a bare target asks for them.
const POOL_PER_SQ: f64 = 8.0;
const APRON_PROB: f64 = 0.35;
const APRON_WAKE_PROB: f64 = 0.6;
const APRON_HALF_W: (f64, f64) = (0.5, 0.9);
const APRON_STRETCH: (f64, f64) = (1.2, 1.9);
const APRON_OFFSET: (f64, f64) = (0.6, 1.0);

/// One-sided bare-earth patches (wake or scour) for a minority of rocks
/// (meadowlab `apron_shapes`).
fn apron_shapes(rng: &mut Rng, flow: &flow::Flow, things: &[Thing]) -> flow::Shapes {
    let mut out = flow::Shapes::default();
    for t in things.iter().filter(|t| t.kind == "rock") {
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
/// area; all of them if none does (meadowlab `fill_bare_earth`).
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
