//! The tiers of a build: **full** (the union and the gate), **low** (the
//! same places drawn with less detail, [`Options::low`]) and **super-low**
//! (a low build cut down to a few thousand triangles, [`super_low`]).
//!
//! Only a full build is checked: a low or super-low build says so
//! ([`Built::checked`]) and is never printed or exported.  Each tier is its
//! own entry in the cache ([`crate::Tier`]).

// cell numbers are small whole numbers
#![allow(clippy::cast_possible_truncation)]

use dt_kit::TriMesh;

use crate::Built;

/// The most triangles a low build keeps: a soup of parts drawn with little
/// detail is a few tens of thousands; one above this (a cliff) is thinned to
/// it.
pub const LOW_BUDGET: usize = 100_000;

/// The most triangles a super-low build has.
pub const SUPER_LOW_BUDGET: usize = 12_000;

/// `mesh` with its vertices snapped together on a grid coarse enough to
/// leave at most `budget` triangles, and the index in `mesh` of each
/// triangle kept.  A cheap stand-in for a view of a draft, a sketch or a
/// low build: it keeps each kept triangle's own colour and owner, and is
/// never printed.
#[must_use]
pub fn thinned(mesh: &TriMesh, budget: usize) -> (TriMesh, Vec<usize>) {
    let n = mesh.triangles.len();
    if n <= budget {
        return (mesh.clone(), (0..n).collect());
    }
    // a first cell from the surface area: about two triangles per cell face
    let area: f64 = mesh
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let x = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            0.5 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt()
        })
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let mut cell = (2.0 * area / budget as f64).sqrt().max(1e-3);
    loop {
        let mut of = std::collections::HashMap::with_capacity(mesh.positions.len() / 2);
        let mut positions = Vec::new();
        let snap: Vec<u32> = mesh
            .positions
            .iter()
            .map(|p| {
                let k = p.map(|c| (c / cell).floor() as i64);
                *of.entry(k).or_insert_with(|| {
                    positions.push(*p);
                    u32::try_from(positions.len() - 1).unwrap_or(u32::MAX)
                })
            })
            .collect();
        let mut triangles = Vec::new();
        let mut kept = Vec::new();
        for (f, t) in mesh.triangles.iter().enumerate() {
            let [a, b, c] = t.map(|i| snap[i as usize]);
            if a != b && b != c && a != c {
                triangles.push([a, b, c]);
                kept.push(f);
            }
        }
        if triangles.len() <= budget {
            let out = TriMesh {
                positions,
                triangles,
                ..TriMesh::default()
            };
            return (out, kept);
        }
        cell *= 1.4;
    }
}

/// `b` with its mesh thinned to `budget` ([`thinned`]) and every
/// per-triangle list kept to match.
#[must_use]
pub fn thin_built(mut b: Built, budget: usize) -> Built {
    let (mesh, kept) = thinned(&b.export.mesh, budget);
    if kept.len() == b.export.mesh.triangles.len() {
        return b;
    }
    fn pick<T: Copy>(v: &[T], kept: &[usize]) -> Vec<T> {
        kept.iter().map(|&f| v[f]).collect()
    }
    b.export.source = pick(&b.export.source, &kept);
    b.export.mesh = mesh;
    b.materials = pick(&b.materials, &kept);
    b.audit.overhang = pick(&b.audit.overhang, &kept);
    b.audit.band = pick(&b.audit.band, &kept);
    b.entries = pick(&b.entries, &kept);
    b.pieces = pick(&b.pieces, &kept);
    b.audit.triangles = b.export.mesh.triangles.len();
    b.audit.vertices = b.export.mesh.positions.len();
    b
}

/// A low build cut down to [`SUPER_LOW_BUDGET`] triangles: what the library
/// ships as its previews and the map draws from far.  Each kept triangle
/// keeps its colour, its owner and its overhang flag; nothing is checked.
#[must_use]
pub fn super_low(low: Built) -> Built {
    thin_built(low, SUPER_LOW_BUDGET)
}
