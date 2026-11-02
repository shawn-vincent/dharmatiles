//! The audit gate: the mesh as a slicer sees it.
//!
//! Ports `stone/solidify.py` (pinch separation, the float32 reload),
//! dirtlab's `stl_safe` (keep the largest body) and clifflab rockpile's
//! layer-reach overhang rule (`overhang_faces`, the 55° rule that wins over
//! the other three `audit_overhang` copies). Order, as `stl_safe`: separate
//! pinches, quantise to float32 and merge by position, split into bodies
//! and keep the largest (reporting the rest), then check watertightness,
//! overhang and see-through holes on what is written.

use std::collections::HashMap;

use dt_geom::TriMesh;
use rayon::prelude::*;

use crate::form::vertex_normals;
use crate::graph::{cross, dist, sub};
use crate::shape::signed_volume;

/// The printer profile the gate checks against: the numbers of a
/// `printer` in library/printers/*.kdl (dt-eval decodes it with dt-doc
/// and calls [`Printer::new`]), plus the gate's own tuning, which no
/// profile sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Printer {
    /// No layer may reach past this cone from vertical (the profile's
    /// `overhang`; rockpile `OVERHANG_DEG`).
    pub overhang_deg: f64,
    /// Layer height, mm (the profile's `layer`).
    pub layer_mm: f64,
    /// One extruded line, mm (the profile's `line`).
    pub line_mm: f64,
    /// The narrowest gap that stays open, mm (the profile's `gap`).
    pub gap_mm: f64,
    /// Gate tuning: one layer may reach this far out over the one below,
    /// mm (rockpile's 0.6).
    pub reach_mm: f64,
    /// Gate tuning: how many mm of layers below are compared.
    pub span_mm: f64,
    /// Gate tuning: vertical-ray spacing for the slice, mm.
    pub grid_mm: f64,
    /// Gate tuning: overhang the gate accepts, mm².
    pub overhang_budget_mm2: f64,
}

impl Printer {
    /// A printer profile's four numbers, with the gate's fixed tuning.
    #[must_use]
    pub fn new(line_mm: f64, layer_mm: f64, overhang_deg: f64, gap_mm: f64) -> Self {
        Self {
            overhang_deg,
            layer_mm,
            line_mm,
            gap_mm,
            reach_mm: 0.6,
            span_mm: 3.0,
            grid_mm: 0.1,
            overhang_budget_mm2: 0.0,
        }
    }
}

/// A body keep-largest dropped.
#[derive(Clone, Debug, PartialEq)]
pub struct Dropped {
    /// Its triangles.
    pub triangles: usize,
    /// Its volume, mm³.
    pub volume_mm3: f64,
}

/// What the gate found.
#[derive(Clone, Debug, PartialEq)]
pub struct Audit {
    /// Triangles written.
    pub triangles: usize,
    /// Vertices written.
    pub vertices: usize,
    /// Volume written, mm³.
    pub volume_mm3: f64,
    /// Bodies before keep-largest.
    pub bodies: usize,
    /// Loose bodies keep-largest dropped, largest first.
    pub dropped: Vec<Dropped>,
    /// Sealed voids keep-largest left out (inward-wound shells, negative
    /// volume), so they print solid: a fill, not a lost part.
    pub voids: Vec<Dropped>,
    /// Vertices pinch separation moved.
    pub pinches_moved: usize,
    /// Triangles that collapsed in the float32 merge (dropped).
    pub collapsed: usize,
    /// Closed and consistently oriented after the float32 reload.
    pub watertight: bool,
    /// Area of the faces past the overhang rule, mm².
    pub overhang_mm2: f64,
    /// Per written triangle: past the overhang rule (painted red).
    pub overhang: Vec<bool>,
    /// Vertical rays inside the footprint that miss the mesh.
    pub holes: usize,
}

