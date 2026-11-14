//! The 3MF writer: the tile as one object whose every triangle names its
//! material twice. As a 3MF base material, for any reader of the standard;
//! and as the filament the slicers' own paint tool would give it
//! (`paint_color` for Bambu Studio and OrcaSlicer,
//! `slic3rpe:mmu_segmentation` for PrusaSlicer), because those ignore base
//! materials. Filaments are numbered by the materials the tile uses, in
//! [`Material`] order, so earth is always filament 1; the connector prints
//! in earth's. A 3MF is a zip of three small files; the zip is written
//! here, deflated.

use std::fmt::Write as _;
use std::io::Write as _;

use dt_geom::TriMesh;

use dt_kit::{BuildRecord, Material, Palette};

/// **write_3mf**: the 3MF bytes of `mesh`, with `materials` giving each
/// triangle's material (as many as triangles).
#[must_use]
pub fn threemf(
    mesh: &TriMesh,
    materials: &[Material],
    record: &BuildRecord,
    palette: &Palette,
) -> Vec<u8> {
    let mut used: Vec<Material> = materials.to_vec();
    used.sort();
    used.dedup();
    let filaments = filaments(&used);
    let paint: Vec<String> = used
        .iter()
        .map(|m| {
            paint_code(
                filaments
                    .iter()
                    .position(|f| f == &filament_of(*m))
                    .unwrap_or(0)
                    + 1,
            )
        })
        .collect();
    let mut x = String::with_capacity(64 * mesh.triangles.len() + 40 * mesh.positions.len());
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str("<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" xmlns:slic3rpe=\"http://schemas.slic3r.org/3mf/2017/06\">\n");
    let _ = writeln!(
        x,
        " <metadata name=\"Application\">dharmatiles {}</metadata>\n <metadata name=\"Title\">{}</metadata>\n <metadata name=\"Description\">system {}, seed {}, document {:016x}</metadata>",
        escape(&record.engine),
        escape(&record.document),
        escape(&record.target()),
        record.seed,
        record.document_hash
    );
    let list: Vec<String> = filaments
        .iter()
        .enumerate()
        .map(|(i, m)| format!("{} {}", i + 1, m.name()))
        .collect();
    let _ = writeln!(
        x,
        " <metadata name=\"dharmatiles:filaments\">{}</metadata>",
        list.join(", ")
    );
    x.push_str(" <resources>\n  <basematerials id=\"1\">\n");
    for m in &used {
        let [r, g, b] = palette.colour(*m).map(byte);
        let _ = writeln!(
            x,
            "   <base name=\"{}\" displaycolor=\"#{r:02X}{g:02X}{b:02X}\"/>",
            m.name()
        );
    }
    x.push_str("  </basematerials>\n  <object id=\"2\" type=\"model\" pid=\"1\" pindex=\"0\">\n   <mesh>\n    <vertices>\n");
    for p in &mesh.positions {
        let _ = writeln!(
            x,
            "     <vertex x=\"{:.5}\" y=\"{:.5}\" z=\"{:.5}\"/>",
            p[0], p[1], p[2]
        );
    }
    x.push_str("    </vertices>\n    <triangles>\n");
    for (t, m) in mesh.triangles.iter().zip(materials) {
        let k = used.iter().position(|u| u == m).unwrap_or(0);
        let _ = writeln!(
            x,
            "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" pid=\"1\" p1=\"{k}\" paint_color=\"{c}\" slic3rpe:mmu_segmentation=\"{c}\"/>",
            t[0],
            t[1],
            t[2],
            c = paint[k]
        );
    }
    x.push_str("    </triangles>\n   </mesh>\n  </object>\n </resources>\n <build>\n  <item objectid=\"2\"/>\n </build>\n</model>\n");
    zip(&[
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#
                .to_vec(),
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#
                .to_vec(),
        ),
        ("3D/3dmodel.model", x.into_bytes()),
    ])
}

/// The filament a material prints in: its own, but the connector's is
/// earth's.
pub(crate) const fn filament_of(m: Material) -> Material {
    match m {
        Material::Connector => Material::Earth,
        other => other,
    }
}

/// The filaments a tile needs, in [`Material`] order (earth first).
pub(crate) fn filaments(used: &[Material]) -> Vec<Material> {
    let mut f: Vec<Material> = used.iter().map(|m| filament_of(*m)).collect();
    f.sort();
    f.dedup();
    f
}

