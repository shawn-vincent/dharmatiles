//! **stand**: cut away what cannot be printed standing on what is below
//! it. The layer-reach rule of the audit ([`crate::audit`]), run as a
//! construction instead of a check: the part is sliced into layers on a
//! fine column grid and, bottom up, each layer keeps only the material
//! within reach of what the layer below kept (or of the support). What
//! hangs in the air beyond that becomes one bevel at the reach angle,
//! rising from whatever holds it up. The cut is made by carving with a
//! level set of the removed voxels, so the rest of the surface is
//! untouched (P5: foliage clumps standing on their branches).

use dt_geom::TriMesh;
use rayon::prelude::*;

use crate::scene::{Material, Scene, Solid};
use crate::{KitError, carve, form};

/// How [`stand`] slices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stand {
    /// Layer height, mm.
    pub layer_mm: f64,
    /// Column spacing, mm.
    pub grid_mm: f64,
    /// The flattest an underside may be, degrees above horizontal (35°
    /// is 55° from vertical; a degree or two above that clears the audit's
    /// sampling).
    pub angle_deg: f64,
    /// One layer may reach this far past the one below, mm (the audit
    /// allows 0.6).
    pub reach_mm: f64,
    /// How many mm of layers below are compared (the audit's 3).
    pub span_mm: f64,
}

impl Default for Stand {
    fn default() -> Self {
        Self {
            layer_mm: 0.2,
            grid_mm: 0.1,
            angle_deg: 37.0,
            reach_mm: 0.45,
            span_mm: 3.0,
        }
    }
}

/// What [`stand`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stood {
    /// Volume cut away, mm³ (voxel estimate).
    pub cut_mm3: f64,
    /// Separate pieces cut.
    pub pieces: usize,
}

