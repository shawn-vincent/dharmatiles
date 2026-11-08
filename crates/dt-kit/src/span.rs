//! Spans: the one shared way to map a field into a band.

use crate::{Band, Banded, Field};

/// How coarse the relaxation lattice is, mm at the 35 mm square (it scales
/// with the system, like every size): wide enough to be quick, fine enough
/// to follow a bank a few mm across.
const RELAX_STEP_MM: f64 = 1.0;
/// How many rounds of the active-set solve at most.
const RELAX_ROUNDS: usize = 40;
/// How many conjugate-gradient steps one round takes at most.
const RELAX_CG: usize = 3000;
/// The active-set rule's weight on a node's violation against the plate's
/// pressure on it: large, so a node is released only when the plate pulls
/// it off its end of the band.
const RELAX_C: f64 = 1e4;
/// A pull toward the band's middle, so the bending energy has one answer
/// where nothing holds the surface (its weight against the energy's).
const RELAX_EPS: f64 = 1e-6;

/// The five-point Laplacian on the coarse lattice, no flux through its edge.
fn laplace(v: &[f64], cx: usize, cy: usize, out: &mut [f64]) {
    for j in 0..cy {
        for i in 0..cx {
            let q = j * cx + i;
            let mut a = 0.0;
            if i > 0 {
                a += v[q - 1] - v[q];
            }
            if i + 1 < cx {
                a += v[q + 1] - v[q];
            }
            if j > 0 {
                a += v[q - cx] - v[q];
            }
            if j + 1 < cy {
                a += v[q + cx] - v[q];
            }
            out[q] = a;
        }
    }
}

/// The gradient of the bending energy at `v`: `L (L v) + eps (v - mid)`.
fn bend(v: &[f64], mid: &[f64], cx: usize, cy: usize, tmp: &mut [f64], out: &mut [f64]) {
    laplace(v, cx, cy, tmp);
    laplace(tmp, cx, cy, out);
    for q in 0..v.len() {
        out[q] += RELAX_EPS * (v[q] - mid[q]);
    }
}

/// The surface of least bending energy (sum of squared Laplacians) with
/// the free nodes `free` unknown and the rest held: conjugate gradient from
/// `s`, which carries the held values.
///
/// Returns whether it converged (the residual fell under its tolerance
/// before [`RELAX_CG`] steps, or the energy has no free direction left).
fn plate(s: &mut [f64], mid: &[f64], free: &[bool], cx: usize, cy: usize) -> bool {
    let n = s.len();
    let (mut tmp, mut g) = (vec![0.0; n], vec![0.0; n]);
    bend(s, mid, cx, cy, &mut tmp, &mut g);
    let mut r: Vec<f64> = (0..n).map(|q| if free[q] { -g[q] } else { 0.0 }).collect();
    let mut d = r.clone();
    let mut rr: f64 = r.iter().map(|v| v * v).sum();
    let stop = rr * 1e-14 + 1e-24;
    let mut bd = vec![0.0; n];
    for _ in 0..RELAX_CG {
        if rr <= stop {
            return true;
        }
        bend(&d, mid, cx, cy, &mut tmp, &mut bd);
        // the energy's gradient is affine: remove its constant part
        for q in 0..n {
            bd[q] += RELAX_EPS * mid[q];
        }
        let mut dbd = 0.0;
        for q in 0..n {
            if free[q] {
                dbd += d[q] * bd[q];
            } else {
                bd[q] = 0.0;
            }
        }
        if dbd <= 0.0 {
            return true;
        }
        let a = rr / dbd;
        let mut rr2 = 0.0;
        for q in 0..n {
            if free[q] {
                s[q] += a * d[q];
                r[q] -= a * bd[q];
                rr2 += r[q] * r[q];
            }
        }
        let beta = rr2 / rr;
        rr = rr2;
        for q in 0..n {
            d[q] = if free[q] { r[q] + beta * d[q] } else { 0.0 };
        }
    }
    rr <= stop
}

/// The surface [`relax`] found, and whether the solve settled.
#[derive(Clone, Debug)]
pub struct Relaxed {
    /// The surface, inside the band at every node.
    pub field: Field,
    /// False when the active-set solve ran out of rounds with nodes still
    /// changing sides, or a round's conjugate gradient ran out of steps:
    /// the surface is still inside the band, but may have a crease the
    /// plate would not.
    pub converged: bool,
}

