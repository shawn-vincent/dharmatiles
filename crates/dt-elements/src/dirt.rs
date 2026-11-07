//! `dirt`: bare earth on the [`ground`](crate::ground) shape.
//!
//! One height field, built in scale order and combined by smooth maximum,
//! not by adding (a clod or pebble is a new surface that wins where it is
//! higher, with a soft fillet):
//!
//! 1. ground: the level, hills and edge ramp of `ground`, plus a low soil
//!    roughness (fBm, 1.6 mm);
//! 2. clods (`clod_layer`, :205): patchy, by a 6 mm patch field with a 30%
//!    floor; each is 1–3 lumpy lobes with a flattish tilted top, partly
//!    sunk, soft-maxed onto the soil;
//! 3. grit (`grit`, :269): fine fBm and sparse sand grains;
//! 4. pebbles (`place_pebbles` :351, `pebble_shape` :300): dart-thrown
//!    into loose clusters biased into low ground, largest first; each a
//!    flattened cap cut by tilted tangent planes and a polygonal skirt
//!    (the half-space cap), sunk 25–45% and filleted into the soil.
//!
//! The texture ramps in over the outer 1 mm, so the rim is exactly the
//! edge level.
//!
//! Sizes: the ground's hills, patches and clusters follow the square
//! (layout); clods, grit and pebbles keep their true mm (default 1), so
//! their counts follow the tile's area.

use dt_kit::{
    Field, Noise, Placed, Plane, Rng, Scale, Scene, Seed, Spread, cap, noise, scatter, smax,
    smooth01,
};

use crate::ground::Shape;
use crate::read;
use crate::{Control, Ctx, Element, LookError, Plan, Value, Write, only};

/// The controls `dirt` takes (`bank` comes with walls and cliffs).
pub const CONTROLS: &[&str] = &[
    "level",
    "bumps",
    "bumps/up",
    "bumps/down",
    "bump-size",
    "clods",
    "stones",
    "bank/against",
    "bank/height",
    "bank/reach",
];

// the bank heaped against a cliff
const APRON_H_MM: (f64, f64) = (1.5, 3.5);
const APRON_R_MM: (f64, f64) = (3.0, 6.5);
/// (height, reach) at the tile ends, the same for every seed (S4).
const APRON_END: (f64, f64) = (2.5, 4.5);
const APRON_EXP: f64 = 1.6;

/// `bank against=… height=… reach=…`.
#[derive(Clone, Debug, PartialEq)]
pub struct Bank {
    /// The kind it leans on.
    pub against: String,
    /// Height at the face, mm at the reference square.
    pub height: (f64, f64),
    /// How far out it runs, mm at the reference square.
    pub reach: (f64, f64),
}

/// The height-field resolution, true mm.
const CELL_MM: f64 = 0.15;
/// Bedded stones per 2×2 (`stones some`).
const STONES_2X2: (i64, i64) = (4, 7);
/// The texture ramps in over this rim width, true mm.
const RIM_TEX_MM: f64 = 1.0;
/// Below any surface: a lobe or pebble that is not there.
const NONE: f64 = -1e9;

