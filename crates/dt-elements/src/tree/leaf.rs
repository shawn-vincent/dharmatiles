//! One leaf: the blade surface, its root oval and the closed solid
//! between them. The grid is 12 stations × 10 lateral sections with
//! a fixed vertex layout, so the placer's index arithmetic is simple.
//!
//! The blade is an ovate teardrop (widest a third from the base) with a
//! quartic Bézier dome across it, a tanh crease along the midrib, a
//! full-length 30° arch, a curl over the tip third, and a rigid lift about
//! the base. The oval is a half-size copy embedded below it; the solid is
//! the blade, the reversed oval, and a wall stitched vertex for vertex
//! round both perimeters. All maths is `libm`.

use dt_kit::TriMesh;

/// Longitudinal sections, base → tip (`_LEAF_N_LONG`).
pub(super) const N_LONG: usize = 12;
/// Lateral sections across the blade (`_LEAF_N_LAT`, even).
pub(super) const N_LAT: usize = 10;
/// Interior stations (rings).
pub(super) const RINGS: usize = N_LONG - 1;
/// Vertices per ring.
pub(super) const STRIDE: usize = N_LAT + 1;
/// The base vertex of a surface.
pub(super) const BASE: usize = RINGS * STRIDE;
/// The tip vertex of a surface.
pub(super) const TIP: usize = BASE + 1;
/// Vertices in a surface (and in an oval).
pub(super) const NV: usize = TIP + 1;

/// `_LEAF_CREASE_SHARPNESS`.
const CREASE_SHARPNESS: f64 = 10.0;
/// The full-length arch at the base (`build_leaf_surface`'s default).
pub(super) const ARCH_DEG: f64 = 30.0;

/// The blade's shape for one leaf.
#[derive(Clone, Copy, Debug)]
pub(super) struct Blade {
    pub length_mm: f64,
    pub width_mm: f64,
    pub thickness_mm: f64,
    pub fold_angle_deg: f64,
    pub inner_curve: f64,
    pub outer_curve: f64,
    pub curl_deg: f64,
    pub lift_mm: f64,
    /// The full-length arch's slope at the base ([`ARCH_DEG`] as built).
    pub arch_deg: f64,
}

/// Interior station `k` (0-based) as numpy's `linspace(0, 1, 13)[1:-1]`.
#[allow(clippy::cast_precision_loss, reason = "k < 12")]
pub(super) fn station(k: usize) -> f64 {
    (k + 1) as f64 * (1.0 / N_LONG as f64)
}

/// Lateral position `j` as numpy's `linspace(-1, 1, 11)`.
#[allow(clippy::cast_precision_loss, reason = "j ≤ 10")]
fn lateral(j: usize) -> f64 {
    if j == N_LAT {
        1.0
    } else {
        j as f64 * (2.0 / N_LAT as f64) - 1.0
    }
}

fn width_profile(s: f64) -> f64 {
    let peak = libm::pow(1.0 / 3.0, 0.4) * libm::pow(2.0 / 3.0, 0.8);
    libm::pow(s, 0.4) * libm::pow(1.0 - s, 0.8) / peak
}

fn lobe(x: f64, inner: f64, outer: f64) -> f64 {
    let q = 1.0 - x;
    4.0 * inner * q * q * q * x + 6.0 * q * q * x * x + 4.0 * outer * q * x * x * x
}

