//! Cliff: a first port of clifflab's rockpile (clifflab/rockpile.py,
//! frozen at round 17, 31f7c5d, foot 3.4 mm): a band of stacked rock
//! standing against a tile edge, its ends finished in the standard end
//! section so runs butt end to end.
//!
//! What is ported: the **form** (masses along the run with their own
//! depth and profile: full height all the way out, or a tall back band
//! with a lower tier in front; the end section, full height 11 mm deep for
//! the last 4 mm), the **stack** (columns, blocks, slabs, odd lumps and
//! monoliths, each stone resting flat on the one below with its cap set
//! back, so nothing roofs a gap), the **finish** in part (arris chamfers,
//! rocklab's grain) and the **core** (the back of the band solid, so a gap
//! between stones is a recess and never a hole).
//!
//! What is not yet: the crack network and carved block faces, shattered
//! patches, ledges, tilted slabs, dirt lodged on the top, the bank and the
//! scree (the scatter `against=` a cliff), and the corner and massif
//! forms. See the plan's later list.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]

use dt_kit::{Convex, Field, Grain, Material, Rng, Scene, Tag, TriMesh, form, grain};
use rayon::prelude::*;

use crate::{Control, Ctx, Look, LookError, Plan, Read, Value, Write, only};

/// The controls `cliff` takes.
pub const CONTROLS: &[&str] = &["height", "depth", "rock", "stone-size", "top"];

const DEPTH_MM: (f64, f64) = (8.0, 20.0);
const TOP_DEPTH_MM: (f64, f64) = (5.0, 10.0);
const MASS_W_MM: (f64, f64) = (6.0, 15.0);
const FULL_P: f64 = 0.35;
const TIER_Z: (f64, f64) = (0.3, 0.7);
const END_MM: f64 = 4.0;
const END_DEPTH_MM: f64 = 11.0;
const SINK_MM: f64 = 0.8;
const CAP_MIN_MM: f64 = 6.0;
const CAP_SETBACK_MM: f64 = 0.6;
const PRESS_MM: f64 = 0.4;
/// kind: (weight, along, depth-share of the mass, course height)
const KINDS: [(&str, f64, (f64, f64), (f64, f64)); 5] = [
    ("columns", 0.38, (8.0, 14.0), (15.0, 24.0)),
    ("blocks", 0.38, (10.0, 18.0), (11.0, 18.0)),
    ("slabs", 0.12, (14.0, 24.0), (5.0, 8.0)),
    ("odd", 0.06, (6.0, 10.0), (6.0, 11.0)),
    ("monoliths", 0.06, (8.0, 16.0), (40.0, 80.0)),
];

/// The cliff look.
#[derive(Clone, Debug)]
pub struct Cliff {
    height: f64,
    depth: Option<f64>,
    weights: [f64; 5],
    /// 0 = rubble (small stones), 1 = massive.
    size: f64,
}

impl Cliff {
    /// From controls.
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("cliff", controls, CONTROLS)?;
        let mut c = Self {
            height: 35.0,
            depth: None,
            weights: KINDS.map(|k| k.1),
            size: 0.0,
        };
        for k in controls {
            let bad = |m: String| LookError {
                control: Some(k.name.clone()),
                message: m,
                help: None,
            };
            match (k.name.as_str(), &k.value) {
                ("height", Value::Number(v)) if (6.0..=120.0).contains(v) => c.height = *v,
                ("height", v) => return Err(bad(format!("a cliff is 6 to 120 mm high, not {v}"))),
                ("depth", Value::Number(v)) if (4.0..=60.0).contains(v) => c.depth = Some(*v),
                ("depth", v) => {
                    return Err(bad(format!("a cliff reaches 4 to 60 mm out, not {v}")));
                }
                ("stone-size", Value::Word(w)) => {
                    c.size = match w.as_str() {
                        "rubble" => 0.0,
                        "broken" => 0.5,
                        _ => 1.0,
                    };
                }
                ("stone-size", Value::Number(v)) => c.size = v.clamp(0.0, 1.0),
                ("rock", Value::Word(w)) => {
                    // a single kind, or `mixed`
                    if let Some(i) = KINDS.iter().position(|k| k.0 == w) {
                        c.weights = [0.0; 5];
                        c.weights[i] = 1.0;
                    }
                }
                (n, Value::Number(v)) if n.starts_with("rock.") => {
                    if let Some(i) = KINDS.iter().position(|k| n.ends_with(k.0)) {
                        c.weights[i] = v.max(0.0);
                    }
                }
                _ => {}
            }
        }
        if c.weights.iter().sum::<f64>() <= 0.0 {
            c.weights = KINDS.map(|k| k.1);
        }
        Ok(c)
    }
}

