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
//! An entry number is a place in the header's `stack`: the stack this mesh
//! was built from, `[{index, key, label, element, what}]`, the connector
//! last when the tile has a base.  The page decodes an entry against that
//! list, never against the stack of the text it holds now, which may have
//! moved on while this mesh was on screen; `key` is the entry's id
//! (`tile/grass/0`), what the page keeps to find the entry again.
//!
//! Colours come from the same painter as `build --png`
//! ([`dt_eval::Built::colours`]); the viewer never recomputes them.  The
//! overhang flags are the gate's own (layer reach), not an angle guess.

// f64 mesh coordinates to f32 for the GPU, and colours 0..1 to bytes.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use dt_eval::Built;
use dt_kit::TriMesh;
use rayon::prelude::*;
use serde_json::{Value, json};

/// First four bytes of every payload.
pub const MAGIC: &[u8; 4] = b"DTM1";

/// A triangle that belongs to no stack entry (the page's `NO_ENTRY`).
pub const NO_ENTRY: u16 = u16::MAX;

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
///
/// # Errors
/// If a colour set is not one colour per triangle.
pub fn encode(
    mesh: &TriMesh,
    painted: &[[f32; 3]],
    clean: &[[f32; 3]],
    meta: &Value,
) -> Result<Vec<u8>, String> {
    encode_traced(mesh, painted, clean, &[], &[], meta)
}

/// [`encode`] with each triangle's stack entry and piece.  An empty
/// `entries` or `pieces` means the mesh has none (every triangle reads
/// [`NO_ENTRY`]); otherwise there is one per triangle.
///
/// # Errors
/// If a colour set, `entries` or `pieces` is neither empty (only for the
/// last two) nor one per triangle: a short list would give some triangle a
/// colour or an entry it was never given.
pub fn encode_traced(
    mesh: &TriMesh,
    painted: &[[f32; 3]],
    clean: &[[f32; 3]],
    entries: &[u16],
    pieces: &[u16],
    meta: &Value,
) -> Result<Vec<u8>, String> {
    let meta = &super::api::camel_keys(meta);
    let n = mesh.triangles.len();
    for (what, len, may_be_empty) in [
        ("painted colours", painted.len(), false),
        ("clean colours", clean.len(), false),
        ("entries", entries.len(), true),
        ("pieces", pieces.len(), true),
    ] {
        if len != n && !(may_be_empty && len == 0) {
            return Err(format!(
                "the view's mesh has {n} triangles but {len} {what}"
            ));
        }
    }
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
    let head = 8 + j.len();
    // the sections laid out in one buffer, each filled by the threads in
    // runs of triangles: the bytes are the ones a single pass would write
    let mut out = vec![0u8; head + n * 36 + n * 36 + n * 9 * 2 + n * 4];
    out[..4].copy_from_slice(MAGIC);
    out[4..8].copy_from_slice(&u32::try_from(j.len()).unwrap_or(u32::MAX).to_le_bytes());
    out[8..head].copy_from_slice(&j);
    let (positions, rest) = out[head..].split_at_mut(n * 36);
    let (normals, rest) = rest.split_at_mut(n * 36);
    let (colours, ids) = rest.split_at_mut(n * 18);
    positions
        .par_chunks_mut(36)
        .zip(normals.par_chunks_mut(36))
        .zip(&mesh.triangles)
        .for_each(|((pos, nrm), t)| {
            let p = t.map(|i| mesh.positions[i as usize]);
            let nf = unit_normal(p);
            for (a, v) in p.iter().enumerate() {
                for (k, c) in v.iter().enumerate() {
                    let at = 12 * a + 4 * k;
                    pos[at..at + 4].copy_from_slice(&(*c as f32).to_le_bytes());
                }
                for (k, c) in nf.iter().enumerate() {
                    let at = 12 * a + 4 * k;
                    nrm[at..at + 4].copy_from_slice(&c.to_le_bytes());
                }
            }
        });
    let (painted_out, clean_out) = colours.split_at_mut(n * 9);
    for (dst, set) in [(painted_out, painted), (clean_out, clean)] {
        dst.par_chunks_mut(9).zip(set).for_each(|(d, colour)| {
            let c = colour.map(byte);
            for a in 0..3 {
                d[3 * a..3 * a + 3].copy_from_slice(&c);
            }
        });
    }
    let (entries_out, pieces_out) = ids.split_at_mut(n * 2);
    for (dst, set) in [(entries_out, entries), (pieces_out, pieces)] {
        dst.par_chunks_mut(2).enumerate().for_each(|(f, d)| {
            d.copy_from_slice(&set.get(f).copied().unwrap_or(NO_ENTRY).to_le_bytes());
        });
    }
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    Ok(out)
}

