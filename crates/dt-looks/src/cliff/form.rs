//! 1 Form: the contact planes the rocks are meant to touch and the target
//! height field T(x, y) the pile grows toward (rockpile.py :339-554).
//! Heights are above the foot (0). Lengths that size masses scale with `s`
//! (the system's scale, 1 on DungeonBlocks); details never do.

use dt_kit::{Rng, blur2};

use super::util::{Draw, Field, G, Mask, lerp};

pub const GRID_MM: f64 = 0.25;
pub const FOOT: f64 = 0.0;
pub const SQUARE_MM: f64 = 35.0;

const DEPTH_MM: (f64, f64) = (8.0, 20.0);
const TOP_DEPTH_MM: (f64, f64) = (5.0, 10.0);
const MASS_W_MM: (f64, f64) = (6.0, 15.0);
const FULL_P: f64 = 0.35;
const TIER_Z: (f64, f64) = (0.3, 0.7);
const GULLIES_PER_SQUARE: f64 = 0.5;
const GULLY_DEPTH_MM: (f64, f64) = (5.0, 7.0);
const GULLY_MIN_W_MM: f64 = 10.0;
const BUTTRESSES_PER_SQUARE: f64 = 0.5;
const BUTTRESS_DEPTH_MM: (f64, f64) = (20.0, 25.0);
const END_MM: f64 = 4.0;
const END_DEPTH_MM: f64 = 11.0;
const TIER_STEPS: (i64, i64) = (1, 3);
const TIER_STEP_MM: f64 = 5.0;
const MIN_STEP_MM: f64 = 6.0;
const MIN_STRIP_MM: f64 = 4.0;
pub const MIN_H_MM: f64 = 3.5;
const RING_JITTER: f64 = 0.15;
const RING_NOTCH_P: f64 = 0.12;
const RING_LOBE_P: f64 = 0.12;
const RING_STEP_MM: f64 = 3.0;
const NEEDLE_SCALE: (f64, f64) = (0.45, 0.8);
const NEEDLE_SPREAD: (f64, f64) = (0.45, 0.7);
const BASE_RAGGED: f64 = 0.15;

/// The nine forms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormName {
    Run,
    Corner,
    Canyon,
    Escarpment,
    Plateau,
    Massif,
    Mound,
    Spire,
    Needles,
}

impl FormName {
    pub fn parse(w: &str) -> Option<Self> {
        Some(match w {
            "run" => Self::Run,
            "corner" => Self::Corner,
            "canyon" => Self::Canyon,
            "escarpment" => Self::Escarpment,
            "plateau" => Self::Plateau,
            "massif" => Self::Massif,
            "mound" => Self::Mound,
            "spire" => Self::Spire,
            "needles" | "needle" => Self::Needles,
            _ => return None,
        })
    }
    pub fn is_band(self) -> bool {
        matches!(
            self,
            Self::Run | Self::Corner | Self::Canyon | Self::Escarpment
        )
    }
}

type Ring = ((f64, f64), (f64, f64));

struct Spec {
    band: bool,
    walls: &'static [&'static str],
    height: (f64, f64), // equal ends: one height; unequal: a ramp
    cap: bool,
    mass: f64,
    kinds: Option<[f64; 5]>,
    depth: Option<(f64, f64)>,
    top_depth: Option<(f64, f64)>,
    buttress: Option<(f64, f64)>,
    rings: &'static [Ring],
    centres: Option<(i64, i64)>,
    base: Option<Ring>,
}

const CORNER_KINDS: [f64; 5] = [0.5, 0.24, 0.08, 0.06, 0.12];

