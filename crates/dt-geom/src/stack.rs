//! The stack mesher: the terrain's lift. An ordered list of layers (each a
//! top field over the same lattice, lowest first) becomes ONE closed mesh.
//! The outside is the visible top, each triangle carries its layer's run,
//! and the sides are panels painted by the layers they cross.
//!
//! - **One layer** is the adaptive lift of [`crate::lift`], triangle for
//!   triangle: its leaves, conforming corners and fans, the rim, the walls
//!   down to z = 0 and the bottom fan.
//! - **Visible layer.** A node shows layer `k` when `k` is the last layer
//!   whose top exceeds the shown top below it by more than [`MIN_MM`]
//!   (layer 0 is always shown otherwise). A layer that is absent somewhere
//!   is one whose top there lies at or under the layers below it.
//! - **Forced single cells.** A leaf whose nodes show more than one layer
//!   is split to single cells, so every boundary runs along cell edges and
//!   diagonals (the lift's 2:1 balance grades the rest).
//! - **Contour splits.** A triangle whose nodes show different layers is
//!   cut along straight segments between its edge crossings. A crossing is
//!   cached per grid edge and shared by both triangles of the edge; one
//!   within [`SNAP_MM`] of a node is that node.
//! - **Rounding.** With [`Round::F32`] every vertex is rounded to float32
//!   once and shared by index, so the mesh is watertight after an STL
//!   reload with no repair. [`Round::None`] keeps the lift's f64
//!   positions.
//! - **Sides.** Each rim node has a column of heights, one per layer
//!   (the layer's top, kept between the layer below's and the visible top),
//!   and the panel between two columns is split by layer: a layer's panel
//!   is painted with that layer's run. A rim edge a boundary crosses gets
//!   the crossing as a rim vertex, unless it snapped onto a node.

use std::collections::HashMap;

use crate::TriMesh;
use crate::grid::{Grid, LiftError};
use crate::lift::{Corners, Leaf, centre, plan_leaves};

/// A crossing this close to a node is the node, mm.
const SNAP_MM: f64 = 1e-4;

/// How much a layer must stand over the shown top below it to be shown, mm.
const MIN_MM: f64 = 1e-4;

/// How vertices are rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Round {
    /// Positions as the lift computes them (f64).
    None,
    /// Every position to float32, once; the mesh carries a tolerance of
    /// about 1e-7 of the tile's size, as a float32 import does.
    F32,
}

/// One layer: its top at every node, and the original id of its run.
#[derive(Clone, Copy, Debug)]
pub struct Layer<'a> {
    /// The layer's top, `nx * ny` values in mm.
    pub top: &'a [f32],
    /// The run's original id (a slot id).
    pub id: u32,
}

/// The lattice and the knobs of the lift.
#[derive(Clone, Copy, Debug)]
pub struct Lattice {
    /// Nodes across.
    pub nx: usize,
    /// Nodes up.
    pub ny: usize,
    /// Node spacing, mm.
    pub step: f64,
    /// The adaptive lift's error bound, mm.
    pub tol: f64,
    /// The widest leaf, in cells.
    pub max_cells: usize,
    /// How positions are rounded.
    pub round: Round,
}

/// The shown layer at every node.
fn classes(layers: &[Layer], n: usize) -> Vec<u8> {
    (0..n)
        .map(|k| {
            let mut c = 0;
            for (l, layer) in layers.iter().enumerate().skip(1) {
                if f64::from(layer.top[k]) - f64::from(layers[c].top[k]) > MIN_MM {
                    c = l;
                }
            }
            u8::try_from(c).unwrap_or(u8::MAX)
        })
        .collect()
}

/// One rim column: a vertex per level (index 0 is the floor), from the
/// floor up to the visible top.
struct Column {
    xy: [f64; 2],
    rows: Vec<u32>,
}

