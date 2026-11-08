//! Cliff: a stacked pile of rocks (foot 3.4 mm).
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

use dt_kit::Kind;
use dt_kit::{Material, Rng, Scene, Support, Tag, smooth01};
use rayon::prelude::*;

use crate::{Control, Ctx, Element, LookError, Plan, Value, Write};
use form::{Ask, FormName};
use stone::{ColState, Env};

type ColResult = Result<(Vec<(usize, dt_kit::Piece)>, usize), dt_kit::KitError>;

/// The cliff element.
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
                ("height", Value::Number(v)) => c.height = Some(*v),
                ("depth", Value::Number(v)) => c.depth = Some((*v, *v)),
                ("depth", Value::Range(a, b)) => c.depth = Some((*a, *b)),
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
            f2,
            fm,
            zone,
            st,
            rng,
        })
    }
}

struct Planned {
    f2: Frame2,
    fm: form::Form,
    zone: form::Zones,
    st: stack::Stacked,
    rng: Rng,
}

impl Element for Cliff {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Cliff),
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
                ends[q] = smooth01(p[0].min(fm.w - p[0]) / (2.0 * foot::END_MM * s));
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
            kind: Kind::Cliff,
            dist,
            zone: us,
            end: ends,
            cells,
            squares,
            nx,
        }))
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let t0 = std::time::Instant::now();
        let progress = scene.progress();
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
            return Err(LookError::general(
                "a cliff has no ground to stand on",
                "put a `ground` or `dirt` before it",
            ));
        };
        let ground = dt_kit::Field {
            nx: surface.nx,
            ny: surface.ny,
            step: surface.step,
            z: surface.z.iter().map(|&v| f64::from(v)).collect(),
        };
        // the foot: the lowest ground under the tile
        let foot = ground.z.iter().copied().fold(f64::INFINITY, f64::min);
        progress.fraction("planning the pile", 0.01);
        eprintln!("[cliff] after planning the pile {:.2}s", t0.elapsed().as_secs_f64());
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
            finish::plan_column(&mut rng, c, &placed, &fm, plan_slot)
                .map_err(|e| LookError::build(format!("plan column: {e}")))?;
        }
        progress.fraction("dressing columns", 0.03);
        eprintln!("[cliff] after dressing columns {:.2}s", t0.elapsed().as_secs_f64());
        let core_mesh = core::core_mesh(&st.s, &cols, &placed)
            .map_err(|e| LookError::build(format!("core mesh: {e}")))?;
        let m = f2.matrix(foot);
        if progress.sketching() {
            let rough = rough_pile(&core_mesh, &placed, &m);
            progress.sketch(&rough, &vec![Material::Rock; rough.triangles.len()], true);
        }
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
            draft: ctx.draft,
        };
        let col_seed = ctx.seed.key("cliff-columns");
        progress.fraction("planning faces and cracks", 0.04);
        eprintln!("[cliff] after planning faces and cracks {:.2}s", t0.elapsed().as_secs_f64());
        let ticks = progress.counter("stones", placed.len(), 0.04, 0.14);
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
                Ok((out, cs.tops))
            })
            .collect();
        progress.fraction("stones finished", 0.14);
        eprintln!("[cliff] after stones finished {:.2}s", t0.elapsed().as_secs_f64());
        let mut pieces: Vec<(usize, dt_kit::Piece)> = Vec::new();
        let mut tops = 0;
        for r in results {
            let (v, t) = r.map_err(LookError::build)?;
            pieces.extend(v);
            tops += t;
        }
        pieces.sort_by_key(|p| p.0);
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
        let core = core::core_piece(slots.get(1), &core_mesh, &fm)
            .map_err(|e| LookError::build(format!("core piece: {e}")))?;
        progress.fraction("carving the core", 0.15);
        eprintln!("[cliff] after carving the core {:.2}s", t0.elapsed().as_secs_f64());
        let core = relief::relief(
            core,
            &all_faces,
            Some((fm.w, fm.h)),
            Some((&progress, 0.15, 0.72)),
            env.coarse(),
        )
        .map_err(LookError::build)?;
        progress.fraction("graining the core", 0.72);
        eprintln!("[cliff] after graining the core {:.2}s", t0.elapsed().as_secs_f64());
        let core = if ctx.draft {
            core
        } else {
            stone::grain(&core, env.seed, form::FOOT).map_err(LookError::build)?
        };
        progress.fraction("cleaning up the core", 0.85);
        eprintln!("[cliff] after cleaning up the core {:.2}s", t0.elapsed().as_secs_f64());
        let core = core.parts_over(1e9).map_err(LookError::build)?;
        let mut core = core;
        if std::env::var_os("DT_STACK").is_some() {
            // SPIKE (M0): the cliff's hand-set band is [foot, the form's top],
            // kept a little wide at the footprint's edge; a stone that
            // pokes above it is cut to it (a top cut by a height field)
            let t_fit = std::time::Instant::now();
            let (nx, ny) = (fm.t.nx, fm.t.ny);
            let g = form::GRID_MM;
            let mut cap = vec![1e3f64; (nx + 2) * (ny + 2)];
            for j in 0..ny + 2 {
                for i in 0..nx + 2 {
                    let (ci, cj) = (i as isize - 1, j as isize - 1);
                    let mut hi = f64::MIN;
                    for dj in -2isize..=2 {
                        for di in -2isize..=2 {
                            let (a, b) = (ci + di, cj + dj);
                            if a >= 0 && b >= 0 && (a as usize) < nx && (b as usize) < ny {
                                hi = hi.max(fm.t.z[b as usize * nx + a as usize]);
                            }
                        }
                    }
                    if hi > form::FOOT {
                        cap[j * (nx + 2) + i] = hi;
                    }
                }
            }
            let field = dt_kit::Field {
                nx: nx + 2,
                ny: ny + 2,
                step: g,
                z: cap.iter().map(|c| c + 5.0).collect(),
            };
            let mut band = dt_kit::hung(&field, 0.0).map_err(LookError::build)?;
            for p in &mut band.positions {
                p[0] -= 0.5 * g;
                p[1] -= 0.5 * g;
                p[2] -= 5.0;
            }
            let over = |m: &dt_kit::TriMesh| {
                m.positions.iter().any(|p| {
                    let i = ((p[0] / g).floor().max(0.0) as usize).min(nx - 1);
                    let j = ((p[1] / g).floor().max(0.0) as usize).min(ny - 1);
                    p[2] > cap[(j + 1) * (nx + 2) + i + 1] + 0.05
                })
            };
            let mut cut = 0;
            for (_, p) in &mut pieces {
                if over(&p.mesh()) {
                    *p = p.within(&band).map_err(LookError::build)?;
                    cut += 1;
                }
            }
            if over(&core.mesh()) {
                core = core.within(&band).map_err(LookError::build)?;
                cut += 1;
            }
            eprintln!(
                "[cliff] band fit: {cut} of {} pieces cut to the form's top in {:.2}s",
                pieces.len() + 1,
                t_fit.elapsed().as_secs_f64()
            );
        }
        progress.fraction("dirt on the tops", 0.90);
        eprintln!("[cliff] after dirt on the tops {:.2}s", t0.elapsed().as_secs_f64());
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
                        kind: Kind::Bank,
                        owner: ctx.path.to_owned(),
                    },
                    d.transformed(&m).into_solid(),
                );
            }
        }
        progress.fraction("placing stones", 0.98);
        eprintln!("[cliff] after placing stones {:.2}s", t0.elapsed().as_secs_f64());
        let n_stones = pieces.len();
        for (_, p) in pieces {
            let solid = p.transformed(&m).into_solid();
            scene.add(
                Tag {
                    kind: Kind::Cliff,
                    owner: ctx.path.to_owned(),
                },
                solid,
            );
        }
        scene.add(
            Tag {
                kind: Kind::Cliff,
                owner: ctx.path.to_owned(),
            },
            core.transformed(&m).into_solid(),
        );
        scene.note("cliff_stones", n_stones as f64);
        scene.note("cliff_top_cracks", tops as f64);
        Ok(())
    }
}

/// The pile as boxes: the core and each rock's footprint stood from its
/// underside to its top, placed on the tile by `m`.  A stand-in for the
/// view while the stones are cut, never printed.
fn rough_pile(core: &dt_kit::TriMesh, placed: &[stack::Placed], m: &[f64; 12]) -> dt_kit::TriMesh {
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
    // m is column-major: x' = m0 x + m3 y + m6 z + m9
    for p in &mut out.positions {
        let [x, y, z] = *p;
        *p = [
            m[0] * x + m[3] * y + m[6] * z + m[9],
            m[1] * x + m[4] * y + m[7] * z + m[10],
            m[2] * x + m[5] * y + m[8] * z + m[11],
        ];
    }
    out
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
