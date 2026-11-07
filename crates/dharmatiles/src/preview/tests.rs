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

fn start(paths: &[&str], eager: bool) -> Running {
    let paths: Vec<String> = paths.iter().map(|p| (*p).to_owned()).collect();
    let opts = Options::default();
    let shared = Arc::new(Shared::new(
        &paths,
        "db",
        opts.printer.overhang_deg,
        opts.printer.steepest_deg,
    ));
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    server::spawn(listener, Arc::clone(&shared));
    edit::drafts(Arc::clone(&shared), opts.clone());
    let stop = Arc::new(AtomicBool::new(false));
    let (s, st) = (Arc::clone(&shared), Arc::clone(&stop));
    std::thread::spawn(move || work(&s, &opts, eager, &st));
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

const FLAT: &str = "lib:tiles/test/1x1-flat.dtile";

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
    let (head, body) = get(&r, "/api/tiles/0/mesh");
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
    assert!(get(&r, "/api/tiles/7/mesh").0.starts_with("HTTP/1.1 404"));
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
        request(r.addr, "POST", "/api/tiles/0/build", &r.addr.to_string())
            .0
            .contains("202")
    );
    assert!(
        request(r.addr, "POST", "/api/tiles/9/build", &r.addr.to_string())
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
        .position(|l| l.contains("ground level=edge"))
        .expect("the flat tile writes `ground level=edge`")
        + 1;
    std::fs::write(&doc, text.replace("ground level=edge", "gruond level=edge")).unwrap();
    let t = wait_for(&r, "failure", |t| t["status"] == "failed");
    let e = &t["errors"][0];
    assert_eq!(e["file"], path.as_str());
    assert_eq!(e["line"].as_u64(), Some(line as u64), "{e}");
    assert_eq!(t["mesh_version"], 1, "the last good mesh is kept");
    assert!(get(&r, "/api/tiles/0/mesh").0.starts_with("HTTP/1.1 200"));

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
    let _ = Shared::new(&[], "db", 55.0, 65.0).snapshot();
}