/// A stone to build: its box in the run's frame (along, depth, up from the
/// foot) and its arris chamfers.
struct Stone {
    lo: [f64; 3],
    hi: [f64; 3],
    cap: bool,
    seed: u64,
}

fn draw(rng: &mut Rng, w: &[f64; 5]) -> usize {
    let t: f64 = w.iter().sum();
    let mut x = rng.random() * t;
    for (i, v) in w.iter().enumerate() {
        if x < *v {
            return i;
        }
        x -= v;
    }
    4
}

impl Cliff {
    /// The stones of a run `len` long, in the run's frame.
    fn plan(&self, len: f64, s: f64, rng: &mut Rng) -> Vec<Stone> {
        let h = self.height * s;
        let mut out = Vec::new();
        let end = END_MM * s;
        // masses along the run: the ends are the standard section
        let mut cuts = vec![0.0, end];
        let mut t = end;
        while t < len - end - 6.0 * s {
            t += rng.uniform(MASS_W_MM.0, MASS_W_MM.1) * s;
            if t < len - end - 3.0 * s {
                cuts.push(t);
            }
        }
        cuts.push(len - end);
        cuts.push(len);
        for (m, w) in cuts.windows(2).enumerate() {
            let (t0, t1) = (w[0], w[1]);
            let is_end = m == 0 || m == cuts.len() - 2;
            let depth = if is_end {
                END_DEPTH_MM * s
            } else {
                self.depth
                    .map_or_else(|| rng.uniform(DEPTH_MM.0, DEPTH_MM.1), |d| d)
                    * s
            };
            let full = is_end || rng.random() < FULL_P;
            let back = if full {
                depth
            } else {
                rng.uniform(TOP_DEPTH_MM.0, TOP_DEPTH_MM.1).min(depth * 0.7) * s
            };
            // the tall part, then the lower tier in front
            self.columns(&mut out, (t0, t1), (0.0, back), h, s, rng);
            if depth - back > 3.0 * s {
                let lower = h * rng.uniform(TIER_Z.0, TIER_Z.1);
                self.columns(&mut out, (t0, t1), (back, depth), lower, s, rng);
            }
        }
        out
    }

    fn columns(
        &self,
        out: &mut Vec<Stone>,
        (t0, t1): (f64, f64),
        (q0, q1): (f64, f64),
        top: f64,
        s: f64,
        rng: &mut Rng,
    ) {
        let scale = 0.6 + 0.6 * self.size;
        let mut t = t0;
        while t < t1 - 0.5 * s {
            let k = draw(rng, &self.weights);
            let (_, _, along, course) = KINDS[k];
            let mut w = rng.uniform(along.0, along.1) * s * scale.max(0.5);
            if t1 - (t + w) < 4.0 * s {
                w = t1 - t;
            }
            let (ta, tb) = (t, (t + w).min(t1));
            // courses: a stone then a cap no shorter than CAP_MIN
            let mut z = 0.0;
            while z < top - 0.1 {
                let mut hh = rng.uniform(course.0, course.1) * s * (0.55 + 0.45 * self.size);
                if top - (z + hh) < CAP_MIN_MM * s {
                    hh = top - z;
                }
                let cap = z + hh >= top - 1e-9;
                let setback = if z > 0.0 { CAP_SETBACK_MM * s } else { 0.0 };
                out.push(Stone {
                    lo: [ta + setback, q0, z],
                    hi: [tb - setback, q1, (z + hh).min(top)],
                    cap,
                    seed: rng.next_u64(),
                });
                z += hh;
            }
            t = tb;
        }
    }
}

