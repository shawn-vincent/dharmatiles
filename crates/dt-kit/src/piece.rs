//! **Piece**: one solid worked in the kernel by a series of cuts.
//!
//! A cliff stone is made by many operations (plane cleaves, chips, a
//! relief of cutters, a seat), each producing new faces. A `Piece` owns
//! one source slot, and everything it is cut or joined with takes that
//! slot's original ID and a fresh run of the slot's face IDs, so the cut
//! faces trace to the stone's material and the result is the same
//! whatever thread works it. Pieces are combined at the end with
//! [`Piece::into_solid`] and [`crate::union`].

use dt_geom::vec3::{cross, dot, sub, unit};
use dt_geom::{BoolOp, TriMesh};

use crate::scene::{Slot, Solid};
use crate::{Field, KitError};

/// A solid under construction, in one slot.
#[derive(Clone)]
pub struct Piece {
    s: dt_geom::Solid,
    id: u32,
}

/// What a cut must not undercut: a union of meshes, asked only for volume.
#[derive(Clone)]
pub struct Support(dt_geom::Solid);

impl Support {
    /// The union of `meshes` (closed, outward). Empty list: nothing.
    ///
    /// # Errors
    /// [`KitError::Mesh`] if a mesh is not a closed manifold.
    pub fn new(meshes: &[TriMesh]) -> Result<Self, KitError> {
        let parts: Result<Vec<dt_geom::Solid>, _> =
            meshes.iter().map(dt_geom::Solid::from_tri_mesh).collect();
        let parts = parts.map_err(KitError::Mesh)?;
        if parts.is_empty() {
            return Ok(Self(empty()));
        }
        Ok(Self(dt_geom::Solid::batch(&parts, BoolOp::Union)))
    }
}

impl Piece {
    fn lift(&self, m: &TriMesh) -> Result<dt_geom::Solid, KitError> {
        let s = dt_geom::Solid::from_tri_mesh(m).map_err(KitError::Mesh)?;
        s.with_original_id(self.id).map_err(KitError::Mesh)
    }

    fn with(&self, s: dt_geom::Solid) -> Self {
        Self { s, id: self.id }
    }

    /// A piece from a closed mesh, in `slot`.
    ///
    /// # Errors
    /// [`KitError::Mesh`] if the mesh is not a closed manifold.
    pub fn new(slot: Slot, m: &TriMesh) -> Result<Self, KitError> {
        let mut p = Self {
            s: empty(),
            id: slot.id,
        };
        p.s = p.lift(m)?;
        Ok(p)
    }

    /// The polygon `contour` (counter-clockwise) extruded from `z0` to `z1`.
    ///
    /// # Errors
    /// [`KitError::Mesh`] on a degenerate polygon.
    pub fn prism(slot: Slot, contour: &[[f64; 2]], z0: f64, z1: f64) -> Result<Self, KitError> {
        let s = dt_geom::Solid::extrude(&[contour.to_vec()], 0.0, z0, z1);
        s.status().map_err(KitError::Mesh)?;
        Self::new(slot, &s.to_tri_mesh())
    }

