//! The audit gate: the mesh as a slicer sees it.
//!
//! Pinch separation, the float32 reload, keeping the largest body, and the
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
use crate::scene::Material;
use crate::shape::signed_volume;

/// The tolerance band: faces past the printer's overhang (55°, the
/// conservative target everything aims for by construction) but within
/// `max_deg` are reported as band, not overhang, up to `share` of the
/// downward-facing area. Past `max_deg` they are overhang and fail. Shawn,
/// 3 Nov 2026 for trees, generalised 4 Nov 2026 to every element: 55–65° in
/// small patches, 65° the hard cap (a 0.2 mm layer steps 0.2·tan 65° =
/// 0.43 mm, under a 0.45 mm line). The share is the trees' measured one.
/// Past the cap, small patches are allowed too (Shawn, 4 Nov 2026: "allow
/// small patches over 65°, like the band"): up to `over_share` of the same
/// downward area; more than that fails.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    /// The materials the band covers; empty: every material.
    pub materials: &'static [Material],
    /// The hard cap, degrees from vertical.
    pub max_deg: f64,
    /// Band area allowed, as a share of those materials' downward-facing
    /// area.
    pub share: f64,
    /// Area past `max_deg` allowed, as a share of the same downward area.
    pub over_share: f64,
}

impl Band {
    /// The band every element gets: 55–65° up to 0.1% of the downward area,
    /// and over 65° up to another 0.1%.
    pub const DEFAULT: Self = Self {
        materials: &[],
        max_deg: 65.0,
        share: 0.001,
        over_share: 0.001,
    };
}

/// The printer profile the gate checks against: the numbers of a
/// `printer` in library/printers/*.dtile (dt-eval decodes it with dt-doc
/// and calls [`Printer::new`]), plus the gate's own tuning, which no
/// profile sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Printer {
    /// No layer may reach past this cone from vertical (the profile's
    /// `overhang`).
    pub overhang_deg: f64,
    /// Past this, degrees from vertical, overhang is red; between
    /// `overhang_deg` and this it is the band (the profile's `steepest`).
    pub steepest_deg: f64,
    /// Layer height, mm (the profile's `layer`).
    pub layer_mm: f64,
    /// One extruded line, mm (the profile's `line`).
    pub line_mm: f64,
    /// The narrowest gap that stays open, mm (the profile's `gap`).
    pub gap_mm: f64,
    /// Gate tuning: one layer may reach this far out over the one below,
    /// mm (0.6).
    pub reach_mm: f64,
    /// Gate tuning: how many mm of layers below are compared.
    pub span_mm: f64,
    /// Gate tuning: vertical-ray spacing for the slice, mm.
    pub grid_mm: f64,
    /// Gate tuning: overhang the gate accepts, mm².
    pub overhang_budget_mm2: f64,
}

impl Printer {
    /// A printer profile's numbers, with the gate's fixed tuning;
    /// `steepest_deg` is 65 until [`Self::steepest`] sets it.
    #[must_use]
    pub fn new(line_mm: f64, layer_mm: f64, overhang_deg: f64, gap_mm: f64) -> Self {
        Self {
            overhang_deg,
            steepest_deg: 65.0,
            layer_mm,
            line_mm,
            gap_mm,
            reach_mm: 0.6,
            span_mm: 3.0,
            grid_mm: 0.1,
            overhang_budget_mm2: 0.0,
        }
    }

    /// The same printer with the profile's `steepest`.
    #[must_use]
    pub const fn steepest(self, steepest_deg: f64) -> Self {
        Self {
            steepest_deg,
            ..self
        }
    }
}

/// A left-out body smaller than this, mm³, is a boolean's sliver, not a
/// part (a printed bead, 0.45 × 0.45 × 0.2 mm, is 0.04; a slicer prints
/// nothing of it): an inward-wound one counts as a void wherever it lies,
/// an outward one is reported in `slivers`, not as a lost part.
const VOID_SLIVER_MM3: f64 = 1e-3;

