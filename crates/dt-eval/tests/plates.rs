//! Plans and layouts: what a print list resolves to, and what the real
//! footprints do to the plates.

#![allow(clippy::unwrap_used)]

use dt_doc::map::{Group, Mode, decode};
use dt_eval::plates::{self, Boxes, Context, Overrides};
use dt_eval::{Files, Options};
use dt_kit::Material;
use std::path::Path;

fn plan_of(text: &str, ov: &Overrides) -> Result<plates::Plan, Vec<dt_eval::Diag>> {
    let list = decode("kit.dmap", &format!("dharmatiles version=1\n{text}")).unwrap();
    let base = Options::default();
    let ctx = Context {
        sources: &Files,
        list_dir: Some(Path::new("/nowhere")),
        home: None,
        base: &base,
    };
    plates::plan(&list, "kit", &ctx, ov)
}

#[test]
fn a_list_resolves_its_tiles_merges_copies_and_takes_the_defaults() {
    let p = plan_of(
        "map {\n piece \"lib:tests/ground/1x1-flat\" count=2\n piece \"lib:tests/ground/1x1-flat.dtile\" count=3\n piece \"lib:tests/ground/1x1-flat\" seed=9\n piece \"lib:tests/ground/1x1-soil\" print=skip\n}",
        &Overrides::default(),
    )
    .unwrap();
    // two names for one tile at one seed are one piece of five; a seed makes another
    assert_eq!(p.kinds.len(), 2);
    assert_eq!(p.kinds[0].count, 5);
    assert_eq!(p.kinds[1].seed, Some(9));
    assert_eq!(p.kinds[0].path, "lib:tests/ground/1x1-flat.dtile");
    assert_eq!(p.skipped, ["lib:tests/ground/1x1-soil"]);
    assert_eq!(p.pieces(), 6);
    // the printer a list that names none gets: the X1 Carbon with an AMS, multicolour
    let s = &p.settings;
    assert_eq!(s.machine.name, "bambu/x1c-ams");
    assert_eq!((s.mode, s.group), (Mode::Multicolour, Group::Filament));
    assert_eq!((s.gap, s.margin, s.turn), (3.0, 5.0, true));
    assert_eq!(s.system, "db");
    // a square is a square: 35 mm on DungeonBlocks, 25.4 on OpenLOCK
    assert_eq!(p.kinds[0].nominal, [35.0, 35.0]);
    let ol = plan_of(
        "map {\n piece \"lib:tests/ground/1x1-flat\"\n piece \"lib:tests/ground/1x1-soil\" system=\"db\"\n}",
        &Overrides {
            system: Some("ol".into()),
            ..Overrides::default()
        },
    )
    .unwrap();
    assert_eq!(ol.kinds[0].nominal, [25.4, 25.4]);
    assert_eq!(
        ol.kinds[1].nominal,
        [35.0, 35.0],
        "a piece's own system wins: both on one plate"
    );
}

#[test]
fn a_one_filament_printer_defaults_to_single_colour_and_a_word_overrides() {
    let l =
        "map {\n piece \"lib:tests/ground/1x1-flat\"\n}\nplates printer=\"sovol/sv06\" gap=\"4mm\"";
    let p = plan_of(l, &Overrides::default()).unwrap();
    assert_eq!(p.settings.mode, Mode::Single);
    assert_eq!(p.settings.gap, 4.0);
    let q = plan_of(
        l,
        &Overrides {
            gap: Some(1.0),
            printer: Some("bambu/a1-mini".into()),
            ..Overrides::default()
        },
    )
    .unwrap();
    assert_eq!(q.settings.gap, 1.0);
    assert_eq!(q.settings.machine.bed, Some([180.0, 180.0]));
}