// 1 soil roughness
const SOIL_MID_MM: f64 = 1.6;
const SOIL_MID_AMP_MM: f64 = 0.22;
// 2 clods
const CLOD_PATCH_SIGMA_MM: f64 = 6.0;
const CLOD_PATCH_BIAS: f64 = 0.15;
const CLOD_PATCH_CONTRAST: f64 = 1.0;
const CLOD_DENS_FLOOR: f64 = 0.3;
/// `clods some`: attempts per mm² at full patch density.
const CLOD_DENSITY: f64 = 0.7;
const CLOD_R_MM: (f64, f64) = (0.45, 2.4);
const CLOD_R_EXP: f64 = 2.2;
const CLOD_H_RATIO: (f64, f64) = (0.16, 0.35);
const CLOD_H_MAX_MM: f64 = 0.75;
const CLOD_ASPECT: (f64, f64) = (1.0, 1.7);
const CLOD_LUMP: f64 = 0.22;
const CLOD_OVERLAP: f64 = 0.6;
const CLOD_SOFT_MM: f64 = 0.06;
const CLOD_SUNK: (f64, f64) = (0.0, 0.5);
const CLOD_LOBES: (i64, i64) = (1, 3);
const CLOD_FLAT: f64 = 2.6;
const CLOD_TILT: f64 = 0.05;
// 3 grit
const GRIT_MM: f64 = 0.45;
const GRIT_AMP_MM: f64 = 0.08;
const GRAIN_DENSITY: f64 = 0.12;
const GRAIN_R_MM: (f64, f64) = (0.15, 0.3);
const GRAIN_H_MM: (f64, f64) = (0.05, 0.12);
// 4 pebbles
const PEB_CLUSTERS_2X2: (i64, i64) = (4, 7);
const PEB_CLUSTER_SIGMA_MM: (f64, f64) = (3.0, 7.0);
const PEB_CLUSTER_INSET_MM: f64 = 4.0;
const PEB_BACKGROUND: f64 = 0.12;
const PEB_LOW_BIAS: f64 = 0.8;
const PEB_LOW_SIGMA_MM: f64 = 3.0;
const SMALL_PER_SQ: (i64, i64) = (45, 75);
const SMALL_D_MM: (f64, f64) = (0.5, 1.5);
const MED_PER_SQ: (i64, i64) = (8, 13);
const MED_D_MM: (f64, f64) = (2.2, 4.5);
const PEB_D_EXP: f64 = 1.8;
const PEB_ASPECT: (f64, f64) = (1.0, 1.9);
const PEB_H_RATIO: (f64, f64) = (0.25, 0.45);
const PEB_SINK: (f64, f64) = (0.25, 0.45);
const PEB_ROUND_PROB: f64 = 0.25;
const PEB_FACETS: (i64, i64) = (6, 11);
const PEB_FACETS_SMALL: (i64, i64) = (4, 7);
const PEB_FACET_DROP: (f64, f64) = (0.03, 0.18);
const PEB_ROUND_DROP: (f64, f64) = (0.06, 0.15);
const PEB_MAX_FACET_DEG: f64 = 65.0;
const PEB_SKIRT_DEG: f64 = 58.0;
const PEB_SKIRT_SIDES: (i64, i64) = (5, 8);
const PEB_FACET_TILT_DEG: f64 = 12.0;
const PEB_FLAT: (f64, f64) = (2.0, 3.5);
const PEB_GAP_MM: f64 = 0.25;
const PEB_SOFT_MM: (f64, f64) = (0.08, 0.22);
const PEB_MOUND: f64 = 0.2;
const PEB_SEAT_SIGMA_MM: f64 = 0.5;
const PEB_TRIES: u32 = 60;
/// The printable-texture rule's window and layer, mm.
const TEXTURE_WINDOW_MM: f64 = 1.5;
const TEXTURE_LAYER_MM: f64 = 0.2;
/// The reference square: per-square counts are per this many mm².
const SQUARE_MM: f64 = 35.0;

/// The earth on a ground shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Dirt {
    /// The ground under it.
    pub shape: Shape,
    /// Clod attempts per mm² at full patch density (`clods`).
    pub clod_density: f64,
    /// Bedded stones per 2×2 tile, drawn from this range (`stones`).
    pub stones: (i64, i64),
    /// Earth heaped against something (`bank`).
    pub bank: Option<Bank>,
}

impl Dirt {
    /// From controls: the ground's, and `clods` (a word whose number the
    /// evaluator passes as `clods.density`).
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("dirt", controls, CONTROLS)?;
        let mut clod_density = CLOD_DENSITY;
        for c in controls {
            if c.name == "clods.density" {
                clod_density = read::number(c)?;
            }
        }
        let mut stones = STONES_2X2;
        for c in controls {
            #[allow(clippy::cast_possible_truncation)]
            match (c.name.as_str(), &c.value) {
                // the numbers behind the `stones` word
                ("stones.min", Value::Number(v)) => stones.0 = libm::round(*v) as i64,
                ("stones.max", Value::Number(v)) => stones.1 = libm::round(*v) as i64,
                _ => {}
            }
        }
        stones.1 = stones.1.max(stones.0);
        let range = |name: &str, d: (f64, f64)| {
            controls
                .iter()
                .find(|c| c.name == name)
                .map_or(d, |c| match c.value {
                    Value::Number(v) => (v, v),
                    Value::Range(a, b) => (a, b),
                    Value::Word(_) => d,
                })
        };
        let bank = controls
            .iter()
            .find_map(|c| match (c.name.as_str(), &c.value) {
                ("bank/against", Value::Word(w)) => Some(Bank {
                    against: w.clone(),
                    height: range("bank/height", APRON_H_MM),
                    reach: range("bank/reach", APRON_R_MM),
                }),
                _ => None,
            });
        Ok(Self {
            bank,
            shape: Shape::new("dirt", controls)?,
            clod_density,
            stones,
        })
    }
}

