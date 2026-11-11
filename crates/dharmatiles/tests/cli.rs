//! The command line from outside: exit codes, the JSON report's shape,
//! the engine version.  Cases from the Phase 1 exit review.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::{Command, Stdio};

const EXE: &str = env!("CARGO_BIN_EXE_dharmatiles");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dt-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_closed_pipe_ends_quietly_with_0() {
    // was: `check … | head -1` panicked writing to the closed pipe (101)
    let d = scratch("pipe");
    let doc = d.join("t.dtile");
    // ~1 MB of errors: far more than a pipe buffers, so the write fails
    let bad = "tile { stack {\n".to_owned() + &"  gruond\n".repeat(4000) + "} }\n";
    std::fs::write(&doc, bad).unwrap();
    let mut child = Command::new(EXE)
        .arg("check")
        .arg(&doc)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stderr.take());
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0), "{status}");
}

#[test]
fn a_write_error_in_json_has_every_error_key() {
    let d = scratch("write");
    let blocked = d.join("out");
    std::fs::write(&blocked, "a file where the out folder should be").unwrap();
    let o = Command::new(EXE)
        .args([
            "build",
            "lib:tests/ground/1x1-flat.dtile",
            "--json",
            "--out",
        ])
        .arg(&blocked)
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let e = &v["tiles"][0]["errors"][0];
    assert_eq!(e["code"], "dt::write");
    for k in ["code", "message", "file", "line", "column", "help"] {
        assert!(e.get(k).is_some(), "no `{k}` in {e}");
    }
}

