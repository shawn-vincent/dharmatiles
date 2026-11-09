//! dt-kit: the vocabulary elements are written in.
//!
//! The scene of tagged solids ([`Scene`]), the operations, the audit gate,
//! the STL writer with its build record, and the CPU preview renderer.
//! Elements see only this crate's public API, so an element cannot reach
//! into the kernels or into another element.
//!
//! The public operations:
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
//! Also the forms ([`form`]: level sets, hulls, solids from meshes,
//! box clips, the common stone relief and 3D noise) that walls, floors
//! and trees are made of; graphs ([`graph`]: trace by space
//! colonisation, loft tubes along a graph with C1 forks); [`stand`] (cut
//! away what cannot print standing on what is below it: the audit's
//! layer-reach rule as a construction); and [`rays`] (first hits and
//! containment over dt-geom's ray trait, for the leaf placer).
//!
//! Kernel rules (from `dt_geom::source`): every leaf solid takes
//! a [`Slot`] reserved in sequence from the scene's block, so its source ID
//! is fixed by index whatever thread builds it; cut faces take the
//! material of the solid cut (cutters get slots of that material); parts
//! are combined with `batch_in_order`; solids stay in the kernel until
//! [`Scene::union`].

mod audit;
mod bambu;
mod band;
mod field;
pub mod form;
pub mod graph;
mod land;
mod outline;
mod paint;
mod piece;
pub mod pieces;
pub mod progress;
pub mod rays;
mod render;
mod scene;
mod shape;
pub mod span;
mod stand;
mod stl;
mod stone;
mod threemf;

pub use audit::{
    Audit, Dropped, Export, Hang, HangBand, Printer, audit, audit_banded, audit_rounded, draft,
    fit, hang,
};
pub use bambu::{bambu_project, bed_centre, project_settings};
pub use band::{
    Band, Banded, Ease, Lint, Pin, Refusal, Shadowed, Solved, Steep, drop_mm, lints, solve,
    solve_traced,
};
pub use dt_geom::Convex;
pub use dt_geom::{Grid, Iv, Mask, Rng, Seed, TriMesh, vec3};
pub use dt_geom::{Structuring, distance_nearest};
pub use field::{
    Field, Noise, OnFrame, Placed, Plane, Scale, Spread, cap, distance, geodesic, noise, noise_at,
    rim, scatter, smax, smooth01,
};
pub use land::{Disc, Land, STEEPEST_GROUND_DEG, Seat, SeatRule, THINGS, seat_on};
pub use outline::Outline;
pub use paint::paint_mesh;
pub use piece::{Piece, Support, hung, prism_mesh, tri_normal, welded};
pub use progress::{Counter, Event, Progress, Reporter};
pub use render::{Image, ORANGE, RED, SurfacePaint, VIEWS, View, paint, paint_surface, render};
pub use scene::{
    ABSENT_UNDER_MM, CORNER_ROUND_MM, Frame, Kind, LIFT_TOL_MM, LayerSpec, Material, Part, Parts,
    REFERENCE_SQUARE_MM, Relief, Rest, Scene, SideSpec, Slot, Slots, Solid, Stage, Standing, Tag,
    Terrain, Under,
};
pub use shape::{area, carve, extrude, loft, signed_volume, union, wobble};
pub use stand::{Stand, Stood, stand};
pub use stl::{BuildRecord, stl, target_name};
pub use stone::{Grain, grain, hull, solid};
pub use threemf::{Palette, RAMPS, Ramp, threemf};

/// Why a kit operation failed.
#[derive(Clone, Debug, PartialEq)]
pub enum KitError {
    /// The mesh kernel refused or failed.
    Mesh(dt_geom::MeshError),
    /// The ground surface cannot be lifted.
    Lift(String),
    /// A shape's inputs are wrong.
    Shape(String),
    /// More leaves than the build's source-ID block holds.
    Slots,
    /// A triangle of the union traces to no material.
    Untraced,
}

impl std::fmt::Display for KitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mesh(e) => write!(f, "{e}"),
            Self::Lift(e) => write!(f, "lifting the terrain: {e}"),
            Self::Shape(e) => f.write_str(e),
            Self::Slots => f.write_str("too many solids for one build"),
            Self::Untraced => f.write_str("a triangle of the union has no material"),
        }
    }
}

impl std::error::Error for KitError {}
