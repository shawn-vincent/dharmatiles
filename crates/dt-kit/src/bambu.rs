//! The tile as a Bambu Studio project: the same painted mesh as
//! [`crate::threemf`], laid out the way Bambu Studio saves its own
//! projects, with one filament slot per material the tile uses, each
//! coloured from the palette. Bambu Studio then opens it with exactly
//! those slots. The slots keep the template's filament profile name (a
//! name Bambu Studio does not know would make it warn about custom presets
//! on every open); the slot order is in the model's metadata.
//!
//! Everything else in a Bambu project (printer, process, the filaments'
//! own settings and G-code) comes from a template: a project the user
//! saved in Bambu Studio. Nothing of Bambu's is shipped here. Each slot
//! takes the template's first filament's settings.

use std::fmt::Write as _;
use std::io::Read as _;

use dt_geom::TriMesh;
use serde_json::{Map, Value};

use crate::threemf::{byte, filament_of, filaments, paint_code, zip};
use crate::{BuildRecord, Material, Palette};

/// The settings Bambu Studio keeps one value (or a fixed run of values) per
/// filament slot, by name. Arrays of other lengths (one per extruder, one
/// per axis) are left alone.
const PER_FILAMENT: &[&str] = &[
    "activate_air_filtration",
    "additional_cooling_fan_speed",
    "additional_fan_full_speed_layer",
    "chamber_temperatures",
    "circle_compensation_speed",
    "close_additional_fan_first_x_layers",
    "close_fan_the_first_x_layers",
    "complete_print_exhaust_fan_speed",
    "cool_plate_temp",
    "cool_plate_temp_initial_layer",
    "cooling_perimeter_transition_distance",
    "cooling_slowdown_logic",
    "counter_coef_1",
    "counter_coef_2",
    "counter_coef_3",
    "counter_limit_max",
    "counter_limit_min",
    "default_filament_colour",
    "diameter_limit",
    "during_print_exhaust_fan_speed",
    "enable_overhang_bridge_fan",
    "enable_pressure_advance",
    "eng_plate_temp",
    "eng_plate_temp_initial_layer",
    "fan_cooling_layer_time",
    "fan_max_speed",
    "fan_min_speed",
    "filament_adaptive_volumetric_speed",
    "filament_adhesiveness_category",
    "filament_bridge_speed",
    "filament_change_length",
    "filament_change_length_nc",
    "filament_colour",
    "filament_colour_type",
    "filament_cooling_before_tower",
    "filament_cost",
    "filament_density",
    "filament_deretraction_speed",
    "filament_dev_ams_drying_ams_limitations",
    "filament_dev_ams_drying_heat_distortion_temperature",
    "filament_dev_ams_drying_temperature",
    "filament_dev_ams_drying_time",
    "filament_dev_chamber_drying_bed_temperature",
    "filament_dev_chamber_drying_time",
    "filament_dev_drying_cooling_temperature",
    "filament_dev_drying_softening_temperature",
    "filament_diameter",
    "filament_enable_overhang_speed",
    "filament_end_gcode",
    "filament_extruder_compatibility",
    "filament_extruder_variant",
    "filament_flow_ratio",
    "filament_flush_temp",
    "filament_flush_temp_fast",
    "filament_flush_volumetric_speed",
    "filament_ids",
    "filament_is_mixed",
    "filament_is_support",
    "filament_long_retractions_when_cut",
    "filament_map",
    "filament_max_volumetric_speed",
    "filament_metal_stickiness",
    "filament_minimal_purge_on_wipe_tower",
    "filament_mixed_gradient",
    "filament_mixed_gradient_curve",
    "filament_mixed_gradient_per_part",
    "filament_multi_colour",
    "filament_nozzle_map",
    "filament_overhang_1_4_speed",
    "filament_overhang_2_4_speed",
    "filament_overhang_3_4_speed",
    "filament_overhang_4_4_speed",
    "filament_overhang_totally_speed",
    "filament_pre_cooling_temperature",
    "filament_pre_cooling_temperature_nc",
    "filament_preheat_temperature_delta",
    "filament_prime_volume",
    "filament_prime_volume_nc",
    "filament_printable",
    "filament_ramming_travel_time",
    "filament_ramming_travel_time_nc",
    "filament_ramming_volumetric_speed",
    "filament_ramming_volumetric_speed_nc",
    "filament_retract_before_wipe",
    "filament_retract_length_nc",
    "filament_retract_restart_extra",
    "filament_retract_when_changing_layer",
    "filament_retraction_distances_when_cut",
    "filament_retraction_length",
    "filament_retraction_minimum_travel",
    "filament_retraction_speed",
    "filament_scarf_gap",
    "filament_scarf_height",
    "filament_scarf_length",
    "filament_scarf_seam_type",
    "filament_self_index",
    "filament_settings_id",
    "filament_shrink",
    "filament_soluble",
    "filament_start_gcode",
    "filament_tower_interface_pre_extrusion_dist",
    "filament_tower_interface_pre_extrusion_length",
    "filament_tower_interface_print_temp",
    "filament_tower_interface_purge_volume",
    "filament_tower_ironing_area",
    "filament_type",
    "filament_velocity_adaptation_factor",
    "filament_vendor",
    "filament_volume_map",
    "filament_wipe",
    "filament_wipe_distance",
    "filament_z_hop",
    "filament_z_hop_types",
    "first_x_layer_fan_speed",
    "first_x_layer_part_fan_speed",
    "flush_volumes_vector",
    "full_fan_speed_layer",
    "hole_coef_1",
    "hole_coef_2",
    "hole_coef_3",
    "hole_limit_max",
    "hole_limit_min",
    "hot_plate_temp",
    "hot_plate_temp_initial_layer",
    "impact_strength_z",
    "ironing_fan_speed",
    "long_retractions_when_ec",
    "no_slow_down_for_cooling_on_outwalls",
    "nozzle_temperature",
    "nozzle_temperature_initial_layer",
    "nozzle_temperature_range_high",
    "nozzle_temperature_range_low",
    "overhang_fan_speed",
    "overhang_fan_threshold",
    "overhang_threshold_participating_cooling",
    "override_process_overhang_speed",
    "pre_start_fan_time",
    "pressure_advance",
    "reduce_fan_stop_start_freq",
    "required_nozzle_HRC",
    "retraction_distances_when_ec",
    "slow_down_for_layer_cooling",
    "slow_down_layer_time",
    "slow_down_min_speed",
    "supertack_plate_temp",
    "supertack_plate_temp_initial_layer",
    "temperature_vitrification",
    "textured_plate_temp",
    "textured_plate_temp_initial_layer",
    "volumetric_speed_coefficients",
];

