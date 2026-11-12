//! The plate writers on small synthetic parts.

use super::*;
use crate::bambu::unzip;

/// A box `w` x `d` x `h` with its corner at `at`: 12 triangles.
fn cuboid(at: [f64; 3], w: f64, d: f64, h: f64) -> TriMesh {
    let v = |x: f64, y: f64, z: f64| [at[0] + x, at[1] + y, at[2] + z];
    TriMesh {
        positions: vec![
            v(0.0, 0.0, 0.0),
            v(w, 0.0, 0.0),
            v(w, d, 0.0),
            v(0.0, d, 0.0),
            v(0.0, 0.0, h),
            v(w, 0.0, h),
            v(w, d, h),
            v(0.0, d, h),
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ],
        ..TriMesh::default()
    }
}

fn record() -> BuildRecord {
    BuildRecord {
        engine: "t".into(),
        document: "kit.dmap".into(),
        document_hash: 1,
        seed: 0,
        system: "db".into(),
        printer: "bambu/x1c".into(),
    }
}

const TEMPLATE: &str = r##"{"filament_colour":["#111111","#222222"],"nozzle_temperature":["220","230"],
    "printable_area":["0x0","256x0","256x256","0x256"],"flush_volumes_matrix":["0","9","9","0"]}"##;

fn two_plates<'a>(
    a: &'a TriMesh,
    am: &'a [Material],
    b: &'a TriMesh,
    bm: &'a [Material],
    r: &'a BuildRecord,
    multicolour: bool,
) -> PlateSet<'a> {
    let stand = |part, plate, x, y, turned| PlateStand {
        part,
        plate,
        x,
        y,
        turned,
    };
    PlateSet {
        title: "kit",
        parts: vec![
            PlatePart {
                name: "a",
                mesh: a,
                materials: am,
            },
            PlatePart {
                name: "b",
                mesh: b,
                materials: bm,
            },
        ],
        stands: vec![
            stand(0, 0, 5.0, 5.0, false),
            stand(0, 0, 45.0, 5.0, true),
            stand(1, 1, 5.0, 5.0, false),
            stand(0, 1, 45.0, 5.0, false),
        ],
        plates: 2,
        multicolour,
        record: r,
    }
}

fn text(z: &[u8], name: &str) -> String {
    String::from_utf8(unzip(z, name).unwrap().unwrap()).unwrap()
}

#[test]
fn plates_stand_in_bambu_studios_grid() {
    assert_eq!(
        (1..=10).map(bambu_columns).collect::<Vec<_>>(),
        [1, 2, 2, 2, 3, 3, 3, 3, 3, 4]
    );
    let bed = [256.0, 256.0];
    assert_eq!(bambu_plate_origin(0, 3, bed), [0.0, 0.0]);
    let o = bambu_plate_origin(1, 3, bed);
    assert!((o[0] - 307.2).abs() < 1e-9 && o[1].abs() < 1e-9);
    let o = bambu_plate_origin(2, 3, bed);
    assert!(
        o[0].abs() < 1e-9 && (o[1] + 307.2).abs() < 1e-9,
        "the second row lies behind the first, at negative y"
    );
}