impl Audit {
    /// Why the gate fails, empty if it passes.
    #[must_use]
    pub fn failures(&self, printer: &Printer) -> Vec<String> {
        let mut f = Vec::new();
        if !self.watertight {
            f.push("not watertight after the float32 reload".to_owned());
        }
        if self.overhang_mm2 > printer.overhang_budget_mm2 {
            f.push(format!(
                "{:.2} mm² of overhang past {}° (budget {:.2} mm²)",
                self.overhang_mm2, printer.overhang_deg, printer.overhang_budget_mm2
            ));
        }
        if self.holes > 0 {
            f.push(format!("{} see-through holes", self.holes));
        }
        if self.triangles == 0 {
            f.push("nothing to print".to_owned());
        }
        f
    }
}

/// The mesh the gate passes on: float32-exact positions, plain indices,
/// and for each triangle the index of the input triangle it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Export {
    /// The mesh as written.
    pub mesh: TriMesh,
    /// Input triangle of each written triangle.
    pub source: Vec<u32>,
}

/// Run the gate on a kernel export. `footprint` is the tile's width and
/// height in mm (for see-through holes).
#[must_use]
pub fn audit(input: &TriMesh, footprint: [f64; 2], printer: &Printer) -> (Export, Audit) {
    let mut m = canonical(input);
    let pinches_moved = separate_pinches(&mut m);
    let (m, source, collapsed) = quantise(&m);
    let (m, source, bodies, left_out) = keep_largest(&m, &source);
    let (voids, dropped) = left_out.into_iter().partition(|d| d.volume_mm3 < 0.0);
    let watertight = m.is_watertight();
    let (overhang, holes) = overhang_faces(&m, footprint, printer);
    let areas = face_areas(&m);
    // (an empty f64 sum is -0.0)
    let overhang_mm2 = 0.0
        + overhang
            .iter()
            .zip(&areas)
            .filter(|(o, _)| **o)
            .map(|(_, a)| a)
            .sum::<f64>();
    let a = Audit {
        triangles: m.triangles.len(),
        vertices: m.positions.len(),
        volume_mm3: signed_volume(&m),
        bodies,
        dropped,
        voids,
        pinches_moved,
        collapsed,
        watertight,
        overhang_mm2,
        overhang,
        holes,
    };
    (Export { mesh: m, source }, a)
}

/// Positions with split vertices merged and unused ones gone; only
/// `positions` and `triangles` are kept.
fn canonical(m: &TriMesh) -> TriMesh {
    let mut to: Vec<u32> = (0..m.positions.len())
        .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
        .collect();
    for (&f, &t) in m.merge_from.iter().zip(&m.merge_to) {
        if let Some(s) = to.get_mut(f as usize) {
            *s = t;
        }
    }
    let mut new_index = vec![u32::MAX; m.positions.len()];
    let mut positions = Vec::new();
    let triangles = m
        .triangles
        .iter()
        .map(|t| {
            t.map(|v| {
                let c = to[v as usize] as usize;
                if new_index[c] == u32::MAX {
                    new_index[c] = u32::try_from(positions.len()).unwrap_or(u32::MAX);
                    positions.push(m.positions[c]);
                }
                new_index[c]
            })
        })
        .collect();
    TriMesh {
        positions,
        triangles,
        ..TriMesh::default()
    }
}

