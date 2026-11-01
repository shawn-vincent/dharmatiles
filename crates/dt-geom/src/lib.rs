//! dharmatiles geometry kernels.
//!
//! Phase 0: the [`mesh::Mesh`] and [`rays::Rays`] traits that every backend
//! sits behind, and [`seed::Seed`], the deterministic path-keyed seed
//! derivation. Mesh backends are features: `manifold-ffi` (default, the C++
//! kernel through manifold-csg) and `manifold-rust` (the pure-Rust port, for
//! wasm). [`Solid`] is the default backend's type.

pub mod backend;
pub mod mesh;
pub mod rays;
pub mod seed;

pub use mesh::{BoolOp, Bounds, Fnv, Mesh, MeshError, TriMesh};
pub use rays::{Hit, ObvhsRays, ParryRays, Ray, Rays};
pub use seed::Seed;

/// The default backend's solid: the C++ kernel when `manifold-ffi` is on,
/// otherwise the pure-Rust port.
#[cfg(feature = "manifold-ffi")]
pub type Solid = backend::ffi::FfiSolid;
/// The default backend's solid: the C++ kernel when `manifold-ffi` is on,
/// otherwise the pure-Rust port.
#[cfg(all(feature = "manifold-rust", not(feature = "manifold-ffi")))]
pub type Solid = backend::port::PortSolid;
