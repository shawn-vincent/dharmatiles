//! Lofting a closed, C1-continuous tube along every edge of a graph
//! (Bézier edges, Bishop frames).

use std::collections::VecDeque;

use super::*;

/// Sets each side's radius of a ring: `(ring, angles, radii)`.
pub type Surface<'a> = dyn Fn(&Ring, &[f64], &mut [f64]) + 'a;

/// How [`tubes`] lofts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TubeStyle {
    /// Longest ring spacing, mm.
    pub step_mm: f64,
    /// No ring thinner than this, mm.
    pub min_radius_mm: f64,
    /// Latitude rings of an end's dome.
    pub dome_rings: usize,
}

/// One ring of a tube, as the surface callback of [`tubes`] sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Ring {
    /// The edge (its child node).
    pub edge: usize,
    /// Arc length from the edge's parent node, mm (negative on the
    /// overlap into the parent).
    pub s: f64,
    /// The edge's arc length, mm.
    pub length: f64,
    /// The ring's centre.
    pub centre: [f64; 3],
    /// The edge's radius here, mm (linear from parent to child).
    pub radius: f64,
}

/// **tubes**: a closed tube along every edge of `g` with a curve: rings
/// on its [`Curve`], frames carried from the parent edge (Bishop frames,
/// so rings do not twist at forks), starting a little inside the parent
/// (a real overlap at every fork) and ending in a dome at an end node.
/// `sides(edge)` gives the ring's side count; `surface(ring, angles,
/// radii)` may set the radius of each side (it starts at the ring's
/// radius). Each tube is outward wound; index `i` is edge `i` (empty for
/// the root).
#[must_use]
pub fn tubes(
    g: &Graph,
    curves: &[Option<Curve>],
    style: &TubeStyle,
    sides: &dyn Fn(usize) -> usize,
    surface: &Surface<'_>,
) -> Vec<TriMesh> {
    let n = g.nodes.len();
    let ch = g.children();
    let mut frames: Vec<Option<([f64; 3], [f64; 3])>> = vec![None; n];
    frames[0] = Some(basis(unit(g.in_dirs[0]).unwrap_or([0.0, 0.0, 1.0])));
    let mut out = vec![TriMesh::default(); n];
    // breadth first: a parent's end frame before its children
    let mut queue: VecDeque<usize> = ch[0].iter().copied().collect();
    while let Some(i) = queue.pop_front() {
        queue.extend(ch[i].iter().copied());
        let Some(p) = g.parents[i] else { continue };
        let pf = frames[p].unwrap_or_else(|| basis(unit(g.in_dirs[p]).unwrap_or([0.0, 0.0, 1.0])));
        let Some(curve) = curves[i] else {
            frames[i] = Some(pf);
            continue;
        };
        let (mesh, end) = tube(
            g,
            i,
            p,
            &curve,
            pf,
            ch[i].is_empty(),
            style,
            sides(i),
            surface,
        );
        frames[i] = Some(end);
        out[i] = mesh;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn tube(
    g: &Graph,
    i: usize,
    p: usize,
    curve: &Curve,
    frame: ([f64; 3], [f64; 3]),
    end: bool,
    style: &TubeStyle,
    n_sides: usize,
    surface: &Surface<'_>,
) -> (TriMesh, ([f64; 3], [f64; 3])) {
    let r_start = g.radii[p].max(style.min_radius_mm);
    let r_end = g.radii[i].max(style.min_radius_mm);
    let p0 = curve.p[0];
    let t_in = unit(g.in_dirs[p]).unwrap_or([0.0, 0.0, 1.0]);
    // sample the curve by arc length
    let fine: Vec<[f64; 3]> = (0..=64).map(|k| curve.at(f64::from(k) / 64.0)).collect();
    let mut arc = vec![0.0];
    for w in fine.windows(2) {
        arc.push(arc[arc.len() - 1] + dist(w[0], w[1]));
    }
    let length = arc[64];
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (libm::ceil(length / style.step_mm) as usize).max(4);
    // the overlap into the parent: straight back along its tangent
    let overlap = if p == 0 {
        0.0
    } else {
        (0.25 * r_start).max(0.15).min(0.35 * length)
    };
    let mut rings: Vec<([f64; 3], [f64; 3], f64, f64)> = Vec::new(); // centre, tangent, s, radius
    if overlap > 0.0 {
        rings.push((sub(p0, scale(t_in, overlap)), t_in, -overlap, r_start));
    }
    #[allow(clippy::cast_precision_loss)]
    for k in 0..=steps {
        let t = k as f64 / steps as f64;
        let c = curve.at(t);
        let tan = unit(curve.tangent(t)).unwrap_or(t_in);
        let s = interp_arc(&arc, t);
        rings.push((
            c,
            tan,
            s,
            r_start + (r_end - r_start) * (s / length.max(1e-9)).clamp(0.0, 1.0),
        ));
    }
    let angles: Vec<f64> = (0..n_sides)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let a = 2.0 * std::f64::consts::PI * k as f64 / n_sides as f64;
            a
        })
        .collect();
    let (mut u, mut v) = frame;
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut radii = vec![0.0; n_sides];
    for (k, (c, tan, s, r)) in rings.iter().enumerate() {
        if k > 0 {
            (u, v) = transport(u, *tan);
        }
        radii.iter_mut().for_each(|x| *x = *r);
        surface(
            &Ring {
                edge: i,
                s: *s,
                length,
                centre: *c,
                radius: *r,
            },
            &angles,
            &mut radii,
        );
        for (a, rr) in angles.iter().zip(&radii) {
            let (ca, sa) = (libm::cos(*a), libm::sin(*a));
            positions.push([0, 1, 2].map(|d| c[d] + rr * (ca * u[d] + sa * v[d])));
        }
    }
    let nr = rings.len();
    let idx = |r: usize, k: usize| u32::try_from(r * n_sides + k % n_sides).unwrap_or(u32::MAX);
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for r in 0..nr - 1 {
        for k in 0..n_sides {
            tris.push([idx(r, k), idx(r, k + 1), idx(r + 1, k + 1)]);
            tris.push([idx(r, k), idx(r + 1, k + 1), idx(r + 1, k)]);
        }
    }
    // start cap
    let cs = u32::try_from(positions.len()).unwrap_or(u32::MAX);
    positions.push(rings[0].0);
    for k in 0..n_sides {
        tris.push([cs, idx(0, k + 1), idx(0, k)]);
    }
    let last = nr - 1;
    let (tip_c, tip_t, _, tip_r) = rings[last];
    if end {
        // a hemisphere dome on the end ring
        let mut prev: Vec<u32> = (0..n_sides).map(|k| idx(last, k)).collect();
        for lat in 1..=style.dome_rings {
            #[allow(clippy::cast_precision_loss)]
            let phi = std::f64::consts::FRAC_PI_2 * lat as f64 / style.dome_rings as f64;
            let centre = add(tip_c, scale(tip_t, tip_r * libm::sin(phi)));
            if lat < style.dome_rings {
                let rr = tip_r * libm::cos(phi);
                let base = positions.len();
                for a in &angles {
                    let (ca, sa) = (libm::cos(*a), libm::sin(*a));
                    positions.push([0, 1, 2].map(|d| centre[d] + rr * (ca * u[d] + sa * v[d])));
                }
                let cur: Vec<u32> = (0..n_sides)
                    .map(|k| u32::try_from(base + k).unwrap_or(u32::MAX))
                    .collect();
                for k in 0..n_sides {
                    let k1 = (k + 1) % n_sides;
                    tris.push([prev[k], prev[k1], cur[k1]]);
                    tris.push([prev[k], cur[k1], cur[k]]);
                }
                prev = cur;
            } else {
                let pole = u32::try_from(positions.len()).unwrap_or(u32::MAX);
                positions.push(centre);
                for k in 0..n_sides {
                    tris.push([pole, prev[k], prev[(k + 1) % n_sides]]);
                }
            }
        }
    } else {
        let ce = u32::try_from(positions.len()).unwrap_or(u32::MAX);
        positions.push(tip_c);
        for k in 0..n_sides {
            tris.push([ce, idx(last, k), idx(last, k + 1)]);
        }
    }
    let mut mesh = TriMesh {
        positions,
        triangles: tris,
        ..TriMesh::default()
    };
    if crate::signed_volume(&mesh) < 0.0 {
        mesh.triangles.iter_mut().for_each(|t| t.swap(1, 2));
    }
    (mesh, (u, v))
}

fn interp_arc(arc: &[f64], t: f64) -> f64 {
    let x = t * 64.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let k = (x.floor() as usize).min(63);
    #[allow(clippy::cast_precision_loss)]
    let f = x - k as f64;
    arc[k] + f * (arc[k + 1] - arc[k])
}

/// A frame round `w` (`_basis`).
#[must_use]
pub fn basis(w: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let r = if dot(w, [0.0, 0.0, 1.0]).abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = unit(cross(w, r)).unwrap_or([1.0, 0.0, 0.0]);
    let v = unit(cross(w, u)).unwrap_or([0.0, 1.0, 0.0]);
    (u, v)
}

/// Parallel transport of a frame onto a new tangent (`_transport`).
fn transport(u: [f64; 3], t: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let un = sub(u, scale(t, dot(u, t)));
    match unit(un) {
        Some(un) => (un, unit(cross(t, un)).unwrap_or([0.0, 1.0, 0.0])),
        None => basis(t),
    }
}
