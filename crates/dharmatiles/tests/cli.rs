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
    let doc = d.join("t.kdl");
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
        .args(["build", "lib:tiles/test/flat-1x1.kdl", "--json", "--out"])
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
        .args(["check", "--json", "no/such/file.kdl"])
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