/// A whole triangle painted with filament `n` (1-based), in the paint
/// tool's encoding (PrusaSlicer's `TriangleSelector::serialize`, which
/// Bambu Studio and OrcaSlicer share): an unsplit triangle is two zero
/// bits, then its state; states under 3 take two bits, the rest `11` and a
/// nibble of `state - 3`, 15 per extra `1111`. Nibbles are written last
/// first, in hex.
pub(crate) fn paint_code(n: usize) -> String {
    let mut bits: Vec<bool> = vec![false, false];
    if n < 3 {
        bits.extend([n & 1 != 0, n & 2 != 0]);
    } else {
        bits.extend([true, true]);
        let mut r = n - 3;
        while r >= 15 {
            bits.extend([true; 4]);
            r -= 15;
        }
        bits.extend((0..4).map(|i| r >> i & 1 != 0));
    }
    bits.chunks(4)
        .rev()
        .map(|c| {
            let v = c
                .iter()
                .enumerate()
                .fold(0u32, |a, (i, &b)| a | u32::from(b) << i);
            char::from_digit(v, 16).unwrap_or('0').to_ascii_uppercase()
        })
        .collect()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn byte(c: f64) -> u8 {
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// A zip of `files`, each deflated.
pub(crate) fn zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let le16 = |v: &mut Vec<u8>, n: u16| v.extend_from_slice(&n.to_le_bytes());
    let le32 = |v: &mut Vec<u8>, n: u32| v.extend_from_slice(&n.to_le_bytes());
    let size = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    for (name, data) in files {
        let mut enc =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        // into memory: writing cannot fail
        enc.write_all(data).expect("deflate into memory");
        let packed = enc.finish().expect("deflate into memory");
        let crc = crc32fast::hash(data);
        let offset = size(out.len());
        // local header
        le32(&mut out, 0x0403_4b50);
        le16(&mut out, 20);
        le16(&mut out, 0);
        le16(&mut out, 8);
        le16(&mut out, 0);
        le16(&mut out, 0x21); // 1980-01-01
        le32(&mut out, crc);
        le32(&mut out, size(packed.len()));
        le32(&mut out, size(data.len()));
        le16(&mut out, u16::try_from(name.len()).unwrap_or(0));
        le16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&packed);
        // central directory entry
        le32(&mut central, 0x0201_4b50);
        le16(&mut central, 20);
        le16(&mut central, 20);
        le16(&mut central, 0);
        le16(&mut central, 8);
        le16(&mut central, 0);
        le16(&mut central, 0x21);
        le32(&mut central, crc);
        le32(&mut central, size(packed.len()));
        le32(&mut central, size(data.len()));
        le16(&mut central, u16::try_from(name.len()).unwrap_or(0));
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le32(&mut central, 0);
        le32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let cd_at = size(out.len());
    out.extend_from_slice(&central);
    le32(&mut out, 0x0605_4b50);
    le16(&mut out, 0);
    le16(&mut out, 0);
    le16(&mut out, u16::try_from(files.len()).unwrap_or(0));
    le16(&mut out, u16::try_from(files.len()).unwrap_or(0));
    le32(&mut out, size(central.len()));
    le32(&mut out, cd_at);
    le16(&mut out, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_codes_are_the_slicers_own() {
        // what Bambu Studio and PrusaSlicer write for extruders 1 to 5
        let got: Vec<String> = (1..=5).map(paint_code).collect();
        assert_eq!(got, ["4", "8", "0C", "1C", "2C"]);
        assert_eq!(paint_code(17), "EC");
        assert_eq!(paint_code(18), "0FC");
    }

    #[test]
    fn earth_is_filament_one_and_the_connector_shares_it() {
        let used = [Material::Connector, Material::Water, Material::Earth];
        assert_eq!(filaments(&used), [Material::Earth, Material::Water]);
    }

    #[test]
    fn a_3mf_is_a_zip_with_the_model_and_its_materials() {
        let mesh = TriMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]],
            ..TriMesh::default()
        };
        let mats = [
            Material::Earth,
            Material::Earth,
            Material::Grass,
            Material::Connector,
        ];
        let rec = BuildRecord {
            engine: "t".into(),
            document: "d".into(),
            document_hash: 1,
            seed: 2,
            system: "db".into(),
            printer: "default".into(),
        };
        let z = threemf(&mesh, &mats, &rec, &Palette::default());
        assert_eq!(&z[..4], b"PK\x03\x04");
        assert_eq!(&z[z.len() - 22..z.len() - 18], b"PK\x05\x06");
        // three entries, each named in the central directory
        for n in ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"] {
            assert!(z.windows(n.len()).any(|w| w == n.as_bytes()), "{n}");
        }
    }
}
