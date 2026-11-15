//! The mesh header: the JSON that goes with every mesh to the page, read before the arrays (see [`super::payload`]).
//! One type, [`MeshHeader`], made by its constructors and written by serializing it; the page's TypeScript type of the
//! same name is generated from it, so a field renamed here fails `tsc` in the page's readers.

use std::collections::BTreeMap;

/// What a mesh is of: the header is read the same way for each, and this says which it got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum MeshKind {
    /// A finished build of a tile.
    Build,
    /// A draft: a build thinned to [`super::payload::VIEW_BUDGET`].
    Draft,
    /// One piece of a map, at a tier.
    MapPiece,
    /// A sketch of a stage while a build runs.
    Sketch,
    /// An STL file.
    Stl,
    /// A plate of a print list.
    Plates,
}

/// One stack entry of the header: its place (the number the mesh's triangles carry), its key (its id, which survives
/// edits to the text), its words and element, and `z`: the lowest and highest height its triangles reach, mm from the
/// tile's zero (none for an entry that made no triangle).  One reading of the mesh the build made, so the page draws the
/// thickness of each entry from here and works nothing out of the mesh itself.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct MeshStackEntry {
    pub index: usize,
    pub key: String,
    pub label: String,
    pub element: String,
    pub what: String,
    pub z: Option<[f64; 2]>,
}

/// The debug layer's marks as the page draws them, thinned to [`super::payload::DEBUG_DOTS`] and
/// [`super::payload::DEBUG_TRAIL_POINTS`].  The clouds are not here: see [`Clouds`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DebugMarks {
    /// By class: `[x, y, z, …]`.
    pub dots: BTreeMap<String, Vec<f64>>,
    /// By class: polylines, each `[x, y, z, …]`.
    pub trails: BTreeMap<String, Vec<Vec<f64>>>,
    pub thinned: bool,
}

/// A grid of heights a cloud is drawn from, an entry's.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CloudSheet {
    /// An entry's key (`tile/dirt/0`).
    pub owner: String,
    pub class: String,
    pub nx: usize,
    pub ny: usize,
    pub step: f64,
    /// Row by row, `nx` by `ny`: the height, or none where the sheet has no node.
    pub z: Vec<Option<f64>>,
    /// 0 to 1, one for each node.
    pub shade: Vec<f64>,
}

/// The clouds at a point (branch tips, stones) of one entry and class.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct CloudColumns {
    /// An entry's key.
    pub owner: String,
    pub class: String,
    /// `x, y, low, high, rolled, shade low, shade high, shade rolled`, for each column.
    pub v: Vec<f64>,
}

/// The clouds a selected `top=` or pin is drawn from, thinned to [`super::payload::DEBUG_SHEET_NODES`] and
/// [`super::payload::DEBUG_COLUMNS`].  Its own section of the header, apart from the debug layer's marks: the page draws
/// a cloud as part of the selection, whether or not the layer is on.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Clouds {
    pub sheets: Vec<CloudSheet>,
    pub columns: Vec<CloudColumns>,
    pub thinned: bool,
}

/// The JSON header of every mesh (its TypeScript type, `MeshHeader`, is generated from this one).  A field is none only
/// where the kind of mesh has no such data.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct MeshHeader {
    pub kind: MeshKind,
    /// How many triangles the arrays hold (set when the mesh is encoded).
    pub triangles: usize,
    /// The mesh's own box, mm (set when the mesh is encoded).
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    /// The printer's overhang angle; a sketch has none.
    pub overhang_deg: Option<f64>,
    /// The tile's footprint, mm; a sketch has none.
    pub footprint: Option<[f64; 2]>,
    /// Faces the gate painted red, and orange; a build and a draft have them.
    pub overhang_faces: Option<usize>,
    pub band_faces: Option<usize>,
    /// Whether the gate checked what this mesh shows: a low or super-low build, a sketch has not been.
    pub checked: bool,
    /// The gate's failures; empty when it passes or did not check.
    pub failures: Vec<String>,
    /// A draft's mesh was thinned.
    pub thinned: bool,
    /// A sketch is drawn rough.
    pub rough: bool,
    /// The tier: a map piece's name, or a plate's (`full`, `mixed`, `low`).
    pub tier: Option<String>,
    /// The revision of the document a map piece is of.
    pub doc_rev: Option<u64>,
    /// A map piece's own box, mm.
    pub tile_lo: Option<[f64; 3]>,
    pub tile_hi: Option<[f64; 3]>,
    /// The stack the mesh was built from; empty for a mesh made without one.
    pub stack: Vec<MeshStackEntry>,
    pub clouds: Option<Clouds>,
    pub debug: Option<DebugMarks>,
    /// A plate's printer bed, mm, and the zones kept clear, `[x0, y0, x1, y1]`.
    pub bed: Option<[f64; 2]>,
    pub excluded: Option<Vec<[f64; 4]>>,
    /// Which plate is shown, and how many there are.
    pub plate: Option<usize>,
    pub plates: Option<usize>,
}

impl MeshHeader {
    /// A header of `kind` with nothing yet said: a checked mesh with no stack; the box and the count are the encoder's.
    #[must_use]
    pub const fn new(kind: MeshKind) -> Self {
        Self {
            kind,
            triangles: 0,
            lo: [0.0; 3],
            hi: [0.0; 3],
            overhang_deg: None,
            footprint: None,
            overhang_faces: None,
            band_faces: None,
            checked: true,
            failures: Vec::new(),
            thinned: false,
            rough: false,
            tier: None,
            doc_rev: None,
            tile_lo: None,
            tile_hi: None,
            stack: Vec::new(),
            clouds: None,
            debug: None,
            bed: None,
            excluded: None,
            plate: None,
            plates: None,
        }
    }
}