#[test]
fn the_filament_estimate_never_leaves_a_material_out() {
    let p = plan_of(
        "map {\n piece \"lib:tests/plants/1x1-tree\"\n piece \"lib:tests/water/1x1-water+grass\"\n}",
        &Overrides {
            mode: Some(Mode::Single),
            ..Overrides::default()
        },
    )
    .unwrap();
    let names = |k: usize| -> Vec<&str> { p.kinds[k].estimate.iter().map(|m| m.name()).collect() };
    assert!(
        names(0).contains(&"wood") && names(0).contains(&"leaf") && names(0).contains(&"earth")
    );
    assert!(names(1).contains(&"water") && names(1).contains(&"grass"));
}

fn boxes(size: [f64; 2], f: &[Material]) -> Boxes {
    Boxes {
        size,
        height: 20.0,
        filaments: f.to_vec(),
    }
}

#[test]
fn the_real_footprints_make_the_plates_and_a_crown_costs_room() {
    let p = plan_of(
        "map {\n piece \"lib:tests/ground/1x1-flat\" count=30\n}\nplates printer=\"bambu/x1c\" mode=\"single\"",
        &Overrides::default(),
    )
    .unwrap();
    let flat = plates::layout(&p, &[boxes([35.0, 35.0], &[])]).unwrap();
    assert_eq!(flat.plates.len(), 1, "6 x 6 squares on a 256 bed");
    // the same thirty with a crown 9.4 mm past the square: five a row, five rows
    let crown = plates::layout(&p, &[boxes([44.3, 44.4], &[])]).unwrap();
    assert_eq!(crown.plates.len(), 2);
    assert_eq!(crown.plates[0].len(), 25);
    assert_eq!(crown.pieces(), 30);
}

#[test]
fn what_cannot_be_laid_out_is_said_by_tile() {
    let p = plan_of(
        "map {\n piece \"lib:tests/ground/1x1-flat\"\n}\nplates printer=\"sovol/sv06\"",
        &Overrides::default(),
    )
    .unwrap();
    let e = plates::layout(&p, &[boxes([300.0, 40.0], &[])]).unwrap_err();
    assert!(
        e[0].message.contains("1x1-flat") && e[0].message.contains("220"),
        "{e:?}"
    );
    let tall = Boxes {
        height: 300.0,
        ..boxes([35.0, 35.0], &[])
    };
    let e = plates::layout(&p, &[tall]).unwrap_err();
    assert!(e[0].message.contains("250"), "{e:?}");
    // a build that needs more filaments than the printer has
    let m = plan_of(
        "map {\n piece \"lib:tests/ground/1x1-flat\"\n}\nplates printer=\"bambu/x1c-ams\" mode=\"multicolour\"",
        &Overrides::default(),
    )
    .unwrap();
    let five = [
        Material::Earth,
        Material::Grass,
        Material::Water,
        Material::Rock,
        Material::Wood,
    ];
    let e = plates::layout(&m, &[boxes([35.0, 35.0], &five)]).unwrap_err();
    assert!(
        e[0].message
            .contains("needs 5 filaments, this printer has 4"),
        "{e:?}"
    );
}

#[test]
fn tile_paths_are_the_librarys_yours_or_beside_the_list() {
    let t = |p: &str, d: Option<&str>, h: Option<&str>| {
        plates::tile_path(p, d.map(Path::new), h.map(Path::new))
    };
    assert_eq!(
        t("lib:tiles/ground/1x1-grass", None, None).unwrap(),
        "lib:tiles/ground/1x1-grass.dtile"
    );
    assert_eq!(
        t("home:meadows/glade", None, Some("/h")).unwrap(),
        "/h/meadows/glade.dtile"
    );
    assert_eq!(
        t("glade", Some("/lists"), None).unwrap(),
        "/lists/glade.dtile"
    );
    assert_eq!(
        t("/abs/glade.dtile", None, None).unwrap(),
        "/abs/glade.dtile"
    );
    assert!(
        t("home:x", None, None)
            .unwrap_err()
            .contains("tiles folder")
    );
    assert!(t("glade", None, None).unwrap_err().contains("relative"));
}
