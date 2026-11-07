//! Graphs: the P5 operations for branching things (trees; later roots and
//! vines). [`colonise`] traces a branching graph from a root to target
//! points by space colonisation, with the tree invariants held by
//! construction; [`simplify`] keeps its forks and ends; [`pipe_radii`]
//! sizes it bottom-up; [`tubes`] lofts a closed, C1-continuous tube along
//! every edge.
//!
//! The skeleton grows from the root to the leaf targets, is simplified,
//! and gets radii bottom-up; the tube half builds each closed edge solid
//! (with the Bézier and Bishop-frame helpers).
//!
//! Invariants of a traced graph (docs/elements/trees.html, S1–S4), each by
//! construction, not by a check after the fact:
//!
//! - S1: every target is an end (a node with no children), and only
//!   targets are ends;
//! - S2: every branch lands exactly on its target;
//! - S3: forks happen only at synthetic in-between nodes;
//! - S4: no edge rises at less than the floor angle above horizontal. A
//!   step that would is turned up onto the floor cone; a target that
//!   cannot be reached at the floor from the branch that owns it is moved
//!   onto the cone (the nearest point), and counted. [`tubes`] then keeps
//!   every point of every edge's curve at or above the floor, shortening
//!   the C1 handles of an edge where they would dip.

use dt_geom::TriMesh;

mod colonise;
mod tube;

pub use colonise::{Colonise, Traced, colonise};
pub use tube::{Ring, Surface, TubeStyle, basis, tubes};

/// Rounds halves to even.
#[must_use]
pub fn round_half_even(x: f64) -> f64 {
    let r = libm::round(x);
    if ((x - libm::trunc(x)).abs() - 0.5).abs() < 1e-12 {
        2.0 * libm::round(x / 2.0)
    } else {
        r
    }
}

/// A traced graph with only its root, forks and ends (`_simplify_skeleton`),
/// in depth-first order (every parent before its children).
#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    /// Node positions; node 0 is the root.
    pub nodes: Vec<[f64; 3]>,
    /// Each node's parent (`None` for the root).
    pub parents: Vec<Option<usize>>,
    /// The direction arriving at each node (the end tangent of its edge).
    pub in_dirs: Vec<[f64; 3]>,
    /// The direction leaving the parent toward each node.
    pub out_dirs: Vec<[f64; 3]>,
    /// Radii, mm (see [`pipe_radii`]).
    pub radii: Vec<f64>,
    /// For each end node, the target it is (`None` elsewhere).
    pub target: Vec<Option<usize>>,
}

impl Graph {
    /// Each node's children, in order.
    #[must_use]
    pub fn children(&self) -> Vec<Vec<usize>> {
        let mut ch = vec![Vec::new(); self.nodes.len()];
        for (i, p) in self.parents.iter().enumerate() {
            if let Some(p) = p {
                ch[*p].push(i);
            }
        }
        ch
    }

    /// The elevation (degrees above horizontal) of every edge's chord.
    #[must_use]
    pub fn elevations(&self) -> Vec<f64> {
        (0..self.nodes.len())
            .filter_map(|i| {
                let p = self.parents[i]?;
                let e = sub(self.nodes[i], self.nodes[p]);
                let l = norm(e);
                (l > 1e-9).then(|| libm::asin((e[2] / l).clamp(-1.0, 1.0)).to_degrees())
            })
            .collect()
    }
}

/// **simplify**: drop the single-child steps of a traced graph; radii are
/// left at zero for [`pipe_radii`].
#[must_use]
pub fn simplify(t: &Traced) -> Graph {
    let n = t.nodes.len();
    let mut ch = vec![Vec::new(); n];
    for i in 1..n {
        ch[t.parents[i]].push(i);
    }
    let mut end_of = vec![None; n];
    for (target, &node) in t.ends.iter().enumerate() {
        if node < n {
            end_of[node] = Some(target);
        }
    }
    let significant = |i: usize| i == 0 || ch[i].len() != 1;
    let mut g = Graph {
        nodes: Vec::new(),
        parents: Vec::new(),
        in_dirs: Vec::new(),
        out_dirs: Vec::new(),
        radii: Vec::new(),
        target: Vec::new(),
    };
    // depth first, children in order (an explicit stack, reversed pushes)
    let mut stack = vec![(0usize, None::<usize>, t.dirs[0])];
    while let Some((old, parent, out)) = stack.pop() {
        let new = g.nodes.len();
        g.nodes.push(t.nodes[old]);
        g.parents.push(parent);
        g.in_dirs.push(t.dirs[old]);
        g.out_dirs.push(out);
        g.radii.push(0.0);
        g.target.push(end_of[old]);
        for &c in ch[old].iter().rev() {
            let first = t.dirs[c];
            let mut cur = c;
            while !significant(cur) {
                cur = ch[cur][0];
            }
            stack.push((cur, Some(new), first));
        }
    }
    g
}

