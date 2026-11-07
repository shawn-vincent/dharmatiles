//! `ground`: the bare ground shape. A level, hills (`bumps`, `bump-size`)
//! and the edge contract: the surface meets every tile edge at the edge
//! level and rises from there.
//!
//! The hills are on position-keyed
//! noise: two smooth noises (`bump-size` and 5/14 of it, the second at
//! 0.15 weight), the lowest 35% left as plains, soft-maxed onto them,
//! scaled to the drawn height, faded over the 9 mm next to the rim and
//! slope-capped at 24°. `bumps up=… down=…` adds hollows the same way
//! from the noise's low end. The level ramps in from the edge level
//! over the outer `max(1 mm, 1.5 |level − edge| / tan 24°)`: a
//! 0.3 mm lift over 1 mm, never steeper than a hill.
//!
//! [`Shape`] is shared with [`crate::dirt`], which builds on the same
//! ground.

use dt_kit::{Field, Noise, Rng, Scale, Scene, Seed, noise, rim, smax, smooth01};

use crate::read;
use crate::{Control, Ctx, Element, LookError, Plan, Write, only};

/// The controls `ground` takes.
pub const CONTROLS: &[&str] = &["level", "bumps", "bumps/up", "bumps/down", "bump-size"];

/// The hill constants (mm at the reference square unless noted).
const PLAIN_FRAC: f64 = 0.35;
const MID_WEIGHT: f64 = 0.15;
const MID_RATIO: f64 = 5.0 / 14.0;
const FADE_MM: f64 = 9.0;
const MAX_SLOPE_DEG: f64 = 24.0;
/// The default `bump-size`: a broad scale.
const SIZE_MM: f64 = 14.0;
/// The level ramp's narrowest width, true mm.
const RAMP_MM: f64 = 1.0;

/// A height an author wrote: one number, or a range drawn once per tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Amount(pub f64, pub f64);

impl Amount {
    fn draw(self, rng: &mut Rng) -> f64 {
        if self.1 > self.0 {
            rng.uniform(self.0, self.1)
        } else {
            self.0
        }
    }

    const fn is_zero(self) -> bool {
        self.1 <= 0.0
    }
}

/// The ground shape both `ground` and `dirt` build: mm at the reference.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// Where the surface sits inside the rim.
    pub level_mm: f64,
    /// How far the hills rise.
    pub up: Amount,
    /// How far the hollows fall.
    pub down: Amount,
    /// The hills' size (the broad noise's sigma).
    pub size_mm: f64,
}

fn amount(c: &Control, lo: f64, hi: f64) -> Result<Amount, LookError> {
    read::span(c, lo, hi).map(|(a, b)| Amount(a, b))
}

impl Shape {
    /// From the controls `ground` and `dirt` share; others are ignored.
    ///
    /// # Errors
    /// [`LookError`] for a value out of range.
    pub fn new(element: &str, controls: &[Control]) -> Result<Self, LookError> {
        let mut s = Self {
            level_mm: f64::NAN,
            up: Amount(0.0, 0.0),
            down: Amount(0.0, 0.0),
            size_mm: SIZE_MM,
        };
        for c in controls {
            match c.name.as_str() {
                "level" => {
                    s.level_mm = read::positive(c).map_err(|e| {
                        e.with_help(
                            "write a named level (ground, edge, water, wall-top) or mm, such as level=5",
                        )
                    })?;
                }
                "bumps" | "bumps/up" => s.up = amount(c, 0.0, 15.0)?,
                "bumps/down" => s.down = amount(c, 0.0, 15.0)?,
                "bump-size" => {
                    s.size_mm = read::number(c, 5.0, 70.0).map_err(|e| {
                        e.with_help("write one width, from 5 to 70 mm, such as bump-size=14")
                    })?;
                }
                _ => {}
            }
        }
        if s.level_mm.is_nan() {
            return Err(LookError::control(
                "level",
                format!("{element} needs a level"),
                "write a named level or mm, such as level=5".into(),
            ));
        }
        Ok(s)
    }

