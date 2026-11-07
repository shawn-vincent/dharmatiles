//! The mesh made what the float32 reload will read: split vertices merged,
//! pinches pushed apart, positions quantised, and only the largest body
//! kept (the rest reported).

use super::*;

/// Positions with split vertices merged and unused ones gone; only
/// `positions` and `triangles` are kept.
pub(super) fn canonical(m: &TriMesh) -> TriMesh {
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
pub(super) fn separate_pinches(m: &mut TriMesh) -> usize {
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
        // (each point on its own; `pairs` is sorted below, so the order
        // they are found in does not matter)
        let near: Vec<(u32, u32, f64)> = m
            .positions
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, p)| {
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
                let cells = &cells;
                let positions = &m.positions;
                (0..8).flat_map(move |c| {
                    let cell = [0, 1, 2].map(|a| k[a] + if c >> a & 1 == 1 { side[a] } else { 0 });
                    cells
                        .get(&cell)
                        .into_iter()
                        .flatten()
                        .filter(move |&&j| j > i)
                        .filter_map(move |&j| {
                            let d = dist(*p, positions[j as usize]);
                            (d < NEAR).then_some((i, j, d))
                        })
                })
            })
            .collect();
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
pub(super) struct FxHasher(u64);

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
pub(super) fn quantise(m: &TriMesh) -> (TriMesh, Vec<u32>, usize) {
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
pub(super) fn keep_largest(
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
pub(super) fn contains(m: &TriMesh, p: [f64; 3]) -> bool {
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