/// **pipe radii**: ends get `tip_mm`; every other node
/// `(Σ r_child^e)^(1/e)` (the pipe model; the root is derived).
pub fn pipe_radii(g: &mut Graph, exponent: f64, tip_mm: f64) {
    let ch = g.children();
    for i in (0..g.nodes.len()).rev() {
        g.radii[i] = if ch[i].is_empty() {
            tip_mm
        } else {
            let s: f64 = ch[i].iter().map(|&c| libm::pow(g.radii[c], exponent)).sum();
            libm::pow(s, 1.0 / exponent)
        };
    }
}

/// One edge's cubic Bézier: from its parent node to its node, with C1
/// handles along the arriving directions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    /// Control points.
    pub p: [[f64; 3]; 4],
}

impl Curve {
    /// The point at `t` in 0..1.
    #[must_use]
    pub fn at(&self, t: f64) -> [f64; 3] {
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        [0, 1, 2].map(|k| (0..4).map(|j| w[j] * self.p[j][k]).sum())
    }

    /// The (unnormalised) tangent at `t`.
    #[must_use]
    pub fn tangent(&self, t: f64) -> [f64; 3] {
        let u = 1.0 - t;
        let p = &self.p;
        [0, 1, 2].map(|k| {
            3.0 * u * u * (p[1][k] - p[0][k])
                + 6.0 * u * t * (p[2][k] - p[1][k])
                + 3.0 * t * t * (p[3][k] - p[2][k])
        })
    }

    /// The lowest elevation of its tangent, degrees, sampled finely.
    #[must_use]
    pub fn min_elevation(&self) -> f64 {
        (0..=64)
            .filter_map(|k| unit(self.tangent(f64::from(k) / 64.0)))
            .map(|d| libm::asin(d[2].clamp(-1.0, 1.0)).to_degrees())
            .fold(90.0, f64::min)
    }
}

/// The C1 curve of every edge (`None` for the root and zero-length
/// edges), handles `handle` × the chord, each edge's handles shortened
/// (halved until it holds) where its curve would dip below `floor_deg`;
/// and how many were shortened.
#[must_use]
pub fn curves(g: &Graph, handle: f64, floor_deg: f64) -> (Vec<Option<Curve>>, usize) {
    let mut shortened = 0;
    let out = (0..g.nodes.len())
        .map(|i| {
            let p = g.parents[i]?;
            let (p0, p3) = (g.nodes[p], g.nodes[i]);
            let len = dist(p0, p3);
            if len < 1e-8 {
                return None;
            }
            let (t0, t3) = (
                unit(g.in_dirs[p]).unwrap_or([0.0, 0.0, 1.0]),
                unit(g.in_dirs[i]).unwrap_or([0.0, 0.0, 1.0]),
            );
            let make = |h: f64| Curve {
                p: [p0, add(p0, scale(t0, h)), sub(p3, scale(t3, h)), p3],
            };
            let mut h = handle * len;
            let mut c = make(h);
            let mut tries = 0;
            while c.min_elevation() < floor_deg && tries < 12 {
                h *= 0.5;
                c = make(h);
                tries += 1;
            }
            if tries > 0 {
                shortened += 1;
                if c.min_elevation() < floor_deg {
                    c = make(0.0);
                }
            }
            Some(c)
        })
        .collect();
    (out, shortened)
}

// --- small vector helpers (public: elements use them for their own frames) ---

pub use dt_geom::vec3::{add, cross, dist, dot, norm, scale, sub};

