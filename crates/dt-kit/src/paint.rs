//! The one painter: cheap acrylic, every material.
//!
//! A dark basecoat, mid tones, and pale highlights drybrushed on whatever
//! stands proud, as the ground and the pool are painted.
//! Each face is coloured by its *local relief*: how far the surface there
//! stands out of, or falls back into, its neighbours (the mesh's own
//! curvature), blended smoothly through the material's ramp in the palette
//! (`library/palettes/default.dtile`). Grass and leaves also paint by their
//! height above the ground, so roots are dark and tips light.
//!
//! Relief is measured on the printed mesh, so the paint shows what the
//! slicer will print; the numbers are normalised for each material (its
//! 3rd to 97th percentile), so a tile with fine grass and a tile with
//! coarse stones are both given their full range of tones.

use dt_geom::TriMesh;
use rayon::prelude::*;

use crate::{Material, Palette};
use dt_geom::vec3::{cross, unit};

/// The overhang paint.
pub const RED: [f32; 3] = [230.0 / 255.0, 30.0 / 255.0, 30.0 / 255.0];
/// Faces in the overhang band (past the printer's 55°, within the 65° cap).
pub const ORANGE: [f32; 3] = [1.0, 0.55, 0.1];

/// Paint per face: the material's colour, red where `overhang` is set.
#[must_use]
pub fn paint(materials: &[Material], overhang: &[bool]) -> Vec<[f32; 3]> {
    materials
        .iter()
        .zip(overhang)
        .map(|(m, &o)| {
            if o {
                RED
            } else {
                match m {
                    Material::Earth => [0.50, 0.38, 0.25],
                    Material::Connector => [0.62, 0.62, 0.64],
                    Material::Rock => [0.58, 0.58, 0.61],
                    Material::Water => [0.24, 0.38, 0.56],
                    Material::Grass => [0.36, 0.56, 0.24],
                    Material::Flower => [0.85, 0.45, 0.62],
                    Material::Masonry => [0.66, 0.64, 0.60],
                    Material::Wood => [0.42, 0.29, 0.18],
                    Material::Leaf => [0.33, 0.52, 0.24],
                }
            }
        })
        .collect()
}

/// flat earth brown.
const BED: [f32; 3] = [128.0, 97.0, 64.0];

/// How far from the blur a node may stand before the blur is taken over
/// the nodes at its own height only, mm: dirt lying on stone, high over
/// the ground round it, has relief against its own surface, not against
/// the ground.
const GATE_MM: f64 = 6.0;
/// The nodes such a blur is taken over: within this of the node's own
/// height, mm.
const GATE_SAME_MM: f64 = 0.8;

/// `blurred` (a Gaussian of `sigma` nodes over `z`) where a node stands near
/// it; where it does not, the Gaussian over the nodes within [`GATE_MM`]
/// of the node's own height.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn gated(z: &[f64], nx: usize, ny: usize, sigma: f64, blurred: Vec<f64>) -> Vec<f64> {
    let reach = (3.0 * sigma).ceil() as i64;
    // the weight of a tap by its squared distance in nodes (a whole number),
    // each the one value the tap's own `exp` gives
    let weight: Vec<f64> = (0..=2 * reach * reach)
        .map(|d2| (-(d2 as f64) / (2.0 * sigma * sigma)).exp())
        .collect();
    let mut out = blurred;
    // every node reads `z` and writes its own: the nodes are independent,
    // so the threads change nothing of the result
    out.par_iter_mut().enumerate().for_each(|(k, o)| {
        let (i, j) = ((k % nx) as i64, (k / nx) as i64);
        // only where the ground round it falls away: a sample of the ring
        let away = (0..12).any(|a| {
            let t = f64::from(a) * std::f64::consts::TAU / 12.0;
            let (ii, jj) = (
                (i as f64 + reach as f64 * t.cos()).round() as i64,
                (j as f64 + reach as f64 * t.sin()).round() as i64,
            );
            ii >= 0
                && jj >= 0
                && ii < nx as i64
                && jj < ny as i64
                && (z[jj as usize * nx + ii as usize] - z[k]).abs() > GATE_MM
        });
        if !away {
            return;
        }
        let (mut sum, mut wsum) = (0.0, 0.0);
        for jj in (j - reach).max(0)..=(j + reach).min(ny as i64 - 1) {
            for ii in (i - reach).max(0)..=(i + reach).min(nx as i64 - 1) {
                let q = (jj as usize) * nx + ii as usize;
                if (z[q] - z[k]).abs() > GATE_SAME_MM {
                    continue;
                }
                let w = weight[((ii - i) * (ii - i) + (jj - j) * (jj - j)) as usize];
                sum += w * z[q];
                wsum += w;
            }
        }
        *o = sum / wsum;
    });
    out
}

