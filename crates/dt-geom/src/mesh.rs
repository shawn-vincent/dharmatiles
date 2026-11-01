//! The mesh trait. Every boolean backend (manifold-csg, manifold-rust)
//! implements it, so young crates stay swappable (risk R10).

/// A plain indexed triangle mesh: the exchange format between backends,
/// export and audit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriMesh {
    /// Vertex positions in mm.
    pub positions: Vec<[f64; 3]>,
    /// Counter-clockwise triangles (outward normals) indexing `positions`.
    pub triangles: Vec<[u32; 3]>,
}

impl TriMesh {
    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }
}

/// Why a backend refused or failed an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshError {
    /// The input is not a closed, consistently oriented 2-manifold.
    NotManifold,
    /// Any other backend-reported failure.
    Backend(String),
}

impl std::fmt::Display for MeshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotManifold => f.write_str("mesh is not a closed 2-manifold"),
            Self::Backend(msg) => write!(f, "mesh backend error: {msg}"),
        }
    }
}

impl std::error::Error for MeshError {}

/// A solid mesh with exact booleans. Implemented once per backend feature.
pub trait Mesh: Sized {
    /// Build a solid from an indexed triangle mesh.
    ///
    /// # Errors
    /// [`MeshError::NotManifold`] if the input is not a closed 2-manifold.
    fn from_tri_mesh(mesh: &TriMesh) -> Result<Self, MeshError>;

    /// Export back to an indexed triangle mesh.
    fn to_tri_mesh(&self) -> TriMesh;

    /// `self ∪ other`.
    #[must_use]
    fn union(&self, other: &Self) -> Self;

    /// `self − other`.
    #[must_use]
    fn difference(&self, other: &Self) -> Self;

    /// `self ∩ other`.
    #[must_use]
    fn intersection(&self, other: &Self) -> Self;

    /// Enclosed volume in mm³.
    fn volume(&self) -> f64;

    /// Number of triangles.
    fn triangle_count(&self) -> usize;
}
