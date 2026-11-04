//! The 3MF writer: the tile as one object whose every triangle names its
//! material (R9), so a slicer that reads 3MF base materials (Bambu Studio,
//! PrusaSlicer) opens it with the materials already assigned. A 3MF is a
//! zip of three small files; the zip is written here, deflated.

use std::fmt::Write as _;
use std::io::Write as _;

use dt_geom::TriMesh;

use crate::{BuildRecord, Material};

/// Material to colour: the separate step after the build (a 3MF is
/// painted by it; the geometry never changes). A palette document is one
/// line a material, `rock "#8a8a90"`, `//` for comments; materials it does
/// not name keep their default colour.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Palette(Vec<(Material, [f64; 3])>);

impl Palette {
    /// The colour of `m`.
    #[must_use]
    pub fn colour(&self, m: Material) -> [f64; 3] {
        self.0
            .iter()
            .find(|(k, _)| *k == m)
            .map_or_else(|| m.colour(), |(_, c)| *c)
    }

    /// Read a palette document.
    ///
    /// # Errors
    /// The line and what is wrong with it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut out = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.split("//").next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut w = line.split_whitespace();
            let name = w.next().unwrap_or("");
            let hex = w
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .trim_start_matches('#');
            let m = Material::from_name(name).ok_or_else(|| {
                format!("line {}: `{name}` is not a material (earth, connector, rock, water, grass, masonry, wood, leaf)", n + 1)
            })?;
            if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!(
                    "line {}: `{name}` needs a colour like \"#8a8a90\"",
                    n + 1
                ));
            }
            let v =
                |i: usize| f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0)) / 255.0;
            out.retain(|(k, _): &(Material, [f64; 3])| *k != m);
            out.push((m, [v(0), v(2), v(4)]));
        }
        Ok(Self(out))
    }
}

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
    let mut x = String::with_capacity(64 * mesh.triangles.len() + 40 * mesh.positions.len());
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str("<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n");
    let _ = writeln!(
        x,
        " <metadata name=\"Application\">dharmatiles {}</metadata>\n <metadata name=\"Title\">{}</metadata>\n <metadata name=\"Description\">system {}, seed {}, document {:016x}</metadata>",
        escape(&record.engine),
        escape(&record.document),
        escape(&record.system),
        record.seed,
        record.document_hash
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
            "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" pid=\"1\" p1=\"{k}\"/>",
            t[0], t[1], t[2]
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

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn byte(c: f64) -> u8 {
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// A zip of `files`, each deflated.
fn zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let le16 = |v: &mut Vec<u8>, n: u16| v.extend_from_slice(&n.to_le_bytes());
    let le32 = |v: &mut Vec<u8>, n: u32| v.extend_from_slice(&n.to_le_bytes());
    let size = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    for (name, data) in files {
        let mut enc =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        let _ = enc.write_all(data);
        let packed = enc.finish().unwrap_or_default();
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
    fn a_palette_names_materials_and_refuses_what_is_not_one() {
        let p = Palette::parse("// mine\nrock \"#ff0000\"\nwater #0000ff").unwrap();
        assert_eq!(p.colour(Material::Rock), [1.0, 0.0, 0.0]);
        assert_eq!(p.colour(Material::Earth), Material::Earth.colour());
        assert!(Palette::parse("lava #ff0000").is_err());
        assert!(Palette::parse("rock red").is_err());
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
