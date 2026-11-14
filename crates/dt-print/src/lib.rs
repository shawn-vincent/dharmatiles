//! dt-print: a built tile on its way to a printer.
//!
//! Everything about printing lives here, above the engine: the print list's
//! plan and the packer that lays tiles on plates ([`plates`]), the inventory
//! of what you have printed ([`inventory`]), and the files a printer or a
//! slicer reads: binary STL ([`stl`]), 3MF painted by material ([`threemf`]),
//! Bambu Studio projects ([`bambu`]), whole plates of either ([`plateset`]).
//! The CPU renderer that draws the four preview PNGs of `build --png` and
//! the contact sheets is here too ([`render`]): its only readers are those
//! two outputs.  The painter that colours a face (`dt_kit::paint_mesh`, the
//! palette) stays in `dt-kit`, because the editor and the browser build show
//! the same colours and neither prints.
//!
//! `dt-eval` reads, plans and builds and returns the product; nothing in it
//! knows this crate exists.

pub mod bambu;
pub mod inventory;
pub mod plates;
pub mod plateset;
pub mod render;
pub mod stl;
pub mod threemf;

pub use bambu::{bambu_project, bed_centre, bed_corners, project_settings};
pub use plateset::{
    PlatePart, PlateSet, PlateStand, bambu_columns, bambu_plate_origin, bambu_plates, box_of,
    filaments_of, plate_3mf, plate_stl, stand_positions,
};
pub use render::{Image, VIEWS, View, render};
pub use stl::stl;
pub use threemf::threemf;
