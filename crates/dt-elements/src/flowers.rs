//! Flowers: a thin
//! dome on a support column for the centre, and one for each petal, an
//! inverted cone under each petal so it prints standing.
//!
//! A flower is a centre dome on a straight cylinder, ringed by `petals`
//! elongated domes, each on an inverted cone. The petals touch, not
//! overlap, at the centre's edge. Flowers stand where the grass would;
//! the grass reads them (a thing standing on the ground) and grows round.
//!
//! One design choice, for the printability gate: a petal's
//! cone points at its own centre, not at the flower's centre. Pointing at
//! the flower's centre put the petal tip 2.5 mm out over a 1 mm column, a
//! 68 degree overhang; under its own centre the same petal leans 41 degrees.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_arguments
)]

use core::f64::consts::{FRAC_PI_2, PI, TAU};
use dt_kit::Kind;

use dt_kit::{Field, Material, Rng, Scene, Spread, Tag, TriMesh, form, scatter, signed_volume};

use crate::read;
use crate::{Control, Ctx, Element, LookError, Plan, Read, Write, only};

/// The controls `flowers` takes.
pub const CONTROLS: &[&str] = &[
    "per-square",
    "petals",
    "radius",
    "centre",
    "stem",
    "dome",
    "petal",
];

const AZ: usize = 16;
const EL: usize = 8;
const SINK_MM: f64 = 0.10;
const TRIES: u32 = 300;

/// The flowers element.
#[derive(Clone, Debug)]
pub struct Flowers {
    per_square: f64,
    petals: usize,
    outer: f64,
    centre: f64,
    stem: f64,
    dome: f64,
    petal: f64,
}

impl Flowers {
    /// From controls.
    ///
    /// # Errors
    /// [`LookError`] for an unknown control or a value out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        only("flowers", controls, CONTROLS)?;
        let mut f = Self {
            per_square: 4.0,
            petals: 5,
            outer: 2.5,
            centre: 1.5,
            stem: 1.0,
            dome: 1.0,
            petal: 0.5,
        };
        for c in controls {
            match c.name.as_str() {
                "per-square" => f.per_square = read::number(c)?,
                "petals" => f.petals = read::number(c)?.round() as usize,
                "radius" => f.outer = read::number(c)?,
                "centre" => f.centre = read::number(c)?,
                "stem" => f.stem = read::number(c)?,
                "dome" => f.dome = read::number(c)?,
                "petal" => f.petal = read::number(c)?,
                _ => {}
            }
        }
        if f.outer <= f.centre {
            return Err(LookError {
                control: Some("radius".into()),
                message: format!(
                    "the petals reach {} mm, no further than the centre's {} mm",
                    f.outer, f.centre
                ),
                help: Some("make `radius` larger than `centre`".into()),
            });
        }
        Ok(f)
    }
}

