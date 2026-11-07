//! Water: swell-chaos, the loved water.
//!
//! The water fills its area up to its level wherever the ground is lower,
//! and is lifted with the ground as one surface whose top over the water
//! is the water material. Its relief is waves sent out by emitters:
//!
//! - the **land** (its border with the water) sends broad rolling swells
//!   (5–16 mm) that wrap round rocks: distances are geodesic through the
//!   water, and their corners round off with distance;
//! - each **rock** standing in the water sends short chop (2–4.5 mm), with
//!   a wake on the side away from the land;
//! - open **corners** of the tile (or far emitters, when fewer than two
//!   corners are open) send weak long swell.
//!
//! Near a strong emitter its own waves rule (blend exponent 8); far from
//! every physical emitter the waves hand over to an interference "chaos"
//! of all of them. Troughs are compressed softly above −1.1 mm (the bed is
//! 2.2 mm down), a small lip rises at every shore, and the relief fades to
//! calm within 4 mm of the tile edge, where the shared tide profile takes
//! over: the edge contract, `level + 0.6·sin(2π(s − org)/λ)` along every
//! edge, λ half a square, crests at square corners and edge centres, so any
//! two water tiles butt with the same shape.
//!
//! Built at the scene's grid (0.2 mm); two quirks are kept on purpose: its geodesic
//! distances are 1.25 times true (it weighs 0.2 mm steps as 0.25 mm), and
//! its noise is Gaussian-filtered white noise of the same sigmas. The
//! final 0.06 mm blur is below the grid and is left out.

use dt_kit::{Field, Noise, Rng, Scale, Scene, Seed, distance, geodesic, noise, rim, smooth01};

use crate::read;
use crate::{Control, Ctx, Element, LookError, Plan, Read, Write, only};

/// The controls `water` takes.
pub const CONTROLS: &[&str] = &["level"];

const CELL_MM: f64 = 0.2;
const BANK_GRID_WEIGHT: f64 = 1.25;
/// Deviation from thresholding the raw ground: the land
/// mask is taken from the ground blurred this much, so pebbles and grain
/// on a bank near the water line don't speckle the shore with one-node
/// islands and pools.
const LAND_BLUR_MM: f64 = 0.3;
const RIM_FADE_MM: f64 = 4.0;
const DIST_SMOOTH_MM: f64 = 1.0;
const TIDE_SIGMA_MM: f64 = 9.0;
const TIDE_RMS_MM: f64 = 0.35;
/// The edge contract's tide amplitude, mm at the reference square.
pub const TIDE_EDGE_AMP_MM: f64 = 0.6;
const TIDE_EDGE_FADE_MM: f64 = 9.0;
const LIP_MM: f64 = 0.3;
const LIP_D_MM: f64 = 0.6;
/// The relief fades to calm over this much true distance from the shore.
/// Fading over 0.4 mm of smoothed distance, which is already
/// about 0.6 mm at the first water cell, so a crest could meet the bank
/// as a step; over true distance the water runs up to the bank.
const SHORE_FADE_MM: f64 = 1.0;
// emitters: strength, decay mm
const LAND: (f64, f64) = (1.0, 34.0);
const ROCK: (f64, f64) = (0.55, 10.0);
const CORNER: (f64, f64) = (0.30, 70.0);
const FAR_SRC_N: usize = 3;
const FAR_SRC_MM: f64 = 220.0;
const EMIT_WOBBLE_MM: f64 = 1.0;
const EMIT_SHARP: f64 = 8.0;
const CHAOS_WOBBLE_GROW: f64 = 0.06;
const CHAOS_ORDER: (f64, f64) = (0.12, 0.45);
const CHAOS_FAR_AMP: f64 = 0.7;
// swell bands: λ lo, λ hi, growth /mm; components
const BAND_LAND: ((f64, f64, f64), usize) = ((5.0, 16.0, 0.012), 4);
const BAND_ROCK: ((f64, f64, f64), usize) = ((2.0, 4.5, 0.035), 3);
const BAND_CORNER: ((f64, f64, f64), usize) = ((5.0, 14.0, 0.0), 3);
const SWELL_AMP_EXP: f64 = 1.4;
const SWELL_LONG_SHARPEN: f64 = 0.15;
const SWELL_SHORT_SHARPEN: f64 = 0.9;
const SWELL_AMP_MM: f64 = 0.8;
const SWELL_ROCK_GAIN: f64 = 1.15;
const SWELL_WAKE: f64 = 0.9;
/// The deepest trough below calm water, mm (a soft floor).
pub const SWELL_TROUGH_MM: f64 = 1.1;
const ENCLOSE_FREE: f64 = 0.4;
const ENCLOSE_CUT: f64 = 0.75;
const ROUND_K: f64 = 0.45;
const ROUND_SIGMAS_MM: [f64; 4] = [2.0, 4.0, 8.0, 16.0];