/// `Metadata/project_settings.config` of a Bambu Studio project.
///
/// # Errors
/// When the bytes are not a zip, or hold no project settings (the file was
/// not saved by Bambu Studio).
pub fn project_settings(threemf: &[u8]) -> Result<String, String> {
    let bytes = unzip(threemf, "Metadata/project_settings.config")?.ok_or(
        "it has no Metadata/project_settings.config: save it from Bambu Studio as a project",
    )?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// One entry of a zip, inflated; `None` when the zip has no such entry.
fn unzip(z: &[u8], want: &str) -> Result<Option<Vec<u8>>, String> {
    let u16_at = |i: usize| {
        z.get(i..i + 2)
            .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
    };
    let u32_at = |i: usize| {
        z.get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let bad = || "not a zip".to_owned();
    // the end-of-central-directory record, searched from the end
    let eocd = (0..z.len().saturating_sub(21))
        .rev()
        .find(|&i| z[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or_else(bad)?;
    let count = u16_at(eocd + 10).ok_or_else(bad)?;
    let mut at = u32_at(eocd + 16).ok_or_else(bad)?;
    for _ in 0..count {
        if z.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err(bad());
        }
        let method = u16_at(at + 10).ok_or_else(bad)?;
        let packed = u32_at(at + 20).ok_or_else(bad)?;
        let name_len = u16_at(at + 28).ok_or_else(bad)?;
        let extra = u16_at(at + 30).ok_or_else(bad)?;
        let comment = u16_at(at + 32).ok_or_else(bad)?;
        let local = u32_at(at + 42).ok_or_else(bad)?;
        let name = z.get(at + 46..at + 46 + name_len).ok_or_else(bad)?;
        if name == want.as_bytes() {
            let skip = u16_at(local + 26).ok_or_else(bad)? + u16_at(local + 28).ok_or_else(bad)?;
            let start = local + 30 + skip;
            let data = z.get(start..start + packed).ok_or_else(bad)?;
            return match method {
                0 => Ok(Some(data.to_vec())),
                8 => {
                    let mut out = Vec::new();
                    flate2::read::DeflateDecoder::new(data)
                        .read_to_end(&mut out)
                        .map_err(|e| format!("{want}: {e}"))?;
                    Ok(Some(out))
                }
                m => Err(format!("{want}: zip method {m} is not supported")),
            };
        }
        at += 46 + name_len + extra + comment;
    }
    Ok(None)
}

/// The template's settings, cut to one filament per colour of `slots`.
fn settings(template: &str, slots: &[String]) -> Result<String, String> {
    let mut d: Map<String, Value> = serde_json::from_str(template)
        .map_err(|e| format!("the template's project settings: {e}"))?;
    let n = d
        .get("filament_colour")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if n == 0 {
        return Err("the template's project settings name no filaments".into());
    }
    let k = slots.len();
    for key in PER_FILAMENT {
        if let Some(Value::Array(v)) = d.get_mut(*key)
            && !v.is_empty()
            && v.len() % n == 0
        {
            let per = v.len() / n;
            let first: Vec<Value> = v[..per].to_vec();
            *v = (0..k).flat_map(|_| first.iter().cloned()).collect();
        }
    }
    // the purge between two filaments: the template's first off-diagonal
    // value, or Bambu's usual 280 mm³
    if let Some(Value::Array(v)) = d.get_mut("flush_volumes_matrix") {
        let f = if n > 1 { v.get(1).cloned() } else { None }
            .unwrap_or_else(|| Value::String("280".into()));
        let blocks = (v.len() / (n * n)).max(1);
        *v = (0..blocks * k * k)
            .map(|i| {
                let (r, c) = ((i % (k * k)) / k, i % k);
                if r == c {
                    Value::String("0".into())
                } else {
                    f.clone()
                }
            })
            .collect();
    }
    // process, the filaments, printer
    for key in ["different_settings_to_system", "inherits_group"] {
        if let Some(Value::Array(v)) = d.get_mut(key)
            && v.len() == n + 2
        {
            let (first, fil, last) = (v[0].clone(), v[1].clone(), v[n + 1].clone());
            *v = std::iter::once(first)
                .chain(std::iter::repeat_n(fil, k))
                .chain(std::iter::once(last))
                .collect();
        }
    }
    let strings = |f: &dyn Fn(usize, &String) -> String| -> Value {
        Value::Array(
            slots
                .iter()
                .enumerate()
                .map(|(i, s)| Value::String(f(i, s)))
                .collect(),
        )
    };
    d.insert("filament_colour".into(), strings(&|_, s| s.clone()));
    d.insert("filament_multi_colour".into(), strings(&|_, s| s.clone()));
    d.insert(
        "filament_self_index".into(),
        strings(&|i, _| (i + 1).to_string()),
    );
    serde_json::to_string_pretty(&Value::Object(d)).map_err(|e| e.to_string())
}

/// The middle of the template's bed, mm (256 × 256 when it does not say).
fn bed_centre(template: &str) -> [f64; 2] {
    let v: Value = serde_json::from_str(template).unwrap_or(Value::Null);
    let pts: Vec<[f64; 2]> = v
        .get("printable_area")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let (x, y) = p.as_str()?.split_once('x')?;
                    Some([x.parse().ok()?, y.parse().ok()?])
                })
                .collect()
        })
        .unwrap_or_default();
    if pts.is_empty() {
        return [128.0, 128.0];
    }
    let (lo, hi) = pts
        .iter()
        .fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), p| {
            (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            )
        });
    [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]
}

