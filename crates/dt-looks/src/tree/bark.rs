//! Bark: V grooves that start at the root, wave and twist up the trunk and
//! carry on through every fork, tapering out where a line does not
//! continue (`trees/bark.py`, the bark helpers of `trees/mesh.py`), with
//! a fine roughness between them. True mm: a print-scale detail.

use dt_kit::graph::{Graph, Ring};

const SPACING_MM: f64 = 2.25;
const DEPTH_MM: f64 = 0.80;
const WIDTH_MM: f64 = 0.72;
const ROUGH_MM: f64 = 0.05;
const ROUGH_CELL_MM: f64 = 0.90;
const WAVE_MM: f64 = 0.22;
const WAVE_LENGTH_MM: f64 = 7.5;
const TWIST_TURNS: f64 = 1.25;
const MIN_BRANCH_RADIUS_MM: f64 = 0.58;
/// Ring spacing on a barked tube (the roughness cell).
pub const STEP_MM: f64 = ROUGH_CELL_MM;
/// Sides of a barked ring, and of a bare one.
pub const SIDES: usize = 48;
pub const BARE_SIDES: usize = 12;

#[derive(Clone, Copy, Debug)]
struct Line {
    id: usize,
    phase: f64,
    theta: f64,
}

/// Every edge's grooves: which lines run along it, and which of them
/// taper out at its end.
pub struct Bark {
    seed: u64,
    lines: Vec<Vec<Line>>,
    taper: Vec<Vec<usize>>,
    base_z: f64,
    height: f64,
}

/// A deterministic 0..1 from integer parts (FNV-1a with a murmur3
/// finaliser, as `_hash01`).
pub fn hash01(parts: &[u64]) -> f64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h ^= 0xFF;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    h ^= h >> 33;
    #[allow(clippy::cast_precision_loss)]
    let v = (h >> 11) as f64 / (1u64 << 53) as f64;
    v
}

const TAU: f64 = std::f64::consts::TAU;

fn wrap(a: f64) -> f64 {
    a.rem_euclid(TAU)
}