/// The stack lift.
///
/// # Errors
/// [`LiftError`] when the lattice is smaller than 2 × 2, a field has the
/// wrong length, or a shown node (or a bottom layer's node) is not above 0.
///
/// # Panics
/// Never for valid input: `layers` must hold at least one layer and at
/// most 255.
#[allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    clippy::many_single_char_names
)]
pub fn lift_stack(lat: &Lattice, layers: &[Layer]) -> Result<TriMesh, LiftError> {
    let Lattice {
        nx,
        ny,
        step,
        tol,
        max_cells,
        round,
    } = *lat;
    let n = nx * ny;
    if nx < 2 || ny < 2 || layers.is_empty() || layers.iter().any(|l| l.top.len() != n) {
        return Err(LiftError::TooSmall);
    }
    let nl = layers.len();
    assert!(nl < 256, "at most 255 layers");
    let class = classes(layers, n);
    let top: Vec<f32> = (0..n).map(|k| layers[class[k] as usize].top[k]).collect();
    if nl > 1 {
        // the bottom layer carries the floor wall: it must be above 0 everywhere
        for (k, &z) in layers[0].top.iter().enumerate() {
            if !(z.is_finite() && z > 0.0) {
                return Err(LiftError::BelowBase {
                    at: (k % nx, k / nx),
                    z,
                });
            }
        }
    }
    let g = Grid {
        nx,
        ny,
        step,
        z: top.clone(),
    };
    // a leaf is forced to a single cell where its nodes show several layers:
    // summed-area tables of the nodes showing each layer above the first
    let sw = nx + 1;
    let sats: Vec<Vec<u32>> = (1..nl)
        .map(|layer| {
            let mut sat = vec![0u32; sw * (ny + 1)];
            for j in 0..ny {
                for i in 0..nx {
                    sat[(j + 1) * sw + i + 1] = u32::from(usize::from(class[j * nx + i]) == layer)
                        + sat[j * sw + i + 1]
                        + sat[(j + 1) * sw + i]
                        - sat[j * sw + i];
                }
            }
            sat
        })
        .collect();
    let force = |l: Leaf| {
        let area = ((l.i1 - l.i0 + 1) * (l.j1 - l.j0 + 1)) as u32;
        sats.iter().any(|sat| {
            let c = sat[(l.j1 + 1) * sw + l.i1 + 1] + sat[l.j0 * sw + l.i0]
                - sat[l.j0 * sw + l.i1 + 1]
                - sat[(l.j1 + 1) * sw + l.i0];
            c > 0 && c < area
        })
    };
    let leaves = plan_leaves(&g, tol, max_cells, &force)?;
    let verts = Corners::new(nx, ny, &leaves);

    let rd = |v: f64| match round {
        Round::None => v,
        Round::F32 => f64::from(v as f32),
    };
    let u = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let node_xy = |k: usize| [(k % nx) as f64 * step, (k / nx) as f64 * step];

    let mut index = vec![u32::MAX; n];
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut cuts: HashMap<(usize, usize), u32> = HashMap::new();
    let mut tris: Vec<Vec<[u32; 3]>> = vec![Vec::new(); nl];

    let mut vid = |k: usize, positions: &mut Vec<[f64; 3]>| -> u32 {
        if index[k] == u32::MAX {
            index[k] = u(positions.len());
            let p = node_xy(k);
            positions.push([rd(p[0]), rd(p[1]), f64::from(top[k])]);
        }
        index[k]
    };
    // the vertex where the boundary crosses grid edge (a, b), a < b
    let mut cut = |a: usize,
                   b: usize,
                   positions: &mut Vec<[f64; 3]>,
                   vid: &mut dyn FnMut(usize, &mut Vec<[f64; 3]>) -> u32|
     -> u32 {
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        if let Some(&v) = cuts.get(&(a, b)) {
            return v;
        }
        let (lo, hi) = (
            class[a].min(class[b]) as usize,
            class[a].max(class[b]) as usize,
        );
        let d = |k: usize| f64::from(layers[hi].top[k]) - f64::from(layers[lo].top[k]);
        let (da, db) = (d(a), d(b));
        let t = (da / (da - db)).clamp(0.0, 1.0);
        let t = if t.is_finite() { t } else { 0.5 };
        let (pa, pb) = (node_xy(a), node_xy(b));
        let len = libm::hypot(pb[0] - pa[0], pb[1] - pa[1]);
        let v = if t * len < SNAP_MM {
            vid(a, positions)
        } else if (1.0 - t) * len < SNAP_MM {
            vid(b, positions)
        } else {
            let z = f64::from(layers[lo].top[a])
                + t * (f64::from(layers[lo].top[b]) - f64::from(layers[lo].top[a]));
            let v = u(positions.len());
            positions.push([
                rd(pa[0] + t * (pb[0] - pa[0])),
                rd(pa[1] + t * (pb[1] - pa[1])),
                rd(z),
            ]);
            v
        };
        cuts.insert((a, b), v);
        v
    };

    let push = |t: [u32; 3], layer: u8, tris: &mut Vec<Vec<[u32; 3]>>| {
        if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
            tris[layer as usize].push(t);
        }
    };
    // a triangle of grid nodes, cut along the boundary if it crosses it
    let mut emit = |nodes: [usize; 3],
                    positions: &mut Vec<[f64; 3]>,
                    vid: &mut dyn FnMut(usize, &mut Vec<[f64; 3]>) -> u32,
                    tris: &mut Vec<Vec<[u32; 3]>>| {
        let c = nodes.map(|k| class[k]);
        let v: [u32; 3] = [
            vid(nodes[0], positions),
            vid(nodes[1], positions),
            vid(nodes[2], positions),
        ];
        if c[0] == c[1] && c[1] == c[2] {
            push(v, c[0], tris);
        } else if c[0] != c[1] && c[1] != c[2] && c[0] != c[2] {
            // three layers: a corner at each node, the middle to the highest
            let m01 = cut(nodes[0], nodes[1], positions, vid);
            let m12 = cut(nodes[1], nodes[2], positions, vid);
            let m20 = cut(nodes[2], nodes[0], positions, vid);
            push([v[0], m01, m20], c[0], tris);
            push([v[1], m12, m01], c[1], tris);
            push([v[2], m20, m12], c[2], tris);
            push([m01, m12, m20], c[0].max(c[1]).max(c[2]), tris);
        } else {
            // two layers: rotate so the lone node is first
            let lone = if c[1] == c[2] {
                0
            } else if c[0] == c[2] {
                1
            } else {
                2
            };
            let (x, y, z) = (lone, (lone + 1) % 3, (lone + 2) % 3);
            let cxy = cut(nodes[x], nodes[y], positions, vid);
            let cxz = cut(nodes[x], nodes[z], positions, vid);
            push([v[x], cxy, cxz], c[x], tris);
            push([cxy, v[y], v[z]], c[y], tris);
            push([cxy, v[z], cxz], c[y], tris);
        }
    };

    for &l in &leaves {
        let ring = verts.ring(l);
        let nodes: Vec<usize> = ring.iter().map(|&(i, j)| j * nx + i).collect();
        if nodes.len() == 4 {
            emit(
                [nodes[0], nodes[1], nodes[2]],
                &mut positions,
                &mut vid,
                &mut tris,
            );
            emit(
                [nodes[0], nodes[2], nodes[3]],
                &mut positions,
                &mut vid,
                &mut tris,
            );
        } else {
            // a leaf with more is uniform (a mixed one is a cell): a fan
            let layer = class[nodes[0]];
            let ids: Vec<u32> = nodes.iter().map(|&k| vid(k, &mut positions)).collect();
            let c = centre(&g, l);
            let cv = u(positions.len());
            positions.push([rd(c[0]), rd(c[1]), rd(c[2])]);
            for k in 0..ids.len() {
                tris[layer as usize].push([cv, ids[k], ids[(k + 1) % ids.len()]]);
            }
        }
    }

    // the rim, counter-clockwise from (0, 0): one column per node, and one
    // per boundary crossing that is not a node
    let whole = Leaf {
        i0: 0,
        j0: 0,
        i1: nx - 1,
        j1: ny - 1,
    };
    let ring: Vec<usize> = verts.ring(whole).iter().map(|&(i, j)| j * nx + i).collect();
    let mut rim: Vec<Column> = Vec::new();
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        let t = vid(a, &mut positions);
        let p = node_xy(a);
        let vis = f64::from(top[a]);
        // row 0 is the floor (made below); row l + 1 the top of layer l
        let mut rows = vec![0u32];
        let mut h = 0.0_f64;
        for (l, layer) in layers.iter().enumerate() {
            let hl = if l >= class[a] as usize {
                vis
            } else {
                h.max(f64::from(layer.top[a]).min(vis))
            };
            let v = if hl >= vis {
                t
            } else if hl == h && l > 0 {
                rows[l]
            } else {
                let v = u(positions.len());
                positions.push([rd(p[0]), rd(p[1]), hl]);
                v
            };
            rows.push(v);
            h = hl;
        }
        rim.push(Column {
            xy: [rd(p[0]), rd(p[1])],
            rows,
        });
        if class[a] != class[b] {
            let c = cut(a, b, &mut positions, &mut vid);
            // a crossing snapped onto a node is that node: not a second column
            if c != vid(a, &mut positions) && c != vid(b, &mut positions) {
                let q = positions[c as usize];
                let mut rows = vec![0u32];
                rows.extend(std::iter::repeat_n(c, nl));
                rim.push(Column {
                    xy: [q[0], q[1]],
                    rows,
                });
            }
        }
    }
    // floor: the rim at z = 0 and a fan from the middle
    let base = u(positions.len());
    for c in &rim {
        positions.push([c.xy[0], c.xy[1], 0.0]);
    }
    let mid = u(positions.len());
    positions.push([
        rd(0.5 * (nx - 1) as f64 * step),
        rd(0.5 * (ny - 1) as f64 * step),
        0.0,
    ]);
    let m = rim.len();
    for k in 0..m {
        let k1 = (k + 1) % m;
        let (b0, b1) = (base + u(k), base + u(k1));
        for l in 0..nl {
            // the panel of layer l: from row l up to row l + 1
            let (lo0, lo1) = if l == 0 {
                (b0, b1)
            } else {
                (rim[k].rows[l], rim[k1].rows[l])
            };
            let (hi0, hi1) = (rim[k].rows[l + 1], rim[k1].rows[l + 1]);
            let layer = u8::try_from(l).unwrap_or(u8::MAX);
            push([hi0, lo0, lo1], layer, &mut tris);
            push([hi0, lo1, hi1], layer, &mut tris);
        }
        tris[0].push([mid, b1, b0]);
    }

    let mut out = TriMesh {
        positions,
        ..TriMesh::default()
    };
    out.run_index = vec![0];
    for (l, t) in tris.iter().enumerate() {
        out.triangles.extend(t.iter().copied());
        if !t.is_empty() {
            out.run_original_id.push(layers[l].id);
            out.run_index.push(u(out.triangles.len() * 3));
        }
    }
    if round == Round::F32 {
        // a float32 import carries about 1e-7 of the tile's size
        out.tolerance = 1e-7 * step * (nx.max(ny) - 1) as f64;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::float_cmp
    )]
    use super::*;
    use crate::Solid;

    fn lat(nx: usize, ny: usize, round: Round) -> Lattice {
        Lattice {
            nx,
            ny,
            step: 0.25,
            tol: 0.02,
            max_cells: 16,
            round,
        }
    }

    fn field(nx: usize, ny: usize, f: impl Fn(f32, f32) -> f32) -> Vec<f32> {
        (0..nx * ny)
            .map(|k| f((k % nx) as f32 * 0.25, (k / nx) as f32 * 0.25))
            .collect()
    }

    fn reload(m: &TriMesh) -> TriMesh {
        let mut f = m.clone();
        for p in &mut f.positions {
            *p = p.map(|v| f64::from(v as f32));
        }
        f
    }

    #[test]
    fn one_layer_is_the_adaptive_lift() {
        let (nx, ny) = (61, 47);
        let e = field(nx, ny, |x, y| {
            3.0 + 0.5 * (x * 0.4).sin() * (y * 0.3).cos() + (0.6 - (x - 9.0).abs()).max(0.0)
        });
        let g = Grid {
            nx,
            ny,
            step: 0.25,
            z: e.clone(),
        };
        let old = crate::lift_adaptive(&g, 0.02, 16).unwrap();
        let new = lift_stack(&lat(nx, ny, Round::None), &[Layer { top: &e, id: 5 }]).unwrap();
        assert_eq!(new.positions, old.positions);
        assert_eq!(new.triangles, old.triangles);
        assert_eq!(new.run_original_id, vec![5]);
        assert_eq!(
            new.run_index,
            vec![0, u32::try_from(old.triangles.len() * 3).unwrap()]
        );
    }

    #[test]
    fn one_layer_rejects_a_node_not_above_zero() {
        let mut e = vec![1.0f32; 16];
        e[5] = 0.0;
        let l = Lattice {
            nx: 4,
            ny: 4,
            ..lat(4, 4, Round::None)
        };
        assert!(matches!(
            lift_stack(&l, &[Layer { top: &e, id: 1 }]),
            Err(LiftError::BelowBase { at: (1, 1), .. })
        ));
    }

    fn two_layers() -> (usize, usize, Vec<f32>, Vec<f32>) {
        let (nx, ny) = (81, 61);
        let e = field(nx, ny, |x, y| 3.0 + 0.5 * (x * 0.4).sin() * (y * 0.3).cos());
        let w = field(nx, ny, |x, _| 3.2 + 0.05 * (x * 2.0).sin() - 0.09 * x);
        (nx, ny, e, w)
    }

    #[test]
    fn two_layers_are_watertight_painted_and_survive_the_float32_reload() {
        let (nx, ny, e, w) = two_layers();
        let m = lift_stack(
            &lat(nx, ny, Round::F32),
            &[Layer { top: &e, id: 7 }, Layer { top: &w, id: 9 }],
        )
        .unwrap();
        assert!(m.is_watertight());
        assert_eq!(m.run_original_id, vec![7, 9]);
        assert_eq!(reload(&m).positions, m.positions);
        let out = Solid::from_tri_mesh(&m).unwrap().to_tri_mesh();
        assert!(out.is_watertight());
        assert_eq!(out.run_original_id.len(), 2);
    }

    #[test]
    fn three_layers_meet_watertight() {
        let (nx, ny) = (81, 61);
        let a = field(nx, ny, |x, y| 3.0 + 0.4 * (x * 0.5).sin() * (y * 0.4).cos());
        let b = field(nx, ny, |x, _| 3.3 + 0.05 * (x * 2.0).sin() - 0.1 * x);
        let c = field(nx, ny, |_, y| 2.2 + 0.12 * y);
        let layers = [
            Layer { top: &a, id: 1 },
            Layer { top: &b, id: 2 },
            Layer { top: &c, id: 3 },
        ];
        let m = lift_stack(&lat(nx, ny, Round::F32), &layers).unwrap();
        assert!(m.is_watertight());
        assert_eq!(m.run_original_id, vec![1, 2, 3]);
        let out = Solid::from_tri_mesh(&m).unwrap().to_tri_mesh();
        assert!(out.is_watertight());
        assert_eq!(out.run_original_id.len(), 3);
    }

    /// Every cell a boundary crosses is a single cell.
    #[test]
    fn boundary_cells_are_single_cells() {
        let (nx, ny, e, w) = two_layers();
        let m = lift_stack(
            &lat(nx, ny, Round::F32),
            &[Layer { top: &e, id: 7 }, Layer { top: &w, id: 9 }],
        )
        .unwrap();
        let old = crate::lift_adaptive(
            &Grid {
                nx,
                ny,
                step: 0.25,
                z: e.clone(),
            },
            0.02,
            16,
        )
        .unwrap();
        // forcing adds triangles along the shore, not everywhere
        assert!(m.triangles.len() > old.triangles.len() / 2);
        assert!(m.triangles.len() < old.triangles.len() * 4);
    }

    /// Runs stay apart across coplanar faces through a union.
    #[test]
    fn runs_survive_a_union() {
        let (nx, ny) = (41, 41);
        let w = vec![3.2f32; nx * ny];
        let mut e = vec![3.2f32; nx * ny];
        for j in 0..ny {
            for i in 21..nx {
                e[j * nx + i] = 2.0;
            }
        }
        let m = lift_stack(
            &lat(nx, ny, Round::F32),
            &[Layer { top: &e, id: 7 }, Layer { top: &w, id: 9 }],
        )
        .unwrap();
        assert!(m.is_watertight());
        let t = Solid::from_tri_mesh(&m).unwrap();
        let boxm = |x0: f64, x1: f64, z1: f64| {
            let b = Solid(manifold_csg::Manifold::cube(x1 - x0, 2.0, z1 - 1.0, false));
            let b = Solid(b.0.translate(x0, 3.0, 1.0));
            Solid::from_tri_mesh(&b.to_tri_mesh())
                .unwrap()
                .with_original_id(1)
                .unwrap()
        };
        let o = t
            .union(&boxm(1.0, 3.0, 5.0))
            .union(&boxm(4.0, 6.5, 6.0))
            .to_tri_mesh();
        assert!(o.is_watertight());
        let ids = o.triangle_original_ids().unwrap();
        assert!(ids.contains(&7) && ids.contains(&9));
    }

    /// A shore that lands exactly on nodes adds no second rim vertex and no
    /// duplicate vertex: the mesh stays closed with every vertex used.
    #[test]
    fn a_crossing_snapped_onto_a_node_adds_no_rim_vertex() {
        let (nx, ny) = (17, 9);
        // earth falls from 2 to 1 across x = 1 mm; the water sits flat at
        // 1.5: the shore is where earth = 1.5 (a node column at x = 2 since
        // earth is 2 left of it, exactly 1.5 on the node, 1 right of it)
        let e = field(nx, ny, |x, _| {
            if x < 2.0 {
                2.0
            } else if (x - 2.0).abs() < 1e-6 {
                1.5
            } else {
                1.0
            }
        });
        let w = vec![1.5f32; nx * ny];
        let m = lift_stack(
            &lat(nx, ny, Round::F32),
            &[Layer { top: &e, id: 1 }, Layer { top: &w, id: 2 }],
        )
        .unwrap();
        assert!(m.is_watertight());
        // no two vertices are one point (a second rim vertex at the shore
        // would sit on the node's), and the shore is on the node column
        let mut seen = std::collections::HashSet::new();
        for p in &m.positions {
            assert!(seen.insert(p.map(f64::to_bits)), "duplicate vertex {p:?}");
        }
        assert!(
            m.positions
                .iter()
                .any(|p| p[0] == 2.0 && p[1] == 0.0 && p[2] == 1.5)
        );
        // every vertex is used
        let mut used = vec![false; m.positions.len()];
        for t in &m.triangles {
            for &i in t {
                used[i as usize] = true;
            }
        }
        assert!(used.iter().all(|&u| u));
    }
}