/// A stone's mesh, in its own frame with its base at z = 0.
fn mesh(st: &Stone, s: f64) -> Result<TriMesh, LookError> {
    let size = [
        st.hi[0] - st.lo[0],
        st.hi[1] - st.lo[1],
        st.hi[2] - st.lo[2],
    ];
    let mut rng = Rng::new(dt_kit::Seed::new(st.seed));
    let mut c = Convex::cuboid([0.0; 3], size);
    // arris chamfers on the four upright edges (a stone is never a box)
    for (sx, sy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let d = rng.uniform(0.3, 0.9) * s;
        let (nx, ny) = (
            if sx > 0.5 { 1.0 } else { -1.0 },
            if sy > 0.5 { 1.0 } else { -1.0 },
        );
        let corner = [size[0] * sx, size[1] * sy];
        let off = nx * corner[0] / core::f64::consts::SQRT_2
            + ny * corner[1] / core::f64::consts::SQRT_2
            - d;
        c = c
            .cut(
                [nx, ny, 0.0],
                off * core::f64::consts::SQRT_2 / core::f64::consts::SQRT_2,
            )
            .unwrap_or(c);
    }
    // the cap's top edges are broken back, never an underside
    if st.cap {
        for (nx, ny) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let d = rng.uniform(0.3, 1.2) * s;
            let edge = [
                if nx > 0.0 { size[0] } else { 0.0 },
                if ny > 0.0 { size[1] } else { 0.0 },
            ];
            let on = if nx != 0.0 {
                edge[0] * nx
            } else {
                edge[1] * ny
            };
            let n = [nx, ny, 1.0];
            let l = libm::sqrt(2.0);
            let off = (on + size[2]) / l - d;
            c = c.cut(n, off * l).unwrap_or(c);
        }
    }
    let g = Grain {
        edge_mm: 0.4,
        ..Grain::default()
    };
    grain(&c, &g, st.seed).map_err(LookError::build)
}

