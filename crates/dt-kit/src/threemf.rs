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

use crate::{BuildRecord, Material};

/// One ramp: tones along relief, each with its place from 0 to 1.
pub type Ramp = Vec<(f32, [f32; 3])>;

/// Material to colour: the separate step after the build (a 3MF is
/// painted by it; the geometry never changes). A palette document has two
/// kinds of line, `//` for comments:
///
/// - `rock "#8a8a90"`: the material's one flat colour (3MF base colour);
/// - `ramp rock "#3a3d46" "#5f636e" "#8c909a@0.7" "#e4e5e6"`: the acrylic
///   paint of the preview and the PNG renders, a dark basecoat to the
///   drybrushed highlights, blended smoothly by local relief. A tone's
///   place is spaced evenly unless it ends `@0.7`.
///
/// What a document does not name keeps its default
/// (`library/palettes/default.dtile`).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    flat: Vec<(Material, [f64; 3])>,
    ramps: std::collections::BTreeMap<String, Ramp>,
}

/// The ramps' names: the materials, plus the ground's `pebble` and
/// `brick` (masonry of a brick wall).
pub const RAMPS: [&str; 12] = [
    "earth",
    "pebble",
    "water",
    "grass",
    "leaf",
    "wood",
    "rock",
    "masonry",
    "brick",
    "flower",
    "connector",
    "side",
];

const DEFAULT: &str = include_str!("../../../library/palettes/default.dtile");

impl Default for Palette {
    fn default() -> Self {
        let mut p = Self {
            flat: Vec::new(),
            ramps: std::collections::BTreeMap::new(),
        };
        // The bundled palette is compiled in and checked by a test
        // (`the_bundled_palette_reads`), so this cannot fail in a program that
        // passed its tests; if it ever did, stopping is right: no colours
        // can be guessed.
        #[allow(clippy::expect_used)]
        p.read(DEFAULT)
            .expect("the bundled palette (library/palettes/default.dtile) reads");
        p
    }
}

fn hex(h: &str) -> Option<[f64; 3]> {
    let h = h.trim_matches('"').trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = |i: usize| f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0)) / 255.0;
    Some([v(0), v(2), v(4)])
}

impl Palette {
    /// The colour of `m`.
    #[must_use]
    pub fn colour(&self, m: Material) -> [f64; 3] {
        self.flat
            .iter()
            .find(|(k, _)| *k == m)
            .map_or_else(|| m.colour(), |(_, c)| *c)
    }

    /// The paint of ramp `name` at `t` (0 to 1) of relief.
    #[must_use]
    pub fn sample(&self, name: &str, t: f32) -> [f32; 3] {
        let Some(r) = self.ramps.get(name) else {
            return [0.7; 3];
        };
        let t = t.clamp(0.0, 1.0);
        let i = (0..r.len().saturating_sub(1))
            .rev()
            .find(|&i| t >= r[i].0)
            .unwrap_or(0);
        if r.len() < 2 {
            return r.first().map_or([0.7; 3], |x| x.1);
        }
        let f = ((t - r[i].0) / (r[i + 1].0 - r[i].0).max(1e-6)).clamp(0.0, 1.0);
        [0, 1, 2].map(|c| r[i].1[c] * (1.0 - f) + r[i + 1].1[c] * f)
    }

    /// Read a palette document.
    ///
    /// # Errors
    /// The line and what is wrong with it.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut p = Self::default();
        p.read(text)?;
        Ok(p)
    }

    fn read(&mut self, text: &str) -> Result<(), String> {
        for (n, line) in text.lines().enumerate() {
            let line = line.split("//").next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut w = line.split_whitespace();
            let first = w.next().unwrap_or("");
            if first == "ramp" {
                let name = w.next().unwrap_or("");
                if !RAMPS.contains(&name) {
                    return Err(format!(
                        "line {}: `{name}` is not a ramp ({})",
                        n + 1,
                        RAMPS.join(", ")
                    ));
                }
                let toks: Vec<&str> = w.collect();
                if toks.len() < 2 {
                    return Err(format!("line {}: a ramp needs at least two tones", n + 1));
                }
                #[allow(clippy::cast_precision_loss)]
                let last = (toks.len() - 1) as f32;
                let mut ramp: Ramp = Vec::new();
                for (k, tok) in toks.iter().enumerate() {
                    let (h, at) = tok
                        .split_once('@')
                        .map_or((*tok, None), |(h, a)| (h, Some(a)));
                    let c = hex(h).ok_or_else(|| {
                        format!("line {}: `{tok}` is not a colour like \"#8a8a90\"", n + 1)
                    })?;
                    #[allow(clippy::cast_precision_loss)]
                    let pos = match at {
                        Some(a) => a
                            .trim_matches('"')
                            .parse::<f32>()
                            .map_err(|_| format!("line {}: `@{a}` is not a number", n + 1))?,
                        None => k as f32 / last,
                    };
                    #[allow(clippy::cast_possible_truncation)]
                    ramp.push((pos, c.map(|v| v as f32)));
                }
                if ramp.windows(2).any(|w| w[1].0 <= w[0].0) {
                    return Err(format!("line {}: the tones' places must increase", n + 1));
                }
                self.ramps.insert(name.to_owned(), ramp);
                continue;
            }
            let m = Material::from_name(first).ok_or_else(|| {
                format!("line {}: `{first}` is not a material (earth, connector, rock, water, grass, masonry, wood, leaf) or `ramp`", n + 1)
            })?;
            let c = hex(w.next().unwrap_or("")).ok_or_else(|| {
                format!("line {}: `{first}` needs a colour like \"#8a8a90\"", n + 1)
            })?;
            self.flat.retain(|(k, _)| *k != m);
            self.flat.push((m, c));
        }
        Ok(())
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
    fn the_bundled_palette_reads() {
        // `Palette::default` expects this to hold
        let mut p = Palette {
            flat: Vec::new(),
            ramps: std::collections::BTreeMap::new(),
        };
        p.read(DEFAULT).unwrap();
        assert!(!p.flat.is_empty());
    }

    #[test]
    fn a_palette_names_materials_and_refuses_what_is_not_one() {
        let p = Palette::parse("// mine\nrock \"#ff0000\"\nwater #0000ff").unwrap();
        assert_eq!(p.colour(Material::Rock), [1.0, 0.0, 0.0]);
        assert_eq!(
            p.colour(Material::Earth),
            Palette::default().colour(Material::Earth)
        );
        assert!(Palette::parse("lava #ff0000").is_err());
        assert!(Palette::parse("rock red").is_err());
    }

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
