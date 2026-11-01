//! dharmatiles geometry kernels.
//!
//! Phase 0 skeleton: the [`mesh::Mesh`] and [`rays::Rays`] traits that every
//! backend sits behind, and [`seed::Seed`], the deterministic path-keyed seed
//! derivation. Backends are chosen by feature (`manifold-ffi` by default,
//! `manifold-rust` for wasm) and are wired by the R1 and R3 spikes.

pub mod mesh;
pub mod rays;
pub mod seed;

pub use mesh::{Mesh, MeshError, TriMesh};
pub use rays::{Hit, ObvhsRays, ParryRays, Ray, Rays};
pub use seed::Seed;
