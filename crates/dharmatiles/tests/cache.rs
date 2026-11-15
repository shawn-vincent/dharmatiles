//! The cache of builds from outside: `build` reuses a kept build and writes
//! the same STL, `--no-cache` builds again, a damaged entry is told and built
//! again, and `dharmatiles cache` shows, lists and clears.  Every run has a
//! cache folder of its own: nothing here touches the user's.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

// the program's own types for what `--json` prints, taken in whole: the tests read the output by deserializing them, never by
// key
#[allow(dead_code)]
#[path = "../src/cli_wire.rs"]
mod cli_wire;

use cli_wire::{BuildReport, BuildTile, CacheShown, CacheStatus};
use dt_eval::CacheState;

const EXE: &str = env!("CARGO_BIN_EXE_dharmatiles");

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dt-cache-cli-{}-{name}", std::process::id()));
    if d.exists() {
        std::fs::remove_dir_all(&d).unwrap();
    }
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The program with the cache in `cache` and a home that is not the user's.
fn exe(cache: &std::path::Path) -> Command {
    let mut c = Command::new(EXE);
    c.env("DT_CACHE", cache)
        .env("DT_HOME", cache.with_extension("home"));
    c
}

/// The one tile of a build's report, which built.
fn printed(v: &BuildReport) -> &cli_wire::Printed {
    let BuildTile::Printed(p) = &v.tiles[0] else {
        panic!("the tile did not build: {v:?}");
    };
    p
}

fn state(v: &BuildReport) -> CacheState {
    printed(v).record.cache.state
}

/// What `cache --json` says: the cache is on, and where it is.
fn shown(o: &std::process::Output) -> CacheShown {
    match serde_json::from_slice(&o.stdout).unwrap() {
        CacheStatus::Shown(s) => s,
        CacheStatus::Off(_) => panic!("the cache is off"),
    }
}

#[test]
fn a_second_build_comes_from_the_cache_and_writes_the_same_stl() {
    let d = scratch("reuse");
    let cache = d.join("cache");
    let out = d.join("out");
    let run = |extra: &[&str]| {
        let o = exe(&cache)
            .args([
                "build",
                "lib:tests/ground/1x1-flat.dtile",
                "--json",
                "--out",
            ])
            .arg(&out)
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let v: BuildReport = serde_json::from_slice(&o.stdout).unwrap();
        (v, o)
    };
    let stl = out.join("db/tests/ground/1x1-flat-db.stl");
    assert_eq!(state(&run(&[]).0), CacheState::Miss);
    let first = std::fs::read(&stl).unwrap();
    std::fs::remove_file(&stl).unwrap();
    assert_eq!(state(&run(&[]).0), CacheState::Hit);
    assert_eq!(
        std::fs::read(&stl).unwrap(),
        first,
        "a cached build writes the same STL"
    );
    assert_eq!(state(&run(&["--no-cache"]).0), CacheState::Off);
    assert_eq!(std::fs::read(&stl).unwrap(), first);

    // `dharmatiles cache` says where it is and what it holds
    let show = exe(&cache).args(["cache", "--json"]).output().unwrap();
    let v = shown(&show);
    assert_eq!(v.entries, 1);
    assert_eq!(v.folder, cache.display().to_string());
    let list = exe(&cache).args(["cache", "list"]).output().unwrap();
    assert!(String::from_utf8_lossy(&list.stdout).contains("lib:tests/ground/1x1-flat.dtile"));

    // a damaged entry is told, the build is real, and the cache is whole again
    let slot = std::fs::read_dir(cache.join("v2"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let entry = std::fs::read_dir(slot)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut bytes = std::fs::read(&entry).unwrap();
    let at = bytes.len() / 2;
    bytes[at] ^= 0xff;
    std::fs::write(&entry, bytes).unwrap();
    let (v, o) = run(&[]);
    assert_eq!(state(&v), CacheState::Miss);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("damaged"), "{err}");
    assert_eq!(printed(&v).record.cache.problems.len(), 1);
    assert_eq!(std::fs::read(&stl).unwrap(), first);
    assert_eq!(state(&run(&[]).0), CacheState::Hit);

    // clear empties it
    let clear = exe(&cache).args(["cache", "clear"]).output().unwrap();
    assert_eq!(clear.status.code(), Some(0));
    let show = exe(&cache).args(["cache", "--json"]).output().unwrap();
    assert_eq!(shown(&show).entries, 0);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn a_cache_limit_that_is_not_a_size_stops_the_build_loudly() {
    let d = scratch("limit");
    let o = exe(&d.join("cache"))
        .env("DT_CACHE_LIMIT", "lots")
        .args(["build", "lib:tests/ground/1x1-flat.dtile", "--out"])
        .arg(d.join("out"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains("DT_CACHE_LIMIT") && err.contains("--no-cache"),
        "{err}"
    );
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn the_cache_can_be_switched_off() {
    let d = scratch("off");
    let o = exe(&d.join("cache"))
        .env("DT_CACHE", "off")
        .args([
            "build",
            "lib:tests/ground/1x1-flat.dtile",
            "--json",
            "--out",
        ])
        .arg(d.join("out"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let v: BuildReport = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(state(&v), CacheState::Off);
    assert!(!d.join("cache").exists(), "nothing was written");
    std::fs::remove_dir_all(&d).unwrap();
}
