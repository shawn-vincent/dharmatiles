//! Cliff: a port of clifflab's rockpile (clifflab/rockpile.py, frozen at
//! round 17, 31f7c5d, foot 3.4 mm).
//!
//! A cliff is **stacked under gravity**: a form gives a target height
//! field; rocks are placed lowest pocket first, each resting on the
//! highest supports under it ([`stack`]). Stones that share a footprint
//! stand in one **column**, cleaved by whole planes planned once on the
//! column's hull ([`finish`]). Each open face of the pile gets one crack
//! network, planned once so cracks run on across stones ([`faces`]); each
//! stone is then cut to its column's planes, grooved under the stone it
//! carries, **seated** on its bed at the overhang limit (so a stone that
//! rests on rock has no ceiling by construction), chipped, carved with the
//! face relief and cracks ([`relief`]), fitted to the tile and grained
//! ([`stone`]). A **core** behind the stones makes a gap between them a
//! recess, never a hole ([`core`]).
//!
//! Forms (`form=`): run, corner, canyon, escarpment (bands grown out from
//! tile edges) and plateau, massif, mound, spire, needles (free-standing
//! rings). Details (cracks, joints, chips, grain) are printability sizes
//! and never scale; masses scale with the system's square.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::needless_range_loop
)]

mod chips;
mod core;
mod faces;
mod finish;
mod foot;
mod form;
mod lines;
mod relief;
mod stack;
mod stone;
mod util;

use dt_kit::{Material, Rng, Scene, Support, Tag};
use rayon::prelude::*;

use crate::{Control, Ctx, Look, LookError, Plan, Value, Write, only};
use form::{Ask, FormName};
use stone::{ColState, Env};

/// The controls `cliff` takes.
pub const CONTROLS: &[&str] = &[
    "height",
    "depth",
    "rock",
    "stone-size",
    "top",
    "form",
    "ledges",
    "ledges/height",
    "shattered/patches",
    "shattered/width",
];

type ColResult = Result<(Vec<(usize, dt_kit::Piece)>, usize), dt_kit::KitError>;

/// The cliff look.
#[derive(Clone, Debug)]
pub struct Cliff {
    height: Option<f64>,
    depth: Option<(f64, f64)>,
    weights: Option<[f64; 5]>,
    /// stone-size multiplier relative to the form's own (massive = 1).
    size: f64,
    form: Option<FormName>,
    top_dirt: bool,
    patches: Option<f64>,
    patch_width: Option<(f64, f64)>,
    ledge_height: Option<f64>,
}

const KIND_NAMES: [&str; 5] = ["columns", "blocks", "slabs", "odd", "monoliths"];

impl Cliff {
    /// From controls.
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("cliff", controls, CONTROLS)?;
        let mut c = Self {
            height: None,
            depth: None,
            weights: None,
            size: 1.0 / 1.35,
            form: None,
            top_dirt: false,
            patches: None,
            patch_width: None,
            ledge_height: None,
        };
        let mut w = [f64::NAN; 5];
        for k in controls {
            let bad = |m: String| LookError {
                control: Some(k.name.clone()),
                message: m,
                help: None,
            };
            match (k.name.as_str(), &k.value) {
                ("height", Value::Number(v)) if (6.0..=120.0).contains(v) => c.height = Some(*v),
                ("height", v) => return Err(bad(format!("a cliff is 6 to 120 mm high, not {v}"))),
                ("depth", Value::Number(v)) if (4.0..=60.0).contains(v) => c.depth = Some((*v, *v)),
                ("depth", Value::Range(a, b)) if *a >= 4.0 && *b <= 60.0 => {
                    c.depth = Some((*a, *b))
                }
                ("depth", v) => {
                    return Err(bad(format!("a cliff reaches 4 to 60 mm out, not {v}")));
                }
                ("stone-size", Value::Word(s)) => {
                    c.size = match s.as_str() {
                        "rubble" => 1.0 / 1.35,
                        "broken" => 1.15 / 1.35,
                        _ => 1.0,
                    };
                }
                ("stone-size", Value::Number(v)) => {
                    c.size = if *v >= 1.0 {
                        *v / 1.35
                    } else {
                        (1.0 + 0.35 * v.clamp(0.0, 1.0)) / 1.35
                    }
                }
                ("form", Value::Word(s)) => {
                    c.form = Some(
                        FormName::parse(s).ok_or_else(|| bad(format!("no cliff form `{s}`")))?,
                    );
                }
                ("rock", Value::Word(s)) => {
                    if let Some(i) = KIND_NAMES.iter().position(|n| n == s) {
                        w = [0.0; 5];
                        w[i] = 1.0;
                    }
                }
                (n, Value::Number(v)) if n.starts_with("rock.") => {
                    if let Some(i) = KIND_NAMES.iter().position(|k| n.ends_with(k)) {
                        w[i] = v.max(0.0);
                    }
                }
                ("top", Value::Word(s)) => c.top_dirt = s == "dirt",
                ("shattered/patches", Value::Number(v)) => c.patches = Some(*v),
                ("shattered/width", Value::Number(v)) => c.patch_width = Some((*v, *v)),
                ("shattered/width", Value::Range(a, b)) => c.patch_width = Some((*a, *b)),
                ("ledges/height", Value::Number(v)) => c.ledge_height = Some(*v),
                _ => {}
            }
        }
        if w.iter().any(|x| !x.is_nan()) {
            let w: [f64; 5] = w.map(|x| if x.is_nan() { 0.0 } else { x });
            if w.iter().sum::<f64>() > 0.0 {
                c.weights = Some(w);
            }
        }
        Ok(c)
    }
}