    /// The surface over the whole tile (world mm), and the distance to the
    /// rim (mm): the level, ramped in from the edge level, plus the hills.
    #[must_use]
    pub fn surface(&self, scene: &Scene, seed: Seed) -> (Field, Field) {
        let frame = *scene.frame();
        let s = frame.scale();
        let level = self.level_mm * s;
        let edge = scene.edge().unwrap_or(level);
        let rim = rim(&frame);
        let mut rng = Rng::new(seed.key("bumps"));
        let (up, down) = (self.up.draw(&mut rng) * s, self.down.draw(&mut rng) * s);
        let tan = libm::tan(MAX_SLOPE_DEG.to_radians());
        let ramp = RAMP_MM.max(1.5 * (level - edge).abs() / tan);
        // a bed below the edge level (a pool's) runs flat to the rim: the
        // water meets the edge there, and the edge rule raises dry ground
        let mut z = if level < edge {
            Field::new(&frame, level)
        } else {
            rim.map(|d| edge + (level - edge) * smooth01(d / ramp))
        };
        if !(self.up.is_zero() && self.down.is_zero()) {
            let h = hills(&frame, seed, &rim, up, down, self.size_mm);
            z = z.zip(&h, |a, b| a + b);
        }
        (z, rim)
    }
}

fn interior(f: &Field, inside: &[bool]) -> impl Iterator<Item = f64> {
    f.z.iter().zip(inside).filter(|(_, k)| **k).map(|(v, _)| *v)
}

/// One side of the hills: `r` (positive where it rises) soft-maxed onto
/// 0, from 0 up to `amount` over the interior.
fn part(r: &Field, inside: &[bool], amount: f64) -> Field {
    let vals: Vec<f64> = interior(r, inside).collect();
    #[allow(clippy::cast_precision_loss)]
    let n = vals.len().max(1) as f64;
    let mean = vals.iter().sum::<f64>() / n;
    let sd = libm::sqrt(vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n);
    let soft = r.map(|v| smax(v, 0.0, 0.6 * sd));
    let lo = interior(&soft, inside).fold(f64::INFINITY, f64::min);
    let shifted = soft.map(|v| v - lo);
    let hi = interior(&shifted, inside).fold(0.0, f64::max);
    shifted.map(|v| v * amount / hi.max(1e-9))
}

/// The rises (and, mirrored, hollows), world mm, zero on the rim.
fn hills(frame: &dt_kit::Frame, seed: Seed, rim: &Field, up: f64, down: f64, size: f64) -> Field {
    let s = frame.scale();
    let broad = noise(
        frame,
        seed.key("broad"),
        Noise::Smooth { sigma_mm: size },
        Scale::Layout,
    );
    let mid = noise(
        frame,
        seed.key("mid"),
        Noise::Smooth {
            sigma_mm: size * MID_RATIO,
        },
        Scale::Layout,
    );
    let n = broad.zip(&mid, |a, b| a + MID_WEIGHT * b);
    let inside: Vec<bool> = rim.z.iter().map(|&d| d > FADE_MM * s).collect();
    let keep = if inside.iter().any(|&k| k) {
        inside
    } else {
        vec![true; rim.z.len()]
    };
    let total = up + down;
    let mut u = n.map(|_| 0.0);
    if up > 0.0 {
        let t = n.quantile(1.0 - (1.0 - PLAIN_FRAC) * up / total, Some(&keep));
        u = u.zip(&part(&n.map(|v| v - t), &keep, up), |a, b| a + b);
    }
    if down > 0.0 {
        let t = n.quantile((1.0 - PLAIN_FRAC) * down / total, Some(&keep));
        u = u.zip(&part(&n.map(|v| t - v), &keep, down), |a, b| a - b);
    }
    let fade = FADE_MM * s;
    let u = u.zip(rim, |v, d| {
        v * (0.5 - 0.5 * libm::cos(core::f64::consts::PI * (d / fade).clamp(0.0, 1.0)))
    });
    let cap = libm::tan(MAX_SLOPE_DEG.to_radians());
    let slope = u.max_slope();
    if slope > cap {
        u.map(|v| v * cap / slope)
    } else {
        u
    }
}

/// The ground surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Ground {
    /// Its shape.
    pub shape: Shape,
}