/// The payload for a finished build: its painted colours (the PNG
/// renders'), its clean colours, and the gate's face counts.
///
/// # Errors
/// If the build's per-triangle lists do not match its mesh (see [`encode_traced`]).
pub fn from_built(
    b: &Built,
    overhang_deg: f64,
    palette: &dt_kit::Palette,
) -> Result<Vec<u8>, String> {
    let (painted, clean) = b.colours_pair(palette);
    let mut meta = json!({
        "overhang_faces": b.audit.overhang.iter().filter(|&&o| o).count(),
        "band_faces": b.audit.band.iter().filter(|&&o| o).count(),
        "overhang_deg": overhang_deg,
        "footprint": b.footprint,
        "stack": stack_json(b),
        "clouds": clouds_json(b),
        "debug": debug_json(b),
    }); // a low or super-low build has not been through the gate: the page says so
    if !b.checked {
        meta["checked"] = json!(false);
    }

    encode_traced(
        &b.export.mesh,
        &painted,
        &clean,
        &b.entries,
        &b.pieces,
        &meta,
    )
}

/// A piece of a map: the mesh with its colours and what tier it is, with
/// no stack, clouds or debug marks (the page only draws it).  A build that
/// has not been through the gate says so (`checked: false`); one that has
/// carries the gate's failures.
///
/// # Errors
/// If the build's per-triangle lists do not match its mesh.
pub fn for_map(
    b: &Built,
    tier: &str,
    doc_rev: u64,
    opts: &dt_eval::Options,
    palette: &dt_kit::Palette,
) -> Result<Vec<u8>, String> {
    let (painted, clean) = b.colours_pair(palette);
    let (lo, hi) = dt_kit::box_of(&b.export.mesh);
    let meta = json!({
        "tier": tier,
        "doc_rev": doc_rev,
        "checked": b.checked,
        "failures": if b.checked { b.audit.failures(&opts.printer) } else { Vec::new() },
        "overhang_deg": opts.printer.overhang_deg,
        "footprint": b.footprint,
        "tile_lo": lo, "tile_hi": hi,
    });
    encode(&b.export.mesh, &painted, &clean, &meta)
}

/// The stack a build was made from, as the mesh header carries it: for each
/// entry its place (the number the mesh's triangles carry), its key (its id,
/// which survives edits to the text), its words and its element, and `z`: the
/// lowest and highest height its triangles reach, mm, from the tile's zero
/// (`null` for an entry that made no triangle).  One reading of the mesh the
/// build made, so the page draws the thickness of each entry from here and
/// works nothing out of the mesh itself.
#[must_use]
pub fn stack_json(b: &Built) -> Value {
    let mut z: Vec<Option<[f64; 2]>> = vec![None; b.stack.len()];
    for (t, &e) in b.export.mesh.triangles.iter().zip(&b.entries) {
        let Some(slot) = z.get_mut(usize::from(e)) else {
            continue; // NO_ENTRY, or an entry the stack does not have
        };
        for &v in t {
            let h = b.export.mesh.positions[v as usize][2];
            *slot = Some(slot.map_or([h, h], |[lo, hi]| [lo.min(h), hi.max(h)]));
        }
    }
    Value::Array(
        b.stack
            .iter()
            .enumerate()
            .map(|(index, n)| {
                json!({
                    "index": index, "key": n.id.as_str(), "label": n.label,
                    "element": n.element, "what": n.what, "z": z[index],
                })
            })
            .collect(),
    )
}