    /// Nothing left.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.s.triangle_count() == 0
    }

    /// Enclosed volume, mm³.
    #[must_use]
    pub fn volume(&self) -> f64 {
        self.s.volume()
    }

    /// Axis-aligned bounds, or `None` when empty.
    #[must_use]
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        self.s.bounds().map(|b| (b.min, b.max))
    }

    /// Its surface.
    #[must_use]
    pub fn mesh(&self) -> TriMesh {
        self.s.to_tri_mesh()
    }

    /// Cross-section at height `z` (closed polygons).
    #[must_use]
    pub fn slice(&self, z: f64) -> Vec<Vec<[f64; 2]>> {
        self.s.slice(z)
    }

    /// The part where `n · x <= off` (the part `n · x > off` taken).
    /// Cut faces take the piece's slot.
    ///
    /// # Errors
    /// [`KitError::Mesh`] if the kernel fails.
    pub fn keep(&self, n: [f64; 3], off: f64) -> Result<Self, KitError> {
        let r = self
            .s
            .trim_by_plane_with_id([-n[0], -n[1], -n[2]], -off, self.id)
            .map_err(KitError::Mesh)?;
        Ok(self.with(r))
    }

    /// This less every cutter mesh (each closed, outward).
    ///
    /// # Errors
    /// [`KitError::Mesh`] if a cutter is not a closed manifold.
    pub fn minus(&self, cutters: &[TriMesh]) -> Result<Self, KitError> {
        if cutters.is_empty() {
            return Ok(self.clone());
        }
        let me = self.clone();
        let parts: Result<Vec<_>, _> = cutters.iter().map(|m| me.lift(m)).collect();
        let parts = parts?;
        let c = dt_geom::Solid::batch(&parts, BoolOp::Union);
        Ok(me.with(self.s.difference(&c)))
    }

    /// This joined with `m`.
    ///
    /// # Errors
    /// [`KitError::Mesh`] if `m` is not a closed manifold.
    pub fn plus(&self, m: &TriMesh) -> Result<Self, KitError> {
        let me = self.clone();
        let c = me.lift(m)?;
        Ok(me.with(self.s.union(&c)))
    }

    /// This within `m`.
    ///
    /// # Errors
    /// [`KitError::Mesh`] if `m` is not a closed manifold.
    pub fn within(&self, m: &TriMesh) -> Result<Self, KitError> {
        let me = self.clone();
        let c = me.lift(m)?;
        Ok(me.with(self.s.intersection(&c)))
    }

    /// Volume of what `other` (a part of this piece's earlier self) takes
    /// from this that lies in `support`: a cut's undercut of what rests on it.
    #[must_use]
    pub fn undercut(&self, kept: &Self, support: &Support) -> f64 {
        self.s.difference(&kept.s).intersection(&support.0).volume()
    }

    /// Linear map and shift (row-major 3x3, then translation), as
    /// [`dt_geom::Solid::transform`].
    #[must_use]
    pub fn transformed(&self, m: &[f64; 12]) -> Self {
        self.with(self.s.transform(m))
    }

    /// Welded vertices and triangles of its surface refined to `edge_mm`.
    #[must_use]
    pub fn refined(&self, edge_mm: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        welded(&self.s.refine_to_length(edge_mm).to_tri_mesh())
    }

    /// A piece of the same slot from welded vertices and triangles.
    ///
    /// # Errors
    /// [`KitError::Mesh`] if the surface is not a closed manifold.
    pub fn rebuilt(&self, pos: Vec<[f64; 3]>, tri: Vec<[u32; 3]>) -> Result<Self, KitError> {
        let m = TriMesh {
            positions: pos,
            triangles: tri,
            ..TriMesh::default()
        };
        let s = self.lift(&m)?;
        Ok(self.with(s))
    }

    /// This with its own cross-section at `z_slice` extruded from `z0` to
    /// `z1` (a seat sunk into its bed).
    ///
    /// # Errors
    /// [`KitError::Mesh`] if the kernel fails.
    pub fn pad(&self, z_slice: f64, z0: f64, z1: f64) -> Result<Self, KitError> {
        let cs = self.s.slice(z_slice);
        if cs.is_empty() {
            return Ok(self.clone());
        }
        let m = prism_mesh(&cs, z0, z1);
        self.plus(&m)
    }

    /// Only its connected parts of at least `min_mm3` (and always the
    /// largest): what a cut left that is more than a scrap may still be
    /// holding up the stone above it.
    ///
    /// # Errors
    /// [`KitError::Mesh`] if the surface cannot be rebuilt.
    pub fn parts_over(&self, min_mm3: f64) -> Result<Self, KitError> {
        let (pos, tri) = welded(&self.s.to_tri_mesh());
        if tri.is_empty() {
            return Ok(self.clone());
        }
        let mut par: Vec<usize> = (0..pos.len()).collect();
        fn find(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        for t in &tri {
            for k in 1..3 {
                let (a, b) = (find(&mut par, t[0] as usize), find(&mut par, t[k] as usize));
                par[a] = b;
            }
        }
        let mut vol: std::collections::BTreeMap<usize, f64> = std::collections::BTreeMap::new();
        let mut count: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
        for t in &tri {
            let r = find(&mut par, t[0] as usize);
            let [a, b, c] = t.map(|i| pos[i as usize]);
            let v = dot(a, cross(b, c)) / 6.0;
            *vol.entry(r).or_default() += v;
            *count.entry(r).or_default() += 1;
        }
        if vol.len() < 2 {
            return Ok(self.clone());
        }
        let best = *vol
            .iter()
            .max_by(|x, y| x.1.abs().total_cmp(&y.1.abs()))
            .map(|x| x.0)
            .unwrap_or(&0);
        let big: Vec<usize> = vol
            .iter()
            .filter(|(k, v)| {
                **k == best
                    || v.abs()
                        >= min_mm3
                            .min(0.05 * vol.get(&best).map_or(0.0, |b| b.abs()))
                            .max(0.0)
            })
            .map(|(k, _)| *k)
            .collect();
        let keep: Vec<[u32; 3]> = tri
            .iter()
            .copied()
            .filter(|t| big.contains(&find(&mut par, t[0] as usize)))
            .collect();
        self.rebuilt(pos, keep)
    }

    /// As a solid for the scene.
    #[must_use]
    pub fn into_solid(self) -> Solid {
        Solid(self.s)
    }
}