#[test]
fn a_build_pushes_progress_events_that_never_go_back() {
    let r = start(&[FLAT], false);
    let mut c = TcpStream::connect(r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    assert!(
        request(r.addr, "POST", "/api/tiles/0/build", &r.addr.to_string())
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
    ]);
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
    // no paths: the personal home, then the library
    let (roots, files) = roots_for(&[]);
    assert!(files.is_empty());
    assert_eq!(roots[0].kind, RootKind::Dir(personal_home()));
    assert_eq!(roots[1].kind, RootKind::Library("tiles/".to_owned()));
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
        Root::new("Library", RootKind::Library("tiles/test/".to_owned())),
    ];
    let shared = Shared::with_roots(roots, &[], "db", 55.0, 65.0);
    let listed = |s: &Shared| -> Vec<(String, Value, Value)> {
        let v = s.snapshot();
        v["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["root"] == 0 && t["gone"] == false)
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
    assert_eq!(shared.pick(true), None, "folder tiles wait to be chosen");

    // a new tile joins; a deleted one leaves, and its index is not reused
    std::fs::write(dir.join("c.dtile"), &text).unwrap();
    std::fs::remove_file(dir.join("a.dtile")).unwrap();
    assert!(shared.rescan());
    let names: Vec<_> = listed(&shared).into_iter().map(|t| t.0).collect();
    assert_eq!(names, vec!["b", "c"]);
    let a = shared.snapshot()["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .position(|t| t["name"] == "a")
        .unwrap();
    assert!(!shared.want(a), "a deleted tile is not built");
    assert!(!shared.rescan(), "nothing changed");

    // and comes back when the file does
    std::fs::write(dir.join("a.dtile"), &text).unwrap();
    assert!(shared.rescan());
    assert_eq!(listed(&shared).len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serves_a_tiles_own_text_and_nothing_else() {
    let r = start(&[FLAT], true);
    wait_for(&r, "ready", |t| t["status"] == "ready");
    let (head, body) = get(&r, "/api/tiles/0/text");
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["file"], FLAT);
    assert!(v["text"].as_str().unwrap().contains("tile "));
    assert_eq!(v["writable"], false, "library files are never written");
    // only files the tile's build read
    let (head, _) = get(&r, "/api/tiles/0/text?file=%2Fetc%2Fpasswd");
    assert!(head.starts_with("HTTP/1.1 404"), "{head}");
    let (head, _) = get(&r, "/api/tiles/0/text?file=lib%3Aparts%2Fgrass.dtile");
    assert!(head.starts_with("HTTP/1.1 404"), "{head}");
}

fn post(
    r: &Running,
    path: &str,
    token: Option<&str>,
    origin: Option<&str>,
    body: &str,
) -> (String, Value) {
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
    let head = String::from_utf8_lossy(&all[..split]).into_owned();
    (
        head,
        serde_json::from_slice(&all[split + 4..]).unwrap_or(Value::Null),
    )
}

#[test]
fn the_editor_checks_edits_and_saves_only_with_the_token() {
    let dir = std::env::temp_dir().join(format!("dt-edit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "tile size=\"1x1\" {\n    stack {\n        dirt\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    assert_eq!(token.len(), 32);
    let body = serde_json::json!({ "text": text }).to_string();
    // no token, a wrong one, or a foreign page: refused
    assert!(
        post(&r, "/api/tiles/0/check", None, None, &body)
            .0
            .starts_with("HTTP/1.1 403")
    );
    assert!(
        post(&r, "/api/tiles/0/check", Some("nope"), None, &body)
            .0
            .starts_with("HTTP/1.1 403")
    );
    assert!(
        post(
            &r,
            "/api/tiles/0/check",
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
        "/api/tiles/0/check",
        Some(&token),
        Some(&format!("http://{}", r.addr)),
        &body,
    );
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    assert_eq!(v["ok"], true, "{v}");
    let at = v["stack"][0]["span"][0].as_u64().unwrap();
    assert_eq!(&text[usize::try_from(at).unwrap()..][..4], "dirt");
    let bad = serde_json::json!({ "text": "tile size=\"1x1\" { stack { durt } }" }).to_string();
    let (_, v) = post(&r, "/api/tiles/0/check", Some(&token), None, &bad);
    assert_eq!(v["ok"], false);
    assert!(v["errors"][0]["line"].is_number(), "{v}");
    // edit: a control written on the line, the rest kept
    let e =
        serde_json::json!({ "text": text, "at": at, "name": "wear", "value": "worn" }).to_string();
    let (h, v) = post(&r, "/api/edit", Some(&token), None, &e);
    assert!(h.starts_with("HTTP/1.1 200"), "{h} {v}");
    let edited = v["text"].as_str().unwrap().to_owned();
    assert_eq!(edited, text.replace("dirt\n", "dirt wear=worn\n"));
    // save: only over the text it was edited from
    let s = serde_json::json!({ "text": edited, "base": "something else" }).to_string();
    let (h, v) = post(&r, "/api/tiles/0/save", Some(&token), None, &s);
    assert!(h.starts_with("HTTP/1.1 409"), "{h}");
    assert_eq!(v["text"], text);
    let s = serde_json::json!({ "text": edited, "base": text }).to_string();
    assert!(
        post(&r, "/api/tiles/0/save", Some(&token), None, &s)
            .0
            .starts_with("HTTP/1.1 200")
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), edited);
    // only the tile's own files
    let s = serde_json::json!({ "text": "x", "base": "", "file": "/etc/hosts" }).to_string();
    assert!(
        post(&r, "/api/tiles/0/save", Some(&token), None, &s)
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
    let (h, _) = post(&r, "/api/tiles/0/save", Some(t), None, &s);
    assert!(h.starts_with("HTTP/1.1 403"), "{h}");
    assert_eq!(dt_eval::Files.read(FLAT).unwrap(), text);
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
fn builds_a_draft_from_text_without_writing_it() {
    let r = start(&[FLAT], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let t = token["token"].as_str().unwrap().to_owned();
    let text = dt_eval::Files.read(FLAT).unwrap();
    let body = serde_json::json!({ "text": text, "id": 7 }).to_string();
    let (h, _) = post(&r, "/api/tiles/0/draft", Some(&t), None, &body);
    assert!(h.starts_with("HTTP/1.1 202"), "{h}");
    let tile = wait_for(&r, "a draft", |t| t["draft_id"] == 7);
    assert_eq!(
        tile["mesh_version"],
        Value::Null,
        "a draft is not the tile's build"
    );
    let (head, body) = get(&r, "/api/tiles/0/mesh?draft=1");
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert_eq!(&body[..4], payload::MAGIC);
    assert_eq!(dt_eval::Files.read(FLAT).unwrap(), text);
}

#[test]
fn the_editor_places_and_takes_out_nodes_and_maps_the_tile() {
    let dir = std::env::temp_dir().join(format!("dt-place-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "tile size=\"1x1\" {\n    line shore { through west \"1,0.5\" }\n    region pool { bounded-by shore; fill \"0.5,0.2\" }\n    stack {\n        dirt\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let call = |path: &str, body: Value| post(&r, path, Some(&token), None, &body.to_string()).1;
    // a rock placed: the end of the stack, and where it starts
    let v = call(
        "/api/edit",
        serde_json::json!({ "text": text, "op": "insert", "place": "stack", "node": "rock at=\"0.3,0.6\"" }),
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
    let v = call("/api/tiles/0/check", serde_json::json!({ "text": text }));
    let map = &v["map"];
    let line = &map["lines"][0];
    assert_eq!(line["name"], "shore");
    let p = &line["points"][1];
    let s = usize::try_from(p["span"][0].as_u64().unwrap()).unwrap();
    assert_eq!(&text[s..][..7], "\"1,0.5\"");
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
    let text = "tile size=\"2x1\" seed=7 {\n    about \"A bank\"\n    levels foot=4\n    stack {\n        dirt\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let v = post(
        &r,
        "/api/tiles/0/check",
        Some(&token),
        None,
        &serde_json::json!({ "text": text }).to_string(),
    )
    .1;
    let own = &v["map"]["tile"];
    assert_eq!(own["at"], 0, "{v}");
    let seed = own["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "seed")
        .unwrap();
    let s = usize::try_from(seed["span"][0].as_u64().unwrap()).unwrap();
    assert_eq!(&text[s..=s], "7");
    assert_eq!(seed["text"], "7");
    // the description and the tile's own levels, where they are written
    let a = &own["about"]["value"];
    let s = usize::try_from(a["span"][0].as_u64().unwrap()).unwrap();
    assert_eq!(&text[s..s + 8], "\"A bank\"");
    assert_eq!(a["text"], "\"A bank\"");
    assert_eq!(own["levels"]["controls"][0]["name"], "foot");
    assert_eq!(own["levels"]["controls"][0]["text"], "4");
    let std = own["standard_levels"].as_array().unwrap();
    assert!(
        std.iter().any(|l| l["name"] == "edge" && l["value"] == 3.4),
        "{std:?}"
    );
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
        "tile size=\"1x1\" {\n    stack {\n        dirt\n    }\n}\n",
    )
    .unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let call = |p: &str, body: Value| post(&r, p, Some(&token), None, &body.to_string());
    // a system that is not one is refused
    let (h, _) = call(
        "/api/tiles/0/export",
        serde_json::json!({ "system": "lego" }),
    );
    assert!(h.starts_with("HTTP/1.1 400"), "{h}");
    // where it would go, without building
    let (_, v) = call("/api/tiles/0/export", serde_json::json!({ "where": true }));
    assert_eq!(v["out"], dir.join("out").to_string_lossy().as_ref(), "{v}");
    let (h, v) = call("/api/tiles/0/export", serde_json::json!({ "system": "ol" }));
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
    let mine = super::roots::personal_home().join("out");
    assert_eq!(edit::export_dir("lib:tiles/ground/1x1-grass.dtile"), mine);
    // the same tile named by its file inside the library folder
    if let Some(lib) = dt_eval::source::library_dir() {
        let file = lib.join("tiles/ground/1x1-grass.dtile");
        assert_eq!(edit::export_dir(&file.to_string_lossy()), mine);
    }
    // a file of yours: its project's out/
    let dir = std::env::temp_dir().join(format!("dt-where-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::write(dir.join("dharmatiles.dtile"), "name \"t\"\n").unwrap();
    assert_eq!(
        edit::export_dir(&dir.join("a/t.dtile").to_string_lossy()),
        dir.join("out")
    );
}

#[test]
fn lines_and_regions_carry_their_sheets_and_rename_with_their_references() {
    let dir = std::env::temp_dir().join(format!("dt-marks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "tile size=\"1x1\" seed=3 {\n    line shore wobble=5 { through \"0,0.38\" \"1,0.42\" }\n    region pool { bounded-by shore; fill \"0.5,0.25\" }\n    region bank { bounded-by shore; fill \"0.5,0.75\" }\n    stack {\n        dirt in=bank\n        water\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let call = |p: &str, body: Value| post(&r, p, Some(&token), None, &body.to_string()).1;
    let v = call("/api/tiles/0/check", serde_json::json!({ "text": text }));
    let line = &v["map"]["lines"][0]["sheet"];
    assert_eq!(line["controls"][0]["name"], "wobble", "{v}");
    assert_eq!(line["controls"][0]["text"], "5");
    assert_eq!(line["through"]["text"], "\"0,0.38\" \"1,0.42\"");
    let s = usize::try_from(line["through"]["span"][0].as_u64().unwrap()).unwrap();
    assert!(text[s..].starts_with("\"0,0.38\" \"1,0.42\" }"));
    let region = &v["map"]["regions"][1]["sheet"];
    assert_eq!(region["bounded_by"]["text"], "shore");
    assert_eq!(region["fill"]["text"], "\"0.5,0.75\"");
    let v = call(
        "/api/edit",
        serde_json::json!({ "op": "rename", "text": text, "mark": "region", "old": "bank", "new": "meadow" }),
    );
    let t = v["text"].as_str().unwrap();
    assert!(
        t.contains("region meadow") && t.contains("dirt in=meadow"),
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
        request(r.addr, "POST", "/api/tiles/0/build", &r.addr.to_string())
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
