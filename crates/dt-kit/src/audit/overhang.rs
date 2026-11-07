//! The overhang test: the mesh sliced into printer layers on a ray
//! lattice, and each downward face flagged under material that droops.

use super::*;

/// The overhang test: the mesh sliced every layer by
/// vertical rays on a `grid_mm` lattice (offset off the mesh's own grids,
/// so no ray grazes an edge); a layer's material farther than `reach +
/// (k − 1) × layer × tan(overhang)` from layer k below it (any k within
/// `span_mm`) droops. Flagged: the downward faces above z = 0.05 under such
/// material.
///
/// The slicing ([`Slices::cut`]) does not depend on the overhang angle;
/// only the flagging ([`Slices::flag`]) does, so one cut serves the gate's
/// angle and the band's.  [`Slices::holes`] counts the rays inside the
/// footprint that hit nothing.
///
/// A void thinner than the printer's `gap` (measured exactly along each
/// ray, not at layer resolution) is filled before the layers are cut: such
/// a gap cannot stay open in print, the layer above lands on the one
/// below, so its roof is not an overhang. This is what a bed joint of
/// 0.11 mm under a wall stone is (walls R10: "the lip over the sub-mm
/// joint prints"); the plain layer-reach rule, written for stacked rock with real
/// gaps, counted every such roof as a drooping shelf.
pub(super) struct Slices {
    pub(super) x0: f64,
    pub(super) y0: f64,
    pub(super) g: f64,
    pub(super) lay: f64,
    pub(super) nx: usize,
    pub(super) ny: usize,
    pub(super) nl: usize,
    pub(super) inside: Vec<bool>,
    pub(super) far: Vec<Vec<f32>>,
    pub(super) hit_any: Vec<bool>,
}

impl Slices {
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn layer_of(&self, z: f64) -> usize {
        ((z / self.lay + 0.5).floor() as i64).clamp(0, self.nl as i64) as usize
    }

    /// `None` for a mesh with no extent on the lattice.
    pub(super) fn cut(m: &TriMesh, p: &Printer) -> Option<Self> {
        let (lo, hi) = bounds(m)?;
        let g = p.grid_mm;
        let (x0, y0) = (lo[0] + 0.0137, lo[1] + 0.0091);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = |a: f64, b: f64| ((b - a) / g).ceil().max(0.0) as usize;
        let (nx, ny) = (count(x0, hi[0]), count(y0, hi[1]));
        if nx == 0 || ny == 0 {
            return None;
        }
        let lay = p.layer_mm;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let nl = ((hi[2] / lay).ceil().max(0.0) as usize) + 1;
        #[allow(clippy::cast_possible_truncation)]
        let layer_of = |z: f64| ((z / lay + 0.5).floor() as i64).clamp(0, nl as i64) as usize;

        // Every (column, z) hit, one list per triangle, gathered in order.
        let hits: Vec<Vec<(usize, f64)>> = m
            .triangles
            .par_iter()
            .map(|t| column_hits(m, t, [x0, y0], g, nx, ny))
            .collect();
        let mut per_col: Vec<Vec<f64>> = vec![Vec::new(); nx * ny];
        for list in &hits {
            for &(col, z) in list {
                per_col[col].push(z);
            }
        }
        let mut occ = vec![0u8; (nl + 1) * nx * ny];
        let mut hit_any = vec![false; nx * ny];
        // each column sorted and gap-filled on its own thread (a column
        // keeps its first hit, so one that was hit stays hit)
        let gap = p.gap_mm;
        per_col.par_iter_mut().for_each(|zs| {
            zs.sort_by(f64::total_cmp);
            // hits alternate in, out, in, …: drop an out and the next in when
            // the void between them is thinner than the printer's gap. An odd
            // count means the column does not alternate (a grazing ray, an
            // open seam): no gap-fill there, the hits are cut as they are.
            let fill = zs.len().is_multiple_of(2);
            let mut kept: Vec<f64> = Vec::with_capacity(zs.len());
            for (k, &z) in zs.iter().enumerate() {
                if fill
                    && k.is_multiple_of(2)
                    && k > 0
                    && kept.len().is_multiple_of(2)
                    && z - zs[k - 1] < gap
                {
                    kept.pop();
                    continue;
                }
                kept.push(z);
            }
            *zs = kept;
        });
        for (col, zs) in per_col.iter().enumerate() {
            hit_any[col] = !zs.is_empty();
            for &z in zs {
                occ[layer_of(z) * nx * ny + col] ^= 1;
            }
        }
        // inside at layer k: an odd count of hits at layers ≤ k
        let mut inside = vec![false; nl * nx * ny];
        let mut odd = vec![0u8; nx * ny];
        for k in 0..nl {
            for col in 0..nx * ny {
                odd[col] ^= occ[k * nx * ny + col];
                inside[k * nx * ny + col] = odd[col] == 1;
            }
        }
        let layer = |k: usize| &inside[k * nx * ny..(k + 1) * nx * ny];
        // distance (mm) to the nearest material of every layer, in parallel
        #[allow(clippy::cast_possible_truncation)]
        let far: Vec<Vec<f32>> = (0..nl)
            .into_par_iter()
            .map(|k| {
                dt_geom::distance(layer(k), nx, ny)
                    .into_iter()
                    .map(|d| (d * g) as f32)
                    .collect()
            })
            .collect();
        Some(Self {
            x0,
            y0,
            g,
            lay,
            nx,
            ny,
            nl,
            inside,
            far,
            hit_any,
        })
    }

