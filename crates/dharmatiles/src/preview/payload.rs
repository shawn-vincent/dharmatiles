//! The mesh payload the viewer fetches: one binary buffer, typed arrays.
//!
//! ```text
//! "DTM1"                      4 bytes magic
//! u32 LE  json_len            padded so everything after starts 4-aligned
//! json (space padded)         {"triangles": n, "lo": [x,y,z], "hi": [x,y,z], …}
//! f32 LE  positions  [9 n]    3 vertices × xyz per triangle (unindexed)
//! f32 LE  normals    [9 n]    the face normal, once per vertex (flat shading)
//! u8      painted    [9 n]    the PNG renders' colours: overhang red, band orange
//! u8      clean      [9 n]    the same without the overhang paint
//! u16 LE  entry      [n]      the stack entry each triangle came from (65535 none)
//! u16 LE  piece      [n]      which of the entry's solids it came from
//! ```
//!
//! Colours come from the same painter as `build --png`
//! ([`dt_eval::Built::colours`]); the viewer never recomputes them.  The
//! overhang flags are the gate's own (layer reach), not an angle guess.

// f64 mesh coordinates to f32 for the GPU, and colours 0..1 to bytes.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use dt_eval::Built;
use dt_kit::TriMesh;
use serde_json::{Value, json};

/// First four bytes of every payload.
pub const MAGIC: &[u8; 4] = b"DTM1";

fn unit_normal(p: [[f64; 3]; 3]) -> [f32; 3] {
    let a = [0, 1, 2].map(|k| p[1][k] - p[0][k]);
    let b = [0, 1, 2].map(|k| p[2][k] - p[0][k]);
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 0.0 {
        n.map(|x| (x / l) as f32)
    } else {
        [0.0; 3]
    }
}

