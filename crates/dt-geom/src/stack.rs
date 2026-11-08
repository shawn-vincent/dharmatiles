//! SPIKE (M0): the terrain stack mesher. Two height fields (an earth
//! layer `e` and a water layer `w` over it) become ONE closed, painted
//! mesh: the exterior top is the higher of the two, each triangle carries
//! its layer's run id, and where the top changes layer (a shore) the mesh
//! has an exact edge along the contour (`w = e`).
//!
//! - cells a boundary crosses are forced to single cells (the adaptive
//!   lift's balance rule grades the rest);
//! - a triangle a boundary crosses is cut along the line, cut points are
//!   cached per grid edge and shared (never duplicated), snapped to a node
//!   within [`SNAP_MM`];
//! - every vertex is rounded to float32 once and shared by index, so the
//!   mesh is watertight after an STL reload with no repair;
//! - the sides are panels painted by layer intervals (earth up to its top,
//!   water from there to the water's top); the bottom is earth.

use std::collections::HashMap;

use crate::TriMesh;
use crate::grid::{Grid, LiftError};
use crate::lift::{Corners, Leaf, centre, plan_leaves};

/// A cut point this close to a node is the node.
pub const SNAP_MM: f64 = 1e-4;

#[allow(clippy::cast_possible_truncation)]
fn r32(v: f64) -> f64 {
    f64::from(v as f32)
}