/// The `water` element.
pub struct Water {
    level_mm: f64,
}

impl Water {
    /// From its controls.
    ///
    /// # Errors
    /// [`LookError`] for a bad level.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("water", controls, CONTROLS)?;
        let mut level_mm = f64::NAN;
        for c in controls {
            if c.name == "level" {
                level_mm = read::number(c)?;
            }
        }
        if level_mm.is_nan() {
            return Err(LookError::control(
                "level",
                "water needs a level".into(),
                "write level=water".into(),
            ));
        }
        Ok(Self { level_mm })
    }
}

/// One emitter: its strength, decay (world mm), distance field, kind.
struct Source {
    strength: f64,
    decay: f64,
    d: Field,
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Land,
    Rock,
    Corner,
}

fn mean_std(v: &[f64]) -> (f64, f64) {
    #[allow(clippy::cast_precision_loss)]
    let n = v.len().max(1) as f64;
    let m = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / n;
    (m, libm::sqrt(var))
}

/// Normalised convolution inside the water only.
struct Smoother<'a> {
    wm: &'a Field,
    cache: Vec<(f64, Field)>,
}

impl Smoother<'_> {
    fn smooth(&mut self, a: &Field, sigma: f64) -> Field {
        let at = if let Some(i) = self
            .cache
            .iter()
            .position(|(s, _)| s.total_cmp(&sigma).is_eq())
        {
            i
        } else {
            let f = self.wm.blur(sigma).map(|v| v.max(1e-6));
            self.cache.push((sigma, f));
            self.cache.len() - 1
        };
        a.zip(self.wm, |x, w| x * w)
            .blur(sigma)
            .zip(&self.cache[at].1, |x, w| x / w)
    }
}

/// `_round_offsets`: iso-line corners round off with distance.
fn round_offsets(dd: &Field, sm: &mut Smoother<'_>, s: f64) -> Field {
    let mut levels = vec![0.0];
    let mut fields = vec![dd.clone()];
    for sg in ROUND_SIGMAS_MM {
        levels.push(sg * s);
        fields.push(sm.smooth(dd, sg * s));
    }
    let mut out = vec![0.0; dd.z.len()];
    for (k, o) in out.iter_mut().enumerate() {
        let want = ROUND_K * dd.z[k];
        let (mut acc, mut ws) = (0.0, 0.0);
        for (lv, f) in levels.iter().zip(&fields) {
            let w = libm::exp(-((want - lv) / (0.5 * lv.max(2.0 * s))).powi(2));
            acc += w * f.z[k];
            ws += w;
        }
        *o = acc / ws.max(1e-9);
    }
    dd.with_z(out)
}

/// `_swell_train`: long round swells plus sharpened short riders, unit RMS.
fn swell_train(
    frame: &dt_kit::Frame,
    seed: Seed,
    rng: &mut Rng,
    d: &Field,
    band: ((f64, f64, f64), usize),
    wobble_grow: f64,
    s: f64,
) -> Field {
    let ((lo, hi, g), n) = band;
    let (lo, hi, g) = (lo * s, hi * s, g / s);
    let split = libm::sqrt(lo * hi);
    let mut long = vec![0.0; d.z.len()];
    let mut short = vec![0.0; d.z.len()];
    for c in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let t = if n > 1 {
            c as f64 / (n - 1) as f64
        } else {
            0.0
        };
        let l = libm::exp(libm::log(lo) + t * (libm::log(hi) - libm::log(lo)))
            * libm::exp(rng.uniform(-0.1, 0.1));
        let nz = noise(
            frame,
            seed.index(c as u64),
            Noise::Smooth {
                sigma_mm: (1.2 * l / s).max(2.5),
            },
            Scale::Layout,
        );
        let a = libm::pow(l / hi, SWELL_AMP_EXP);
        let ph0 = rng.uniform(0.0, std::f64::consts::TAU);
        let wl = libm::pow(l / hi, -0.3);
        let out = if l >= split { &mut long } else { &mut short };
        for (k, o) in out.iter_mut().enumerate() {
            let dk = d.z[k];
            let wob = EMIT_WOBBLE_MM * s + wobble_grow * dk.min(80.0 * s) * wl;
            let dw = (dk + wob * nz.z[k]).max(0.0);
            let phase = if g > 0.0 {
                libm::log1p(g * dw) / (g * l)
            } else {
                dw / l
            };
            *o += a * libm::cos(std::f64::consts::TAU * phase + ph0);
        }
    }
    let sharp = |u: &[f64], k: f64| -> Vec<f64> {
        let (_, sd) = mean_std(u);
        let sd = sd + 1e-12;
        u.iter()
            .map(|&x| {
                let v = 2.5 * libm::tanh(x / sd / 2.5);
                (v + k * 0.5 * (v * v - 1.0)) * sd
            })
            .collect()
    };
    let a = sharp(&long, SWELL_LONG_SHARPEN);
    let b = sharp(&short, SWELL_SHORT_SHARPEN);
    let out: Vec<f64> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
    let (m, sd) = mean_std(&out);
    d.with_z(out.iter().map(|x| (x - m) / (sd + 1e-12)).collect())
}