/// **stand**: `part` less what cannot stand on itself or on `support`
/// (nothing is cut from `support`). The cutters take slots of `material`
/// (the part's) from `scene`, in sequence. The bed (z ≤ one layer)
/// always supports.
///
/// # Errors
/// The kernel's, or [`KitError::Slots`].
pub fn stand(
    scene: &mut Scene,
    part: &Solid,
    support: &Solid,
    material: Material,
    s: &Stand,
) -> Result<(Solid, Stood), KitError> {
    let t = part.mesh();
    let w = support.mesh();
    let Some((lo, hi)) = bounds(&t) else {
        return Ok((part.clone(), Stood::default()));
    };
    let g = s.grid_mm;
    let lay = s.layer_mm;
    let lo = [lo[0] - 2.0 * g, lo[1] - 2.0 * g, lo[2] - lay];
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = |a: f64, b: f64, d: f64| (((b - a) / d).ceil().max(1.0) as usize) + 2;
    let (nx, ny, nz) = (
        count(lo[0], hi[0], g),
        count(lo[1], hi[1], g),
        count(lo[2], hi[2], lay),
    );
    let cols = nx * ny;
    // the centre of voxel (i, j, k)
    #[allow(clippy::cast_precision_loss)]
    let centre = |i: usize, j: usize, k: usize| {
        [
            lo[0] + (i as f64 + 0.5) * g,
            lo[1] + (j as f64 + 0.5) * g,
            lo[2] + (k as f64 + 0.5) * lay,
        ]
    };
    let inside_t = voxels(&t, lo, g, lay, nx, ny, nz);
    let inside_w = voxels(&w, lo, g, lay, nx, ny, nz);
    // the audit's rule (audit.rs, `overhang_faces`), with margins: a
    // voxel stays if, for every j up to the span, material j layers down
    // lies within reach + (j − 1) × the cone's step
    let cone = lay / libm::tan(s.angle_deg.to_radians());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let span = ((s.span_mm / lay) as usize).max(1);
    let mut keep = vec![false; nz * cols];
    let mut far: Vec<Vec<f64>> = Vec::with_capacity(nz);
    for k in 0..nz {
        let z = centre(0, 0, k)[2];
        let layer: Vec<bool> = if z <= lay {
            (0..cols).map(|c| inside_t[k * cols + c]).collect()
        } else {
            let far = &far;
            (0..cols)
                .into_par_iter()
                .map(|c| {
                    inside_t[k * cols + c]
                        && (1..=span.min(k)).all(|j| {
                            #[allow(clippy::cast_precision_loss)]
                            let limit = s.reach_mm + (j - 1) as f64 * cone;
                            far[k - j][c] <= limit + 1e-9
                        })
                })
                .collect()
        };
        keep[k * cols..(k + 1) * cols].copy_from_slice(&layer);
        let solid: Vec<bool> = (0..cols)
            .map(|c| layer[c] || inside_w[k * cols + c])
            .collect();
        far.push(
            dt_geom::distance(&solid, nx, ny)
                .into_iter()
                .map(|d| d * g)
                .collect(),
        );
    }
    let removed: Vec<bool> = (0..nz * cols).map(|v| inside_t[v] && !keep[v]).collect();
    let n_removed = removed.iter().filter(|&&r| r).count();
    if n_removed == 0 {
        return Ok((part.clone(), Stood::default()));
    }
    // pieces: 26-connected components of removed voxels, each cut by its
    // own small level set
    let comps = merge(components(&removed, nx, ny, nz), 3);
    if std::env::var_os("DT_STAND_DEBUG").is_some() {
        for (a, b) in &comps {
            eprintln!(
                "stand: {:?}..{:?} z {:.2}",
                a,
                b,
                centre(a[0], a[1], a[2])[2]
            );
        }
    }
    // the cutter field: + in removed voxels and in the air round them
    // (never in kept voxels or the support), − elsewhere
    let near = 3usize;
    let cutters: Vec<TriMesh> = comps
        .par_iter()
        .map(|(vlo, vhi)| {
            let a = [
                vlo[0].saturating_sub(near),
                vlo[1].saturating_sub(near),
                vlo[2].saturating_sub(near),
            ];
            let b = [
                (vhi[0] + near).min(nx - 1),
                (vhi[1] + near).min(ny - 1),
                (vhi[2] + near).min(nz - 1),
            ];
            let field = |i: usize, j: usize, k: usize| -> f64 {
                let v = k * cols + j * nx + i;
                if removed[v] {
                    return 1.0;
                }
                if keep[v] || inside_w[v] {
                    return -1.0;
                }
                // air: + only next to removed material
                let (i0, j0, k0) = (
                    i.saturating_sub(near),
                    j.saturating_sub(near),
                    k.saturating_sub(1),
                );
                for kk in k0..=(k + 1).min(nz - 1) {
                    for jj in j0..=(j + near).min(ny - 1) {
                        for ii in i0..=(i + near).min(nx - 1) {
                            if removed[kk * cols + jj * nx + ii] {
                                return 1.0;
                            }
                        }
                    }
                }
                -1.0
            };
            let mut vals: Vec<f64> =
                vec![0.0; (b[0] - a[0] + 1) * (b[1] - a[1] + 1) * (b[2] - a[2] + 1)];
            let (sx, sy) = (b[0] - a[0] + 1, b[1] - a[1] + 1);
            for k in a[2]..=b[2] {
                for j in a[1]..=b[1] {
                    for i in a[0]..=b[0] {
                        vals[((k - a[2]) * sy + (j - a[1])) * sx + (i - a[0])] = field(i, j, k);
                    }
                }
            }
            // soften the voxel terraces: [1 2 1] twice along each axis
            let sz = b[2] - a[2] + 1;
            let vals = soften(vals, [sx, sy, sz]);
            let c0 = centre(a[0], a[1], a[2]);
            let c1 = centre(b[0], b[1], b[2]);
            let sample = |p: [f64; 3]| -> f64 {
                // trilinear over voxel centres, −1 outside the block
                let fx = (p[0] - c0[0]) / g;
                let fy = (p[1] - c0[1]) / g;
                let fz = (p[2] - c0[2]) / lay;
                #[allow(clippy::cast_precision_loss)]
                let inb = fx >= 0.0
                    && fy >= 0.0
                    && fz >= 0.0
                    && fx <= (sx - 1) as f64
                    && fy <= (sy - 1) as f64
                    && fz <= (b[2] - a[2]) as f64;
                if !inb {
                    return -1.0;
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (i, j, k) = (
                    (fx.floor() as usize).min(sx.saturating_sub(2)),
                    (fy.floor() as usize).min(sy.saturating_sub(2)),
                    (fz.floor() as usize).min((b[2] - a[2]).saturating_sub(1)),
                );
                #[allow(clippy::cast_precision_loss)]
                let (u, v, wv) = (fx - i as f64, fy - j as f64, fz - k as f64);
                let at = |di: usize, dj: usize, dk: usize| {
                    let (ii, jj, kk) = ((i + di).min(sx - 1), (j + dj).min(sy - 1), k + dk);
                    vals.get((kk * sy + jj) * sx + ii).copied().unwrap_or(-1.0)
                };
                let l = |x: f64, y: f64, t: f64| x + (y - x) * t;
                let x00 = l(at(0, 0, 0), at(1, 0, 0), u);
                let x10 = l(at(0, 1, 0), at(1, 1, 0), u);
                let x01 = l(at(0, 0, 1), at(1, 0, 1), u);
                let x11 = l(at(0, 1, 1), at(1, 1, 1), u);
                l(l(x00, x10, v), l(x01, x11, v), wv)
            };
            let m = [g, g, lay];
            form::level_set(
                &sample,
                [c0[0] - m[0], c0[1] - m[1], c0[2] - m[2]],
                [c1[0] + m[0], c1[1] + m[1], c1[2] + m[2]],
                g.max(lay) * 0.75,
            )
        })
        .collect::<Result<_, _>>()?;
    let cutters: Vec<TriMesh> = cutters
        .into_iter()
        .filter(|m| !m.triangles.is_empty())
        .collect();
    let slots = scene.slots(
        u32::try_from(cutters.len()).map_err(|_| KitError::Slots)?,
        material,
    )?;
    let solids: Vec<Solid> = cutters
        .par_iter()
        .enumerate()
        .map(|(k, m)| form::solid(slots.get(u32::try_from(k).unwrap_or(u32::MAX)), m))
        .collect::<Result<_, _>>()?;
    #[allow(clippy::cast_precision_loss)]
    let stood = Stood {
        cut_mm3: n_removed as f64 * g * g * lay,
        pieces: comps.len(),
    };
    Ok((carve(part, &solids)?, stood))
}

fn bounds(m: &TriMesh) -> Option<([f64; 3], [f64; 3])> {
    if m.positions.is_empty() {
        return None;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &m.positions {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    Some((lo, hi))
}

/// Inside flags of every voxel (layer-major), by the parity of vertical
/// ray hits per column at each voxel centre.
fn voxels(
    m: &TriMesh,
    lo: [f64; 3],
    g: f64,
    lay: f64,
    nx: usize,
    ny: usize,
    nz: usize,
) -> Vec<bool> {
    let cols = nx * ny;
    let hits: Vec<Vec<(usize, f64)>> = m
        .triangles
        .par_iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| m.positions[i as usize]);
            let mut out = Vec::new();
            let x0 = a[0].min(b[0]).min(c[0]);
            let x1 = a[0].max(b[0]).max(c[0]);
            let y0 = a[1].min(b[1]).min(c[1]);
            let y1 = a[1].max(b[1]).max(c[1]);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let span = |v0: f64, v1: f64, o: f64, n: usize| {
                let s = ((v0 - o) / g - 0.5).ceil().max(0.0) as usize;
                let e = (((v1 - o) / g - 0.5).floor() as i64).min(n as i64 - 1);
                (s, e)
            };
            let (is, ie) = span(x0, x1, lo[0], nx);
            let (js, je) = span(y0, y1, lo[1], ny);
            #[allow(
                clippy::cast_possible_wrap,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            for j in js..=(je.max(-1) as usize).min(ny - 1) {
                if je < 0 {
                    break;
                }
                for i in is..=(ie.max(-1) as usize).min(nx - 1) {
                    if ie < 0 {
                        break;
                    }
                    let px = lo[0] + (i as f64 + 0.5) * g + 1.37e-5;
                    let py = lo[1] + (j as f64 + 0.5) * g + 0.91e-5;
                    if let Some(z) = vertical_hit(a, b, c, px, py) {
                        out.push((j * nx + i, z));
                    }
                }
            }
            out
        })
        .collect();
    let mut per: Vec<Vec<f64>> = vec![Vec::new(); cols];
    for list in &hits {
        for &(c, z) in list {
            per[c].push(z);
        }
    }
    let mut inside = vec![false; nz * cols];
    for (c, zs) in per.iter_mut().enumerate() {
        if zs.is_empty() {
            continue;
        }
        zs.sort_by(f64::total_cmp);
        let mut h = 0;
        for k in 0..nz {
            #[allow(clippy::cast_precision_loss)]
            let z = lo[2] + (k as f64 + 0.5) * lay;
            while h < zs.len() && zs[h] < z {
                h += 1;
            }
            inside[k * cols + c] = h % 2 == 1;
        }
    }
    inside
}