/// The ground surface as built, for a painted preview: heights, local
/// relief at two scales, and which nodes are stone.
#[derive(Clone, Debug)]
pub struct SurfacePaint {
    nx: usize,
    ny: usize,
    step: f64,
    z: Vec<f64>,
    rel: Vec<f64>,
    rel2: Vec<f64>,
    /// Relief at 2 mm and 6 mm: what a drybrush catches on the water.
    wrel: Vec<f64>,
    whi: Vec<f64>,
    stone: Vec<bool>,
    covered: Vec<bool>,
    low: f64,
}

impl SurfacePaint {
    /// From the lifted surface grid and its stone marks.
    #[must_use]
    pub fn new(g: &dt_geom::Grid, stone: &dt_geom::Mask) -> Self {
        let z: Vec<f64> = g.z.iter().map(|&v| f64::from(v)).collect();
        let blur = |mm: f64| dt_geom::blur(&z, g.nx, g.ny, mm / g.step);
        let ((b1, b2), (w2, w6)) = rayon::join(
            || rayon::join(|| blur(0.8), || blur(4.0)),
            || rayon::join(|| blur(2.0), || blur(6.0)),
        );
        let low = z.iter().copied().fold(f64::INFINITY, f64::min);
        let (b1, b2) = rayon::join(
            || gated(&z, g.nx, g.ny, 0.8 / g.step, b1),
            || gated(&z, g.nx, g.ny, 4.0 / g.step, b2),
        );
        Self {
            nx: g.nx,
            ny: g.ny,
            step: g.step,
            rel: z.iter().zip(&b1).map(|(a, b)| a - b).collect(),
            rel2: z.iter().zip(&b2).map(|(a, b)| a - b).collect(),
            wrel: z.iter().zip(&w2).map(|(a, b)| a - b).collect(),
            whi: z.iter().zip(&w6).map(|(a, b)| a - b).collect(),
            z,
            stone: stone.bits.clone(),
            covered: Vec::new(),
            low,
        }
    }

    /// Mark the nodes a solid covers (a floor's or wall's footprint):
    /// earth under them is painted as bedding, flat brown, not as
    /// the earth drybrush or the dark tile wall.
    #[must_use]
    pub fn covered(mut self, covered: &dt_geom::Mask) -> Self {
        self.covered.clone_from(&covered.bits);
        self
    }