/// A thin dome cap on a column (a straight cylinder, or an inverted cone
/// with its apex at `apex`), closed and wound outward.
fn dome_on_column(
    c: [f64; 2],
    base_z: f64,
    (rx, ry): (f64, f64),
    (dome_h, column_h): (f64, f64),
    angle: f64,
    cone_apex: Option<[f64; 2]>,
) -> TriMesh {
    let (ca, sa) = (libm::cos(angle), libm::sin(angle));
    let top = base_z + column_h;
    let mut p: Vec<[f64; 3]> = vec![[c[0], c[1], top + dome_h]];
    for ei in 1..=EL {
        let phi = ei as f64 / EL as f64 * FRAC_PI_2;
        let (rf, zf) = (libm::sin(phi), libm::cos(phi));
        for ai in 0..AZ {
            let th = TAU * ai as f64 / AZ as f64;
            let (lx, ly) = (rx * rf * libm::cos(th), ry * rf * libm::sin(th));
            p.push([
                c[0] + ca * lx - sa * ly,
                c[1] + sa * lx + ca * ly,
                top + dome_h * zf,
            ]);
        }
    }
    let u = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    let mut t: Vec<[u32; 3]> = Vec::new();
    for a in 0..AZ {
        t.push([0, u(1 + a), u(1 + (a + 1) % AZ)]);
    }
    for e in 1..EL {
        let (ra, rb) = (1 + (e - 1) * AZ, 1 + e * AZ);
        for a in 0..AZ {
            let a1 = (a + 1) % AZ;
            t.push([u(ra + a), u(rb + a), u(ra + a1)]);
            t.push([u(ra + a1), u(rb + a), u(rb + a1)]);
        }
    }
    let eq = 1 + (EL - 1) * AZ;
    match cone_apex {
        None => {
            // straight cylinder: a bottom ring under the equator, and a centre
            let bot = p.len();
            for a in 0..AZ {
                let (x, y, _) = (p[eq + a][0], p[eq + a][1], 0.0);
                p.push([x, y, base_z]);
            }
            let centre = p.len();
            p.push([c[0], c[1], base_z]);
            for a in 0..AZ {
                let a1 = (a + 1) % AZ;
                t.push([u(eq + a), u(bot + a), u(eq + a1)]);
                t.push([u(eq + a1), u(bot + a), u(bot + a1)]);
                t.push([u(bot + a), u(centre), u(bot + a1)]);
            }
        }
        Some(ap) => {
            let apex = p.len();
            p.push([ap[0], ap[1], base_z]);
            for a in 0..AZ {
                t.push([u(eq + a), u(apex), u(eq + (a + 1) % AZ)]);
            }
        }
    }
    let mut m = TriMesh {
        positions: p,
        triangles: t,
        ..TriMesh::default()
    };
    if signed_volume(&m) < 0.0 {
        m.triangles.iter_mut().for_each(|t| t.swap(1, 2));
    }
    m
}

impl Flowers {
    /// Petal half-length, half-width and centre radius: adjacent petals
    /// touch at the centre's edge.
    fn petal_dims(&self) -> (f64, f64, f64) {
        let n = self.petals as f64;
        let base_r = self.centre * 0.5;
        let hl = (self.outer - base_r) / 2.0;
        let ctr = (base_r + self.outer) / 2.0;
        let lx = self.centre * libm::cos(PI / n) - ctr;
        let ly = self.centre * libm::sin(PI / n);
        let d2 = 1.0 - (lx / hl).powi(2);
        let hw = if d2 <= 0.0 { ly } else { ly / libm::sqrt(d2) };
        (hl, hw, ctr)
    }

    /// One flower's parts at `c`, its lowest ground `z`, turned `angle`.
    fn meshes(&self, c: [f64; 2], z: f64, angle: f64, s: f64) -> Vec<TriMesh> {
        let base = z - SINK_MM;
        let (stem, dome, petal, centre) = (
            self.stem * s,
            self.dome * s,
            self.petal * s,
            self.centre * s,
        );
        let mut out = vec![dome_on_column(
            c,
            base,
            (centre, centre),
            (dome, stem),
            0.0,
            None,
        )];
        if self.petals > 0 {
            let (hl, hw, ctr) = self.petal_dims();
            for i in 0..self.petals {
                let a = angle + i as f64 * TAU / self.petals as f64;
                let p = [c[0] + ctr * s * libm::cos(a), c[1] + ctr * s * libm::sin(a)];
                out.push(dome_on_column(
                    p,
                    base,
                    (hl * s, hw * s),
                    (petal, stem),
                    a,
                    Some(p),
                ));
            }
        }
        out
    }
}