fn wrap_signed(a: f64) -> f64 {
    (a + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI
}

fn theta_at(l: &Line, s: f64, radius: f64) -> f64 {
    let amp = WAVE_MM.min(0.25 * SPACING_MM);
    l.theta + amp / radius.max(1e-6) * libm::sin(TAU * s / WAVE_LENGTH_MM + l.phase)
}

fn select(parent: &[Line], radius: f64) -> Vec<Line> {
    if parent.is_empty() || radius < MIN_BRANCH_RADIUS_MM {
        return Vec::new();
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let want = libm::floor(TAU * radius / SPACING_MM) as usize;
    if want >= parent.len() {
        return parent.to_vec();
    }
    let mut ordered = parent.to_vec();
    ordered.sort_by(|a, b| wrap(a.theta).total_cmp(&wrap(b.theta)));
    let mut picked: Vec<Line> = Vec::new();
    for k in 0..want {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let i = libm::round(k as f64 * ordered.len() as f64 / want as f64) as usize % ordered.len();
        if !picked.iter().any(|l| l.id == ordered[i].id) {
            picked.push(ordered[i]);
        }
    }
    for l in &ordered {
        if picked.len() >= want {
            break;
        }
        if !picked.iter().any(|p| p.id == l.id) {
            picked.push(*l);
        }
    }
    picked
}

impl Bark {
    /// The grooves of every edge of `g` (radii set), seeded by `seed`.
    pub fn new(g: &Graph, seed: u64, base_z: f64, height: f64) -> Self {
        let n = g.nodes.len();
        let ch = g.children();
        let mut node: Vec<Vec<Line>> = vec![Vec::new(); n];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n_root = (libm::floor(TAU * g.radii[0] / SPACING_MM) as usize).max(5);
        #[allow(clippy::cast_precision_loss)]
        {
            node[0] = (0..n_root)
                .map(|i| Line {
                    id: i,
                    phase: TAU * hash01(&[seed, 1, i as u64]),
                    theta: TAU * i as f64 / n_root as f64,
                })
                .collect();
        }
        let mut lines = vec![Vec::new(); n];
        let mut taper = vec![Vec::new(); n];
        // nodes are in depth-first order: every parent before its children
        for i in 1..n {
            let Some(p) = g.parents[i] else { continue };
            let r0 = g.radii[p].max(0.42);
            let r1 = g.radii[i].max(0.42);
            let len = dt_kit::graph::dist(g.nodes[i], g.nodes[p]);
            let edge = select(&node[p], r0);
            let end: Vec<Line> = edge
                .iter()
                .map(|l| Line {
                    theta: wrap(theta_at(l, len, r1)),
                    ..*l
                })
                .collect();
            let mut going: Vec<usize> = Vec::new();
            if !ch[i].is_empty() {
                going = select(&end, r1).iter().map(|l| l.id).collect();
            }
            taper[i] = edge
                .iter()
                .map(|l| l.id)
                .filter(|id| !going.contains(id))
                .collect();
            lines[i] = edge;
            node[i] = end;
        }
        Self {
            seed,
            lines,
            taper,
            base_z,
            height: height.max(1e-6),
        }
    }

    /// Whether edge `i` carries grooves.
    pub fn barked(&self, i: usize) -> bool {
        !self.lines[i].is_empty()
    }

    /// The groove centres on one ring: (line, angle, strength).
    fn centres(&self, ring: &Ring) -> Vec<(usize, f64, f64)> {
        let i = ring.edge;
        if ring.radius < MIN_BRANCH_RADIUS_MM {
            return Vec::new();
        }
        let twist =
            TAU * TWIST_TURNS * ((ring.centre[2] - self.base_z) / self.height).clamp(0.0, 1.0);
        let s = ring.s.max(0.0);
        let mut c: Vec<(usize, f64, f64)> = self.lines[i]
            .iter()
            .filter_map(|l| {
                let mut strength = 1.0;
                if self.taper[i].contains(&l.id) {
                    let len = (WIDTH_MM * 3.0)
                        .max(1.0)
                        .min((ring.length * 0.45).max(1e-6));
                    let x = ((ring.length - s) / len).clamp(0.0, 1.0);
                    strength = x * x * (3.0 - 2.0 * x);
                }
                (strength > 1e-6)
                    .then(|| (l.id, wrap(theta_at(l, s, ring.radius) + twist), strength))
            })
            .collect();
        // drop the lower-priority of the closest pair until all clear
        c.sort_by(|a, b| a.1.total_cmp(&b.1));
        while c.len() > 1 {
            let n = c.len();
            let (k, gap) = (0..n)
                .map(|k| (k, wrap(c[(k + 1) % n].1 - c[k].1) * ring.radius))
                .fold((0, f64::INFINITY), |b, x| if x.1 < b.1 { x } else { b });
            if gap >= WIDTH_MM * 1.25 {
                break;
            }
            let (a, b) = (c[k].0, c[(k + 1) % n].0);
            let pa = hash01(&[self.seed, 2, i as u64, a as u64]);
            let pb = hash01(&[self.seed, 2, i as u64, b as u64]);
            let drop = if pa < pb { a } else { b };
            c.retain(|x| x.0 != drop);
        }
        c
    }

    /// Set the radius of each side of `ring` (`_make_ring` with bark).
    pub fn surface(&self, ring: &Ring, angles: &[f64], radii: &mut [f64]) {
        if !self.barked(ring.edge) {
            return;
        }
        let centres = self.centres(ring);
        if centres.is_empty() {
            return;
        }
        let r = ring.radius;
        let amp = ROUGH_MM.min(0.45 * DEPTH_MM);
        #[allow(clippy::cast_possible_truncation)]
        let s_cell = libm::floor(ring.s / ROUGH_CELL_MM) as i64;
        let (lo, hi) = ((r - 0.35 * r).max(0.42), r + amp);
        for (a, out) in angles.iter().zip(radii.iter_mut()) {
            let mut cut: f64 = 0.0;
            for &(_, g, st) in &centres {
                let hw = 0.5 * WIDTH_MM * st;
                if hw <= 1e-9 {
                    continue;
                }
                let d = r * wrap_signed(a - g).abs();
                if d < hw {
                    cut = cut.max(DEPTH_MM * st * (1.0 - d / hw));
                }
            }
            let mut rough = 0.0;
            if cut <= 1e-9 {
                #[allow(clippy::cast_possible_truncation)]
                let t_cell = libm::floor(r * wrap(*a) / ROUGH_CELL_MM) as i64;
                #[allow(clippy::cast_sign_loss)]
                let key = [self.seed, 3, ring.edge as u64, s_cell as u64, t_cell as u64];
                let u1 = hash01(&key).max(1e-12);
                let mut k2 = key;
                k2[1] = 4;
                let u2 = hash01(&k2);
                let n = libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(TAU * u2);
                rough = n.clamp(-2.0, 2.0) * amp;
            }
            *out = (r - cut + rough).clamp(lo, hi);
        }
    }
}
