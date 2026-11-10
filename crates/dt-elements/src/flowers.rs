//! `flower`: a thing like a rock or a tree, a thin dome on a support
//! column for the centre, and one for each petal, an inverted cone under
//! each petal so it prints standing.
//!
//! A flower is a centre dome on a straight cylinder, ringed by `petals`
//! elongated domes, each on an inverted cone. The petals touch, not
//! overlap, at the centre's edge. A flower is placed and sized as every
//! thing is: one flower stands `at=` a point or somewhere in its region,
//! and a `scatter { flower }` sows many. Flowers stand where the grass
//! would; the grass reads them (a thing standing on the ground) and grows
//! round.
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

use dt_kit::{
    Field, Material, Placed, Rng, Scene, Spread, Tag, TriMesh, form, scatter, signed_volume,
};

use crate::read::Settings;
use crate::{Child, Control, Ctx, Draw, Element, LookError, Plan, Read, Write};

const AZ: usize = 16;
const EL: usize = 8;
const SINK_MM: f64 = 0.10;
const TRIES: u32 = 300;

/// The width across the petals' tips at the default size, mm at the 35 mm
/// square.
const SIZE_MM: f64 = 5.0;
/// How far flowers keep from one another, mm at the 35 mm square.
const GAP_MM: f64 = 0.5;
/// Placements a square gets with no `perSquare` written.
const PER_SQUARE: f64 = 4.0;

/// What one flower's controls set.
#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    /// The width across the petals' tips: a range is drawn, with its odds.
    size: Draw,
    petals: usize,
    centre: f64,
    stem: f64,
    dome: f64,
    petal: f64,
    /// Its share of a scatter's mix.
    weight: f64,
}

impl Plant {
    /// From a flower's controls.
    ///
    /// # Errors
    /// [`LookError`] for a value of the wrong shape, or a size whose petals
    /// reach no further than the centre.
    pub fn new(controls: &[Control], weight: f64) -> Result<Self, LookError> {
        let s = Settings::new(controls);
        let mut f = Self {
            size: Draw::flat(SIZE_MM, SIZE_MM),
            petals: 5,
            centre: 1.5,
            stem: 1.0,
            dome: 1.0,
            petal: 0.5,
            weight,
        };
        f.size = s.dist_or("size", f.size)?;
        if let Some(n) = s.number("petals")? {
            f.petals = n.round() as usize;
        }
        f.centre = s.number_or("centre", f.centre)?;
        f.stem = s.number_or("stem", f.stem)?;
        f.dome = s.number_or("dome", f.dome)?;
        f.petal = s.number_or("petal", f.petal)?;
        if f.size.lo() / 2.0 <= f.centre {
            return Err(LookError {
                control: Some("size".into()),
                message: format!(
                    "the petals reach {} mm, no further than the centre's {} mm",
                    f.size.lo() / 2.0,
                    f.centre
                ),
                help: Some("make `size` more than twice the `centre`".into()),
                code: None,
            });
        }
        Ok(f)
    }

    /// Its share of a scatter's mix.
    pub(crate) fn weight(&self) -> f64 {
        self.weight
    }

