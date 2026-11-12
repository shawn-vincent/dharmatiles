//! The importer on small fixtures written to a scratch folder: the inherits
//! chains, a bed that is not a rectangle, the AMS profile, the profiles it
//! writes reading back through the real decoder, and a refusal to overwrite.

use std::path::PathBuf;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dt-slicers-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch folder");
    d
}

fn put(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn bambu_fixture(root: &Path) {
    put(
        root,
        "BBL/machine/common.json",
        r#"{"type":"machine","name":"common","instantiation":"false",
        "nozzle_diameter":["0.4"],"printable_height":"250",
        "printable_area":["0x0","256x0","256x256","0x256"],
        "bed_exclude_area":["0x0","28x0","28x28","0x28","0x28","8x28","8x256","0x256"]}"#,
    );
    put(
        root,
        "BBL/machine/Bambu Lab X1 Carbon 0.4 nozzle.json",
        r#"{"type":"machine","name":"Bambu Lab X1 Carbon 0.4 nozzle","inherits":"common",
        "instantiation":"true","printer_model":"Bambu Lab X1 Carbon",
        "bed_exclude_area":["0x0","18x0","18x28","0x28"]}"#,
    );
    put(
        root,
        "BBL/machine/Bambu Lab X1 Carbon 0.6 nozzle.json",
        r#"{"type":"machine","name":"Bambu Lab X1 Carbon 0.6 nozzle","inherits":"common",
        "instantiation":"true","printer_model":"Bambu Lab X1 Carbon","nozzle_diameter":["0.6"]}"#,
    );
    put(
        root,
        "BBL/machine/Bambu Lab H2D 0.4 nozzle.json",
        r#"{"type":"machine","name":"Bambu Lab H2D 0.4 nozzle","inherits":"common",
        "instantiation":"true","printer_model":"Bambu Lab H2D","nozzle_diameter":["0.4","0.4"],
        "printable_area":["0x0","350x0","350x320","0x320"],"printable_height":"325","bed_exclude_area":[]}"#,
    );
    put(
        root,
        "Delta/machine/Round 0.4 nozzle.json",
        r#"{"type":"machine","name":"Round 0.4 nozzle","instantiation":"true",
        "printable_area":["100x0","70x70","0x100","-70x70","-100x0","-70x-70","0x-100","70x-70"]}"#,
    );
    put(root, "BBL/machine/broken.json", "{ not json");
}

fn install(root: &Path) -> Install {
    Install {
        slicer: Slicer::Bambu,
        version: "2.9.1".into(),
        roots: vec![root.to_path_buf()],
    }
}

#[test]
fn bambu_profiles_follow_inherits_and_the_childs_own_exclusion_wins() {
    let root = scratch("bambu");
    bambu_fixture(&root);
    let s = scan(&install(&root));
    let names: Vec<&str> = s.candidates.iter().map(|c| c.model.as_str()).collect();
    assert_eq!(
        names,
        ["H2D", "X1 Carbon"],
        "one per model, the maker's name dropped"
    );
    let x1 = &s.candidates[1];
    assert_eq!(x1.brand, "Bambu Lab");
    assert_eq!(
        x1.profile, "Bambu Lab X1 Carbon 0.4 nozzle",
        "the 0.4 nozzle's profile"
    );
    assert_eq!(x1.bed, [256.0, 256.0]);
    assert_eq!(x1.height, Some(250.0), "the height comes from the parent");
    assert_eq!(x1.excluded, vec![[0.0, 0.0, 18.0, 28.0]]);
    assert_eq!(x1.slots, 1);
    let h2d = &s.candidates[0];
    assert_eq!(h2d.slots, 2, "two nozzles are two filaments at once");
    assert!(h2d.excluded.is_empty());
    // the parent's flat list of corners is read as rectangles where the child says nothing
    let parent = chain_zones();
    assert_eq!(
        parent,
        vec![[0.0, 0.0, 28.0, 28.0], [0.0, 28.0, 8.0, 256.0]]
    );
}

fn chain_zones() -> Vec<[f64; 4]> {
    let pts: Vec<[f64; 2]> = [
        "0x0", "28x0", "28x28", "0x28", "0x28", "8x28", "8x256", "0x256",
    ]
    .iter()
    .filter_map(|s| point(s))
    .collect();
    zones_of(&pts, [0.0, 0.0], [256.0, 256.0])
}

#[test]
fn what_cannot_be_read_is_said_not_dropped() {
    let root = scratch("problems");
    bambu_fixture(&root);
    let s = scan(&install(&root));
    assert!(
        s.problems.iter().any(|p| p.contains("broken.json")),
        "{:?}",
        s.problems
    );
    assert!(
        s.problems.iter().any(|p| p.contains("not a rectangle")),
        "a round bed is said: {:?}",
        s.problems
    );
}