/// a / |a|, or `None` for a (near) zero vector.
#[must_use]
pub fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = norm(a);
    (n > 1e-12).then(|| scale(a, 1.0 / n))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::needless_range_loop)]
    use super::*;

    fn cloud(seed: u64, n: usize) -> Vec<[f64; 3]> {
        let mut rng = dt_geom::Rng::new(dt_geom::Seed::new(seed));
        (0..n)
            .map(|_| {
                let a = rng.uniform(0.0, std::f64::consts::TAU);
                let r = rng.uniform(4.0, 12.0);
                [r * libm::cos(a), r * libm::sin(a), rng.uniform(24.0, 40.0)]
            })
            .collect()
    }

    fn params() -> Colonise {
        Colonise {
            step_mm: 1.0,
            split_cos: 0.8 * libm::cos(30f64.to_radians()),
            aim_sin: libm::sin(35f64.to_radians()),
            floor_deg: 26.0,
            climb_deg: 26.0,
            max_branches: 3,
            alpha: 0.1,
            max_steps: 160,
            aim: 0.33,
            balance: 1.0,
        }
    }

    #[test]
    fn invariants_hold_over_many_seeds() {
        for seed in 1..=60 {
            let pts = cloud(seed, 40);
            let groups: Vec<usize> = (0..pts.len()).map(|i| i % 4).collect();
            let gr = if seed % 2 == 0 {
                Some(groups.as_slice())
            } else {
                None
            };
            let t = colonise([0.0, 0.0, 0.0], &pts, gr, &params());
            let g = simplify(&t);
            let ch = g.children();
            // S1, S2: every target an end, at its (possibly moved) position
            let ends: Vec<usize> = (0..g.nodes.len()).filter(|&i| ch[i].is_empty()).collect();
            assert_eq!(ends.len(), pts.len(), "seed {seed}");
            for &e in &ends {
                let k = g.target[e].unwrap();
                assert!(dist(g.nodes[e], t.targets[k]) < 1e-12);
            }
            // S3: no target is a fork or in-between node
            for i in 0..g.nodes.len() {
                if !ch[i].is_empty() {
                    assert!(g.target[i].is_none());
                }
            }
            // S4: every chord, and every curve, at or above the floor
            let min = g.elevations().into_iter().fold(90.0, f64::min);
            assert!(min >= 26.0, "seed {seed}: {min}");
            let (cv, _) = curves(&g, 0.45, 26.0);
            for c in cv.iter().flatten() {
                assert!(c.min_elevation() >= 26.0 - 1e-9);
            }
        }
    }

    #[test]
    fn a_target_below_the_floor_is_moved_onto_it() {
        let pts = [[10.0, 0.0, 1.0], [0.0, 0.0, 20.0]];
        let t = colonise([0.0, 0.0, 0.0], &pts, None, &params());
        assert!(t.moved >= 1);
        let g = simplify(&t);
        assert!(g.elevations().into_iter().all(|e| e >= 26.0));
    }

    #[test]
    fn pipe_radii_follow_the_pipe_model() {
        let pts = cloud(3, 20);
        let mut g = simplify(&colonise([0.0; 3], &pts, None, &params()));
        pipe_radii(&mut g, 3.0, 1.0);
        let ch = g.children();
        for i in 0..g.nodes.len() {
            if ch[i].is_empty() {
                assert!((g.radii[i] - 1.0).abs() < 1e-12);
            } else {
                let s: f64 = ch[i].iter().map(|&c| g.radii[c].powi(3)).sum();
                assert!((g.radii[i] - s.cbrt()).abs() < 1e-9);
            }
        }
        // 20 ends of radius 1 under exponent 3: the root is 20^(1/3)
        assert!((g.radii[0] - 20f64.cbrt()).abs() < 1e-9);
    }

    #[test]
    fn tubes_are_closed() {
        let pts = cloud(5, 16);
        let mut g = simplify(&colonise([0.0; 3], &pts, None, &params()));
        pipe_radii(&mut g, 3.0, 1.0);
        let (cv, _) = curves(&g, 0.45, 26.0);
        let style = TubeStyle {
            step_mm: 0.9,
            min_radius_mm: 0.42,
            dome_rings: 4,
        };
        let ts = tubes(&g, &cv, &style, &|_| 12, &|_, _, _| {});
        for m in ts.iter().filter(|m| !m.triangles.is_empty()) {
            assert!(m.is_watertight());
            assert!(crate::signed_volume(m) > 0.0);
        }
    }
}