/// `lo + (hi − lo)·u^exp`: a draw skewed toward `lo`.
fn skewed(rng: &mut Rng, (lo, hi): (f64, f64), exp: f64) -> f64 {
    lo + (hi - lo) * libm::pow(rng.random(), exp)
}

fn between(rng: &mut Rng, (lo, hi): (f64, f64)) -> f64 {
    rng.uniform(lo, hi)
}

/// What the dirt element counted, for the build record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counts {
    /// Clods placed.
    pub clods: usize,
    /// Small pebbles (under 2.2 mm).
    pub small: usize,
    /// Medium pebbles.
    pub medium: usize,
    /// Worn (rounded) pebbles among them.
    pub rounded: usize,
    /// Bedded stones.
    pub stones: usize,
    /// The share of the interior meeting the printable-texture rule.
    pub texture_ok: f64,
}

/// The dirt surface over the whole tile (world mm), the pebble mask and
/// the counts.
///
/// Its stones (`stones`) are rock solids added to the scene, owned by
/// `owner`, bedded into this field in its area.
///
/// # Errors
/// [`LookError`] if a stone fails to build.
pub fn surface(
    dirt: &Dirt,
    scene: &mut Scene,
    seed: Seed,
    owner: &str,
    area: &dt_kit::Mask,
    foots: &[crate::Foot],
) -> Result<(Field, Vec<bool>, Counts), LookError> {
    let frame = *scene.frame();
    let (mut base, rim) = dirt.shape.surface(scene, seed.key("ground"));
    if let Some(b) = &dirt.bank {
        let s = frame.scale();
        for f in foots.iter().filter(|f| f.kind.name() == b.against) {
            for k in 0..base.z.len() {
                let (d, u, e) = (f.dist[k], f.zone[k], f.end[k]);
                let rr = APRON_END.1 * s
                    + e * (b.reach.0 * s + (b.reach.1 - b.reach.0) * s * u - APRON_END.1 * s);
                if d >= rr {
                    continue;
                }
                let hh = APRON_END.0 * s
                    + e * (b.height.0 * s + (b.height.1 - b.height.0) * s * u - APRON_END.0 * s);
                base.z[k] += hh * libm::pow((1.0 - d / rr).clamp(0.0, 1.0), APRON_EXP);
            }
        }
    }
    let w_tex = rim.map(|d| smooth01(d / RIM_TEX_MM));
    let mid = noise(
        &frame,
        seed.key("soil"),
        Noise::Fbm {
            scale_mm: SOIL_MID_MM,
            octaves: 3,
        },
        Scale::Print,
    );
    let ground = base.with_z(
        (0..base.z.len())
            .map(|k| base.z[k] + w_tex.z[k] * SOIL_MID_AMP_MM * mid.z[k])
            .collect(),
    );
    let (clodded, clods) = clod_layer(dirt, &frame, seed, &ground);
    let g = grit(&frame, seed);
    let mut z = ground.with_z(
        (0..ground.z.len())
            .map(|k| {
                let soil = ground.z[k] + w_tex.z[k] * (clodded.z[k] - ground.z[k]);
                soil + w_tex.z[k] * g.z[k]
            })
            .collect(),
    );
    let avoid = if dirt.stones.1 > 0 {
        crate::rock::dirt_stones(scene, owner, seed, &mut z, &rim, dirt.stones, area)?
    } else {
        Vec::new()
    };
    let (mask, mut counts) = place_pebbles(&frame, seed, &mut z, &ground, &avoid);
    counts.clods = clods;
    counts.stones = avoid.len();
    counts.texture_ok = texture_ok(&z, &rim);
    Ok((z, mask, counts))
}

