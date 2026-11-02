//! R9 spike: carry source (material) IDs through one union and one level
//! set, on each backend, and check every output triangle maps to a source.
//!
//! ```text
//! cargo run --release -p dt-geom --features manifold-rust --example r9_materials
//! ```
//!
//! Union path: two solids made originals (`as_original`) are unioned; each
//! output triangle's run original ID must be one of the two, and the
//! triangle must lie on that source's surface.
//!
//! Level-set path: a level set has no materials of its own (one original
//! ID for the whole mesh), so the material field is co-sampled: each
//! level-set triangle takes the material of the nearer blob at its
//! centroid. The level set is then unioned with a box, and each output
//! triangle is traced back through (original ID, face ID) to its level-set
//! source triangle and that triangle's material.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::float_cmp
)]

use std::collections::HashMap;

use dt_geom::{Bounds, Mesh, TriMesh};

fn centroid(m: &TriMesh, t: usize) -> [f64; 3] {
    let [a, b, c] = m.triangles[t].map(|i| m.positions[i as usize]);
    [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0)
}

fn box_mesh(lo: [f64; 3], hi: [f64; 3]) -> TriMesh {
    let p = |i: usize| {
        [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ]
    };
    TriMesh {
        positions: (0..8).map(p).collect(),
        triangles: vec![
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ],
        ..TriMesh::default()
    }
}

/// Distance from `p` to the surface of the axis-aligned box `lo..hi`.
fn box_surface_dist(p: [f64; 3], lo: [f64; 3], hi: [f64; 3]) -> f64 {
    let mut out = 0.0f64;
    let mut inside = f64::INFINITY;
    for k in 0..3 {
        let d = (lo[k] - p[k]).max(p[k] - hi[k]);
        out += d.max(0.0).powi(2);
        inside = inside.min((p[k] - lo[k]).min(hi[k] - p[k]));
    }
    if out > 0.0 { out.sqrt() } else { inside }
}

fn spheres(p: [f64; 3]) -> (f64, u32) {
    // two blobs: material 1 at (-3,0,0) r 4, material 2 at (3,0,0) r 3.5
    let d = |c: [f64; 3], r: f64| {
        r - ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2)).sqrt()
    };
    let (a, b) = (d([-3.0, 0.0, 0.0], 4.0), d([3.0, 0.0, 0.0], 3.5));
    // smooth union (positive inside), material = the larger field
    let k = 1.0;
    let h = (0.5 + 0.5 * (a - b) / k).clamp(0.0, 1.0);
    (
        b + (a - b) * h + k * h * (1.0 - h),
        if a >= b { 1 } else { 2 },
    )
}

fn check<S: Mesh>(name: &str) -> bool {
    let mut ok = true;
    // ── union ──
    let (alo, ahi) = ([0.0, 0.0, 0.0], [10.0, 10.0, 10.0]);
    let (blo, bhi) = ([5.0, 3.0, 4.0], [14.0, 8.0, 12.0]);
    let a = S::from_tri_mesh(&box_mesh(alo, ahi)).unwrap().as_original();
    let b = S::from_tri_mesh(&box_mesh(blo, bhi)).unwrap().as_original();
    let (ia, ib) = (a.original_id() as u32, b.original_id() as u32);
    let u = a.union(&b).to_tri_mesh();
    let ids = u.triangle_original_ids().unwrap();
    let mut unmapped = 0;
    let mut off_surface = 0;
    let mut per = HashMap::<u32, usize>::new();
    for (t, &id) in ids.iter().enumerate() {
        *per.entry(id).or_default() += 1;
        let c = centroid(&u, t);
        let d = if id == ia {
            box_surface_dist(c, alo, ahi)
        } else if id == ib {
            box_surface_dist(c, blo, bhi)
        } else {
            unmapped += 1;
            continue;
        };
        off_surface += usize::from(d.abs() > 1e-9);
    }
    let union_ok = unmapped == 0 && off_surface == 0 && per.len() == 2 && u.is_watertight();
    ok &= union_ok;
    println!(
        "{name} union: {} tris, ids a={ia} b={ib}, per-id {per:?}, unmapped {unmapped}, off-source-surface {off_surface}, watertight {} -> {}",
        u.triangle_count(),
        u.is_watertight(),
        if union_ok { "PASS" } else { "FAIL" }
    );

    // ── level set, co-sampled material, then a union ──
    let bounds = Bounds {
        min: [-8.0, -5.0, -5.0],
        max: [8.0, 5.0, 5.0],
    };
    let ls = S::level_set(&|p| spheres(p).0, bounds, 0.25).as_original();
    let lsm = ls.to_tri_mesh();
    let ls_id = ls.original_id() as u32;
    let mat: Vec<u32> = (0..lsm.triangle_count())
        .map(|t| spheres(centroid(&lsm, t)).1)
        .collect();
    let blk = S::from_tri_mesh(&box_mesh([-1.0, -1.0, 2.0], [1.0, 1.0, 9.0]))
        .unwrap()
        .as_original();
    let blk_id = blk.original_id() as u32;
    let out = ls.union(&blk).to_tri_mesh();
    let ids = out.triangle_original_ids().unwrap();
    let (mut traced, mut agree, mut near, mut unmapped, mut from_box) = (0, 0, 0, 0, 0);
    for (t, &id) in ids.iter().enumerate() {
        if id == blk_id {
            from_box += 1;
        } else if id == ls_id {
            // face ID of a level-set triangle = its index in the original
            let src = out.face_id.get(t).copied().unwrap_or(u32::MAX) as usize;
            if let Some(&m) = mat.get(src) {
                traced += 1;
                let (c, s) = (centroid(&out, t), centroid(&lsm, src));
                agree += usize::from(m == spheres(c).1);
                // the output piece lies on (within an edge of) its source
                near += usize::from(
                    (0..3).map(|k| (c[k] - s[k]).powi(2)).sum::<f64>() < 0.25f64.powi(2),
                );
            } else {
                unmapped += 1;
            }
        } else {
            unmapped += 1;
        }
    }
    let ls_ok =
        unmapped == 0 && traced > 0 && near == traced && from_box > 0 && out.is_watertight();
    ok &= ls_ok;
    println!(
        "{name} level set: {} source tris (mat1 {}, mat2 {}), after union {} tris: {} traced to a source tri ({} within 0.25 mm of it, {} same material as re-sampling), {} from box, {} unmapped -> {}",
        lsm.triangle_count(),
        mat.iter().filter(|&&m| m == 1).count(),
        mat.iter().filter(|&&m| m == 2).count(),
        out.triangle_count(),
        traced,
        near,
        agree,
        from_box,
        unmapped,
        if ls_ok { "PASS" } else { "FAIL" }
    );
    ok
}

fn main() {
    let mut ok = true;
    #[cfg(feature = "manifold-ffi")]
    {
        ok &= check::<dt_geom::backend::ffi::FfiSolid>("ffi");
    }
    #[cfg(feature = "manifold-rust")]
    {
        ok &= check::<dt_geom::backend::port::PortSolid>("port");
    }
    std::process::exit(i32::from(!ok));
}