/// Where the vertical line through `(x, y)` meets triangle `abc`.
fn vertical_hit(a: [f64; 3], b: [f64; 3], c: [f64; 3], x: f64, y: f64) -> Option<f64> {
    let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if d.abs() < 1e-18 {
        return None;
    }
    let l1 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
    let l2 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
    let l3 = 1.0 - l1 - l2;
    (l1 >= 0.0 && l2 >= 0.0 && l3 >= 0.0).then(|| l1 * a[2] + l2 * b[2] + l3 * c[2])
}

/// 26-connected components of set voxels, as voxel-index bounding boxes,
/// in order of their first voxel.
fn components(set: &[bool], nx: usize, ny: usize, nz: usize) -> Vec<([usize; 3], [usize; 3])> {
    let cols = nx * ny;
    let mut seen = vec![false; set.len()];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for v0 in 0..set.len() {
        if !set[v0] || seen[v0] {
            continue;
        }
        seen[v0] = true;
        stack.push(v0);
        let mut lo = [usize::MAX; 3];
        let mut hi = [0; 3];
        while let Some(v) = stack.pop() {
            let (k, j, i) = (v / cols, (v % cols) / nx, v % nx);
            for (a, x) in [i, j, k].into_iter().enumerate() {
                lo[a] = lo[a].min(x);
                hi[a] = hi[a].max(x);
            }
            // 26-connected: a bevel's staircase is one piece
            for dk in -1i64..=1 {
                for dj in -1i64..=1 {
                    for di in -1i64..=1 {
                        #[allow(clippy::cast_possible_wrap)]
                        let (a, b, c) = (i as i64 + di, j as i64 + dj, k as i64 + dk);
                        #[allow(clippy::cast_possible_wrap)]
                        let ok = a >= 0
                            && b >= 0
                            && c >= 0
                            && a < nx as i64
                            && b < ny as i64
                            && c < nz as i64;
                        if !ok {
                            continue;
                        }
                        #[allow(clippy::cast_sign_loss)]
                        let u = c as usize * cols + b as usize * nx + a as usize;
                        if set[u] && !seen[u] {
                            seen[u] = true;
                            stack.push(u);
                        }
                    }
                }
            }
        }
        out.push((lo, hi));
    }
    out
}