/// Clods soft-maxed onto `base`.
fn clod_layer(dirt: &Dirt, frame: &dt_kit::Frame, seed: Seed, base: &Field) -> (Field, usize) {
    let patch = noise(
        frame,
        seed.key("clod-patch"),
        Noise::Smooth {
            sigma_mm: CLOD_PATCH_SIGMA_MM,
        },
        Scale::Layout,
    );
    let dens = patch.map(|p| {
        CLOD_DENS_FLOOR
            + (1.0 - CLOD_DENS_FLOOR) * smooth01(0.5 + CLOD_PATCH_CONTRAST * (p + CLOD_PATCH_BIAS))
    });
    let [w, h] = frame.size_mm();
    let mut rng = Rng::new(seed.key("clods"));
    let n = rng.poisson(dirt.clod_density * w * h);
    #[allow(clippy::cast_possible_truncation)]
    let radii: Vec<f64> = (0..n as usize)
        .map(|_| skewed(&mut rng, CLOD_R_MM, CLOD_R_EXP))
        .collect();
    let spread = Spread {
        tries: 1,
        inset: [RIM_TEX_MM, 0.5],
        confine: false,
        gap: 0.0,
        factor: CLOD_OVERLAP,
    };
    let placed = scatter(
        frame,
        &mut rng,
        &radii,
        &spread,
        &|x, y| dens.sample(x, y),
        &[],
    );
    let mut out = base.clone();
    for p in &placed {
        stamp_clod(&mut out, base, seed, p);
    }
    (out, placed.len())
}

fn stamp_clod(out: &mut Field, base: &Field, seed: Seed, p: &Placed) {
    let mut rng = Rng::new(seed.key("clod").index(p.index as u64));
    let ([cx, cy], r) = (p.at, p.r);
    let h = (r * between(&mut rng, CLOD_H_RATIO)).min(CLOD_H_MAX_MM);
    let sunk = between(&mut rng, CLOD_SUNK) * h;
    let tilt = [rng.normal(0.0, CLOD_TILT), rng.normal(0.0, CLOD_TILT)];
    let lobes = rng.int(CLOD_LOBES.0, CLOD_LOBES.1);
    struct Lobe {
        lr: f64,
        ox: f64,
        oy: f64,
        el: f64,
        ca: f64,
        sa: f64,
        harm: [(f64, f64, f64); 3],
        lh: f64,
        reach: f64,
    }
    let shapes: Vec<Lobe> = (0..lobes)
        .map(|li| {
            let lr = r * if li == 0 { 1.0 } else { rng.uniform(0.45, 0.8) };
            let off = if li == 0 {
                0.0
            } else {
                rng.uniform(0.4, 0.9) * r
            };
            let oa = rng.uniform(0.0, 2.0 * core::f64::consts::PI);
            let el = between(&mut rng, CLOD_ASPECT);
            let ang = rng.uniform(0.0, core::f64::consts::PI);
            let harm = [2.0, 3.0, 5.0].map(|k: f64| {
                let a = rng.normal(0.0, CLOD_LUMP / libm::sqrt(k));
                let ph = rng.uniform(0.0, 2.0 * core::f64::consts::PI);
                (k, a, ph)
            });
            let lh = h * if li == 0 { 1.0 } else { rng.uniform(0.6, 1.0) };
            Lobe {
                reach: lr * (1.0 + harm.iter().map(|h| h.1.abs()).sum::<f64>()),
                lr,
                ox: off * libm::cos(oa),
                oy: off * libm::sin(oa),
                el,
                ca: libm::cos(ang),
                sa: libm::sin(ang),
                harm,
                lh,
            }
        })
        .collect();
    // the window holds every lobe whatever its lumps (2r does not:
    // a plain 2r window cuts about 1 clod in 140 straight)
    let reach = shapes
        .iter()
        .map(|l| libm::hypot(l.ox, l.oy) + l.reach * l.el.max(1.0))
        .fold(0.0, f64::max);
    let (is, js) = out.window(cx, cy, reach + out.step);
    for j in js {
        for i in is.clone() {
            let (px, py) = (out.x(i) - cx, out.y(j) - cy);
            let k = j * out.nx + i;
            let mut top = NONE;
            for l in &shapes {
                let (lx, ly) = (px - l.ox, py - l.oy);
                let qa = (lx * l.ca + ly * l.sa) / l.el;
                let qb = -lx * l.sa + ly * l.ca;
                let q = libm::sqrt(qa * qa + qb * qb);
                if q >= l.reach {
                    // outside the lobe whatever its lumps: smax with
                    // nothing leaves `top` as it is
                    continue;
                }
                let th = libm::atan2(qb, qa);
                let rr = 1.0
                    + l.harm
                        .iter()
                        .map(|(n, a, ph)| a * libm::cos(n * th + ph))
                        .sum::<f64>();
                if rr <= 0.0 {
                    continue; // lumps that fold the outline over: no lobe
                }
                let rho = q / (l.lr * rr);
                let lobe = if rho < 1.0 {
                    let prof = l.lh * libm::pow((1.0 - libm::pow(rho, CLOD_FLAT)).max(0.0), 0.8);
                    base.z[k] - sunk + prof + tilt[0] * px + tilt[1] * py
                } else {
                    NONE
                };
                top = smax(top, lobe, CLOD_SOFT_MM);
            }
            out.z[k] = smax(out.z[k], top, CLOD_SOFT_MM);
        }
    }
}

