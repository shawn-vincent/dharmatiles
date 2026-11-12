//! Printer profiles by brand and model: the seeded library reads, a
//! machine names its bed and filaments, `from` starts one from another,
//! and the floors a machine leaves out are the default profile's.

#![allow(clippy::unwrap_used)]

use dt_eval::{ProfileKind, printer_machine, printer_names, printer_profile};

fn home(tag: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let h = std::env::temp_dir().join(format!("dt-printers-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&h);
    for (name, text) in files {
        let f = h.join("printers").join(name);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, text).unwrap();
    }
    h
}

#[test]
fn the_library_holds_the_seeded_models_by_brand() {
    let names = printer_names(None);
    let lib: Vec<&str> = names
        .iter()
        .filter(|(_, k)| *k == ProfileKind::Library)
        .map(|(n, _)| n.as_str())
        .collect();
    assert!(lib.contains(&"default") && lib.contains(&"fine"));
    for want in [
        "bambu/x1c",
        "bambu/x1c-ams",
        "bambu/a1-mini",
        "prusa/mk4",
        "sovol/sv06",
        "voron/2-4-300",
    ] {
        assert!(lib.contains(&want), "{want} in {lib:?}");
    }
    assert!(
        lib.iter().filter(|n| n.contains('/')).count() >= 25,
        "about 25 models"
    );
    // every one reads, with a bed, and a verify mark where the page marks one
    for n in &lib {
        let floors = printer_profile(n, None).unwrap_or_else(|e| panic!("{n}: {e:?}"));
        assert!(floors.layer_mm > 0.0);
        if n.contains('/') {
            let m = printer_machine(n, None).unwrap();
            assert!(m.bed.is_some(), "{n} has a bed");
            assert!(m.slot_count() >= 1);
        }
    }
}

#[test]
fn your_two_printers_are_there_as_the_page_says() {
    let x1c = printer_machine("bambu/x1c", None).unwrap();
    assert_eq!(x1c.bed, Some([256.0, 256.0]));
    assert_eq!(x1c.slot_count(), 1);
    assert!(x1c.is_bambu());
    assert!(x1c.verify.is_none(), "the X1 Carbon's numbers agree");
    // the AMS one starts from the plain one and says only its slots
    let ams = printer_machine("bambu/x1c-ams", None).unwrap();
    assert_eq!(ams.bed, Some([256.0, 256.0]));
    assert_eq!(ams.slot_count(), 4);
    assert_eq!(ams.brand.as_deref(), Some("Bambu Lab"));
    let sv06 = printer_machine("sovol/sv06", None).unwrap();
    assert_eq!(sv06.bed, Some([220.0, 220.0]));
    assert_eq!(sv06.height, Some(250.0));
    assert_eq!(sv06.slot_count(), 1);
    assert!(!sv06.is_bambu());
}

#[test]
fn rows_the_sources_disagree_on_carry_their_verify_mark() {
    for n in ["bambu/h2d", "bambu/h2c", "bambu/x2d", "voron/2-4-300"] {
        let m = printer_machine(n, None).unwrap();
        assert!(m.verify.is_some(), "{n} is marked verify");
    }
    let h2d = printer_machine("bambu/h2d", None).unwrap();
    assert!(h2d.verify.unwrap().contains("nozzle mode"));
}

#[test]
fn a_machine_leaves_the_floors_to_the_default_profile() {
    let d = printer_profile("default", None).unwrap();
    for n in ["bambu/x1c", "sovol/sv06", "bambu/x1c-ams"] {
        assert_eq!(printer_profile(n, None).unwrap(), d, "{n}");
    }
    // and the constants the decoder fills in are the library's default
    let f = dt_doc::model::DEFAULT_FLOORS;
    assert_eq!(
        (
            d.line_mm,
            d.layer_mm,
            d.overhang_deg,
            d.steepest_deg,
            d.gap_mm
        ),
        (f[0], f[1], f[2], f[3], f[4])
    );
    // while `default` and `fine` are floors only: no bed, so no plates
    assert!(printer_machine("default", None).unwrap().bed.is_none());
    let e = printer_machine("fine", None)
        .unwrap()
        .bed_or_say()
        .unwrap_err();
    assert!(e.message.contains("has no bed"), "{e:?}");
}

#[test]
fn your_own_machine_adds_zones_and_starts_from_a_library_one() {
    let h = home(
        "own",
        &[
            (
                "shed-voron.dtile",
                "dharmatiles version=1\nprinter shed-voron {\n from \"lib:printers/voron/2-4-300\"\n bed \"296x296mm\"\n slots 2\n excluded {\n  zone \"(0mm, 0mm) (30mm, 10mm)\"\n  zone \"(286mm, 286mm) (296mm, 296mm)\"\n }\n}\n",
            ),
            (
                "mine/ender.dtile",
                "dharmatiles version=1\nprinter ender {\n bed \"235x235mm\"\n line \"0.5mm\"\n}\n",
            ),
        ],
    );
    let m = printer_machine("shed-voron", Some(&h)).unwrap();
    assert_eq!(m.bed, Some([296.0, 296.0]));
    assert_eq!(m.slots, Some(2));
    assert_eq!(
        m.excluded,
        [[0.0, 0.0, 30.0, 10.0], [286.0, 286.0, 296.0, 296.0]]
    );
    // what it does not say is the parent's
    assert_eq!(m.brand.as_deref(), Some("Voron Design"));
    assert_eq!(m.height, Some(300.0));
    assert!(m.verify.is_some());
    // yours is listed, in a folder of its own too
    let names = printer_names(Some(&h));
    assert!(names.contains(&("shed-voron".to_owned(), ProfileKind::Mine)));
    assert!(names.contains(&("mine/ender".to_owned(), ProfileKind::Mine)));
    // a floor it names is its own, the others the default's
    let p = printer_profile("mine/ender", Some(&h)).unwrap();
    assert_eq!(p.line_mm, 0.5);
    assert_eq!(p.layer_mm, 0.2);
    // yours replaces the library's of the same name
    let own = home(
        "replace",
        &[(
            "bambu/x1c.dtile",
            "dharmatiles version=1\nprinter x1c {\n bed \"200x200mm\"\n}\n",
        )],
    );
    assert_eq!(
        printer_machine("bambu/x1c", Some(&own)).unwrap().bed,
        Some([200.0, 200.0])
    );
    let _ = std::fs::remove_dir_all(h);
    let _ = std::fs::remove_dir_all(own);
}

#[test]
fn a_mistake_in_a_machine_says_where() {
    let bad = |body: &str| {
        let h = home(
            "bad",
            &[(
                "b.dtile",
                &format!("dharmatiles version=1\nprinter b {{\n{body}\n}}\n"),
            )],
        );
        let e = printer_machine("b", Some(&h)).unwrap_err();
        let _ = std::fs::remove_dir_all(&h);
        e.iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; ")
    };
    assert!(bad(" bed \"256x256\"").contains("no unit"));
    assert!(bad(" bed \"256mm\"").contains("not a size"));
    assert!(bad(" bed \"0x256mm\"").contains("positive"));
    assert!(bad(" bed \"256x256mm\"\n slots 0").contains("slots"));
    assert!(bad(" bed \"256x256mm\"\n slots 2.5").contains("slots"));
    assert!(
        bad(" bed \"256x256mm\"\n excluded {\n zone \"(0mm, 0mm)\"\n }").contains("not a zone")
    );
    assert!(
        bad(" bed \"256x256mm\"\n excluded {\n zone \"(0mm, 0mm) (0mm, 9mm)\"\n }")
            .contains("area")
    );
    assert!(bad(" bed \"256x256mm\"\n bd \"1x1mm\"").contains("no `bd`"));
    // no bed and no floors: still the old complaint
    assert!(bad(" slots 2").contains("no line"));
    // a loop of `from`
    let loopy = home(
        "loop",
        &[(
            "l.dtile",
            "dharmatiles version=1\nprinter l {\n from \"l\"\n}\n",
        )],
    );
    let e = printer_machine("l", Some(&loopy)).unwrap_err();
    assert!(e[0].message.contains("starts from itself"));
    let _ = std::fs::remove_dir_all(loopy);
    // a name with two slashes is not a name
    assert!(printer_machine("a/b/c", None).is_err());
    assert!(printer_machine("../x", None).is_err());
    assert!(printer_machine("bambu/", None).is_err());
}

#[test]
fn a_brand_profile_names_a_build_target_folder_without_a_slash() {
    assert_eq!(dt_kit::target_name("db", "bambu/x1c"), "db-bambu-x1c");
    assert_eq!(dt_kit::target_name("ol", "default"), "ol");
    let mut o = dt_eval::Options::default();
    o.use_printer("bambu/x1c", None).unwrap();
    assert_eq!(o.target(), "db-bambu-x1c");
}

#[test]
fn the_printers_page_lists_every_library_profile() {
    let page = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/printers.html"),
    )
    .unwrap();
    for (n, k) in printer_names(None) {
        if k == ProfileKind::Library && n.contains('/') {
            assert!(
                page.contains(&format!("<code>{n}</code>")),
                "{n} is on docs/printers.html"
            );
        }
    }
    // a verify row says so on the page
    assert!(page.contains("verify"));
}