/// Where a form stands: canonical (x east, y north, back wall at y = H)
/// to world.
struct Frame2 {
    /// Canonical size.
    w: f64,
    h: f64,
    /// Columns of the xy linear map and the shift.
    cx: [f64; 2],
    cy: [f64; 2],
    t: [f64; 2],
}

impl Frame2 {
    fn matrix(&self, z_shift: f64) -> [f64; 12] {
        [
            self.cx[0], self.cx[1], 0.0, self.cy[0], self.cy[1], 0.0, 0.0, 0.0, 1.0, self.t[0],
            self.t[1], z_shift,
        ]
    }
}

impl Cliff {
    /// Everything that decides where the stones stand, with no geometry:
    /// the plan pass (a bank or scatter reads its foot) and the build both
    /// start here, from the same seed.
    fn pile_plan(&self, frame: &dt_kit::Frame, ctx: &Ctx<'_>) -> Result<Planned, LookError> {
        let s = frame.scale();
        let sq = frame.square_mm;
        let size = frame.size_mm();
        // which form, and where it stands
        let default_form = match ctx.along {
            Some(a) if a.len() > 2 => FormName::Corner,
            _ => FormName::Run,
        };
        let form = self.form.unwrap_or(default_form);
        let f2 = if form.is_band()
            && matches!(
                form,
                FormName::Run | FormName::Corner | FormName::Escarpment
            ) {
            let Some(along) = ctx.along else {
                return Err(LookError {
                    control: None,
                    message: "this cliff form needs along= the tile edge it stands against".into(),
                    help: Some("write along=\"north\"".into()),
                });
            };
            if along.len() < 2 {
                return Err(LookError {
                    control: Some("along".into()),
                    message: "a cliff needs an edge to stand against".into(),
                    help: Some("write along=\"north\" (one edge)".into()),
                });
            }
            let a = [along[0][0] * sq, along[0][1] * sq];
            let b = [along[1][0] * sq, along[1][1] * sq];
            let len = libm::hypot(b[0] - a[0], b[1] - a[1]);
            let d = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
            let n = [-d[1], d[0]]; // inward: the tile is on the left
            // perpendicular extent of the tile along n
            let h = (size[0] * n[0].abs() + size[1] * n[1].abs()).max(1.0);
            // world = a + (L - x) d + (H - y) n
            Frame2 {
                w: len,
                h,
                cx: [-d[0], -d[1]],
                cy: [-n[0], -n[1]],
                t: [a[0] + len * d[0] + h * n[0], a[1] + len * d[1] + h * n[1]],
            }
        } else {
            Frame2 {
                w: size[0],
                h: size[1],
                cx: [1.0, 0.0],
                cy: [0.0, 1.0],
                t: [0.0, 0.0],
            }
        };
        let mut rng = Rng::new(ctx.seed.key("cliff"));
        let ask = Ask {
            form,
            height: self.height,
            depth: self.depth,
            mass_gain: self.size,
            kinds: self.weights,
            ledge_height: self.ledge_height,
        };
        let fm = form::make_form(&mut rng, &ask, f2.w, f2.h, s);
        let zone = form::Zones::new(&mut rng, f2.w, s, self.patches, self.patch_width);
        let max_rocks = 400 * ((f2.w * f2.h / (sq * sq)).ceil() as usize).max(1);
        let mut st = stack::stack(&mut rng, &fm, &zone, s, max_rocks);
        stack::reconcile(&mut st.rocks, &fm, &mut st.s);
        Ok(Planned {
            form,
            f2,
            fm,
            zone,
            st,
            rng,
        })
    }
}

