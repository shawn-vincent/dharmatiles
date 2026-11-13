//! Operations for stones: the convex hull of points, the grain pass that
//! roughens a convex stone, the seat that beds a
//! stone into the ground, and a solid from a stone's mesh.
//!
//! Facets and chips are half-space cuts of a [`dt_geom::Convex`], so a
//! stone stays one closed convex polytope until the grain pass; the grain
//! moves vertices only, along their normals, so the result is the same
//! closed mesh with the same triangles.

use dt_geom::vec3::{cross, dot, norm, sub, unit};
use dt_geom::{Convex, TriMesh};

use crate::piece::welded;
use crate::scene::{Slot, Solid, leaf};
use crate::{KitError, smooth};

/// **hull**: the convex hull of `points` (mm), as a closed mesh, made by
/// Manifold's kernel in f64.  [`crate::form::hull`] is another function: it
/// hulls with parry3d in f32 about the points' centre, and the two do not
/// give the same vertices, so a form picks the one whose numbers its hashes
/// were made with.
///
/// # Errors
/// [`KitError::Shape`] when the points span no volume.
pub fn hull(points: &[[f64; 3]]) -> Result<TriMesh, KitError> {
    let m = dt_geom::Solid::hull(points).to_tri_mesh();
    if m.triangles.len() < 4 {
        return Err(KitError::Shape(
            "a hull needs points that span a volume".into(),
        ));
    }
    Ok(m)
}

/// How [`grain`] roughens a stone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grain {
    /// Refine to edges no longer than this, mm (`TEX_EDGE_MM`).
    pub edge_mm: f64,
    /// fBm scale, mm (`GRAIN_MM`).
    pub scale_mm: f64,
    /// fBm octaves.
    pub octaves: u32,
    /// Displacement amplitude, mm (`GRAIN_AMP_MM`).
    pub amp_mm: f64,
    /// Edges sharper than this (degrees) stay crisp (`SHARP_EDGE_DEG`).
    pub sharp_deg: f64,
    /// Within this of a sharp edge the grain is halved, mm (`EDGE_KEEP_MM`).
    pub keep_mm: f64,
    /// The grain starts this high above the base, mm.
    pub base_mm: f64,
    /// and fades in over this, mm (`TEX_BASE_FADE_MM`).
    pub fade_mm: f64,
    /// No grain on faces whose normal z is at or below this.
    pub down_nz: f64,
}

impl Default for Grain {
    fn default() -> Self {
        Self {
            edge_mm: 0.25,
            scale_mm: 0.45,
            octaves: 4,
            amp_mm: 0.12,
            sharp_deg: 18.0,
            keep_mm: 0.5,
            base_mm: 0.15,
            fade_mm: 0.8,
            down_nz: -0.3,
        }
    }
}

/// **grain**: refine a convex stone to `edge_mm` and push every vertex
/// along its normal by fBm grain (`seed`), halved near sharp edges, faded
/// in above the base and left off down-facing faces, so the footprint
/// stays seated and the ridges stay crisp. The stone's base is at z = 0.
///
/// # Errors
/// [`KitError::Mesh`] if the kernel refuses the stone.
pub fn grain(stone: &Convex, g: &Grain, seed: u64) -> Result<TriMesh, KitError> {
    let sharp: Vec<[[f64; 3]; 2]> = stone
        .edges()
        .into_iter()
        .filter(|e| e.angle > g.sharp_deg.to_radians())
        .map(|e| e.ends)
        .collect();
    let base = dt_geom::Solid::from_tri_mesh(&stone.to_tri_mesh()).map_err(KitError::Mesh)?;
    let fine = base.refine_to_length(g.edge_mm).to_tri_mesh();
    let (p, tri) = welded(&fine);
    // angle-weighted vertex normals
    let mut n = vec![[0.0; 3]; p.len()];
    for t in &tri {
        let v = t.map(|i| p[i as usize]);
        let fnrm = unit(cross(sub(v[1], v[0]), sub(v[2], v[0])));
        for k in 0..3 {
            let (a, b) = (sub(v[(k + 1) % 3], v[k]), sub(v[(k + 2) % 3], v[k]));
            let c = dot(unit(a), unit(b)).clamp(-1.0, 1.0);
            let w = libm::acos(c);
            let o = &mut n[t[k] as usize];
            for d in 0..3 {
                o[d] += w * fnrm[d];
            }
        }
    }
    let moved: Vec<[f64; 3]> = p
        .iter()
        .zip(&n)
        .map(|(&q, &nv)| {
            let nv = unit(nv);
            let gr = dt_geom::noise::fbm3(seed ^ 0x5A5A, g.scale_mm, g.octaves, q);
            let dmin = sharp
                .iter()
                .map(|[a, b]| {
                    let ab = sub(*b, *a);
                    let t = (dot(sub(q, *a), ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
                    let c = [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]];
                    norm(sub(q, c))
                })
                .fold(f64::INFINITY, f64::min);
            let we = if sharp.is_empty() {
                1.0
            } else {
                smooth(dmin / g.keep_mm)
            };
            let w = if nv[2] > g.down_nz {
                smooth((q[2] - g.base_mm) / g.fade_mm)
            } else {
                0.0
            };
            let s = g.amp_mm * gr * (0.5 + 0.5 * we) * w;
            [q[0] + s * nv[0], q[1] + s * nv[1], q[2] + s * nv[2]]
        })
        .collect();
    Ok(TriMesh {
        positions: moved,
        triangles: tri,
        ..TriMesh::default()
    })
}

/// **solid**: a stone's closed mesh as a solid in `slot` (rule 2).
///
/// # Errors
/// [`KitError::Mesh`] if the mesh is not a closed manifold.
pub fn solid(slot: Slot, mesh: &TriMesh) -> Result<Solid, KitError> {
    leaf(slot, mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grain_keeps_a_closed_stone_on_its_base() {
        let c = Convex::cuboid([-3.0, -3.0, 0.0], [3.0, 3.0, 4.0])
            .cut([0.6, 0.0, 0.8], 2.5)
            .unwrap();
        let m = grain(&c, &Grain::default(), 7).unwrap();
        assert!(m.is_watertight());
        let zmin = m
            .positions
            .iter()
            .map(|p| p[2])
            .fold(f64::INFINITY, f64::min);
        assert!(zmin.abs() < 1e-9, "zmin {zmin}");
        let v = crate::signed_volume(&m);
        assert!(
            (v - c.volume()).abs() < 0.05 * c.volume(),
            "{v} {}",
            c.volume()
        );
        // grain moved something
        let moved = m.positions.iter().any(|p| {
            (p[0].abs() - 3.0).abs() > 1e-6 && p[2] > 1.0 && p[2] < 3.0 && p[1].abs() < 2.9
        });
        assert!(moved);
        // same seed, same stone
        assert_eq!(m, grain(&c, &Grain::default(), 7).unwrap());
    }
}