/// Whether node `k` is on the tile's perimeter.
const fn on_edge(k: usize, nx: usize, ny: usize) -> bool {
    let (i, j) = (k % nx, k / nx);
    i == 0 || j == 0 || i == nx - 1 || j == ny - 1
}

/// Where perimeter node `k` lies along its edge (mm): x on the south and
/// north edges, y on the west and east.
fn along_edge(f: &Field, k: usize) -> f64 {
    let (i, j) = (k % f.nx, k / f.nx);
    if j == 0 || j == f.ny - 1 {
        f.x(i)
    } else {
        f.y(j)
    }
}

/// Nodes within one node of a `m` node.
fn dilate4(m: &[bool], nx: usize, ny: usize) -> Vec<bool> {
    (0..nx * ny)
        .map(|k| {
            let (i, j) = (k % nx, k / nx);
            m[k] || (i > 0 && m[k - 1])
                || (i + 1 < nx && m[k + 1])
                || (j > 0 && m[k - nx])
                || (j + 1 < ny && m[k + nx])
        })
        .collect()
}

/// The tide: interior swell noise, forced to the edge profile at the
/// tile edges (`tide_field` :375).
fn tide(frame: &dt_kit::Frame, seed: Seed) -> Field {
    let s = frame.scale();
    let [w, h] = frame.size_mm();
    let inner = noise(
        frame,
        seed,
        Noise::Smooth {
            sigma_mm: TIDE_SIGMA_MM,
        },
        Scale::Layout,
    );
    Field::from_fn(frame, |x, y| {
        let (dx, dy) = (x.min(w - x), y.min(h - y));
        let (wy, wx) = (1.0 / dy.max(1e-6), 1.0 / dx.max(1e-6));
        let edge = (wy * profile(frame, x) + wx * profile(frame, y)) / (wy + wx);
        let b = edge_fade(s, dx.min(dy));
        (1.0 - b) * TIDE_RMS_MM * s * inner.sample(x, y) + b * edge
    })
}

/// How far the edge contract holds at `d` mm from the nearest tile edge:
/// 1 at the edge, fading to 0 over the tide's edge fade.
fn edge_fade(s: f64, d: f64) -> f64 {
    smooth01(1.0 - d / (TIDE_EDGE_FADE_MM * s))
}

/// The edge contract's tide profile at position `t` (mm) along an edge.
#[must_use]
pub fn profile(frame: &dt_kit::Frame, t: f64) -> f64 {
    let lam = frame.square_mm / 2.0;
    let org = frame.square_mm * 0.5 - lam * 0.25;
    TIDE_EDGE_AMP_MM * frame.scale() * libm::sin(std::f64::consts::TAU * (t - org) / lam)
}

/// What the build measured (its notes).
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// Relief over open water (rim > 0.99): min, max, rms.
    pub relief: (f64, f64, f64),
    /// The steepest water slope, degrees.
    pub max_slope_deg: f64,
    /// The worst wet edge node's miss from the contract, mm.
    pub edge_err_mm: f64,
    /// Wet share of the tile.
    pub wet: f64,
}