/// The blade's top surface (`build_leaf_surface`): `NV` vertices, rings of
/// `STRIDE` from the base, then the base and tip points.
pub(super) fn surface(base: V, tangent: V, up_hint: V, b: &Blade) -> Vec<V> {
    let along = safe_norm(tangent);
    let mut world_up = safe_norm(up_hint);
    if dot(along, world_up).abs() > 0.9 {
        world_up = if dot(along, [0.0, 0.0, 1.0]).abs() > 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
    }
    let across = scale(
        cross(world_up, along),
        1.0 / norm(cross(world_up, along)).max(1e-10),
    );
    let normal = scale(
        cross(along, across),
        1.0 / norm(cross(along, across)).max(1e-10),
    );

    let len = b.length_mm;
    let fold_slope = libm::tan(b.fold_angle_deg.to_radians());
    let long_t_peak = libm::pow(0.25, 0.5) * libm::pow(0.75, 1.5);
    let arch_slope = libm::tan(b.arch_deg.abs().to_radians());
    let curl_tip_slope = libm::tan(b.curl_deg.abs().to_radians());
    let curl_start = 2.0 / 3.0;
    let curl_zone = (1.0 - curl_start) * len;
    let curl_corr = if b.curl_deg.abs() > 1e-8 {
        curl_tip_slope + arch_slope
    } else {
        0.0
    };
    let profile = |s: f64| -> (f64, f64) {
        let u = ((s - curl_start) / (1.0 - curl_start)).clamp(0.0, 1.0);
        let arch_z = len * arch_slope * s * (1.0 - s);
        let arch_d = arch_slope * (1.0 - 2.0 * s);
        let (cz, cd) = if s > curl_start {
            let u3 = u * u * u;
            let u4 = u3 * u;
            let u5 = u4 * u;
            let u6 = u5 * u;
            (
                curl_zone * curl_corr * (2.5 * u4 - 3.0 * u5 + u6),
                curl_corr * (10.0 * u3 - 15.0 * u4 + 6.0 * u5),
            )
        } else {
            (0.0, 0.0)
        };
        (arch_z + cz, arch_d + cd)
    };
    let inner = b.inner_curve.max(0.0);
    let outer = b.outer_curve.max(0.0);
    #[allow(clippy::cast_precision_loss, reason = "i ≤ 256")]
    let lobe_norm = (0..257)
        .map(|i| lobe(if i == 256 { 1.0 } else { i as f64 / 256.0 }, inner, outer))
        .fold(f64::NEG_INFINITY, f64::max);

    let mut v = vec![[0.0; 3]; NV];
    for r in 0..RINGS {
        let s = station(r);
        let w = 0.5 * b.width_mm * width_profile(s);
        let taper = libm::pow(s, 0.5) * libm::pow(1.0 - s, 1.5) / long_t_peak;
        let (h, slope) = profile(s);
        let mid = add(add(base, scale(along, s * len)), scale(normal, h));
        let tilt = libm::atan(slope);
        let sn = add(
            scale(along, -libm::sin(tilt)),
            scale(normal, libm::cos(tilt)),
        );
        let ramp = 6.0 * s.powi(5) - 15.0 * s.powi(4) + 10.0 * s.powi(3);
        for j in 0..STRIDE {
            let lat = lateral(j);
            let a = lat.abs();
            let spread = add(mid, scale(across, lat * w));
            let crease = libm::tanh(a * CREASE_SHARPNESS) * (w * fold_slope * taper * ramp);
            let dome_shape = if lobe_norm <= 1e-12 {
                0.0
            } else {
                lobe(a, inner, outer) / lobe_norm
            };
            let dome = b.thickness_mm * dome_shape * taper;
            v[r * STRIDE + j] = add(spread, scale(sn, crease + dome));
        }
    }
    v[BASE] = base;
    v[TIP] = add(add(base, scale(along, len)), scale(normal, profile(1.0).0));

    if b.lift_mm.abs() > 1e-8 {
        let ang = libm::atan2(b.lift_mm, len);
        let (c, s) = (libm::cos(ang), libm::sin(ang));
        for p in &mut v {
            let rel = sub(*p, base);
            let rot = add(
                sub(scale(rel, c), scale(cross(across, rel), s)),
                scale(across, (1.0 - c) * dot(rel, across)),
            );
            *p = add(base, rot);
        }
    }
    v
}

/// The surface's triangles (base fan, body quads, tip fan), as
/// `build_leaf_surface` writes them.
#[allow(clippy::cast_possible_truncation, reason = "indices < 123")]
pub(super) fn surface_triangles() -> Vec<[u32; 3]> {
    let mut f = Vec::with_capacity(2 * N_LAT * RINGS);
    let (b, t) = (BASE as u32, TIP as u32);
    for j in 0..N_LAT as u32 {
        f.push([b, j, j + 1]);
    }
    let st = STRIDE as u32;
    for ri in 0..(RINGS - 1) as u32 {
        for j in 0..N_LAT as u32 {
            let a = ri * st + j;
            let bb = ri * st + j + 1;
            let c = (ri + 1) * st + j + 1;
            let d = (ri + 1) * st + j;
            f.push([a, d, c]);
            f.push([a, c, bb]);
        }
    }
    let last = (RINGS as u32 - 1) * st;
    for j in 0..N_LAT as u32 {
        f.push([t, last + j + 1, last + j]);
    }
    f
}

/// The root oval (`build_leaf_oval_offsets`) relative to the leaf frame's
/// origin: half size, spanning `[L/2, L]` along `t`, `embed_mm` below.
pub(super) fn oval(n_hat: V, t: V, across: V, len: f64, width: f64, embed_mm: f64) -> Vec<V> {
    let e = scale(n_hat, -embed_mm);
    let mut v = vec![[0.0; 3]; NV];
    for r in 0..RINGS {
        let s = station(r);
        let hw = width / 4.0 * libm::sin(std::f64::consts::PI * s);
        for j in 0..STRIDE {
            v[r * STRIDE + j] = add(
                add(e, scale(t, (0.5 + 0.5 * s) * len)),
                scale(across, lateral(j) * hw),
            );
        }
    }
    v[BASE] = add(scale(t, 0.5 * len), e);
    v[TIP] = add(scale(t, len), e);
    v
}