    /// The ground's height under `c` (x, y), for grass and leaves to paint
    /// by their height above it.
    pub(crate) fn ground_z(&self, c: [f64; 3]) -> f64 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let node = |v: f64, n: usize| (libm::round(v / self.step).max(0.0) as usize).min(n - 1);
        self.z[node(c[1], self.ny) * self.nx + node(c[0], self.nx)]
    }

    /// The water paint at a face centre, or `None` off the grid.
    pub(crate) fn water(&self, c: [f64; 3], pal: &crate::Palette) -> Option<[f32; 3]> {
        #[allow(clippy::cast_precision_loss)]
        let (w, h) = (
            (self.nx - 1) as f64 * self.step,
            (self.ny - 1) as f64 * self.step,
        );
        if c[0] < -1e-6 || c[1] < -1e-6 || c[0] > w + 1e-6 || c[1] > h + 1e-6 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let node = |v: f64, n: usize| (libm::round(v / self.step).max(0.0) as usize).min(n - 1);
        let k = node(c[1], self.ny) * self.nx + node(c[0], self.nx);
        #[allow(clippy::cast_possible_truncation)]
        let t = (0.5 + 0.5 * (0.65 * self.wrel[k] / 0.5 + 0.35 * self.whi[k] / 0.9)).clamp(0.0, 1.0)
            as f32;
        Some(pal.sample("water", t))
    }

    /// The earth paint for one face (centre `c`, unit normal z `nz`), or
    /// `None` off the surface's footprint.
    pub(crate) fn face(&self, c: [f64; 3], nz: f64, pal: &crate::Palette) -> Option<[f32; 3]> {
        #[allow(clippy::cast_precision_loss)]
        let w = (self.nx - 1) as f64 * self.step;
        #[allow(clippy::cast_precision_loss)]
        let h = (self.ny - 1) as f64 * self.step;
        if c[0] < -1e-6 || c[1] < -1e-6 || c[0] > w + 1e-6 || c[1] > h + 1e-6 || c[2] < 0.0 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let node = |v: f64, n: usize| (libm::round(v / self.step).max(0.0) as usize).min(n - 1);
        let k = node(c[1], self.ny) * self.nx + node(c[0], self.nx);
        let (zs, rel) = (self.z[k], self.rel[k]);
        // under a floor or wall the earth only shows as bedding: on the
        // tile's side and on the sliver of bed beside a stone
        if self.covered.get(k).copied().unwrap_or(false) {
            return Some(BED.map(|v| v / 255.0));
        }
        if (nz.abs() < 0.35 && c[2] < zs - 0.3) || c[2] < self.low - 0.3 {
            return Some(pal.sample("side", 0.0));
        }
        #[allow(clippy::cast_possible_truncation)]
        if self.stone[k] || c[2] > zs + 0.06 {
            let t = (0.5 + 0.5 * nz - 0.4 + 0.6 * rel / 0.5) as f32;
            return Some(pal.sample("pebble", t));
        }
        #[allow(clippy::cast_possible_truncation)]
        let t = (0.5 + 0.7 * rel / 0.35 + 0.3 * self.rel2[k] / 1.0) as f32;
        Some(pal.sample("earth", t))
    }
}

/// [`paint_mesh`] with the default palette, no overhang paint beyond
/// `overhang`, and stone masonry.
#[must_use]
pub fn paint_surface(
    mesh: &TriMesh,
    materials: &[Material],
    overhang: &[bool],
    surface: Option<&SurfacePaint>,
) -> Vec<[f32; 3]> {
    crate::paint::paint_mesh(
        mesh,
        materials,
        overhang,
        &[],
        surface,
        &crate::Palette::default(),
        false,
    )
}