impl Element for Water {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Fill,
            reads: vec![Read::IfAny(dt_kit::Kind::Rock)],
            cell_mm: Some(crate::CellMm::mm(CELL_MM)),
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let frame = *scene.frame();
        let s = frame.scale();
        let wl = self.level_mm * s;
        let (nx, ny) = frame.nodes();
        let nn = nx * ny;
        scene.bank();
        let land = scene
            .ground()
            .unwrap_or_else(|| Field::new(&frame, wl - 2.2 * s));
        let rock_mask = scene.footprint(dt_kit::Kind::Rock, wl).bits;
        let soft = land.blur(LAND_BLUR_MM);
        let land_mask: Vec<bool> = (0..nn)
            .map(|k| (soft.z[k] > wl || !ctx.area.bits[k]) && !rock_mask[k])
            .collect();
        let water: Vec<bool> = (0..nn).map(|k| !land_mask[k] && !rock_mask[k]).collect();
        if !water.iter().any(|&b| b) {
            return Err(LookError::build(
                "the water has nowhere to go: the ground is above its level everywhere in its area",
            ));
        }
        let wm =
            Field::new(&frame, 0.0).with_z(water.iter().map(|&w| f64::from(u8::from(w))).collect());
        let mut sm = Smoother {
            wm: &wm,
            cache: Vec::new(),
        };
        // distance to the shore: true (for the shore fade) and smoothed
        let obst: Vec<bool> = water.iter().map(|w| !w).collect();
        let (raw_d, d) = if obst.iter().any(|&b| b) {
            let raw = distance(&frame, &obst).0;
            let smoothed = sm.smooth(&raw, DIST_SMOOTH_MM * s);
            let d = smoothed.with_z(
                (0..nn)
                    .map(|k| if water[k] { smoothed.z[k] } else { 0.0 })
                    .collect(),
            );
            (raw, d)
        } else {
            (Field::new(&frame, 1e3), Field::new(&frame, 1e3))
        };
        let rim_f = rim(&frame).map(|r| smooth01(r / (RIM_FADE_MM * s)));
        let seed = ctx.seed;
        let mut rng = Rng::new(seed.key("swell"));
        let masks = Masks {
            land: &land_mask,
            rock: &rock_mask,
            water: &water,
        };
        let sources = emitters(&frame, &land, &masks, &mut sm, &mut rng);
        let z = relief(&frame, seed, &mut rng, &sources, &water, &d);
        // the surface: water over the ground where it is higher
        let tide = tide(&frame, seed.key("tide"));
        let edge_wl = scene.water_edge().unwrap_or(wl);
        // a water level off the edge contract's eases to it near the edges
        let [tw, th] = frame.size_mm();
        let level = Field::from_fn(&frame, |x, y| {
            let d = x.min(tw - x).min(y).min(th - y);
            wl + edge_fade(s, d) * (edge_wl - wl)
        });
        let mut top = vec![0.0; nn];
        let mut wet = vec![false; nn];
        for k in 0..nn {
            let wz = if on_edge(k, nx, ny) && water[k] {
                // the contract, exactly
                edge_wl + profile(&frame, along_edge(&land, k))
            } else {
                let fade = smooth01(raw_d.z[k] / (SHORE_FADE_MM * s));
                level.z[k] + z[k] * rim_f.z[k] * f64::from(u8::from(water[k])) * fade + tide.z[k]
            };
            wet[k] = water[k] && wz >= land.z[k];
            top[k] = if ctx.area.bits[k] {
                wz.max(land.z[k])
            } else {
                land.z[k]
            };
        }
        // the ground's edge rule leaves the water and its banks alone
        let (dw, _) = distance(&frame, &wet);
        let bank = dt_kit::BANK_MM * s;
        let no_edge: Vec<bool> = (0..nn).map(|k| wet[k] || dw.z[k] < bank).collect();
        let top_f = land.with_z(top);
        let st = stats(&frame, &top_f, &z, &water, &wet, &rim_f, edge_wl);
        scene.fill(&top_f, &wet, &no_edge);
        for (k, v) in [
            ("relief_min", st.relief.0),
            ("relief_max", st.relief.1),
            ("relief_rms", st.relief.2),
            ("max_slope_deg", st.max_slope_deg),
            ("edge_err_mm", st.edge_err_mm),
            ("wet", st.wet),
        ] {
            scene.note(k, v);
        }
        Ok(())
    }
}

/// The water's masks: land (higher ground and outside its area), rocks
/// standing in it, and the water itself.
struct Masks<'a> {
    land: &'a [bool],
    rock: &'a [bool],
    water: &'a [bool],
}