/// Port of `separate_pinches`: vertex pairs closer than 0.2 µm that are
/// not joined by an edge (or are joined by a sliver edge under 0.02 µm)
/// are nudged inward along their own normals by 0.15 and 0.30 µm, so the
/// float32 merge on reload cannot fuse them. Returns how many moved.
fn separate_pinches(m: &mut TriMesh) -> usize {
    const NEAR: f64 = 2e-4;
    const CELL: f64 = 2.0 * NEAR;
    let mut edges: Option<std::collections::HashSet<(u32, u32), Fx>> = None;
    let mut normals: Option<Vec<[f64; 3]>> = None;
    let mut moved = 0;
    for _ in 0..4 {
        let mut cells: HashMap<[i64; 3], Vec<u32>, Fx> =
            HashMap::with_capacity_and_hasher(m.positions.len(), Fx::default());
        #[allow(clippy::cast_possible_truncation)]
        let key = |p: [f64; 3]| p.map(|x| (x / CELL).floor() as i64);
        for (i, p) in m.positions.iter().enumerate() {
            cells
                .entry(key(*p))
                .or_default()
                .push(u32::try_from(i).unwrap_or(u32::MAX));
        }
        // pairs closer than NEAR (rare: found by cell, edges checked after)
        let mut near = Vec::new();
        for (i, p) in m.positions.iter().enumerate() {
            let k = key(*p);
            // cells are 2 NEAR wide, so a point within NEAR lies in this
            // cell or the one on the side of the cell this point is nearer
            #[allow(clippy::cast_precision_loss)]
            let side = [0, 1, 2].map(|a| {
                if p[a] / CELL - k[a] as f64 >= 0.5 {
                    1
                } else {
                    -1
                }
            });
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            for c in 0..8 {
                let cell = [0, 1, 2].map(|a| k[a] + if c >> a & 1 == 1 { side[a] } else { 0 });
                let Some(list) = cells.get(&cell) else {
                    continue;
                };
                for &j in list {
                    if j > i {
                        let d = dist(*p, m.positions[j as usize]);
                        if d < NEAR {
                            near.push((i, j, d));
                        }
                    }
                }
            }
        }
        if near.is_empty() {
            break;
        }
        let edges = edges.get_or_insert_with(|| {
            let mut e = std::collections::HashSet::with_capacity_and_hasher(
                3 * m.triangles.len(),
                Fx::default(),
            );
            for t in &m.triangles {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    e.insert((a.min(b), a.max(b)));
                }
            }
            e
        });
        let mut pairs: Vec<(usize, usize)> = near
            .into_iter()
            .filter(|&(i, j, d)| !edges.contains(&(i, j)) || d <= 2e-5)
            .map(|(i, j, _)| (i as usize, j as usize))
            .collect();
        if pairs.is_empty() {
            break;
        }
        let normals = normals.get_or_insert_with(|| vertex_normals(&m.positions, &m.triangles));
        pairs.sort_unstable();
        for (i, j) in pairs {
            let (ni, nj) = (normals[i], normals[j]);
            for (k, (a, b)) in ni.iter().zip(nj).enumerate() {
                m.positions[i][k] -= a * 1.5e-4;
                m.positions[j][k] -= b * 3.0e-4;
            }
            moved += 2;
        }
    }
    moved
}

/// A fast, fixed (unseeded) hasher for the audit's integer keys: no
/// result depends on iteration order, so any hasher is deterministic here.
#[derive(Clone, Copy, Default)]
struct FxHasher(u64);

impl std::hash::Hasher for FxHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, rest) = bytes.as_chunks::<8>();
        for c in chunks {
            self.write_u64(u64::from_le_bytes(*c));
        }
        for &b in rest {
            self.write_u64(u64::from(b));
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    fn write_u32(&mut self, v: u32) {
        self.write_u64(u64::from(v));
    }
    #[allow(clippy::cast_sign_loss)]
    fn write_i64(&mut self, v: i64) {
        self.write_u64(v as u64);
    }
}

type Fx = std::hash::BuildHasherDefault<FxHasher>;

