//! `dharmatiles plates` from outside: the plan and its refusals, then a small
//! list laid on two printers' beds and written as files.  Builds only test
//! tiles (`lib:tests/…`).

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

const EXE: &str = env!("CARGO_BIN_EXE_dharmatiles");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dt-plates-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The program with a cache and a home of this run's own.
fn exe(d: &Path) -> Command {
    let mut c = Command::new(EXE);
    c.env("DT_CACHE", d.join("cache"))
        .env("DT_HOME", d.join("home"))
        .env("DT_SETTINGS", d.join("settings"));
    c
}

fn list(d: &Path, body: &str) -> PathBuf {
    let p = d.join("kit.dmap");
    std::fs::write(&p, format!("dharmatiles version=\"1.0.0\"\n{body}\n")).unwrap();
    p
}

fn run(d: &Path, args: &[&str]) -> (i32, String, String) {
    let o = exe(d).arg("plates").args(args).output().unwrap();
    (
        o.status.code().unwrap(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn the_plan_says_the_printer_and_checks_the_filaments_before_any_build() {
    let d = scratch("check");
    let l = list(
        &d,
        "map {\n piece \"lib:tests/plants/1x1-soil+grass\" count=2\n piece \"lib:tests/ground/1x1-flat\"\n}\nplates printer=\"sovol/sv06\" mode=\"multicolour\"",
    );
    // a one-filament printer in multicolour: refused with the count, before a build
    let (code, _, err) = run(&d, &[l.to_str().unwrap(), "--check"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("needs 2 filaments"), "{err}");
    assert!(err.contains("this printer has 1"), "{err}");
    assert!(err.contains("kit.dmap"), "{err}");
    // single colour passes, and the AMS printer
    let (code, _, err) = run(&d, &[l.to_str().unwrap(), "--check", "--mode", "single"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains("Sovol SV06") && err.contains("220 x 220"),
        "{err}"
    );
    let (code, out, err) = run(
        &d,
        &[
            l.to_str().unwrap(),
            "--check",
            "--printer",
            "bambu/x1c-ams",
            "--json",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["plan"]["printer"], "bambu/x1c-ams");
    assert_eq!(v["plan"]["slots"], 4);
    assert_eq!(v["plan"]["mode"], "multicolour", "the AMS default");
    assert_eq!(v["plan"]["kinds"].as_array().unwrap().len(), 2);
    assert_eq!(v["plan"]["kinds"][0]["count"], 2);
}

#[test]
fn mistakes_in_the_list_are_shown_at_their_text() {
    let d = scratch("mistakes");
    let l = list(&d, "map {\n piece \"lib:tests/ground/1x1-nothing\"\n}");
    let (code, _, err) = run(&d, &[l.to_str().unwrap(), "--check"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("1x1-nothing") && err.contains("kit.dmap"),
        "{err}"
    );
    let l = list(
        &d,
        "map {\n piece \"lib:tests/ground/1x1-flat\"\n}\nplates printer=\"bambu/x1d\"",
    );
    let (code, _, err) = run(&d, &[l.to_str().unwrap(), "--check"]);
    assert_eq!(code, 1);
    assert!(
        err.contains("no printer profile `bambu/x1d`") && err.contains("bambu/x1c"),
        "{err}"
    );
    // a printer that is only floors has no bed
    let l = list(
        &d,
        "map {\n piece \"lib:tests/ground/1x1-flat\"\n}\nplates printer=\"fine\"",
    );
    let (code, _, err) = run(&d, &[l.to_str().unwrap(), "--check"]);
    assert_eq!(code, 1);
    assert!(err.contains("has no bed"), "{err}");
    // everything skipped
    let l = list(
        &d,
        "map {\n piece \"lib:tests/ground/1x1-flat\" print=skip\n}",
    );
    let (code, _, err) = run(&d, &[l.to_str().unwrap(), "--check"]);
    assert_eq!(code, 1);
    assert!(err.contains("nothing to print"), "{err}");
}

/// The triangles an STL file holds.
fn stl_triangles(p: &Path) -> usize {
    let b = std::fs::read(p).unwrap();
    u32::from_le_bytes([b[80], b[81], b[82], b[83]]) as usize
}

#[test]
fn an_sv06_list_is_one_stl_per_plate_and_the_x1c_fits_more_on_a_plate() {
    let d = scratch("sv06");
    let l = list(
        &d,
        "map {\n piece \"lib:tests/ground/1x1-flat\" count=30\n}\nplates printer=\"sovol/sv06\"",
    );
    let out = d.join("out");
    let (code, _, err) = run(&d, &[l.to_str().unwrap(), "--out", out.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    // the SV06's bed holds 5 x 5 squares: 25, then 5
    assert!(
        err.contains("plate 1: 25 piece(s)") && err.contains("plate 2: 5 piece(s)"),
        "{err}"
    );
    let dir = out.join("plates/kit");
    let one = stl_triangles(&dir.join("kit-plate-01.stl"));
    let two = stl_triangles(&dir.join("kit-plate-02.stl"));
    assert_eq!(one % 25, 0, "25 copies of one tile");
    assert_eq!(two * 5, one, "5 copies against 25: {one} {two}");
    // the same list on the X1 Carbon: 6 x 6 = 36, so one plate
    let (code, _, err) = run(
        &d,
        &[
            l.to_str().unwrap(),
            "--printer",
            "bambu/x1c",
            "--format",
            "stl",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains("plate 1: 30 piece(s)") && !err.contains("plate 2"),
        "{err}"
    );
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("kit.plates.json")).unwrap()).unwrap();
    assert_eq!(record["plates"].as_array().unwrap().len(), 1);
    assert_eq!(record["format"], "stl");
}

#[test]
fn a_bambu_printer_with_no_template_stops_before_building_and_says_how_to_get_one() {
    let d = scratch("notemplate");
    let l = list(
        &d,
        "map {\n piece \"lib:tests/ground/1x1-flat\"\n}\nplates printer=\"bambu/x1c\"",
    );
    let (code, _, err) = run(
        &d,
        &[
            l.to_str().unwrap(),
            "--out",
            d.join("out").to_str().unwrap(),
        ],
    );
    assert_eq!(code, 1, "{err}");
    assert!(
        err.contains("needs a template") && err.contains("--format stl"),
        "{err}"
    );
    assert!(!err.contains("building"), "refused before any build: {err}");
    // and a plain 3MF per plate needs none
    let (code, _, err) = run(
        &d,
        &[
            l.to_str().unwrap(),
            "--format",
            "3mf",
            "--out",
            d.join("out").to_str().unwrap(),
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(d.join("out/plates/kit/kit-plate-01.3mf").is_file());
}

#[test]
fn a_low_tier_build_never_becomes_a_plate() {
    // `build --tier low` refuses to export; plates build only the full tier
    let d = scratch("tier");
    let o = exe(&d)
        .args([
            "build",
            "lib:tests/ground/1x1-flat.dtile",
            "--tier",
            "low",
            "--3mf",
        ])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("never exported"));
}