struct Planned {
    #[allow(dead_code)]
    form: FormName,
    f2: Frame2,
    fm: form::Form,
    zone: form::Zones,
    st: stack::Stacked,
    rng: Rng,
}

impl Look for Cliff {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid("cliff"),
            // the painted surface (always first), not the lifted ground: a bank
            // heaped against the cliff is made before the lift
            reads: vec![],
            cell_mm: None,
        }
    }

    fn foot(&self, frame: &dt_kit::Frame, ctx: &Ctx<'_>) -> Result<Option<crate::Foot>, LookError> {
        let Planned {
            f2, fm, zone, st, ..
        } = self.pile_plan(frame, ctx)?;
        let s = frame.scale();
        let (nx, ny) = frame.nodes();
        let step = frame.step_mm();
        let to_canon = |w: [f64; 2]| {
            let d = [w[0] - f2.t[0], w[1] - f2.t[1]];
            [
                d[0] * f2.cx[0] + d[1] * f2.cx[1],
                d[0] * f2.cy[0] + d[1] * f2.cy[1],
            ]
        };
        let mut rock = vec![false; nx * ny];
        let mut us = vec![0.0; nx * ny];
        let mut ends = vec![0.0; nx * ny];
        let mut wts = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let p = to_canon([i as f64 * step, j as f64 * step]);
                let (ci, cj) = (
                    (p[0] / form::GRID_MM).floor(),
                    (p[1] / form::GRID_MM).floor(),
                );
                let q = j * nx + i;
                if ci >= 0.0 && cj >= 0.0 && (ci as usize) < st.s.nx && (cj as usize) < st.s.ny {
                    rock[q] = st.s.at(ci as usize, cj as usize) > form::FOOT + 0.5;
                }
                let z = zone.at(p[0]);
                us[q] = (z - form::ZONE_FLOOR) / (1.0 - form::ZONE_FLOOR);
                wts[q] = 0.5 + z;
                ends[q] = dt_kit::smooth01(p[0].min(fm.w - p[0]) / (2.0 * foot::END_MM * s));
            }
        }
        let (d, _) = dt_kit::distance(frame, &rock);
        let dist: Vec<f64> =
            d.z.iter()
                .map(|&v| if v.is_finite() { v } else { 1e9 })
                .collect();
        let mut cells = Vec::new();
        for j in 1..ny - 1 {
            for i in 1..nx - 1 {
                let q = j * nx + i;
                if dist[q] > 0.0 && dist[q] <= 1.01 * step {
                    let g = [dist[q + 1] - dist[q - 1], dist[q + nx] - dist[q - nx]];
                    let l = libm::hypot(g[0], g[1]).max(1e-9);
                    cells.push((
                        [i as f64 * step, j as f64 * step],
                        [g[0] / l, g[1] / l],
                        wts[q],
                    ));
                }
            }
        }
        let squares = cells.len() as f64 * step / frame.square_mm;
        Ok(Some(crate::Foot {
            kind: "cliff",
            dist,
            zone: us,
            end: ends,
            cells,
            squares,
            nx,
        }))
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let frame = *scene.frame();
        let Planned {
            f2,
            fm,
            zone,
            st,
            mut rng,
            ..
        } = self.pile_plan(&frame, ctx)?;
        let Some((surface, _)) = scene.surface() else {
            return Err(LookError {
                control: None,
                message: "a cliff has no ground to stand on".into(),
                help: Some("put a `ground` or `dirt` before it".into()),
            });
        };
        let ground = dt_kit::Field {
            nx: surface.nx,
            ny: surface.ny,
            step: surface.step,
            z: surface.z.iter().map(|&v| f64::from(v)).collect(),
        };
        // the foot: the lowest ground under the tile
        let foot = ground.z.iter().copied().fold(f64::INFINITY, f64::min);

        let placed = st.rocks;
        if placed.is_empty() {
            return Ok(());
        }
        let mut cols = finish::columns(&mut rng, &placed, &fm, &st.s, &zone);
        let nslots = placed.len() as u32 + 3;
        let slots = scene
            .slots(nslots, Material::Rock)
            .map_err(LookError::build)?;
        let plan_slot = slots.get(0);
        // columns that carry another column's stone stand where they are
        for c in &mut cols {
            let cpoly = &c.poly;
            for (ri, r) in placed.iter().enumerate() {
                if c.stones.contains(&ri) || r.z0 <= c.zlo + 1.0 {
                    continue;
                }
                let rp = r.poly();
                if poly_overlap(&rp, cpoly) > 1.0 {
                    c.carries = true;
                    c.back.iter_mut().for_each(|b| b.1 = 0.0);
                    break;
                }
            }
        }
        for c in &mut cols {
            finish::plan_column(&mut rng, c, &placed, &fm, plan_slot).map_err(LookError::build)?;
        }
        let core_mesh = core::core_mesh(&st.s, &cols, &placed).map_err(LookError::build)?;
        let mut frng = Rng::new(ctx.seed.key("cliff-faces"));
        faces::plan_faces(&mut frng, &mut cols, &placed);
        // support cones of resting stones, for the cleaving guard
        let cones: Vec<Option<dt_kit::TriMesh>> = placed
            .iter()
            .map(|r| {
                if r.z0 > form::FOOT {
                    finish::support_cone(r)
                } else {
                    None
                }
            })
            .collect();
        let core_support =
            Support::new(std::slice::from_ref(&core_mesh)).map_err(LookError::build)?;
        let env = Env {
            form: &fm,
            s: &st.s,
            placed: &placed,
            seed: ctx.seed.key("cliff-grain").value(),
        };
        let col_seed = ctx.seed.key("cliff-columns");
        if std::env::var("DT_OVERHANG_IDS").is_ok() {
            eprintln!("cliff stone 0 id {}", slots.get(2).id());
        }
        let ticks = scene.progress().counter("stones", placed.len(), 0.0, 0.9);
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
                let cone_support = Support::new(&near)?;
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
                Ok((out, cs.tops))
            })
            .collect();
        let mut pieces: Vec<(usize, dt_kit::Piece)> = Vec::new();
        let mut tops = 0;
        for r in results {
            let (v, t) = r.map_err(LookError::build)?;
            pieces.extend(v);
            tops += t;
        }
        pieces.sort_by_key(|p| p.0);
        if std::env::var("DT_LIST").is_ok() {
            for (ri, r) in placed.iter().enumerate() {
                let b = pieces.iter().find(|p| p.0 == ri).and_then(|p| p.1.bounds());
                let nc = r.cells.v.iter().filter(|&&c| c).count();
                eprintln!("stone {ri} cells {nc} z0 {:.1} top {:.1} piece {:?}", r.z0, r.top.0, b.map(|(l, h)| (l.map(|v| (v * 10.0).round() / 10.0), h.map(|v| (v * 10.0).round() / 10.0))));
            }
        }
        // the core where it shows, carved like the faces it lies behind
        let all_faces: Vec<std::sync::Arc<faces::Face>> = {
            let mut v: Vec<std::sync::Arc<faces::Face>> = Vec::new();
            for c in &cols {
                for f in &c.faces {
                    if !v.iter().any(|g| std::sync::Arc::ptr_eq(g, f)) {
                        v.push(std::sync::Arc::clone(f));
                    }
                }
            }
            v
        };
        let core = core::core_piece(slots.get(1), &core_mesh, &fm).map_err(LookError::build)?;
        let core =
            relief::relief(core, &all_faces, Some((fm.w, fm.h))).map_err(LookError::build)?;
        let core = stone::grain(&core, env.seed, form::FOOT).map_err(LookError::build)?;
        let m = f2.matrix(foot);
        if self.top_dirt {
            let mut pile = dt_kit::TriMesh::default();
            for mesh in pieces.iter().map(|p| p.1.mesh()).chain([core.mesh()]) {
                let off = pile.positions.len() as u32;
                pile.positions.extend(mesh.positions);
                pile.triangles
                    .extend(mesh.triangles.iter().map(|t| t.map(|v| v + off)));
            }
            let slot = slots.get(2 + placed.len() as u32);
            if let Some(d) = foot::dirt_top(
                &pile,
                fm.t.nx,
                fm.t.ny,
                ctx.seed.key("cliff-dirt").value(),
                slot,
            )? {
                scene.add(
                    Tag {
                        kind: "bank",
                        owner: ctx.path.to_owned(),
                    },
                    d.transformed(&m).into_solid(),
                );
            }
        }
        let n_stones = pieces.len();
        for (_, p) in pieces {
            let solid = p.transformed(&m).into_solid();
            scene.add(
                Tag {
                    kind: "cliff",
                    owner: ctx.path.to_owned(),
                },
                solid,
            );
        }
        scene.add(
            Tag {
                kind: "cliff",
                owner: ctx.path.to_owned(),
            },
            core.transformed(&m).into_solid(),
        );
        scene.note("cliff_stones", n_stones as f64);
        scene.note("cliff_top_cracks", tops as f64);
        Ok(())
    }
}

/// Approximate overlap area of two convex-ish polygons by the bounds of
/// their intersection test on the cells (cheap): the area of the
/// intersection of their bounding boxes if their outlines overlap.
fn poly_overlap(a: &[util::P2], b: &[util::P2]) -> f64 {
    // sample the smaller polygon's bounds on a 0.5 mm grid
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