/// Fine fBm plus sparse sand grains.
fn grit(frame: &dt_kit::Frame, seed: Seed) -> Field {
    let mut g = noise(
        frame,
        seed.key("grit"),
        Noise::Fbm {
            scale_mm: GRIT_MM,
            octaves: 2,
        },
        Scale::Print,
    )
    .map(|v| GRIT_AMP_MM * v);
    let [w, h] = frame.size_mm();
    let mut bumps = g.map(|_| 0.0);
    let mut rng = Rng::new(seed.key("grains"));
    for _ in 0..rng.poisson(GRAIN_DENSITY * w * h) {
        let (cx, cy) = (rng.uniform(0.0, w), rng.uniform(0.0, h));
        let (r, gh) = (between(&mut rng, GRAIN_R_MM), between(&mut rng, GRAIN_H_MM));
        let (is, js) = bumps.window(cx, cy, r);
        for j in js {
            for i in is.clone() {
                let (dx, dy) = (bumps.x(i) - cx, bumps.y(j) - cy);
                let d2 = (dx * dx + dy * dy) / (r * r);
                let k = j * bumps.nx + i;
                bumps.z[k] = bumps.z[k].max(gh * libm::pow((1.0 - d2).max(0.0), 0.7));
            }
        }
    }
    g.z.iter_mut().zip(&bumps.z).for_each(|(a, b)| *a += b);
    g
}

/// Where pebbles gather, in [0, 1].
fn cluster_field(frame: &dt_kit::Frame, seed: Seed, ground: &Field) -> Field {
    let s = frame.scale();
    let [w, h] = frame.size_mm();
    let squares = f64::from(frame.cols) * f64::from(frame.rows);
    let mut rng = Rng::new(seed.key("clusters"));
    #[allow(clippy::cast_precision_loss)]
    let n = libm::round(rng.int(PEB_CLUSTERS_2X2.0, PEB_CLUSTERS_2X2.1) as f64 * squares / 4.0);
    let inset = PEB_CLUSTER_INSET_MM * s;
    #[allow(clippy::cast_possible_truncation)]
    let n = n as i64;
    let blobs: Vec<(f64, f64, f64)> = (0..n)
        .map(|_| {
            let cx = rng.uniform(inset, w - inset);
            let cy = rng.uniform(inset, h - inset);
            (cx, cy, between(&mut rng, PEB_CLUSTER_SIGMA_MM) * s)
        })
        .collect();
    let c = Field::from_fn(frame, |x, y| {
        PEB_BACKGROUND
            + blobs
                .iter()
                .map(|(cx, cy, sg)| {
                    libm::exp(-((x - cx) * (x - cx) + (y - cy) * (y - cy)) / (2.0 * sg * sg))
                })
                .sum::<f64>()
    });
    let low = ground.blur(PEB_LOW_SIGMA_MM * s);
    let (lo, hi) = (low.quantile(0.05, None), low.quantile(0.95, None));
    let c = c.zip(&low, |v, l| {
        v * (1.0 + PEB_LOW_BIAS * (1.0 - ((l - lo) / (hi - lo).max(1e-9)).clamp(0.0, 1.0)))
    });
    let max = c.z.iter().copied().fold(0.0, f64::max);
    c.map(|v| v / max.max(1e-12))
}

/// The planes of one pebble (local, rotated
/// and aspect-scaled coordinates), its standing height, whether it is
/// worn, its cap exponent and half-width.
struct Pebble {
    planes: Vec<Plane>,
    hp: f64,
    rounded: bool,
    pf: f64,
    a: f64,
    el: f64,
    ca: f64,
    sa: f64,
}