fn spec(f: FormName) -> Spec {
    let band = |walls: &'static [&'static str], height: (f64, f64), cap: bool, mass: f64| Spec {
        band: true,
        walls,
        height,
        cap,
        mass,
        kinds: None,
        depth: None,
        top_depth: None,
        buttress: None,
        rings: &[],
        centres: None,
        base: None,
    };
    let rings = |height: f64, cap: bool, mass: f64, r: &'static [Ring]| Spec {
        band: false,
        walls: &[],
        height: (height, height),
        cap,
        mass,
        kinds: None,
        depth: None,
        top_depth: None,
        buttress: None,
        rings: r,
        centres: None,
        base: None,
    };
    match f {
        FormName::Run => band(&["back"], (35.0, 35.0), true, 1.35),
        FormName::Corner => Spec {
            kinds: Some(CORNER_KINDS),
            ..band(&["back", "left"], (35.0, 35.0), true, 1.35)
        },
        FormName::Canyon => Spec {
            depth: Some((8.0, 14.0)),
            top_depth: Some((5.0, 9.0)),
            buttress: Some((14.0, 17.0)),
            ..band(&["left", "right"], (35.0, 35.0), true, 1.3)
        },
        FormName::Escarpment => band(&["back"], (10.0, 40.0), false, 1.2),
        FormName::Plateau => rings(14.0, true, 1.3, &[((22.0, 28.0), (1.0, 1.0))]),
        FormName::Massif => rings(
            35.0,
            true,
            1.4,
            &[((24.0, 29.0), (0.4, 0.6)), ((13.0, 17.0), (1.0, 1.0))],
        ),
        FormName::Mound => rings(
            34.0,
            false,
            1.15,
            &[
                ((25.0, 29.0), (0.12, 0.22)),
                ((19.0, 22.0), (0.32, 0.45)),
                ((13.0, 16.0), (0.52, 0.66)),
                ((8.0, 10.0), (0.74, 0.86)),
                ((5.0, 7.0), (1.0, 1.0)),
            ],
        ),
        FormName::Spire => rings(
            58.0,
            false,
            1.0,
            &[
                ((24.0, 28.0), (0.08, 0.14)),
                ((17.0, 20.0), (0.25, 0.4)),
                ((11.0, 13.0), (0.5, 0.68)),
                ((6.5, 8.0), (1.0, 1.0)),
            ],
        ),
        FormName::Needles => Spec {
            centres: Some((2, 3)),
            base: Some(((1.2, 1.4), (0.12, 0.2))),
            ..rings(
                55.0,
                false,
                1.0,
                &[
                    ((14.0, 17.0), (0.1, 0.18)),
                    ((9.0, 11.0), (0.35, 0.5)),
                    ((5.0, 6.5), (1.0, 1.0)),
                ],
            )
        },
    }
}

/// The form: what the pile grows toward.
pub struct Form {
    pub w: f64,
    pub h: f64,
    pub t: Field,
    pub ztop: Option<f64>,
    pub round: bool,
    pub mass: f64,
    pub along_y: Option<Mask>,
    pub kind_p: Option<[f64; 5]>,
    /// (axis, high side) of the tile-boundary planes the cliff stands against.
    pub rims: Vec<(usize, bool)>,
}

/// What a tile asks for.
pub struct Ask {
    pub form: FormName,
    pub height: Option<f64>,
    pub depth: Option<(f64, f64)>,
    pub mass_gain: f64,
    pub kinds: Option<[f64; 5]>,
    pub ledge_height: Option<f64>,
}

pub fn grid_dims(w: f64, h: f64) -> (usize, usize) {
    (
        (w / GRID_MM).round() as usize,
        (h / GRID_MM).round() as usize,
    )
}

pub fn make_form(rng: &mut Rng, ask: &Ask, w: f64, h: f64, s: f64) -> Form {
    let sp = spec(ask.form);
    let (nx, ny) = grid_dims(w, h);
    let mut t = Field::new(nx, ny, FOOT);
    let hgt = ask.height.map_or(sp.height, |v| (v, v));
    let hgt = (hgt.0 * s, hgt.1 * s);
    let mut along_y = None;
    let mut rims = Vec::new();
    if sp.band {
        let mut ay = Mask::new(nx, ny, false);
        for wall in sp.walls {
            let (axis, high) = match *wall {
                "back" => (1, true),
                "front" => (1, false),
                "left" => (0, false),
                _ => (0, true),
            };
            rims.push((axis, high));
            let tw = band(rng, &sp, ask, hgt, (nx, ny), (w, h), wall, s);
            for k in 0..nx * ny {
                if tw.v[k] > t.v[k] {
                    ay.v[k] = *wall == "left" || *wall == "right";
                    t.v[k] = tw.v[k];
                }
            }
        }
        along_y = Some(ay);
    } else {
        t = rings(rng, &sp, hgt.0, nx, ny, w, h);
    }
    let ztop = (sp.cap && (hgt.0 - hgt.1).abs() < 1e-9).then_some(FOOT + hgt.0);
    Form {
        w,
        h,
        t,
        ztop,
        round: !sp.band,
        mass: sp.mass * ask.mass_gain,
        along_y,
        kind_p: ask.kinds.or(sp.kinds),
        rims,
    }
}

