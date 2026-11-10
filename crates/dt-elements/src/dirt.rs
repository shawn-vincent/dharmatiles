//! `dirt`: bare earth on the [`ground`](crate::ground) shape.
//!
//! One height field, built in scale order and combined by smooth maximum,
//! not by adding (a clod or pebble is a new surface that wins where it is
//! higher, with a soft fillet):
//!
//! 1. the calculator: hills in the dirt's band (the smoothest surface
//!    the band allows, with broad hills over as much of the band as it has
//!    room for), plus a low soil roughness (fBm, 1.6 mm);
//! 2. clods (`clod_layer`, :205): patchy, by a 6 mm patch field with a 30%
//!    floor; each is 1–3 lumpy lobes with a flattish tilted top, partly
//!    sunk, soft-maxed onto the soil;
//! 3. grit (`grit`, :269): fine fBm and sparse sand grains;
//! 4. pebbles (`place_pebbles` :351, `pebble_shape` :300): dart-thrown
//!    into loose clusters biased into low ground, largest first; each a
//!    flattened cap cut by tilted tangent planes and a polygonal skirt
//!    (the half-space cap), sunk 25–45% and filleted into the soil.
//!
//! The texture ramps in over the outer 1 mm, so a pinned rim is exactly at
//! its pin.
//!
//! Sizes: the ground's hills, patches and clusters follow the square
//! (layout); clods, grit and pebbles keep their true mm (default 1), so
//! their counts follow the tile's area.

use dt_kit::OnFrame;
use dt_kit::{
    Band, Banded, Field, Noise, Placed, Plane, Rng, Scale, Scene, Seed, Spread, cap, noise, rim,
    scatter, smax, smooth01,
};

use crate::read::Settings;
use crate::{Control, Ctx, Element, LookError, Plan, Write};

/// The height-field resolution, true mm.
const CELL_MM: f64 = 0.15;
/// How many nodes from the tile's edge the slope's blur is whole.
const SLOPE_EDGE_NODES: usize = 6;
/// A node lower than the nearest lip or steep node by more than this is
/// below it, at the foot of a rise, and keeps no standoff from it.
const RISE_EPS_MM: f64 = 0.1;
/// How steeply dirt lies when nothing holds it, degrees: the easing the
/// solver gives it, and the reach of its hills from a pin.
pub const SLOPE_DEG: f64 = 24.0;
/// The most a hill spans, mm at the reference square: a band wider than
/// this (a bank drawn between two places) is mostly the smooth surface.
const HILL_MM: f64 = 4.0;
/// The default `bumpSize`: a broad scale, mm.
const SIZE_MM: f64 = 14.0;
/// The hills' second noise: its size as a share of the first, and its weight.
const MID_RATIO: f64 = 5.0 / 14.0;
const MID_WEIGHT: f64 = 0.15;
/// Bedded stones per 2×2 (`stones some`).
const STONES_2X2: (i64, i64) = (4, 7);
/// The clods and pebbles keep this far from the rim, true mm.
const RIM_TEX_MM: f64 = 1.0;
/// Dirt's edge-round, mm at the 35 mm square: a drop of the top at the rim,
/// cut from the sharp corner at the edge height.
const EDGE_ROUND_MM: f64 = 0.2;
/// The relief of dirt's sides, mm at the 35 mm square (never under the
/// printer's one line).
const SIDE_RELIEF_MM: f64 = 0.6;
/// Dry ground stays this far over the water's level, mm at the 35 mm square
/// (about one printer layer; it scales with the system with every size, so
/// a system's tiles are the same model scaled): the grain fades out where the
/// water's low end stands within it of the hills, and never takes the surface
/// under it.
const DRY_MARGIN_MM: f64 = 0.2;
/// Below any surface: a lobe or pebble that is not there.
const NONE: f64 = -1e9;
/// The steepest surface dirt stays on, degrees: the default `maxSlope`.
/// Dirt slides off anything steeper and leaves it bare (an angle: it never
/// scales).
pub const MAX_SLOPE_DEG: f64 = 35.0;
/// Dirt does not lie within this of a lip or a slit in the surface under
/// it, mm at the reference square (scaled with the system).
const LIP_INSET_MM: f64 = 1.5;
/// A step between neighbouring nodes this deep in the surface under dirt is
/// a lip, mm at the reference square (scaled).
const LIP_MM: f64 = 1.0;
/// How far the smoothing reaches that measures the slope of the surface
/// under dirt, nodes: the slope of the surface, not of its grain.
const SLOPE_SMOOTH_NODES: f64 = 2.0;
/// A gap this narrow between two places dirt may not lie, nodes, is closed:
/// a slit finer than the lattice hits and misses from node to node, and
/// without it the inset would follow that alternation as a comb.
const SLIT_CLOSE_NODES: f64 = 1.5;
/// How far the outline of the places dirt may not lie is rounded, nodes (the
/// blur's sigma): a lattice staircase or a comb becomes a clean edge.
const EDGE_SMOOTH_NODES: f64 = 4.0;
/// Where dirt lies thinner than this over what is under it, mm at the
/// reference square (scaled), no stone is bedded in it: it would stand clear.
const BED_DEPTH_MM: f64 = 2.0;

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
/// Pebbles are left off ground steeper than this: it fades out between the
/// two angles, measured on the ground blurred by the second value (mm).
const PEB_SIT_DEG: (f64, f64) = (22.0, dt_kit::STEEPEST_GROUND_DEG);
const PEB_SIT_BLUR_MM: f64 = 1.0;
const PEB_SEAT_SIGMA_MM: f64 = 0.5;
const PEB_TRIES: u32 = 60;
/// The printable-texture rule's window and layer, mm.
const TEXTURE_WINDOW_MM: f64 = 1.5;
const TEXTURE_LAYER_MM: f64 = 0.2;
/// The earth on a ground shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Dirt {
    /// The hills' size (the broad noise's sigma), mm at the reference.
    pub size_mm: f64,
    /// Clod attempts per mm² at full patch density (`clods`).
    pub clod_density: f64,
    /// Bedded stones per 2×2 tile, drawn from this range (`stones`).
    pub stones: (i64, i64),
    /// The steepest surface it stays on, degrees (`maxSlope`).
    pub max_slope_deg: f64,
}

