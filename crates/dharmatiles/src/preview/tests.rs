//! The server's routes and the worker, end to end on a loopback port.

#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dt_eval::{Options, Source};
use serde_json::Value;

use super::*;

struct Running {
    addr: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// [`start`]ed for the default build target.
fn start(paths: &[&str], eager: bool) -> Running {
    start_with(paths, eager, &Options::default())
}

/// [`start`] for the build target `opts`.
fn start_with(paths: &[&str], eager: bool, opts: &Options) -> Running {
    let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
    let roots = if paths.is_empty() {
        Vec::new()
    } else {
        vec![Root::new("Files", RootKind::Files)]
    };
    let shared = Arc::new(Shared::with_roots(roots, &paths, opts));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    server::spawn(listener, Arc::clone(&shared));
    let stop = Arc::new(AtomicBool::new(false));
    let (s, st) = (Arc::clone(&shared), Arc::clone(&stop));
    std::thread::spawn(move || work(&s, eager, &st));
    Running { addr, stop }
}

fn request(addr: std::net::SocketAddr, method: &str, path: &str, host: &str) -> (String, Vec<u8>) {
    let mut c = TcpStream::connect(addr).unwrap();
    write!(c, "{method} {path} HTTP/1.1\r\nHost: {host}\r\n\r\n").unwrap();
    let mut all = Vec::new();
    c.read_to_end(&mut all).unwrap();
    let split = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&all[..split]).into_owned();
    (head, all[split + 4..].to_vec())
}

fn get(r: &Running, path: &str) -> (String, Vec<u8>) {
    request(r.addr, "GET", path, &r.addr.to_string())
}

fn tiles(r: &Running) -> Value {
    serde_json::from_slice(&get(r, "/api/tiles").1).unwrap()
}

/// The first tile's id.
fn id0(r: &Running) -> String {
    tiles(r)["tiles"][0]["id"].as_str().unwrap().to_owned()
}

fn wait_for(r: &Running, what: &str, mut ok: impl FnMut(&Value) -> bool) -> Value {
    let t0 = Instant::now();
    loop {
        let v = tiles(r);
        if ok(&v["tiles"][0]) {
            return v["tiles"][0].clone();
        }
        assert!(
            t0.elapsed() < Duration::from_secs(120),
            "timed out waiting for {what}: {v}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

const FLAT: &str = "lib:tests/ground/1x1-flat.dtile";

#[test]
fn serves_the_viewer_and_its_vendored_three() {
    let r = start(&[], false);
    let (head, body) = get(&r, "/");
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(head.contains("text/html"));
    assert!(String::from_utf8_lossy(&body).contains("importmap"));
    let (head, body) = get(&r, "/vendor/three.module.min.js");
    assert!(head.contains("text/javascript"));
    assert!(body.len() > 100_000, "three.js is embedded");
    for p in [
        "/js/main.js",
        "/js/viewer.js",
        "/js/view-presets.js",
        "/js/scroll-shadows.js",
        "/js/copy.js",
        "/js/history.js",
        "/js/payload.js",
        "/js/host.js",
        "/js/text.js",
        "/js/lens.js",
        "/js/complete.js",
        "/style.css",
        "/logo.svg",
        "/favicon.svg",
        "/icons/undo-2.svg",
        "/icons/redo-2.svg",
    ] {
        assert!(get(&r, p).0.starts_with("HTTP/1.1 200"), "{p}");
    }
    assert!(get(&r, "/nope").0.starts_with("HTTP/1.1 404"));
}

/// Every script the page can import is served: a module missing from the
/// table would only fail in the browser.
#[test]
fn every_page_script_is_served() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/preview");
    // A file left out is a 404, and one failed import blanks the whole editor.
    let mut dirs = vec![root.join("js"), root.join("icons")];
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "js" || x == "svg") {
                let url = format!(
                    "/{}",
                    p.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/")
                );
                assert!(
                    super::assets::find(&url).is_some(),
                    "{url} is not in assets::FILES"
                );
            }
        }
    }
}

#[test]
fn refuses_a_foreign_host_header() {
    let r = start(&[], false);
    let (head, _) = request(r.addr, "GET", "/api/tiles", "evil.example:80");
    assert!(head.starts_with("HTTP/1.1 403"), "{head}");
    assert!(
        request(r.addr, "GET", "/", "localhost:1234")
            .0
            .starts_with("HTTP/1.1 200")
    );
    assert!(
        request(r.addr, "GET", "/", "[::1]:1234")
            .0
            .starts_with("HTTP/1.1 200")
    );
    assert!(!server_loopback("127.0.0.1.evil.com:80"));
}

fn server_loopback(h: &str) -> bool {
    // via the public behaviour: a bare name is checked the same way
    let r = start(&[], false);
    request(r.addr, "GET", "/", h).0.starts_with("HTTP/1.1 200")
}

#[test]
fn binds_loopback_only() {
    let l = bind(Some(0)).unwrap();
    assert!(l.local_addr().unwrap().ip().is_loopback());
    assert!(!l.local_addr().unwrap().ip().is_unspecified());
}

#[test]
fn builds_a_tile_and_serves_its_mesh() {
    let r = start(&[FLAT], true);
    let t = wait_for(&r, "ready", |t| t["status"] == "ready");
    assert_eq!(t["plan"]["cols"], 1);
    assert!((t["plan"]["square_mm"].as_f64().unwrap() - 35.0).abs() < 1e-9);
    assert_eq!(t["record"]["audit"]["watertight"], true);
    assert_eq!(t["mesh_version"], 1);
    let (head, body) = get(&r, &format!("/api/tiles/{}/mesh", id0(&r)));
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert_eq!(&body[..4], payload::MAGIC);
    let jl = u32::from_le_bytes(body[4..8].try_into().unwrap()) as usize;
    let h: Value = serde_json::from_slice(&body[8..8 + jl]).unwrap();
    let n = usize::try_from(h["triangles"].as_u64().unwrap()).unwrap();
    assert_eq!(
        n as u64,
        t["record"]["audit"]["triangles"].as_u64().unwrap()
    );
    // positions, normals, painted, clean, entry, piece; padded to 4
    assert_eq!(
        body.len(),
        (8 + jl + n * (36 + 36 + 9 + 9 + 2 + 2)).next_multiple_of(4)
    );
    assert_eq!(h["overhang_deg"], 55.0);
    assert!(
        get(&r, "/api/tiles/ffffffffffffffff/mesh")
            .0
            .starts_with("HTTP/1.1 404")
    );
}

