//! dharmatiles geometry kernels.
//!
//! [`grid`] (lattices, the uniform lift, the distance transform) and
//! [`raster`] (line rasterising, the paint bucket) arrived in P1; P2 added
//! [`noise`] (position-keyed noise on one lattice), [`filter`] (blurs,
//! grey morphology, quantiles), the distance transform's nearest indices,
//! [`hash`] (a spatial hash), [`lift`] (the error-bounded adaptive lift)
//! and the seeded stream [`Rng`]. P3 added [`convex`] (polytopes cut by
//! half-spaces: rock facets and chips), [`geodesic`] (multi-source
//! distance through a mask), [`noise::fbm3`], polygon rasterising
//! ([`fill_polygons`]) and the mesh trait's hull, refine, transform and
//! slice.
//!
//! Phase 0: the [`mesh::Mesh`] and [`rays::Rays`] traits that every backend
//! sits behind, and [`seed::Seed`], the deterministic path-keyed seed
//! derivation, and [`source`], the source-ID rule that keeps booleans
//! deterministic and materials traceable. Mesh backends are features: `manifold-ffi` (default, the C++
//! kernel through manifold-csg) and `manifold-rust` (the pure-Rust port, for
//! wasm). [`Solid`] is the default backend's type.

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

pub use convex::Convex;
pub use filter::{Footprint, blur, blur_recursive, dilate, erode, quantile};
pub use geodesic::geodesic;
pub use grid::{Grid, LiftError, distance, distance_nearest, lift};
pub use hash::SpatialHash;
pub use lift::lift_adaptive;
pub use mesh::{BOX_TRIANGLES, BoolOp, Bounds, Fnv, Mesh, MeshError, TriMesh, half_space};
pub use raster::{Mask, draw_line, fill, fill_polygons};
pub use rays::{Hit, ObvhsRays, ParryRays, Ray, Rays};
pub use seed::{Rng, Seed};
pub use source::{IdBlock, Materials, batch_in_order};

/// The default backend's solid: the C++ kernel when `manifold-ffi` is on,
/// otherwise the pure-Rust port.
#[cfg(feature = "manifold-ffi")]
pub type Solid = backend::ffi::FfiSolid;
/// The default backend's solid: the C++ kernel when `manifold-ffi` is on,
/// otherwise the pure-Rust port.
#[cfg(all(feature = "manifold-rust", not(feature = "manifold-ffi")))]
pub type Solid = backend::port::PortSolid;
