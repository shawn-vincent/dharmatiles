//! Plates as files: pieces stood on beds, written as one STL per plate, a
//! plain 3MF per plate, or one Bambu Studio project with every plate.
//!
//! A *part* is one built mesh (a tile at a seed on a system); a *stand* is one
//! copy of a part on a plate, at the place the packer gave it, turned a
//! quarter or not.  Copies of a part share one object in a 3MF and have one
//! build item each.  Every part rests on the bed (its lowest point at zero),
//! whatever base it has: a DungeonBlocks piece and an OpenLOCK piece may share
//! a plate.  Colour is per triangle in multicolour (the paint the slicers'
//! own tool writes) and absent in single colour.

use std::fmt::Write as _;

use dt_geom::TriMesh;
use serde_json::Value;

use crate::bambu::{bed_corners, esc, settings};
use crate::threemf::{byte, filament_of, filaments, paint_code, zip};
use crate::{BuildRecord, Material, Palette};

/// One built mesh, with each triangle's material.
#[derive(Clone, Copy, Debug)]
pub struct PlatePart<'a> {
    /// What it is called (the tile's name).
    pub name: &'a str,
    /// The mesh as written.
    pub mesh: &'a TriMesh,
    /// Each triangle's material.
    pub materials: &'a [Material],
}

/// One copy of a part on a plate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlateStand {
    /// Which part.
    pub part: usize,
    /// Which plate, from 0.
    pub plate: usize,
    /// The left edge of its footprint on the bed, mm.
    pub x: f64,
    /// The front edge of its footprint on the bed, mm.
    pub y: f64,
    /// Turned a quarter, counter-clockwise.
    pub turned: bool,
}

/// What to write.
#[derive(Clone, Debug)]
pub struct PlateSet<'a> {
    /// The list's name, for the project's title.
    pub title: &'a str,
    /// The distinct meshes.
    pub parts: Vec<PlatePart<'a>>,
    /// Every copy of them, with its plate.
    pub stands: Vec<PlateStand>,
    /// How many plates.
    pub plates: usize,
    /// Each material in its own filament, or one filament and no paint.
    pub multicolour: bool,
    /// What the files say they were made from.
    pub record: &'a BuildRecord,
}

/// The filaments a set of materials prints in, earth first (the connector
/// prints in earth's).
#[must_use]
pub fn filaments_of(used: &[Material]) -> Vec<Material> {
    let mut u = used.to_vec();
    u.sort();
    u.dedup();
    filaments(&u)
}

/// A part's box: its lowest and highest corner.
fn bounds(mesh: &TriMesh) -> ([f64; 3], [f64; 3]) {
    mesh.positions
        .iter()
        .fold(([f64::MAX; 3], [f64::MIN; 3]), |(lo, hi), p| {
            (
                [0, 1, 2].map(|i| lo[i].min(p[i])),
                [0, 1, 2].map(|i| hi[i].max(p[i])),
            )
        })
}

/// The box of a part, as [`footprint_of`] reports it.
#[must_use]
pub fn box_of(mesh: &TriMesh) -> ([f64; 3], [f64; 3]) {
    bounds(mesh)
}

/// Where a stand puts its part's middle, mm on the bed, with the part's
/// height: the part rests on the bed.
fn middle(lo: [f64; 3], hi: [f64; 3], s: &PlateStand) -> [f64; 3] {
    let (w, d) = (hi[0] - lo[0], hi[1] - lo[1]);
    let (w, d) = if s.turned { (d, w) } else { (w, d) };
    [s.x + w / 2.0, s.y + d / 2.0, (hi[2] - lo[2]) / 2.0]
}

/// The 3MF matrix of a stand's rotation (row-vector form, three rows).
const fn rotation(turned: bool) -> &'static str {
    if turned {
        "0 1 0 -1 0 0 0 0 1"
    } else {
        "1 0 0 0 1 0 0 0 1"
    }
}