#[test]
fn a_turned_piece_lies_where_the_packer_put_it_resting_on_the_bed() {
    // a piece 20 x 10 x 5 that comes from anywhere in the model's own space
    let a = cuboid([100.0, -50.0, -3.0], 20.0, 10.0, 5.0);
    let am = vec![Material::Earth; 12];
    let r = record();
    let set = two_plates(&a, &am, &a, &am, &r, false);
    let m = baked(&set, 0);
    // two copies: one at (5, 5), one turned at (45, 5) whose box is 10 wide, 20 deep
    let b = |from: usize| {
        m.positions[from..from + 8]
            .iter()
            .fold(([f64::MAX; 3], [f64::MIN; 3]), |(lo, hi), q| {
                (
                    [0, 1, 2].map(|i| lo[i].min(q[i])),
                    [0, 1, 2].map(|i| hi[i].max(q[i])),
                )
            })
    };
    assert_eq!(b(0), ([5.0, 5.0, 0.0], [25.0, 15.0, 5.0]));
    assert_eq!(b(8), ([45.0, 5.0, 0.0], [55.0, 25.0, 5.0]));
    // outward normals stay outward: the signed volume is positive
    let vol: f64 = m.triangles[12..24]
        .iter()
        .map(|t| {
            let p = t.map(|i| m.positions[i as usize]);
            (p[0][0] * (p[1][1] * p[2][2] - p[1][2] * p[2][1])
                - p[0][1] * (p[1][0] * p[2][2] - p[1][2] * p[2][0])
                + p[0][2] * (p[1][0] * p[2][1] - p[1][1] * p[2][0]))
                / 6.0
        })
        .sum();
    assert!((vol - 1000.0).abs() < 1e-6, "{vol}");
    // one STL per plate: its triangles are its pieces'
    let stl = plate_stl(&set, 0);
    assert_eq!(u32::from_le_bytes([stl[80], stl[81], stl[82], stl[83]]), 24);
}

#[test]
fn a_bambu_project_holds_every_plate_its_instances_and_its_paint() {
    let a = cuboid([0.0, 0.0, 0.0], 20.0, 10.0, 5.0);
    let am = [vec![Material::Earth; 6], vec![Material::Grass; 6]].concat();
    let b = cuboid([0.0, 0.0, 0.0], 10.0, 10.0, 5.0);
    let bm = vec![Material::Water; 12];
    let r = record();
    let set = two_plates(&a, &am, &b, &bm, &r, true);
    let z = bambu_plates(&set, &Palette::default(), TEMPLATE, [256.0, 256.0]).unwrap();
    let model = text(&z, "3D/3dmodel.model");
    // an object each for two distinct parts, a build item for every copy
    assert_eq!(model.matches("<object ").count(), 2);
    assert_eq!(model.matches("<item ").count(), 4);
    // the plate-2 copy of b is a bed and a fifth along: 256 x 1.2 + 5 + 10 / 2
    assert!(
        model.contains("transform=\"1 0 0 0 1 0 0 0 1 317.20000 10.00000 2.50000\""),
        "{model}"
    );
    // the turned copy carries the quarter turn: its box is 10 x 20 at (45, 5)
    assert!(
        model.contains("transform=\"0 1 0 -1 0 0 0 0 1 50.00000 15.00000 2.50000\""),
        "{model}"
    );
    let ms = text(&z, "Metadata/model_settings.config");
    assert_eq!(ms.matches("<plate>").count(), 2);
    assert_eq!(ms.matches("<model_instance>").count(), 4);
    // the copies of one part are its instances 0, 1, 2
    assert!(ms.contains("instance_id\" value=\"2\""));
    // paint: three materials in three filament slots of the project
    let o1 = text(&z, "3D/Objects/object_1.model");
    assert_eq!(
        o1.matches("paint_color=\"4\"").count(),
        6,
        "earth is filament 1"
    );
    let cfg: serde_json::Value =
        serde_json::from_str(&text(&z, "Metadata/project_settings.config")).unwrap();
    assert_eq!(cfg["filament_colour"].as_array().unwrap().len(), 3);
    // single colour: one slot, no paint at all
    let single = two_plates(&a, &am, &b, &bm, &r, false);
    let z = bambu_plates(&single, &Palette::default(), TEMPLATE, [256.0, 256.0]).unwrap();
    assert!(!text(&z, "3D/Objects/object_1.model").contains("paint_color"));
    let cfg: serde_json::Value =
        serde_json::from_str(&text(&z, "Metadata/project_settings.config")).unwrap();
    assert_eq!(cfg["filament_colour"].as_array().unwrap().len(), 1);
}