/// Per vertex: how far it stands out of its neighbours, in units of the
/// local edge length, at a fine scale and a coarse one (positive = ridge
/// or tip, negative = hollow or joint).
fn relief(mesh: &TriMesh) -> (Vec<f32>, Vec<f32>) {
    let n = mesh.positions.len();
    let mut normal = vec![[0.0f64; 3]; n];
    let mut sum = vec![[0.0f64; 3]; n];
    let mut len = vec![0.0f64; n];
    let mut cnt = vec![0u32; n];
    for t in &mesh.triangles {
        let p = t.map(|i| mesh.positions[i as usize]);
        let fnorm = cross(
            [0, 1, 2].map(|k| p[1][k] - p[0][k]),
            [0, 1, 2].map(|k| p[2][k] - p[0][k]),
        );
        for a in 0..3 {
            let i = t[a] as usize;
            for k in 0..3 {
                normal[i][k] += fnorm[k];
            }
            for o in [(a + 1) % 3, (a + 2) % 3] {
                let q = p[o];
                for k in 0..3 {
                    sum[i][k] += q[k];
                }
                len[i] += ((q[0] - p[a][0]).powi(2)
                    + (q[1] - p[a][1]).powi(2)
                    + (q[2] - p[a][2]).powi(2))
                .sqrt();
                cnt[i] += 1;
            }
        }
    }
    let fine: Vec<f32> = (0..n)
        .map(|i| {
            if cnt[i] == 0 {
                return 0.0;
            }
            let c = f64::from(cnt[i]);
            let nrm = unit(normal[i]);
            let p = mesh.positions[i];
            let d = [0, 1, 2].map(|k| sum[i][k] / c - p[k]);
            let edge = (len[i] / c).max(1e-9);
            #[allow(clippy::cast_possible_truncation)]
            let r = (-(d[0] * nrm[0] + d[1] * nrm[1] + d[2] * nrm[2]) / edge) as f32;
            r
        })
        .collect();
    // a coarser scale: the fine relief averaged over the neighbours, twice
    let mut coarse = fine.clone();
    for _ in 0..2 {
        let mut acc = vec![0.0f32; n];
        for t in &mesh.triangles {
            for a in 0..3 {
                for o in [(a + 1) % 3, (a + 2) % 3] {
                    acc[t[a] as usize] += coarse[t[o] as usize];
                }
            }
        }
        for i in 0..n {
            if cnt[i] > 0 {
                #[allow(clippy::cast_precision_loss)]
                {
                    acc[i] /= cnt[i] as f32;
                }
            }
        }
        coarse = acc;
    }
    (fine, coarse)
}

/// The value below which `share` of `v` lies.
fn percentile(v: &mut [f32], share: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let k = ((v.len() - 1) as f32 * share).round() as usize;
    *v.select_nth_unstable_by(k, f32::total_cmp).1
}

/// Where `x` lies between the `lo` and `hi` percentiles of `v`, 0 to 1.
struct Scale {
    lo: f32,
    span: f32,
}

impl Scale {
    fn of(mut v: Vec<f32>) -> Self {
        let lo = percentile(&mut v, 0.03);
        let hi = percentile(&mut v, 0.97);
        Self {
            lo,
            span: (hi - lo).max(1e-6),
        }
    }

    fn at(&self, x: f32) -> f32 {
        ((x - self.lo) / self.span).clamp(0.0, 1.0)
    }
}

/// One material's three scales: fine relief, coarse relief, height.
type Scales = (Scale, Scale, Scale);

/// What one face is measured as: relief, coarse relief, up-facing, height
/// above the ground, centre and unit normal.
type Measure = (f32, f32, f32, f32, [f64; 3], [f64; 3]);

const fn slot(m: Material) -> usize {
    match m {
        Material::Earth => 0,
        Material::Connector => 1,
        Material::Rock => 2,
        Material::Water => 3,
        Material::Grass => 4,
        Material::Flower => 5,
        Material::Masonry => 6,
        Material::Wood => 7,
        Material::Leaf => 8,
    }
}

const fn ramp_name(m: Material, brick: bool) -> &'static str {
    match m {
        Material::Earth => "earth",
        Material::Connector => "connector",
        Material::Rock => "rock",
        Material::Water => "water",
        Material::Grass => "grass",
        Material::Flower => "flower",
        Material::Masonry => {
            if brick {
                "brick"
            } else {
                "masonry"
            }
        }
        Material::Wood => "wood",
        Material::Leaf => "leaf",
    }
}