fn segments(rng: &mut Rng, length: f64, s: f64) -> Vec<f64> {
    let mut xs = vec![0.0];
    while *xs.last().unwrap_or(&0.0) < length {
        let l = *xs.last().unwrap_or(&0.0);
        xs.push(l + rng.uniform(MASS_W_MM.0 * s, MASS_W_MM.1 * s));
    }
    let n = xs.len();
    xs[n - 1] = length;
    if xs.len() > 2 && xs[xs.len() - 1] - xs[xs.len() - 2] < MASS_W_MM.0 * s * 0.5 {
        xs.remove(xs.len() - 2);
    }
    xs
}

/// One band grown out from a wall: masses along it, each with its depth,
/// a stepped lower tier, the odd gully or buttress, and the standard end
/// section at both ends (S4).
#[allow(clippy::too_many_arguments)]
fn band(
    rng: &mut Rng,
    sp: &Spec,
    ask: &Ask,
    hgt: (f64, f64),
    (nx, ny): (usize, usize),
    (w, h): (f64, f64),
    wall: &str,
    s: f64,
) -> Field {
    // A: along the wall, Dp: out from it
    let (l, alongx) = match wall {
        "back" | "front" => (w, true),
        _ => (h, false),
    };
    let coord = |i: usize, j: usize| -> (f64, f64) {
        let (x, y) = ((i as f64 + 0.5) * GRID_MM, (j as f64 + 0.5) * GRID_MM);
        match wall {
            "back" => (x, h - y),
            "front" => (x, y),
            "left" => (y, x),
            _ => (y, w - x),
        }
    };
    let _ = alongx;
    let mut t = Field::new(nx, ny, FOOT);
    let ramp: Option<(f64, f64)> = if (hgt.0 - hgt.1).abs() < 1e-9 {
        None
    } else if rng.random() < 0.5 {
        Some(hgt)
    } else {
        Some((hgt.1, hgt.0))
    };
    let top_at = |a: f64| match ramp {
        None => FOOT + hgt.0,
        Some(r) => FOOT + r.0 + (r.1 - r.0) * (a / l).clamp(0.0, 1.0),
    };
    let depth = ask.depth.or(sp.depth).unwrap_or(DEPTH_MM);
    let depth = (depth.0 * s, depth.1 * s);
    let top_depth = sp.top_depth.unwrap_or(TOP_DEPTH_MM);
    let top_depth = (top_depth.0 * s, top_depth.1 * s);
    let xs = segments(rng, l, s);
    let inner: Vec<usize> = if xs.len() > 3 {
        let p = rng.perm(xs.len() - 3);
        p.into_iter().map(|k| k + 1).collect()
    } else {
        Vec::new()
    };
    let n_g = (rng.poisson(GULLIES_PER_SQUARE * l / (SQUARE_MM * s)).max(1)) as usize;
    let n_b = (rng
        .poisson(BUTTRESSES_PER_SQUARE * l / (SQUARE_MM * s))
        .max(1)) as usize;
    let gul: Vec<usize> = inner
        .iter()
        .copied()
        .filter(|&k| xs[k + 1] - xs[k] >= GULLY_MIN_W_MM * s)
        .take(n_g)
        .collect();
    let but: Vec<usize> = inner
        .iter()
        .copied()
        .filter(|k| !gul.contains(k))
        .take(n_b)
        .collect();
    // per-cell coordinates, once
    let mut aa = vec![0.0; nx * ny];
    let mut dd = vec![0.0; nx * ny];
    for j in 0..ny {
        for i in 0..nx {
            let (a, d) = coord(i, j);
            aa[j * nx + i] = a;
            dd[j * nx + i] = d;
        }
    }
    let foot_h = |zt: f64| zt - FOOT;
    for si in 0..xs.len() - 1 {
        let (a, b) = (xs[si], xs[si + 1]);
        let zt = top_at(0.5 * (a + b));
        let mut d_all = rng.uniform(depth.0, depth.1);
        let mut d_top = if rng.random() < FULL_P {
            d_all
        } else {
            rng.uniform(top_depth.0, top_depth.1)
        };
        if gul.contains(&si) {
            d_all = rng.uniform(GULLY_DEPTH_MM.0 * s, GULLY_DEPTH_MM.1 * s);
            d_top = d_all;
        } else if but.contains(&si) {
            let b2 = sp.buttress.unwrap_or(BUTTRESS_DEPTH_MM);
            d_all = rng.uniform(b2.0 * s, b2.1 * s);
            d_top = d_all;
        }
        if d_top < d_all {
            d_all = d_all.max(d_top + MIN_STRIP_MM * s);
        }
        let tz = rng.uniform(TIER_Z.0, TIER_Z.1);
        let z_mid = FOOT + ask.ledge_height.unwrap_or(tz) * foot_h(zt);
        let mut k = rng.count(TIER_STEPS);
        k = k.min(((b - a) / (MIN_STEP_MM * s)).floor() as i64).max(1);
        let mut cuts: Vec<f64> = Vec::new();
        if k > 1 {
            let free = (b - a) - k as f64 * MIN_STEP_MM * s;
            let mut r: Vec<f64> = (0..k - 1).map(|_| rng.uniform(0.0, free)).collect();
            r.sort_by(f64::total_cmp);
            cuts = (1..k)
                .zip(r)
                .map(|(m, e)| a + MIN_STEP_MM * s * m as f64 + e)
                .collect();
        }
        if foot_h(zt) > 2.0 * MIN_H_MM + 3.0 {
            let mut edges = vec![a];
            edges.extend(&cuts);
            edges.push(b);
            for pair in edges.windows(2) {
                let (a2, b2) = (pair[0], pair[1]);
                let z2 = (z_mid + rng.uniform(-TIER_STEP_MM, TIER_STEP_MM))
                    .clamp(FOOT + MIN_H_MM + 1.0, zt - MIN_H_MM - 1.0);
                let mut d2 = d_all;
                if d_top < d_all && rng.random() < 0.5 {
                    d2 = d_all + rng.signed((MIN_STRIP_MM * s, 1.5 * MIN_STRIP_MM * s));
                    if d2 < d_top + MIN_STRIP_MM * s {
                        d2 = d_all + (d2 - d_all).abs();
                    }
                }
                for q in 0..nx * ny {
                    if aa[q] >= a2 && aa[q] < b2 && dd[q] <= d2 {
                        t.v[q] = z2;
                    }
                }
            }
        }
        for q in 0..nx * ny {
            if aa[q] >= a && aa[q] < b && dd[q] <= d_top {
                t.v[q] = zt;
            }
        }
    }
    let (end_mm, end_depth) = (END_MM * s, END_DEPTH_MM * s);
    for q in 0..nx * ny {
        let a = aa[q];
        if a < end_mm || a > l - end_mm {
            t.v[q] = if dd[q] <= end_depth { top_at(a) } else { FOOT };
        }
    }
    t
}

