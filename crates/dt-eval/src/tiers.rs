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
    let (m, kept, _) = thinned_snap(mesh, budget);
    (m, kept)
}

/// [`thinned`], and the vertex each original vertex was snapped to.
fn thinned_snap(mesh: &TriMesh, budget: usize) -> (TriMesh, Vec<usize>, Vec<u32>) {
    let n = mesh.triangles.len();
    if n <= budget {
        let identity = (0..u32::try_from(mesh.positions.len()).unwrap_or(u32::MAX)).collect();
        return (mesh.clone(), (0..n).collect(), identity);
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
            return (out, kept, snap);
        }
        cell *= 1.4;
    }
}

/// `b` with its mesh thinned to `budget` ([`thinned`]) and every
/// per-triangle list kept to match.
#[must_use]
pub fn thin_built(mut b: Built, budget: usize) -> Built {
    let (mesh, kept, snap) = thinned_snap(&b.export.mesh, budget);
    if kept.len() == b.export.mesh.triangles.len() {
        return b;
    }
    let old = &b.export.mesh;
    // Every original triangle votes, by its area, for the kept triangle that
    // stands where it was (a kept triangle at its first vertex's cluster, the
    // one it faces most alike).  A kept triangle takes the material, owner
    // and overhang flag most of the area under it had, so a sliver of
    // geometry never makes a face of its own: nothing is flagged or coloured
    // that the low build did not have there.
    let at = |m: &TriMesh, t: &[u32; 3]| {
        let [p, q, r] = t.map(|i| m.positions[i as usize]);
        let (u, v) = (
            [q[0] - p[0], q[1] - p[1], q[2] - p[2]],
            [r[0] - p[0], r[1] - p[1], r[2] - p[2]],
        );
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let mut touching: std::collections::HashMap<u32, Vec<usize>> = std::collections::HashMap::new();
    for (k, t) in mesh.triangles.iter().enumerate() {
        for v in t {
            touching.entry(*v).or_default().push(k);
        }
    }
    let nk = kept.len();
    let mut over = vec![0.0f64; nk];
    let mut band = vec![0.0f64; nk];
    let mut total = vec![0.0f64; nk];
    // area by (material, entry, piece) per kept triangle
    type Key = (dt_kit::Material, u16, u16);
    let mut owners: Vec<Vec<(Key, f64)>> = vec![Vec::new(); nk];
    for (f, t) in old.triangles.iter().enumerate() {
        let cross = at(old, t);
        let area = 0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        let Some(cands) = touching.get(&snap[t[0] as usize]) else {
            continue;
        };
        let best = cands
            .iter()
            .copied()
            .max_by(|&x, &y| {
                let align = |k: usize| {
                    let c = at(&mesh, &mesh.triangles[k]);
                    c[0] * cross[0] + c[1] * cross[1] + c[2] * cross[2]
                };
                align(x).total_cmp(&align(y))
            })
            .unwrap_or(cands[0]);
        total[best] += area;
        if b.audit.overhang[f] {
            over[best] += area;
        } else if b.audit.band[f] {
            band[best] += area;
        }
        let key = (b.materials[f], b.entries[f], b.pieces[f]);
        match owners[best].iter_mut().find(|(k, _)| *k == key) {
            Some((_, a)) => *a += area,
            None => owners[best].push((key, area)),
        }
    }
    let mut materials = Vec::with_capacity(nk);
    let mut entries = Vec::with_capacity(nk);
    let mut pieces = Vec::with_capacity(nk);
    for (k, &f) in kept.iter().enumerate() {
        // a kept triangle nothing voted for keeps its own
        let own = (b.materials[f], b.entries[f], b.pieces[f]);
        let (m, e, p) = owners[k]
            .iter()
            .max_by(|x, y| x.1.total_cmp(&y.1))
            .map_or(own, |o| o.0);
        materials.push(m);
        entries.push(e);
        pieces.push(p);
    }
    let flagged = |w: &[f64]| -> Vec<bool> {
        (0..nk)
            .map(|k| total[k] > 0.0 && w[k] > 0.5 * total[k])
            .collect()
    };
    let overhang = flagged(&over);
    let band: Vec<bool> = flagged(&band)
        .into_iter()
        .zip(&overhang)
        .map(|(b, o)| b && !o)
        .collect();
    b.export.source = kept.iter().map(|&f| b.export.source[f]).collect();
    b.export.mesh = mesh;
    b.materials = materials;
    b.audit.overhang = overhang;
    b.audit.band = band;
    b.entries = entries;
    b.pieces = pieces;
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