impl Pebble {
    fn new(rng: &mut Rng, d: f64) -> Self {
        let a = d / 2.0;
        let el = between(rng, PEB_ASPECT);
        let ang = rng.uniform(0.0, core::f64::consts::PI);
        let hp = d * between(rng, PEB_H_RATIO) / libm::pow(el, 0.3);
        let rounded = rng.random() < PEB_ROUND_PROB;
        let pf = between(rng, PEB_FLAT);
        let mut k = if d >= MED_D_MM.0 {
            rng.int(PEB_FACETS.0, PEB_FACETS.1)
        } else {
            rng.int(PEB_FACETS_SMALL.0, PEB_FACETS_SMALL.1)
        };
        if rounded {
            k = (k / 3).max(3);
        }
        let tau = 2.0 * core::f64::consts::PI;
        #[allow(clippy::cast_precision_loss)]
        let kf = k as f64;
        let jitter: Vec<f64> = (0..k).map(|_| rng.random()).collect();
        let turn = rng.uniform(0.0, tau);
        let mut rs: Vec<f64> = (0..k)
            .map(|_| libm::sqrt(rng.uniform(0.05, 0.85)))
            .collect();
        rs[0] = rng.uniform(0.0, 0.35);
        let tl = libm::tan(PEB_FACET_TILT_DEG.to_radians());
        let cap_g = libm::tan(PEB_MAX_FACET_DEG.to_radians());
        let mut planes = Vec::new();
        for (m, (&u, &r)) in jitter.iter().zip(&rs).enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let t = (m as f64 + u) * tau / kf + turn;
            let (pa, pb) = (a * r * libm::cos(t), a * r * libm::sin(t));
            let sq = libm::sqrt((1.0 - libm::pow(r, pf)).max(1e-3));
            let z0 = hp * sq;
            let dz = hp * pf / 2.0 * libm::pow(r, pf - 2.0) / (a * a * sq);
            let (mut ga, mut gb) = (
                -dz * pa + rng.normal(0.0, tl),
                -dz * pb + rng.normal(0.0, tl),
            );
            let gn = libm::hypot(ga, gb);
            if gn > cap_g {
                ga *= cap_g / gn;
                gb *= cap_g / gn;
            }
            let drop = hp
                * between(
                    rng,
                    if rounded {
                        PEB_ROUND_DROP
                    } else {
                        PEB_FACET_DROP
                    },
                );
            planes.push(Plane {
                z0: z0 - drop - ga * pa - gb * pb,
                gx: ga,
                gy: gb,
            });
        }
        // the polygonal skirt: no side steeper than the skirt angle
        let sides = rng.int(PEB_SKIRT_SIDES.0, PEB_SKIRT_SIDES.1);
        #[allow(clippy::cast_precision_loss)]
        let sf = sides as f64;
        let jit: Vec<f64> = (0..sides).map(|_| rng.uniform(-0.3, 0.3)).collect();
        let turn = rng.uniform(0.0, tau);
        let sk = libm::tan(PEB_SKIRT_DEG.to_radians());
        for (m, j) in jit.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let t = (m as f64 + j) * tau / sf + turn;
            let off = a * rng.uniform(0.82, 1.05) * if rounded { 1.1 } else { 1.0 };
            planes.push(Plane {
                z0: sk * off,
                gx: -sk * libm::cos(t),
                gy: -sk * libm::sin(t),
            });
        }
        Self {
            planes,
            hp,
            rounded,
            pf,
            a,
            el,
            ca: libm::cos(ang),
            sa: libm::sin(ang),
        }
    }

    /// Height (≥ 0) at `(qx, qy)` from its centre, before sinking.
    fn height(&self, qx: f64, qy: f64) -> f64 {
        let qa = (qx * self.ca + qy * self.sa) / self.el;
        let qb = -qx * self.sa + qy * self.ca;
        let mut h = cap(&self.planes, qa, qb);
        if self.rounded {
            let rho2 = (qa * qa + qb * qb) / (self.a * self.a);
            h = h.min(self.hp * libm::sqrt((1.0 - libm::pow(rho2, self.pf / 2.0)).max(0.0)));
        }
        h.max(0.0)
    }
}

