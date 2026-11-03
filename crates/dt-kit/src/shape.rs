//! Operations that make and cut solids: loft, extrude, carve, and the
//! area operation that turns drawn lines and fill points into a mask.

use dt_geom::{BoolOp, Mask, Mesh, TriMesh, batch_in_order};

use crate::scene::{Frame, Slot, Solid, leaf, relabel};
use crate::{KitError, Outline};

/// **loft**: a closed solid through `rings` (each a closed loop of the same
/// number of points, in order), capped at the first and the last. Rings may
/// run either way round; the result winds outward.
///
/// # Errors
/// [`KitError`] if the rings differ in length, have fewer than 3 points,
/// or the kernel refuses the result.
pub fn loft(slot: Slot, rings: &[Vec<[f64; 3]>]) -> Result<Solid, KitError> {
    let n = rings.first().map_or(0, Vec::len);
    if rings.len() < 2 || n < 3 || rings.iter().any(|r| r.len() != n) {
        return Err(KitError::Shape(
            "loft needs two or more rings of equal length (≥ 3)".into(),
        ));
    }
    let u = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    let positions: Vec<[f64; 3]> = rings.iter().flatten().copied().collect();
    let mut triangles = Vec::new();
    for k in 1..n - 1 {
        triangles.push([0, u(k), u(k + 1)]);
        let b = (rings.len() - 1) * n;
        triangles.push([u(b), u(b + k + 1), u(b + k)]);
    }
    for r in 0..rings.len() - 1 {
        let (a, c) = (r * n, (r + 1) * n);
        for j in 0..n {
            let j1 = (j + 1) % n;
            triangles.push([u(a + j), u(c + j), u(c + j1)]);
            triangles.push([u(a + j), u(c + j1), u(a + j1)]);
        }
    }
    let mut mesh = TriMesh {
        positions,
        triangles,
        ..TriMesh::default()
    };
    if signed_volume(&mesh) < 0.0 {
        mesh.triangles.iter_mut().for_each(|t| t.swap(1, 2));
    }
    leaf(slot, &mesh)
}

/// **extrude**: the outline's even-odd region, shrunk by `inset` mm with
/// mitred corners, as a prism from `z0` to `z1`.
///
/// # Errors
/// [`KitError`] if the kernel refuses the result.
pub fn extrude(
    slot: Slot,
    outline: &Outline,
    inset: f64,
    z0: f64,
    z1: f64,
) -> Result<Solid, KitError> {
    relabel(
        slot,
        &dt_geom::Solid::extrude(&outline.contours, inset, z0, z1),
    )
}

/// **carve**: `solid` less every cutter. The cut faces keep the solid's
/// material: give the cutters slots of that material.
///
/// # Errors
/// The kernel's status.
pub fn carve(solid: &Solid, cutters: &[Solid]) -> Result<Solid, KitError> {
    let mut parts = Vec::with_capacity(cutters.len() + 1);
    parts.push(solid.0.clone());
    parts.extend(cutters.iter().map(|c| c.0.clone()));
    let out = batch_in_order(&parts, BoolOp::Difference);
    out.status().map_err(KitError::Mesh)?;
    Ok(Solid(out))
}

/// **union** of solids, in slot order.
///
/// # Errors
/// The kernel's status.
pub fn union(solids: &[Solid]) -> Result<Solid, KitError> {
    let parts: Vec<dt_geom::Solid> = solids.iter().map(|s| s.0.clone()).collect();
    let out = batch_in_order(&parts, BoolOp::Union);
    out.status().map_err(KitError::Mesh)?;
    Ok(Solid(out))
}

/// **area**: the paint bucket. `lines` are polylines in squares (x east,
/// y north from the south-west corner) drawn as walls; the area is every
/// node reached from any of `fills` (points in squares) without crossing
/// one. Port of `core/region.py`'s straight paths.
#[must_use]
pub fn area(frame: &Frame, lines: &[Vec<[f64; 2]>], fills: &[[f64; 2]]) -> Mask {
    let (nx, ny) = frame.nodes();
    let per = f64::from(frame.cells_per_square);
    let mut walls = Mask::new(nx, ny, false);
    for line in lines {
        for w in line.windows(2) {
            dt_geom::draw_line(
                &mut walls,
                [w[0][0] * per, w[0][1] * per],
                [w[1][0] * per, w[1][1] * per],
            );
        }
    }
    let mut out = Mask::new(nx, ny, false);
    for p in fills {
        let node = |v: f64, n: usize| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let k = (v * per).round().clamp(0.0, (n - 1) as f64) as usize;
            k
        };
        let region = dt_geom::fill(&walls, (node(p[0], nx), node(p[1], ny)));
        out.bits
            .iter_mut()
            .zip(region.bits)
            .for_each(|(o, r)| *o |= r);
    }
    out
}

/// Signed volume of a closed mesh (positive when it winds outward).
#[must_use]
pub fn signed_volume(m: &TriMesh) -> f64 {
    m.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| m.positions[i as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

/// The unit normal of triangle `a b c` (zero for a degenerate one).
#[must_use]
pub(crate) fn normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    use dt_geom::vec3::{cross, sub, unit};
    // a zero cross product is already [0; 3]
    unit(cross(sub(b, a), sub(c, a)))
}

/// **wobble**: a polyline (mm) resampled every `step` mm and pushed
/// sideways by smooth noise of `sigma` mm along its length, scaled so the
/// largest push is `amount` mm, and tapered to nothing within `sigma` of
/// each end so its end points stay where they are (a shore's wander).
#[must_use]
pub fn wobble(
    points: &[[f64; 2]],
    amount: f64,
    sigma: f64,
    step: f64,
    seed: dt_geom::Seed,
) -> Vec<[f64; 2]> {
    let positive = |v: f64| v.is_finite() && v > 0.0;
    if points.len() < 2 || !positive(amount) || !positive(sigma) || !positive(step) {
        return points.to_vec();
    }
    // resample, with each sample's arc length and unit normal
    let mut pts: Vec<([f64; 2], f64, [f64; 2])> = Vec::new();
    let mut s0 = 0.0;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = libm::hypot(b[0] - a[0], b[1] - a[1]);
        if len <= 0.0 {
            continue;
        }
        let n = [-(b[1] - a[1]) / len, (b[0] - a[0]) / len];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let k = (libm::ceil(len / step) as usize).max(1);
        for i in 0..k {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f64 / k as f64;
            pts.push((
                [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])],
                s0 + t * len,
                n,
            ));
        }
        s0 += len;
    }
    let last = points[points.len() - 1];
    let n_last = pts.last().map_or([0.0, 0.0], |p| p.2);
    pts.push((last, s0, n_last));
    let xs: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let v = dt_geom::noise::smooth(seed.value(), sigma, &xs, &[0.0]);
    let peak = v.iter().fold(0.0f64, |m, x| m.max(x.abs())).max(1e-9);
    let total = s0;
    pts.iter()
        .zip(&v)
        .map(|((p, s, n), x)| {
            let taper = crate::smooth01(s / sigma) * crate::smooth01((total - s) / sigma);
            let o = amount * x / peak * taper;
            [p[0] + o * n[0], p[1] + o * n[1]]
        })
        .collect()
}