    /// One draw of the size, then the reach: the centre to a petal's tip.
    pub(crate) fn reach(&self, rng: &mut Rng) -> f64 {
        self.size.draw(rng) / 2.0
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

impl Plant {
    /// Petal half-length, half-width and centre radius for petals reaching
    /// `outer`: adjacent petals touch at the centre's edge.
    fn petal_dims(&self, outer: f64) -> (f64, f64, f64) {
        let n = self.petals as f64;
        let base_r = self.centre * 0.5;
        let hl = (outer - base_r) / 2.0;
        let ctr = (base_r + outer) / 2.0;
        let lx = self.centre * libm::cos(PI / n) - ctr;
        let ly = self.centre * libm::sin(PI / n);
        let d2 = 1.0 - (lx / hl).powi(2);
        let hw = if d2 <= 0.0 { ly } else { ly / libm::sqrt(d2) };
        (hl, hw, ctr)
    }

    /// One flower's parts at `c`, petals reaching `outer`, its lowest
    /// ground `z`, turned `angle`.
    fn meshes(&self, outer: f64, c: [f64; 2], z: f64, angle: f64, s: f64) -> Vec<TriMesh> {
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
            let (hl, hw, ctr) = self.petal_dims(outer);
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

/// What flowers read: the ground they stand on, and whatever would stand
/// over them.
fn plan() -> Plan {
    Plan {
        writes: Write::Solid(Kind::Flowers),
        reads: vec![
            Read::Solids(Kind::Terrain),
            Read::IfAny(Kind::Rock),
            Read::IfAny(Kind::Tree),
            Read::IfAny(Kind::Wall),
        ],
        cell_mm: None,
    }
}

/// Stand the `flowers` (each its plant and the reach it was drawn)
/// at `placed`, on the ground, each turned a fresh angle; add them to the
/// scene.
pub(crate) fn stand(
    scene: &mut Scene,
    ctx: &Ctx<'_>,
    rng: &mut Rng,
    flowers: &[(&Plant, f64)],
    placed: &[Placed],
) -> Result<(), LookError> {
    let frame = *scene.frame();
    let s = frame.scale();
    let [w, h] = frame.size_mm();
    let Some((surface, _)) = scene.surface() else {
        return Err(LookError::general(
            "flowers have no ground to grow on",
            "put a `ground` or `dirt` before them in the stack",
        ));
    };
    let ground = Field::from(&surface);
    let mut parts: Vec<TriMesh> = Vec::new();
    for p in placed {
        let (plant, outer) = flowers[p.index];
        let reach = outer * s;
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
        parts.extend(plant.meshes(outer, p.at, low, angle, s));
    }
    let slots = scene
        .slots(
            u32::try_from(parts.len()).map_err(LookError::build)?,
            Material::Flower,
        )
        .map_err(LookError::build)?;
    for (i, m) in parts.iter().enumerate() {
        let solid =
            form::solid(slots.get(u32::try_from(i).unwrap_or(0)), m).map_err(LookError::build)?;
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

/// Dart-throw the flowers' discs (each its reach, mm at the 35 mm square)
/// into the entry's area: every disc dry, in the area and off what stands
/// there. `gap` is mm at the 35 mm square.
///
/// `also` are discs (x, y, r) of other things already placed, kept clear of.
pub(crate) fn place(
    scene: &Scene,
    area: &dt_kit::Mask,
    rng: &mut Rng,
    reaches: &[f64],
    gap: f64,
    also: &[[f64; 3]],
) -> Result<Vec<Placed>, LookError> {
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
    let radii: Vec<f64> = reaches.iter().map(|r| r * s).collect();
    let widest = radii.iter().copied().fold(0.0, f64::max);
    let spread = Spread {
        tries: TRIES,
        inset: [0.5 * s, 1.0],
        confine: true,
        gap: gap * s,
        factor: 1.0,
    };
    let density = |x: f64, y: f64| {
        // the flower's whole disc must be dry, in its area, off what stands there
        let ok = (0..8).all(|k| {
            let a = TAU * f64::from(k) / 8.0;
            let (px, py) = (x + widest * libm::cos(a), y + widest * libm::sin(a));
            let (i, j) = ground.node(px.clamp(0.0, w), py.clamp(0.0, h));
            let q = j * nx + i;
            area.bits[q] && !wet.bits[q] && !blocker.bits[q]
        });
        let (i, j) = ground.node(x, y);
        if ok && area.bits[j * nx + i] {
            1.0
        } else {
            0.0
        }
    };
    Ok(scatter(&frame, rng, &radii, &spread, &density, also))
}

/// One flower: at `at=` a point, or somewhere in its region.
#[derive(Clone, Debug)]
pub struct Flower {
    plant: Plant,
}

impl Flower {
    /// From controls.
    ///
    /// # Errors
    /// [`LookError`] for a control of the wrong shape or a size out of range.
    pub fn new(controls: &[Control]) -> Result<Self, LookError> {
        Ok(Self {
            plant: Plant::new(controls, 1.0)?,
        })
    }
}

impl Element for Flower {
    fn plan(&self) -> Plan {
        plan()
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        let frame = *scene.frame();
        let s = frame.scale();
        let sq = frame.square_mm;
        let mut rng = Rng::new(ctx.seed.key("flower"));
        let outer = self.plant.reach(&mut rng);
        let placed = match ctx.at {
            Some([dx, dy]) => {
                // a range is drawn from the entry's seed, with its odds; a number never draws
                let x = if dx.is_point() {
                    dx.lo()
                } else {
                    dx.draw(&mut rng)
                };
                let y = if dy.is_point() {
                    dy.lo()
                } else {
                    dy.draw(&mut rng)
                };
                let at = [x * sq, y * sq];
                let [w, h] = frame.size_mm();
                let m = outer * s + 0.5 * s;
                if at[0] < m || at[1] < m || at[0] > w - m || at[1] > h - m {
                    return Err(LookError::general(
                        format!(
                            "a flower {:.1} mm wide at this point stands past the tile's edge",
                            outer * 2.0
                        ),
                        format!("keep it {:.2} squares from every edge", m / sq),
                    ));
                }
                vec![Placed {
                    index: 0,
                    at,
                    r: outer * s,
                }]
            }
            None => {
                let placed = place(scene, ctx.area, &mut rng, &[outer], GAP_MM, &[])?;
                if placed.is_empty() {
                    return Err(LookError::build(format!(
                        "no room for a flower {:.1} mm wide here",
                        outer * 2.0
                    )));
                }
                placed
            }
        };
        stand(scene, ctx, &mut rng, &[(&self.plant, outer)], &placed)
    }
}

/// `scatter { flower }`: many flowers, by count or per square.
#[derive(Clone, Debug)]
pub struct Scatter {
    count: Option<Draw>,
    per_square: Draw,
    gap: f64,
    reach: f64,
    plants: Vec<Plant>,
}

impl Scatter {
    /// From its controls and its children (each a flower and its weight).
    ///
    /// # Errors
    /// [`LookError`] for a child that is not a flower, or bad controls.
    pub fn new(controls: &[Control], children: &[Child]) -> Result<Self, LookError> {
        let set = Settings::new(controls);
        let mut plants = Vec::new();
        for ch in children {
            if ch.name != "flower" {
                return Err(LookError::general(
                    format!("`{}` is not a flower", ch.name),
                    "a scatter of flowers alone holds flowers; a mix goes through the mixed scatter",
                ));
            }
            plants.push(Plant::new(&ch.controls, ch.weight)?);
        }
        if plants.is_empty() {
            return Err(LookError::general(
                "a scatter needs a child to say what it scatters",
                "write `scatter perSquare=4 { flower }`",
            ));
        }
        Ok(Self {
            count: set.dist("count")?,
            per_square: set.dist_or("perSquare", Draw::flat(PER_SQUARE, PER_SQUARE))?,
            gap: set.number_or("gap", GAP_MM)?,
            reach: set.number_or("reach", crate::LINE_REACH_MM)?,
            plants,
        })
    }
}

impl Element for Scatter {
    fn plan(&self) -> Plan {
        plan()
    }

    fn build(&self, scene: &mut Scene, ctx: &Ctx<'_>) -> Result<(), LookError> {
        if ctx.against.is_some() {
            return Err(LookError::general(
                "flowers are not gathered at a foot",
                "scatter flowers `at=` a region",
            ));
        }
        let frame = *scene.frame();
        let mut rng = Rng::new(ctx.seed.key("flowers"));
        // at= a line: the band either side of it
        let band = ctx.spread_area(&frame, self.reach);
        let squares = ctx.spread_squares(&frame, &band);
        // a whole number: the ends round and every one is as likely as any
        let n = match &self.count {
            Some(r) => r.int_by(1.0, &mut rng),
            None => self.per_square.int_by(squares, &mut rng),
        };
        #[allow(clippy::cast_sign_loss)]
        let n = n.clamp(0, 10_000) as usize;
        let total: f64 = self.plants.iter().map(|p| p.weight).sum();
        let mut flowers: Vec<(&Plant, f64)> = Vec::with_capacity(n);
        for i in 0..n {
            let mut r = Rng::new(ctx.seed.key("item").index(i as u64));
            let mut u = r.random() * total;
            let plant = self
                .plants
                .iter()
                .find(|p| {
                    let hit = u < p.weight;
                    u -= p.weight;
                    hit
                })
                .unwrap_or(&self.plants[self.plants.len() - 1]);
            flowers.push((plant, plant.reach(&mut r)));
        }
        let reaches: Vec<f64> = flowers.iter().map(|f| f.1).collect();
        let placed = place(scene, &band, &mut rng, &reaches, self.gap, &[])?;
        stand(scene, ctx, &mut rng, &flowers, &placed)
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
        let f = Plant::new(&[], 1.0).unwrap();
        let parts = f.meshes(SIZE_MM / 2.0, [10.0, 10.0], 3.4, 0.0, 1.0);
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

    #[test]
    fn petals_that_reach_no_further_than_the_centre_are_refused() {
        let c = Control {
            name: "size".into(),
            value: crate::Value::Number(3.0),
        };
        let e = Plant::new(&[c], 1.0).unwrap_err();
        assert_eq!(e.control.as_deref(), Some("size"));
    }
}