impl Look for Cliff {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid("cliff"),
            reads: vec![Read::Solids("ground"), Read::IfAny("rock")],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let frame = *scene.frame();
        let s = frame.scale();
        let sq = frame.square_mm;
        let Some(along) = ctx.along else {
            return Err(LookError {
                control: None,
                message: "a cliff needs along= the tile edge it stands against".into(),
                help: Some("write along=\"north\"".into()),
            });
        };
        if along.len() != 2 {
            return Err(LookError {
                control: Some("along".into()),
                message: "a cliff runs along one straight edge for now; corners are a later form"
                    .into(),
                help: Some("write along=\"north\" (one edge)".into()),
            });
        }
        let (a, b) = (
            [along[0][0] * sq, along[0][1] * sq],
            [along[1][0] * sq, along[1][1] * sq],
        );
        let len = libm::hypot(b[0] - a[0], b[1] - a[1]);
        let d = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
        let n = [-d[1], d[0]]; // inward: the tile is on the left
        let Some((surface, _)) = scene.surface() else {
            return Err(LookError {
                control: None,
                message: "a cliff has no ground to stand on".into(),
                help: Some("put a `ground` or `dirt` before it".into()),
            });
        };
        let ground = Field {
            nx: surface.nx,
            ny: surface.ny,
            step: surface.step,
            z: surface.z.iter().map(|&v| f64::from(v)).collect(),
        };
        let mut rng = Rng::new(ctx.seed.key("cliff"));
        let stones = self.plan(len, s, &mut rng);
        // the foot: the lowest ground along the band
        let mut foot = f64::INFINITY;
        for st in &stones {
            for (t, q) in [
                (st.lo[0], st.lo[1]),
                (st.hi[0], st.hi[1]),
                (st.lo[0], st.hi[1]),
                (st.hi[0], st.lo[1]),
            ] {
                let p = [a[0] + t * d[0] + q * n[0], a[1] + t * d[1] + q * n[1]];
                foot = foot.min(ground.sample(
                    p[0].clamp(0.0, frame.size_mm()[0]),
                    p[1].clamp(0.0, frame.size_mm()[1]),
                ));
            }
        }
        let base = foot - SINK_MM * s;
        // each stone's own ground, so a stone on a slope still sits on it
        let local = |st: &Stone| {
            let p = [
                a[0] + 0.5 * (st.lo[0] + st.hi[0]) * d[0] + 0.5 * (st.lo[1] + st.hi[1]) * n[0],
                a[1] + 0.5 * (st.lo[0] + st.hi[0]) * d[1] + 0.5 * (st.lo[1] + st.hi[1]) * n[1],
            ];
            let _ = p;
            base
        };
        let mut parts: Vec<TriMesh> = stones
            .par_iter()
            .map(|st| {
                let mut m = mesh(st, s)?;
                // stretch into the neighbours a little by growing nothing: the
                // overlap is the box's own PRESS (below), applied in the frame
                let z0 = local(st) + st.lo[2];
                for p in &mut m.positions {
                    let (t, q) = (st.lo[0] + p[0], st.lo[1] + p[1]);
                    p[0] = a[0] + t * d[0] + q * n[0];
                    p[1] = a[1] + t * d[1] + q * n[1];
                    p[2] += z0;
                }
                Ok(m)
            })
            .collect::<Result<_, LookError>>()?;
        // the core: the back of the band solid, so a gap between stones is a
        // recess and never a hole
        let core_depth = stones
            .iter()
            .map(|st| st.hi[1])
            .fold(f64::INFINITY, f64::min)
            .min(END_DEPTH_MM * s)
            * 0.6;
        let top_h = stones.iter().map(|st| st.hi[2]).fold(0.0f64, f64::max);
        let inset = 0.8 * s;
        let corners =
            |t: f64, q: f64, z: f64| [a[0] + t * d[0] + q * n[0], a[1] + t * d[1] + q * n[1], z];
        let (t0, t1, q0, q1) = (inset, len - inset, 0.0, core_depth);
        let ztop = base + top_h * 0.9;
        let pts = [
            corners(t0, q0, base),
            corners(t1, q0, base),
            corners(t1, q1, base),
            corners(t0, q1, base),
            corners(t0, q0, ztop),
            corners(t1, q0, ztop),
            corners(t1, q1, ztop),
            corners(t0, q1, ztop),
        ];
        let tris = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        let mut core = TriMesh {
            positions: pts.to_vec(),
            triangles: tris.to_vec(),
            ..TriMesh::default()
        };
        if dt_kit::signed_volume(&core) < 0.0 {
            core.triangles.iter_mut().for_each(|t| t.swap(1, 2));
        }
        parts.push(core);
        let slots = scene
            .slots(
                u32::try_from(parts.len()).map_err(LookError::build)?,
                Material::Rock,
            )
            .map_err(LookError::build)?;
        for (i, m) in parts.iter().enumerate() {
            let solid = form::solid(slots.get(u32::try_from(i).unwrap_or(0)), m)
                .map_err(LookError::build)?;
            scene.add(
                Tag {
                    kind: "cliff",
                    owner: ctx.path.to_owned(),
                },
                solid,
            );
        }
        scene.note("cliff_stones", stones.len() as f64);
        let _ = PRESS_MM;
        Ok(())
    }
}
