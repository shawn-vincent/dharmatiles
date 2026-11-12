//! `dharmatiles audit`: the printability gate on a binary STL, and the STL
//! readers the preview shares.

use serde_json::json;

use crate::{AUDIT_FAILED, ERROR, OK};

/// `audit FILE.stl`: the gate on a binary STL as a slicer would read it.
pub(crate) fn audit_stl(path: &str, width: Option<f64>, height: Option<f64>, json: bool) -> u8 {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            tell!("error: cannot read {path}: {e}");
            return ERROR;
        }
    };
    let Some(mesh) = read_stl(&bytes) else {
        tell!("error: {path} is not a binary STL");
        return ERROR;
    };
    let footprint = stl_footprint(&mesh, width, height);
    let printer = match dt_eval::printer_profile("default", None) {
        Ok(p) => p,
        Err(errs) => {
            crate::report(&errs);
            return ERROR;
        }
    };
    let round = stl_corner_round(footprint);
    let (_, a) = dt_kit::audit_rounded(
        &mesh,
        footprint,
        round,
        &printer,
        &[],
        Some(&dt_kit::HangBand::DEFAULT),
    );
    let failures = a.failures(&printer);
    if json {
        say!(
            "{}",
            json!({"ok": failures.is_empty(), "path": path, "triangles": a.triangles,
                "volume_mm3": a.volume_mm3, "watertight": a.watertight, "bodies": a.bodies,
                "overhang_mm2": a.overhang_mm2, "band_mm2": a.band_mm2,
                "band_budget_mm2": a.band_budget_mm2, "holes": a.holes,
                "dropped": a.dropped.len(), "failures": failures})
        );
    } else {
        tell!(
            "{path}: {} triangles, watertight {}, overhang (over 65°) {:.2} mm², band (55–65°) {:.2}/{:.2} mm², {} holes, {} loose bodies dropped",
            a.triangles,
            a.watertight,
            a.overhang_mm2,
            a.band_mm2,
            a.band_budget_mm2,
            a.holes,
            a.dropped.len()
        );
        for f in &failures {
            tell!("  audit failed: {f}");
        }
    }
    if failures.is_empty() {
        OK
    } else {
        AUDIT_FAILED
    }
}

/// The tile's size in mm for an STL: `width`/`height` when given, else the
/// whole squares (DungeonBlocks 35 mm, OpenLOCK 25.4 mm) nearest the
/// mesh's extent, which the base's flare overshoots a little.
pub(crate) fn stl_footprint(
    mesh: &dt_kit::TriMesh,
    width: Option<f64>,
    height: Option<f64>,
) -> [f64; 2] {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &mesh.positions {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    // the tile's size: whole squares (DungeonBlocks 35 mm, OpenLOCK 25.4 mm)
    // nearest the mesh's extent, which the base's flare overshoots a little
    let tile = |extent: f64| -> f64 {
        [35.0, 25.4]
            .iter()
            .filter_map(|sq| {
                let n = libm::round(extent / sq).max(1.0);
                let err = (extent - n * sq).abs();
                (err < 1.5).then_some((err, n * sq))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(extent, |(_, v)| v)
    };
    [
        width.unwrap_or_else(|| tile(hi[0] - lo[0])),
        height.unwrap_or_else(|| tile(hi[1] - lo[1])),
    ]
}

/// The plan round of an STL's four vertical corners: the tile's, on the
/// standard (35 mm or 25.4 mm squares) its footprint is whole squares of.
pub(crate) fn stl_corner_round(footprint: [f64; 2]) -> f64 {
    let whole = |sq: f64| {
        footprint
            .iter()
            .all(|e| (e / sq - libm::round(e / sq)).abs() < 1e-3)
    };
    let sq = if whole(35.0) { 35.0 } else { 25.4 };
    dt_kit::CORNER_ROUND_MM * sq / 35.0
}

/// A binary STL's triangles, vertices merged by position (as a slicer
/// reads it).
pub(crate) fn read_stl(b: &[u8]) -> Option<dt_kit::TriMesh> {
    let n = usize::try_from(u32::from_le_bytes(b.get(80..84)?.try_into().ok()?)).ok()?;
    if b.len() < 84 + 50 * n {
        return None;
    }
    let f = |o: usize| f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let mut index: std::collections::HashMap<[u32; 3], u32> = std::collections::HashMap::new();
    let mut m = dt_kit::TriMesh::default();
    for t in 0..n {
        let base = 84 + 50 * t + 12;
        let mut tri = [0u32; 3];
        for (v, slot) in tri.iter_mut().enumerate() {
            let o = base + 12 * v;
            let p = [f(o), f(o + 4), f(o + 8)];
            *slot = *index.entry(p.map(f32::to_bits)).or_insert_with(|| {
                m.positions.push(p.map(f64::from));
                u32::try_from(m.positions.len() - 1).unwrap_or(u32::MAX)
            });
        }
        m.triangles.push(tri);
    }
    Some(m)
}