impl Dirt {
    /// Whether this dirt has no texture: no clods and no stones (`clods
    /// none stones none`), so no grit or pebbles either.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.clod_density <= 0.0 && self.stones.1 <= 0
    }

    /// From controls: `bumpSize`, `clods` (a word whose number the
    /// evaluator passes as `clods.density`), `stones` and `maxSlope`.
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        let mut clod_density = CLOD_DENSITY;
        let mut size_mm = SIZE_MM;
        let s = Settings::new(controls);
        clod_density = s.number_or("clods.density", clod_density)?;
        size_mm = s.number_or("bumpSize", size_mm)?;
        let max_slope_deg = s.number_or("maxSlope", MAX_SLOPE_DEG)?;
        let mut stones = STONES_2X2;
        // the numbers behind the `stones` word
        #[allow(clippy::cast_possible_truncation)]
        {
            if let Some(v) = s.number("stones.min")? {
                stones.0 = libm::round(v) as i64;
            }
            if let Some(v) = s.number("stones.max")? {
                stones.1 = libm::round(v) as i64;
            }
        }
        stones.1 = stones.1.max(stones.0);
        Ok(Self {
            size_mm,
            clod_density,
            stones,
            max_slope_deg,
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

/// The dirt's calculator: hills in its band, the surface before any grain.
#[must_use]
pub fn hills(dirt: &Dirt, frame: &dt_kit::Frame, seed: Seed, band: &Band) -> dt_kit::span::Hills {
    let s = frame.scale();
    let broad = noise(
        frame,
        seed.key("broad"),
        Noise::Smooth {
            sigma_mm: dirt.size_mm,
        },
        Scale::Layout,
    );
    let mid = noise(
        frame,
        seed.key("mid"),
        Noise::Smooth {
            sigma_mm: dirt.size_mm * MID_RATIO,
        },
        Scale::Layout,
    );
    let n = broad.zip(&mid, |a, b| a + MID_WEIGHT * b);
    dt_kit::span::hills(&n, band, HILL_MM * s, SLOPE_DEG, s)
}

/// What a dirt is made on: its area, its solved band, and where water lies
/// over it.
#[derive(Clone, Copy)]
pub struct Ground<'a> {
    /// The nodes the dirt covers.
    pub area: &'a dt_kit::Mask,
    /// The solved band.
    pub band: &'a Band,
    /// About the height the water laid after it stands at, if any is.
    pub under: Option<&'a Field>,
    /// The printer's skin, mm.
    pub skin_mm: f64,
    /// Where it is a thin layer over what is under it: no stone is bedded
    /// there. `None` for dirt that is not laid over anything.
    pub thin: Option<&'a dt_kit::Mask>,
}

/// What the dirt made on its band: the hills, the grain's relief and where the surface is set in to hold it, the pebble
/// mask and the counts.
pub struct Made {
    /// The hills, inside the band.
    pub shape: Banded,
    /// The grain's relief over the hills, mm.
    pub relief: Field,
    /// Where the surface is set in, 0 to 1: not at the rim, which is held
    /// exactly, and not under water.
    pub weight: Field,
    /// The lowest the grain may take the surface (just over the water that
    /// will lie on it), if water will.
    pub floor: Option<Field>,
    /// The surface nodes that are pebbles.
    pub stone: Vec<bool>,
    /// What was counted.
    pub counts: Counts,
}

/// The dirt as a layer of earth on the land, before its edge-round and its
/// sides: what a stone bedded into the grain asks the scene how far the grain
/// sets in ([`Scene::grain_offset`]), and what the dirt lays ([`Scene::lay`]).
fn earth<'a>(
    owner: &'a str,
    area: &'a dt_kit::Mask,
    shape: &'a Banded,
    grain: &'a dt_kit::Relief<'a>,
    skin_mm: f64,
) -> dt_kit::LayerSpec<'a> {
    dt_kit::LayerSpec {
        owner,
        material: dt_kit::Material::Earth,
        fluid: false,
        mask: area,
        shape,
        grain: Some(grain),
        skin_mm,
        round_mm: 0.0,
        side: None,
    }
}

