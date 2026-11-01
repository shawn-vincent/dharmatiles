//! dharmatiles geometry kernels.
//!
//! Phase 0: the [`mesh::Mesh`] and [`rays::Rays`] traits that every backend
//! sits behind, and [`seed::Seed`], the deterministic path-keyed seed
//! derivation, and [`source`], the source-ID rule that keeps booleans
//! deterministic and materials traceable. Mesh backends are features: `manifold-ffi` (default, the C++
//! kernel through manifold-csg) and `manifold-rust` (the pure-Rust port, for
//! wasm). [`Solid`] is the default backend's type.

pub mod backend;
pub mod mesh;
pub mod rays;
pub mod seed;
pub mod source;

pub use mesh::{BOX_TRIANGLES, BoolOp, Bounds, Fnv, Mesh, MeshError, TriMesh, half_space};
pub use rays::{Hit, ObvhsRays, ParryRays, Ray, Rays};
pub use seed::Seed;
pub use source::{IdBlock, Materials, batch_in_order};

/// The default backend's solid: the C++ kernel when `manifold-ffi` is on,
/// otherwise the pure-Rust port.
#[cfg(feature = "manifold-ffi")]
pub type Solid = backend::ffi::FfiSolid;
/// The default backend's solid: the C++ kernel when `manifold-ffi` is on,
/// otherwise the pure-Rust port.
#[cfg(all(feature = "manifold-rust", not(feature = "manifold-ffi")))]
pub type Solid = backend::port::PortSolid;
