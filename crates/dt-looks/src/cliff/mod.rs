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
    "bank-height",
    "bank-reach",
    "scree",
    "boulders",
    "grit",
];

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
    foot: foot::FootAsk,
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
            foot: foot::FootAsk::default(),
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
                ("depth", Value::Range(a, b)) if *a >= 4.0 && *b <= 60.0 => c.depth = Some((*a, *b)),
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
                ("stone-size", Value::Number(v)) => c.size = if *v >= 1.0 { *v / 1.35 } else { (1.0 + 0.35 * v.clamp(0.0, 1.0)) / 1.35 },
                ("form", Value::Word(s)) => {
                    c.form = Some(FormName::parse(s).ok_or_else(|| bad(format!("no cliff form `{s}`")))?);
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
                ("bank-height", Value::Number(v)) => c.foot.bank_h = (*v, *v),
                ("bank-height", Value::Range(a, b)) => c.foot.bank_h = (*a, *b),
                ("bank-reach", Value::Number(v)) => c.foot.bank_r = (*v, *v),
                ("bank-reach", Value::Range(a, b)) => c.foot.bank_r = (*a, *b),
                ("scree", Value::Number(v)) => c.foot.scree = v.max(0.0),
                ("boulders", Value::Number(v)) => c.foot.boulders = v.max(0.0),
                ("grit", Value::Number(v)) => c.foot.grit = v.max(0.0),
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
    fn world(&self, p: [f64; 2]) -> [f64; 2] {
        [
            self.cx[0] * p[0] + self.cy[0] * p[1] + self.t[0],
            self.cx[1] * p[0] + self.cy[1] * p[1] + self.t[1],
        ]
    }
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

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let frame = *scene.frame();
        let s = frame.scale();
        let sq = frame.square_mm;
        let size = frame.size_mm();
        // which form, and where it stands
        let form = self.form.unwrap_or_else(|| match ctx.along {
            Some(a) if a.len() > 2 => FormName::Corner,
            _ => FormName::Run,
        });
        let f2 = if form.is_band() && matches!(form, FormName::Run | FormName::Corner | FormName::Escarpment) {
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
            Frame2 { w: size[0], h: size[1], cx: [1.0, 0.0], cy: [0.0, 1.0], t: [0.0, 0.0] }
        };
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
        let st = stack::stack(&mut rng, &fm, &zone, s, max_rocks);
        let placed = st.rocks;
        if placed.is_empty() {
            return Ok(());
        }
        let mut cols = finish::columns(&mut rng, &placed, &fm, &st.s, &zone);
        let nslots = placed.len() as u32 + 2;
        let slots = scene.slots(nslots, Material::Rock).map_err(LookError::build)?;
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
        let cones: Vec<Option<dt_kit::TriMesh>> =
            placed.iter().map(|r| if r.z0 > form::FOOT { finish::support_cone(r) } else { None }).collect();
        let core_support = Support::new(std::slice::from_ref(&core_mesh)).map_err(LookError::build)?;
        let env = Env { form: &fm, s: &st.s, placed: &placed, seed: ctx.seed.key("cliff-grain").value() };
        let col_seed = ctx.seed.key("cliff-columns");
        let results: Vec<Result<(Vec<(usize, dt_kit::Piece)>, usize), dt_kit::KitError>> = cols
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
                        (pb[0] < b[2] + 3.0 && pb[2] > b[0] - 3.0 && pb[1] < b[3] + 3.0 && pb[3] > b[1] - 3.0)
                            .then(|| m.clone())
                    })
                    .collect();
                let cone_support = Support::new(&near)?;
                let sup = [&cone_support, &core_support];
                let mut cs = ColState::default();
                let mut out = Vec::new();
                for &ri in c.stones.iter().rev() {
                    let slot = slots.get(2 + ri as u32);
                    if let Some(p) = stone::rock_piece(&mut rng, &mut crng, ri, c, &mut cs, &env, &sup, slot)? {
                        out.push((ri, p));
                    }
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
        let core = relief::relief(core, &all_faces, Some((fm.w, fm.h))).map_err(LookError::build)?;
        let core = stone::grain(&core, env.seed, form::FOOT).map_err(LookError::build)?;
        let m = f2.matrix(foot);
        let n_stones = pieces.len();
        for (_, p) in pieces {
            let solid = p.transformed(&m).into_solid();
            scene.add(Tag { kind: "cliff", owner: ctx.path.to_owned() }, solid);
        }
        scene.add(Tag { kind: "cliff", owner: ctx.path.to_owned() }, core.transformed(&m).into_solid());
        let mut frng = Rng::new(ctx.seed.key("cliff-foot"));
        let world = |p: [f64; 2]| f2.world(p);
        foot::build(
            scene,
            ctx.path,
            &mut frng,
            &fm,
            &st.s,
            &zone,
            &foot::Place { world: &world, matrix: m, ground: &ground, foot, scale: s },
            &self.foot,
            ctx.seed.key("cliff-scree").value(),
        )?;
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
    let (x0, y0, x1, y1) = (ba[0].max(bb[0]), ba[1].max(bb[1]), ba[2].min(bb[2]), ba[3].min(bb[3]));
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