/// The dirt surface over the whole tile (world mm), its grain, the pebble
/// mask and the counts.
///
/// Its stones (`stones`) are rock solids added to the scene, owned by
/// `owner`, bedded into this field in its area.  The grain is what the
/// texture adds to the hills (and, round a stone, takes away).
///
/// # Errors
/// [`LookError`] if a stone fails to build.
pub fn surface(
    dirt: &Dirt,
    scene: &mut Scene,
    seed: Seed,
    owner: &str,
    on: &Ground<'_>,
) -> Result<Made, LookError> {
    let Ground {
        area,
        band,
        under,
        skin_mm,
        thin,
    } = *on;
    let frame = *scene.frame();
    let dt_kit::span::Hills { shape, converged } = hills(dirt, &frame, seed.key("hills"), band);
    if !converged {
        // the surface is still in its band; the plate under it did not settle
        scene.note("relax_unconverged", 1.0);
    }
    if dirt.is_plain() {
        // `clods none` and `stones none`: the hills alone, no grain
        let none = Field::new(&frame, 0.0);
        return Ok(Made {
            shape,
            relief: none.clone(),
            weight: none.clone(),
            floor: None,
            stone: vec![false; none.z.len()],
            counts: Counts::default(),
        });
    }
    let rim = rim(&frame);
    let base = shape.field().clone();
    // grain stays off what water covers: where the water's low end stands
    // over the hills, the texture fades out (a pool's bed is smooth)
    let keep = under.map_or_else(
        || Field::new(&frame, 1.0),
        |u| {
            let s = frame.scale();
            base.zip(u, |b, w| {
                1.0 - smooth01((w - b - DRY_MARGIN_MM * s) / (0.8 * s))
            })
        },
    );
    let stone_area = dt_kit::Mask {
        nx: area.nx,
        ny: area.ny,
        bits: (0..area.bits.len())
            .map(|k| area.bits[k] && keep.z[k] > 0.5 && !thin.is_some_and(|t| t.bits[k]))
            .collect(),
    };
    // the grain runs on to the rim and over the edge-round: no fade
    let w_rim = rim.map(|_| 1.0);
    let w_tex = w_rim.zip(&keep, |w, k| w * k);
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
    let (avoid, bedded) = if dirt.stones.1 > 0 {
        crate::rock::dirt_stones(scene, seed, &mut z, &rim, dirt.stones, &stone_area)?
    } else {
        (Vec::new(), Vec::new())
    };
    let (mask, mut counts) = place_pebbles(&frame, seed, &mut z, &ground, &avoid, &keep, &w_rim);
    counts.clods = clods;
    counts.stones = avoid.len();
    counts.texture_ok = texture_ok(&z, &rim);
    let relief = z.with_z((0..z.z.len()).map(|k| z.z[k] - base.z[k]).collect());
    let weight = keep.clone();
    // dry ground stays dry: the grain never takes it under the water's level
    let floor = under.map(|u| u.map(|w| w + DRY_MARGIN_MM * frame.scale()));
    // the grain sets the surface in, so a bedded stone sits as far lower
    if !bedded.is_empty() {
        let grain = dt_kit::Relief {
            relief: &relief,
            weight: &weight,
            floor: floor.as_ref(),
        };
        let off = scene.grain_offset(&earth(owner, area, &shape, &grain, skin_mm));
        for b in bedded {
            let lower = off.sample(b.at[0], b.at[1]);
            b.add(scene, owner, lower);
        }
    }
    Ok(Made {
        shape,
        relief,
        weight,
        floor,
        stone: mask,
        counts,
    })
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
    let mut bumps = g.map(|_| 0.0_f64);
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
    keep: &Field,
    w_rim: &Field,
) -> (Vec<bool>, Counts) {
    let [w, h] = frame.size_mm();
    let area_sq = w * h / (dt_kit::REFERENCE_SQUARE_MM * dt_kit::REFERENCE_SQUARE_MM);
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
    // a pebble sits on ground it can lie on: none on a face steeper than
    // PEB_SIT_DEG (the ground seen over about a pebble's width)
    let (gx, gy) = ground.blur(PEB_SIT_BLUR_MM).gradient();
    let (lo_t, hi_t) = (
        libm::tan(PEB_SIT_DEG.0.to_radians()),
        libm::tan(PEB_SIT_DEG.1.to_radians()),
    );
    let sits = gx.zip(&gy, |a, b| {
        1.0 - smooth01((libm::hypot(a, b) - lo_t) / (hi_t - lo_t))
    });
    let placed = scatter(
        frame,
        &mut rng,
        &radii,
        &spread,
        &|x, y| c.sample(x, y) * keep.sample(x, y) * sits.sample(x, y),
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
                    // none at the rim: a mound's tail must not lift a pinned edge
                    z.z[k] += w_rim.z[k] * PEB_MOUND * peb.hp * libm::exp(-d2 * 1.4);
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

/// A dirt laid over what is under it: where it stays and the band it fills.
struct Held {
    area: dt_kit::Mask,
    band: Band,
    /// Where it is a thin layer over what is under it.
    thin: dt_kit::Mask,
}

/// Where `ctx`'s dirt lies on the surface under it, the same for every dirt.
///
/// The surface under it is what the scene has laid there (land, a cliff's
/// stones, a floor's slabs: [`Scene::laid_surface`]), uneven as it is. The
/// dirt's band is measured from it: `bottom + [0.6 .. 1.2]` is 0.6 to 1.2 mm
/// over the surface, wherever that stands. The dirt stays only where that
/// surface is flatter than `maxSlope` and does not drop away within the lip
/// inset: it slides off a steep face and does not bridge a slit or hang over
/// an edge. `None` when nothing is laid under it (nothing is under it but
/// its own band); a band that cannot be laid over the surface is counted as
/// left out.
fn hold(
    scene: &Scene,
    ctx: &Ctx<'_>,
    band: &Band,
    max_slope_deg: f64,
    left_out: &mut usize,
) -> Option<Held> {
    let under = scene.laid_surface()?;
    let below = ctx.below?;
    let frame = scene.frame();
    let (nx, ny) = frame.nodes();
    let n = nx * ny;
    let scale = frame.scale();
    if !(0..n).any(|k| ctx.area.bits[k] && under.z[k].is_finite()) {
        return None;
    }
    let (lo, hi) = (band.lo(), band.hi());
    let (mut flo, mut fhi) = (lo.clone(), hi.clone());
    // the surface it lies on: what is laid, and where nothing is, what its
    // band was measured from
    let mut surface = below.lo().clone();
    let mut thin = dt_kit::Mask::new(nx, ny, false);
    for k in 0..n {
        let u = under.z[k];
        if !u.is_finite() {
            continue;
        }
        surface.z[k] = u;
        if ctx.area.bits[k] {
            flo.z[k] = u + (lo.z[k] - below.lo().z[k]);
            fhi.z[k] = u + (hi.z[k] - below.hi().z[k]);
            thin.bits[k] = fhi.z[k] - u < BED_DEPTH_MM * scale;
        }
    }
    let area = held_area(frame, ctx.area, &surface, max_slope_deg);
    let band = Band::new(flo, fhi).map_err(|_| *left_out += 1).ok()?;
    Some(Held { area, band, thin })
}

/// `area` less where the surface is steeper than `max_slope_deg`, is a lip
/// (a node with a neighbour more than a step lower: dirt there would hang
/// over an edge) or lies within the lip inset of a lip, on the lip's own side
/// of it: the same rule everywhere. Only a drop counts. A rise (the foot of a
/// stone, where the surface goes up) is no lip, so dirt banks right up to it.
fn held_area(
    frame: &dt_kit::Frame,
    area: &dt_kit::Mask,
    surface: &Field,
    max_slope_deg: f64,
) -> dt_kit::Mask {
    let (nx, ny) = frame.nodes();
    let (step, scale) = (frame.step_mm(), frame.scale());
    let (gx, gy) = surface.blur(SLOPE_SMOOTH_NODES * step).gradient();
    let (lip_mm, inset_mm) = (LIP_MM * scale, LIP_INSET_MM * scale);
    let tan = libm::tan(max_slope_deg.to_radians());
    let z = &surface.z;
    let rise_mm = RISE_EPS_MM * scale;
    // `bad`: a lip or a steep face, each with ground lower beside it; dirt
    // stands back from these, on their own side
    let mut bad = vec![false; nx * ny];
    let mut tops = vec![false; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let k = j * nx + i;
            let differs = |mm: f64, up: bool| {
                [(-1, 0), (1, 0), (0, -1), (0, 1)]
                    .into_iter()
                    .any(|(di, dj): (isize, isize)| {
                        let (ni, nj) = (i.wrapping_add_signed(di), j.wrapping_add_signed(dj));
                        ni < nx && nj < ny && {
                            let d = z[k] - z[nj * nx + ni];
                            if up { -d > mm } else { d > mm }
                        }
                    })
            };
            let lip = differs(lip_mm, false);
            // the blur is lopsided within its reach of the tile's edge: there the
            // slope is the nearest whole-blur node's
            let m = SLOPE_EDGE_NODES;
            let c = (i.clamp(m, nx - 1 - m), j.clamp(m, ny - 1 - m));
            let kc = c.1 * nx + c.0;
            // a steep node counts only where the surface is not flat beside
            // it: the blur carries steepness a little way past the foot of a
            // rise, onto flat ground that is no part of the face
            let steep = libm::hypot(gx.z[kc], gy.z[kc]) > tan
                && (differs(rise_mm, false) || differs(rise_mm, true));
            bad[k] = lip || steep;
            // dirt stands back from what has ground lower beside it
            tops[k] = bad[k] && (lip || differs(rise_mm, false));
        }
    }
    if !bad.iter().any(|&b| b) {
        return area.clone();
    }
    let closed = close(frame, &bad, SLIT_CLOSE_NODES * step);
    let bad: Vec<bool> = bad.iter().zip(&closed).map(|(&a, &b)| a || b).collect();
    let (to_lip, near) = dt_kit::distance(frame, &tops);
    let zone = Field::new(frame, 0.0)
        .with_z(
            (0..nx * ny)
                .map(|k| {
                    // within the inset of a lip, unless it is the lip's
                    // foot: lower than it by more than a step
                    let beside =
                        to_lip.z[k] < inset_mm && near[k].is_some_and(|l| z[l] <= z[k] + rise_mm);
                    f64::from(u8::from(bad[k] || beside))
                })
                .collect(),
        )
        .blur(EDGE_SMOOTH_NODES * step);
    dt_kit::Mask {
        nx,
        ny,
        bits: (0..nx * ny)
            .map(|k| area.bits[k] && zone.z[k] < 0.5 && !bad[k])
            .collect(),
    }
}

/// `mask` closed by `r_mm`: grown by it, then shrunk by it, so a gap narrower
/// than twice that between two set places is filled and the rest is as it was.
fn close(frame: &dt_kit::Frame, mask: &[bool], r_mm: f64) -> Vec<bool> {
    let (d, _) = dt_kit::distance(frame, mask);
    let grown: Vec<bool> = d.z.iter().map(|&v| v <= r_mm).collect();
    let rest: Vec<bool> = grown.iter().map(|&b| !b).collect();
    let (d, _) = dt_kit::distance(frame, &rest);
    d.z.iter().map(|&v| v > r_mm).collect()
}

impl Element for Dirt {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Surface,
            // it lies on what the stack laid before it, whatever that is
            reads: vec![crate::Read::Laid],
            cell_mm: Some(crate::CellMm::mm(CELL_MM)),
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let Some(band) = ctx.band else {
            return Err(LookError::general(
                "this dirt has no solved top",
                "write the tile in the heights language: `dirt` takes the library's top, or `dirt top=…`",
            ));
        };
        // dirt lies on the surface under it, where that is flat enough
        let skin_mm = 3.0 * ctx.printer.layer_mm;
        let mut left_out = 0;
        let held = hold(scene, ctx, band, self.max_slope_deg, &mut left_out);
        scene.left_out("dirt", left_out);
        if left_out > 0
            || held
                .as_ref()
                .is_some_and(|h| !h.area.bits.iter().any(|&b| b))
        {
            // a band that cannot be laid over the surface, or no surface
            // flat enough for dirt to stay on
            return Ok(());
        }
        let (area, band, thin) = held.as_ref().map_or((ctx.area, band, None), |h| {
            (&h.area, &h.band, Some(&h.thin))
        });
        let on = Ground {
            area,
            band,
            under: ctx.under,
            skin_mm,
            thin,
        };
        let Made {
            shape,
            relief,
            weight,
            floor,
            stone,
            counts,
        } = surface(self, scene, ctx.seed, ctx.path, &on)?;
        let grain = dt_kit::Relief {
            relief: &relief,
            weight: &weight,
            floor: floor.as_ref(),
        };
        scene
            .lay(&dt_kit::LayerSpec {
                round_mm: EDGE_ROUND_MM * scene.frame().scale(),
                side: Some(dt_kit::SideSpec {
                    depth_mm: (SIDE_RELIEF_MM * scene.frame().scale()).max(ctx.printer.line_mm),
                    seed: ctx.seed.key("side"),
                }),
                ..earth(ctx.path, area, &shape, &grain, 3.0 * ctx.printer.layer_mm)
            })
            .map_err(LookError::build)?;
        scene.mark_stone(area, &stone);
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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    use super::*;
    use dt_kit::Frame;

    /// A surface over a 1x1 tile, `z(x, y)` mm, and the whole tile as the area.
    fn on(z: impl Fn(f64, f64) -> f64) -> (Frame, dt_kit::Mask, Field) {
        let frame = Frame::dungeonblocks(1, 1);
        let (nx, ny) = frame.nodes();
        let step = frame.step_mm();
        let surface = Field::new(&frame, 0.0).with_z(
            (0..nx * ny)
                .map(|k| z((k % nx) as f64 * step, (k / nx) as f64 * step))
                .collect(),
        );
        (frame, dt_kit::Mask::new(nx, ny, true), surface)
    }

    fn count(m: &dt_kit::Mask) -> usize {
        m.bits.iter().filter(|&&b| b).count()
    }

    #[test]
    fn dirt_stays_on_a_surface_flatter_than_its_max_slope() {
        let ramp = |deg: f64| move |x: f64, _: f64| x * libm::tan(deg.to_radians());
        let (frame, area, flat) = on(ramp(25.0));
        assert_eq!(count(&held_area(&frame, &area, &flat, 35.0)), count(&area));
        // the same ground, steeper than dirt may lie: bare
        let (frame, area, steep) = on(ramp(45.0));
        assert_eq!(count(&held_area(&frame, &area, &steep, 35.0)), 0);
    }

    #[test]
    fn max_slope_changes_how_much_of_a_surface_holds_dirt() {
        // 25 degrees on the west half, 50 on the east
        let two = |x: f64, _: f64| {
            let (a, b) = (libm::tan(25f64.to_radians()), libm::tan(50f64.to_radians()));
            let mid = 17.5;
            if x < mid {
                x * a
            } else {
                mid * a + (x - mid) * b
            }
        };
        let (frame, area, surface) = on(two);
        let at = |deg: f64| count(&held_area(&frame, &area, &surface, deg));
        let (low, mid, high) = (at(20.0), at(35.0), at(60.0));
        assert_eq!(low, 0, "nothing is flatter than 20 degrees");
        assert!(mid > 0 && mid < count(&area), "the west half holds: {mid}");
        assert_eq!(high, count(&area), "all of it is under 60 degrees");
    }

    #[test]
    fn dirt_stands_back_from_a_lip_and_is_the_same_on_every_surface() {
        // flat, with a 3 mm step down at x = 17.5 mm
        let (frame, area, surface) = on(|x, _| if x < 17.5 { 10.0 } else { 7.0 });
        let held = held_area(&frame, &area, &surface, 35.0);
        let (nx, ny) = frame.nodes();
        let step = frame.step_mm();
        let j = ny / 2;
        let row = |i: usize| held.bits[j * nx + i];
        // far from the lip on both sides it lies; at the lip and within the
        // inset of it on the high side, it does not
        assert!(row((10.0 / step) as usize) && row((25.0 / step) as usize));
        assert!(
            !row((17.5 / step) as usize - 1),
            "the lip itself is not held"
        );
        assert!(!row(((17.5 - 1.0) / step) as usize));
        // the low side is the foot of a rise: dirt banks up to it
        assert!(row(((17.5 + 1.0) / step) as usize));
        // and a smooth surface is left whole
        let (frame, area, flat) = on(|_, _| 10.0);
        assert_eq!(count(&held_area(&frame, &area, &flat, 35.0)), count(&area));
        let _ = ny;
    }

    #[test]
    fn dirt_banks_against_a_foot_and_stands_back_from_a_lip() {
        // a block 3 mm high from x = 12 to 24 mm: its west foot at 12, its
        // east foot at 24, its lips on top at the same x
        let (frame, area, surface) = on(|x, _| if (12.0..24.0).contains(&x) { 3.0 } else { 0.0 });
        let held = held_area(&frame, &area, &surface, 35.0);
        let (nx, ny) = frame.nodes();
        let step = frame.step_mm();
        let j = ny / 2;
        let row = |x: f64| held.bits[j * nx + (x / step).round() as usize];
        // the node just off the foot is held; the block's rim is not, nor the
        // top within the inset of it, but the middle of the top is
        assert!(row(12.0 - 2.0 * step), "the foot is held");
        assert!(row(24.0 + 2.0 * step), "the east foot is held");
        assert!(!row(12.0 + step) && !row(24.0 - step), "the lip is not");
        assert!(!row(12.0 + 1.0), "the top within the inset is not");
        assert!(row(18.0), "the middle of the top is held");
    }
}
