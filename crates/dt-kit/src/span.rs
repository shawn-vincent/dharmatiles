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
