//! Reeds: clumps of tapered stems at the waterline, leaning toward the
//! water.
//!
//! A clump roots where the land is within a hand's breadth of the water's
//! calm level (a height window on the land: it reads the land and the
//! library's calm height, never the water); its stems rise from a near-common
//! base and fan outward, each a chunky lemon-sectioned blade tapering to a
//! blunt point, bent gently outward (offset grows with the square of the
//! height, so it stands vertical at its base).

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

use dt_kit::Kind;
use dt_kit::{Material, Rng, Scene, Tag, TriMesh, form};

use crate::read::Settings;
use crate::{Control, Ctx, Draw, Element, LookError, Plan, Write};

const REED_SEP_MM: f64 = 12.0;
/// Land height relative to the calm water level where reeds root.
const BAND_MM: (f64, f64) = (-1.1, 0.2);
const SPREAD_MM: f64 = 0.35;
const FAN_DEG: (f64, f64) = (14.0, 34.0);
const HALF_W_MM: (f64, f64) = (1.7, 2.3);
const THICK: (f64, f64) = (0.5, 0.65);
const TIP_SCALE: (f64, f64) = (0.25, 0.35);
const TIP_CAP: f64 = 2.2;
const TIP_MIN: f64 = 0.03;
const LEAN_DEG: f64 = 8.0;
const SINK_MM: f64 = 0.8;
const SECTION: usize = 24;

/// The reeds element.
#[derive(Clone, Debug)]
pub struct Reeds {
    clumps: Draw,
    stems: Draw,
    height: Draw,
}

impl Reeds {
    /// From controls.
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        let mut r = Self {
            clumps: Draw::flat(3.0, 5.0),
            stems: Draw::flat(5.0, 9.0),
            height: Draw::flat(8.0, 13.0),
        };
        let s = Settings::new(controls);
        r.clumps = s.dist_or("clumps", r.clumps)?;
        r.stems = s.dist_or("stems", r.stems)?;
        r.height = s.dist_or("height", r.height)?;
        Ok(r)
    }
}

/// One stem, drawn.
struct Stem {
    at: [f64; 2],
    z0: f64,
    h: f64,
    hw: f64,
    ht: f64,
    ts: f64,
    rot: f64,
    lean: [f64; 2],
}

impl Stem {
    fn taper(&self, z: f64) -> f64 {
        let big_h = self.h + 0.8;
        let cap = TIP_CAP * self.hw * self.ts;
        let zb = big_h - cap;
        let lin = 1.0 + (self.ts - 1.0) * (z / zb).clamp(0.0, 1.0);
        let u = ((z - zb) / cap).clamp(0.0, 1.0);
        (lin * libm::sqrt((1.0 - u * u).clamp(0.0, 1.0))).max(TIP_MIN)
    }

    fn mesh(&self) -> TriMesh {
        let big_h = self.h + 0.8;
        let cap = TIP_CAP * self.hw * self.ts;
        let zb = big_h - cap;
        let mut zs: Vec<f64> = (0..=8).map(|i| zb * f64::from(i) / 8.0).collect();
        zs.extend((1..=8).map(|j| {
            let u = libm::sin(core::f64::consts::FRAC_PI_2 * f64::from(j) / 8.0);
            zb + cap * u
        }));
        // bend: vertical at the base, tip slope tan(fan) at the top
        let [ux, uy] = self.lean;
        let m = libm::hypot(ux, uy);
        let (dx, dy) = (ux / (m + 1e-9), uy / (m + 1e-9));
        let kk = libm::tan(m.to_radians()) / (2.0 * big_h);
        let (sr, cr) = (libm::sin(self.rot), libm::cos(self.rot));
        let rings: Vec<Vec<[f64; 3]>> = zs
            .iter()
            .map(|&z| {
                let f = self.taper(z);
                (0..SECTION)
                    .map(|k| {
                        let t = core::f64::consts::TAU * k as f64 / SECTION as f64;
                        let s = libm::sin(t);
                        let (sx, sy) = (
                            self.hw * libm::cos(t) * f,
                            self.ht * s.signum() * libm::pow(s.abs(), 1.4) * f,
                        );
                        let (rx, ry) = (sx * cr - sy * sr, sx * sr + sy * cr);
                        [
                            self.at[0] + rx + dx * kk * z * z,
                            self.at[1] + ry + dy * kk * z * z,
                            self.z0 + z,
                        ]
                    })
                    .collect()
            })
            .collect();
        super::grass::ring_mesh(&rings)
    }
}