/// The closed leaf (`solidify_leaf`): blade vertices first (`0..NV`), then
/// the oval's (`NV..2·NV`); the oval reversed and a wall stitched round
/// the matching perimeters. Wound outward (positive volume).
#[allow(clippy::cast_possible_truncation, reason = "indices < 246")]
pub(super) fn solidify(outer: &[V], inner: &[V], tris: &[[u32; 3]]) -> TriMesh {
    let n = NV as u32;
    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(2 * tris.len() + 4 * N_LONG + 4);
    triangles.extend_from_slice(tris);
    triangles.extend(tris.iter().map(|t| [t[2] + n, t[1] + n, t[0] + n]));
    let st = STRIDE as u32;
    let mut perim = vec![BASE as u32];
    perim.extend((0..RINGS as u32).map(|i| i * st));
    perim.push(TIP as u32);
    perim.extend((0..RINGS as u32).rev().map(|i| i * st + N_LAT as u32));
    // The blade surface runs each perimeter edge i1 → i0, the reversed oval
    // j1 → j0, so the wall takes i0 → i1 and j0 → j1.
    for k in 0..perim.len() {
        let i0 = perim[k];
        let i1 = perim[(k + 1) % perim.len()];
        let (j0, j1) = (i0 + n, i1 + n);
        triangles.push([i1, i0, j0]);
        triangles.push([i1, j0, j1]);
    }
    let mut positions = outer.to_vec();
    positions.extend_from_slice(inner);
    let mut m = TriMesh {
        positions,
        triangles,
        ..TriMesh::default()
    };
    if dt_kit::signed_volume(&m) < 0.0 {
        for t in &mut m.triangles {
            t.swap(1, 2);
        }
    }
    m
}

// ── small vector helpers (f64 mm) ────────────────────────────────────────────

pub(super) type V = [f64; 3];

pub(super) use dt_kit::graph::{add, cross, dot, norm, scale, sub};

/// Unit, or unchanged when near zero.
pub(super) fn safe_norm(v: V) -> V {
    let n = norm(v);
    if n > 1e-12 { scale(v, 1.0 / n) } else { v }
}

/// Rodrigues rotation of `v` about unit `axis`.
pub(super) fn rotate_about(v: V, axis: V, angle: f64) -> V {
    let (c, s) = (libm::cos(angle), libm::sin(angle));
    add(
        add(scale(v, c), scale(cross(axis, v), s)),
        scale(axis, dot(axis, v) * (1.0 - c)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blade() -> Blade {
        Blade {
            length_mm: 4.5,
            width_mm: 3.0,
            thickness_mm: 0.24,
            fold_angle_deg: 6.0,
            inner_curve: 1.5,
            outer_curve: 0.72,
            curl_deg: 32.0,
            lift_mm: 1.2,
            arch_deg: ARCH_DEG,
        }
    }

    /// Every edge is shared by exactly two triangles, once each way.
    pub(crate) fn closed(m: &TriMesh) -> bool {
        let mut e = std::collections::HashMap::new();
        for t in &m.triangles {
            for k in 0..3 {
                *e.entry((t[k], t[(k + 1) % 3])).or_insert(0) += 1;
            }
        }
        e.iter()
            .all(|(&(a, b), &c)| c == 1 && e.get(&(b, a)) == Some(&1))
    }

    #[test]
    fn leaf_solid_is_closed_and_outward() {
        let (n, t) = ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        let across = cross(n, t);
        let origin = [0.0, 0.0, 0.0];
        let surf = surface(add(origin, scale(n, 0.3)), t, n, &blade());
        let inner: Vec<V> = oval(n, t, across, 4.5, 3.0, 0.75)
            .into_iter()
            .map(|p| add(p, origin))
            .collect();
        let m = solidify(&surf, &inner, &surface_triangles());
        assert_eq!(m.positions.len(), 2 * NV);
        assert_eq!(m.triangles.len(), 2 * 220 + 2 * 24);
        assert!(closed(&m));
        assert!(m.is_watertight());
        assert!(dt_kit::signed_volume(&m) > 0.0);
        // The tip is the blade's far end, lifted by the curl and the lift.
        assert!((surf[TIP][0] - 4.5).abs() < 0.6 && surf[TIP][2] > 0.3);
    }
}