fn byte(c: f32) -> u8 {
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Encode `mesh` with its two per-face colour sets.  `meta` is merged into
/// the JSON header beside `triangles`, `lo` and `hi`.
#[must_use]
pub fn encode(mesh: &TriMesh, painted: &[[f32; 3]], clean: &[[f32; 3]], meta: &Value) -> Vec<u8> {
    encode_traced(mesh, painted, clean, &[], &[], meta)
}

/// [`encode`] with each triangle's stack entry and piece.
#[must_use]
pub fn encode_traced(
    mesh: &TriMesh,
    painted: &[[f32; 3]],
    clean: &[[f32; 3]],
    entries: &[u16],
    pieces: &[u16],
    meta: &Value,
) -> Vec<u8> {
    let n = mesh.triangles.len();
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &mesh.positions {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if n == 0 {
        lo = [0.0; 3];
        hi = [0.0; 3];
    }
    let mut header = meta.clone();
    header["triangles"] = json!(n);
    header["lo"] = json!(lo);
    header["hi"] = json!(hi);
    let mut j = header.to_string().into_bytes();
    while !(8 + j.len()).is_multiple_of(4) {
        j.push(b' ');
    }
    let mut out = Vec::with_capacity(8 + j.len() + n * 90);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&u32::try_from(j.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&j);
    let mut normals = Vec::with_capacity(n * 36);
    for t in &mesh.triangles {
        let p = t.map(|i| mesh.positions[i as usize]);
        for v in p {
            for c in v {
                out.extend_from_slice(&(c as f32).to_le_bytes());
            }
        }
        let nf = unit_normal(p);
        for _ in 0..3 {
            for c in nf {
                normals.extend_from_slice(&c.to_le_bytes());
            }
        }
    }
    out.extend_from_slice(&normals);
    for set in [painted, clean] {
        for f in 0..n {
            let c = set.get(f).copied().unwrap_or([0.7; 3]).map(byte);
            for _ in 0..3 {
                out.extend_from_slice(&c);
            }
        }
    }
    for set in [entries, pieces] {
        for f in 0..n {
            out.extend_from_slice(
                &set.get(f)
                    .copied()
                    .unwrap_or(if set.is_empty() { u16::MAX } else { 0 })
                    .to_le_bytes(),
            );
        }
    }
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out
}

/// The payload for a finished build: its painted colours (the PNG
/// renders'), its clean colours, and the gate's face counts.
#[must_use]
pub fn from_built(b: &Built, overhang_deg: f64, palette: &dt_kit::Palette) -> Vec<u8> {
    let painted = b.colours_with(palette, true);
    let clean = b.colours_with(palette, false);
    let meta = json!({
        "overhang_faces": b.audit.overhang.iter().filter(|&&o| o).count(),
        "band_faces": b.audit.band.iter().filter(|&&o| o).count(),
        "overhang_deg": overhang_deg,
        "footprint": b.footprint,
    });
    encode_traced(
        &b.export.mesh,
        &painted,
        &clean,
        &b.entries,
        &b.pieces,
        &meta,
    )
}

/// The most triangles a draft or a sketch is sent with; more are thinned
/// ([`thinned`]).  A finished build is always sent whole.
pub const VIEW_BUDGET: usize = 400_000;

/// `mesh` with its vertices snapped together on a grid coarse enough to
/// leave at most `budget` triangles, and the index in `mesh` of each
/// triangle kept.  A cheap stand-in for a view of a draft or a sketch: it
/// keeps each kept triangle's own colour and owner, and is never printed.
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
                let k = p.map(|c| libm::floor(c / cell) as i64);
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

/// [`from_built`] thinned to [`VIEW_BUDGET`]: a draft's soup of parts can
/// be millions of triangles.
#[must_use]
pub fn from_draft(b: &Built, overhang_deg: f64, palette: &dt_kit::Palette) -> Vec<u8> {
    let painted = b.colours_with(palette, true);
    let clean = b.colours_with(palette, false);
    let (mesh, kept) = thinned(&b.export.mesh, VIEW_BUDGET);
    let pick = |v: &[[f32; 3]]| kept.iter().map(|&f| v[f]).collect::<Vec<_>>();
    let pick16 = |v: &[u16]| {
        kept.iter()
            .map(|&f| v.get(f).copied().unwrap_or(u16::MAX))
            .collect::<Vec<_>>()
    };
    let meta = json!({
        "overhang_faces": b.audit.overhang.iter().filter(|&&o| o).count(),
        "band_faces": b.audit.band.iter().filter(|&&o| o).count(),
        "overhang_deg": overhang_deg,
        "footprint": b.footprint,
        "thinned": kept.len() < b.export.mesh.triangles.len(),
    });
    encode_traced(
        &mesh,
        &pick(&painted),
        &pick(&clean),
        &pick16(&b.entries),
        &pick16(&b.pieces),
        &meta,
    )
}

/// A sketch of a stage while the build runs ([`dt_kit::Reporter::sketch`]):
/// painted by material alone, no owner, thinned to [`VIEW_BUDGET`].
#[must_use]
pub fn sketch(
    mesh: &TriMesh,
    materials: &[dt_kit::Material],
    rough: bool,
    palette: &dt_kit::Palette,
) -> Vec<u8> {
    let (thin, kept) = thinned(mesh, VIEW_BUDGET);
    let mats: Vec<dt_kit::Material> = kept
        .iter()
        .map(|&f| materials.get(f).copied().unwrap_or(dt_kit::Material::Rock))
        .collect();
    let none = vec![false; mats.len()];
    let colours = dt_kit::paint_mesh(&thin, &mats, &none, &none, None, palette, false);
    encode(
        &thin,
        &colours,
        &colours,
        &json!({ "sketch": true, "rough": rough }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri() -> TriMesh {
        TriMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [0.0, 3.0, 1.0],
                [0.0, 0.0, 5.0],
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            ..TriMesh::default()
        }
    }

    #[test]
    fn layout_is_header_then_four_typed_arrays() {
        let red = [1.0, 0.0, 0.0];
        let grey = [0.5, 0.5, 0.5];
        let b = encode(&tri(), &[red, grey], &[grey, grey], &json!({"x": 1}));
        assert_eq!(&b[..4], MAGIC);
        let jl = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
        assert_eq!((8 + jl) % 4, 0, "arrays start 4-aligned");
        let h: Value = serde_json::from_slice(&b[8..8 + jl]).unwrap();
        assert_eq!(h["triangles"], 2);
        assert_eq!(h["x"], 1);
        assert_eq!(h["hi"], json!([2.0, 3.0, 5.0]));
        // 2 triangles: positions 18 f32, normals 18 f32, 2 × 18 bytes of
        // colour, 2 × 2 u16 of entry and piece
        assert_eq!(b.len(), 8 + jl + 72 + 72 + 18 + 18 + 8);
        let f = |o: usize| f32::from_le_bytes(b[8 + jl + o..8 + jl + o + 4].try_into().unwrap());
        assert!((f(4 * 3) - 2.0).abs() < 1e-6, "second vertex x");
        // normals: first face is (2,0,0)x(0,3,1) = (0,-2,6)/|…|
        let nl = (4.0f32 + 36.0).sqrt();
        assert!((f(72 + 4) - (-2.0 / nl)).abs() < 1e-6);
        assert!((f(72 + 8) - (6.0 / nl)).abs() < 1e-6);
        // painted colours: face 0 red on all three vertices, face 1 grey
        let c0 = 8 + jl + 144;
        assert_eq!(&b[c0..c0 + 3], &[255, 0, 0]);
        assert_eq!(&b[c0 + 6..c0 + 9], &[255, 0, 0]);
        assert_eq!(&b[c0 + 9..c0 + 12], &[128, 128, 128]);
        // clean colours follow, no red
        assert_eq!(&b[c0 + 18..c0 + 21], &[128, 128, 128]);
    }

    #[test]
    fn thinning_keeps_a_mesh_under_its_budget_and_each_kept_triangle_known() {
        // a 200 x 200 grid of squares: 80 000 triangles
        let n = 200;
        let mut m = TriMesh::default();
        for j in 0..=n {
            for i in 0..=n {
                m.positions
                    .push([f64::from(i) * 0.1, f64::from(j) * 0.1, 0.0]);
            }
        }
        let at = |i: u32, j: u32| j * (n + 1) + i;
        for j in 0..n {
            for i in 0..n {
                m.triangles.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                m.triangles.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        let (same, all) = thinned(&m, 100_000);
        assert_eq!(same.triangles.len(), 80_000);
        assert_eq!(all.len(), 80_000);
        let (thin, kept) = thinned(&m, 5_000);
        assert!(
            thin.triangles.len() <= 5_000 && thin.triangles.len() > 500,
            "{}",
            thin.triangles.len()
        );
        assert_eq!(kept.len(), thin.triangles.len());
        assert!(kept.windows(2).all(|w| w[0] < w[1]), "in order, each once");
    }

    #[test]
    fn an_empty_mesh_still_has_a_header() {
        let b = encode(&TriMesh::default(), &[], &[], &json!({}));
        assert_eq!(&b[..4], MAGIC);
        assert_eq!(b.len() % 4, 0);
        assert_eq!(
            b.len(),
            8 + u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize
        );
    }
}
