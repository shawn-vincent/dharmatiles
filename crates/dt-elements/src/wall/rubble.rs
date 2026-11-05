//! Rough stones and plain hulls: the rubble chip, the rounded box of the mortar cores (`rounded_box`), and
//! plain boxes and prisms.

use dt_kit::form::hull;
use dt_kit::{KitError, Rng, TriMesh};

/// `fibonacci_sphere`: `n` evenly spread unit directions.
pub(crate) fn fibonacci_sphere(n: usize) -> Vec<[f64; 3]> {
    #[allow(clippy::cast_precision_loss)]
    let nf = n as f64;
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let i = i as f64 + 0.5;
            let phi = libm::acos(1.0 - 2.0 * i / nf);
            let theta = core::f64::consts::PI * (1.0 + libm::sqrt(5.0)) * i;
            [
                libm::sin(phi) * libm::cos(theta),
                libm::sin(phi) * libm::sin(theta),
                libm::cos(phi),
            ]
        })
        .collect()
}

/// `rubble_stone`: a cheap rough hull chip in `[0, l]³` — strongly
/// jittered directions, an ellipsoid↔box radial blend, heavy lump
/// variance, crisp facets (smooth rubble reads as mortar: E26).
pub(crate) fn rubble_points(l: [f64; 3], rng: &mut Rng) -> Vec<[f64; 3]> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let m = rng.int(10, 14) as usize;
    let squash = rng.uniform(0.5, 0.8);
    let half = l.map(|v| v / 2.0);
    let blocky = rng.uniform(0.15, 0.50);
    fibonacci_sphere(m)
        .into_iter()
        .map(|d0| {
            let mut d = d0.map(|v| v + rng.normal(0.0, 0.30));
            d[1] *= squash;
            let len = libm::sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) + 1e-12;
            let d = d.map(|v| v / len);
            let ell = 1.0 / libm::sqrt((0..3).map(|k| (d[k] / half[k]).powi(2)).sum::<f64>());
            let bx = 1.0 / (0..3).map(|k| d[k].abs() / half[k]).fold(0.0, f64::max);
            let r = ((1.0 - blocky) * ell + blocky * bx) * rng.uniform(0.70, 1.05);
            [0, 1, 2].map(|k| d[k] * r + half[k])
        })
        .collect()
}

/// A rubble chip stretched to exactly fill the box `[lo, hi]`.
pub(crate) fn rubble(lo: [f64; 3], hi: [f64; 3], rng: &mut Rng) -> Result<TriMesh, KitError> {
    let l = [0, 1, 2].map(|k| hi[k] - lo[k]);
    let mut pts = rubble_points(l, rng);
    let (mut b0, mut b1) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in &pts {
        for k in 0..3 {
            b0[k] = b0[k].min(p[k]);
            b1[k] = b1[k].max(p[k]);
        }
    }
    for p in &mut pts {
        *p = [0, 1, 2].map(|k| lo[k] + (p[k] - b0[k]) * l[k] / (b1[k] - b0[k]).max(1e-9));
    }
    hull(&pts)
}

/// `rounded_box`: the box `[lo, hi]` with every edge filleted at `r` (the
/// hull of spheres at the eight inset corners).
pub(crate) fn rounded_box(lo: [f64; 3], hi: [f64; 3], r: f64) -> Result<TriMesh, KitError> {
    let ex = [0, 1, 2].map(|k| hi[k] - lo[k]);
    let r = r.min(0.45 * ex.iter().copied().fold(f64::INFINITY, f64::min));
    if r <= 1e-6 {
        return boxed(lo, hi);
    }
    let dirs = fibonacci_sphere(64);
    let mut pts = Vec::with_capacity(8 * dirs.len());
    for c in 0..8 {
        let corner = [0, 1, 2].map(|k| {
            if c >> k & 1 == 1 {
                hi[k] - r
            } else {
                lo[k] + r
            }
        });
        pts.extend(dirs.iter().map(|d| [0, 1, 2].map(|k| corner[k] + r * d[k])));
    }
    hull(&pts)
}

/// A plain box.
pub(crate) fn boxed(lo: [f64; 3], hi: [f64; 3]) -> Result<TriMesh, KitError> {
    let pts: Vec<[f64; 3]> = (0..8)
        .map(|c| [0, 1, 2].map(|k| if c >> k & 1 == 1 { hi[k] } else { lo[k] }))
        .collect();
    hull(&pts)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn chips_fill_their_box_and_are_closed() {
        let mut rng = Rng::new(dt_kit::Seed::new(4));
        for _ in 0..20 {
            let m = rubble([1.0, 2.0, 3.0], [5.0, 4.5, 6.5], &mut rng).unwrap();
            assert!(m.is_watertight());
            let lo = m
                .positions
                .iter()
                .map(|p| p[0])
                .fold(f64::INFINITY, f64::min);
            assert!((lo - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn a_rounded_box_is_close_to_its_box() {
        let m = rounded_box([0.0; 3], [10.0, 4.0, 6.0], 0.6).unwrap();
        let v = dt_kit::signed_volume(&m);
        assert!(v < 240.0 && v > 225.0, "{v}");
    }
}