/// **bambu_project**: the 3MF bytes of `mesh` as a Bambu Studio project,
/// standing on the middle of the bed, one filament slot per material
/// (coloured from `palette`, earth first), with `template`'s
/// other settings (see [`project_settings`]).
///
/// # Errors
/// When the template's settings cannot be read.
pub fn bambu_project(
    mesh: &TriMesh,
    materials: &[Material],
    record: &BuildRecord,
    palette: &Palette,
    template: &str,
) -> Result<Vec<u8>, String> {
    let mut used: Vec<Material> = materials.to_vec();
    used.sort();
    used.dedup();
    let fil = filaments(&used);
    let slots: Vec<String> = fil
        .iter()
        .map(|m| {
            let [r, g, b] = palette.colour(*m).map(byte);
            format!("#{r:02X}{g:02X}{b:02X}")
        })
        .collect();
    let codes: Vec<String> = used
        .iter()
        .map(|m| paint_code(fil.iter().position(|f| *f == filament_of(*m)).unwrap_or(0) + 1))
        .collect();
    let config = settings(template, &slots)?;
    // the object's origin is the middle of its box; the item stands it on
    // the middle of the bed
    let (lo, hi) = mesh
        .positions
        .iter()
        .fold(([f64::MAX; 3], [f64::MIN; 3]), |(lo, hi), p| {
            (
                [0, 1, 2].map(|i| lo[i].min(p[i])),
                [0, 1, 2].map(|i| hi[i].max(p[i])),
            )
        });
    let mid = [0, 1, 2].map(|i| (lo[i] + hi[i]) / 2.0);
    let bed = bed_centre(template);
    let name = std::path::Path::new(&record.document)
        .file_stem()
        .map_or_else(|| "tile".to_owned(), |s| s.to_string_lossy().into_owned());
    const NS: &str = "xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\" xmlns:BambuStudio=\"http://schemas.bambulab.com/package/2021\" xmlns:p=\"http://schemas.microsoft.com/3dmanufacturing/production/2015/06\" requiredextensions=\"p\"";

    let mut o = String::with_capacity(48 * mesh.triangles.len() + 40 * mesh.positions.len());
    let _ = write!(
        o,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" {NS}>\n <metadata name=\"BambuStudio:3mfVersion\">1</metadata>\n <resources>\n  <object id=\"1\" p:UUID=\"00010000-81cb-4c03-9d28-80fed5dfa1dc\" type=\"model\">\n   <mesh>\n    <vertices>\n"
    );
    for p in &mesh.positions {
        let _ = writeln!(
            o,
            "     <vertex x=\"{:.5}\" y=\"{:.5}\" z=\"{:.5}\"/>",
            p[0] - mid[0],
            p[1] - mid[1],
            p[2] - mid[2]
        );
    }
    o.push_str("    </vertices>\n    <triangles>\n");
    for (t, m) in mesh.triangles.iter().zip(materials) {
        let k = used.iter().position(|u| u == m).unwrap_or(0);
        let _ = writeln!(
            o,
            "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" paint_color=\"{}\"/>",
            t[0], t[1], t[2], codes[k]
        );
    }
    o.push_str("    </triangles>\n   </mesh>\n  </object>\n </resources>\n <build/>\n</model>\n");

    let top = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" {NS}>\n <metadata name=\"Application\">BambuStudio-02.09.01.70</metadata>\n <metadata name=\"BambuStudio:3mfVersion\">1</metadata>\n <metadata name=\"Title\">{t}</metadata>\n <metadata name=\"Description\">dharmatiles {e}: system {s}, seed {seed}, document {h:016x}; filaments {f}</metadata>\n <resources>\n  <object id=\"2\" p:UUID=\"00000001-61cb-4c03-9d28-80fed5dfa1dc\" type=\"model\">\n   <components>\n    <component p:path=\"/3D/Objects/object_1.model\" objectid=\"1\" p:UUID=\"00010000-b206-40ff-9872-83e8017abed1\" transform=\"1 0 0 0 1 0 0 0 1 0 0 0\"/>\n   </components>\n  </object>\n </resources>\n <build p:UUID=\"2c7c17d8-22b5-4d84-8835-1976022ea369\">\n  <item objectid=\"2\" p:UUID=\"00000002-b1ec-4553-aec9-835e5b724bb4\" transform=\"1 0 0 0 1 0 0 0 1 {x:.5} {y:.5} {z:.5}\" printable=\"1\"/>\n </build>\n</model>\n",
        t = esc(&name),
        e = esc(&record.engine),
        s = esc(&record.system),
        seed = record.seed,
        h = record.document_hash,
        f = fil
            .iter()
            .enumerate()
            .map(|(i, m)| format!("{} {}", i + 1, m.name()))
            .collect::<Vec<_>>()
            .join(", "),
        x = bed[0],
        y = bed[1],
        z = mid[2] - lo[2],
    );
    let settings_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n  <object id=\"2\">\n    <metadata key=\"name\" value=\"{n}\"/>\n    <metadata key=\"extruder\" value=\"1\"/>\n    <part id=\"1\" subtype=\"normal_part\">\n      <metadata key=\"name\" value=\"{n}\"/>\n      <metadata key=\"matrix\" value=\"1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1\"/>\n    </part>\n  </object>\n  <plate>\n    <metadata key=\"plater_id\" value=\"1\"/>\n    <metadata key=\"plater_name\" value=\"\"/>\n    <metadata key=\"locked\" value=\"false\"/>\n    <model_instance>\n      <metadata key=\"object_id\" value=\"2\"/>\n      <metadata key=\"instance_id\" value=\"0\"/>\n    </model_instance>\n  </plate>\n</config>\n",
        n = esc(&name)
    );
    Ok(zip(&[
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
        (
            "3D/_rels/3dmodel.model.rels",
            br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/Objects/object_1.model" Id="rel-1" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#
                .to_vec(),
        ),
        ("3D/Objects/object_1.model", o.into_bytes()),
        ("Metadata/model_settings.config", settings_xml.into_bytes()),
        ("Metadata/project_settings.config", config.into_bytes()),
    ]))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_keep_one_slot_per_material() {
        let t = r##"{"filament_colour":["#111111","#222222","#333333"],
            "nozzle_temperature":["220","230","240"],
            "flush_volumes_vector":["1","2","3","4","5","6"],
            "flush_volumes_matrix":["0","9","9","9","0","9","9","9","0"],
            "nozzle_diameter":["0.4"],
            "inherits_group":["process","f1","f2","f3","printer"]}"##;
        let slots = ["#806140".to_owned(), "#3D618F".to_owned()];
        let v: Value = serde_json::from_str(&settings(t, &slots).unwrap()).unwrap();
        assert_eq!(
            v["filament_colour"],
            serde_json::json!(["#806140", "#3D618F"])
        );
        assert_eq!(v["nozzle_temperature"], serde_json::json!(["220", "220"]));
        assert_eq!(
            v["flush_volumes_vector"],
            serde_json::json!(["1", "2", "1", "2"])
        );
        assert_eq!(
            v["flush_volumes_matrix"],
            serde_json::json!(["0", "9", "9", "0"])
        );
        assert_eq!(v["nozzle_diameter"], serde_json::json!(["0.4"]));
        assert_eq!(
            v["inherits_group"],
            serde_json::json!(["process", "f1", "f1", "printer"])
        );
    }

    #[test]
    fn a_project_reads_back_its_settings() {
        let z = zip(&[("Metadata/project_settings.config", b"{\"a\":1}".to_vec())]);
        assert_eq!(project_settings(&z).unwrap(), "{\"a\":1}");
        let none = zip(&[("x", b"y".to_vec())]);
        assert!(project_settings(&none).is_err());
    }
}