#[test]
fn a_missing_document_is_a_source_error() {
    let o = Command::new(EXE)
        .args(["check", "--json", "no/such/file.dtile"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["tiles"][0]["errors"][0]["code"], "dt::source");
}

#[test]
fn the_version_is_drawn_from_git_and_lists_the_tile_formats() {
    let o = Command::new(EXE).arg("--version").output().unwrap();
    let text = String::from_utf8(o.stdout).unwrap();
    let v = text.trim().strip_prefix("dharmatiles ").unwrap();
    let (v, formats) = v.split_once(" (tile formats ").unwrap();
    assert_eq!(formats, "0.x)", "{text}");
    let rest = v.strip_prefix(env!("CARGO_PKG_VERSION")).unwrap();
    // plain at the tag, else `+[branch.]N.gHASH[.dirty]` (or `+dirty`)
    if let Some(meta) = rest.strip_prefix('+') {
        let ok =
            |i: &str| !i.is_empty() && i.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-');
        assert!(meta.split('.').all(ok), "{v}");
        assert!(
            meta == "dirty" || meta.split('.').any(|i| i.starts_with('g') && i.len() == 8),
            "{v}"
        );
    } else {
        assert!(rest.is_empty(), "{v}");
    }
}

#[test]
fn elements_lists_every_element() {
    let o = Command::new(EXE).arg("elements").output().unwrap();
    assert_eq!(o.status.code(), Some(0));
    let text = String::from_utf8(o.stdout).unwrap();
    for name in ["dirt", "grass", "tree", "wall", "cliff", "scatter"] {
        assert!(
            text.lines().any(|l| l.starts_with(name)),
            "no `{name}` in\n{text}"
        );
    }
    assert!(
        text.lines()
            .any(|l| l.starts_with("dirt") && l.contains("built")),
        "{text}"
    );
}

#[test]
fn elements_json_lists_every_function_with_its_numbers() {
    let o = Command::new(EXE)
        .args(["elements", "rock", "--json"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let names: Vec<&str> = v["weightings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "flat", "low", "high", "bell", "peak", "ends", "steps", "list", "mix"
        ]
    );
    let bell = &v["weightings"][3];
    assert_eq!(bell["call"], "bell(range, spread = 0.2, at = 0.5)");
    assert_eq!(bell["args"][0]["name"], "spread");
    assert_eq!(bell["args"][0]["default"], 0.2);
    assert_eq!(bell["args"][0]["least"], 0.05);
    assert_eq!(bell["args"][0]["most"], 1.0);
    // no upper limit is null; over 0 says so
    let low = &v["weightings"][1];
    assert_eq!(low["args"][0]["most"], serde_json::Value::Null);
    assert_eq!(low["args"][0]["above"], true);
    // the numbers a function takes any number of
    let steps = &v["weightings"][6];
    assert_eq!(steps["rest"]["atLeast"], 2);
    assert_eq!(steps["rest"]["atMost"], 32);
    assert_eq!(v["weightings"][7]["rest"]["atMost"], 64);
    assert_eq!(v["weightings"][8]["rest"]["atMost"], 16);
}

#[test]
fn elements_rock_lists_the_weightings() {
    let o = Command::new(EXE)
        .args(["elements", "rock"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let text = String::from_utf8(o.stdout).unwrap();
    let size = text
        .lines()
        .find(|l| l.trim_start().starts_with("size "))
        .unwrap();
    assert!(size.contains("or a weighted range"), "{size}");
    for f in [
        "flat(range",
        "low(range",
        "high(range",
        "bell(range",
        "peak(range",
        "ends(range",
        "steps(range",
        "[value:weight",
        "mix(part",
    ] {
        assert!(text.contains(f), "{f}: {text}");
    }
    assert!(!text.contains("skews"), "{text}");
    // an element with no weighted control lists none
    let o = Command::new(EXE)
        .args(["elements", "water"])
        .output()
        .unwrap();
    assert!(!String::from_utf8(o.stdout).unwrap().contains("low(range"));
}

#[test]
fn elements_dirt_lists_its_controls() {
    let o = Command::new(EXE)
        .args(["elements", "dirt"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(text.starts_with("dirt: "), "{text}");
    let size = text
        .lines()
        .find(|l| l.trim_start().starts_with("bumpSize "))
        .unwrap();
    assert!(size.contains("mm, 5 to 70"), "{size}");
    // the old forms are gone
    assert!(!text.lines().any(|l| l.trim_start().starts_with("level ")));
    assert!(!text.lines().any(|l| l.trim_start().starts_with("bumps ")));
}

#[test]
fn an_unknown_look_is_an_error_with_a_hint() {
    let o = Command::new(EXE)
        .args(["elements", "dirtt"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8(o.stderr).unwrap();
    assert!(err.contains("did you mean `dirt`?"), "{err}");

    let o = Command::new(EXE)
        .args(["elements", "nope", "--json"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["errors"][0]["code"], "dt::unknown-element");
}

#[test]
fn elements_dirt_json_has_units_limits_defaults_and_subs() {
    let o = Command::new(EXE)
        .args(["elements", "dirt", "--json"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["name"], "dirt");
    assert_eq!(v["role"], "surface");
    assert!(v["built"].is_boolean());
    let controls = v["controls"].as_array().unwrap();
    let find = |n: &str| controls.iter().find(|c| c["name"] == n).unwrap();
    let size = find("bumpSize");
    assert_eq!(size["unit"], "mm");
    assert_eq!(size["limits"], serde_json::json!([5.0, 70.0]));
    assert!(
        controls
            .iter()
            .all(|c| c["name"] != "level" && c["name"] != "bumps")
    );
    let clods = find("clods");
    assert_eq!(clods["unit"], "word");
    assert_eq!(clods["default"], "some");
    let some = clods["words"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["word"] == "some")
        .unwrap();
    assert!(some["sets"]["density"].is_number());
    // a group's subs: the cliff's shattered patches
    let o = Command::new(EXE)
        .args(["elements", "cliff", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let cliff = v["controls"].as_array().unwrap();
    let shattered: Vec<_> = cliff.iter().find(|c| c["name"] == "shattered").unwrap()["subs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(shattered, ["patches", "width"]);
    assert!(cliff.iter().find(|c| c["name"] == "shattered").unwrap()["unit"].is_null());
    for c in controls {
        assert!(c["taken"].is_boolean(), "{c}");
    }
}

#[test]
fn watch_rebuilds_when_a_used_file_changes() {
    use std::io::BufRead;
    let d = scratch("watch");
    let parts = d.join("p.dtile");
    let tile = d.join("t.dtile");
    std::fs::write(&parts, "define raised { stack { rock size=6 } }\n").unwrap();
    std::fs::write(
        &tile,
        "use \"./p\" raised\ntile size=\"1x1\" seed=1 {\n    stack {\n        dirt\n        raised\n    }\n}\n",
    )
    .unwrap();
    let mut child = Command::new(EXE)
        .arg("watch")
        .arg(&tile)
        .arg("--out")
        .arg(d.join("out"))
        .args(["--max-builds", "2"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let stderr = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr).lines() {
            if tx.send(line.unwrap_or_default()).is_err() {
                break;
            }
        }
    });
    let timeout = std::time::Duration::from_secs(120);
    let mut seen = Vec::new();
    let mut wait_for = |what: &str, seen: &mut Vec<String>| loop {
        match rx.recv_timeout(timeout) {
            Ok(l) => {
                let hit = l.contains(what);
                seen.push(l);
                if hit {
                    return;
                }
            }
            Err(e) => {
                let _ = child.kill();
                panic!("no `{what}` ({e}); stderr:\n{}", seen.join("\n"));
            }
        }
    };
    // the tile, its part, and the library files every tile reads (the
    // standard levels and the edge contract, on disk)
    wait_for("watching ", &mut seen);
    // a coarse file clock (1 s) still sees the edit
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(&parts, "define raised { stack { rock size=7 } }\n").unwrap();
    wait_for("changed:", &mut seen);
    wait_for("built ", &mut seen);
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0), "{}", seen.join("\n"));
    assert_eq!(
        seen.iter().filter(|l| l.starts_with("built ")).count(),
        2,
        "{}",
        seen.join("\n")
    );
}

#[test]
fn watch_of_a_library_tile_watches_its_file_on_disk() {
    // the library is read from disk (this checkout's library/), so a
    // `lib:` tile is watched like any other file
    use std::io::BufRead;
    let d = scratch("watch-lib");
    let mut child = Command::new(EXE)
        .args(["watch", "lib:tests/ground/1x1-flat.dtile", "--out"])
        .arg(&d)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stderr).lines() {
            if tx.send(line.unwrap_or_default()).is_err() {
                break;
            }
        }
    });
    let mut seen = Vec::new();
    let watching = loop {
        match rx.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(l) => {
                let hit = l.starts_with("watching ");
                seen.push(l);
                if hit {
                    break true;
                }
            }
            Err(_) => break false,
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    assert!(watching, "never started watching:\n{}", seen.join("\n"));
    assert!(d.join("db/tests/ground/1x1-flat-db.stl").exists());
}

#[test]
fn a_build_is_for_one_target_named_in_its_paths_and_record() {
    let d = scratch("target");
    // your own profile, beside your tiles, found by name
    let home = d.join("home");
    std::fs::create_dir_all(home.join("printers")).unwrap();
    std::fs::write(
        home.join("printers/mine.dtile"),
        "printer mine {\n line 0.5\n layer 0.2\n overhang 55\n steepest 65\n gap 0.2\n}\n",
    )
    .unwrap();
    let build = |args: &[&str]| {
        Command::new(EXE)
            .args([
                "build",
                "lib:tests/ground/1x1-flat.dtile",
                "--json",
                "--out",
            ])
            .arg(d.join("out"))
            .args(args)
            .env("DT_HOME", &home)
            .output()
            .unwrap()
    };
    // the default printer: the paths are the system's alone
    let o = build(&[]);
    assert_eq!(o.status.code(), Some(0));
    assert!(d.join("out/db/tests/ground/1x1-flat-db.stl").is_file());
    // another printer, the library's and the user's own, and the other system
    for (args, folder, file, target) in [
        (
            ["--printer", "fine"],
            "db-fine",
            "1x1-flat-db-fine.stl",
            "db-fine",
        ),
        (
            ["--printer", "mine"],
            "db-mine",
            "1x1-flat-db-mine.stl",
            "db-mine",
        ),
        (["--system", "ol"], "ol", "1x1-flat-ol.stl", "ol"),
    ] {
        let o = build(&args);
        assert_eq!(o.status.code(), Some(0), "{args:?}");
        assert!(
            d.join("out")
                .join(folder)
                .join("tests/ground")
                .join(file)
                .is_file(),
            "{args:?}"
        );
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["tiles"][0]["record"]["target"], target, "{v}");
    }
    // one build is one target: there is no `both`, and no such printer
    for args in [
        ["--system", "both"],
        ["--printer", "nope"],
        ["--printer", "../x"],
    ] {
        let o = build(&args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(!o.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn a_sweep_reports_every_failing_tile_by_name_and_exits_1() {
    let d = scratch("sweep");
    let o = Command::new(EXE)
        .args([
            "build",
            "no/such/first.dtile",
            "lib:tests/ground/1x1-flat.dtile",
            "no/such/last.dtile",
            "--out",
        ])
        .arg(&d)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        d.join("db/tests/ground/1x1-flat-db.stl").is_file(),
        "the good tile still builds: {err}"
    );
    assert!(err.contains("2 of 3 tiles failed"), "{err}");
    assert!(
        err.contains("no/such/first.dtile") && err.contains("no/such/last.dtile"),
        "{err}"
    );
}

#[test]
fn build_all_with_no_library_to_sweep_is_an_error_not_a_success() {
    let d = scratch("empty-library");
    for lib in [d.clone(), d.join("not-there")] {
        let o = Command::new(EXE)
            .args(["build", "--all", "--out"])
            .arg(d.join("out"))
            .env("DT_LIBRARY", &lib)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(err.contains("build --all"), "{err}");
    }
}

#[test]
fn a_panic_reaching_main_exits_101_naming_a_bug() {
    let o = Command::new(EXE)
        .args(["elements"])
        .env("DT_TEST_PANIC", "main")
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(101));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("this is a bug in dharmatiles"), "{err}");
    assert!(err.contains("DT_TEST_PANIC=main"), "{err}");
}

#[test]
fn a_panic_in_one_tile_of_a_sweep_exits_101_and_the_rest_build() {
    let d = scratch("sweep-panic");
    let o = Command::new(EXE)
        .args([
            "build",
            "lib:tests/ground/1x1-flat.dtile",
            "lib:tests/floors/1x1-planks.dtile",
            "--out",
        ])
        .arg(&d)
        .env("DT_TEST_PANIC", "1x1-planks")
        .output()
        .unwrap();
    assert_eq!(
        o.status.code(),
        Some(101),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("1x1-planks"), "{err}");
    assert!(err.contains("built lib:tests/ground/1x1-flat"), "{err}");
}

#[test]
fn the_worst_outcome_of_a_sweep_wins() {
    // a refused document (1) with a panicked tile (101): 101
    let d = scratch("sweep-worst");
    let bad = d.join("bad.dtile");
    std::fs::write(&bad, "tile { stack { gruond } }\n").unwrap();
    let run = |panic: Option<&str>| {
        let mut c = Command::new(EXE);
        c.args(["build"])
            .arg(&bad)
            .arg("lib:tests/ground/1x1-flat.dtile")
            .arg("--out")
            .arg(&d);
        if let Some(p) = panic {
            c.env("DT_TEST_PANIC", p);
        }
        c.output().unwrap().status.code()
    };
    assert_eq!(run(None), Some(1));
    assert_eq!(run(Some("1x1-flat")), Some(101));
}