/// Every plate's pieces merged into one mesh at their places on the bed.
fn baked(set: &PlateSet, plate: usize) -> TriMesh {
    let mut out = TriMesh::default();
    for s in set.stands.iter().filter(|s| s.plate == plate) {
        let part = &set.parts[s.part];
        let (lo, hi) = bounds(part.mesh);
        let mid = [0, 1, 2].map(|i| (lo[i] + hi[i]) / 2.0);
        let at = middle(lo, hi, s);
        let base = u32::try_from(out.positions.len()).unwrap_or(u32::MAX);
        for p in &part.mesh.positions {
            let c = [p[0] - mid[0], p[1] - mid[1], p[2] - mid[2]];
            let c = if s.turned { [-c[1], c[0], c[2]] } else { c };
            out.positions.push([at[0] + c[0], at[1] + c[1], at[2] + c[2]]);
        }
        out.triangles
            .extend(part.mesh.triangles.iter().map(|t| t.map(|i| i + base)));
    }
    out
}

/// One plate as a binary STL: every piece on it, in one mesh.
#[must_use]
pub fn plate_stl(set: &PlateSet, plate: usize) -> Vec<u8> {
    crate::stl(&baked(set, plate), set.record)
}

/// The paint code of each material of a part, by the project's filaments.
fn codes(used: &[Material], fil: &[Material]) -> Vec<String> {
    used.iter()
        .map(|m| paint_code(fil.iter().position(|f| *f == filament_of(*m)).unwrap_or(0) + 1))
        .collect()
}

/// The distinct materials of the set, sorted.
fn used_of(set: &PlateSet) -> Vec<Material> {
    let mut used: Vec<Material> = set
        .parts
        .iter()
        .flat_map(|p| p.materials.iter().copied())
        .collect();
    used.sort();
    used.dedup();
    used
}

fn vertices(o: &mut String, mesh: &TriMesh, mid: [f64; 3]) {
    for p in &mesh.positions {
        let _ = writeln!(
            o,
            "     <vertex x=\"{:.5}\" y=\"{:.5}\" z=\"{:.5}\"/>",
            p[0] - mid[0],
            p[1] - mid[1],
            p[2] - mid[2]
        );
    }
}

