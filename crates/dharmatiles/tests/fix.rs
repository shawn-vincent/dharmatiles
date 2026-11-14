//! `dharmatiles fix` from outside: it shows without `--write`, writes with it, repeats until no fix applies, shows a
//! moved point in millimetres on the target's square, and fails while a mistake with no fix remains.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::{Command, Output};

const EXE: &str = env!("CARGO_BIN_EXE_dharmatiles");

fn run(args: &[&str]) -> Output {
    Command::new(EXE)
        .env(
            "DT_CACHE",
            std::env::temp_dir().join(format!("dt-fix-cache-{}", std::process::id())),
        )
        .args(args)
        .output()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dt-fix-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const NEAR: &str = "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    line e { through \"(0.02sq, 0.5sq)\" \"(0.5sq, 0.5sq)\" }\n    stack { dirt }\n}\n";

#[test]
fn a_point_a_hair_off_a_side_is_put_on_it() {
    let d = scratch("near");
    let f = d.join("near.dtile");
    std::fs::write(&f, NEAR).unwrap();
    let o = run(&["fix", f.to_str().unwrap(), "--write"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(
        std::fs::read_to_string(&f)
            .unwrap()
            .contains("\"(0sq, 0.5sq)\"")
    );
}

const SHORE: &str = "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" seed=3 {\n    line shore wobble=\"5dmm\" { through \"(0sq, 0.62sq)\" \"(1sq, 0.58sq)\" }\n    stack { dirt }\n}\n";

#[test]
fn without_write_it_shows_the_change_and_touches_nothing() {
    let d = scratch("show");
    let f = d.join("shore.dtile");
    std::fs::write(&f, SHORE).unwrap();
    let o = run(&["fix", f.to_str().unwrap()]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(
        o.status.success(),
        "{out}{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(out.contains(dt_lang::codes::SEAM), "{out}");
    assert!(out.contains("- \"(0sq, 0.62sq)\""), "{out}");
    assert!(out.contains("+ \"wsw\""), "{out}");
    // 0.62 of a 35 mm square to the nearest seam (2/3): 1.6 mm; shown in mm, never dmm
    assert!(out.contains("moves the point 1.6 mm"), "{out}");
    assert!(out.contains("nothing written"), "{out}");
    assert_eq!(std::fs::read_to_string(&f).unwrap(), SHORE);
}

#[test]
fn with_write_the_file_builds_and_a_second_run_finds_nothing() {
    let d = scratch("write");
    let f = d.join("shore.dtile");
    std::fs::write(&f, SHORE).unwrap();
    let o = run(&["fix", f.to_str().unwrap(), "--write"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = std::fs::read_to_string(&f).unwrap();
    assert!(
        text.contains("\"wsw\"") && !text.contains("0.62sq"),
        "{text}"
    );
    assert!(run(&["check", f.to_str().unwrap()]).status.success());
    let again = run(&["fix", f.to_str().unwrap(), "--write"]);
    assert!(again.status.success());
    assert!(String::from_utf8_lossy(&again.stdout).contains("no fix applies"));
}

#[test]
fn a_folder_is_walked_and_a_mistake_with_no_fix_fails() {
    let d = scratch("folder");
    std::fs::create_dir_all(d.join("sub")).unwrap();
    std::fs::write(d.join("a.dtile"), SHORE).unwrap();
    std::fs::write(
        d.join("sub/b.dtile"),
        "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {\n    stack { dirt top=\"3dmm\" unheard=1 }\n}\n",
    )
    .unwrap();
    let o = run(&["fix", d.to_str().unwrap(), "--write"]);
    let out = String::from_utf8_lossy(&o.stdout);
    // a.dtile is fixed and written; b.dtile has a mistake no fix mends, which is said and fails the run
    assert!(out.contains("a.dtile") && out.contains("wrote"), "{out}");
    assert!(!o.status.success(), "a mistake with no fix remains");
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("unheard"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        std::fs::read_to_string(d.join("a.dtile"))
            .unwrap()
            .contains("\"wsw\"")
    );
}

#[test]
fn the_millimetres_follow_the_build_target() {
    let d = scratch("ol");
    let f = d.join("shore.dtile");
    std::fs::write(&f, SHORE).unwrap();
    let o = run(&["fix", f.to_str().unwrap(), "--system", "ol"]);
    let out = String::from_utf8_lossy(&o.stdout);
    // the same share of a 25.4 mm square: 0.0467 x 25.4 = 1.2 mm
    assert!(out.contains("moves the point 1.2 mm"), "{out}");
}

/// Write `through` points into a tile, fix it, and give back the file's text.
fn fixed_line(name: &str, points: &str) -> String {
    let d = scratch(name);
    let f = d.join("t.dtile");
    std::fs::write(
        &f,
        format!(
            "dharmatiles version=\"1.0.0\"\ntile size=\"1x1\" {{\n    line a {{ through {points} }}\n    stack {{ dirt }}\n}}\n"
        ),
    )
    .unwrap();
    let o = run(&["fix", f.to_str().unwrap(), "--write"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    std::fs::read_to_string(&f).unwrap()
}

#[test]
fn a_range_on_a_side_becomes_its_middle_and_then_the_seam() {
    let text = fixed_line("range", "\"(0sq, 0.4sq .. 0.6sq)\" \"(1sq, 0.5sq)\"");
    assert!(text.contains("through \"(0sq, 0.5sq)\""), "{text}");
}

#[test]
fn a_number_on_a_corner_word_is_dropped() {
    let text = fixed_line("corner", "\"nw(2)\" \"e\"");
    assert!(text.contains("through \"nw\" \"e\""), "{text}");
}