/// The most debug points sent; more are thinned evenly, class by class.
pub const DEBUG_DOTS: usize = 40_000;
/// The most debug polyline points sent; more polylines are thinned evenly.
pub const DEBUG_TRAIL_POINTS: usize = 150_000;
/// The most nodes a debug sheet is sent with across or up.
pub const DEBUG_SHEET_NODES: usize = 72;
/// The most debug clouds at a point (branch tips, stones) sent; more are
/// thinned evenly, class by class.
pub const DEBUG_COLUMNS: usize = 20_000;

/// A coordinate to a hundredth of a millimetre: the JSON stays short.
fn mm(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Every k-th of `n`, so the ones kept spread over the whole tile and number
/// no more than `budget`.
fn every(n: usize, budget: usize) -> usize {
    n.div_ceil(budget.max(1)).max(1)
}

/// The debug layer's marks as the page draws them, thinned to
/// [`DEBUG_DOTS`] and [`DEBUG_TRAIL_POINTS`]:
/// `{"dots": {class: [x, y, z, …]}, "trails": {class: [[x, y, z, …], …]},
/// "thinned": bool}`; `null` when the build collected none (the page did not
/// ask for the layer).  The clouds are not here: see [`clouds_json`].
#[must_use]
pub fn debug_json(b: &Built) -> Value {
    use std::collections::BTreeMap;
    let Some(d) = &b.debug else {
        return Value::Null;
    };
    let mut thinned = false;
    let k_dot = every(d.dots.len(), DEBUG_DOTS);
    thinned |= k_dot > 1;
    let mut dots: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &d.dots {
        let n = seen.entry(p.class).or_default();
        if n.is_multiple_of(k_dot) {
            dots.entry(p.class).or_default().extend(p.at.map(mm));
        }
        *n += 1;
    }
    let total: usize = d.trails.iter().map(|t| t.points.len()).sum();
    let k_trail = every(total, DEBUG_TRAIL_POINTS);
    thinned |= k_trail > 1;
    let mut trails: BTreeMap<&str, Vec<Vec<f64>>> = BTreeMap::new();
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &d.trails {
        let n = seen.entry(t.class).or_default();
        if n.is_multiple_of(k_trail) {
            trails
                .entry(t.class)
                .or_default()
                .push(t.points.iter().flat_map(|p| p.map(mm)).collect());
        }
        *n += 1;
    }
    json!({"dots": dots, "trails": trails, "thinned": thinned})
}

/// The clouds a selected `top=` or pin is drawn from, thinned to
/// [`DEBUG_SHEET_NODES`] and [`DEBUG_COLUMNS`]:
/// `{"sheets": [{owner, class, nx, ny, step, z: [h or null, …],
/// shade: [0 to 1, …]}], "columns": [{owner, class, v: [x, y, low, high,
/// rolled, shade low, shade high, shade rolled, …]}], "thinned": bool}`, each
/// `owner` an entry's key (`tile/dirt/0`); `null` when the build collected
/// none.  Its own section of the header, apart from the debug layer's marks:
/// the page draws a cloud as part of the selection, whether or not the layer
/// is on.
#[must_use]
pub fn clouds_json(b: &Built) -> Value {
    use std::collections::BTreeMap;
    let Some(d) = &b.clouds else {
        return Value::Null;
    };
    let k_col = every(d.columns.len(), DEBUG_COLUMNS);
    let thinned = k_col > 1;
    // grouped by entry and class: the page draws one entry's clouds
    let mut columns: BTreeMap<(&str, &str), Vec<f64>> = BTreeMap::new();
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for c in &d.columns {
        let n = seen.entry(c.class).or_default();
        if n.is_multiple_of(k_col) {
            let v = columns.entry((c.owner.as_str(), c.class)).or_default();
            v.extend([c.at[0], c.at[1], c.span[0], c.span[1], c.rolled].map(mm));
            v.extend(c.shade.map(|s| (f64::from(s) * 100.0).round() / 100.0));
        }
        *n += 1;
    }
    let sheets: Vec<Value> = d
        .sheets
        .iter()
        .map(|s| {
            let k = every(s.z.nx.max(s.z.ny), DEBUG_SHEET_NODES);
            let (nx, ny) = (s.z.nx.div_ceil(k), s.z.ny.div_ceil(k));
            let z: Vec<Value> = (0..ny)
                .flat_map(|j| (0..nx).map(move |i| (i * k, j * k)))
                .map(|(i, j)| {
                    let n = j * s.z.nx + i;
                    if s.region.bits[n] {
                        json!(mm(s.z.z[n]))
                    } else {
                        Value::Null
                    }
                })
                .collect();
            let shade: Vec<f64> = (0..ny)
                .flat_map(|j| (0..nx).map(move |i| (i * k, j * k)))
                .map(|(i, j)| (f64::from(s.shade[j * s.z.nx + i]) * 100.0).round() / 100.0)
                .collect();
            json!({
                "owner": s.owner.as_str(), "class": s.class,
                "nx": nx, "ny": ny, "step": s.z.step * k as f64, "z": z,
                "shade": shade,
            })
        })
        .collect();
    let columns: Vec<Value> = columns
        .into_iter()
        .map(|((owner, class), v)| json!({"owner": owner, "class": class, "v": v}))
        .collect();
    json!({"sheets": sheets, "columns": columns, "thinned": thinned})
}

/// The most triangles a draft or a sketch is sent with; more are thinned
/// ([`thinned`]).  A finished build is always sent whole.
pub const VIEW_BUDGET: usize = 400_000;

pub use dt_eval::tiers::thinned;

/// [`from_built`] thinned to [`VIEW_BUDGET`]: a draft's soup of parts can
/// be millions of triangles.
///
/// # Errors
/// If the build's per-triangle lists do not match its mesh (see [`encode_traced`]).
pub fn from_draft(
    b: &Built,
    overhang_deg: f64,
    palette: &dt_kit::Palette,
) -> Result<Vec<u8>, String> {
    let (painted, clean) = b.colours_pair(palette);
    let (mesh, kept) = thinned(&b.export.mesh, VIEW_BUDGET);
    let triangles = b.export.mesh.triangles.len();
    if painted.len() != triangles || clean.len() != triangles {
        return Err(format!(
            "the draft's mesh has {triangles} triangles but {} painted and {} clean colours",
            painted.len(),
            clean.len()
        ));
    }
    let pick = |v: &[[f32; 3]]| kept.iter().map(|&f| v[f]).collect::<Vec<_>>();
    // a list that is empty has none to give (every kept triangle reads NO_ENTRY); a short one is a mismatch
    let pick16 = |what: &str, v: &[u16]| -> Result<Vec<u16>, String> {
        if v.is_empty() {
            return Ok(Vec::new());
        }
        if v.len() != triangles {
            return Err(format!(
                "the draft's mesh has {triangles} triangles but {} {what}",
                v.len()
            ));
        }
        Ok(kept.iter().map(|&f| v[f]).collect())
    };
    let meta = json!({
        "overhang_faces": b.audit.overhang.iter().filter(|&&o| o).count(),
        "band_faces": b.audit.band.iter().filter(|&&o| o).count(),
        "overhang_deg": overhang_deg,
        "footprint": b.footprint,
        "thinned": kept.len() < b.export.mesh.triangles.len(),
        "stack": stack_json(b),
        "clouds": clouds_json(b),
        "debug": debug_json(b),
    });
    encode_traced(
        &mesh,
        &pick(&painted),
        &pick(&clean),
        &pick16("entries", &b.entries)?,
        &pick16("pieces", &b.pieces)?,
        &meta,
    )
}

/// A sketch of a stage while the build runs ([`dt_kit::Reporter::sketch`]):
/// painted by material alone, no owner, thinned to [`VIEW_BUDGET`].
///
/// # Errors
/// If `materials` is not one per triangle of `mesh`.
pub fn sketch(
    mesh: &TriMesh,
    materials: &[dt_kit::Material],
    rough: bool,
    palette: &dt_kit::Palette,
) -> Result<Vec<u8>, String> {
    if materials.len() != mesh.triangles.len() {
        return Err(format!(
            "a sketch's mesh has {} triangles but {} materials",
            mesh.triangles.len(),
            materials.len()
        ));
    }
    let (thin, kept) = thinned(mesh, VIEW_BUDGET);
    let mats: Vec<dt_kit::Material> = kept.iter().map(|&f| materials[f]).collect();
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
        let b = encode(&tri(), &[red, grey], &[grey, grey], &json!({"x": 1})).unwrap();
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
    fn a_short_per_triangle_list_is_an_error_not_a_default() {
        let grey = [0.5; 3];
        // two triangles, one entry: the second would have read entry 0
        let e = encode_traced(&tri(), &[grey; 2], &[grey; 2], &[3], &[], &json!({}));
        assert!(e.unwrap_err().contains("2 triangles but 1 entries"));
        let e = encode_traced(&tri(), &[grey], &[grey; 2], &[], &[], &json!({}));
        assert!(e.unwrap_err().contains("painted colours"));
        // none at all is allowed: every triangle reads NO_ENTRY
        let b = encode_traced(&tri(), &[grey; 2], &[grey; 2], &[], &[], &json!({})).unwrap();
        assert_eq!(&b[b.len() - 8..b.len() - 4], &[0xff; 4]);
    }

    /// The encoder as it was written before the threads: one triangle after
    /// another into a growing buffer.
    fn encode_in_order(
        mesh: &TriMesh,
        painted: &[[f32; 3]],
        clean: &[[f32; 3]],
        entries: &[u16],
        pieces: &[u16],
        j: &[u8],
    ) -> Vec<u8> {
        let n = mesh.triangles.len();
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(j.len() as u32).to_le_bytes());
        out.extend_from_slice(j);
        let mut normals = Vec::new();
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
            for colour in set {
                let c = colour.map(byte);
                for _ in 0..3 {
                    out.extend_from_slice(&c);
                }
            }
        }
        for set in [entries, pieces] {
            for f in 0..n {
                out.extend_from_slice(&set.get(f).copied().unwrap_or(NO_ENTRY).to_le_bytes());
            }
        }
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        out
    }

    #[test]
    fn the_view_bytes_are_unchanged_by_painting_once_and_threading() {
        let pal = dt_kit::Palette::default();
        for tile in [
            "lib:tests/plants/1x1-tree.dtile",
            "lib:tests/plants/1x1-soil+grass.dtile",
            "lib:tiles/walls/1x1-wall-stone.dtile",
        ] {
            let b = dt_eval::build(&dt_eval::Library, tile, &dt_eval::Options::default()).unwrap();
            let (painted, clean) = b.colours_pair(&pal);
            assert_eq!(painted, b.colours_with(&pal, true), "{tile}: painted");
            assert_eq!(clean, b.colours_with(&pal, false), "{tile}: clean");
            let (over, band) = (
                b.audit.overhang.iter().any(|&o| o),
                b.audit.band.iter().any(|&o| o),
            );
            eprintln!("{tile}: overhang {over}, band {band}");
            let bytes = from_built(&b, 55.0, &pal).unwrap();
            let jl = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
            let want = encode_in_order(
                &b.export.mesh,
                &painted,
                &clean,
                &b.entries,
                &b.pieces,
                &bytes[8..8 + jl],
            );
            let first = bytes.iter().zip(&want).position(|(a, b)| a != b);
            assert!(
                bytes == want,
                "{tile}: the threaded encoding differs: lengths {} and {}, first at {first:?} (head {})",
                bytes.len(),
                want.len(),
                8 + jl
            );
        }
    }

    /// Not a test: `DT_TIME_TILE=lib:tiles/…` prints what encoding a finished
    /// build for the view costs, and the payload's hash.
    #[test]
    #[ignore = "a timing run: DT_TIME_TILE=lib:tiles/cliffs/2x1-cliff-blocks.dtile"]
    fn time_the_view_encoding() {
        let Ok(tile) = std::env::var("DT_TIME_TILE") else {
            return;
        };
        let opts = dt_eval::Options::default();
        let b = dt_eval::build(&dt_eval::Library, &tile, &opts).unwrap();
        let pal = dt_kit::Palette::default();
        let t = std::time::Instant::now();
        let bytes = from_built(&b, 55.0, &pal).unwrap();
        eprintln!(
            "{tile}: {} triangles, view encoding {:.3} s, {} bytes, fnv {:016x}",
            b.export.mesh.triangles.len(),
            t.elapsed().as_secs_f64(),
            bytes.len(),
            bytes
                .iter()
                .fold(0xcbf2_9ce4_8422_2325u64, |h, &x| (h ^ u64::from(x))
                    .wrapping_mul(0x0100_0000_01b3))
        );
    }

    #[test]
    fn an_empty_mesh_still_has_a_header() {
        let b = encode(&TriMesh::default(), &[], &[], &json!({})).unwrap();
        assert_eq!(&b[..4], MAGIC);
        assert_eq!(b.len() % 4, 0);
        assert_eq!(
            b.len(),
            8 + u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize
        );
    }

    #[test]
    fn the_header_carries_the_stack_the_mesh_was_built_from() {
        let text = "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" seed=21 {\n stack {\n  base\n  dirt top=\"$dirtEdgeHeight\"\n  scatter count=3 { rock size=\"9dmm .. 12dmm\" }\n }\n}\n";
        let m = dt_eval::Memory([("t.dtile".to_owned(), text.to_owned())].into());
        let opts = dt_eval::Options::default();
        let b = dt_eval::build(&m, "t.dtile", &opts).unwrap();
        let bytes = from_built(&b, 55.0, &dt_kit::Palette::default()).unwrap();
        let jl = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let h: Value = serde_json::from_slice(&bytes[8..8 + jl]).unwrap();
        let stack = h["stack"].as_array().unwrap();
        let keys: Vec<&str> = stack.iter().map(|e| e["key"].as_str().unwrap()).collect();
        assert_eq!(keys, ["tile/dirt/0", "tile/scatter/0", "tile/connector"]);
        for (i, e) in stack.iter().enumerate() {
            assert_eq!(e["index"], i);
        }
        assert_eq!(stack[1]["label"], "rocks");
        assert_eq!(stack[1]["what"], "scatter");
        assert_eq!(stack[1]["element"], "rock");
        // every triangle's entry is a place in that stack (or none)
        let n = h["triangles"].as_u64().unwrap() as usize;
        let at = 8 + jl + 72 * n + 18 * n;
        let entries: Vec<u16> = (0..n)
            .map(|t| u16::from_le_bytes(bytes[at + 2 * t..at + 2 * t + 2].try_into().unwrap()))
            .collect();
        assert!(
            entries
                .iter()
                .all(|&e| e == NO_ENTRY || usize::from(e) < stack.len())
        );
        assert!(
            entries.contains(&2),
            "the base is the last place in the stack"
        );
    }

    #[test]
    fn clouds_json_sends_each_cloud_with_its_shade() {
        let text = "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" seed=21 {\n stack {\n  base\n  dirt top=\"$groundHeight + low(0dmm .. 3dmm)\"\n  scatter count=3 { rock size=\"9dmm .. 12dmm\" top=\"$bottom + high(3dmm .. 9dmm)\" }\n }\n}\n";
        let m = dt_eval::Memory([("t.dtile".to_owned(), text.to_owned())].into());
        let opts = dt_eval::Options {
            clouds: true,
            ..dt_eval::Options::default()
        };
        let b = dt_eval::build(&m, "t.dtile", &opts).unwrap();
        // the clouds travel apart from the debug layer's marks, which were not asked for
        assert!(debug_json(&b).is_null());
        let d = clouds_json(&b);
        let sheets = d["sheets"].as_array().unwrap();
        assert!(sheets.iter().any(|s| s["class"] == "band-hi"));
        for s in sheets {
            let n = s["nx"].as_u64().unwrap() * s["ny"].as_u64().unwrap();
            assert_eq!(s["shade"].as_array().unwrap().len() as u64, n);
            assert_eq!(s["z"].as_array().unwrap().len() as u64, n);
        }
        let cols = d["columns"].as_array().unwrap();
        let rock = cols.iter().find(|c| c["class"] == "stone-cloud").unwrap();
        // the scatter is the stack's second entry (the base is not one)
        assert_eq!(rock["owner"], "tile/scatter/0", "{rock}");
        let stones = rock["v"].as_array().unwrap();
        assert!(
            stones.len() >= 3 * 8 && stones.len().is_multiple_of(8),
            "{}",
            stones.len()
        );
        // off: nothing at all
        let off = dt_eval::build(&m, "t.dtile", &dt_eval::Options::default()).unwrap();
        assert!(clouds_json(&off).is_null() && debug_json(&off).is_null());
    }
}