/// The mesh with split vertices merged: one position per point.
#[must_use]
pub fn welded(m: &TriMesh) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let mut to: Vec<u32> = (0..u32::try_from(m.positions.len()).unwrap_or(u32::MAX)).collect();
    for (&f, &t) in m.merge_from.iter().zip(&m.merge_to) {
        to[f as usize] = t;
    }
    // a vertex merged into one that is itself merged: follow to the end
    // (the kernel names final targets, so this is a guard, not a change)
    for i in 0..to.len() {
        let mut t = to[i];
        while to[t as usize] != t {
            t = to[t as usize];
        }
        to[i] = t;
    }
    let mut new = vec![u32::MAX; m.positions.len()];
    let mut pos = Vec::new();
    for (i, &t) in to.iter().enumerate() {
        if t as usize == i {
            new[i] = u32::try_from(pos.len()).unwrap_or(u32::MAX);
            pos.push(m.positions[i]);
        }
    }
    let tri = m
        .triangles
        .iter()
        .map(|t| t.map(|v| new[to[v as usize] as usize]))
        .collect();
    (pos, tri)
}

/// **hung**: the solid between z = 0 and the surface `depth` (row-major,
/// `nx` by `ny` nodes `step` mm apart, every value above 0), as a mesh,
/// triangulated adaptively within `tol` mm. The cliff's relief cutters
/// and its seats are such height fields, moved into place by the caller.
///
/// # Errors
/// [`KitError::Lift`] when the grid cannot be lifted.
#[allow(clippy::cast_possible_truncation)]
pub fn hung(depth: &Field, tol: f64) -> Result<TriMesh, KitError> {
    let z: Vec<f32> = depth.z.iter().map(|&d| d.max(1e-3) as f32).collect();
    let lat = dt_geom::Lattice {
        nx: depth.nx,
        ny: depth.ny,
        step: depth.step,
        tol,
        max_cells: usize::MAX,
        round: dt_geom::Round::None,
    };
    dt_geom::lift_stack(&lat, &[dt_geom::Layer::new(&z, 0)], &dt_geom::Sides::flat())
        .map_err(|e| KitError::Lift(e.to_string()))
}

/// Unit normal of a triangle.
#[must_use]
pub fn tri_normal(p: &[[f64; 3]], t: [u32; 3]) -> [f64; 3] {
    let [a, b, c] = t.map(|i| p[i as usize]);
    unit(cross(sub(b, a), sub(c, a)))
}

fn empty() -> dt_geom::Solid {
    dt_geom::Solid::from_tri_mesh(&TriMesh::default()).unwrap_or_else(|_| dt_geom::Solid::hull(&[]))
}

/// A prism of closed polygons (even-odd) from `z0` to `z1`, as a mesh.
#[must_use]
pub fn prism_mesh(contours: &[Vec<[f64; 2]>], z0: f64, z1: f64) -> TriMesh {
    dt_geom::Solid::extrude(contours, 0.0, z0, z1).to_tri_mesh()
}