/// **relax**: the smoothest surface that stays inside the band: the one of
/// least bending energy (a thin plate, not a membrane), so where a wide band
/// meets a narrow one the surface leaves the narrow band's height with the
/// same slope it had there, with no crease, and runs smoothly across the
/// wide band. It is found over a coarse lattice by an active-set solve (the
/// nodes held at an end of their band are the ones the plate presses on),
/// then carried back to the lattice, bilinear, and held inside the band
/// there. `scale` is the system's ([`crate::Frame::scale`]): the coarse
/// lattice is [`RELAX_STEP_MM`] times it.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn relax(band: &Band, scale: f64) -> Relaxed {
    relax_rounds(band, scale, RELAX_ROUNDS)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn relax_rounds(band: &Band, scale: f64, rounds: usize) -> Relaxed {
    let (lo, hi) = (band.lo(), band.hi());
    let (nx, ny) = (lo.nx, lo.ny);
    // coarse nodes at every `c`-th fine node, and the last one
    let c = ((RELAX_STEP_MM * scale / lo.step).round() as usize).max(1);
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
    let n = cx * cy;
    let mid: Vec<f64> = clo.iter().zip(&chi).map(|(a, b)| 0.5 * (a + b)).collect();
    let mut s = mid.clone();
    // 0: free, 1: held at its low, 2: held at its high; a thin band is held
    let mut held: Vec<u8> = (0..n)
        .map(|q| if chi[q] - clo[q] < 1e-9 { 1 } else { 0 })
        .collect();
    let fixed = held.clone();
    let (mut tmp, mut g) = (vec![0.0; n], vec![0.0; n]);
    let mut converged = true;
    for round in 0..rounds {
        for q in 0..n {
            match held[q] {
                1 => s[q] = clo[q],
                2 => s[q] = chi[q],
                _ => {}
            }
        }
        let free: Vec<bool> = held.iter().map(|&h| h == 0).collect();
        converged &= plate(&mut s, &mid, &free, cx, cy);
        // the pressure the plate puts on each node: positive on a node held
        // up by its low, negative on one held down by its high
        bend(&s, &mid, cx, cy, &mut tmp, &mut g);
        let mut changed = false;
        for q in 0..n {
            if fixed[q] == 1 {
                continue;
            }
            let h = if g[q] + RELAX_C * (clo[q] - s[q]) > 0.0 {
                1
            } else if g[q] + RELAX_C * (chi[q] - s[q]) < 0.0 {
                2
            } else {
                0
            };
            if h != held[q] {
                held[q] = h;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        if round + 1 == rounds {
            converged = false;
        }
    }
    for q in 0..n {
        s[q] = s[q].clamp(clo[q], chi[q]);
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
    Relaxed {
        field: lo.with_z(z),
        converged,
    }
}

/// How far the distance to a pin is smoothed, mm.
const PIN_BLUR_MM: f64 = 2.0;

/// A band thinner than this is a pin, mm.
const THIN_MM: f64 = 0.02;

/// A body's hills ([`hills`]) and whether the plate under them settled.
#[derive(Clone, Debug)]
pub struct Hills {
    /// The surface, inside the band.
    pub shape: Banded,
    /// See [`Relaxed::converged`]; an element notes it in the build record
    /// when it is false.
    pub converged: bool,
}

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
pub fn hills(noise: &Field, band: &Band, hill_mm: f64, slope_deg: f64, scale: f64) -> Hills {
    assert!(
        noise.nx == band.lo().nx && noise.ny == band.lo().ny,
        "hills: another lattice"
    );
    let (lo, hi) = (band.lo(), band.hi());
    let n = lo.z.len();
    let room: Vec<bool> = (0..n).map(|k| hi.z[k] - lo.z[k] >= THIN_MM).collect();
    let thin: Vec<bool> = room.iter().map(|r| !r).collect();
    // distance to the nearest pin, mm
    // blurred over `PIN_BLUR_MM`, so the ridge where two pins are equally
    // near (the middle of a bank) is not a crease in how far the hills grow
    let to_pin: Vec<f64> = if thin.iter().any(|&b| b) {
        let (d, _) = dt_geom::distance_nearest(&thin, lo.nx, lo.ny);
        let d = lo.with_z(d.into_iter().map(|v| v * lo.step).collect());
        d.blur(PIN_BLUR_MM).z
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
    let Relaxed {
        field: smooth,
        converged,
    } = relax(band, scale);
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
    Hills {
        shape: band.fit(&raw),
        converged,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::cast_precision_loss)]
    use super::*;
    use dt_geom::Iv;

    /// A band over a `nx` by `ny` lattice: pinned (zero width) where `pin`
    /// gives a height, otherwise `lo(x, y)` to `hi(x, y)`.
    fn band(
        nx: usize,
        ny: usize,
        step: f64,
        pin: impl Fn(f64, f64) -> Option<f64>,
        lo: impl Fn(f64, f64) -> f64,
        hi: impl Fn(f64, f64) -> f64,
    ) -> Band {
        Band::from_fn(nx, ny, step, |x, y| match pin(x, y) {
            Some(z) => Iv { lo: z, hi: z },
            None => Iv {
                lo: lo(x, y),
                hi: hi(x, y),
            },
        })
    }

    /// A plane held at two ends (each over a few coarse cells, so its slope
    /// is held too), in a band wide enough never to touch it, stays the
    /// plane: the plate has no bending to spend on it.
    #[test]
    fn a_plane_in_a_wide_band_stays_the_plane() {
        let (nx, ny, step) = (121, 81, 0.25);
        let plane = |x: f64| 3.0 + 0.08 * x;
        let w = (nx - 1) as f64 * step;
        let b = band(
            nx,
            ny,
            step,
            |x, _| (x < 2.3 || x > w - 2.3).then(|| plane(x)),
            |x, _| plane(x) - 40.0,
            |x, _| plane(x) + 40.0,
        );
        let r = relax(&b, 1.0);
        assert!(r.converged);
        for k in 0..nx * ny {
            let (x, y) = ((k % nx) as f64 * step, (k / nx) as f64 * step);
            if x > 2.3 && x < w - 2.3 {
                let d = (r.field.z[k] - plane(x)).abs();
                assert!(d < 0.02, "({x}, {y}): {d}");
            }
        }
    }

    /// The surface is inside the band at every node, meets its pins, and is
    /// pressed against the narrow band's ends where the plate must be.
    #[test]
    fn a_pinned_band_stays_inside_and_meets_its_pins() {
        let (nx, ny, step) = (121, 81, 0.25);
        let b = band(
            nx,
            ny,
            step,
            |x, y| {
                if x < 0.3 {
                    Some(2.0)
                } else if x > 29.7 {
                    Some(5.0)
                } else if (14.0..16.0).contains(&x) && y < 10.0 {
                    Some(1.0)
                } else {
                    None
                }
            },
            |_, y| if y > 12.0 { 3.0 } else { 0.5 },
            |_, y| if y > 12.0 { 3.4 } else { 7.0 },
        );
        let r = relax(&b, 1.0);
        assert!(r.converged);
        let (lo, hi) = (b.lo(), b.hi());
        for k in 0..nx * ny {
            let z = r.field.z[k];
            assert!(z >= lo.z[k] && z <= hi.z[k], "node {k}: {z}");
            if hi.z[k] - lo.z[k] < 1e-9 {
                assert!((z - lo.z[k]).abs() < 1e-9, "pin {k}: {z}");
            }
        }
    }

    /// The same model at the 35 mm square and at 0.7257 of it (OpenLOCK) is
    /// the same surface scaled: the coarse lattice scales with it.
    #[test]
    fn the_same_band_at_another_scale_is_the_same_surface_scaled() {
        let scale = 25.4 / 35.0;
        let (nx, ny) = (101, 61);
        let make = |sc: f64| {
            band(
                nx,
                ny,
                0.25 * sc,
                |x, _| {
                    if x < 0.3 * sc {
                        Some(2.0 * sc)
                    } else if x > 24.7 * sc {
                        Some(4.0 * sc)
                    } else {
                        None
                    }
                },
                |_, y| if y > 8.0 * sc { 2.5 * sc } else { 0.5 * sc },
                |_, y| if y > 8.0 * sc { 3.0 * sc } else { 6.0 * sc },
            )
        };
        let (a, b) = (relax(&make(1.0), 1.0), relax(&make(scale), scale));
        assert!(a.converged && b.converged);
        for k in 0..nx * ny {
            let d = (a.field.z[k] * scale - b.field.z[k]).abs();
            assert!(d < 1e-6, "node {k}: {d}");
        }
    }

    /// A solve that runs out of rounds says so instead of passing quietly,
    /// and the surface it leaves is still inside the band.
    #[test]
    fn a_solve_that_does_not_settle_says_so() {
        let (nx, ny, step) = (121, 81, 0.25);
        // a pinned spike between narrow ends: the plate must press on the band
        let b = band(
            nx,
            ny,
            step,
            |x, y| {
                if x < 0.3 || x > 29.7 {
                    Some(1.0)
                } else {
                    ((x - 15.0).abs() < 0.3 && (y - 10.0).abs() < 0.3).then_some(3.0)
                }
            },
            |_, _| 0.9,
            |_, _| 1.3,
        );
        let settled = relax(&b, 1.0);
        assert!(settled.converged);
        let cut = relax_rounds(&b, 1.0, 1);
        assert!(!cut.converged);
        for k in 0..nx * ny {
            assert!(cut.field.z[k] >= b.lo().z[k] && cut.field.z[k] <= b.hi().z[k]);
        }
    }
}