/// Boxes that come within `pad` voxels of each other, merged (in order).
fn merge(mut boxes: Vec<([usize; 3], [usize; 3])>, pad: usize) -> Vec<([usize; 3], [usize; 3])> {
    loop {
        let mut merged = false;
        let mut out: Vec<([usize; 3], [usize; 3])> = Vec::new();
        for b in boxes {
            if let Some(o) = out
                .iter_mut()
                .find(|o| (0..3).all(|a| b.0[a] <= o.1[a] + 2 * pad && o.0[a] <= b.1[a] + 2 * pad))
            {
                for a in 0..3 {
                    o.0[a] = o.0[a].min(b.0[a]);
                    o.1[a] = o.1[a].max(b.1[a]);
                }
                merged = true;
            } else {
                out.push(b);
            }
        }
        boxes = out;
        if !merged {
            return boxes;
        }
    }
}

/// Two passes of a [1 2 1] / 4 filter along each axis of a block
/// (`dims` = x, y, z sizes, x fastest); edges clamp.
fn soften(mut v: Vec<f64>, dims: [usize; 3]) -> Vec<f64> {
    let stride = [1, dims[0], dims[0] * dims[1]];
    for axis in 0..3 {
        for _ in 0..2 {
            let src = v.clone();
            for (idx, out) in v.iter_mut().enumerate() {
                let pos = (idx / stride[axis]) % dims[axis];
                let lo = if pos > 0 { idx - stride[axis] } else { idx };
                let hi = if pos + 1 < dims[axis] {
                    idx + stride[axis]
                } else {
                    idx
                };
                *out = 0.25 * src[lo] + 0.5 * src[idx] + 0.25 * src[hi];
            }
        }
    }
    v
}
