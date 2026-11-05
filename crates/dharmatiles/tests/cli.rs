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
    let bad = "tile t { stack {\n".to_owned() + &"  gruond\n".repeat(4000) + "} }\n";
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
        .args(["build", "lib:tiles/test/1x1-flat.dtile", "--json", "--out"])
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
fn the_engine_version_is_the_workspace_version_and_commit() {
    let o = Command::new(EXE).arg("--version").output().unwrap();
    let text = String::from_utf8(o.stdout).unwrap();
    let v = text.trim().strip_prefix("dharmatiles ").unwrap();
    let rest = v.strip_prefix(env!("CARGO_PKG_VERSION")).unwrap();
    assert!(
        rest.is_empty() || (rest.starts_with("+g") && rest.len() >= 9),
        "{v}"
    );
}

#[test]
fn elements_lists_every_element() {
    let o = Command::new(EXE).arg("elements").output().unwrap();
    assert_eq!(o.status.code(), Some(0));
    let text = String::from_utf8(o.stdout).unwrap();
    for name in [
        "ground", "dirt", "grass", "tree", "wall", "cliff", "scatter",
    ] {
        assert!(
            text.lines().any(|l| l.starts_with(name)),
            "no `{name}` in\n{text}"
        );
    }
    assert!(
        text.lines()
            .any(|l| l.starts_with("ground") && l.contains("built")),
        "{text}"
    );
}

#[test]
fn elements_ground_lists_its_controls() {
    let o = Command::new(EXE)
        .args(["elements", "ground"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let text = String::from_utf8(o.stdout).unwrap();
    assert!(text.starts_with("ground: "), "{text}");
    let level = text
        .lines()
        .find(|l| l.trim_start().starts_with("level "))
        .unwrap();
    assert!(level.contains("ground"), "{level}");
    let bumps = text
        .lines()
        .find(|l| l.trim_start().starts_with("bumps "))
        .unwrap();
    assert!(bumps.contains("mm 0..15"), "{bumps}");
    assert!(text.contains("up="), "{text}");
}

#[test]
fn an_unknown_look_is_an_error_with_a_hint() {
    let o = Command::new(EXE)
        .args(["elements", "grond"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8(o.stderr).unwrap();
    assert!(err.contains("did you mean `ground`?"), "{err}");

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
    let bumps = find("bumps");
    assert_eq!(bumps["unit"], "mm");
    assert_eq!(bumps["limits"], serde_json::json!([0.0, 15.0]));
    assert_eq!(bumps["default"], "0");
    let subs: Vec<_> = bumps["subs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(subs, ["up", "down"]);
    let clods = find("clods");
    assert_eq!(clods["unit"], "word");
    assert_eq!(clods["default"], "some");
    assert_eq!(clods["words"][1]["word"], "some");
    assert!(clods["words"][1]["sets"]["density"].is_number());
    assert!(find("bank")["unit"].is_null());
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
    std::fs::write(&parts, "define raised { stack { ground level=6 } }\n").unwrap();
    std::fs::write(
        &tile,
        "use \"./p\" raised\ntile t size=\"1x1\" seed=1 {\n    stack {\n        raised\n    }\n}\n",
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
    std::fs::write(&parts, "define raised { stack { ground level=7 } }\n").unwrap();
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
        .args(["watch", "lib:tiles/test/1x1-flat.dtile", "--out"])
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
    assert!(d.join("db/test/1x1-flat-db.stl").exists());
}