impl Element for Reeds {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Reeds),
            reads: Vec::new(),
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let frame = *scene.frame();
        let s = frame.scale();
        let [w, h] = frame.size_mm();
        let (Some(land), Some(wl)) = (scene.land_view(), ctx.calm_mm) else {
            return Err(LookError::general(
                "reeds have no water to stand in",
                "put a `water` in the stack",
            ));
        };
        let bed = land.field();
        let (nx, _) = frame.nodes();
        let mut rng = Rng::new(ctx.seed.key("reeds"));
        let ground = |x: f64, y: f64| land.top(x.clamp(0.0, w), y.clamp(0.0, h));
        // candidate roots in the shallows, inside the area
        let mut cand: Vec<[f64; 2]> = (0..4000)
            .map(|_| {
                [
                    rng.uniform(4.0 * s, w - 4.0 * s),
                    rng.uniform(4.0 * s, h - 4.0 * s),
                ]
            })
            .filter(|c| {
                let g = ground(c[0], c[1]) - wl;
                let (i, j) = bed.node(c[0], c[1]);
                (BAND_MM.0 * s..BAND_MM.1 * s).contains(&g) && ctx.area.bits[j * nx + i]
            })
            .collect();
        // a shuffle (Fisher–Yates), then the first that stand clear
        for i in (1..cand.len()).rev() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let j = rng.int(0, i as i64) as usize;
            cand.swap(i, j);
        }
        let n = self.clumps.int_by(1.0, &mut rng);
        let mut centres: Vec<[f64; 2]> = Vec::new();
        for c in cand {
            if centres
                .iter()
                .all(|q| libm::hypot(c[0] - q[0], c[1] - q[1]) >= REED_SEP_MM * s)
            {
                centres.push(c);
            }
            if i64::try_from(centres.len()).unwrap_or(0) == n {
                break;
            }
        }
        let e = 0.3;
        let mut stems: Vec<Stem> = Vec::new();
        for c in &centres {
            let k = self.stems.int_by(1.0, &mut rng);
            let dx = -(ground(c[0] + e, c[1]) - ground(c[0] - e, c[1])) / (2.0 * e);
            let dy = -(ground(c[0], c[1] + e) - ground(c[0], c[1] - e)) / (2.0 * e);
            let nw = libm::hypot(dx, dy) + 1e-9;
            let ang0 = rng.uniform(0.0, core::f64::consts::TAU);
            for j in 0..k {
                let fan_a =
                    ang0 + core::f64::consts::TAU * j as f64 / k as f64 + rng.normal(0.0, 0.35);
                let rr = rng.random();
                let p = [
                    c[0] + SPREAD_MM * libm::cos(fan_a) * rr,
                    c[1] + SPREAD_MM * libm::sin(fan_a) * rr,
                ];
                let height = self.height.draw(&mut rng) * s;
                let hw = rng.uniform(HALF_W_MM.0, HALF_W_MM.1) * s;
                let ht = hw * rng.uniform(THICK.0, THICK.1);
                let ts = rng.uniform(TIP_SCALE.0, TIP_SCALE.1);
                let rot =
                    fan_a + core::f64::consts::FRAC_PI_2 + rng.normal(0.0, 15.0_f64.to_radians());
                let fan = rng.uniform(FAN_DEG.0, FAN_DEG.1);
                stems.push(Stem {
                    at: p,
                    z0: ground(p[0], p[1]) - SINK_MM,
                    h: height,
                    hw,
                    ht,
                    ts,
                    rot,
                    lean: [
                        fan * libm::cos(fan_a) + LEAN_DEG * dx / nw,
                        fan * libm::sin(fan_a) + LEAN_DEG * dy / nw,
                    ],
                });
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        let slots = scene
            .slots(stems.len() as u32, Material::Grass)
            .map_err(LookError::build)?;
        for (i, st) in stems.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let solid = form::solid(slots.get(i as u32), &st.mesh()).map_err(LookError::build)?;
            scene.add(
                Tag {
                    kind: Kind::Reeds,
                    owner: ctx.path.clone(),
                },
                solid,
            );
        }
        #[allow(clippy::cast_precision_loss)]
        for (k, v) in [
            ("reed_clumps", centres.len() as f64),
            ("reed_stems", stems.len() as f64),
        ] {
            scene.note(k, v);
        }
        Ok(())
    }
}