/// One plate as a plain 3MF: an object per distinct part on it, an item per
/// copy.  Paint per triangle in multicolour (as the single-tile 3MF writes
/// it), none in single colour.
#[must_use]
pub fn plate_3mf(set: &PlateSet, plate: usize, palette: &Palette) -> Vec<u8> {
    let used = used_of(set);
    let fil = filaments(&used);
    let paint = codes(&used, &fil);
    let here: Vec<&PlateStand> = set.stands.iter().filter(|s| s.plate == plate).collect();
    let mut parts: Vec<usize> = here.iter().map(|s| s.part).collect();
    parts.sort_unstable();
    parts.dedup();
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str("<model unit=\"millimeter\" xml:lang=\"en-US\" xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" xmlns:slic3rpe=\"http://schemas.slic3r.org/3mf/2017/06\">\n");
    let _ = writeln!(
        x,
        " <metadata name=\"Application\">dharmatiles {}</metadata>\n <metadata name=\"Title\">{} plate {}</metadata>\n <metadata name=\"Description\">{} plate {} of {}, system {}</metadata>",
        esc(&set.record.engine),
        esc(set.title),
        plate + 1,
        if set.multicolour { "multicolour" } else { "single colour" },
        plate + 1,
        set.plates,
        esc(&set.record.target())
    );
    x.push_str(" <resources>\n");
    if set.multicolour {
        x.push_str("  <basematerials id=\"1\">\n");
        for m in &used {
            let [r, g, b] = palette.colour(*m).map(byte);
            let _ = writeln!(
                x,
                "   <base name=\"{}\" displaycolor=\"#{r:02X}{g:02X}{b:02X}\"/>",
                m.name()
            );
        }
        x.push_str("  </basematerials>\n");
    }
    for (k, &pi) in parts.iter().enumerate() {
        let part = &set.parts[pi];
        let (lo, hi) = bounds(part.mesh);
        let mid = [0, 1, 2].map(|i| (lo[i] + hi[i]) / 2.0);
        let _ = write!(
            x,
            "  <object id=\"{}\" type=\"model\" name=\"{}\">\n   <mesh>\n    <vertices>\n",
            k + 2,
            esc(part.name)
        );
        vertices(&mut x, part.mesh, mid);
        x.push_str("    </vertices>\n    <triangles>\n");
        for (t, m) in part.mesh.triangles.iter().zip(part.materials) {
            if set.multicolour {
                let u = used.iter().position(|q| q == m).unwrap_or(0);
                let _ = writeln!(
                    x,
                    "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" pid=\"1\" p1=\"{u}\" paint_color=\"{c}\" slic3rpe:mmu_segmentation=\"{c}\"/>",
                    t[0], t[1], t[2], c = paint[u]
                );
            } else {
                let _ = writeln!(
                    x,
                    "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>",
                    t[0], t[1], t[2]
                );
            }
        }
        x.push_str("    </triangles>\n   </mesh>\n  </object>\n");
    }
    x.push_str(" </resources>\n <build>\n");
    for s in &here {
        let (lo, hi) = bounds(set.parts[s.part].mesh);
        let at = middle(lo, hi, s);
        let k = parts.iter().position(|p| *p == s.part).unwrap_or(0);
        let _ = writeln!(
            x,
            "  <item objectid=\"{}\" transform=\"{} {:.5} {:.5} {:.5}\"/>",
            k + 2,
            rotation(s.turned),
            at[0],
            at[1],
            at[2]
        );
    }
    x.push_str(" </build>\n</model>\n");
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

/// How many plates Bambu Studio lays in a row of its plate grid for `n`
/// plates (its `compute_colum_count`).
#[must_use]
pub fn bambu_columns(n: usize) -> usize {
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let v = (n as f64).sqrt();
    let r = v.round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let c = if v > r { r as usize + 1 } else { r as usize };
    c.max(1)
}

/// Where plate `i` of `n` has its bed's front-left corner in Bambu Studio's
/// world, mm: plates stand in a grid, `columns` to a row, a fifth of a bed
/// apart, the first row at the front and later rows behind it.
#[must_use]
pub fn bambu_plate_origin(i: usize, n: usize, bed: [f64; 2]) -> [f64; 2] {
    let cols = bambu_columns(n);
    let (row, col) = (i / cols, i % cols);
    #[allow(clippy::cast_precision_loss)]
    [
        col as f64 * bed[0] * 1.2,
        -(row as f64) * bed[1] * 1.2,
    ]
}

fn uuid(kind: u32, n: usize) -> String {
    format!("{n:08x}-{kind:04x}-4c03-9d28-80fed5dfa1dc")
}

/// The whole set as one Bambu Studio project: every plate, an object per
/// distinct part (copies share it), an item per copy placed on its plate, one
/// filament slot per material (one in single colour), and `template`'s
/// other settings.
///
/// # Errors
/// When the template's settings cannot be read, it has no printable area, or
/// its bed is not `bed` (a template saved for another printer would put the
/// plates in the wrong places).
pub fn bambu_plates(
    set: &PlateSet,
    palette: &Palette,
    template: &str,
    bed: [f64; 2],
) -> Result<Vec<u8>, String> {
    let (corner, far) = bed_corners(template)?;
    let tbed = [far[0] - corner[0], far[1] - corner[1]];
    if (tbed[0] - bed[0]).abs() > 0.5 || (tbed[1] - bed[1]).abs() > 0.5 {
        return Err(format!(
            "the template's bed is {} x {} mm and the printer profile's is {} x {} mm: save a template from Bambu Studio for this printer, or choose the matching profile",
            tbed[0], tbed[1], bed[0], bed[1]
        ));
    }
    let used = used_of(set);
    let fil = if set.multicolour {
        filaments(&used)
    } else {
        vec![Material::Earth]
    };
    let slots: Vec<String> = fil
        .iter()
        .map(|m| {
            let [r, g, b] = palette.colour(*m).map(byte);
            format!("#{r:02X}{g:02X}{b:02X}")
        })
        .collect();
    let paint = codes(&used, &fil);
    let config = settings(template, &slots)?;
    const NS: &str = "xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" xmlns:BambuStudio=\"http://schemas.bambulab.com/package/2021\" xmlns:p=\"http://schemas.microsoft.com/3dmanufacturing/production/2015/06\" requiredextensions=\"p\"";

    // one object file per distinct part: ids 1, 3, 5 …, and its object in the
    // top model 2, 4, 6 …
    let mut parts: Vec<usize> = set.stands.iter().map(|s| s.part).collect();
    parts.sort_unstable();
    parts.dedup();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut top_objects = String::new();
    let mut rels = String::new();
    let mut model_settings = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n");
    for (k, &pi) in parts.iter().enumerate() {
        let part = &set.parts[pi];
        let (lo, hi) = bounds(part.mesh);
        let mid = [0, 1, 2].map(|i| (lo[i] + hi[i]) / 2.0);
        let (sub, top) = (2 * k + 1, 2 * k + 2);
        let mut o = String::with_capacity(48 * part.mesh.triangles.len());
        let _ = write!(
            o,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" {NS}>\n <metadata name=\"BambuStudio:3mfVersion\">1</metadata>\n <resources>\n  <object id=\"{sub}\" p:UUID=\"{}\" type=\"model\">\n   <mesh>\n    <vertices>\n",
            uuid(0x81cb, sub)
        );
        vertices(&mut o, part.mesh, mid);
        o.push_str("    </vertices>\n    <triangles>\n");
        for (t, m) in part.mesh.triangles.iter().zip(part.materials) {
            if set.multicolour {
                let u = used.iter().position(|q| q == m).unwrap_or(0);
                let _ = writeln!(
                    o,
                    "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" paint_color=\"{}\"/>",
                    t[0], t[1], t[2], paint[u]
                );
            } else {
                let _ = writeln!(
                    o,
                    "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>",
                    t[0], t[1], t[2]
                );
            }
        }
        o.push_str("    </triangles>\n   </mesh>\n  </object>\n </resources>\n <build/>\n</model>\n");
        files.push((format!("3D/Objects/object_{}.model", k + 1), o.into_bytes()));
        let _ = write!(
            top_objects,
            "  <object id=\"{top}\" p:UUID=\"{}\" type=\"model\">\n   <components>\n    <component p:path=\"/3D/Objects/object_{}.model\" objectid=\"{sub}\" p:UUID=\"{}\" transform=\"1 0 0 0 1 0 0 0 1 0 0 0\"/>\n   </components>\n  </object>\n",
            uuid(0x61cb, top),
            k + 1,
            uuid(0xb206, sub)
        );
        let _ = write!(
            rels,
            "<Relationship Target=\"/3D/Objects/object_{}.model\" Id=\"rel-{}\" Type=\"http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel\"/>",
            k + 1,
            k + 1
        );
        let _ = write!(
            model_settings,
            "  <object id=\"{top}\">\n    <metadata key=\"name\" value=\"{n}\"/>\n    <metadata key=\"extruder\" value=\"1\"/>\n    <part id=\"{sub}\" subtype=\"normal_part\">\n      <metadata key=\"name\" value=\"{n}\"/>\n      <metadata key=\"matrix\" value=\"1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1\"/>\n    </part>\n  </object>\n",
            n = esc(part.name)
        );
    }
    // every copy is a build item; its place in its object's instances is its instance id
    let mut items = String::new();
    let mut seen: Vec<usize> = vec![0; parts.len()];
    let mut instance: Vec<(usize, usize)> = Vec::with_capacity(set.stands.len());
    for (n, s) in set.stands.iter().enumerate() {
        let k = parts.iter().position(|p| *p == s.part).unwrap_or(0);
        instance.push((k, seen[k]));
        seen[k] += 1;
        let (lo, hi) = bounds(set.parts[s.part].mesh);
        let at = middle(lo, hi, s);
        let o = bambu_plate_origin(s.plate, set.plates, bed);
        let _ = writeln!(
            items,
            "  <item objectid=\"{}\" p:UUID=\"{}\" transform=\"{} {:.5} {:.5} {:.5}\" printable=\"1\"/>",
            2 * k + 2,
            uuid(0xb1ec, n + 1),
            rotation(s.turned),
            corner[0] + o[0] + at[0],
            corner[1] + o[1] + at[1],
            at[2]
        );
    }
    for plate in 0..set.plates {
        let _ = write!(
            model_settings,
            "  <plate>\n    <metadata key=\"plater_id\" value=\"{}\"/>\n    <metadata key=\"plater_name\" value=\"\"/>\n    <metadata key=\"locked\" value=\"false\"/>\n",
            plate + 1
        );
        for (n, s) in set.stands.iter().enumerate() {
            if s.plate != plate {
                continue;
            }
            let (k, i) = instance[n];
            let _ = write!(
                model_settings,
                "    <model_instance>\n      <metadata key=\"object_id\" value=\"{}\"/>\n      <metadata key=\"instance_id\" value=\"{i}\"/>\n    </model_instance>\n",
                2 * k + 2
            );
        }
        model_settings.push_str("  </plate>\n");
    }
    model_settings.push_str("</config>\n");
    let top = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" {NS}>\n <metadata name=\"Application\">BambuStudio-02.09.01.70</metadata>\n <metadata name=\"BambuStudio:3mfVersion\">1</metadata>\n <metadata name=\"Title\">{t}</metadata>\n <metadata name=\"Description\">dharmatiles {e}: {n} plates, system {s}, {m}</metadata>\n <resources>\n{top_objects} </resources>\n <build p:UUID=\"2c7c17d8-22b5-4d84-8835-1976022ea369\">\n{items} </build>\n</model>\n",
        t = esc(set.title),
        e = esc(&set.record.engine),
        n = set.plates,
        s = esc(&set.record.target()),
        m = if set.multicolour {
            format!(
                "filaments {}",
                fil.iter()
                    .enumerate()
                    .map(|(i, m)| format!("{} {}", i + 1, m.name()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            "single colour".to_owned()
        }
    );
    let rels_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>"
    );
    let mut all: Vec<(&str, Vec<u8>)> = vec![
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/><Default Extension="png" ContentType="image/png"/></Types>"#
                .to_vec(),
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel-1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#
                .to_vec(),
        ),
        ("3D/3dmodel.model", top.into_bytes()),
        ("3D/_rels/3dmodel.model.rels", rels_xml.into_bytes()),
    ];
    for (n, b) in &files {
        all.push((n.as_str(), b.clone()));
    }
    all.push(("Metadata/model_settings.config", model_settings.into_bytes()));
    all.push(("Metadata/project_settings.config", config.into_bytes()));
    Ok(zip(&all))
}

/// The plate of every filament slot a project needs, as the project lists
/// them: the filaments of `used` in slot order.
#[must_use]
pub fn project_filaments(parts: &[PlatePart], multicolour: bool) -> Vec<Material> {
    if !multicolour {
        return vec![Material::Earth];
    }
    let mut used: Vec<Material> = parts
        .iter()
        .flat_map(|p| p.materials.iter().copied())
        .collect();
    used.sort();
    used.dedup();
    filaments(&used)
}

/// A JSON summary of the set for the build record next to the files.
#[must_use]
pub fn summary(set: &PlateSet) -> Value {
    serde_json::json!({
        "plates": set.plates,
        "parts": set.parts.iter().map(|p| p.name).collect::<Vec<_>>(),
        "pieces": set.stands.len(),
        "multicolour": set.multicolour,
    })
}