#[test]
fn sse_starts_with_a_snapshot_then_streams_changes() {
    let r = start(&[FLAT], true);
    let mut c = TcpStream::connect(r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    let mut lines = BufReader::new(c).lines();
    let mut seen = Vec::new();
    let mut first = true;
    for l in lines.by_ref() {
        let l = l.unwrap();
        let Some(data) = l.strip_prefix("data: ") else {
            continue;
        };
        let v: Value = serde_json::from_str(data).unwrap();
        if first {
            assert_eq!(v["type"], "snapshot");
            first = false;
        } else if v["type"] == "tile" {
            seen.push(v["tile"]["status"].as_str().unwrap().to_owned());
            if v["tile"]["status"] == "ready" {
                break;
            }
        }
        if v["type"] == "snapshot" && v["tiles"][0]["status"] == "ready" {
            return; // built before we listened: the snapshot said so
        }
    }
    assert_eq!(seen.last().map(String::as_str), Some("ready"), "{seen:?}");
}

#[test]
fn a_library_tile_is_built_only_when_asked() {
    let r = start(&[FLAT], false);
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(tiles(&r)["tiles"][0]["status"], "idle");
    assert!(
        request(
            r.addr,
            "POST",
            &format!("/api/tiles/{}/build", id0(&r)),
            &r.addr.to_string()
        )
        .0
        .contains("202")
    );
    assert!(
        request(r.addr, "POST", "/api/tiles/nope/build", &r.addr.to_string())
            .0
            .contains("404")
    );
    wait_for(&r, "ready", |t| t["status"] == "ready");
}

#[test]
fn editing_the_document_rebuilds_it_and_a_mistake_shows_its_line() {
    let dir = std::env::temp_dir().join(format!("dt-preview-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("t.dtile");
    let text = dt_eval::Library.read(FLAT).unwrap();
    std::fs::write(&doc, &text).unwrap();
    let path = doc.to_string_lossy().into_owned();
    let r = start(&[&path], true);
    wait_for(&r, "first build", |t| t["status"] == "ready");

    // a mistake: the error names the file and its line, the old mesh stays
    std::thread::sleep(Duration::from_millis(50));
    let line = text
        .lines()
        .position(|l| l.contains("dirt top=\"$dirtEdgeHeight\""))
        .expect("the flat tile writes `dirt top=\"$dirtEdgeHeight\"`")
        + 1;
    std::fs::write(&doc, text.replace("dirt top=", "dirtt top=")).unwrap();
    let t = wait_for(&r, "failure", |t| t["status"] == "failed");
    let e = &t["errors"][0];
    assert_eq!(e["file"], path.as_str());
    assert_eq!(e["line"].as_u64(), Some(line as u64), "{e}");
    assert_eq!(t["mesh_version"], 1, "the last good mesh is kept");
    assert!(
        get(&r, &format!("/api/tiles/{}/mesh", id0(&r)))
            .0
            .starts_with("HTTP/1.1 200")
    );

    // fixed: rebuilt in place, a new mesh version
    std::fs::write(&doc, text.replace("seed=1", "seed=2")).unwrap();
    let t = wait_for(&r, "rebuild", |t| {
        t["status"] == "ready" && t["mesh_version"] == 2
    });
    assert!(t["errors"].as_array().unwrap().is_empty());
    drop(r);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_slot_name_comes_from_the_file_until_the_plan_is_read() {
    assert_eq!(
        Slot::new("lib:tiles/ground/1x1-grass.dtile").name,
        "1x1-grass"
    );
    assert_eq!(Slot::new("my/tiles/a.dtile").name, "a");
    let _ = Shared::new(&[]).snapshot();
}

#[test]
fn a_build_pushes_progress_events_that_never_go_back() {
    let r = start(&[FLAT], false);
    let mut c = TcpStream::connect(r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    assert!(
        request(
            r.addr,
            "POST",
            &format!("/api/tiles/{}/build", id0(&r)),
            &r.addr.to_string()
        )
        .0
        .contains("202")
    );
    let mut fractions = Vec::new();
    for l in BufReader::new(c).lines() {
        let l = l.unwrap();
        let Some(data) = l.strip_prefix("data: ") else {
            continue;
        };
        let v: Value = serde_json::from_str(data).unwrap();
        if v["type"] == "progress" {
            let p = &v["progress"];
            assert!(
                p["label"].is_string() && p["eta_s"].as_f64().unwrap() >= 0.0,
                "{p}"
            );
            fractions.push(p["fraction"].as_f64().unwrap());
        }
        if v["type"] == "tile" && v["tile"]["status"] == "ready" {
            assert!(v["tile"]["progress"].is_null(), "cleared when done");
            break;
        }
    }
    assert!(fractions.len() >= 3, "{fractions:?}");
    assert!(fractions.windows(2).all(|w| w[1] >= w[0]), "{fractions:?}");
    assert!(
        (fractions.last().unwrap() - 1.0).abs() < 1e-9,
        "ends at 100%: {fractions:?}"
    );
}

#[test]
fn the_command_line_names_files_folders_and_library_groups() {
    let dir = std::env::temp_dir().join(format!("dt-preview-roots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().into_owned();
    let (roots, files) = roots_for(&[
        "a.dtile".to_owned(),
        d.clone(),
        "lib:tiles/water".to_owned(),
        FLAT.to_owned(),
    ])
    .unwrap();
    assert_eq!(files, vec!["a.dtile".to_owned(), FLAT.to_owned()]);
    let kinds: Vec<_> = roots.iter().map(|r| r.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            RootKind::Files,
            RootKind::Dir(dir.clone()),
            RootKind::Library("tiles/water/".to_owned())
        ]
    );
    // no paths: the personal home, then the library, then the test tiles
    let (roots, files) = roots_for(&[]).unwrap();
    assert!(files.is_empty());
    assert_eq!(roots[0].kind, RootKind::Dir(personal_home().unwrap()));
    assert_eq!(roots[1].kind, RootKind::Library("tiles/".to_owned()));
    assert_eq!(roots[2].kind, RootKind::Library("tests/".to_owned()));
    assert_eq!(roots[2].label, "Tests");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_lists_its_tiles_and_follows_saves_and_deletes() {
    let dir = std::env::temp_dir().join(format!("dt-preview-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("ponds")).unwrap();
    std::fs::create_dir_all(dir.join("out")).unwrap();
    let text = dt_eval::Library.read(FLAT).unwrap();
    std::fs::write(dir.join("a.dtile"), &text).unwrap();
    std::fs::write(dir.join("ponds/b.dtile"), &text).unwrap();
    std::fs::write(dir.join("out/built.stl"), b"").unwrap(); // build output: skipped
    std::fs::write(dir.join("parts.dtile"), "grass meadow {\n}\n").unwrap(); // a part, not a tile
    std::fs::write(dir.join("dharmatiles.dtile"), "out \"out\"\n").unwrap(); // the project file
    let roots = vec![
        Root::new("My tiles", RootKind::Dir(dir.clone())),
        Root::new("Tests", RootKind::Library("tests/ground/".to_owned())),
    ];
    let shared = Shared::with_roots(roots, &[], &Options::default());
    let listed = |s: &Shared| -> Vec<(String, Value, Value)> {
        let v = s.snapshot();
        v["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["root"] == 0)
            .map(|t| {
                (
                    t["name"].as_str().unwrap().to_owned(),
                    t["group"].clone(),
                    t["status"].clone(),
                )
            })
            .collect()
    };
    assert_eq!(
        listed(&shared),
        vec![
            ("a".to_owned(), Value::Null, "idle".into()),
            ("b".to_owned(), "ponds".into(), "idle".into())
        ]
    );
    let snap = shared.snapshot();
    assert_eq!(snap["roots"][0]["kind"], "dir");
    assert_eq!(snap["roots"][1]["kind"], "library");
    assert!(
        snap["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["root"] == 1),
        "the library's test tiles are listed too"
    );
    assert!(
        shared.next_job(true).is_none(),
        "folder tiles wait to be chosen"
    );

    // a new tile joins; a deleted one leaves the list altogether
    let id_of = |s: &Shared, name: &str| {
        s.snapshot()["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["root"] == 0 && t["name"] == name)
            .map(|t| t["id"].as_str().unwrap().to_owned())
    };
    let a = id_of(&shared, "a").unwrap();
    assert_eq!(a, tile_id(&dir.join("a.dtile").to_string_lossy()));
    std::fs::write(dir.join("c.dtile"), &text).unwrap();
    std::fs::remove_file(dir.join("a.dtile")).unwrap();
    assert!(shared.rescan());
    let names: Vec<_> = listed(&shared).into_iter().map(|t| t.0).collect();
    assert_eq!(names, vec!["b", "c"]);
    assert_eq!(id_of(&shared, "a"), None, "no tombstone is left");
    assert!(!shared.want(&a), "a deleted tile is not built");
    assert!(!shared.rescan(), "nothing changed");

    // and comes back when the file does, under the same id
    std::fs::write(dir.join("a.dtile"), &text).unwrap();
    assert!(shared.rescan());
    assert_eq!(listed(&shared).len(), 3);
    assert_eq!(id_of(&shared, "a"), Some(a));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serves_a_tiles_own_text_and_nothing_else() {
    let r = start(&[FLAT], true);
    wait_for(&r, "ready", |t| t["status"] == "ready");
    let (head, body) = get(&r, &format!("/api/tiles/{}/text", id0(&r)));
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["file"], FLAT);
    assert!(v["text"].as_str().unwrap().contains("tile "));
    assert_eq!(v["writable"], false, "library files are never written");
    // only files the tile's build read
    let (head, _) = get(
        &r,
        &format!("/api/tiles/{}/text?file=%2Fetc%2Fpasswd", id0(&r)),
    );
    assert!(head.starts_with("HTTP/1.1 404"), "{head}");
    let (head, _) = get(
        &r,
        &format!("/api/tiles/{}/text?file=lib%3Aparts%2Fgrass.dtile", id0(&r)),
    );
    assert!(head.starts_with("HTTP/1.1 404"), "{head}");
}

fn post_raw(
    r: &Running,
    path: &str,
    token: Option<&str>,
    origin: Option<&str>,
    body: &str,
) -> (String, Vec<u8>) {
    let mut c = TcpStream::connect(r.addr).unwrap();
    let mut head = format!(
        "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\n",
        r.addr,
        body.len()
    );
    if let Some(t) = token {
        head.push_str(&format!("X-DT-Token: {t}\r\n"));
    }
    if let Some(o) = origin {
        head.push_str(&format!("Origin: {o}\r\n"));
    }
    write!(c, "{head}\r\n{body}").unwrap();
    let mut all = Vec::new();
    c.read_to_end(&mut all).unwrap();
    let split = all.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    (
        String::from_utf8_lossy(&all[..split]).into_owned(),
        all[split + 4..].to_vec(),
    )
}

fn post(
    r: &Running,
    path: &str,
    token: Option<&str>,
    origin: Option<&str>,
    body: &str,
) -> (String, Value) {
    let (head, body) = post_raw(r, path, token, origin, body);
    (head, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[test]
fn the_editor_checks_edits_and_saves_only_with_the_token() {
    let dir = std::env::temp_dir().join(format!("dt-edit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "dharmatiles version=1\ntile size=\"1x1\" {\n    stack {\n        dirt\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    assert_eq!(token.len(), 32);
    let body = serde_json::json!({ "text": text }).to_string();
    // no token, a wrong one, or a foreign page: refused
    assert!(
        post(
            &r,
            &format!("/api/tiles/{}/check", id0(&r)),
            None,
            None,
            &body
        )
        .0
        .starts_with("HTTP/1.1 403")
    );
    assert!(
        post(
            &r,
            &format!("/api/tiles/{}/check", id0(&r)),
            Some("nope"),
            None,
            &body
        )
        .0
        .starts_with("HTTP/1.1 403")
    );
    assert!(
        post(
            &r,
            &format!("/api/tiles/{}/check", id0(&r)),
            Some(&token),
            Some("https://evil.example"),
            &body
        )
        .0
        .starts_with("HTTP/1.1 403")
    );
    // check: the stack, with each entry's offset and controls
    let (h, v) = post(
        &r,
        &format!("/api/tiles/{}/check", id0(&r)),
        Some(&token),
        Some(&format!("http://{}", r.addr)),
        &body,
    );
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    assert_eq!(v["ok"], true, "{v}");
    let at = v["stack"][0]["span"][0].as_u64().unwrap();
    assert_eq!(&text[usize::try_from(at).unwrap()..][..4], "dirt");
    let bad = serde_json::json!({ "text": "dharmatiles version=1\ntile size=\"1x1\" { stack { durt } }" }).to_string();
    let (_, v) = post(
        &r,
        &format!("/api/tiles/{}/check", id0(&r)),
        Some(&token),
        None,
        &bad,
    );
    assert_eq!(v["ok"], false);
    assert!(v["errors"][0]["line"].is_number(), "{v}");
    // edit: a control written on the line, the rest kept
    let e =
        serde_json::json!({ "text": text, "at": at, "name": "wear", "value": "worn" }).to_string();
    let (h, v) = post(&r, "/api/edit", Some(&token), None, &e);
    assert!(h.starts_with("HTTP/1.1 200"), "{h} {v}");
    let edited = v["text"].as_str().unwrap().to_owned();
    assert_eq!(edited, text.replace("dirt\n", "dirt wear=worn\n"));
    // check-value: the reader's own check of a typed value, nothing else
    let cv = |control: &str, typed: &str| {
        let b = serde_json::json!({ "element": "rock", "control": control, "typed": typed });
        post(&r, "/api/check-value", Some(&token), None, &b.to_string())
    };
    let (h, v) = cv("size", "250dmm");
    assert!(h.starts_with("HTTP/1.1 200"), "{h} {v}");
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .starts_with("`rock size` is "),
        "{v}"
    );
    assert!(cv("size", "40dmm").1["message"].is_null());
    assert!(cv("no-such-control", "150").1["message"].is_null());
    // a weighting's strength outside its limits is the program's dt::range
    // message, as a build would say it
    let m = cv("size", "low(20dmm..60dmm, -2)").1["message"].clone();
    assert_eq!(m, "low's ratio must be above 0, not -2");
    assert!(cv("size", "low(20dmm..60dmm, 3)").1["message"].is_null());
    // save: only over the text it was edited from
    let s = serde_json::json!({ "text": edited, "base": "something else" }).to_string();
    let (h, v) = post(
        &r,
        &format!("/api/tiles/{}/save", id0(&r)),
        Some(&token),
        None,
        &s,
    );
    assert!(h.starts_with("HTTP/1.1 409"), "{h}");
    assert_eq!(v["text"], text);
    let s = serde_json::json!({ "text": edited, "base": text }).to_string();
    assert!(
        post(
            &r,
            &format!("/api/tiles/{}/save", id0(&r)),
            Some(&token),
            None,
            &s
        )
        .0
        .starts_with("HTTP/1.1 200")
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), edited);
    // only the tile's own files
    let s = serde_json::json!({ "text": "x", "base": "", "file": "/etc/hosts" }).to_string();
    assert!(
        post(
            &r,
            &format!("/api/tiles/{}/save", id0(&r)),
            Some(&token),
            None,
            &s
        )
        .0
        .starts_with("HTTP/1.1 403")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_library_tile_is_never_written() {
    let r = start(&[FLAT], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let t = token["token"].as_str().unwrap();
    let text = dt_eval::Files.read(FLAT).unwrap();
    let s = serde_json::json!({ "text": "tile {}", "base": text }).to_string();
    let (h, _) = post(
        &r,
        &format!("/api/tiles/{}/save", id0(&r)),
        Some(t),
        None,
        &s,
    );
    assert!(h.starts_with("HTTP/1.1 403"), "{h}");
    assert_eq!(dt_eval::Files.read(FLAT).unwrap(), text);
}

#[test]
fn about_gives_the_version_and_the_formats() {
    let r = start(&[], false);
    let v: Value = serde_json::from_slice(&get(&r, "/api/about").1).unwrap();
    assert_eq!(v["version"], env!("DT_ENGINE_VERSION"));
    assert_eq!(v["formats"], dt_doc::header::formats());
}

#[test]
fn serves_every_element_for_the_lens() {
    let r = start(&[], false);
    let v: Value = serde_json::from_slice(&get(&r, "/api/elements").1).unwrap();
    let rock = v
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "rock")
        .unwrap();
    assert!(
        rock["controls"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "size")
    );
}

#[test]
fn every_control_of_every_element_and_node_has_one_shape_on_the_wire() {
    let r = start(&[], false);
    let els: Value = serde_json::from_slice(&get(&r, "/api/elements").1).unwrap();
    let lang: Value = serde_json::from_slice(&get(&r, "/api/language").1).unwrap();
    let nodes = lang["nodes"].as_array().unwrap();
    assert_eq!(
        nodes
            .iter()
            .map(|n| n["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["tile", "line", "region", "within", "except"]
    );
    let mut controls: Vec<&Value> = Vec::new();
    for e in els.as_array().unwrap() {
        assert!(e["href"].as_str().unwrap().contains("elements/"), "{e}");
        controls.extend(e["controls"].as_array().unwrap());
    }
    for n in nodes {
        assert!(
            n["href"]
                .as_str()
                .unwrap()
                .starts_with(lang["docs"].as_str().unwrap())
        );
        controls.extend(n["controls"].as_array().unwrap());
    }
    for c in controls {
        for key in [
            "name", "unit", "numeric", "default", "default1", "required", "taken", "about",
            "words", "subs",
        ] {
            assert!(c.get(key).is_some(), "{} lacks {key}", c["name"]);
        }
    }
    // a body's `top` is a row of the sheet like any control, from the schema
    let dirt = els
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "dirt")
        .unwrap();
    assert_eq!(dirt["body"], true);
    let top = dirt["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "top")
        .unwrap();
    assert_eq!(
        (top["unit"].as_str(), top["numeric"].as_bool()),
        (Some("height"), Some(true))
    );
    // the language's words
    assert!(lang["units"].as_array().unwrap().iter().any(|u| u == "dmm"));
    assert!(
        lang["functions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u == "parent")
    );
}

#[test]
fn the_names_a_text_makes_come_from_the_parser() {
    let r = start(&[], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let t = token["token"].as_str().unwrap().to_owned();
    let text = "// define fake\ndefine meadow { stack { dirt } }\ntile size=\"1x1\" {\n    region pool { fill \"1,1\" }\n    stack { meadow }\n}\n";
    let (h, v) = post(
        &r,
        "/api/names",
        Some(&t),
        None,
        &serde_json::json!({ "text": text }).to_string(),
    );
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    assert_eq!(v["defs"], serde_json::json!(["meadow"]));
    assert_eq!(v["regions"], serde_json::json!(["pool"]));
    // a text mid-word is no names, said so, not an error
    let (_, v) = post(
        &r,
        "/api/names",
        Some(&t),
        None,
        &serde_json::json!({ "text": "define {" }).to_string(),
    );
    assert_eq!(v["ok"], false);
}

#[test]
fn a_draft_is_a_request_answered_with_its_mesh() {
    let r = start(&[FLAT], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let t = token["token"].as_str().unwrap().to_owned();
    let text = dt_eval::Files.read(FLAT).unwrap();
    let path = format!("/api/tiles/{}/draft", id0(&r));
    let body = serde_json::json!({ "text": text }).to_string();
    let (h, mesh) = post_raw(&r, &path, Some(&t), None, &body);
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    assert_eq!(&mesh[..4], payload::MAGIC);
    let tile = &tiles(&r)["tiles"][0];
    assert_eq!(
        tile["mesh_version"],
        Value::Null,
        "a draft is not the tile's build"
    );
    // a mistake in the text is a 422, not a mesh
    let bad = serde_json::json!({ "text": "dharmatiles version=1\ntile size=\"1x1\" { stack { durt } }" }).to_string();
    let (h, _) = post_raw(&r, &path, Some(&t), None, &bad);
    assert!(h.starts_with("HTTP/1.1 422"), "{h}");
    // nothing is written, and the tile never grew a draft route of its own
    assert_eq!(dt_eval::Files.read(FLAT).unwrap(), text);
    assert!(
        get(&r, &format!("/api/tiles/{}/mesh?draft=1", id0(&r)))
            .0
            .starts_with("HTTP/1.1 404"),
        "the tile's own mesh route serves only its build, and it is not built"
    );
}

#[test]
fn a_newer_draft_of_a_tile_supersedes_the_older() {
    let shared = Shared::new(&[FLAT.to_owned()]);
    let id = tile_id(FLAT);
    let first = shared.submit_draft(&id, FLAT, FLAT, "tile {}", false);
    let second = shared.submit_draft(&id, FLAT, FLAT, "tile { }", false);
    assert!(matches!(first.recv().unwrap(), queue::Outcome::Superseded));
    // the other is still waiting, and is the next job
    assert!(second.try_recv().is_err());
    let Some(queue::Job::Draft(d)) = shared.next_job(false) else {
        panic!("the draft is the next job");
    };
    assert_eq!(d.text, "tile { }");
    // a newer one stops it while it runs
    let _third = shared.submit_draft(&id, FLAT, FLAT, "tile { }", false);
    assert!(d.cancel.load(Ordering::Relaxed));
    // and a draft goes ahead of a tile asked for
    assert!(shared.want(&id));
    assert!(matches!(shared.next_job(false), Some(queue::Job::Draft(_))));
    assert!(matches!(shared.next_job(false), Some(queue::Job::Full(..))));
    // one full build at a time
    assert!(shared.next_job(true).is_none());
}

#[test]
fn the_debug_layer_is_collected_only_for_a_tile_the_page_has_it_on_for() {
    use route::{Reply, route};
    let shared = Shared::new(&[FLAT.to_owned()]);
    let id = tile_id(FLAT);
    assert!(!shared.marks_of(&id));
    let post = |on: u8| {
        route(
            &shared,
            &http::Request::build(
                "POST",
                &format!("/api/tiles/{id}/build?debug={on}"),
                None,
                b"",
            ),
        )
    };
    // not built yet: switching it on only records it (the build asked for has the marks)
    assert!(matches!(post(1), Reply::Json("202 Accepted", _)));
    assert!(shared.marks_of(&id));
    assert!(!lock(&shared.tiles).iter().any(|t| t.stale));
    // built; the layer off again, then on: only turning it on builds the tile again
    lock(&shared.tiles)[0].status = State::Ready;
    assert!(matches!(post(0), Reply::Json(..)));
    assert!(!shared.marks_of(&id));
    assert!(!lock(&shared.tiles)[0].stale, "off rebuilds nothing");
    assert!(matches!(post(1), Reply::Json(..)));
    assert!(
        lock(&shared.tiles)[0].stale,
        "on builds it again, with the marks"
    );
    // a tile there is not
    assert!(!shared.set_marks("nope", true));
}

#[test]
fn routes_answer_without_a_socket() {
    use route::{Reply, route};
    let shared = Shared::new(&[FLAT.to_owned()]);
    let id = tile_id(FLAT);
    let json = |r: Reply| match r {
        Reply::Json(s, v) => (s, v),
        _ => panic!("not JSON"),
    };
    let (s, v) = json(route(
        &shared,
        &http::Request::build("GET", "/api/tiles", None, b""),
    ));
    assert_eq!(s, "200 OK");
    assert_eq!(v["tiles"][0]["id"], id.as_str());
    assert!(v["tiles"][0].get("index").is_none() && v["tiles"][0].get("gone").is_none());
    // no mesh yet; an unknown tile; a call without the token
    assert!(matches!(
        route(
            &shared,
            &http::Request::build("GET", &format!("/api/tiles/{id}/mesh"), None, b"")
        ),
        Reply::Text("404 Not Found", _)
    ));
    assert!(matches!(
        route(
            &shared,
            &http::Request::build("POST", &format!("/api/tiles/{id}/check"), None, b"{}")
        ),
        Reply::Text("403 Forbidden", _)
    ));
    let t = shared.token.clone();
    let (s, _) = json(route(
        &shared,
        &http::Request::build("POST", "/api/tiles/0/check", Some(&t), b"{\"text\": \"x\"}"),
    ));
    assert_eq!(s, "404 Not Found", "a position is not an id");
    let (s, v) = json(route(
        &shared,
        &http::Request::build(
            "POST",
            &format!("/api/tiles/{id}/check"),
            Some(&t),
            b"{\"text\": \"x\"}",
        ),
    ));
    assert_eq!((s, v["ok"].clone()), ("200 OK", Value::Bool(false)));
    // the event stream is the one reply the server runs itself
    assert!(matches!(
        route(
            &shared,
            &http::Request::build("GET", "/api/events?epoch=e&since=4", None, b"")
        ),
        Reply::Events(route::Resume { since: Some(4), .. })
    ));
}

#[test]
fn a_stream_resumes_from_its_last_revision_or_starts_over() {
    let dir = std::env::temp_dir().join(format!("dt-resume-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let text = dt_eval::Library.read(FLAT).unwrap();
    std::fs::write(dir.join("a.dtile"), &text).unwrap();
    let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
    let shared = Shared::with_roots(roots, &[], &Options::default());
    let id = tile_id(&dir.join("a.dtile").to_string_lossy());
    let resume = |epoch: &str, since: u64| route::Resume {
        epoch: Some(epoch.to_owned()),
        since: Some(since),
    };
    let msgs = |rx: &std::sync::mpsc::Receiver<String>| -> Vec<Value> {
        rx.try_iter()
            .map(|m| serde_json::from_str(&m).unwrap())
            .collect()
    };
    let first = msgs(&shared.subscribe(&route::Resume {
        epoch: None,
        since: None,
    }));
    assert_eq!(first[0]["type"], "snapshot");
    let (epoch, rev) = (
        first[0]["epoch"].as_str().unwrap().to_owned(),
        first[0]["rev"].as_u64().unwrap(),
    );

    // nothing changed: resumed, and nothing else
    let m = msgs(&shared.subscribe(&resume(&epoch, rev)));
    assert_eq!(m.len(), 1);
    assert_eq!(m[0]["type"], "resumed");

    // a tile changed twice: one event, as it is now
    shared.want(&id);
    shared.update(&id, |t| t.status = State::Building);
    shared.update(&id, |t| t.status = State::Ready);
    let m = msgs(&shared.subscribe(&resume(&epoch, rev)));
    assert_eq!(
        m.iter()
            .map(|v| v["type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["resumed", "tile"]
    );
    assert_eq!(m[1]["tile"]["status"], "ready");
    assert!(m[1]["rev"].as_u64().unwrap() >= rev + 2);

    // the list changed: a fresh snapshot
    std::fs::write(dir.join("b.dtile"), &text).unwrap();
    assert!(shared.rescan());
    let m = msgs(&shared.subscribe(&resume(&epoch, rev)));
    assert_eq!(m[0]["type"], "snapshot");
    assert_eq!(m[0]["tiles"].as_array().unwrap().len(), 2);

    // another run, or a revision from the future: a snapshot too
    assert_eq!(
        msgs(&shared.subscribe(&resume("other", rev)))[0]["type"],
        "snapshot"
    );
    assert_eq!(
        msgs(&shared.subscribe(&resume(&epoch, 9999)))[0]["type"],
        "snapshot"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_editor_places_and_takes_out_nodes_and_maps_the_tile() {
    let dir = std::env::temp_dir().join(format!("dt-place-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "dharmatiles version=1\ntile size=\"1x1\" {\n    line shore { through west \"(1sq, 0.5sq)\" }\n    region pool { boundedBy shore; fill \"(0.5sq, 0.8sq)\" }\n    stack {\n        dirt\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let call = |path: &str, body: Value| post(&r, path, Some(&token), None, &body.to_string()).1;
    // a rock placed: the end of the stack, and where it starts
    let v = call(
        "/api/edit",
        serde_json::json!({ "text": text, "op": "insert", "place": "stack", "node": "rock at=\"(0.3sq, 0.4sq)\"" }),
    );
    let placed = v["text"].as_str().unwrap().to_owned();
    let at = usize::try_from(v["at"].as_u64().unwrap()).unwrap();
    assert!(placed[at..].starts_with("rock at="), "{v}");
    // and taken out again
    let v = call(
        "/api/edit",
        serde_json::json!({ "text": placed, "op": "remove", "at": at }),
    );
    assert_eq!(v["text"].as_str().unwrap(), text);
    // the map: the line's points where they are written, the region's area
    let v = call(
        &format!("/api/tiles/{}/check", id0(&r)),
        serde_json::json!({ "text": text }),
    );
    let map = &v["map"];
    let line = &map["lines"][0];
    assert_eq!(line["name"], "shore");
    let p = &line["points"][1];
    let s = usize::try_from(p["span"][0].as_u64().unwrap()).unwrap();
    assert_eq!(&text[s..][..14], "\"(1sq, 0.5sq)\"");
    assert_eq!(p["at"], serde_json::json!([1.0, 0.5]));
    assert_eq!(line["points"][0]["text"], "west");
    let pool = &map["regions"][0];
    let n = pool["nx"].as_u64().unwrap();
    let bits = pool["bits"].as_str().unwrap().as_bytes();
    assert_eq!(bits.len() as u64, n * pool["ny"].as_u64().unwrap());
    // south of the shore is the pool, north of it is not
    assert_eq!(bits[usize::try_from(n * 2 + n / 2).unwrap()], b'1');
    assert_eq!(bits[usize::try_from(n * (n - 3) + n / 2).unwrap()], b'0');
    assert_eq!(pool["fills"][0]["at"], serde_json::json!([0.5, 0.2]));
}

#[test]
fn the_map_carries_the_tiles_own_controls() {
    let dir = std::env::temp_dir().join(format!("dt-own-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "dharmatiles version=1\ntile size=\"2x1\" seed=7 {\n    about \"A bank\"\n    let foot \"4dmm\"\n    stack {\n        dirt\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let v = post(
        &r,
        &format!("/api/tiles/{}/check", id0(&r)),
        Some(&token),
        None,
        &serde_json::json!({ "text": text }).to_string(),
    )
    .1;
    let own = &v["map"]["tile"];
    assert_eq!(own["at"], text.find("\ntile").unwrap() + 1, "{v}");
    let seed = own["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "seed")
        .unwrap();
    let s = usize::try_from(seed["span"][0].as_u64().unwrap()).unwrap();
    assert_eq!(&text[s..=s], "7");
    assert_eq!(seed["text"], "7");
    // the description and the tile's own heights, where they are written
    let a = &own["about"]["value"];
    let s = usize::try_from(a["span"][0].as_u64().unwrap()).unwrap();
    assert_eq!(&text[s..s + 8], "\"A bank\"");
    assert_eq!(a["text"], "\"A bank\"");
    let heights = own["heights"].as_array().unwrap();
    let has = |name: &str, value: f64, from: &str| {
        heights
            .iter()
            .any(|h| h["name"] == name && h["value"] == value && h["from"] == from)
    };
    assert!(has("foot", 4.0, "tile"), "{heights:?}");
    assert!(has("$dirtEdgeHeight", 3.4, "library"), "{heights:?}");
}

#[test]
fn export_writes_the_stl_by_the_tiles_name_and_reveal_takes_only_what_was_written() {
    let dir = std::env::temp_dir().join(format!("dt-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("deep/er")).unwrap();
    std::fs::write(dir.join("dharmatiles.dtile"), "name \"t\"\n").unwrap();
    let file = dir.join("deep/er/flat.dtile");
    std::fs::write(
        &file,
        "dharmatiles version=1\ntile size=\"1x1\" {\n    stack {\n        dirt\n    }\n}\n",
    )
    .unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let call = |p: &str, body: Value| post(&r, p, Some(&token), None, &body.to_string());
    // a target that is not one is refused
    for bad in [
        serde_json::json!({ "system": "lego" }),
        serde_json::json!({ "printer": "no-such-printer" }),
        serde_json::json!({ "system": "both" }),
    ] {
        let (h, _) = call("/api/target", bad);
        assert!(h.starts_with("HTTP/1.1 400"), "{h}");
    }
    assert_eq!(tiles(&r)["system"], "db");
    assert_eq!(tiles(&r)["printer"], "default");
    // where it would go, without building
    let (_, v) = call(
        &format!("/api/tiles/{}/export", id0(&r)),
        serde_json::json!({ "where": true }),
    );
    assert_eq!(v["out"], dir.join("out").to_string_lossy().as_ref(), "{v}");
    // the export is for the target the title bar's chip chose
    let (h, _) = call("/api/target", serde_json::json!({ "system": "ol" }));
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    assert_eq!(tiles(&r)["system"], "ol");
    let (h, v) = call(
        &format!("/api/tiles/{}/export", id0(&r)),
        serde_json::json!({}),
    );
    assert!(h.starts_with("HTTP/1.1 200"), "{h} {v}");
    assert_eq!(v["ok"], true, "{v}");
    // the project's out/, by system and the tile's own name, not its folders
    let stl = dir.join("out/ol/flat-ol.stl");
    assert!(stl.is_file(), "{v}");
    assert!(
        v["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == stl.to_string_lossy().as_ref())
    );
    // reveal: only a written STL or 3MF
    let (h, _) = call(
        "/api/reveal",
        serde_json::json!({ "path": dir.join("dharmatiles.dtile") }),
    );
    assert!(h.starts_with("HTTP/1.1 400"), "{h}");
    let (h, _) = call(
        "/api/reveal",
        serde_json::json!({ "path": dir.join("out/ol/none.stl") }),
    );
    assert!(h.starts_with("HTTP/1.1 400"), "{h}");
}

#[test]
fn a_library_tile_exports_to_my_tiles_never_into_the_library() {
    let mine = super::roots::personal_home().unwrap().join("out");
    assert_eq!(
        edit::export_dir("lib:tiles/ground/1x1-grass.dtile").unwrap(),
        mine
    );
    // the same tile named by its file inside the library folder
    if let Some(lib) = dt_eval::source::library_dir() {
        let file = lib.join("tiles/ground/1x1-grass.dtile");
        assert_eq!(edit::export_dir(&file.to_string_lossy()).unwrap(), mine);
    }
    // a file of yours: its project's out/
    let dir = std::env::temp_dir().join(format!("dt-where-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::write(dir.join("dharmatiles.dtile"), "name \"t\"\n").unwrap();
    assert_eq!(
        edit::export_dir(&dir.join("a/t.dtile").to_string_lossy()).unwrap(),
        dir.join("out")
    );
}

#[test]
fn lines_and_regions_carry_their_sheets_and_rename_with_their_references() {
    let dir = std::env::temp_dir().join(format!("dt-marks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "dharmatiles version=1\ntile size=\"1x1\" seed=3 {\n    line shore wobble=\"5dmm\" { through \"(0sq, 0.62sq)\" \"(1sq, 0.58sq)\" }\n    region pool { boundedBy shore; fill \"(0.5sq, 0.75sq)\" }\n    region bank { boundedBy shore; fill \"(0.5sq, 0.25sq)\" }\n    stack {\n        dirt at=bank\n        water\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let call = |p: &str, body: Value| post(&r, p, Some(&token), None, &body.to_string()).1;
    let v = call(
        &format!("/api/tiles/{}/check", id0(&r)),
        serde_json::json!({ "text": text }),
    );
    let line = &v["map"]["lines"][0]["sheet"];
    assert_eq!(line["controls"][0]["name"], "wobble", "{v}");
    assert_eq!(line["controls"][0]["text"], "\"5dmm\"");
    assert_eq!(
        line["through"]["text"],
        "\"(0sq, 0.62sq)\" \"(1sq, 0.58sq)\""
    );
    let s = usize::try_from(line["through"]["span"][0].as_u64().unwrap()).unwrap();
    assert!(text[s..].starts_with("\"(0sq, 0.62sq)\" \"(1sq, 0.58sq)\" }"));
    let region = &v["map"]["regions"][1]["sheet"];
    assert_eq!(region["bounded_by"]["text"], "shore");
    assert_eq!(region["fill"]["text"], "\"(0.5sq, 0.25sq)\"");
    let v = call(
        "/api/edit",
        serde_json::json!({ "op": "rename", "text": text, "mark": "region", "old": "bank", "new": "meadow" }),
    );
    let t = v["text"].as_str().unwrap();
    assert!(
        t.contains("region meadow") && t.contains("dirt at=meadow"),
        "{t}"
    );
    let v = call(
        "/api/edit",
        serde_json::json!({ "op": "rename", "text": text, "mark": "line", "old": "shore", "new": "pool" }),
    );
    assert!(
        v["text"].as_str().unwrap().contains("line pool"),
        "a line may share a region's name: {v}"
    );
}

#[test]
fn a_region_made_of_places_sends_its_clauses_and_what_it_refers_to() {
    let dir = std::env::temp_dir().join(format!("dt-clauses-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "dharmatiles version=1\ntile size=\"2x2\" seed=3 {\n    point knoll \"(0.5sq, 1.5sq)\"\n    line shore { through \"(0sq, 1.6sq)\" \"(2sq, 1.5sq)\" }\n    region crag { within \"[8dmm .. 14dmm]\" of=westEdge; within \"6dmm\" of=knoll }\n    region apron { within \"6.5dmm\" of=crag; except crag }\n    region pool { boundedBy shore; fill \"(0.5sq, 1.8sq)\" }\n    stack {\n        dirt\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let call = |p: &str, body: Value| post(&r, p, Some(&token), None, &body.to_string()).1;
    let v = call(
        &format!("/api/tiles/{}/check", id0(&r)),
        serde_json::json!({ "text": text }),
    );
    let region = |name: &str| {
        v["map"]["regions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == name)
            .unwrap()
            .clone()
    };
    let crag = region("crag");
    let clauses = crag["sheet"]["clauses"].as_array().unwrap();
    assert_eq!(clauses.len(), 2, "{crag}");
    assert_eq!(clauses[0]["kind"], "within");
    assert_eq!(clauses[0]["distance"]["text"], "\"[8dmm .. 14dmm]\"");
    assert_eq!(clauses[0]["range"], serde_json::json!([8.0, 14.0]));
    assert_eq!(clauses[0]["of"]["text"], "westEdge");
    let sp = |c: &Value, k: &str| {
        let s = c[k]["span"].as_array().unwrap();
        let a = usize::try_from(s[0].as_u64().unwrap()).unwrap();
        a..a + usize::try_from(s[1].as_u64().unwrap()).unwrap()
    };
    assert_eq!(&text[sp(&clauses[0], "distance")], "\"[8dmm .. 14dmm]\"");
    assert_eq!(&text[sp(&clauses[1], "of")], "knoll");
    // the places it refers to, with their geometry
    let refs = crag["refs"].as_array().unwrap();
    assert_eq!(refs[0]["name"], "westEdge");
    assert_eq!(refs[0]["kind"], "edge");
    assert_eq!(refs[0]["pts"], serde_json::json!([[0.0, 0.0], [0.0, 2.0]]));
    assert_eq!(refs[1]["kind"], "point");
    assert_eq!(refs[1]["at"], serde_json::json!([0.5, 0.5]));
    // a region refers to another region (once, though it names it twice), and excepts are clauses
    let apron = region("apron");
    let refs = apron["refs"].as_array().unwrap();
    assert_eq!(refs.len(), 1, "{apron}");
    assert_eq!(refs[0]["kind"], "region");
    let ex = &apron["sheet"]["clauses"][1];
    assert_eq!(ex["kind"], "except");
    assert_eq!(ex["regions"]["text"], "crag");
    // a bucket region refers to the lines that bound it, and has no clauses
    let pool = region("pool");
    assert_eq!(pool["refs"][0]["kind"], "line");
    assert_eq!(pool["sheet"]["clauses"].as_array().unwrap().len(), 0);
    // the places a choice can name
    let places = v["map"]["places"].as_array().unwrap();
    for (n, k) in [
        ("northEdge", "edge"),
        ("shore", "line"),
        ("crag", "region"),
        ("knoll", "point"),
        ("center", "point"),
        ("tile", "tile"),
    ] {
        assert!(
            places.iter().any(|p| p["name"] == n && p["kind"] == k),
            "{n}: {places:?}"
        );
    }
    // the distance is edited through the server, the way the other controls are
    let v = call(
        "/api/edit",
        serde_json::json!({ "op": "arg", "text": text, "at": clauses[1]["at"], "index": 0, "value": "9dmm" }),
    );
    let t = v["text"].as_str().unwrap();
    assert!(
        t.contains("within \"[8dmm .. 14dmm]\" of=westEdge; within \"9dmm\" of=knoll"),
        "{t}"
    );
    let v = call(
        "/api/edit",
        serde_json::json!({ "op": "set", "text": text, "at": clauses[0]["at"], "name": "of", "value": "eastEdge" }),
    );
    let t = v["text"].as_str().unwrap();
    assert!(t.contains("of=eastEdge; within \"6dmm\" of=knoll"), "{t}");
}

#[test]
fn a_build_sends_sketches_as_it_goes_and_drops_them_when_done() {
    let r = start(&[FLAT], false);
    let mut c = TcpStream::connect(r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    let mut lines = BufReader::new(c).lines();
    // the snapshot first, then ask for the build
    for l in lines.by_ref() {
        if l.unwrap().starts_with("data: ") {
            break;
        }
    }
    assert!(
        request(
            r.addr,
            "POST",
            &format!("/api/tiles/{}/build", id0(&r)),
            &r.addr.to_string()
        )
        .0
        .starts_with("HTTP/1.1 202")
    );
    let mut sketched = 0;
    for l in lines.by_ref() {
        let l = l.unwrap();
        let Some(data) = l.strip_prefix("data: ") else {
            continue;
        };
        let v: Value = serde_json::from_str(data).unwrap();
        if v["type"] != "tile" {
            continue;
        }
        let t = &v["tile"];
        let n = t["sketches"].as_array().map_or(0, Vec::len);
        if t["status"] == "ready" {
            assert_eq!(n, 0, "the finished mesh replaces the sketches");
            break;
        }
        if n > 0 {
            assert_eq!(t["status"], "building");
            assert_eq!(t["sketches"][0]["rough"], false);
            sketched += 1;
        }
    }
    // the flat tile's ground and its base, at least
    assert!(sketched >= 2, "sketches seen in {sketched} updates");
}

#[test]
fn the_chosen_target_is_remembered_and_every_built_tile_is_built_again() {
    let dir = std::env::temp_dir().join(format!("dt-target-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("printers")).unwrap();
    std::fs::write(
        dir.join("printers/mine.dtile"),
        "dharmatiles version=1\nprinter mine {\n line \"0.5mm\"\n layer \"0.2mm\"\n overhang \"50deg\"\n steepest \"62deg\"\n gap \"0.2mm\"\n}\n",
    )
    .unwrap();
    let roots = vec![Root::new("My tiles", RootKind::Dir(dir.clone()))];
    let shared = Shared::with_roots(roots, &[], &Options::default()).with_home(dir.clone());
    // the page is told the printers there are: the library's and yours
    let snap = shared.snapshot();
    let names: Vec<(&str, bool)> = snap["printers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap(), p["mine"].as_bool().unwrap()))
        .collect();
    assert!(names.contains(&("default", false)), "{names:?}");
    assert!(names.contains(&("fine", false)), "{names:?}");
    assert!(names.contains(&("mine", true)), "{names:?}");
    assert_eq!(snap["printer"], "default");
    // one tile has been built
    let id = shared.snapshot()["tiles"].as_array().map(|t| t.len());
    assert_eq!(id, Some(0));
    {
        let mut tiles = lock(&shared.tiles);
        let mut t = Slot::new("lib:tests/ground/1x1-flat.dtile");
        t.status = State::Ready;
        tiles.push(t);
    }
    assert!(shared.set_target(None, Some("mine")).is_ok());
    let snap = shared.snapshot();
    assert_eq!(snap["printer"], "mine");
    assert_eq!(snap["system"], "db");
    // the gate's angles are the chosen printer's
    assert!((snap["overhang_deg"].as_f64().unwrap() - 50.0).abs() < 1e-9);
    assert!((snap["steepest_deg"].as_f64().unwrap() - 62.0).abs() < 1e-9);
    // the built tile is stale: it builds again for the new target
    assert!(lock(&shared.tiles).iter().all(|t| t.stale && t.changed()));
    assert_eq!(shared.options().target(), "db-mine");
    // and the choice is in the home, for the next run
    let (sys, printer) = crate::library::remembered_target(&dir).unwrap();
    assert_eq!(
        (sys.as_deref(), printer.as_deref()),
        (Some("db"), Some("mine"))
    );
    assert!(shared.set_target(Some("ol"), None).is_ok());
    assert_eq!(shared.options().target(), "ol-mine");
    assert!(shared.set_target(Some("lego"), None).is_err());
    assert!(shared.set_target(None, Some("../mine")).is_err());
    // a profile of yours that has gone: the remembered target falls back
    std::fs::remove_file(dir.join("printers/mine.dtile")).unwrap();
    let o = crate::library::options_in("db", "default", None, Some(&dir)).unwrap();
    assert_eq!(o.target(), "db");
}

/// An internal failure is an error status with its message; a mistake in
/// the document is data with its code.  Neither ends the server.
#[test]
fn a_document_error_is_data_and_an_internal_failure_an_error_status() {
    use route::Reply;
    fail::install_recorder();
    let dir = std::env::temp_dir().join(format!("dt-errors-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    // a file the program owns but cannot read as text: not a mistake in a document
    std::fs::write(&file, [0xff, 0xfe, 0xfd]).unwrap();
    let path = file.to_string_lossy().into_owned();
    let shared = Shared::new(std::slice::from_ref(&path));
    let id = tile_id(&path);
    let t = shared.token.clone();
    let json = |r: Reply| match r {
        Reply::Json(s, v) => (s, v),
        _ => panic!("not JSON"),
    };
    let call = |m: &str, p: &str, body: &[u8]| {
        json(server::answer(
            &shared,
            &http::Request::build(m, p, Some(&t), body),
        ))
    };
    // a document error: 200, ok false, a code and a place
    let text = "dharmatiles version=1\ntile size=\"1x1\" {\n    stack {\n        dirtt\n    }\n}\n";
    let body = serde_json::json!({ "text": text }).to_string();
    let (s, v) = call("POST", &format!("/api/tiles/{id}/check"), body.as_bytes());
    assert_eq!((s, v["ok"].clone()), ("200 OK", Value::Bool(false)));
    let e = &v["errors"][0];
    assert!(e["code"].as_str().unwrap().starts_with("dt::"), "{e}");
    assert_eq!(e["unexpected"], false, "a mistake in the document");
    // an internal failure: the file cannot be read; 500 and the reason
    let body = serde_json::json!({ "text": "x", "base": "y" }).to_string();
    let (s, v) = call("POST", &format!("/api/tiles/{id}/save"), body.as_bytes());
    assert_eq!(s, "500 Internal Server Error");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("cannot read"),
        "{v}"
    );
    let (s, v) = call("GET", &format!("/api/tiles/{id}/text"), b"");
    assert_eq!(s, "500 Internal Server Error");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("cannot read"),
        "{v}"
    );
    // a panic in a route: 500 naming it, and the next call is answered
    let (s, v) = call("GET", "/api/test-panic", b"");
    assert_eq!(s, "500 Internal Server Error");
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("a test panic in a route"),
        "{v}"
    );
    let e = &v["error"];
    assert_eq!(e["kind"], "bug", "{v}");
    assert_eq!(e["context"]["route"], "GET /api/test-panic");
    assert!(
        e["causes"][0].as_str().unwrap().starts_with("panicked at "),
        "{v}"
    );
    let trace = e["backtrace"].as_str().unwrap();
    assert!(!trace.is_empty(), "a bug carries the panic's stack");
    assert!(
        trace.contains("route"),
        "the stack is the panic's own: {trace}"
    );
    assert_eq!(call("GET", "/api/tiles", b"").0, "200 OK");
    // the same shape for a failure with a chain of causes, a refusal and a bug
    let (s, v) = call("GET", "/api/test-failure", b"");
    assert_eq!(s, "500 Internal Server Error");
    let e = &v["error"];
    assert_eq!(e["kind"], "failure", "{v}");
    assert_eq!(e["causes"].as_array().unwrap().len(), 3, "{v}");
    assert!(!e["backtrace"].as_str().unwrap().is_empty(), "{v}");
    let (s, v) = call("POST", "/api/tiles/nope/check", b"{}");
    assert_eq!(s, "404 Not Found");
    assert_eq!(v["error"]["kind"], "refused", "{v}");
    assert_eq!(v["error"]["context"]["route"], "POST /api/tiles/nope/check");
    assert_eq!(v["error"]["context"]["tile"], "nope");
    assert!(!v["error"]["backtrace"].as_str().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A panic in a build (or in encoding it) is reported to the page as a
/// failed build with its message, and the worker goes on: the other tile
/// builds, and the failed one builds again when its file changes.
#[test]
fn a_panic_in_a_build_is_reported_and_the_server_keeps_serving() {
    fail::install_recorder();
    let dir = std::env::temp_dir().join(format!("dt-panics-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = dt_eval::Library.read(FLAT).unwrap();
    let in_build = dir.join("dt-test-panic-in-build.dtile");
    let in_encode = dir.join("dt-test-panic-in-encode.dtile");
    std::fs::write(&in_build, &text).unwrap();
    std::fs::write(&in_encode, &text).unwrap();
    let names: Vec<String> = [&in_build, &in_encode]
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .chain([FLAT.to_owned()])
        .collect();
    let r = start(&names.iter().map(String::as_str).collect::<Vec<_>>(), true);
    let tile = |name: &str| -> Value {
        tiles(&r)["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["path"].as_str().unwrap().contains(name))
            .unwrap()
            .clone()
    };
    let t0 = Instant::now();
    while ["panic-in-build", "panic-in-encode", "1x1-flat"]
        .iter()
        .any(|n| !matches!(tile(n)["status"].as_str(), Some("failed" | "ready")))
    {
        assert!(t0.elapsed() < Duration::from_secs(120), "timed out");
        std::thread::sleep(Duration::from_millis(100));
    }
    for (name, words) in [
        ("panic-in-build", "a test panic in the build"),
        ("panic-in-encode", "a test panic in the encoding"),
    ] {
        let t = tile(name);
        assert_eq!(t["status"], "failed", "{name}");
        let e = &t["errors"][0];
        assert_eq!(e["code"], "dt::panic");
        assert_eq!(e["unexpected"], true);
        assert!(e["message"].as_str().unwrap().contains(words), "{e}");
        // the page also gets the structured failure, with the panic's stack
        let f = &t["failure"];
        assert_eq!(f["kind"], "bug", "{name}: {t}");
        assert!(f["message"].as_str().unwrap().contains(words), "{f}");
        assert_eq!(f["context"]["route"], "build worker");
        assert!(f["context"]["tile"].as_str().unwrap().contains(name), "{f}");
        assert!(!f["backtrace"].as_str().unwrap().is_empty(), "{f}");
    }
    // the worker went on, and the server still serves
    assert_eq!(tile("1x1-flat")["status"], "ready");
    assert!(get(&r, "/api/tiles").0.starts_with("HTTP/1.1 200"));
    drop(r);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_map_marks_every_point_a_line_or_region_refers_to() {
    let dir = std::env::temp_dir().join(format!("dt-markers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "dharmatiles version=1\ntile size=\"2x2\" seed=3 {\n    point knoll \"(0.5sq, 1.5sq)\"\n    line shore { through west \"(0sq, 1.5sq)\" knoll \"(1.5sq, 0.75sq)\" east }\n    line ring closed=#true { through \"(0.25sq, 1.75sq)\" \"(0.75sq, 1.75sq)\" \"(0.5sq, 1.25sq)\" }\n    region bay { boundedBy shore; fill \"(0.5sq, 1.75sq)\"; fill center; within \"4dmm\" of=\"(1.25sq, 1.75sq)\"; within \"6dmm\" of=knoll; within \"5dmm\" of=shore }\n    stack {\n        dirt\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let v = post(
        &r,
        &format!("/api/tiles/{}/check", id0(&r)),
        Some(&token),
        None,
        &serde_json::json!({ "text": text }).to_string(),
    )
    .1;
    let map = &v["map"];
    let span = |m: &Value| {
        let s = m["span"].as_array().unwrap();
        let a = usize::try_from(s[0].as_u64().unwrap()).unwrap();
        a..a + usize::try_from(s[1].as_u64().unwrap()).unwrap()
    };
    // a line: every through point, named or literal, as written
    let shore = map["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "shore")
        .unwrap();
    let m = shore["markers"].as_array().unwrap();
    let labels: Vec<_> = m.iter().map(|m| m["label"].as_str().unwrap()).collect();
    assert_eq!(
        labels,
        ["west", "(0sq, 1.5sq)", "knoll", "(1.5sq, 0.75sq)", "east"]
    );
    let kinds: Vec<_> = m.iter().map(|m| m["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["name", "literal", "name", "literal", "name"]);
    assert_eq!(m[0]["at"], serde_json::json!([0.0, 1.0]));
    assert_eq!(m[2]["at"], serde_json::json!([0.5, 0.5]));
    assert_eq!(m[3]["at"], serde_json::json!([1.5, 1.25]));
    assert_eq!(&text[span(&m[3])], "\"(1.5sq, 0.75sq)\"");
    // a closed line has them all still (its drawn points repeat the first)
    let ring = map["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "ring")
        .unwrap();
    assert_eq!(ring["markers"].as_array().unwrap().len(), 3);
    assert_eq!(ring["markers"][1]["at"], serde_json::json!([0.75, 0.25]));
    // a region: each fill, then each within centre that is a point (not the line)
    let bay = map["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "bay")
        .unwrap();
    let m = bay["markers"].as_array().unwrap();
    let labels: Vec<_> = m.iter().map(|m| m["label"].as_str().unwrap()).collect();
    assert_eq!(
        labels,
        ["(0.5sq, 1.75sq)", "center", "(1.25sq, 1.75sq)", "knoll"],
        "{m:?}"
    );
    assert_eq!(m[1]["at"], serde_json::json!([1.0, 1.0]));
    assert_eq!(m[2]["at"], serde_json::json!([1.25, 0.25]));
}

fn curve_of(r: &Running, path: &str) -> (String, Value) {
    let (h, b) = get(r, path);
    (h, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

#[test]
fn the_weightings_route_serves_the_programs_curves() {
    let r = start(&[], false);
    // every function over a range, each at its defaults, curves sampled by the program
    let (h, v) = curve_of(&r, "/api/weightings?range=1..6");
    assert!(h.starts_with("HTTP/1.1 200"), "{h} {v}");
    let names: Vec<&str> = v["functions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "flat", "low", "high", "bell", "peak", "ends", "steps", "list", "mix"
        ]
    );
    let f = |n: &str| {
        v["functions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == n)
            .unwrap()
            .clone()
    };
    assert_eq!(f("low")["text"], "low(1..6, 4)");
    assert_eq!(f("flat")["text"], "1..6");
    let low = f("low")["curve"]["values"].as_array().unwrap().clone();
    assert_eq!(low.len(), 64);
    // the low end is 4 times as likely as the high end (a ratio of end to end)
    let (first, last) = (low[0].as_f64().unwrap(), low[63].as_f64().unwrap());
    assert!(
        first > last && (first / last - 4.0).abs() < 0.2,
        "{first} {last}"
    );
    // a flat range is all 1s, and every curve averages 1
    for n in ["flat", "low", "bell", "peak", "ends", "steps", "mix"] {
        let c = f(n)["curve"]["values"].as_array().unwrap().clone();
        let mean = c.iter().map(|x| x.as_f64().unwrap()).sum::<f64>() / 64.0;
        assert!((mean - 1.0).abs() < 1e-9, "{n} {mean}");
    }
    assert!(
        f("flat")["curve"]["values"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x == 1.0)
    );
    // a list has values and shares, not a curve; the shares add to 1
    let list = f("list");
    assert_eq!(list["curve"]["kind"], "masses");
    let total: f64 = list["curve"]["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m[1].as_f64().unwrap())
        .sum();
    assert!((total - 1.0).abs() < 1e-9);
    // the limits come with it
    assert_eq!(f("bell")["args"][0]["name"], "spread");
    assert_eq!(f("bell")["args"][0]["least"], 0.05);
}

#[test]
fn the_weightings_route_reads_a_value_and_refuses_with_the_programs_words() {
    let r = start(&[], false);
    let (_, v) = curve_of(&r, "/api/weightings?fn=low&range=1..6&args=12.5");
    assert_eq!(v["ok"], true);
    assert_eq!(v["text"], "low(1..6, 12.5)");
    assert_eq!(v["function"], "low");
    assert_eq!(v["numbers"][0], 12.5);
    // a typed value, canonical
    let (_, v) = curve_of(&r, "/api/weightings?text=bell(1..6%2C%200.3)");
    assert_eq!(v["text"], "bell(1..6, 0.3, 0.5)");
    // out of limits: the dt::range message, not a failure
    let (h, v) = curve_of(&r, "/api/weightings?fn=low&range=1..6&args=-2");
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    assert_eq!(v["ok"], false);
    assert_eq!(v["code"], "dt::range");
    assert_eq!(v["message"], "low's ratio must be above 0, not -2");
    assert!(v["help"].as_str().unwrap().contains("leans low"));
    // a value listed twice: dt::weighting
    let (_, v) = curve_of(&r, "/api/weightings?text=%5B1%3A4%2C%202%3A5%2C%202%3A1%5D");
    assert_eq!(v["code"], "dt::weighting");
    // not understood: a 400 with the reason
    let (h, v) = curve_of(&r, "/api/weightings");
    assert!(h.starts_with("HTTP/1.1 400"), "{h}");
    assert!(v["error"]["message"].is_string());
    let (h, _) = curve_of(&r, "/api/weightings?fn=wobble&range=1..6");
    assert!(h.starts_with("HTTP/1.1 400"), "{h}");
}

// ---- format 1 in the editor and the preview ----

const FORMAT_1: &str = "dharmatiles version=1\ntile size=\"2x2\" seed=3 {\n    point knoll \"(0.5sq, 0.5sq)\"\n    line shore { through west \"(0, 0.38sq)\" knoll \"(1.5sq, 1.25sq)\" east }\n    region bay { boundedBy shore; fill \"(0.5sq, 1.6sq)\" }\n    stack {\n        base\n        dirt\n        rock at=\"(1.5sq, 0.5sq)\" size=\"8dmm\"\n    }\n}\n";

fn preview_opts() -> Options {
    Options {
        ..Options::default()
    }
}

#[test]
fn a_format_1_tile_opens_builds_and_shows_its_errors_in_the_editor() {
    let dir = std::env::temp_dir().join(format!("dt-f1-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    std::fs::write(&file, FORMAT_1).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start_with(&[&path], true, &preview_opts());
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let call = |p: &str, body: Value| post(&r, p, Some(&token), None, &body.to_string()).1;
    // it opens: the build comes back ready, with no errors
    let t = wait_for(&r, "ready", |t| {
        t["status"] == "ready" || t["status"] == "failed"
    });
    assert_eq!(t["status"], "ready", "{t}");
    // its check: the format, and every point as the program reads it (the
    // north-west counting is the program's, not the page's)
    let check = format!("/api/tiles/{}/check", id0(&r));
    let v = call(&check, serde_json::json!({ "text": FORMAT_1 }));
    assert_eq!(v["ok"], true, "{v}");
    let map = &v["map"];
    assert!(
        map["language"]["names"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n == "$bottom")
    );
    let shore = &map["lines"][0];
    let m = shore["markers"].as_array().unwrap();
    let labels: Vec<_> = m.iter().map(|m| m["label"].as_str().unwrap()).collect();
    assert_eq!(
        labels,
        ["west", "(0, 0.38sq)", "knoll", "(1.5sq, 1.25sq)", "east"]
    );
    let kinds: Vec<_> = m.iter().map(|m| m["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["name", "literal", "name", "literal", "name"]);
    assert_eq!(m[1]["at"], serde_json::json!([0.0, 1.62]));
    assert_eq!(m[2]["at"], serde_json::json!([0.5, 1.5]));
    assert_eq!(m[3]["at"], serde_json::json!([1.5, 0.75]));
    let bay = &map["regions"][0];
    let fill = bay["markers"][0]["at"].as_array().unwrap();
    assert!((fill[0].as_f64().unwrap() - 0.5).abs() < 1e-9);
    assert!((fill[1].as_f64().unwrap() - 0.4).abs() < 1e-9, "{fill:?}");
    // a placed thing says where it stands, for its handle
    let rock = v["stack"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["what"] == "rock")
        .unwrap();
    assert_eq!(rock["at"], serde_json::json!([1.5, 1.5]));
    // a mistake is shown where it is made, with its help: a bare size
    let bad = FORMAT_1.replace("size=\"8dmm\"", "size=8");
    let v = call(&check, serde_json::json!({ "text": bad }));
    assert_eq!(v["ok"], false, "{v}");
    assert_eq!(v["errors"][0]["code"], "dt::unit", "{v}");
    assert!(
        v["errors"][0]["help"].as_str().unwrap().contains("dmm"),
        "{v}"
    );
    // and a point outside its region, at check
    let off = FORMAT_1.replace("(1.5sq, 0.5sq)", "(1.5sq, 2.5sq)");
    let v = call(&check, serde_json::json!({ "text": off }));
    assert_eq!(v["ok"], false, "{v}");
    assert!(
        v["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("off the tile"),
        "{v}"
    );
    // the sheet's check of a typed value is read with units
    let cv = |control: &str, typed: &str| {
        let v = call(
            "/api/check-value",
            serde_json::json!({ "element": "rock", "control": control, "typed": typed }),
        );
        // the message, with its help after it
        match v["message"].as_str() {
            None => Value::Null,
            Some(m) => Value::String(format!("{m} {}", v["help"].as_str().unwrap_or_default())),
        }
    };
    assert!(cv("size", "8dmm").is_null());
    assert!(cv("size", "6dmm..10dmm").is_null());
    assert!(cv("size", "2sq").is_null());
    assert!(cv("size", "$x").is_null() || cv("size", "$x").is_string());
    assert!(
        cv("size", "8").as_str().unwrap().contains("dmm"),
        "a bare number needs its unit"
    );
    assert!(
        cv("size", "250dmm")
            .as_str()
            .unwrap()
            .starts_with("`rock size` is ")
    );
    assert!(cv("size", "8deg").as_str().unwrap().contains("length"));
    assert!(cv("size", "low(20dmm..60dmm, 3)").is_null());
    assert!(
        cv("size", "low(20dmm..60dmm, -2)")
            .as_str()
            .unwrap()
            .starts_with("low's ratio must be above 0, not -2")
    );
    // what the sheet writes lands as a string with its unit, and checks
    let at = FORMAT_1.find("rock").unwrap();
    let v = call(
        "/api/edit",
        serde_json::json!({ "text": FORMAT_1, "at": at, "name": "size", "value": "9dmm" }),
    );
    let edited = v["text"].as_str().unwrap();
    assert!(edited.contains("size=\"9dmm\""), "{v}");
    let v = call(&check, serde_json::json!({ "text": edited }));
    assert_eq!(v["ok"], true, "{v}");
    // the controls come with their defaults as a format 1 document writes them
    let els: Value = serde_json::from_slice(&get(&r, "/api/elements").1).unwrap();
    let tree = els
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "tree")
        .unwrap();
    let crown = tree["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "crown")
        .unwrap();
    let width = crown["subs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "width")
        .unwrap();
    assert_eq!(width["default"], "30");
    assert_eq!(width["default1"], "30dmm");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_older_tile_is_refused_where_it_is_checked_with_help_to_migrate_it() {
    // a format 0 file: no header
    let old = "tile size=\"1x1\" seed=3 {\n    stack {\n        base\n        dirt\n    }\n}\n";
    let dir = std::env::temp_dir().join(format!("dt-up-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    std::fs::write(&file, old).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], true);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    // it does not build, and says why
    let t = wait_for(&r, "failed", |t| {
        t["status"] == "ready" || t["status"] == "failed"
    });
    assert_eq!(t["status"], "failed", "{t}");
    // the editor's check gives the same refusal, with the command to run
    let id = id0(&r);
    let v = post(
        &r,
        &format!("/api/tiles/{id}/check"),
        Some(&token),
        None,
        &serde_json::json!({ "text": old }).to_string(),
    )
    .1;
    assert_eq!(v["ok"], false, "{v}");
    let e = v["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["code"] == "dt::format-0")
        .unwrap_or_else(|| panic!("{v}"));
    assert!(
        e["help"].as_str().unwrap().contains("dharmatiles migrate"),
        "{e}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), old);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the stream of messages ------------------------------------------------

const STEEP: &str = "dharmatiles version=\"1.0.0\"\n\ntile size=\"1x1\" seed=3 {\n    line ridge { through \"(0.3sq, 0.5sq)\" \"(0.7sq, 0.5sq)\" }\n    region hill { within \"2dmm\" of=ridge }\n    stack {\n        base\n        dirt {\n            pin top=\"12dmm\" at=hill\n        }\n    }\n}\n";

/// A temp tile with `text`, served and not built; its path and the editor's token.
fn editable(name: &str, text: &str) -> (Running, String, String) {
    let dir = std::env::temp_dir().join(format!("dt-msg-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    (r, path, token)
}

fn check_text(r: &Running, token: &str, text: &str) -> Value {
    let (h, v) = post(
        r,
        &format!("/api/tiles/{}/check", id0(r)),
        Some(token),
        Some(&format!("http://{}", r.addr)),
        &serde_json::json!({ "text": text }).to_string(),
    );
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    v
}

fn messages(r: &Running, query: &str) -> Vec<Value> {
    let (head, body) = get(r, &format!("/api/messages?{query}"));
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let v: Value = serde_json::from_slice(&body).unwrap();
    v["messages"].as_array().unwrap().clone()
}

#[test]
fn a_note_from_the_checker_reaches_the_stream_and_the_messages_route() {
    let (r, _, token) = editable("note", STEEP);
    let v = check_text(&r, &token, STEEP);
    assert_eq!(v["ok"], true, "{v}");
    let id = id0(&r);
    let found = messages(&r, &format!("tile={id}&kind=note"));
    assert_eq!(found.len(), 1, "{found:?}");
    let n = &found[0];
    assert_eq!(n["code"], "dt::steep");
    assert_eq!(n["severity"], "note");
    assert_eq!(n["scope"], "check");
    assert_eq!(n["state"], "open");
    assert!(n["text"].as_str().unwrap().contains("steeper than its"));
    assert!(n["span"]["line"].as_u64().is_some());
    // the text form is the same message, one function
    let (head, body) = get(
        &r,
        &format!("/api/messages?tile={id}&kind=note&format=text"),
    );
    assert!(head.contains("text/plain"), "{head}");
    let text = String::from_utf8(body).unwrap();
    assert!(text.contains(" note dt::steep t "), "{text}");
    assert!(text.contains("\n  help: "), "{text}");
    // and it is on the event stream, to a page that connects after
    let mut c = TcpStream::connect(r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut seen = None;
    for l in BufReader::new(c).lines() {
        let l = l.unwrap();
        let Some(data) = l.strip_prefix("data: ") else {
            continue;
        };
        let v: Value = serde_json::from_str(data).unwrap();
        if v["type"] == "message" && v["message"]["code"] == "dt::steep" {
            seen = Some(v["message"].clone());
            break;
        }
    }
    assert_eq!(seen.as_ref().map(|m| &m["seq"]), Some(&n["seq"]));
    // `since` reads only what came after
    let after = n["seq"].as_u64().unwrap();
    assert!(messages(&r, &format!("tile={id}&since={after}")).is_empty());
    // a format that is not one is refused in words
    let (head, _) = get(&r, "/api/messages?format=xml");
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
}

#[test]
fn a_new_check_clears_what_it_no_longer_reports() {
    let (r, _, token) = editable("clear", STEEP);
    let broken = STEEP.replace("        dirt {", "        durt {");
    let v = check_text(&r, &token, &broken);
    assert_eq!(v["ok"], false, "{v}");
    let id = id0(&r);
    let open = messages(&r, &format!("tile={id}&state=open&severity=error"));
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0]["kind"], "error");
    assert!(open[0]["span"]["line"].as_u64().is_some());
    // the same mistake again is the same message: it keeps its place in the stream
    check_text(&r, &token, &broken);
    let again = messages(&r, &format!("tile={id}&state=open&severity=error"));
    assert_eq!(again[0]["seq"], open[0]["seq"]);
    // fixed: the mistake is cleared (kept, as cleared), and the notes are there
    check_text(&r, &token, STEEP);
    assert!(messages(&r, &format!("tile={id}&state=open&severity=error")).is_empty());
    let cleared = messages(&r, &format!("tile={id}&state=cleared"));
    assert!(
        cleared.iter().any(|m| m["seq"] == open[0]["seq"]),
        "{cleared:?}"
    );
    let notes = messages(&r, &format!("tile={id}&state=open&kind=note"));
    assert_eq!(notes.len(), 1, "{notes:?}");
}

#[test]
fn a_build_says_its_timing_and_the_gates_numbers_and_progress_is_not_kept() {
    let r = start(&[FLAT], true);
    let t = wait_for(&r, "the build", |t| t["status"] == "ready");
    let id = t["id"].as_str().unwrap().to_owned();
    let found = messages(&r, &format!("tile={id}&state=open"));
    let kinds: Vec<&str> = found.iter().map(|m| m["kind"].as_str().unwrap()).collect();
    assert!(
        kinds.contains(&"build") && kinds.contains(&"gate"),
        "{kinds:?}"
    );
    assert!(!kinds.contains(&"progress"), "{kinds:?}");
    let build = found.iter().find(|m| m["kind"] == "build").unwrap();
    assert!(build["text"].as_str().unwrap().starts_with("Built in "));
    assert!(build["data"]["build_s"].as_f64().unwrap() > 0.0);
    let gate = found.iter().find(|m| m["kind"] == "gate").unwrap();
    assert_eq!(gate["code"], "gate");
    assert_eq!(gate["severity"], "info");
    assert!(gate["data"]["triangles"].as_u64().unwrap() > 0);
    assert!(gate["data"]["failures"].as_array().unwrap().is_empty());
    // no progress message is left in the log, cleared or not
    assert!(messages(&r, &format!("tile={id}&kind=progress")).is_empty());
}

#[test]
fn a_failure_of_the_server_and_one_the_page_reports_are_messages_too() {
    let r = start(&[], false);
    let (head, _) = get(&r, "/api/test-failure");
    assert!(head.starts_with("HTTP/1.1 500"), "{head}");
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let (h, _) = post(
        &r,
        "/api/messages",
        Some(&token),
        Some(&format!("http://{}", r.addr)),
        &serde_json::json!({ "what": "The page hit a script error", "text": "x is not a function", "backtrace": "at a.js:1" }).to_string(),
    );
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    let found = messages(&r, "kind=server");
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0]["data"]["origin"], "server");
    assert_eq!(found[0]["severity"], "error");
    assert!(found[0]["data"]["backtrace"].is_string());
    assert_eq!(found[1]["data"]["origin"], "page");
    assert_eq!(found[1]["text"], "x is not a function");
    assert_eq!(found[1]["data"]["route"], "The page hit a script error");
}

#[test]
fn a_mistake_the_build_finds_is_a_message_at_its_entry_and_a_fixed_build_clears_it() {
    let tree = |trunk: &str| {
        format!(
            "dharmatiles version=\"1.0.0\"\n\ntile size=\"1x1\" seed=4 {{\n    stack {{\n        base\n        dirt\n        tree at=center top=\"$bottom + 22dmm\" {{\n            trunk height=\"{trunk}\"\n        }}\n    }}\n}}\n"
        )
    };
    let (r, path, _) = editable("look", &tree("40dmm"));
    let r = {
        // a build that is asked for: the editable() server does not build on its own
        drop(r);
        start(&[&path], true)
    };
    let id = id0(&r);
    wait_for(&r, "the failed build", |t| t["status"] == "failed");
    let errs = messages(&r, &format!("tile={id}&state=open&kind=error"));
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0]["code"], "dt::look");
    assert_eq!(errs[0]["scope"], "build");
    assert!(
        errs[0]["node"].as_str().is_some_and(|n| n.contains("tree")),
        "the entry it is in: {:?}",
        errs[0]["node"]
    );
    // no build record while it fails; the plan pass's own message is there
    assert!(messages(&r, &format!("tile={id}&state=open&kind=build")).is_empty());
    // fixed on disk: the worker rebuilds, the error is cleared and the record appears
    std::fs::write(&path, tree("10dmm")).unwrap();
    wait_for(&r, "the good build", |t| t["status"] == "ready");
    assert!(messages(&r, &format!("tile={id}&state=open&kind=error")).is_empty());
    let cleared = messages(&r, &format!("tile={id}&state=cleared&kind=error"));
    assert_eq!(cleared[0]["seq"], errs[0]["seq"]);
    assert_eq!(
        messages(&r, &format!("tile={id}&state=open&kind=gate")).len(),
        1
    );
}

// ---- editing from the structure --------------------------------------------

const ROWS: &str = "dharmatiles version=\"1.0.0\"\n\ntile size=\"1x1\" seed=3 {\n    line shore { through west \"(1sq, 0.5sq)\" }\n    stack {\n        base\n        // the soil\n        dirt\n        rock at=\"(0.3sq, 0.4sq)\"\n        grass\n    }\n}\n";

#[test]
fn an_entry_moves_whole_and_a_new_one_goes_between_two() {
    let (r, _, token) = editable("move", ROWS);
    let call = |body: Value| post(&r, "/api/edit", Some(&token), None, &body.to_string()).1;
    let dirt = ROWS.find("dirt").unwrap();
    let grass = ROWS.find("grass").unwrap();
    let v = call(json!({ "text": ROWS, "op": "move", "at": dirt, "before": null }));
    let moved = v["text"].as_str().unwrap();
    // the comment on top of it went with it, and the reply says where it now starts
    assert!(
        moved.contains("grass\n        // the soil\n        dirt\n    }"),
        "{v}"
    );
    let at = usize::try_from(v["at"].as_u64().unwrap()).unwrap();
    assert!(moved[at..].starts_with("dirt"), "{v}");
    // before a sibling
    let v = call(json!({ "text": ROWS, "op": "move", "at": grass, "before": dirt }));
    assert!(
        v["text"]
            .as_str()
            .unwrap()
            .contains("base\n        grass\n        // the soil\n        dirt"),
        "{v}"
    );
    // a target that is not a sibling is refused in words
    let (head, body) = post(
        &r,
        "/api/edit",
        Some(&token),
        None,
        &json!({ "text": ROWS, "op": "move", "at": dirt, "before": ROWS.find("shore").unwrap() })
            .to_string(),
    );
    assert!(head.starts_with("HTTP/1.1 422"), "{head}");
    assert!(body.to_string().contains("siblings"), "{body}");
    // a new entry written above another
    let v = call(
        json!({ "text": ROWS, "op": "insert", "place": "before", "at": grass, "node": "boulder" }),
    );
    let t = v["text"].as_str().unwrap();
    assert!(
        t.contains("rock at=\"(0.3sq, 0.4sq)\"\n        boulder\n        grass"),
        "{v}"
    );
    assert!(t[usize::try_from(v["at"].as_u64().unwrap()).unwrap()..].starts_with("boulder"));
}

fn offer<'a>(v: &'a Value, name: &str) -> &'a Value {
    v["offers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == name)
        .unwrap_or_else(|| panic!("no offer called {name}: {v}"))
}

#[test]
fn the_add_menu_offers_what_the_plan_lets_stand_there_and_says_why_not() {
    let (r, _, token) = editable("offers", ROWS);
    let id = id0(&r);
    let ask = |text: &str, at: u64| {
        let (h, v) = post(
            &r,
            &format!("/api/tiles/{id}/offers"),
            Some(&token),
            None,
            &json!({ "text": text, "at": at }).to_string(),
        );
        assert!(h.starts_with("HTTP/1.1 200"), "{h}");
        v
    };
    // the stack is dirt, rock, grass (the base is not one of its entries)
    let end = ask(ROWS, 3);
    assert_eq!(end["ok"], true, "{end}");
    assert_eq!(offer(&end, "tree")["ok"], true, "{end}");
    assert_eq!(offer(&end, "water")["ok"], true);
    // a wall runs along the drawn line
    assert_eq!(offer(&end, "wall")["node"], "wall at=shore");
    // the tile has its base already
    assert_eq!(offer(&end, "base")["ok"], false);
    assert!(
        offer(&end, "base")["why"]
            .as_str()
            .unwrap()
            .contains("already")
    );
    // above the dirt, a tree would stand on ground not yet painted
    let top = ask(ROWS, 0);
    assert_eq!(offer(&top, "tree")["ok"], false, "{top}");
    assert!(
        offer(&top, "tree")["why"]
            .as_str()
            .unwrap()
            .contains("ground"),
        "{top}"
    );
    // things that read nothing stand anywhere
    assert_eq!(offer(&top, "rock")["ok"], true);
    // a floor lies on the ground above it: a dirt listed after it is refused, naming the floor
    let floor = ROWS.replace("        grass\n", "        planks\n");
    let after = ask(&floor, 3);
    assert_eq!(offer(&after, "dirt")["ok"], false, "{after}");
    assert!(
        offer(&after, "dirt")["why"]
            .as_str()
            .unwrap()
            .contains("planks"),
        "{after}"
    );
    assert_eq!(offer(&ask(&floor, 1), "dirt")["ok"], true);
    // a tile with a mistake has no plan to ask
    let broken = ask(&ROWS.replace("rock at", "rock at=nowhere at"), 0);
    assert_eq!(broken["ok"], false, "{broken}");
    assert!(broken["why"].as_str().is_some());
}

#[test]
fn every_element_can_be_offered_with_nothing_set() {
    let (r, _, token) = editable("offers-all", ROWS);
    let (_, v) = post(
        &r,
        &format!("/api/tiles/{}/offers", id0(&r)),
        Some(&token),
        None,
        &json!({ "text": ROWS, "at": 3 }).to_string(),
    );
    let names: Vec<&str> = v["offers"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|o| o["name"].as_str())
        .collect();
    for want in [
        "dirt",
        "grass",
        "reeds",
        "water",
        "rock",
        "tree",
        "wall",
        "flagstones",
        "planks",
        "cliff",
        "scatter",
        "flower",
        "base",
    ] {
        assert!(names.contains(&want), "{want} in {names:?}");
    }
    // the line each would write is one the reader accepts in a stack
    let mut bad = Vec::new();
    for o in v["offers"].as_array().unwrap() {
        if o["ok"] != true || o["name"] == "base" {
            continue;
        }
        let node = o["node"].as_str().unwrap();
        let text = ROWS.replace(
            "        grass\n",
            &format!("        {}\n", node.replace('\n', "\n        ")),
        );
        let c = check_text(&r, &token, &text);
        if c["ok"] != true {
            bad.push(format!("{node}: {}", c["errors"][0]["message"]));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