/// Positions rounded to float32 and merged by position (as an STL reader
/// does); triangles that collapse are dropped and counted.
fn quantise(m: &TriMesh) -> (TriMesh, Vec<u32>, usize) {
    #[allow(clippy::cast_possible_truncation)]
    let q = |p: [f64; 3]| p.map(|x| x as f32);
    let mut index: HashMap<[u32; 3], u32> = HashMap::with_capacity(m.positions.len());
    let mut positions = Vec::with_capacity(m.positions.len());
    let remap: Vec<u32> = m
        .positions
        .iter()
        .map(|p| {
            let f = q(*p);
            *index.entry(f.map(f32::to_bits)).or_insert_with(|| {
                positions.push(f.map(f64::from));
                u32::try_from(positions.len() - 1).unwrap_or(u32::MAX)
            })
        })
        .collect();
    let mut triangles = Vec::with_capacity(m.triangles.len());
    let mut source = Vec::with_capacity(m.triangles.len());
    for (k, t) in m.triangles.iter().enumerate() {
        let v = t.map(|i| remap[i as usize]);
        if v[0] != v[1] && v[1] != v[2] && v[0] != v[2] {
            triangles.push(v);
            source.push(u32::try_from(k).unwrap_or(u32::MAX));
        }
    }
    let collapsed = m.triangles.len() - triangles.len();
    (
        TriMesh {
            positions,
            triangles,
            ..TriMesh::default()
        },
        source,
        collapsed,
    )
}

/// Split into bodies (triangles sharing vertices) and keep the one with the
/// most triangles, as `stl_safe` does; report the rest.
fn keep_largest(m: &TriMesh, source: &[u32]) -> (TriMesh, Vec<u32>, usize, Vec<Dropped>) {
    let mut parent: Vec<usize> = (0..m.positions.len()).collect();
    fn root(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for t in &m.triangles {
        let a = root(&mut parent, t[0] as usize);
        for &v in &t[1..] {
            let b = root(&mut parent, v as usize);
            if a != b {
                parent[b.max(a)] = a.min(b);
            }
        }
    }
    let mut body_of: HashMap<usize, Vec<usize>> = HashMap::new();
    for (k, t) in m.triangles.iter().enumerate() {
        body_of
            .entry(root(&mut parent, t[0] as usize))
            .or_default()
            .push(k);
    }
    let mut bodies: Vec<Vec<usize>> = body_of.into_values().collect();
    // most triangles first; ties by first triangle, so the order is fixed
    bodies.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));
    let n = bodies.len();
    let sub_mesh = |tris: &[usize]| TriMesh {
        positions: m.positions.clone(),
        triangles: tris.iter().map(|&k| m.triangles[k]).collect(),
        ..TriMesh::default()
    };
    let dropped = bodies
        .iter()
        .skip(1)
        .map(|b| Dropped {
            triangles: b.len(),
            volume_mm3: signed_volume(&sub_mesh(b)),
        })
        .collect();
    let Some(keep) = bodies.first() else {
        return (m.clone(), source.to_vec(), 0, Vec::new());
    };
    if n == 1 {
        return (m.clone(), source.to_vec(), 1, dropped);
    }
    let mut keep = keep.clone();
    keep.sort_unstable();
    let kept = canonical(&sub_mesh(&keep));
    let src = keep.iter().map(|&k| source[k]).collect();
    (kept, src, n, dropped)
}

fn face_areas(m: &TriMesh) -> Vec<f64> {
    m.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| m.positions[i as usize]);
            let f = cross(sub(b, a), sub(c, a));
            0.5 * (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt()
        })
        .collect()
}