fn wobble(rng: &mut Rng, nx: usize, ny: usize, sigma_mm: f64) -> Field {
    let mut g = Field::new(nx, ny, 0.0);
    for v in &mut g.v {
        *v = rng.normal(0.0, 1.0);
    }
    let v = blur2(&g.v, nx, ny, sigma_mm / GRID_MM);
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let sd = libm::sqrt(v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n);
    Field {
        nx,
        ny,
        v: v.iter().map(|x| (x - mean) / (sd + 1e-12)).collect(),
    }
}

fn rings(rng: &mut Rng, sp: &Spec, height: f64, nx: usize, ny: usize, w: f64, h: f64) -> Field {
    let mut t = Field::new(nx, ny, FOOT);
    let sc = w.min(h) / 70.0;
    let k = sp.centres.map_or(1, |c| rng.count(c));
    let mut centres: Vec<([f64; 2], f64)> = Vec::new();
    for i in 0..k {
        let (size, hgt) = if i == 0 {
            (1.0, 1.0)
        } else {
            let v = rng.uniform(NEEDLE_SCALE.0, NEEDLE_SCALE.1);
            (v, v)
        };
        let r_out = sp.rings[0].0.1 * sc * size;
        let mut c = [
            w / 2.0 + rng.uniform(-2.0, 2.0),
            h / 2.0 + rng.uniform(-2.0, 2.0),
        ];
        if i > 0 {
            for _ in 0..50 {
                let (c0, r0) = centres[0];
                let th = rng.uniform(0.0, std::f64::consts::TAU);
                let d = rng.uniform(NEEDLE_SPREAD.0, NEEDLE_SPREAD.1) * (r0 + r_out);
                c = [c0[0] + d * libm::cos(th), c0[1] + d * libm::sin(th)];
                if r_out + 2.0 < c[0]
                    && c[0] < w - r_out - 2.0
                    && r_out + 2.0 < c[1]
                    && c[1] < h - r_out - 2.0
                    && centres.iter().all(|(c2, r2)| {
                        libm::hypot(c[0] - c2[0], c[1] - c2[1]) > NEEDLE_SPREAD.0 * (r_out + r2)
                    })
                {
                    break;
                }
            }
        }
        centres.push((c, r_out));
        let circ = std::f64::consts::TAU * r_out;
        let segs: Vec<f64> = segments(rng, circ, 1.0)
            .iter()
            .map(|a| a / circ.max(1e-9) * std::f64::consts::TAU)
            .collect();
        for pair in segs.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let u = rng.random();
            let f = if u < RING_NOTCH_P {
                0.6
            } else if u < RING_NOTCH_P + RING_LOBE_P {
                1.2
            } else {
                1.0
            };
            for &(rr, hr) in sp.rings {
                let r = rng.uniform(rr.0, rr.1)
                    * sc
                    * size
                    * f
                    * (1.0 + rng.uniform(-RING_JITTER, RING_JITTER));
                let mut z = FOOT + rng.uniform(hr.0, hr.1) * height * hgt;
                if hr.0 < 1.0 {
                    z += rng.uniform(-RING_STEP_MM, RING_STEP_MM);
                }
                z = z.max(FOOT + MIN_H_MM);
                for j in 0..ny {
                    for i in 0..nx {
                        let (x, y) = ((i as f64 + 0.5) * GRID_MM, (j as f64 + 0.5) * GRID_MM);
                        let th = libm::atan2(y - c[1], x - c[0]).rem_euclid(std::f64::consts::TAU);
                        if th >= a && th < b && libm::hypot(x - c[0], y - c[1]) <= r {
                            let q = j * nx + i;
                            t.v[q] = t.v[q].max(z);
                        }
                    }
                }
            }
        }
    }
    if let Some((rs, hs)) = sp.base {
        let wob = wobble(rng, nx, ny, 6.0);
        let r_scale = rng.uniform(rs.0, rs.1);
        let z0 = FOOT + (rng.uniform(hs.0, hs.1) * height).max(MIN_H_MM);
        let wz = wobble(rng, nx, ny, 8.0);
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = ((i as f64 + 0.5) * GRID_MM, (j as f64 + 0.5) * GRID_MM);
                let q = j * nx + i;
                let mut rb = centres
                    .iter()
                    .map(|(c, r)| libm::hypot(x - c[0], y - c[1]) / (r * r_scale))
                    .fold(f64::INFINITY, f64::min);
                rb *= 1.0 + BASE_RAGGED * wob.v[q];
                let z = (z0 + RING_STEP_MM * libm::round(wz.v[q])).max(FOOT + MIN_H_MM);
                if rb <= 1.0 {
                    t.v[q] = t.v[q].max(z);
                }
            }
        }
    }
    t
}

