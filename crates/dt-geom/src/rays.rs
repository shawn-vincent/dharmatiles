//! Ray casts on a triangle mesh: [`ObvhsRays`] (obvhs CWBVH, with
//! Embree's triangle test), queried in batches.

mod obvhs;

pub use self::obvhs::ObvhsRays;

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

/// Rays per rayon task. Many batches are a few hundred rays; those stay on
/// one thread.
const RAYS_PER_TASK: usize = 256;

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

    #[test]
    fn obvhs_square() {
        let (p, t) = square();
        let r = ObvhsRays::build(&p, &t);
        let rays = [
            Ray::new([0.7, 0.2, 1.0], [0., 0., -1.]),
            Ray::new([0.2, 0.7, -2.0], [0., 0., 2.]),
            Ray::new([2.0, 2.0, 1.0], [0., 0., -1.]),
            Ray {
                t_max: 0.5,
                ..Ray::new([0.5, 0.2, 1.0], [0., 0., -1.])
            },
            // A negative-zero component (horizontal rays):
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
        assert_eq!(
            r.structure_hash(),
            ObvhsRays::build(&p, &t).structure_hash()
        );
    }
}