/// Port of rockpile's `overhang_faces`: the mesh sliced every layer by
/// vertical rays on a `grid_mm` lattice (offset off the mesh's own grids,
/// so no ray grazes an edge); a layer's material farther than `reach +
/// (k − 1) × layer × tan(overhang)` from layer k below it (any k within
/// `span_mm`) droops. Flagged: the downward faces above z = 0.05 under such
/// material. Also returns the rays inside `footprint` that hit nothing.
///
/// A void thinner than the printer's `gap` (measured exactly along each
/// ray, not at layer resolution) is filled before the layers are cut: such
/// a gap cannot stay open in print, the layer above lands on the one
/// below, so its roof is not an overhang. This is what a bed joint of
/// 0.11 mm under a wall stone is (walls R10: "the lip over the sub-mm
/// joint prints"); rockpile's rule, written for stacked rock with real
/// gaps, counted every such roof as a drooping shelf.
fn overhang_faces(m: &TriMesh, footprint: [f64; 2], p: &Printer) -> (Vec<bool>, usize) {
    let none = (vec![false; m.triangles.len()], 0);
    let Some((lo, hi)) = bounds(m) else {
        return none;
    };
    let g = p.grid_mm;
    let (x0, y0) = (lo[0] + 0.0137, lo[1] + 0.0091);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = |a: f64, b: f64| ((b - a) / g).ceil().max(0.0) as usize;
    let (nx, ny) = (count(x0, hi[0]), count(y0, hi[1]));
    if nx == 0 || ny == 0 {
        return none;
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
    for (col, zs) in per_col.iter_mut().enumerate() {
        if zs.is_empty() {
            continue;
        }
        hit_any[col] = true;
        zs.sort_by(f64::total_cmp);
        // hits alternate in, out, in, …: drop an out and the next in when
        // the void between them is thinner than the printer's gap
        let mut kept: Vec<f64> = Vec::with_capacity(zs.len());
        for (k, &z) in zs.iter().enumerate() {
            if k.is_multiple_of(2)
                && k > 0
                && kept.len().is_multiple_of(2)
                && z - zs[k - 1] < p.gap_mm
            {
                kept.pop();
                continue;
            }
            kept.push(z);
        }
        for z in kept {
            occ[layer_of(z) * nx * ny + col] ^= 1;
        }
    }
    // inside at layer k: an odd count of hits at layers ≤ k
    let mut inside = vec![false; nl * nx * ny];
    let mut parity = vec![0u8; nx * ny];
    for k in 0..nl {
        for col in 0..nx * ny {
            parity[col] ^= occ[k * nx * ny + col];
            inside[k * nx * ny + col] = parity[col] == 1;
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
    let c = lay * libm::tan(p.overhang_deg.to_radians());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let span = (p.span_mm / lay) as usize;
    let mut bad = vec![false; nl * nx * ny];
    for i in 1..nl {
        let cur = layer(i);
        for col in 0..nx * ny {
            if !cur[col] {
                continue;
            }
            bad[i * nx * ny + col] = (1..=span.min(i)).any(|j| {
                #[allow(clippy::cast_possible_truncation)]
                let limit = (p.reach_mm + (j - 1) as f64 * c) as f32;
                far[i - j][col] > limit
            });
        }
    }
    // 3 × 3 dilation in each layer, then a face may sit under layer k or k + 1
    let mut grown = vec![false; nl * nx * ny];
    for k in 0..nl {
        for jy in 0..ny {
            for ix in 0..nx {
                if !bad[k * nx * ny + jy * nx + ix] {
                    continue;
                }
                for dy in jy.saturating_sub(1)..=(jy + 1).min(ny - 1) {
                    for dx in ix.saturating_sub(1)..=(ix + 1).min(nx - 1) {
                        grown[k * nx * ny + dy * nx + dx] = true;
                    }
                }
            }
        }
    }
    for k in 0..nl - 1 {
        for col in 0..nx * ny {
            if grown[(k + 1) * nx * ny + col] {
                grown[k * nx * ny + col] = true;
            }
        }
    }
    let flagged = m
        .triangles
        .iter()
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
            grown[kz * nx * ny + iy * nx + ix]
        })
        .collect();
    let holes = (0..nx * ny)
        .filter(|&col| {
            let (x, y) = (x0 + (col % nx) as f64 * g, y0 + (col / nx) as f64 * g);
            !hit_any[col] && x < footprint[0] && y < footprint[1] && x > 0.0 && y > 0.0
        })
        .count();
    (flagged, holes)
}

pub(crate) fn bounds(m: &TriMesh) -> Option<([f64; 3], [f64; 3])> {
    let first = *m.positions.first()?;
    Some(m.positions.iter().fold((first, first), |(lo, hi), p| {
        (
            [0, 1, 2].map(|k| lo[k].min(p[k])),
            [0, 1, 2].map(|k| hi[k].max(p[k])),
        )
    }))
}

/// Vertical-ray hits of one triangle: `(column, z)` for every lattice
/// column strictly inside its projection, or on an edge by the top-left
/// rule, so a ray through a shared edge counts once.
fn column_hits(
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use dt_geom::{BOX_TRIANGLES, Grid, lift};

    #[test]
    fn pinches_are_found_across_cell_borders() {
        // two cubes touching at a corner, the corners 0.1 µm apart, placed
        // so the pair straddles a hash cell border in every axis
        for off in [0.0, 1.5e-4, 3.9e-4] {
            let a = cube([0.0, 0.0, 0.0], [1.0 + off, 1.0 + off, 1.0 + off]);
            let b = cube([1.0 + off + 1e-4; 3], [2.0 + off, 2.0 + off, 2.0 + off]);
            let mut m = a.clone();
            let n = u32::try_from(a.positions.len()).unwrap();
            m.positions.extend(b.positions);
            m.triangles
                .extend(b.triangles.iter().map(|t| t.map(|v| v + n)));
            assert_eq!(separate_pinches(&mut m), 2, "offset {off}");
        }
    }

    fn cube(lo: [f64; 3], hi: [f64; 3]) -> TriMesh {
        TriMesh {
            positions: (0..8)
                .map(|i| {
                    [
                        if i & 1 == 0 { lo[0] } else { hi[0] },
                        if i & 2 == 0 { lo[1] } else { hi[1] },
                        if i & 4 == 0 { lo[2] } else { hi[2] },
                    ]
                })
                .collect(),
            triangles: BOX_TRIANGLES.to_vec(),
            ..TriMesh::default()
        }
    }

    /// library/printers/default.kdl's numbers.
    fn fdm() -> Printer {
        Printer::new(0.45, 0.2, 55.0, 0.2)
    }

    #[test]
    fn a_flat_slab_passes() {
        let m = lift(&Grid::new(21, 21, 0.5, 2.0)).unwrap();
        let (e, a) = audit(&m, [10.0, 10.0], &fdm());
        assert!(a.watertight);
        assert_eq!((a.bodies, a.holes), (1, 0));
        assert!(a.overhang_mm2 == 0.0, "{}", a.overhang_mm2);
        assert_eq!(e.mesh.triangles.len(), m.triangles.len());
        assert!(a.failures(&fdm()).is_empty());
    }

    #[test]
    fn a_sealed_void_is_filled_not_dropped() {
        // a 6 mm block with a 1 mm cavity inside: an inward-wound shell
        let mut m = cube([0.0, 0.0, 0.0], [6.0, 6.0, 6.0]);
        let cavity = cube([2.0, 2.0, 2.0], [3.0, 3.0, 3.0]);
        let base = u32::try_from(m.positions.len()).unwrap();
        m.positions.extend(cavity.positions);
        m.triangles.extend(
            cavity
                .triangles
                .iter()
                .map(|t| [t[0] + base, t[2] + base, t[1] + base]),
        );
        let (_, a) = audit(&m, [10.0, 10.0], &fdm());
        assert_eq!(a.bodies, 2);
        assert!(a.dropped.is_empty(), "{:?}", a.dropped);
        assert_eq!(a.voids.len(), 1);
        assert!((a.voids[0].volume_mm3 + 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_floating_shelf_is_red_and_a_stray_body_is_dropped() {
        // a post with a 4 mm shelf sticking out at the top, plus a far crumb
        let mut m = cube([0.0, 0.0, 0.0], [2.0, 2.0, 6.0]);
        let shelf = cube([2.0, 0.0, 5.0], [6.0, 2.0, 6.0]);
        let crumb = cube([9.0, 9.0, 0.0], [9.5, 9.5, 0.5]);
        for extra in [shelf, crumb] {
            let base = u32::try_from(m.positions.len()).unwrap();
            m.positions.extend(extra.positions);
            m.triangles
                .extend(extra.triangles.iter().map(|t| t.map(|i| i + base)));
        }
        let (_, a) = audit(&m, [10.0, 10.0], &fdm());
        // post and shelf touch only face to face, so they are two bodies here
        assert_eq!(a.bodies, 3);
        assert_eq!(a.dropped.len(), 2);
        let (_, b) = audit(&shelf_on_post(), [10.0, 10.0], &fdm());
        // rockpile.audit_overhang gives 8.0 mm² on this shelf (4 × 2 mm underside)
        assert!(
            (b.overhang_mm2 - 8.0).abs() < 1e-9,
            "shelf underside: {}",
            b.overhang_mm2
        );
    }

    /// One closed L-shaped body: a post with a shelf at the top.
    fn shelf_on_post() -> TriMesh {
        let prof = [
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 5.0],
            [6.0, 5.0],
            [6.0, 6.0],
            [0.0, 6.0],
        ];
        prism(&prof, &[[0, 1, 2], [0, 2, 5], [2, 3, 4], [2, 4, 5]])
    }

    /// A profile in x–z, extruded 2 mm in y; `fan` triangulates it.
    fn prism(prof: &[[f64; 2]], fan: &[[u32; 3]]) -> TriMesh {
        let n = u32::try_from(prof.len()).unwrap();
        let mut positions = Vec::new();
        for y in [0.0, 2.0] {
            positions.extend(prof.iter().map(|p| [p[0], y, p[1]]));
        }
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        for f in fan {
            triangles.push([f[0], f[2], f[1]]);
            triangles.push([f[0] + n, f[1] + n, f[2] + n]);
        }
        for k in 0..n {
            let k1 = (k + 1) % n;
            triangles.push([k, k1, k1 + n]);
            triangles.push([k, k1 + n, k + n]);
        }
        let mut m = TriMesh {
            positions,
            triangles,
            ..TriMesh::default()
        };
        if signed_volume(&m) < 0.0 {
            m.triangles.iter_mut().for_each(|t| t.swap(1, 2));
        }
        assert!(m.is_watertight());
        m
    }

    /// A block on a slab, held by a post at one end, with a bed joint of
    /// `gap` mm between them: a C in profile.
    fn block_on_joint(gap: f64) -> TriMesh {
        let g = 2.0 + gap;
        let prof = [
            [0.0, 0.0],
            [6.0, 0.0],
            [6.0, 2.0],
            [1.0, 2.0],
            [1.0, g],
            [6.0, g],
            [6.0, 4.0],
            [0.0, 4.0],
        ];
        prism(
            &prof,
            &[
                [0, 1, 2],
                [0, 2, 3],
                [0, 3, 4],
                [0, 4, 7],
                [4, 5, 6],
                [4, 6, 7],
            ],
        )
    }

    #[test]
    fn a_bed_joint_thinner_than_the_gap_is_no_overhang() {
        // 0.11 mm: the wall's bed joint; the layer above lands on the stone below
        let (_, a) = audit(&block_on_joint(0.11), [10.0, 10.0], &fdm());
        assert!(a.overhang_mm2 == 0.0, "{}", a.overhang_mm2);
        // rockpile's rule as it was (every void open) flags its roof
        let open = Printer {
            gap_mm: 0.0,
            ..fdm()
        };
        let (_, was) = audit(&block_on_joint(0.11), [10.0, 10.0], &open);
        assert!(was.overhang_mm2 > 1.0, "{}", was.overhang_mm2);
        // 1 mm: a real slot, whose roof is a 5 mm shelf
        let (_, b) = audit(&block_on_joint(1.0), [10.0, 10.0], &fdm());
        assert!(b.overhang_mm2 > 5.0, "{}", b.overhang_mm2);
    }
}