/// Shattered-zone intensity along the run, ZONE_FLOOR..1.
pub struct Zones {
    cx: Vec<f64>,
    sg: Vec<f64>,
}

pub const ZONE_FLOOR: f64 = 0.25;
const ZONES_PER_SQUARE: f64 = 1.5;
const ZONE_SIGMA_MM: (f64, f64) = (5.0, 10.0);

impl Zones {
    pub fn new(
        rng: &mut Rng,
        w: f64,
        s: f64,
        patches: Option<f64>,
        width: Option<(f64, f64)>,
    ) -> Self {
        let k = patches.map_or(
            ((w / (SQUARE_MM * s) * ZONES_PER_SQUARE).round() as usize).max(1),
            |p| p.max(0.0) as usize,
        );
        let sig = width.map_or(ZONE_SIGMA_MM, |(a, b)| (0.5 * a, 0.5 * b));
        let cx: Vec<f64> = (0..k).map(|_| rng.uniform(0.0, w)).collect();
        let sg: Vec<f64> = (0..k)
            .map(|_| rng.uniform(sig.0, sig.1.max(sig.0 + 1e-6)))
            .collect();
        Self { cx, sg }
    }
    pub fn at(&self, x: f64) -> f64 {
        let g = self
            .cx
            .iter()
            .zip(&self.sg)
            .map(|(c, s)| libm::exp(-0.5 * ((x - c) / s).powi(2)))
            .fold(0.0, f64::max);
        ZONE_FLOOR + (1.0 - ZONE_FLOOR) * g
    }
}

#[allow(dead_code)]
fn _unused(_: G<u8>) {
    let _ = lerp((0.0, 1.0), 0.5);
}
