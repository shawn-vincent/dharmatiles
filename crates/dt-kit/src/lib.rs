//! dt-kit: the vocabulary looks are written in.
//!
//! The scene of tagged solids ([`Scene`]), the operations, the audit gate,
//! the STL writer with its build record, and the CPU preview renderer.
//! Looks see only this crate's public API (docs/rust/architecture.html,
//! "Six crates"), so a look cannot reach into the kernels or into another look.
//!
//! Public operations (budget: about 22 over the whole project; P1 had 12,
//! P2 adds 5: noise, distance, morphology, scatter and the cap, and makes
//! paint take fields and lift adaptive):
//!
//! | op | what it does |
//! |---|---|
//! | [`area`] | paint bucket: drawn lines and fill points → an area |
//! | [`Scene::paint`] | set the ground surface over an area |
//! | [`Scene::lift`] | the painted surface → a solid (adaptive lift, error-bounded, edge contract held) |
//! | [`noise`] | position-keyed noise over the tile (one lattice) |
//! | [`distance`] | distance transform with nearest nodes |
//! | [`Field`] morphology | blur, dilate, erode, range, quantile, slope, gradient; [`smax`] |
//! | [`scatter`] | things by density, largest first, with spacing |
//! | [`cap`] | carve as a half-space cap over the ground (pebbles) |
//! | [`loft`] | a closed solid through rings |
//! | [`extrude`] | an outline → a prism |
//! | [`carve`] | a solid less cutters |
//! | [`union`] | solids combined in slot order |
//! | [`Scene::top`] | query: highest z of every solid so far |
//! | [`Scene::union`] | the whole scene, exported once, with materials |
//! | [`audit`] | the printability gate |
//! | [`stl`] | binary STL with the build record |
//! | [`render`] | a painted preview |
//!
//! Kernel rules (R1, R2, R9, from `dt_geom::source`): every leaf solid takes
//! a [`Slot`] reserved in sequence from the scene's block, so its source ID
//! is fixed by index whatever thread builds it; cut faces take the
//! material of the solid cut (cutters get slots of that material); parts
//! are combined with `batch_in_order`; solids stay in the kernel until
//! [`Scene::union`].

mod audit;
mod field;
mod outline;
mod render;
mod scene;
mod shape;
mod stl;
mod stone;

pub use audit::{Audit, Dropped, Export, Printer, audit};
pub use dt_geom::Convex;
pub use dt_geom::{Grid, Mask, Rng, Seed, TriMesh, vec3};
pub use field::{
    Field, Noise, Placed, Plane, Scale, Spread, cap, distance, geodesic, noise, rim, scatter, smax,
    smooth01,
};
pub use outline::Outline;
pub use render::{Image, RED, SurfacePaint, VIEWS, View, paint, paint_surface, render};
pub use scene::{
    BANK_MM, Frame, LIFT_TOL_MM, Material, Part, REFERENCE_SQUARE_MM, Scene, Slot, Slots, Solid,
    Stage, Tag,
};
pub use shape::{area, carve, extrude, loft, signed_volume, union, wobble};
pub use stl::{BuildRecord, stl};
pub use stone::{Grain, grain, hull, seat, solid};

/// Why a kit operation failed.
#[derive(Clone, Debug, PartialEq)]
pub enum KitError {
    /// The mesh kernel refused or failed.
    Mesh(dt_geom::MeshError),
    /// The ground surface cannot be lifted.
    Surface(String),
    /// A shape's inputs are wrong.
    Shape(String),
    /// More leaves than the build's source-ID block holds.
    Slots,
    /// A triangle of the union traces to no material (R9).
    Untraced,
}

impl std::fmt::Display for KitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mesh(e) => write!(f, "{e}"),
            Self::Surface(e) => write!(f, "ground surface: {e}"),
            Self::Shape(e) => f.write_str(e),
            Self::Slots => f.write_str("too many solids for one build"),
            Self::Untraced => f.write_str("a triangle of the union has no material"),
        }
    }
}

impl std::error::Error for KitError {}