#[test]
fn a_template_for_another_bed_is_refused() {
    let a = cuboid([0.0; 3], 20.0, 10.0, 5.0);
    let am = vec![Material::Earth; 12];
    let r = record();
    let set = two_plates(&a, &am, &a, &am, &r, false);
    let e = bambu_plates(&set, &Palette::default(), TEMPLATE, [180.0, 180.0]).unwrap_err();
    assert!(e.contains("256") && e.contains("180"), "{e}");
}

#[test]
fn a_plain_3mf_per_plate_has_its_objects_and_items() {
    let a = cuboid([0.0; 3], 20.0, 10.0, 5.0);
    let am = vec![Material::Earth; 12];
    let b = cuboid([0.0; 3], 10.0, 10.0, 5.0);
    let bm = vec![Material::Water; 12];
    let r = record();
    let set = two_plates(&a, &am, &b, &bm, &r, true);
    let z = plate_3mf(&set, 1, &Palette::default());
    let m = text(&z, "3D/3dmodel.model");
    assert_eq!(m.matches("<object ").count(), 2);
    assert_eq!(m.matches("<item ").count(), 2);
    assert!(m.contains("paint_color"));
    let one = plate_3mf(
        &two_plates(&a, &am, &b, &bm, &r, false),
        1,
        &Palette::default(),
    );
    let m = text(&one, "3D/3dmodel.model");
    assert!(!m.contains("paint_color") && !m.contains("basematerials"));
}

/// The box of some positions.
fn box_at(p: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    p.iter().fold(([f64::MAX; 3], [f64::MIN; 3]), |(lo, hi), p| {
        (
            [0, 1, 2].map(|i| lo[i].min(p[i])),
            [0, 1, 2].map(|i| hi[i].max(p[i])),
        )
    })
}

#[test]
fn every_piece_lies_within_its_footprint_in_the_export_and_in_the_view() {
    // an off-centre part (its box does not start at the origin), and a thinned
    // stand-in that lost its outermost vertices, as the view's mesh does
    let full = cuboid([-7.0, 3.0, 2.0], 30.0, 12.0, 9.0);
    let thin = cuboid([-7.0, 3.0, 2.0], 22.0, 9.0, 9.0);
    let (lo, hi) = box_of(&full);
    let am = vec![Material::Earth; 12];
    let r = record();
    let set = two_plates(&full, &am, &full, &am, &r, false);
    for plate in 0..2 {
        // the export: the plate's STL is the baked mesh; read each piece's box back
        let baked = baked(&set, plate);
        let here: Vec<&PlateStand> = set.stands.iter().filter(|s| s.plate == plate).collect();
        for (n, s) in here.iter().enumerate() {
            let piece = &baked.positions[n * 8..(n + 1) * 8];
            let (a, b) = box_at(piece);
            let (w, d) = if s.turned { (12.0, 30.0) } else { (30.0, 12.0) };
            for (got, want) in [(a[0], s.x), (a[1], s.y), (b[0], s.x + w), (b[1], s.y + d), (a[2], 0.0)] {
                assert!((got - want).abs() < 1e-9, "export: {got} is not {want}");
            }
        }
    }
    // the view: the thinned mesh placed by the packer's box stays inside the footprint (centred
    // by its own, smaller box it stood off the footprint: the bug this guards)
    for turned in [false, true] {
        let (x, y) = (5.0, 45.0);
        let (w, d) = if turned { (12.0, 30.0) } else { (30.0, 12.0) };
        let p = stand_positions(&thin.positions, lo, hi, x, y, turned);
        let (a, b) = box_at(&p);
        assert!(a[0] >= x - 1e-9 && a[1] >= y - 1e-9, "view: {a:?} is past the front-left of ({x},{y})");
        assert!(b[0] <= x + w + 1e-9 && b[1] <= y + d + 1e-9, "view: {b:?} is past the far edge");
    }
}