/// The emitters: the land and the rocks (geodesic through the water), the
/// open corners, and far sources when fewer than two corners are open.
/// Physical emitters first, then the corners and far sources.
fn emitters(
    frame: &dt_kit::Frame,
    land: &Field,
    m: &Masks<'_>,
    sm: &mut Smoother<'_>,
    rng: &mut Rng,
) -> Vec<Source> {
    let s = frame.scale();
    let (nx, ny) = frame.nodes();
    let nn = nx * ny;
    let mut sources: Vec<Source> = Vec::new();
    let wdil = dilate4(m.water, nx, ny);
    for (kind, src, (st, dec)) in [(Kind::Land, m.land, LAND), (Kind::Rock, m.rock, ROCK)] {
        if !src.iter().any(|&b| b) {
            continue;
        }
        let border: Vec<bool> = (0..nn).map(|k| src[k] && wdil[k]).collect();
        if !border.iter().any(|&b| b) {
            continue;
        }
        let g = geodesic(frame, &border, m.water, BANK_GRID_WEIGHT).map(|v| v.min(200.0 * s));
        let mut dd = sm.smooth(&g, 1.2 * s);
        if kind == Kind::Land {
            dd = round_offsets(&dd, sm, s);
        }
        sources.push(Source {
            strength: st,
            decay: dec * s,
            d: dd,
            kind,
        });
    }
    // open corners
    let [w, h] = frame.size_mm();
    let mut corners = Vec::new();
    let r4 = 2.0 * s; // ±20 cells at 0.1 mm
    for (cx, cy) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
        let (mut wet, mut all) = (0usize, 0usize);
        for (k, &wk) in m.water.iter().enumerate() {
            let (x, y) = (land.x(k % nx), land.y(k / nx));
            if (x - cx).abs() < r4 && (y - cy).abs() < r4 {
                all += 1;
                wet += usize::from(wk);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        if wet as f64 > 0.5 * all as f64 {
            let st = CORNER.0 * rng.uniform(0.8, 1.2);
            corners.push(Source {
                strength: st,
                decay: CORNER.1 * s,
                d: Field::from_fn(frame, |x, y| libm::hypot(x - cx, y - cy)),
                kind: Kind::Corner,
            });
        }
    }
    // enclosure: the share of the tile's perimeter that is land
    let perim: Vec<bool> = (0..nn)
        .filter(|&k| on_edge(k, nx, ny))
        .map(|k| m.land[k])
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let f_enc = ((perim.iter().filter(|&&b| b).count() as f64 / perim.len().max(1) as f64
        - ENCLOSE_FREE)
        / (1.0 - ENCLOSE_FREE))
        .clamp(0.0, 1.0);
    for src in &mut sources {
        if src.kind == Kind::Land {
            src.decay *= 1.0 - ENCLOSE_CUT * f_enc;
        }
    }
    if corners.len() < 2 {
        for _ in 0..FAR_SRC_N {
            let th = rng.uniform(0.0, std::f64::consts::TAU);
            let (cx, cy) = (
                w / 2.0 + FAR_SRC_MM * s * libm::cos(th),
                h / 2.0 + FAR_SRC_MM * s * libm::sin(th),
            );
            let st = CORNER.0 * rng.uniform(0.8, 1.2);
            corners.push(Source {
                strength: st,
                decay: 1e9,
                d: Field::from_fn(frame, |x, y| {
                    (libm::hypot(x - cx, y - cy) - (FAR_SRC_MM - 45.0) * s).max(0.0)
                }),
                kind: Kind::Corner,
            });
        }
    }
    sources.extend(corners);
    sources
}

/// The relief (world mm about calm water): each emitter's swell train,
/// blended so its own waves rule near it and hand over to chaos far from
/// every physical emitter, troughs soft-floored, plus the shore lip
/// (`d`: smoothed distance to the shore).
fn relief(
    frame: &dt_kit::Frame,
    seed: Seed,
    rng: &mut Rng,
    sources: &[Source],
    water: &[bool],
    d: &Field,
) -> Vec<f64> {
    let s = frame.scale();
    let nn = d.z.len();
    let land_grad = sources
        .iter()
        .find(|q| q.kind == Kind::Land)
        .map(|q| q.d.gradient());
    let mut num = vec![0.0; nn];
    let mut den = vec![0.0; nn];
    let mut lin = vec![0.0; nn];
    let mut w2 = vec![0.0; nn];
    let mut order = vec![0.0f64; nn];
    for (si, src) in sources.iter().enumerate() {
        let (band, grow) = match src.kind {
            Kind::Land => (BAND_LAND, CHAOS_WOBBLE_GROW),
            Kind::Rock => (BAND_ROCK, CHAOS_WOBBLE_GROW),
            Kind::Corner => (BAND_CORNER, 0.5 * CHAOS_WOBBLE_GROW),
        };
        let mut wt: Vec<f64> = src
            .d
            .z
            .iter()
            .map(|&dk| src.strength * libm::exp(-dk / src.decay))
            .collect();
        if src.kind == Kind::Rock {
            for v in &mut wt {
                *v *= SWELL_ROCK_GAIN;
            }
            if let Some((lx, ly)) = &land_grad {
                let (rx, ry) = src.d.gradient();
                for (k, w) in wt.iter_mut().enumerate() {
                    let down = (lx.z[k] * rx.z[k] + ly.z[k] * ry.z[k]).clamp(0.0, 1.0);
                    *w *= 1.0 + SWELL_WAKE * down * libm::exp(-src.d.z[k] / (2.5 * src.decay));
                }
            }
        }
        let tr = swell_train(
            frame,
            seed.key("train").index(si as u64),
            rng,
            &src.d,
            band,
            grow,
            s,
        );
        for k in 0..nn {
            let wk = wt[k];
            let p = libm::pow(wk, EMIT_SHARP);
            num[k] += p * tr.z[k];
            den[k] += p;
            lin[k] += wk * tr.z[k];
            w2[k] += wk * wk;
            if src.kind != Kind::Corner {
                order[k] = order[k].max(wk.min(1.0));
            }
        }
    }
    let mut z: Vec<f64> = (0..nn)
        .map(|k| {
            let dom = num[k] / den[k].max(1e-12);
            let chaos = lin[k] / libm::sqrt(w2[k]).max(1e-9);
            let c = smooth01((order[k] - CHAOS_ORDER.0) / (CHAOS_ORDER.1 - CHAOS_ORDER.0));
            (c * dom + (1.0 - c) * chaos) * (CHAOS_FAR_AMP + (1.0 - CHAOS_FAR_AMP) * c)
        })
        .collect();
    let wet_vals: Vec<f64> = (0..nn).filter(|&k| water[k]).map(|k| z[k]).collect();
    let (m, _) = mean_std(&wet_vals);
    let (fl, kk) = (-SWELL_TROUGH_MM * s, 0.25 * s);
    for (k, v) in z.iter_mut().enumerate() {
        let a = (*v - m) * SWELL_AMP_MM * s;
        let soft = fl + kk * libm::log1p(libm::exp((a - fl) / kk));
        let lip = LIP_MM * s * libm::exp(-((d.z[k] - LIP_D_MM * s) / (0.5 * s)).powi(2));
        *v = if water[k] { soft } else { 0.0 } + lip;
    }
    z
}

/// The build's measures: relief over open water, the steepest wet slope
/// of `top`, the worst wet edge node's miss from the contract, and the
/// wet share.
fn stats(
    frame: &dt_kit::Frame,
    top: &Field,
    z: &[f64],
    water: &[bool],
    wet: &[bool],
    rim_f: &Field,
    edge_wl: f64,
) -> Stats {
    let (nx, ny) = (top.nx, top.ny);
    let nn = nx * ny;
    let open: Vec<f64> = (0..nn)
        .filter(|&k| water[k] && rim_f.z[k] > 0.99)
        .map(|k| z[k])
        .collect();
    let (_, rs) = mean_std(&open);
    let (gx, gy) = top.gradient();
    #[allow(clippy::cast_precision_loss)]
    let wet_share = wet.iter().filter(|&&b| b).count() as f64 / nn as f64;
    Stats {
        relief: (
            open.iter().copied().fold(f64::INFINITY, f64::min),
            open.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            rs,
        ),
        max_slope_deg: (0..nn)
            .filter(|&k| wet[k])
            .map(|k| libm::atan(libm::hypot(gx.z[k], gy.z[k])).to_degrees())
            .fold(0.0, f64::max),
        edge_err_mm: (0..nn)
            .filter(|&k| wet[k] && on_edge(k, nx, ny))
            .map(|k| (top.z[k] - edge_wl - profile(frame, along_edge(top, k))).abs())
            .fold(0.0, f64::max),
        wet: wet_share,
    }
}