#[test]
fn prusa_sections_merge_their_parents_in_order_and_an_mmu_is_its_filaments() {
    let root = scratch("prusa");
    put(
        &root,
        "PrusaResearch.ini",
        "[vendor]\nname = Prusa Research\n\n\
         [printer:*common*]\nbed_shape = 0x0,250x0,250x210,0x210\nmax_print_height = 210\nnozzle_diameter = 0.4\nprinter_technology = FFF\n\n\
         [printer:*mmu*]\nnozzle_diameter = 0.4,0.4,0.4,0.4,0.4\n\n\
         [printer:Original Prusa MK4 0.4 nozzle]\ninherits = *common*\nmax_print_height = 220\n\n\
         [printer:Original Prusa MK4 MMU3 0.4 nozzle]\ninherits = *common*; *mmu*\nmax_print_height = 220\n\n\
         [printer:Original Prusa SL1]\nprinter_technology = SLA\nbed_shape = 0x0,1x0,1x1,0x1\n",
    );
    let i = Install {
        slicer: Slicer::Prusa,
        version: "2.9.0".into(),
        roots: vec![root],
    };
    let s = scan(&i);
    assert!(s.problems.is_empty(), "{:?}", s.problems);
    let by: Vec<(&str, u32)> = s
        .candidates
        .iter()
        .map(|c| (c.model.as_str(), c.slots))
        .collect();
    assert_eq!(
        by,
        [("MK4", 1), ("MK4 MMU3", 5)],
        "SLA printers are left out"
    );
    assert_eq!(s.candidates[0].height, Some(220.0));
    assert_eq!(s.candidates[0].bed, [250.0, 210.0]);
}

#[test]
fn cura_definitions_follow_inherits() {
    let root = scratch("cura");
    put(
        &root,
        "fdmprinter.def.json",
        r#"{"name":"base","settings":{"machine_settings":{"children":{
        "machine_width":{"default_value":100},"machine_depth":{"default_value":100},
        "machine_height":{"default_value":100},"machine_extruder_count":{"default_value":1}}}}}"#,
    );
    put(
        &root,
        "sovol_base.def.json",
        r#"{"name":"Sovol base","inherits":"fdmprinter","metadata":{"visible":false},
        "overrides":{"machine_width":{"default_value":220}}}"#,
    );
    put(
        &root,
        "sovol_sv06.def.json",
        r#"{"name":"Sovol SV06","inherits":"sovol_base","metadata":{"visible":true,"manufacturer":"Sovol"},
        "overrides":{"machine_depth":{"default_value":220},"machine_height":{"default_value":250}}}"#,
    );
    let i = Install {
        slicer: Slicer::Cura,
        version: "5.4.0".into(),
        roots: vec![root],
    };
    let s = scan(&i);
    assert_eq!(s.candidates.len(), 1, "{:?}", s.problems);
    let c = &s.candidates[0];
    assert_eq!((c.brand.as_str(), c.model.as_str()), ("Sovol", "SV06"));
    assert_eq!((c.bed, c.height), ([220.0, 220.0], Some(250.0)));
}

#[test]
fn written_profiles_read_back_through_the_decoder_with_an_ams_beside_a_bambu() {
    let root = scratch("write-src");
    bambu_fixture(&root);
    let s = scan(&install(&root));
    let home = scratch("write-home");
    let chosen: Vec<&Candidate> = s
        .candidates
        .iter()
        .filter(|c| c.model == "X1 Carbon")
        .collect();
    let done = write(&chosen, &s.install, &home, &|n| n == "taken/x", false).unwrap();
    let names: Vec<&str> = done.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["bambu-lab/x1-carbon", "bambu-lab/x1-carbon-ams"]);
    let m = dt_eval::printer_machine("bambu-lab/x1-carbon", Some(&home)).unwrap();
    assert_eq!(m.bed, Some([256.0, 256.0]));
    assert_eq!(m.height, Some(250.0));
    assert_eq!(m.excluded, vec![[0.0, 0.0, 18.0, 28.0]]);
    assert_eq!(m.slots, Some(1));
    assert_eq!(
        m.from_slicer.as_deref(),
        Some("Bambu Studio 2.9.1: Bambu Lab X1 Carbon 0.4 nozzle")
    );
    let ams = dt_eval::printer_machine("bambu-lab/x1-carbon-ams", Some(&home)).unwrap();
    assert_eq!((ams.slots, ams.bed), (Some(4), Some([256.0, 256.0])));
    // a second import leaves yours alone, and says so
    let again = write(&chosen, &s.install, &home, &|_| false, false).unwrap();
    assert!(again.iter().all(|w| w.outcome == Outcome::Kept));
    // a name the library uses gets a name of its own, never replacing the library's
    let taken = write(
        &chosen,
        &s.install,
        &home,
        &|n| n == "bambu-lab/x1-carbon",
        false,
    )
    .unwrap();
    assert_eq!(taken[0].name, "bambu-lab/x1-carbon-imported");
}
