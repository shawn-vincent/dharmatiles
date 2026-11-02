//! The ray trait and its pure-Rust backends.
//!
//! The R3 spike (docs/rust/spikes.html, "R3: rays without Embree") compared
//! two backends against Embree on the leaf placer's captured ray batches:
//! [`ObvhsRays`] (obvhs CWBVH, with Embree's triangle test) and
//! [`ParryRays`] (parry3d `TriMesh`). Both sit behind [`Rays`], so the
//! choice can change without touching callers (R10).

mod obvhs;
mod parry;

pub use self::obvhs::ObvhsRays;
pub use self::parry::ParryRays;

/// A ray in mm. `dir` need not be normalised; hit distances are in units of
/// `dir`'s length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: [f32; 3],
    pub dir: [f32; 3],
    /// Ignore hits closer than this.
    pub t_min: f32,
    /// Ignore hits farther than this.
    pub t_max: f32,
}

impl Ray {
    /// A ray from `origin` along `dir` with Embree's defaults: `[0, inf)`.
    pub fn new(origin: [f32; 3], dir: [f32; 3]) -> Self {
        Self {
            origin,
            dir,
            t_min: 0.0,
            t_max: f32::INFINITY,
        }
    }
}

/// The first surface a ray meets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Distance along the ray, in units of `dir`'s length.
    pub t: f32,
    /// Index of the triangle that was hit.
    pub triangle: u32,
}

/// An acceleration structure over one triangle mesh, queried in batches.
/// Builds must be deterministic: the same mesh gives the same answers.
pub trait Rays: Sized {
    /// Build the acceleration structure.
    fn build(positions: &[[f32; 3]], triangles: &[[u32; 3]]) -> Self;

    /// Closest hit for each ray, in input order.
    fn intersect(&self, rays: &[Ray]) -> Vec<Option<Hit>>;

    /// Whether each ray hits anything in `[t_min, t_max]`, in input order.
    fn occluded(&self, rays: &[Ray]) -> Vec<bool> {
        self.intersect(rays).iter().map(Option::is_some).collect()
    }

    /// A hash of the built structure, for build-determinism checks.
    fn structure_hash(&self) -> u64;
}

/// Rays per rayon task. Many batches are a few hundred rays; those stay on
/// one thread.
const RAYS_PER_TASK: usize = 256;

/// Embree's Möller–Trumbore test (`MoellerTrumboreIntersector1`, non-robust
/// mode, no culling), so ties and rounding follow Embree's as closely as a
/// scalar port can. Returns `t` in units of `dir`, or `None`.
#[inline]
pub fn intersect_triangle(tri: [[f32; 3]; 3], ray: &Ray) -> Option<f32> {
    let [v0, v1, v2] = tri;
    let e1 = sub(v0, v1);
    let e2 = sub(v2, v0);
    let ng = cross(e2, e1);
    let c = sub(v0, ray.origin);
    let r = cross(c, ray.dir);
    let den = dot(ng, ray.dir);
    let abs_den = den.abs();
    let sgn = den.to_bits() & 0x8000_0000;
    let u = f32::from_bits(dot(r, e2).to_bits() ^ sgn);
    let v = f32::from_bits(dot(r, e1).to_bits() ^ sgn);
    if abs_den <= 0.0 || u < 0.0 || v < 0.0 || u + v > abs_den {
        return None;
    }
    let t = f32::from_bits(dot(ng, c).to_bits() ^ sgn);
    if !(abs_den * ray.t_min < t && t <= abs_den * ray.t_max) {
        return None;
    }
    Some(t / abs_den)
}

#[inline]
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// FNV-1a over bytes: stable across platforms and toolchains, unlike
/// `DefaultHasher`.
pub(crate) fn fnv1a(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub(crate) const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit square in z = 0, two triangles sharing the diagonal.
    fn square() -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
        (
            vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
            vec![[0, 1, 2], [0, 2, 3]],
        )
    }

    fn check<R: Rays>() {
        let (p, t) = square();
        let r = R::build(&p, &t);
        let rays = [
            Ray::new([0.7, 0.2, 1.0], [0., 0., -1.]),
            Ray::new([0.2, 0.7, -2.0], [0., 0., 2.]),
            Ray::new([2.0, 2.0, 1.0], [0., 0., -1.]),
            Ray {
                t_max: 0.5,
                ..Ray::new([0.5, 0.2, 1.0], [0., 0., -1.])
            },
            // A negative-zero component (rockpile's horizontal relief rays):
            // obvhs 0.4 drops these without the +0.0 fix.
            Ray::new([0.7, 0.2, 1.0], [-0.0, -0.0, -1.]),
        ];
        let h = r.intersect(&rays);
        assert_eq!(h[0].map(|h| h.triangle), Some(0));
        assert!(h[0].is_some_and(|h| (h.t - 1.0).abs() < 1e-6));
        assert_eq!(h[1].map(|h| h.triangle), Some(1));
        assert!(h[1].is_some_and(|h| (h.t - 1.0).abs() < 1e-6));
        assert_eq!(h[2], None);
        assert_eq!(h[3], None);
        assert_eq!(h[4].map(|h| h.triangle), Some(0));
        assert_eq!(r.occluded(&rays), vec![true, true, false, false, true]);
        assert_eq!(r.structure_hash(), R::build(&p, &t).structure_hash());
    }

    #[test]
    fn obvhs_square() {
        check::<ObvhsRays>();
    }

    #[test]
    fn parry_square() {
        check::<ParryRays>();
    }
}