/// **paint**: each face's colour. Earth and water are painted from the
/// ground surface when there is one;
/// everything else from the mesh's own relief. `overhang` faces are red
/// and `band` faces orange, over every other paint. `brick` paints
/// masonry from the brick ramp.
///
/// # Panics
/// When `materials` is shorter than the mesh's faces (a bug in the caller).
#[must_use]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn paint_mesh(
    mesh: &TriMesh,
    materials: &[Material],
    overhang: &[bool],
    band: &[bool],
    surface: Option<&SurfacePaint>,
    palette: &Palette,
    brick: bool,
) -> Vec<[f32; 3]> {
    let nf = mesh.triangles.len();
    let (fine, coarse) = relief(mesh);
    // per face: relief, coarse relief, up-facing, height above the ground
    // every face is read and written alone, so the threads change nothing
    let per: Vec<Measure> = mesh
        .triangles
        .par_iter()
        .map(|t| {
            let p = t.map(|i| mesh.positions[i as usize]);
            let c = [0, 1, 2].map(|d| (p[0][d] + p[1][d] + p[2][d]) / 3.0);
            let n = unit(cross(
                [0, 1, 2].map(|d| p[1][d] - p[0][d]),
                [0, 1, 2].map(|d| p[2][d] - p[0][d]),
            ));
            let r = t.iter().map(|&i| fine[i as usize]).sum::<f32>() / 3.0;
            let rc = t.iter().map(|&i| coarse[i as usize]).sum::<f32>() / 3.0;
            #[allow(clippy::cast_possible_truncation)]
            let (u, h) = (
                (0.5 + 0.5 * n[2]) as f32,
                surface.map_or(c[2], |s| c[2] - s.ground_z(c)) as f32,
            );
            (r, rc, u, h, c, n)
        })
        .collect();
    let fr: Vec<f32> = per.iter().map(|f| f.0).collect();
    let fc: Vec<f32> = per.iter().map(|f| f.1).collect();
    let up: Vec<f32> = per.iter().map(|f| f.2).collect();
    let hgt: Vec<f32> = per.iter().map(|f| f.3).collect();
    let cen: Vec<[f64; 3]> = per.iter().map(|f| f.4).collect();
    let nrm: Vec<[f64; 3]> = per.iter().map(|f| f.5).collect();
    // one scale per material
    let mut scales: Vec<Option<Scales>> = (0..9).map(|_| None).collect();
    let made: Vec<(Material, Option<Scales>)> = [
        Material::Earth,
        Material::Connector,
        Material::Rock,
        Material::Water,
        Material::Grass,
        Material::Flower,
        Material::Masonry,
        Material::Wood,
        Material::Leaf,
    ]
    .into_par_iter()
    .map(|m| {
        let of = |v: &[f32]| -> Vec<f32> {
            materials
                .iter()
                .zip(v)
                .filter(|(mm, _)| **mm == m)
                .map(|(_, x)| *x)
                .collect()
        };
        let r = of(&fr);
        if r.is_empty() {
            return (m, None);
        }
        (
            m,
            Some((Scale::of(r), Scale::of(of(&fc)), Scale::of(of(&hgt)))),
        )
    })
    .collect();
    for (m, sc) in made {
        scales[slot(m)] = sc;
    }
    let mut out: Vec<[f32; 3]> = (0..nf)
        .into_par_iter()
        .map(|k| {
            // every caller builds `materials` face for face from the same mesh
            let m = materials
                .get(k)
                .copied()
                .expect("paint_mesh: one material per face");
            if overhang.get(k).copied().unwrap_or(false) {
                return RED;
            }
            let ground = |m: Material| -> Option<[f32; 3]> {
                let s = surface?;
                match m {
                    Material::Earth => s.face(cen[k], nrm[k][2], palette),
                    Material::Water => s.water(cen[k], palette),
                    _ => None,
                }
            };
            if let Some(c) = ground(m) {
                return c;
            }
            let Some((sr, sc, sh)) = &scales[slot(m)] else {
                return [0.7; 3];
            };
            let (r, c, h) = (sr.at(fr[k]), sc.at(fc[k]), sh.at(hgt[k]));
            // the drybrush catches what stands out, and leans to what faces up
            let t = match m {
                // roots dark, tips light
                Material::Grass => 0.35 * r + 0.20 * c + 0.45 * h,
                Material::Leaf => 0.12 * r + 0.40 * c + 0.20 * h + 0.28 * up[k],
                Material::Flower => 0.5 * r + 0.2 * c + 0.3 * up[k],
                _ => 0.62 * r + 0.18 * c + 0.20 * up[k],
            };
            // a little contrast: the darks deeper, the pale edges brighter
            let t = ((t - 0.5) * 1.25 + 0.5).clamp(0.0, 1.0);
            // drybrush is sparing: most of a surface stays in the base tones
            let t = t.powf(match m {
                Material::Grass | Material::Flower => 1.5,
                _ => 2.0,
            });
            palette.sample(ramp_name(m, brick), t)
        })
        .collect();
    for (c, _) in out.iter_mut().zip(band).filter(|(_, b)| **b) {
        *c = ORANGE;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_palette_has_every_ramp() {
        let p = Palette::default();
        for name in crate::RAMPS {
            let (lo, hi) = (p.sample(name, 0.0), p.sample(name, 1.0));
            assert_ne!(lo, [0.7; 3], "`{name}` has no ramp");
            if name != "side" {
                assert!(
                    lo.iter().sum::<f32>() < hi.iter().sum::<f32>(),
                    "`{name}` goes from a dark basecoat to pale highlights"
                );
            }
        }
    }

    #[test]
    fn a_palette_document_overrides_one_ramp_and_keeps_the_rest() {
        let p = Palette::parse("ramp rock \"#000000\" \"#ff0000@0.5\" \"#ffffff\"").unwrap();
        assert_eq!(p.sample("rock", 0.0), [0.0; 3]);
        assert_eq!(p.sample("rock", 0.5), [1.0, 0.0, 0.0]);
        assert_eq!(p.sample("rock", 1.0), [1.0; 3]);
        assert_eq!(
            p.sample("grass", 0.0),
            Palette::default().sample("grass", 0.0)
        );
        assert!(Palette::parse("ramp nonsense \"#000000\" \"#ffffff\"").is_err());
        assert!(Palette::parse("ramp rock \"#000000\"").is_err());
        assert!(Palette::parse("ramp rock \"#000000@0.9\" \"#ffffff@0.1\"").is_err());
    }

    /// A flat plate with one raised bump: the bump takes the pale tones.
    fn bumpy() -> TriMesh {
        let n = 12;
        let mut m = TriMesh::default();
        for j in 0..=n {
            for i in 0..=n {
                let (x, y) = (f64::from(i), f64::from(j));
                let d2 = (x - 6.0).powi(2) + (y - 6.0).powi(2);
                m.positions.push([x, y, 3.0 * (-d2 / 4.0).exp()]);
            }
        }
        let at = |i: u32, j: u32| j * (n + 1) + i;
        for j in 0..n {
            for i in 0..n {
                m.triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                m.triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        m
    }

    #[test]
    fn raised_detail_is_painted_paler_and_overhang_stays_red() {
        let m = bumpy();
        let mats = vec![Material::Rock; m.triangles.len()];
        let none = vec![false; mats.len()];
        let p = Palette::default();
        let c = paint_mesh(&m, &mats, &none, &none, None, &p, false);
        let lum = |c: [f32; 3]| c[0] + c[1] + c[2];
        // the faces around the bump's crest against the faces at the corner
        let crest = (0..m.triangles.len())
            .max_by(|&a, &b| {
                let z = |k: usize| {
                    m.triangles[k]
                        .iter()
                        .map(|&i| m.positions[i as usize][2])
                        .sum::<f64>()
                };
                z(a).total_cmp(&z(b))
            })
            .unwrap();
        assert!(lum(c[crest]) > lum(c[0]), "{:?} vs {:?}", c[crest], c[0]);
        let mut over = none.clone();
        over[crest] = true;
        let c = paint_mesh(&m, &mats, &over, &none, None, &p, false);
        assert_eq!(c[crest], RED);
        let mut band = none.clone();
        band[3] = true;
        assert_eq!(
            paint_mesh(&m, &mats, &none, &band, None, &p, false)[3],
            ORANGE
        );
        // brick masonry is painted from its own ramp
        let mats = vec![Material::Masonry; m.triangles.len()];
        assert_ne!(
            paint_mesh(&m, &mats, &none, &none, None, &p, true)[crest],
            paint_mesh(&m, &mats, &none, &none, None, &p, false)[crest]
        );
    }
}