impl Ground {
    /// From controls: `level` (mm; the evaluator resolves named levels
    /// and fills in the schema default, so it is always given), `bumps`
    /// (or `bumps up= down=`) and `bump-size`.
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("ground", controls, CONTROLS)?;
        Ok(Self {
            shape: Shape::new("ground", controls)?,
        })
    }
}

impl Element for Ground {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Surface,
            reads: Vec::new(),
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let (z, _) = self.shape.surface(scene, ctx.seed);
        scene.paint_field(ctx.area, &z);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]
    use super::*;
    use crate::Value;
    use dt_kit::Frame;

    pub(crate) fn controls(list: &[(&str, Value)]) -> Vec<Control> {
        list.iter()
            .map(|(n, v)| Control {
                name: (*n).to_owned(),
                value: v.clone(),
            })
            .collect()
    }

    #[test]
    fn hills_rise_only_from_the_edge_and_cap_the_slope() {
        let shape = Shape::new(
            "ground",
            &controls(&[
                ("level", Value::Number(3.7)),
                ("bumps", Value::Range(2.2, 3.2)),
            ]),
        )
        .unwrap();
        let mut scene = Scene::new(Frame::dungeonblocks(2, 2), Seed::new(1));
        scene.set_edge(Some(3.4));
        let (z, rim) = shape.surface(&scene, Seed::new(1).key("g"));
        let (lo, hi) =
            z.z.iter()
                .fold((f64::INFINITY, 0.0f64), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!((lo - 3.4).abs() < 1e-9, "edge {lo}");
        assert!(hi > 3.7 + 1.5 && hi < 3.7 + 3.3, "top {hi}");
        assert!(z.max_slope() <= libm::tan(24f64.to_radians()) + 0.05);
        for (v, d) in z.z.iter().zip(&rim.z) {
            if *d == 0.0 {
                assert!((v - 3.4).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn hollows_fall_below_the_level() {
        let shape = Shape::new(
            "ground",
            &controls(&[
                ("level", Value::Number(8.0)),
                ("bumps/up", Value::Number(2.0)),
                ("bumps/down", Value::Number(3.0)),
            ]),
        )
        .unwrap();
        let scene = Scene::new(Frame::dungeonblocks(2, 2), Seed::new(1));
        let (z, _) = shape.surface(&scene, Seed::new(3));
        let (lo, hi) =
            z.z.iter()
                .fold((f64::INFINITY, 0.0f64), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(lo < 8.0 - 1.5 && hi > 8.0 + 1.0, "{lo} {hi}");
    }

    #[test]
    fn hills_are_a_photocopy_at_openlock_scale() {
        // the same hills at the same relative positions, scaled by
        // 25.4 / 35 in height too (layout-scale noise, R7)
        let shape = Shape::new(
            "ground",
            &controls(&[("level", Value::Number(5.0)), ("bumps", Value::Number(3.0))]),
        )
        .unwrap();
        let at = |square: f64| {
            let f = Frame {
                cols: 2,
                rows: 2,
                square_mm: square,
                cells_per_square: 70,
            };
            shape.surface(&Scene::new(f, Seed::new(1)), Seed::new(9)).0
        };
        let (db, ol) = (at(35.0), at(25.4));
        let k = 25.4 / 35.0;
        let worst =
            db.z.iter()
                .zip(&ol.z)
                .map(|(a, b)| (a * k - b).abs())
                .fold(0.0, f64::max);
        assert!(worst < 1e-4, "worst {worst}"); // float rounding of keyed positions
    }

    #[test]
    fn out_of_range_bumps_are_refused() {
        let e = Ground::new(&controls(&[
            ("level", Value::Number(5.0)),
            ("bumps", Value::Number(40.0)),
        ]))
        .unwrap_err();
        assert_eq!(e.control.as_deref(), Some("bumps"));
        let e = Ground::new(&controls(&[
            ("level", Value::Number(5.0)),
            ("bump-size", Value::Range(10.0, 20.0)),
        ]))
        .unwrap_err();
        assert_eq!(e.control.as_deref(), Some("bump-size"));
    }
}
