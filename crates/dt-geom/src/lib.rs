//! dharmatiles geometry kernels.
//!
//! - [`grid`]: lattices, the uniform lift, the distance transform (with
//!   nearest indices); [`raster`]: line and polygon rasterising
//!   ([`fill_polygons`]), the paint bucket.
//! - [`noise`]: position-keyed noise on one lattice, and [`noise::fbm3`];
//!   [`filter`]: blurs, grey morphology, quantiles; [`geodesic`]:
//!   multi-source distance through a mask.
//! - [`lift`]: the error-bounded adaptive lift; [`hash`]: a spatial hash;
//!   [`convex`]: polytopes cut by half-spaces (rock facets and chips);
//!   [`vec3`]: the 3-vector helpers.
//! - [`seed::Seed`], the deterministic path-keyed seed derivation, and the
//!   seeded stream [`Rng`].
//! - [`mesh::Mesh`] (hull, refine, transform, slice, booleans) and
//!   [`rays::Rays`], the traits the backends sit behind; [`source`], the
//!   source-ID rule that keeps booleans deterministic and materials
//!   traceable.
//!
//! The mesh backend is the `manifold-ffi` feature (default, the C++
//! kernel through manifold-csg). [`Solid`] is its solid type.

pub mod backend;
pub mod convex;
pub mod filter;
pub mod geodesic;
pub mod grid;
pub mod hash;
pub mod lift;
pub mod mesh;
pub mod noise;
pub mod raster;
pub mod rays;
pub mod seed;
pub mod source;
pub mod vec3;

pub use convex::Convex;
pub use filter::{Footprint, blur, blur_recursive, dilate, erode, quantile};
pub use geodesic::geodesic;
pub use grid::{Grid, LiftError, distance, distance_nearest, lift};
pub use hash::SpatialHash;
pub use lift::lift_adaptive;
pub use mesh::{BOX_TRIANGLES, BoolOp, Bounds, Fnv, Mesh, MeshError, TriMesh, half_space};
pub use raster::{Mask, draw_line, fill, fill_polygons};
pub use rays::{Hit, ObvhsRays, Ray, Rays};
pub use seed::{Rng, Seed};
pub use source::{IdBlock, Materials, batch_in_order};

/// The backend's solid: the C++ kernel.
#[cfg(feature = "manifold-ffi")]
pub type Solid = backend::ffi::FfiSolid;