/// A body keep-largest dropped.
#[derive(Clone, Debug, PartialEq)]
pub struct Dropped {
    /// Its triangles.
    pub triangles: usize,
    /// Its volume, mm³.
    pub volume_mm3: f64,
    /// Its bounding box, mm (low corner, high corner): where to element.
    pub bounds: [[f64; 3]; 2],
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
    /// Loose boolean slivers keep-largest left out: outward bodies under
    /// a thousandth of a mm³, far below one printed bead. Not a lost part.
    pub slivers: Vec<Dropped>,
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
    /// The overhang area allowed, mm²: the printer's budget, or the band's
    /// share past its cap, whichever is larger.
    pub overhang_budget_mm2: f64,
    /// Area of the faces in an element's tolerance band, mm² (past the
    /// printer's overhang, within the band's cap; not in `overhang_mm2`).
    pub band_mm2: f64,
    /// The band area allowed, mm² (0 without a band).
    pub band_budget_mm2: f64,
    /// Per written triangle: in the band (painted orange).
    pub band: Vec<bool>,
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
        if self.overhang_mm2 > self.overhang_budget_mm2.max(printer.overhang_budget_mm2) {
            f.push(format!(
                "{:.2} mm² of overhang (over {}°; budget {:.2} mm²)",
                self.overhang_mm2,
                Band::DEFAULT.max_deg,
                self.overhang_budget_mm2.max(printer.overhang_budget_mm2)
            ));
        }
        if self.band_mm2 > self.band_budget_mm2 {
            f.push(format!(
                "{:.2} mm² in the band ({}–{}°; budget {:.2} mm²)",
                self.band_mm2,
                printer.overhang_deg,
                Band::DEFAULT.max_deg,
                self.band_budget_mm2
            ));
        }
        if self.holes > 0 {
            f.push(format!("{} see-through holes", self.holes));
        }
        if self.triangles == 0 {
            f.push("nothing to print".to_owned());
        }
        // A dropped body is a part of the tile the print would lose (a
        // floating brick, a crumb): no library tile drops any, so there is
        // no volume floor.
        if let Some(big) = self.dropped.first() {
            let total: f64 = self.dropped.iter().map(|d| d.volume_mm3.abs()).sum();
            f.push(format!(
                "{} loose bodies dropped ({total:.2} mm³ in all, largest {} triangles)",
                self.dropped.len(),
                big.triangles
            ));
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
    audit_banded(input, footprint, printer, &[], Some(&Band::DEFAULT))
}

/// [`audit`] with an element's tolerance [`Band`]; `materials` holds each
/// input triangle's material.
#[must_use]
pub fn audit_banded(
    input: &TriMesh,
    footprint: [f64; 2],
    printer: &Printer,
    materials: &[Material],
    band: Option<&Band>,
) -> (Export, Audit) {
    let mut m = canonical(input);
    let pinches_moved = separate_pinches(&mut m);
    let (m, source, collapsed) = quantise(&m);
    let (m, source, bodies, left_out) = keep_largest(&m, &source);
    // a void is an inward-wound shell sealed inside the kept body; an
    // inward-wound shell anywhere else is a lost part like any other.
    // Under VOID_SLIVER_MM3 an inward shell is a boolean's sliver lying
    // on the kept surface (its inside is ambiguous, and it holds no
    // material): filled, not lost.
    let inside: Vec<bool> = left_out
        .par_iter()
        .map(|(d, probe)| {
            d.volume_mm3 < 0.0 && (d.volume_mm3 > -VOID_SLIVER_MM3 || contains(&m, *probe))
        })
        .collect();
    let (voids, dropped): (Vec<_>, Vec<_>) = left_out
        .into_iter()
        .zip(inside)
        .map(|((d, _), i)| (d, i))
        .partition(|(_, i)| *i);
    let voids = voids.into_iter().map(|(d, _)| d).collect();
    let (slivers, dropped): (Vec<_>, Vec<_>) = dropped
        .into_iter()
        .map(|(d, _)| d)
        .partition(|d| d.volume_mm3.abs() < VOID_SLIVER_MM3);
    let watertight = m.is_watertight();
    let (mut overhang, holes) = overhang_faces(&m, footprint, printer);
    let areas = face_areas(&m);
    let mut in_band = vec![false; overhang.len()];
    let mut band_budget_mm2 = 0.0;
    let mut overhang_budget_mm2 = printer.overhang_budget_mm2;
    if let Some(b) = band {
        let tolerant: Vec<bool> = source
            .iter()
            .map(|&t| {
                b.materials.is_empty()
                    || materials
                        .get(t as usize)
                        .is_some_and(|mat| b.materials.contains(mat))
            })
            .collect();
        if overhang.iter().zip(&tolerant).any(|(o, t)| *o && *t) {
            let cap = Printer {
                overhang_deg: b.max_deg,
                ..*printer
            };
            let (hard, _) = overhang_faces(&m, footprint, &cap);
            for k in 0..overhang.len() {
                if overhang[k] && tolerant[k] && !hard[k] {
                    overhang[k] = false;
                    in_band[k] = true;
                }
            }
        }
        let down: f64 = m
            .triangles
            .iter()
            .zip(&tolerant)
            .zip(&areas)
            .filter(|((t, tol), _)| {
                **tol && {
                    let [a, b2, c] = t.map(|i| m.positions[i as usize]);
                    cross(sub(b2, a), sub(c, a))[2] < 0.0
                }
            })
            .map(|(_, a)| a)
            .sum();
        band_budget_mm2 = b.share * down;
        overhang_budget_mm2 = overhang_budget_mm2.max(b.over_share * down);
    }
    let band_mm2 = 0.0
        + in_band
            .iter()
            .zip(&areas)
            .filter(|(o, _)| **o)
            .map(|(_, a)| a)
            .sum::<f64>();
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
        slivers,
        pinches_moved,
        collapsed,
        watertight,
        overhang_mm2,
        overhang,
        overhang_budget_mm2,
        band_mm2,
        band_budget_mm2,
        band: in_band,
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
/// Each left-out body comes with the mean of its vertices, the probe for
/// [`contains`] (inside a small void even where its surface lies within
/// rounding of the kept body's).
#[allow(clippy::type_complexity)]
fn keep_largest(
    m: &TriMesh,
    source: &[u32],
) -> (TriMesh, Vec<u32>, usize, Vec<(Dropped, [f64; 3])>) {
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
        .map(|b| {
            (
                Dropped {
                    triangles: b.len(),
                    volume_mm3: signed_volume(&sub_mesh(b)),
                    bounds: b.iter().flat_map(|&k| m.triangles[k]).fold(
                        [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]],
                        |[lo, hi], v| {
                            let p = m.positions[v as usize];
                            [
                                [0, 1, 2].map(|k| lo[k].min(p[k])),
                                [0, 1, 2].map(|k| hi[k].max(p[k])),
                            ]
                        },
                    ),
                },
                {
                    let (mut sum, mut n) = ([0.0; 3], 0.0);
                    for &k in b {
                        for v in m.triangles[k] {
                            let p = m.positions[v as usize];
                            sum = [0, 1, 2].map(|i| sum[i] + p[i]);
                            n += 1.0;
                        }
                    }
                    sum.map(|x| x / n)
                },
            )
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

/// Is `p` inside the closed mesh `m`? The odd or even count of the crossings of a
/// vertical ray up from `p`, nudged off the lattice by a fixed offset so
/// it misses `p`'s own shell's edges; a crossing on a shared edge is
/// counted once by the top-left rule. Deterministic: no sampling.
fn contains(m: &TriMesh, p: [f64; 3]) -> bool {
    let (px, py) = (p[0] + 1.37e-4, p[1] + 0.91e-4);
    #[allow(clippy::float_cmp)]
    let top_left = |a: [f64; 3], b: [f64; 3]| (a[1] == b[1] && b[0] < a[0]) || b[1] < a[1];
    let mut odd = false;
    for t in &m.triangles {
        let mut v = t.map(|i| m.positions[i as usize]);
        if v.iter().all(|q| q[2] <= p[2])
            || v.iter().all(|q| q[0] < px)
            || v.iter().all(|q| q[0] > px)
            || v.iter().all(|q| q[1] < py)
            || v.iter().all(|q| q[1] > py)
        {
            continue;
        }
        let area =
            (v[1][0] - v[0][0]) * (v[2][1] - v[0][1]) - (v[2][0] - v[0][0]) * (v[1][1] - v[0][1]);
        if area == 0.0 {
            continue;
        }
        if area < 0.0 {
            v.swap(1, 2);
        }
        let edges = [(v[1], v[2]), (v[2], v[0]), (v[0], v[1])];
        let w = edges.map(|(a, b)| (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0]));
        #[allow(clippy::float_cmp)]
        let hit = (0..3).all(|k| w[k] > 0.0 || (w[k] == 0.0 && top_left(edges[k].0, edges[k].1)));
        if hit {
            let z = (w[0] * v[0][2] + w[1] * v[1][2] + w[2] * v[2][2]) / area.abs();
            if z > p[2] {
                odd = !odd;
            }
        }
    }
    odd
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

/// The overhang faces: the mesh sliced every layer by
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
/// joint prints"); the plain layer-reach rule, written for stacked rock with real
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

    /// library/printers/default.dtile's numbers.
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
        assert!(a.failures(&fdm()).is_empty());
        // the same inward shell outside the block is a lost part, not a fill
        let mut m = cube([0.0, 0.0, 0.0], [6.0, 6.0, 6.0]);
        let stray = cube([7.0, 7.0, 2.0], [8.0, 8.0, 3.0]);
        let base = u32::try_from(m.positions.len()).unwrap();
        m.positions.extend(stray.positions);
        m.triangles.extend(
            stray
                .triangles
                .iter()
                .map(|t| [t[0] + base, t[2] + base, t[1] + base]),
        );
        let (_, a) = audit(&m, [10.0, 10.0], &fdm());
        assert!(a.voids.is_empty(), "{:?}", a.voids);
        assert_eq!(a.dropped.len(), 1);
        assert!(!a.failures(&fdm()).is_empty());
    }

    #[test]
    fn a_loose_sliver_is_not_a_lost_part() {
        // a block and, apart from it, a 0.05 mm speck (1.25e-4 mm³)
        let mut m = cube([0.0, 0.0, 0.0], [6.0, 6.0, 6.0]);
        let speck = cube([8.0, 8.0, 1.0], [8.05, 8.05, 1.05]);
        let base = u32::try_from(m.positions.len()).unwrap();
        m.positions.extend(speck.positions);
        m.triangles
            .extend(speck.triangles.iter().map(|t| t.map(|i| i + base)));
        let (_, a) = audit(&m, [10.0, 10.0], &fdm());
        assert!(a.dropped.is_empty(), "{:?}", a.dropped);
        assert_eq!(a.slivers.len(), 1);
        assert!(a.failures(&fdm()).is_empty());
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
        let fails = a.failures(&fdm());
        assert!(
            fails.iter().any(|f| f.contains("2 loose bodies")),
            "{fails:?}"
        );
        let (_, b) = audit(&shelf_on_post(), [10.0, 10.0], &fdm());
        // the layer-reach rule gives 8.0 mm² on this shelf (4 × 2 mm underside)
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

    /// A post whose underside leans out at `deg` from vertical.
    fn leaning(deg: f64) -> TriMesh {
        let rise = 4.0 / deg.to_radians().tan();
        let prof = [[0.0, 0.0], [2.0, 0.0], [6.0, rise], [6.0, 5.0], [0.0, 5.0]];
        prism(&prof, &[[0, 1, 2], [0, 2, 3], [0, 3, 4]])
    }

    #[test]
    fn a_band_tolerates_its_materials_up_to_its_cap() {
        use crate::scene::Material;
        let band = Band {
            materials: &[Material::Leaf],
            max_deg: 65.0,
            share: 1.0,
            over_share: 0.0,
        };
        let at = |deg: f64, mat: Material, band: Option<&Band>| {
            let m = leaning(deg);
            let mats = vec![mat; m.triangles.len()];
            audit_banded(&m, [10.0, 10.0], &fdm(), &mats, band).1
        };
        // 60°: past 55, within 65: band for leaves, overhang for anything else
        let a = at(60.0, Material::Leaf, Some(&band));
        assert!(a.overhang_mm2 == 0.0 && a.band_mm2 > 1.0, "{a:?}");
        assert!(a.failures(&fdm()).is_empty(), "{:?}", a.failures(&fdm()));
        assert!(at(60.0, Material::Masonry, Some(&band)).overhang_mm2 > 1.0);
        assert!(at(60.0, Material::Leaf, None).overhang_mm2 > 1.0);
        // 70°: past the cap fails whatever the material
        let b = at(70.0, Material::Leaf, Some(&band));
        assert!(b.overhang_mm2 > 1.0 && b.band_mm2 == 0.0, "{b:?}");
        assert!(!b.failures(&fdm()).is_empty());
        // over the budget fails
        let tight = Band { share: 0.0, ..band };
        let c = at(60.0, Material::Leaf, Some(&tight));
        assert!(!c.failures(&fdm()).is_empty());
        // past the cap within `over_share` passes, red but allowed
        let loose = Band {
            over_share: 1.0,
            ..band
        };
        let d = at(70.0, Material::Leaf, Some(&loose));
        assert!(
            d.overhang_mm2 > 1.0 && d.overhang_budget_mm2 >= d.overhang_mm2,
            "{d:?}"
        );
        assert!(d.failures(&fdm()).is_empty(), "{:?}", d.failures(&fdm()));
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
        // the plain rule (every void open) flags its roof
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