/// The stack lift. `e` is the earth's top at every node; `w` the water's
/// top (`None`: no water). `ids` are the runs' original IDs (earth,
/// water). `x0, y0` shift nothing: the tile's corner is the origin.
///
/// # Errors
/// [`LiftError`] when a node is not above 0 or the grid is too small.
#[allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]
pub fn stack_lift(
    nx: usize,
    ny: usize,
    step: f64,
    e: &[f32],
    w: Option<&[f32]>,
    ids: [u32; 2],
    tol: f64,
    max_cells: usize,
) -> Result<TriMesh, LiftError> {
    let n = nx * ny;
    let class: Vec<u8> = (0..n)
        .map(|k| w.map_or(0, |w| u8::from(f64::from(w[k]) - f64::from(e[k]) > 1e-4)))
        .collect();
    let top: Vec<f32> = (0..n)
        .map(|k| if class[k] == 1 { w.map_or(e[k], |w| w[k]) } else { e[k] })
        .collect();
    let g = Grid {
        nx,
        ny,
        step,
        z: top.clone(),
    };
    // summed-area table of water nodes
    let sw = nx + 1;
    let mut sat = vec![0u32; sw * (ny + 1)];
    for j in 0..ny {
        for i in 0..nx {
            sat[(j + 1) * sw + i + 1] =
                u32::from(class[j * nx + i]) + sat[j * sw + i + 1] + sat[(j + 1) * sw + i]
                    - sat[j * sw + i];
        }
    }
    let force = |l: Leaf| {
        let c = sat[(l.j1 + 1) * sw + l.i1 + 1] + sat[l.j0 * sw + l.i0]
            - sat[l.j0 * sw + l.i1 + 1]
            - sat[(l.j1 + 1) * sw + l.i0];
        let area = ((l.i1 - l.i0 + 1) * (l.j1 - l.j0 + 1)) as u32;
        c > 0 && c < area
    };
    let leaves = plan_leaves(&g, tol, max_cells, &force)?;
    let verts = Corners::new(nx, ny, &leaves);
    let d = |k: usize| w.map_or(-1.0, |w| f64::from(w[k]) - f64::from(e[k]));
    let mut index = vec![u32::MAX; n];
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut cuts: HashMap<(usize, usize), u32> = HashMap::new();
    let mut tris: [Vec<[u32; 3]>; 2] = [Vec::new(), Vec::new()];
    let mut wall: Vec<[u32; 3]> = Vec::new();
    let mut wall_w: Vec<[u32; 3]> = Vec::new();
    let u = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);

    let mut vid = |k: usize, positions: &mut Vec<[f64; 3]>| -> u32 {
        if index[k] == u32::MAX {
            index[k] = u(positions.len());
            let (i, j) = (k % nx, k / nx);
            positions.push([r32(i as f64 * step), r32(j as f64 * step), f64::from(top[k])]);
        }
        index[k]
    };
    // the vertex where the boundary crosses grid edge (a, b), a < b
    let mut cut = |a: usize, b: usize,
                   positions: &mut Vec<[f64; 3]>,
                   vid: &mut dyn FnMut(usize, &mut Vec<[f64; 3]>) -> u32|
     -> u32 {
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        if let Some(&v) = cuts.get(&(a, b)) {
            return v;
        }
        let (da, db) = (d(a), d(b));
        let t = (da / (da - db)).clamp(0.0, 1.0);
        let pa = [(a % nx) as f64 * step, (a / nx) as f64 * step];
        let pb = [(b % nx) as f64 * step, (b / nx) as f64 * step];
        let p = [pa[0] + t * (pb[0] - pa[0]), pa[1] + t * (pb[1] - pa[1])];
        let z = f64::from(e[a]) + t * (f64::from(e[b]) - f64::from(e[a]));
        let v = if t * libm::hypot(pb[0] - pa[0], pb[1] - pa[1]) < SNAP_MM {
            vid(a, positions)
        } else if (1.0 - t) * libm::hypot(pb[0] - pa[0], pb[1] - pa[1]) < SNAP_MM {
            vid(b, positions)
        } else {
            let v = u(positions.len());
            positions.push([r32(p[0]), r32(p[1]), r32(z)]);
            v
        };
        cuts.insert((a, b), v);
        v
    };

    // a triangle of grid nodes, cut along the boundary if it crosses it
    let mut emit = |nodes: [usize; 3],
                    positions: &mut Vec<[f64; 3]>,
                    vid: &mut dyn FnMut(usize, &mut Vec<[f64; 3]>) -> u32,
                    tris: &mut [Vec<[u32; 3]>; 2]| {
        let c = nodes.map(|k| class[k]);
        let v: [u32; 3] = [
            vid(nodes[0], positions),
            vid(nodes[1], positions),
            vid(nodes[2], positions),
        ];
        let push = |t: [u32; 3], layer: u8, tris: &mut [Vec<[u32; 3]>; 2]| {
            if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
                tris[layer as usize].push(t);
            }
        };
        if c[0] == c[1] && c[1] == c[2] {
            push(v, c[0], tris);
            return;
        }
        // rotate so the lone node is first
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
    };

    for &l in &leaves {
        let ring = verts.ring(l);
        let nodes: Vec<usize> = ring.iter().map(|&(i, j)| j * nx + i).collect();
        if nodes.len() == 4 {
            emit([nodes[0], nodes[1], nodes[2]], &mut positions, &mut vid, &mut tris);
            emit([nodes[0], nodes[2], nodes[3]], &mut positions, &mut vid, &mut tris);
        } else {
            // a uniform leaf (a mixed one is a cell): a fan from its centre
            let layer = class[nodes[0]];
            let ids_: Vec<u32> = nodes.iter().map(|&k| vid(k, &mut positions)).collect();
            let c = centre(&g, l);
            let cv = u(positions.len());
            positions.push([r32(c[0]), r32(c[1]), r32(c[2])]);
            for k in 0..ids_.len() {
                tris[layer as usize].push([cv, ids_[k], ids_[(k + 1) % ids_.len()]]);
            }
        }
    }
    // the rim: grid nodes with the boundary's crossings inserted
    let whole = Leaf {
        i0: 0,
        j0: 0,
        i1: nx - 1,
        j1: ny - 1,
    };
    let ring: Vec<usize> = verts
        .ring(whole)
        .iter()
        .map(|&(i, j)| j * nx + i)
        .collect();
    // (top vertex, earth-top vertex, x, y, earth z)
    let mut rim: Vec<(u32, u32, f64, f64, f64)> = Vec::new();
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        let (i, j) = (a % nx, a / nx);
        let t = vid(a, &mut positions);
        let m = if class[a] == 1 {
            let v = u(positions.len());
            positions.push([r32(i as f64 * step), r32(j as f64 * step), f64::from(e[a])]);
            v
        } else {
            t
        };
        rim.push((t, m, r32(i as f64 * step), r32(j as f64 * step), f64::from(e[a])));
        if class[a] != class[b] {
            let c = cut(a, b, &mut positions, &mut vid);
            // a cut snapped onto a node is that node: not a second rim vertex
            if c != vid(a, &mut positions) && c != vid(b, &mut positions) {
                let p = positions[c as usize];
                rim.push((c, c, p[0], p[1], p[2]));
            }
        }
    }
    // floor: the rim at z = 0 and a fan from the middle
    let base = u(positions.len());
    for r in &rim {
        positions.push([r.2, r.3, 0.0]);
    }
    let mid = u(positions.len());
    positions.push([
        r32(0.5 * (nx - 1) as f64 * step),
        r32(0.5 * (ny - 1) as f64 * step),
        0.0,
    ]);
    let m = rim.len();
    for k in 0..m {
        let k1 = (k + 1) % m;
        let (t0, m0, t1, m1) = (rim[k].0, rim[k].1, rim[k1].0, rim[k1].1);
        let (b0, b1) = (base + u(k), base + u(k1));
        // earth: floor to the earth's top
        for t in [[m0, b0, b1], [m0, b1, m1]] {
            if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
                wall.push(t);
            }
        }
        // water: the earth's top to the water's
        for t in [[t0, m0, m1], [t0, m1, t1]] {
            if t[0] != t[1] && t[1] != t[2] && t[0] != t[2] {
                wall_w.push(t);
            }
        }
        wall.push([mid, b1, b0]);
    }
    tris[0].extend(wall);
    tris[1].extend(wall_w);
    let mut out = TriMesh {
        positions,
        ..TriMesh::default()
    };
    let (n0, n1) = (tris[0].len(), tris[1].len());
    out.triangles.extend(tris[0].iter().copied());
    out.triangles.extend(tris[1].iter().copied());
    out.run_index = vec![0];
    if n0 > 0 {
        out.run_original_id.push(ids[0]);
        out.run_index.push(u(n0 * 3));
    }
    if n1 > 0 {
        out.run_original_id.push(ids[1]);
        out.run_index.push(u((n0 + n1) * 3));
    }
    if out.run_original_id.len() == 1 {
        out.run_index = vec![0, u(out.triangles.len() * 3)];
    }
    // a float32 import carries about 1e-7 of the tile's size
    out.tolerance = 1e-7 * step * (nx.max(ny) - 1) as f64;
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::cast_precision_loss)]
    use super::*;
    use crate::Solid;

    fn wavy(nx: usize, ny: usize) -> (Vec<f32>, Vec<f32>) {
        let mut e = vec![3.0f32; nx * ny];
        let mut w = vec![3.0f32; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (i as f32 * 0.25, j as f32 * 0.25);
                e[j * nx + i] = 3.0 + 0.5 * (x * 0.4).sin() * (y * 0.3).cos();
                w[j * nx + i] = 3.2 + 0.05 * (x * 2.0).sin() - 0.3 * x * 0.3; // crosses e
            }
        }
        (e, w)
    }

    #[test]
    fn stack_is_watertight_painted_and_exact() {
        let (nx, ny) = (81, 61);
        let (e, w) = wavy(nx, ny);
        let m = stack_lift(nx, ny, 0.25, &e, Some(&w), [7, 9], 0.02, 16).unwrap();
        assert!(m.is_watertight());
        assert_eq!(m.run_original_id, vec![7, 9]);
        // survives the float32 reload
        let mut f = m.clone();
        for p in &mut f.positions {
            *p = p.map(|v| f64::from(v as f32));
        }
        assert_eq!(f.positions, m.positions);
        let s = Solid::from_tri_mesh(&m).unwrap();
        let out = s.to_tri_mesh();
        assert!(out.is_watertight());
        assert_eq!(out.run_original_id.len(), 2);
    }

    /// Risk test: do runs stay apart across coplanar faces through a union?
    #[test]
    fn runs_survive_coplanar_union() {
        let (nx, ny) = (41, 41);
        // two layers at the very same flat plane except a ramp, so water and
        // earth tops are coplanar where they meet
        let w = vec![3.2f32; nx * ny];
        let mut e = vec![3.2f32; nx * ny];
        for j in 0..ny {
            for i in 21..nx {
                e[j * nx + i] = 2.0;
            }
        }
        let m = stack_lift(nx, ny, 0.25, &e, Some(&w), [7, 9], 0.02, 16).unwrap();
        assert!(m.is_watertight());
        let before = m.triangle_original_ids().unwrap();
        let (b0, b1) = (
            before.iter().filter(|&&i| i == 7).count(),
            before.iter().filter(|&&i| i == 9).count(),
        );
        let t = Solid::from_tri_mesh(&m).unwrap();
        // a box standing on the earth half, a coplanar-touching one on water
        let boxm = |x0: f64, x1: f64, z1: f64| {
            let mut b = Solid(manifold_csg::Manifold::cube(x1 - x0, 2.0, z1 - 1.0, false));
            b = Solid(b.0.translate(x0, 3.0, 1.0));
            let m = b.to_tri_mesh();
            Solid::from_tri_mesh(&m).unwrap().with_original_id(1).unwrap()
        };
        let u = t.union(&boxm(1.0, 3.0, 5.0)).union(&boxm(4.0, 6.5, 6.0));
        let o = u.to_tri_mesh();
        assert!(o.is_watertight());
        let ids = o.triangle_original_ids().unwrap();
        let (a0, a1) = (
            ids.iter().filter(|&&i| i == 7).count(),
            ids.iter().filter(|&&i| i == 9).count(),
        );
        eprintln!("runs before: earth {b0} water {b1}; after union: earth {a0} water {a1}");
        assert!(a0 > 0 && a1 > 0);
        // geometry: run 7's upward faces stay left of the shore (x = 5),
        // run 9's right of it
        let (mut bad7, mut bad9, mut up7, mut up9) = (0, 0, 0, 0);
        for (t, id) in o.triangles.iter().zip(&ids) {
            let [a, b, c] = t.map(|i| o.positions[i as usize]);
            let nz = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if nz <= 1e-9 || a[2] < 3.0 {
                continue;
            }
            let (lo, hi) = (a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]));
            match id {
                7 => {
                    up7 += 1;
                    bad7 += usize::from(hi > 5.0 + 1e-6);
                }
                9 => {
                    up9 += 1;
                    bad9 += usize::from(lo < 5.0 - 1e-6);
                }
                _ => {}
            }
        }
        eprintln!("upward: earth {up7} (misplaced {bad7}), water {up9} (misplaced {bad9})");
        assert!(up7 > 0 && up9 > 0 && bad7 == 0 && bad9 == 0);
    }
}
