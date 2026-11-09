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

use crate::render::{ORANGE, RED, SurfacePaint};
use crate::{Material, Palette};
use dt_geom::vec3::{cross, unit};

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
    let mut fr = vec![0.0f32; nf];
    let mut fc = vec![0.0f32; nf];
    let mut up = vec![0.0f32; nf];
    let mut hgt = vec![0.0f32; nf];
    let mut cen = vec![[0.0f64; 3]; nf];
    let mut nrm = vec![[0.0f64; 3]; nf];
    for (k, t) in mesh.triangles.iter().enumerate() {
        let p = t.map(|i| mesh.positions[i as usize]);
        let c = [0, 1, 2].map(|d| (p[0][d] + p[1][d] + p[2][d]) / 3.0);
        let n = unit(cross(
            [0, 1, 2].map(|d| p[1][d] - p[0][d]),
            [0, 1, 2].map(|d| p[2][d] - p[0][d]),
        ));
        cen[k] = c;
        nrm[k] = n;
        fr[k] = t.iter().map(|&i| fine[i as usize]).sum::<f32>() / 3.0;
        fc[k] = t.iter().map(|&i| coarse[i as usize]).sum::<f32>() / 3.0;
        #[allow(clippy::cast_possible_truncation)]
        {
            up[k] = (0.5 + 0.5 * n[2]) as f32;
            hgt[k] = surface.map_or(c[2], |s| c[2] - s.ground_z(c)) as f32;
        }
    }
    // one scale per material
    let mut scales: Vec<Option<(Scale, Scale, Scale)>> = (0..9).map(|_| None).collect();
    for m in [
        Material::Earth,
        Material::Connector,
        Material::Rock,
        Material::Water,
        Material::Grass,
        Material::Flower,
        Material::Masonry,
        Material::Wood,
        Material::Leaf,
    ] {
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
            continue;
        }
        scales[slot(m)] = Some((Scale::of(r), Scale::of(of(&fc)), Scale::of(of(&hgt))));
    }
    let mut out = Vec::with_capacity(nf);
    for k in 0..nf {
        // every caller builds `materials` face for face from the same mesh
        let m = materials
            .get(k)
            .copied()
            .expect("paint_mesh: one material per face");
        if overhang.get(k).copied().unwrap_or(false) {
            out.push(RED);
            continue;
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
            out.push(c);
            continue;
        }
        let Some((sr, sc, sh)) = &scales[slot(m)] else {
            out.push([0.7; 3]);
            continue;
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
        out.push(palette.sample(ramp_name(m, brick), t));
    }
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