    /// The downward faces under material that droops at `p`'s overhang.
    pub(super) fn flag(&self, m: &TriMesh, p: &Printer) -> Vec<bool> {
        let Self {
            x0,
            y0,
            g,
            lay,
            nx,
            ny,
            nl,
            ..
        } = *self;
        let far = &self.far;
        let layer = |k: usize| &self.inside[k * nx * ny..(k + 1) * nx * ny];
        let layer_of = |z: f64| self.layer_of(z);
        let c = lay * libm::tan(p.overhang_deg.to_radians());
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let span = (p.span_mm / lay) as usize;
        let cells = nx * ny;
        // a cell droops when material sits in it with too little below
        // (each layer on its own thread)
        let mut bad = vec![false; nl * cells];
        bad.par_chunks_mut(cells)
            .enumerate()
            .skip(1)
            .for_each(|(i, row)| {
                for (col, b) in row.iter_mut().enumerate() {
                    *b = layer(i)[col]
                        && (1..=span.min(i)).any(|j| {
                            #[allow(clippy::cast_possible_truncation)]
                            let limit = (p.reach_mm + (j - 1) as f64 * c) as f32;
                            far[i - j][col] > limit
                        });
                }
            });
        // a face is flagged under a drooping cell within one cell of it, in
        // its layer or the one above (asked per face, not dilated everywhere)
        let near = |k: usize, ix: usize, iy: usize| {
            (iy.saturating_sub(1)..=(iy + 1).min(ny - 1)).any(|y| {
                (ix.saturating_sub(1)..=(ix + 1).min(nx - 1)).any(|x| bad[k * cells + y * nx + x])
            })
        };
        m.triangles
            .par_iter()
            .map(|t| {
                let [a, b, cc] = t.map(|i| m.positions[i as usize]);
                let f = cross(sub(b, a), sub(cc, a));
                let l = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt();
                if l == 0.0 {
                    return false;
                }
                let n = f.map(|x| x / l);
                let pc = [0, 1, 2].map(|k| (a[k] + b[k] + cc[k]) / 3.0);
                if !(n[2] < 0.0 && pc[2] > 0.05) {
                    return false;
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let at = |v: f64, o: f64, n: usize| {
                    ((v - o) / g).round().clamp(0.0, (n - 1) as f64) as usize
                };
                let ix = at(pc[0] - n[0] * 0.5 * g, x0, nx);
                let iy = at(pc[1] - n[1] * 0.5 * g, y0, ny);
                let kz = layer_of(pc[2]).min(nl - 1);
                near(kz, ix, iy) || (kz + 1 < nl && near(kz + 1, ix, iy))
            })
            .collect()
    }

    /// The rays inside `footprint` that hit nothing.
    pub(super) fn holes(&self, footprint: [f64; 2]) -> usize {
        let Self {
            x0, y0, g, nx, ny, ..
        } = *self;
        let hit_any = &self.hit_any;
        (0..nx * ny)
            .filter(|&col| {
                let (x, y) = (x0 + (col % nx) as f64 * g, y0 + (col / nx) as f64 * g);
                !hit_any[col] && x < footprint[0] && y < footprint[1] && x > 0.0 && y > 0.0
            })
            .count()
    }
}

/// Vertical-ray hits of one triangle: `(column, z)` for every lattice
/// column strictly inside its projection, or on an edge by the top-left
/// rule, so a ray through a shared edge counts once.
pub(super) fn column_hits(
    m: &TriMesh,
    t: &[u32; 3],
    origin: [f64; 2],
    g: f64,
    nx: usize,
    ny: usize,
) -> Vec<(usize, f64)> {
    let mut v = t.map(|i| m.positions[i as usize]);
    let area =
        (v[1][0] - v[0][0]) * (v[2][1] - v[0][1]) - (v[2][0] - v[0][0]) * (v[1][1] - v[0][1]);
    if area == 0.0 {
        return Vec::new();
    }
    if area < 0.0 {
        v.swap(1, 2);
    }
    let area = area.abs();
    let (minx, maxx) = (
        v[0][0].min(v[1][0]).min(v[2][0]),
        v[0][0].max(v[1][0]).max(v[2][0]),
    );
    let (miny, maxy) = (
        v[0][1].min(v[1][1]).min(v[2][1]),
        v[0][1].max(v[1][1]).max(v[2][1]),
    );
    #[allow(clippy::cast_possible_truncation)]
    let span = |lo: f64, hi: f64, o: f64, n: usize| {
        let a = ((lo - o) / g).ceil().max(0.0) as i64;
        let b = (((hi - o) / g).floor() as i64).min(n as i64 - 1);
        (a, b)
    };
    let (i0, i1) = span(minx, maxx, origin[0], nx);
    let (j0, j1) = span(miny, maxy, origin[1], ny);
    // top-left rule on the counter-clockwise projection
    // (exact comparison on purpose: ties are what the rule decides)
    #[allow(clippy::float_cmp)]
    let top_left = |a: [f64; 3], b: [f64; 3]| (a[1] == b[1] && b[0] < a[0]) || b[1] < a[1];
    let edges = [(v[1], v[2]), (v[2], v[0]), (v[0], v[1])];
    let tl = edges.map(|(a, b)| top_left(a, b));
    let mut out = Vec::new();
    for j in j0..=j1 {
        for i in i0..=i1 {
            #[allow(clippy::cast_precision_loss)]
            let (px, py) = (origin[0] + i as f64 * g, origin[1] + j as f64 * g);
            let w = edges.map(|(a, b)| (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0]));
            if (0..3).all(|k| w[k] > 0.0 || (w[k] == 0.0 && tl[k])) {
                let z = (w[0] * v[0][2] + w[1] * v[1][2] + w[2] * v[2][2]) / area;
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                out.push((j as usize * nx + i as usize, z));
            }
        }
    }
    out
}