/// Clustered half-sunk pebbles stamped into
/// `z`. Returns the pebble mask and counts.
fn place_pebbles(
    frame: &dt_kit::Frame,
    seed: Seed,
    z: &mut Field,
    ground: &Field,
    avoid: &[[f64; 3]],
) -> (Vec<bool>, Counts) {
    let [w, h] = frame.size_mm();
    let area_sq = w * h / (SQUARE_MM * SQUARE_MM);
    let c = cluster_field(frame, seed, ground);
    let mut rng = Rng::new(seed.key("pebbles"));
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let count = |rng: &mut Rng, (lo, hi): (i64, i64)| (rng.int(lo, hi) as f64 * area_sq) as usize;
    let n_med = count(&mut rng, MED_PER_SQ);
    let mut sizes: Vec<f64> = (0..n_med)
        .map(|_| skewed(&mut rng, MED_D_MM, PEB_D_EXP))
        .collect();
    let n_small = count(&mut rng, SMALL_PER_SQ);
    sizes.extend((0..n_small).map(|_| skewed(&mut rng, SMALL_D_MM, PEB_D_EXP)));
    let radii: Vec<f64> = sizes.iter().map(|d| d / 2.0).collect();
    let spread = Spread {
        tries: PEB_TRIES,
        inset: [RIM_TEX_MM, PEB_ASPECT.1],
        confine: true,
        gap: PEB_GAP_MM,
        factor: 1.0,
    };
    let placed = scatter(
        frame,
        &mut rng,
        &radii,
        &spread,
        &|x, y| c.sample(x, y),
        avoid,
    );
    let seat = z.blur(PEB_SEAT_SIGMA_MM);
    let mut mask = vec![false; z.z.len()];
    let mut counts = Counts::default();
    for p in &placed {
        let d = sizes[p.index];
        let [cx, cy] = p.at;
        let mut rng = Rng::new(seed.key("pebble").index(p.index as u64));
        let peb = Pebble::new(&mut rng, d);
        let med = d >= MED_D_MM.0;
        let sink = between(&mut rng, PEB_SINK) * peb.hp;
        let g0 = seat.sample(cx, cy);
        let k_soft = if med { PEB_SOFT_MM.1 } else { PEB_SOFT_MM.0 };
        let reach = 0.5 * d * PEB_ASPECT.1 * 1.6 + 0.3;
        let (is, js) = z.window(cx, cy, reach);
        let mound = 0.5 * d * 1.25;
        for j in js {
            for i in is.clone() {
                let (qx, qy) = (z.x(i) - cx, z.y(j) - cy);
                let k = j * z.nx + i;
                if med {
                    let d2 = (qx * qx + qy * qy) / (mound * mound);
                    z.z[k] += PEB_MOUND * peb.hp * libm::exp(-d2 * 1.4);
                }
                let hh = peb.height(qx, qy);
                let top = if hh > 0.0 { g0 - sink + hh } else { NONE };
                if top > z.z[k] + 0.03 {
                    mask[k] = true;
                }
                z.z[k] = smax(z.z[k], top, k_soft);
            }
        }
        if med {
            counts.medium += 1;
        } else {
            counts.small += 1;
        }
        counts.rounded += usize::from(peb.rounded);
    }
    (mask, counts)
}

impl Element for Dirt {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Surface,
            reads: Vec::new(),
            cell_mm: Some(crate::CellMm::mm(CELL_MM)),
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let (z, stone, counts) = surface(self, scene, ctx.seed, ctx.path, ctx.area, ctx.foots)?;
        scene.paint_field(ctx.area, &z);
        scene.mark_stone(ctx.area, &stone);
        #[allow(clippy::cast_precision_loss)]
        for (k, v) in [
            ("clods", counts.clods),
            ("pebbles_small", counts.small),
            ("pebbles_medium", counts.medium),
            ("pebbles_rounded", counts.rounded),
            ("stones", counts.stones),
        ] {
            scene.note(k, v as f64);
        }
        scene.note("texture_ok", counts.texture_ok);
        Ok(())
    }
}

/// The printable-texture rule (docs/elements/ground.html): the share of the
/// interior (1 mm or more from the rim) whose 1.5 mm window holds at
/// least one 0.2 mm layer of relief. The rule is 97% or more.
#[must_use]
pub fn texture_ok(z: &Field, rim: &Field) -> f64 {
    let range = z.range(TEXTURE_WINDOW_MM);
    let (ok, all) = range
        .z
        .iter()
        .zip(&rim.z)
        .filter(|(_, d)| **d >= RIM_TEX_MM)
        .fold((0usize, 0usize), |(ok, all), (r, _)| {
            (ok + usize::from(*r >= TEXTURE_LAYER_MM), all + 1)
        });
    #[allow(clippy::cast_precision_loss)]
    let share = ok as f64 / all.max(1) as f64;
    share
}