impl Element for Flowers {
    fn plan(&self) -> Plan {
        Plan {
            writes: Write::Solid(Kind::Flowers),
            reads: vec![
                Read::Solids(Kind::Ground),
                Read::IfAny(Kind::Rock),
                Read::IfAny(Kind::Tree),
                Read::IfAny(Kind::Wall),
            ],
            cell_mm: None,
        }
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let frame = *scene.frame();
        let s = frame.scale();
        let [w, h] = frame.size_mm();
        let (nx, _) = frame.nodes();
        let Some((surface, _)) = scene.surface() else {
            return Err(LookError::general(
                "flowers have no ground to grow on",
                "put a `ground` or `dirt` before them in the stack",
            ));
        };
        let ground = Field::from(&surface);
        let blocker = scene.standing(&[Kind::Rock, Kind::Tree, Kind::Wall], 0.45);
        let wet = scene.wet().clone();
        let n_sq = w * h / (frame.square_mm * frame.square_mm);
        let share = ctx.area.share();
        let n = libm::round(self.per_square * n_sq * share) as usize;
        let mut rng = Rng::new(ctx.seed.key("flowers"));
        let reach = self.outer * s;
        let radii = vec![reach; n];
        let spread = Spread {
            tries: TRIES,
            inset: [0.5 * s, 1.0],
            confine: true,
            gap: 0.5 * s,
            factor: 1.0,
        };
        let density = |x: f64, y: f64| {
            // the flower's whole disc must be dry, in its area, off what stands there
            let ok = (0..8).all(|k| {
                let a = TAU * f64::from(k) / 8.0;
                let (px, py) = (x + reach * libm::cos(a), y + reach * libm::sin(a));
                let (i, j) = ground.node(px.clamp(0.0, w), py.clamp(0.0, h));
                let q = j * nx + i;
                ctx.area.bits[q] && !wet.bits[q] && !blocker.bits[q]
            });
            let (i, j) = ground.node(x, y);
            if ok && ctx.area.bits[j * nx + i] {
                1.0
            } else {
                0.0
            }
        };
        let placed = scatter(&frame, &mut rng, &radii, &spread, &density, &[]);
        let mut parts: Vec<TriMesh> = Vec::new();
        for p in &placed {
            let low = (0..9)
                .map(|k| {
                    let a = TAU * f64::from(k) / 8.0;
                    let r = if k == 8 { 0.0 } else { reach };
                    ground.sample(
                        (p.at[0] + r * libm::cos(a)).clamp(0.0, w),
                        (p.at[1] + r * libm::sin(a)).clamp(0.0, h),
                    )
                })
                .fold(f64::INFINITY, f64::min);
            let angle = rng.uniform(0.0, TAU);
            parts.extend(self.meshes(p.at, low, angle, s));
        }
        let slots = scene
            .slots(
                u32::try_from(parts.len()).map_err(LookError::build)?,
                Material::Flower,
            )
            .map_err(LookError::build)?;
        for (i, m) in parts.iter().enumerate() {
            let solid = form::solid(slots.get(u32::try_from(i).unwrap_or(0)), m)
                .map_err(LookError::build)?;
            scene.add(
                Tag {
                    kind: Kind::Flowers,
                    owner: ctx.path.to_owned(),
                },
                solid,
            );
        }
        scene.note("flowers", placed.len() as f64);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flower at its default settings (ground 3.4 mm): a centre dome of 11.4364 mm³, five petals of
    /// 1.7756 mm³ each (20.3142 in all, a cone's volume does not depend on
    /// where its apex is), from z = 3.3 to 5.3.
    #[test]
    fn a_flower_is_unchanged() {
        let f = Flowers::new(&[]).unwrap();
        let parts = f.meshes([10.0, 10.0], 3.4, 0.0, 1.0);
        assert_eq!(parts.len(), 6);
        let v: Vec<f64> = parts.iter().map(signed_volume).collect();
        assert!((v[0] - 11.4364).abs() < 0.02, "{}", v[0]);
        for p in &v[1..] {
            assert!((p - 1.7756).abs() < 0.02, "{p}");
        }
        let top = parts
            .iter()
            .flat_map(|m| m.positions.iter().map(|p| p[2]))
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((top - 5.3).abs() < 1e-9, "{top}");
    }
}
