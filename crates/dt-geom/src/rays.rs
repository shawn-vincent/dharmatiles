//! The ray trait. Candidates (obvhs, parry3d, embree3 over FFI) are compared
//! by the R3 spike; all of them sit behind this one interface.

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
}
