//! Spans: the one shared way to map a field into a band.

use dt_geom::Mask;

use crate::{Band, Banded, Field};

/// **into_band**: map any noise field into a band so that both ends are
/// reached. The noise's least and greatest value over `within` become 0 and
/// 1, and the surface at every node is `lo + t x (hi - lo)`: a body's hills
/// span the band from its low to its high edge wherever the band has
/// room, and sit on it where it has none. Nodes outside `within` use the
/// same map, clamped into the band.
///
/// A constant noise (nothing to span) sits in the middle of the band.
///
/// # Panics
/// If the noise is on another lattice than the band.
#[must_use]
pub fn into_band(noise: &Field, band: &Band, within: &Mask) -> Banded {
    assert!(
        noise.nx == within.nx && noise.ny == within.ny,
        "into_band: another lattice"
    );
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (&v, &m) in noise.z.iter().zip(&within.bits) {
        if m && v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    let span = hi - lo;
    let raw = Field {
        nx: noise.nx,
        ny: noise.ny,
        step: noise.step,
        z: (0..noise.z.len())
            .map(|k| {
                let t = if span > 0.0 {
                    (noise.z[k] - lo) / span
                } else {
                    0.5
                };
                let (l, h) = (band.lo().z[k], band.hi().z[k]);
                l + t * (h - l)
            })
            .collect(),
    };
    band.fit(&raw)
}

/// How coarse the relaxation lattice is, mm at the reference square: wide
/// enough to be quick, fine enough to follow a bank a few mm across.
const RELAX_STEP_MM: f64 = 1.0;
/// How many sweeps the relaxation makes on its coarse lattice.
const RELAX_SWEEPS: usize = 500;

/// **relax**: the smoothest surface that stays inside the band. It starts
/// at the band's middle and, over a coarse lattice, sweeps each node to the
/// mean of its four neighbours and back into its band; where the band is
/// wide (a bank drawn between two places) the surface runs smoothly from
/// one neighbour to the other, and where it is thin it sits on the band.
/// The coarse answer is carried back to the lattice, bilinear, and held
/// inside the band there.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn relax(band: &Band) -> Field {
    let (lo, hi) = (band.lo(), band.hi());
    let (nx, ny) = (lo.nx, lo.ny);
    // coarse nodes at every `c`-th fine node, and the last one
    let c = ((RELAX_STEP_MM / lo.step).round() as usize).max(1);
    let axis = |n: usize| -> Vec<usize> {
        let mut v: Vec<usize> = (0..n).step_by(c).collect();
        if v.last() != Some(&(n - 1)) {
            v.push(n - 1);
        }
        v
    };
    let (xs, ys) = (axis(nx), axis(ny));
    let (cx, cy) = (xs.len(), ys.len());
    let at = |i: usize, j: usize| ys[j] * nx + xs[i];
    let (clo, chi): (Vec<f64>, Vec<f64>) = (0..cx * cy)
        .map(|q| {
            let k = at(q % cx, q / cx);
            (lo.z[k], hi.z[k])
        })
        .unzip();
    let mut s: Vec<f64> = clo.iter().zip(&chi).map(|(a, b)| 0.5 * (a + b)).collect();
    let mut next = s.clone();
    for _ in 0..RELAX_SWEEPS {
        for j in 0..cy {
            for i in 0..cx {
                let q = j * cx + i;
                let l = s[if i > 0 { q - 1 } else { q }];
                let r = s[if i + 1 < cx { q + 1 } else { q }];
                let d = s[if j > 0 { q - cx } else { q }];
                let u = s[if j + 1 < cy { q + cx } else { q }];
                next[q] = (0.25 * (l + r + d + u)).clamp(clo[q], chi[q]);
            }
        }
        std::mem::swap(&mut s, &mut next);
    }
    // bilinear back to the lattice
    let locate = |v: &[usize], n: usize| -> Vec<(usize, f64)> {
        let mut out = Vec::with_capacity(n);
        let mut a = 0;
        for f in 0..n {
            while a + 2 < v.len() && v[a + 1] <= f {
                a += 1;
            }
            let (p, q) = (v[a], v[(a + 1).min(v.len() - 1)]);
            let t = if q > p {
                (f as f64 - p as f64) / (q - p) as f64
            } else {
                0.0
            };
            out.push((a, t.clamp(0.0, 1.0)));
        }
        out
    };
    let (ax, ay) = (locate(&xs, nx), locate(&ys, ny));
    let mut z = Vec::with_capacity(nx * ny);
    for (j, &(b, v)) in ay.iter().enumerate() {
        for (i, &(a, u)) in ax.iter().enumerate() {
            let g = |ii: usize, jj: usize| s[jj.min(cy - 1) * cx + ii.min(cx - 1)];
            let top = g(a, b) * (1.0 - u) + g(a + 1, b) * u;
            let bot = g(a, b + 1) * (1.0 - u) + g(a + 1, b + 1) * u;
            let k = j * nx + i;
            z.push((top * (1.0 - v) + bot * v).clamp(lo.z[k], hi.z[k]));
        }
    }
    lo.with_z(z)
}

/// A band thinner than this is a pin, mm.
const THIN_MM: f64 = 0.02;

/// **hills**: a body's hills in its band. The surface is the smoothest one
/// in the band ([`relax`]) plus the noise, spread over as much of the band
/// as there is room for: `hill` mm at most, and less near a pin, growing
/// from it no steeper than `slope_deg`, so a bank drawn between two places
/// stays a bank and a flat top gets hills that reach both its edges.
/// The noise is scaled by its least and greatest value over the nodes where
/// the band has room.
///
/// # Panics
/// If the noise is on another lattice than the band.
#[must_use]
#[allow(clippy::needless_range_loop)]
pub fn hills(noise: &Field, band: &Band, hill_mm: f64, slope_deg: f64) -> Banded {
    assert!(
        noise.nx == band.lo().nx && noise.ny == band.lo().ny,
        "hills: another lattice"
    );
    let (lo, hi) = (band.lo(), band.hi());
    let n = lo.z.len();
    let room: Vec<bool> = (0..n).map(|k| hi.z[k] - lo.z[k] >= THIN_MM).collect();
    let thin: Vec<bool> = room.iter().map(|r| !r).collect();
    // distance to the nearest pin, mm
    let to_pin: Vec<f64> = if thin.iter().any(|&b| b) {
        let (d, _) = dt_geom::distance_nearest(&thin, lo.nx, lo.ny);
        d.into_iter().map(|v| v * lo.step).collect()
    } else {
        vec![f64::INFINITY; n]
    };
    let (mut nlo, mut nhi) = (f64::INFINITY, f64::NEG_INFINITY);
    for k in 0..n {
        if room[k] && noise.z[k].is_finite() {
            nlo = nlo.min(noise.z[k]);
            nhi = nhi.max(noise.z[k]);
        }
    }
    let span = nhi - nlo;
    let smooth = relax(band);
    let reach = 2.0 * libm::tan(slope_deg.to_radians());
    let raw = lo.with_z(
        (0..n)
            .map(|k| {
                let t = if span > 0.0 {
                    (noise.z[k] - nlo) / span
                } else {
                    0.5
                };
                let room_mm = (hi.z[k] - lo.z[k]).min(hill_mm).min(reach * to_pin[k]);
                smooth.z[k] + (t - 0.5) * room_mm
            })
            .collect(),
    );
    band.fit(&raw)
}
