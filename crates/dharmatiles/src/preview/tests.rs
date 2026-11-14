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

mod inventory;
mod messages_wire;
mod plates;

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

/// This run's token, as the page reads it.
fn session_token(r: &Running) -> String {
    let v: Value = serde_json::from_slice(&get(r, "/api/session").1).unwrap();
    v["token"].as_str().unwrap().to_owned()
}

/// `POST /api/tiles/ID/build` as the page makes it: with the token.
fn build_post(r: &Running, id: &str) -> String {
    post_raw(
        r,
        &format!("/api/tiles/{id}/build"),
        Some(&session_token(r)),
        None,
        "",
    )
    .0
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
        // the server's own word that nothing is queued or running, not a guess at how long a build takes
        wait_idle(r);
        let v = tiles(r);
        if ok(&v["tiles"][0]) {
            return v["tiles"][0].clone();
        }
        assert!(
            t0.elapsed() < Duration::from_secs(120),
            "timed out waiting for {what}: {v}"
        );
        // idle and not yet so: a tile the page has not chosen waits to be asked for, so look again shortly
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The URL of the first tile's mesh as the page asks for it: by the revision the tile's summary says it is of.
fn mesh_url(r: &Running) -> String {
    let t = tiles(r)["tiles"][0].clone();
    format!(
        "/api/tiles/{}/mesh?docRev={}",
        t["id"].as_str().unwrap(),
        t["docRev"].as_u64().unwrap_or(0)
    )
}

fn token_of(r: &Running) -> String {
    let t: Value = serde_json::from_slice(&get(r, "/api/session").1).unwrap();
    t["token"].as_str().unwrap().to_owned()
}

/// The page's post of the tiers it wants: the keys of the ones asked, in order.
fn want_tiers(r: &Running, token: &str, scope: &str, wants: &[Value]) -> Vec<String> {
    let body = serde_json::json!({"scope": scope, "wants": wants}).to_string();
    let (head, v) = post(r, "/api/tiers/want", Some(token), None, &body);
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v}");
    v["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_owned())
        .collect()
}

/// The result of a wanted tier once it is built (what the stream announces; the test polls): head and body.
fn wait_tier(r: &Running, key: &str, doc_rev: Option<u64>) -> (String, Vec<u8>) {
    let t0 = Instant::now();
    let url = match doc_rev {
        Some(n) => format!("/api/tiers/result?key={key}&docRev={n}"),
        None => format!("/api/tiers/result?key={key}"),
    };
    loop {
        let (head, body) = get(r, &url);
        if !head.starts_with("HTTP/1.1 404") {
            return (head, body);
        }
        assert!(
            t0.elapsed() < Duration::from_secs(120),
            "the tier was never built"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Wait until the server has no work left (queued, running or changed on disk and not yet picked up), for as long
/// as it takes: the way to know that nothing more will happen, in place of a sleep.
fn wait_idle(r: &Running) {
    let t0 = Instant::now();
    let mut quiet = 0;
    loop {
        let v: Value = serde_json::from_slice(&get(r, "/api/test-idle").1).unwrap();
        // idle twice running: a worker between taking a job and marking it is not idle
        quiet = if v["pending"] == 0 { quiet + 1 } else { 0 };
        if quiet >= 2 {
            return;
        }
        assert!(t0.elapsed() < Duration::from_secs(120), "never idle: {v}");
        std::thread::sleep(Duration::from_millis(20));
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
        "/js/view/icons.js",
    ] {
        assert!(get(&r, p).0.starts_with("HTTP/1.1 200"), "{p}");
    }
    assert!(get(&r, "/nope").0.starts_with("HTTP/1.1 404"));
}

#[test]
fn serves_the_docs_an_explain_link_opens() {
    let r = start(&[], false);
    let (head, body) = get(&r, "/docs/e/type.html");
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    assert!(head.contains("text/html"), "{head}");
    assert!(String::from_utf8_lossy(&body).contains(dt_lang::codes::TYPE));
    // the page's own style and the folder's index come from the same place
    assert!(get(&r, "/docs/assets/site.css").0.contains("text/css"));
    assert!(get(&r, "/docs/").0.starts_with("HTTP/1.1 200"));
    // the editor's scripts and libraries are where the site's pages import them from, the files the editor itself is served
    for p in [
        "/docs/editor/js/messages.js",
        "/docs/editor/vendor/three.module.min.js",
        "/docs/editor/vendor/OrbitControls.js",
    ] {
        let (head, body) = get(&r, p);
        assert!(
            head.starts_with("HTTP/1.1 200") && head.contains("javascript"),
            "{p}: {head}"
        );
        assert_eq!(
            body,
            get(&r, p.strip_prefix("/docs/editor").unwrap()).1,
            "{p}"
        );
    }
    // nothing outside the docs folder, however the path is spelt
    for p in [
        "/docs/editor/js/../../Cargo.toml",
        "/docs/editor/nope.js",
        "/docs/../Cargo.toml",
        "/docs/e/../../Cargo.toml",
        "/docs/%2e%2e/Cargo.toml",
        "/docs/e/nope.html",
    ] {
        assert!(get(&r, p).0.starts_with("HTTP/1.1 404"), "{p}");
    }
}

/// Every script the page can import is served: a module missing from the
/// table would only fail in the browser.
#[test]
fn every_page_script_is_served() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/preview");
    // A file left out is a 404, and one failed import blanks the whole editor.
    let mut dirs = vec![root.join("js")];
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "js") {
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
    assert!((t["plan"]["squareMm"].as_f64().unwrap() - 35.0).abs() < 1e-9);
    assert_eq!(t["record"]["audit"]["watertight"], true);
    assert_eq!(t["meshVersion"], 1);
    assert_eq!(t["docRev"], 0, "a library tile's document has no revision");
    let (head, body) = get(&r, &mesh_url(&r));
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
    assert_eq!(h["overhangDeg"], 55.0);
    assert!(
        get(&r, "/api/tiles/ffffffffffffffff/mesh?docRev=0")
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
    wait_idle(&r); // the worker has had its look and found nothing it is to build
    assert_eq!(tiles(&r)["tiles"][0]["status"], "idle");
    // the build call is a POST like the rest: no token, a wrong one or a foreign page is refused
    let path = format!("/api/tiles/{}/build", id0(&r));
    assert!(post_raw(&r, &path, None, None, "").0.contains("403"));
    assert!(
        post_raw(&r, &path, Some("wrong"), None, "")
            .0
            .contains("403")
    );
    let token = session_token(&r);
    assert!(
        post_raw(&r, &path, Some(&token), Some("http://evil.example"), "")
            .0
            .contains("403")
    );
    assert_eq!(
        tiles(&r)["tiles"][0]["status"],
        "idle",
        "a refused call built nothing"
    );
    assert!(build_post(&r, &id0(&r)).contains("202"));
    assert!(build_post(&r, "nope").contains("404"));
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
    wait_idle(&r); // the build is over, so the file's new time is a change the worker has not seen
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
    assert_eq!(t["meshVersion"], 1, "the last good mesh is kept");
    assert!(get(&r, &mesh_url(&r)).0.starts_with("HTTP/1.1 200"));

    // fixed: rebuilt in place, a new mesh version
    std::fs::write(&doc, text.replace("seed=1", "seed=2")).unwrap();
    let t = wait_for(&r, "rebuild", |t| {
        t["status"] == "ready" && t["meshVersion"] == 2
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
    let _ = Shared::new(&[]).snapshot_json();
}

#[test]
fn a_build_pushes_progress_events_that_never_go_back() {
    let r = start(&[FLAT], false);
    let mut c = TcpStream::connect(r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    assert!(build_post(&r, &id0(&r)).contains("202"));
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
                p["label"].is_string() && p["etaS"].as_f64().unwrap() >= 0.0,
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
        let v = s.snapshot_json();
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
    let snap = shared.snapshot_json();
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
        s.snapshot_json()["tiles"]
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
        serde_json::json!({ "op": "set", "text": text, "at": at, "name": "wear", "value": "worn" })
            .to_string();
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
    // save: only against the revision it was edited from
    let served: Value =
        serde_json::from_slice(&get(&r, &format!("/api/tiles/{}/text", id0(&r))).1).unwrap();
    let rev = served["docRev"].as_u64().unwrap();
    let s = serde_json::json!({ "text": edited, "docRev": rev + 1 }).to_string();
    let (h, v) = post(
        &r,
        &format!("/api/tiles/{}/save", id0(&r)),
        Some(&token),
        None,
        &s,
    );
    assert!(h.starts_with("HTTP/1.1 409"), "{h}");
    assert_eq!(v["text"], text);
    assert_eq!(v["docRev"], rev);
    let s = serde_json::json!({ "text": edited, "docRev": rev }).to_string();
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
    let s = serde_json::json!({ "text": "x", "docRev": 1, "file": "/etc/hosts" }).to_string();
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
    let s = serde_json::json!({ "text": "tile {}", "docRev": 0 }).to_string();
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
            "name", "unit", "numeric", "default", "default1", "absent", "required", "taken",
            "about", "words", "subs",
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
    // what each unit means, in the program's own words
    assert_eq!(
        lang["unitAbout"]["dmm"],
        "design millimetre: 1/35 of a square; 1 mm on DungeonBlocks, 0.73 mm on OpenLOCK"
    );
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
        tile["meshVersion"],
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
        get(&r, &format!("/api/tiles/{}/mesh?draft=1&docRev=0", id0(&r)))
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
                Some(&shared.token),
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
            &http::Request::build("GET", &format!("/api/tiles/{id}/mesh?docRev=0"), None, b"")
        ),
        Reply::Json("404 Not Found", _)
    ));
    assert!(matches!(
        route(
            &shared,
            &http::Request::build("POST", &format!("/api/tiles/{id}/check"), None, b"{}")
        ),
        Reply::Json("403 Forbidden", _)
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
fn a_length_prints_what_the_program_says_on_the_target_and_a_printed_size_is_written_in_its_own_unit()
 {
    let dir = std::env::temp_dir().join(format!("dt-printed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("t.dtile");
    let text = "dharmatiles version=1\ntile size=\"1x1\" {\n    override $groundHeight \"6dmm\"\n    stack {\n        dirt\n    }\n}\n";
    std::fs::write(&file, text).unwrap();
    let path = file.to_string_lossy().into_owned();
    let r = start(&[&path], false);
    let token: Value = serde_json::from_slice(&get(&r, "/api/session").1).unwrap();
    let token = token["token"].as_str().unwrap().to_owned();
    let call = |p: &str, body: Value| post(&r, p, Some(&token), None, &body.to_string());
    let ask = |items: Value| {
        let (h, v) = call(
            &format!("/api/tiles/{}/printed", id0(&r)),
            serde_json::json!({ "text": text, "items": items }),
        );
        assert!(h.starts_with("HTTP/1.1 200"), "{h} {v}");
        v["items"].clone()
    };
    let items = serde_json::json!([
        { "element": "rock", "control": "size", "typed": "7dmm" },
        { "element": "rock", "control": "size", "typed": "3.5dmm .. 6.5dmm" },
        { "element": "dirt", "control": "top", "typed": "$groundHeight + 4dmm" },
        { "element": "rock", "control": "size", "typed": "1sq" },
        { "element": "rock", "control": "count", "typed": "3" },
    ]);
    // DungeonBlocks: dmm prints as itself, so only the formula (the tile's own
    // height) and sq show
    let v = ask(items.clone());
    assert!(v[0].is_null() && v[1].is_null() && v[4].is_null(), "{v}");
    assert_eq!(v[2]["ends"], serde_json::json!(["10"]), "{v}");
    assert_eq!(v[2]["editable"], false);
    assert_eq!(v[3]["ends"], serde_json::json!(["35"]));
    // OpenLOCK: every dmm length shows
    let (h, _) = call("/api/target", serde_json::json!({ "system": "ol" }));
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    let v = ask(items);
    assert_eq!(v[0]["ends"], serde_json::json!(["5.08"]), "{v}");
    assert_eq!(v[0]["editable"], true);
    assert_eq!(v[1]["ends"], serde_json::json!(["2.54", "4.72"]), "{v}");
    assert_eq!(v[2]["ends"], serde_json::json!(["7.26"]), "{v}");
    assert_eq!(v[3]["ends"], serde_json::json!(["25.4"]));
    // typing a printed size writes the value's own unit; a formula refuses
    let (h, v) = call(
        "/api/printed-set",
        serde_json::json!({ "typed": "7dmm", "end": 0, "mm": 5.0 }),
    );
    assert!(h.starts_with("HTTP/1.1 200"), "{h} {v}");
    assert_eq!(v["typed"], "6.89dmm");
    let (h, _) = call(
        "/api/printed-set",
        serde_json::json!({ "typed": "$groundHeight + 4dmm", "end": 0, "mm": 5.0 }),
    );
    assert!(h.starts_with("HTTP/1.1 422"), "{h}");
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
    let text = "dharmatiles version=1\ntile size=\"1x1\" seed=3 seams=free {\n    line shore wobble=\"5dmm\" { through \"(0sq, 0.62sq)\" \"(1sq, 0.58sq)\" }\n    region pool { boundedBy shore; fill \"(0.5sq, 0.75sq)\" }\n    region bank { boundedBy shore; fill \"(0.5sq, 0.25sq)\" }\n    stack {\n        dirt at=bank\n        water\n    }\n}\n";
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
    assert_eq!(region["boundedBy"]["text"], "shore");
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
    let text = "dharmatiles version=1\ntile size=\"2x2\" seed=3 seams=free {\n    point knoll \"(0.5sq, 1.5sq)\"\n    line shore { through \"(0sq, 1.6sq)\" \"(2sq, 1.5sq)\" }\n    region crag { within \"[8dmm .. 14dmm]\" of=westEdge; within \"6dmm\" of=knoll }\n    region apron { within \"6.5dmm\" of=crag; except crag }\n    region pool { boundedBy shore; fill \"(0.5sq, 1.8sq)\" }\n    stack {\n        dirt\n    }\n}\n";
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
    assert!(
        clauses[0]["printed"].is_null(),
        "printed as written on DungeonBlocks: {crag}"
    );
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
    // OpenLOCK: the program says what each distance prints (the page writes no unit word of its own)
    call("/api/target", serde_json::json!({ "system": "ol" }));
    let v = call(
        &format!("/api/tiles/{}/check", id0(&r)),
        serde_json::json!({ "text": text }),
    );
    let crag = v["map"]["regions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "crag")
        .unwrap()
        .clone();
    let clauses = crag["sheet"]["clauses"].as_array().unwrap();
    assert_eq!(
        clauses[0]["printed"],
        serde_json::json!(["5.81", "10.16"]),
        "{crag}"
    );
    assert_eq!(clauses[1]["printed"], serde_json::json!(["4.35"]), "{crag}");
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
    assert!(build_post(&r, &id0(&r)).starts_with("HTTP/1.1 202"));
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
    let snap = shared.snapshot_json();
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
    let id = shared.snapshot_json()["tiles"].as_array().map(|t| t.len());
    assert_eq!(id, Some(0));
    {
        let mut tiles = lock(&shared.tiles);
        let mut t = Slot::new("lib:tests/ground/1x1-flat.dtile");
        t.status = State::Ready;
        tiles.push(t);
    }
    assert!(shared.set_target(None, Some("mine")).is_ok());
    let snap = shared.snapshot_json();
    assert_eq!(snap["printer"], "mine");
    assert_eq!(snap["system"], "db");
    // the gate's angles are the chosen printer's
    assert!((snap["overhangDeg"].as_f64().unwrap() - 50.0).abs() < 1e-9);
    assert!((snap["steepestDeg"].as_f64().unwrap() - 62.0).abs() < 1e-9);
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
    let body = serde_json::json!({ "text": "x", "docRev": 1 }).to_string();
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
        wait_idle(&r);
    }
    for (name, words) in [
        ("panic-in-build", "a test panic in the build"),
        ("panic-in-encode", "a test panic in the encoding"),
    ] {
        let t = tile(name);
        assert_eq!(t["status"], "failed", "{name}");
        let e = &t["errors"][0];
        assert_eq!(e["code"], dt_lang::codes::PANIC);
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
    let text = "dharmatiles version=1\ntile size=\"2x2\" seed=3 seams=free {\n    point knoll \"(0.5sq, 1.5sq)\"\n    line shore { through west \"(0sq, 1.5sq)\" knoll \"(1.5sq, 0.75sq)\" east }\n    line ring closed=#true { through \"(0.25sq, 1.75sq)\" \"(0.75sq, 1.75sq)\" \"(0.5sq, 1.25sq)\" }\n    region bay { boundedBy shore; fill \"(0.5sq, 1.75sq)\"; fill center; within \"4dmm\" of=\"(1.25sq, 1.75sq)\"; within \"6dmm\" of=knoll; within \"5dmm\" of=shore }\n    stack {\n        dirt\n    }\n}\n";
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

// ---- format 1 in the editor and the preview ----

const FORMAT_1: &str = "dharmatiles version=1\ntile size=\"2x2\" seed=3 seams=free {\n    point knoll \"(0.5sq, 0.5sq)\"\n    line shore { through west \"(0, 0.38sq)\" knoll \"(1.5sq, 1.25sq)\" east }\n    region bay { boundedBy shore; fill \"(0.5sq, 1.6sq)\" }\n    stack {\n        base\n        dirt\n        rock at=\"(1.5sq, 0.5sq)\" size=\"8dmm\"\n    }\n}\n";

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
    assert_eq!(v["errors"][0]["code"], dt_lang::codes::UNIT, "{v}");
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
        serde_json::json!({ "op": "set", "text": FORMAT_1, "at": at, "name": "size", "value": "9dmm" }),
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
        .find(|e| e["code"] == dt_lang::codes::FORMAT_0)
        .unwrap_or_else(|| panic!("{v}"));
    assert!(e["help"].as_str().unwrap().contains("v1.0.0"), "{e}");
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
    assert_eq!(n["code"], dt_lang::codes::STEEP);
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
        if v["type"] == "message" && v["message"]["code"] == dt_lang::codes::STEEP {
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
    // Report this defect: the program's own failures are reportable, with the one text as the body and no source for a
    // message about no tile; a mistake in a document is not
    assert_eq!(found[0]["reportable"], true);
    let (h, body) = get(
        &r,
        &format!(
            "/api/messages/report?seq={}&env=Environment%0A%20%20page%20x",
            found[0]["seq"]
        ),
    );
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    let rep: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        rep["url"]
            .as_str()
            .unwrap()
            .starts_with("https://github.com/shawn-vincent/dharmatiles/issues/new?title="),
        "{rep}"
    );
    assert_eq!(rep["labels"][0], "defect");
    assert_eq!(rep["truncated"], false);
    let (_, text) = get(
        &r,
        &format!("/api/messages?seq={}&format=text", found[0]["seq"]),
    );
    let text = String::from_utf8(text).unwrap();
    let body = rep["body"].as_str().unwrap();
    assert!(body.starts_with(text.trim_end()), "{body}");
    assert!(body.contains("page x"), "{body}");
    let (h, _) = get(&r, "/api/messages/report?seq=999999");
    assert!(h.starts_with("HTTP/1.1 404"), "{h}");
}

#[test]
fn a_mistake_in_the_document_has_no_report() {
    let (r, _, token) = editable("note-report", STEEP);
    check_text(&r, &token, STEEP);
    let found = messages(&r, &format!("tile={}&kind=note", id0(&r)));
    assert_eq!(found[0]["reportable"], false, "{found:?}");
    let (h, _) = get(&r, &format!("/api/messages/report?seq={}", found[0]["seq"]));
    assert!(h.starts_with("HTTP/1.1 400"), "{h}");
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
    assert_eq!(errs[0]["code"], dt_lang::codes::LOOK);
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

#[test]
fn an_entry_a_definition_call_makes_says_which_call_so_it_can_be_moved() {
    let text = "dharmatiles version=\"1.0.0\"\n\ndefine knoll {\n    stack {\n        dirt top=\"$groundHeight + 1.5dmm\" clods=none\n    }\n}\n\ntile size=\"1x1\" seed=6 {\n    stack {\n        base\n        knoll\n        rock at=\"(0.3sq, 0.4sq)\"\n    }\n}\n";
    let (r, _, token) = editable("call", text);
    let v = check_text(&r, &token, text);
    assert_eq!(v["ok"], true, "{v}");
    let stack = v["stack"].as_array().unwrap();
    let dirt = stack.iter().find(|s| s["what"] == "dirt").unwrap();
    let rock = stack.iter().find(|s| s["what"] == "rock").unwrap();
    let call = text.find("        knoll\n").unwrap() + 8;
    // the dirt is written in the definition; the document's own node for it is the call
    assert_eq!(
        dirt["call"]["span"][0].as_u64().unwrap(),
        call as u64,
        "{dirt}"
    );
    assert!(dirt["call"]["file"].as_str().unwrap().ends_with("t.dtile"));
    // an entry the document writes itself has no call
    assert!(rock["call"].is_null(), "{rock}");
    // and the call moves like any node
    let moved = post(
        &r,
        "/api/edit",
        Some(&token),
        None,
        &json!({ "text": text, "op": "move", "at": call, "before": null }).to_string(),
    )
    .1;
    assert!(
        moved["text"]
            .as_str()
            .unwrap()
            .contains("rock at=\"(0.3sq, 0.4sq)\"\n        knoll\n    }"),
        "{moved}"
    );
}

/// The cache of builds in the preview: a tile whose build is kept opens from
/// it with the same mesh and the same gate messages, and an older build (or
/// the library's stored preview) shows, said so, while a changed tile builds.
mod stored {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("dt-preview-store-{}-{name}", std::process::id()));
        if d.exists() {
            std::fs::remove_dir_all(&d).unwrap();
        }
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn shared_for(path: &str, store: &std::path::Path) -> Arc<Shared> {
        Arc::new(
            Shared::new(&[path.to_owned()]).with_store(Some(dt_store::Store::new(store, "test"))),
        )
    }

    fn rebuild(shared: &Arc<Shared>, path: &str) {
        let opts = Options {
            clouds: true,
            ..Options::default()
        };
        worker::rebuild(
            shared,
            &tile_id(path),
            Arc::new(AtomicBool::new(false)),
            &opts,
        );
    }

    fn texts(shared: &Shared, path: &str) -> Vec<String> {
        lock(&shared.events)
            .messages
            .query(Some(&tile_id(path)), 0, |_| true)
            .iter()
            .filter(|m| m.open)
            .map(|m| m.text.clone())
            .collect()
    }

    fn slot<T>(shared: &Shared, path: &str, f: impl Fn(&Slot) -> T) -> T {
        f(lock(&shared.tiles)
            .iter()
            .find(|t| t.id == tile_id(path))
            .unwrap())
    }

    #[test]
    fn a_kept_build_opens_without_building_and_says_the_same_things() {
        let dir = temp("hit");
        let first = shared_for(FLAT, &dir);
        rebuild(&first, FLAT);
        assert_eq!(slot(&first, FLAT, |t| t.status), State::Ready);
        assert_eq!(
            slot(&first, FLAT, |t| t.record.clone().unwrap()["cache"].clone()),
            "miss"
        );
        let mesh = slot(&first, FLAT, |t| t.mesh.clone().unwrap());
        let said = texts(&first, FLAT);
        assert!(said.iter().any(|t| t.starts_with("Built in")), "{said:?}");
        assert!(
            said.iter().any(|t| t == "Passes the printability gate"),
            "{said:?}"
        );

        // a new run (nothing in memory) with the same cache folder
        let again = shared_for(FLAT, &dir);
        rebuild(&again, FLAT);
        assert_eq!(slot(&again, FLAT, |t| t.status), State::Ready);
        assert_eq!(
            slot(&again, FLAT, |t| t.record.clone().unwrap()["cache"].clone()),
            "hit"
        );
        assert_eq!(
            slot(&again, FLAT, |t| t.mesh.clone().unwrap()),
            mesh,
            "the view is the same bytes"
        );
        let said = texts(&again, FLAT);
        assert!(
            said.iter().any(|t| t.starts_with("From the cache")),
            "{said:?}"
        );
        assert!(
            said.iter().any(|t| t == "Passes the printability gate"),
            "the gate's verdict flows the same: {said:?}"
        );
        assert!(!said.iter().any(|t| t.starts_with("Built in")), "{said:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_earlier_build_shows_while_a_changed_tile_builds_and_is_replaced() {
        let dir = temp("earlier");
        let tile = dir.join("t.dtile");
        let text = std::fs::read_to_string(dt_eval::source::library_file(FLAT).unwrap()).unwrap();
        std::fs::write(&tile, &text).unwrap();
        let path = tile.to_str().unwrap().to_owned();
        let cache = dir.join("cache");
        let first = shared_for(&path, &cache);
        rebuild(&first, &path);
        let old_mesh = slot(&first, &path, |t| t.mesh.clone().unwrap());

        // the tile changes; a fresh run has nothing in memory
        std::fs::write(&tile, text.replace("seed=1", "seed=2")).unwrap();
        let next = shared_for(&path, &cache);
        let rec = crate::watch::Recording::watching(&path);
        let tracked = dt_store::Tracked::new(&rec);
        let opts = Options::default();
        dt_eval::check(&tracked, &path, &opts).unwrap();
        worker::show_stored(
            &next,
            &tile_id(&path),
            next.store.as_deref().unwrap(),
            &tracked.reads(),
            &path,
            &opts,
        );
        assert_eq!(
            slot(&next, &path, |t| t.shown.clone().unwrap()["kind"].clone()),
            "earlier"
        );
        assert_eq!(slot(&next, &path, |t| t.mesh.clone().unwrap()), old_mesh);
        let said = texts(&next, &path);
        assert!(
            said.iter()
                .any(|t| t.starts_with("Showing the build made by test")),
            "{said:?}"
        );

        // the build lands: the view is the new tile's, and the note is gone
        rebuild(&next, &path);
        assert!(slot(&next, &path, |t| t.shown.is_none()));
        assert_eq!(slot(&next, &path, |t| t.status), State::Ready);
        assert_ne!(slot(&next, &path, |t| t.mesh.clone().unwrap()), old_mesh);
        let said = texts(&next, &path);
        assert!(!said.iter().any(|t| t.starts_with("Showing")), "{said:?}");
        assert!(said.iter().any(|t| t.starts_with("Built in")), "{said:?}");

        // a tile that then does not read leaves no stale view behind
        lock(&next.tiles)[0].shown = Some(json!({"kind": "earlier", "note": "x"}));
        std::fs::write(&tile, "tile {").unwrap();
        rebuild(&next, &path);
        assert_eq!(slot(&next, &path, |t| t.status), State::Failed);
        assert!(slot(&next, &path, |t| t.shown.is_none() && t.mesh.is_none()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stored_preview_shows_until_the_full_build_lands() {
        let dir = temp("preview");
        let store = dt_store::Store::new(&dir, "test");
        dt_store::build_tier(
            &dt_eval::Files,
            FLAT,
            &Options::default(),
            dt_store::Tier::SuperLow,
            &dt_eval::Progress::none(),
            None,
            Some(&store),
        )
        .unwrap();
        let shared = shared_for(FLAT, &dir);
        let tracked = dt_store::Tracked::new(&dt_eval::Files);
        let opts = Options::default();
        dt_eval::check(&tracked, FLAT, &opts).unwrap();
        worker::show_stored(
            &shared,
            &tile_id(FLAT),
            shared.store.as_deref().unwrap(),
            &tracked.reads(),
            FLAT,
            &opts,
        );
        assert_eq!(
            slot(&shared, FLAT, |t| t.shown.clone().unwrap()["kind"].clone()),
            "preview"
        );
        assert!(
            texts(&shared, FLAT)
                .iter()
                .any(|t| t.starts_with("Showing the stored super-low preview"))
        );
        // a low build on show has not been through the gate: the status line says so
        let shown = slot(&shared, FLAT, |t| t.shown.clone().unwrap());
        assert_eq!(shown["checked"], false);
        rebuild(&shared, FLAT);
        assert!(slot(&shared, FLAT, |t| t.shown.is_none()));
        assert_eq!(slot(&shared, FLAT, |t| t.status), State::Ready);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_damaged_entry_is_a_warning_in_the_messages_and_the_build_is_real() {
        let dir = temp("damaged");
        let first = shared_for(FLAT, &dir);
        rebuild(&first, FLAT);
        let store = dt_store::Store::new(&dir, "test");
        let entry = store.list().unwrap().remove(0).file;
        let mut bytes = std::fs::read(&entry).unwrap();
        let at = bytes.len() / 2;
        bytes[at] ^= 0xff;
        std::fs::write(&entry, bytes).unwrap();
        let again = shared_for(FLAT, &dir);
        rebuild(&again, FLAT);
        assert_eq!(slot(&again, FLAT, |t| t.status), State::Ready);
        let said = texts(&again, FLAT);
        assert!(
            said.iter()
                .any(|t| t.starts_with("The build cache:") && t.contains("damaged")),
            "{said:?}"
        );
        assert!(said.iter().any(|t| t.starts_with("Built in")), "{said:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

/// A document of the user's is written only by `dt_eval::atomic::write_atomic`
/// (a whole file or none).  A bare `std::fs::write` or `File::create` in the
/// preview server, or in the other writers of the user's files, fails here.
/// (A clippy `disallowed-methods` entry cannot be scoped to one module; it
/// would also bind the tests and every other crate.)
#[test]
fn atomic_only() {
    fn rs_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "tests") {
                    rs_files(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "rs")
                && p.file_name().is_some_and(|n| n != "tests.rs")
            {
                out.push(p);
            }
        }
    }
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&src.join("preview"), &mut files);
    files.push(src.join("slicers.rs"));
    files.push(src.join("library.rs"));
    files.push(src.join("../../dt-print/src/inventory.rs"));
    let mut bad = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        // the file's tests come last: only the program's own code is held to it
        let code = text.split("#[cfg(test)]").next().unwrap();
        for (n, line) in code.lines().enumerate() {
            let l = line.trim_start();
            if !l.starts_with("//") && (l.contains("fs::write(") || l.contains("File::create(")) {
                bad.push(format!("{}:{}: {l}", f.display(), n + 1));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "write a document with dt_eval::atomic::write_atomic:\n{}",
        bad.join("\n")
    );
}

/// A document of the user's (a tile, a map, a print list) is written by
/// `docs::Docs::edit` and nothing else: it takes the file's lock and gives
/// the write its revision.  Of the preview's own files only the settings
/// (`places.rs`) and the plates' exported files (`plates.rs`, new files, one
/// call) write otherwise.
#[test]
fn documents_are_written_only_through_the_document_registry() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/preview");
    let mut bad = Vec::new();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if !name.ends_with(".rs") || ["docs.rs", "places.rs", "tests.rs"].contains(&name.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap();
        let n = code.matches("write_atomic(").count();
        if n > usize::from(name == "plates.rs") {
            bad.push(format!("{name}: {n} calls of write_atomic"));
        }
    }
    assert!(
        bad.is_empty(),
        "write a document through Shared::docs.edit:\n{}",
        bad.join("\n")
    );
}

/// A step of the history is built as a picture: a wanted tier of a text, in place of the file's, kept by the text,
/// built on the tier lane (never the tile's own build) and the file left as it was.
#[test]
fn a_wanted_tier_can_be_of_a_text_other_than_the_files() {
    let dir = std::env::temp_dir().join(format!("dt-tiertext-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("t.dtile");
    let on_disk = dt_eval::Library.read(FLAT).unwrap();
    std::fs::write(&doc, &on_disk).unwrap();
    let path = doc.to_string_lossy().into_owned();
    let r = start(&[&path], true);
    wait_for(&r, "first build", |t| t["status"] == "ready");
    let (id, token) = (id0(&r), token_of(&r));
    let other = format!(
        "{on_disk}
// another text
"
    );
    let want = |text: Option<&str>, file: Option<&str>| {
        let mut w = serde_json::json!({"tile": id, "tier": "super-low"});
        if let Some(t) = text {
            w["text"] = t.into();
        }
        if let Some(f) = file {
            w["file"] = f.into();
        }
        w
    };
    // the file's own text and another text are two pictures with two keys
    let keys = want_tiers(
        &r,
        &token,
        "h",
        &[want(None, None), want(Some(&other), Some(&path))],
    );
    assert_ne!(keys[0], keys[1], "the text is part of the key");
    assert_eq!(
        wait_tier(&r, &keys[1], None).0.split(' ').nth(1),
        Some("200")
    );
    // a file that is not the tile's is refused, as a draft's is
    let body = serde_json::json!({"scope": "h", "wants": [want(Some(&other), Some("/etc/hosts"))]})
        .to_string();
    let (head, _) = post(&r, "/api/tiers/want", Some(&token), None, &body);
    assert!(head.starts_with("HTTP/1.1 403"), "{head}");
    // the document on disk is as it was
    assert_eq!(std::fs::read_to_string(&doc).unwrap(), on_disk);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_mesh_is_fetched_by_the_revision_of_the_document_and_another_is_refused() {
    let dir = std::env::temp_dir().join(format!("dt-meshrev-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("t.dtile");
    std::fs::write(&doc, dt_eval::Library.read(FLAT).unwrap()).unwrap();
    let path = doc.to_string_lossy().into_owned();
    let r = start(&[&path], true);
    let t = wait_for(&r, "first build", |t| t["status"] == "ready");
    let rev = t["docRev"].as_u64().unwrap();
    let id = id0(&r);
    // the revision the summary says: the mesh
    assert!(
        get(&r, &format!("/api/tiles/{id}/mesh?docRev={rev}"))
            .0
            .starts_with("HTTP/1.1 200")
    );
    // an older (or any other) one is refused with the revision the mesh is of, never served as if it were
    let (head, body) = get(&r, &format!("/api/tiles/{id}/mesh?docRev={}", rev + 7));
    assert!(head.starts_with("HTTP/1.1 409"), "{head}");
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["docRev"].as_u64(), Some(rev));
    // no revision asked for is no mesh
    assert!(
        get(&r, &format!("/api/tiles/{id}/mesh"))
            .0
            .starts_with("HTTP/1.1 400")
    );
    // the file changes: the next build is of the next revision, and the old number no longer fetches
    let text = std::fs::read_to_string(&doc).unwrap();
    std::fs::write(&doc, format!("{text}\n// changed\n")).unwrap();
    let t = wait_for(&r, "the rebuild", |t| {
        t["docRev"].as_u64().is_some_and(|n| n > rev) && t["status"] == "ready"
    });
    let now = t["docRev"].as_u64().unwrap();
    assert!(
        get(&r, &format!("/api/tiles/{id}/mesh?docRev={rev}"))
            .0
            .starts_with("HTTP/1.1 409")
    );
    assert!(
        get(&r, &format!("/api/tiles/{id}/mesh?docRev={now}"))
            .0
            .starts_with("HTTP/1.1 200")
    );
    // a tier is wanted, built on the lane and fetched by key and revision: the result of the revision the tile is at
    let token = token_of(&r);
    let keys = want_tiers(
        &r,
        &token,
        "t",
        &[serde_json::json!({"tile": id, "tier": "super-low"})],
    );
    let (head, body) = wait_tier(&r, &keys[0], None);
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    // and the payload says which revision it is of
    let jl = u32::from_le_bytes(body[4..8].try_into().unwrap()) as usize;
    let meta: Value = serde_json::from_slice(&body[8..8 + jl]).unwrap();
    assert_eq!(meta["docRev"].as_u64(), Some(now));
    assert!(
        get(
            &r,
            &format!("/api/tiers/result?key={}&docRev={now}", keys[0])
        )
        .0
        .starts_with("HTTP/1.1 200")
    );
    // a result of another revision is refused with the one it is of, never served as if it were
    let (head, body) = get(
        &r,
        &format!("/api/tiers/result?key={}&docRev={rev}", keys[0]),
    );
    assert!(head.starts_with("HTTP/1.1 409"), "{head}");
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["docRev"].as_u64(),
        Some(now)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A server with its lanes running and nothing listed, and its `Shared` for a test that talks to the lanes directly.
fn lanes() -> (Arc<Shared>, Arc<AtomicBool>) {
    let shared = Arc::new(Shared::with_roots(Vec::new(), &[], &Options::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let (s, st) = (Arc::clone(&shared), Arc::clone(&stop));
    std::thread::spawn(move || work(&s, false, &st));
    (shared, stop)
}

#[test]
fn a_request_for_a_newer_revision_cancels_the_older_one_on_a_lane_running_or_waiting() {
    use queue::{Interrupted, Lane};
    let (shared, stop) = lanes();
    let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
    let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
    // the export lane runs one at a time: a first job holds it until told, and sees its cancel token set
    let s1 = Arc::clone(&shared);
    let held = std::thread::spawn(move || {
        s1.on_lane(Lane::Export, "k".to_owned(), 1, "t", move |_, cancel| {
            started_tx.send(()).unwrap();
            go_rx.recv().unwrap();
            if cancel.load(Ordering::Relaxed) {
                Err(Interrupted::Superseded)
            } else {
                Ok("old")
            }
        })
    });
    started_rx.recv().unwrap();
    // a waiting job, then a newer revision of the same key that replaces it
    let s2 = Arc::clone(&shared);
    let waiting = std::thread::spawn(move || {
        s2.on_lane(Lane::Export, "j".to_owned(), 1, "t", |_, _| Ok("never"))
    });
    while !shared.pending_has_side_waiting() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let s3 = Arc::clone(&shared);
    let newer = std::thread::spawn(move || {
        s3.on_lane(Lane::Export, "j".to_owned(), 2, "t", |_, _| Ok("new"))
    });
    assert!(
        matches!(waiting.join().unwrap(), Err(Interrupted::Superseded)),
        "the waiting older one never ran"
    );
    // the running one is cancelled by a newer revision of its key
    let s4 = Arc::clone(&shared);
    let newest = std::thread::spawn(move || {
        s4.on_lane(Lane::Export, "k".to_owned(), 2, "t", |_, _| Ok("newest"))
    });
    std::thread::sleep(Duration::from_millis(100));
    go_tx.send(()).unwrap();
    assert!(
        matches!(held.join().unwrap(), Err(Interrupted::Superseded)),
        "the running older one saw its cancel"
    );
    assert!(matches!(newer.join().unwrap(), Ok("new")));
    assert!(matches!(newest.join().unwrap(), Ok("newest")));
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn a_job_that_panics_on_a_lane_is_raised_again_in_its_request() {
    use queue::{Interrupted, Lane};
    let (shared, stop) = lanes();
    let r = shared.on_lane::<()>(Lane::Tier, "p".to_owned(), 1, "t", |_, _| {
        panic!("a test panic on a lane")
    });
    assert!(matches!(r, Err(Interrupted::Panicked(_))));
    // and the lane goes on
    assert!(matches!(
        shared.on_lane(Lane::Tier, "q".to_owned(), 1, "t", |_, _| Ok(7)),
        Ok(7)
    ));
    stop.store(true, Ordering::Relaxed);
}

#[test]
fn a_maps_pieces_loading_is_one_job_of_its_document_said_as_progress_and_in_the_messages_and_ending_cleanly()
 {
    let r = start(&[FLAT], false);
    let id = id0(&r);
    let token = token_of(&r);
    let mut c = TcpStream::connect(r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    // four pieces wanted on behalf of the document, each at two tiers: seeds of one tile (eight meshes, four pieces)
    let wants: Vec<Value> = (1..=4)
        .flat_map(|s| {
            ["super-low", "low"].map(|t| serde_json::json!({"tile": id, "tier": t, "seed": s}))
        })
        .collect();
    let body = serde_json::json!({"scope": "map", "doc": id, "wants": wants}).to_string();
    let (head, v) = post(&r, "/api/tiers/want", Some(&token), None, &body);
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v}");
    let (mut counts, mut began, mut ended, mut cleared) = (Vec::new(), false, false, false);
    for l in BufReader::new(c).lines() {
        let l = l.unwrap();
        let Some(data) = l.strip_prefix("data: ") else {
            continue;
        };
        let v: Value = serde_json::from_str(data).unwrap();
        match v["type"].as_str() {
            Some("progress") if v["id"] == id => {
                let p = &v["progress"];
                assert_eq!(p["what"], "loading", "{p}");
                assert_eq!(p["key"], "pieces", "{p}");
                counts.push((
                    p["count"][1].as_u64().unwrap(),
                    p["count"][2].as_u64().unwrap(),
                ));
            }
            Some("message")
                if v["message"]["kind"] == "progress" && v["message"]["code"] == "pieces" =>
            {
                if v["message"]["state"] == "open" {
                    began = true;
                } else {
                    cleared = true;
                }
            }
            Some("tile")
                if v["tile"]["id"] == id
                    && v["tile"]["progress"].is_null()
                    && !counts.is_empty() =>
            {
                ended = true;
            }
            _ => {}
        }
        if ended && cleared {
            break;
        }
    }
    assert!(began && cleared, "the job's running message came and went");
    assert!(
        counts.iter().all(|(d, t)| d <= t && *t <= 4) && counts.last().map(|c| c.1) == Some(4),
        "done of total, ending on the four pieces asked for: {counts:?}"
    );
    assert!(
        counts.windows(2).all(|w| w[1].1 >= w[0].1),
        "the total only grows: {counts:?}"
    );
    assert!(
        counts.windows(2).all(|w| w[1].0 >= w[0].0),
        "pieces done never go back: {counts:?}"
    );
    assert!(
        counts.last().is_some_and(|c| c.0 >= 3),
        "ends with all but the last done (the last one ends the job): {counts:?}"
    );
}

#[test]
fn a_withdrawn_job_ends_its_row_cleanly() {
    let r = start(&[FLAT], false);
    let id = id0(&r);
    let token = token_of(&r);
    let many: Vec<Value> = (1..=30)
        .map(|s| serde_json::json!({"tile": id, "tier": "low", "seed": s}))
        .collect();
    let body = serde_json::json!({"scope": "map", "doc": id, "wants": many}).to_string();
    assert!(
        post(&r, "/api/tiers/want", Some(&token), None, &body)
            .0
            .starts_with("HTTP/1.1 200")
    );
    // the page lets go of everything: the job ends at once, whatever the lane was at
    let none = serde_json::json!({"scope": "map", "doc": id, "wants": []}).to_string();
    assert!(
        post(&r, "/api/tiers/want", Some(&token), None, &none)
            .0
            .starts_with("HTTP/1.1 200")
    );
    wait_idle(&r);
    let t = tiles(&r);
    assert!(
        t["tiles"][0]["progress"].is_null(),
        "no work in flight: {}",
        t["tiles"][0]["progress"]
    );
    let open: Vec<Value> = messages(&r, &format!("tile={id}&state=open&kind=progress"));
    assert!(open.is_empty(), "no running row is left: {open:?}");
}

#[test]
fn an_edit_is_answered_while_the_tier_lane_is_full_and_a_withdrawn_want_is_cancelled() {
    let r = start(&[FLAT], false);
    let id = id0(&r);
    let token = token_of(&r);
    // thirty pieces of one tile, each a seed of its own: far more than the lane (two at a time) can build at once
    let many: Vec<Value> = (1..=30)
        .map(|s| serde_json::json!({"tile": id, "tier": "low", "seed": s}))
        .collect();
    let t0 = Instant::now();
    let keys = want_tiers(&r, &token, "map", &many);
    assert!(
        t0.elapsed() < Duration::from_secs(2),
        "posting what is wanted waits for nothing"
    );
    assert_eq!(keys.len(), 30);
    // the lane is full: an edit is answered at once, not after the builds
    let t1 = Instant::now();
    let (head, v) = post(
        &r,
        "/api/names",
        Some(&token),
        None,
        &serde_json::json!({"text": "tile {}"}).to_string(),
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head} {v}");
    assert!(
        t1.elapsed() < Duration::from_secs(1),
        "the edit waited {:?} behind the builds",
        t1.elapsed()
    );
    // the page lets go of all but the first: the rest is withdrawn, so the lane has little left to do
    want_tiers(&r, &token, "map", &many[..1]);
    wait_idle(&r);
    let built = (1..=30)
        .filter(|s| {
            get(&r, &format!("/api/tiers/result?key={}", keys[*s - 1]))
                .0
                .starts_with("HTTP/1.1 200")
        })
        .count();
    assert!(
        built < 30,
        "{built} of 30 were built although all but one were withdrawn at once"
    );
    assert!(
        get(&r, &format!("/api/tiers/result?key={}", keys[0]))
            .0
            .starts_with("HTTP/1.1 200"),
        "what stayed wanted was built"
    );
}

const ROCKS_ONE: &str = "lib:tests/rocks/1x1-rock-tops.dtile";

/// The thumbnail service of the page (js/thumbs.js) asks the tier lane for a small build of a tile at a seed it
/// is given, without editing the file: two seeds are two pictures.
#[test]
fn a_library_tile_has_a_super_low_picture_at_each_seed_and_they_differ() {
    let r = start(&[ROCKS_ONE], false);
    let id = id0(&r);
    let token = token_of(&r);
    let keys = want_tiers(
        &r,
        &token,
        "thumbs",
        &[
            serde_json::json!({"tile": id, "tier": "super-low", "seed": 11}),
            serde_json::json!({"tile": id, "tier": "super-low", "seed": 12}),
        ],
    );
    assert_ne!(keys[0], keys[1], "a seed is part of the key");
    let (h0, a) = wait_tier(&r, &keys[0], None);
    let (h1, b) = wait_tier(&r, &keys[1], None);
    assert!(
        h0.starts_with("HTTP/1.1 200") && h1.starts_with("HTTP/1.1 200"),
        "{h0} {h1}"
    );
    assert!(!a.is_empty() && !b.is_empty());
    assert_ne!(a, b, "two seeds drew the same picture");
}

/// A want the program cannot make is refused by name, never built and never answered with a shrug.
#[test]
fn the_tiers_route_refuses_a_bad_want_by_name() {
    let r = start(&[ROCKS_ONE], false);
    let id = id0(&r);
    let token = token_of(&r);
    let ask = |want: Value| {
        let body = serde_json::json!({"scope": "thumbs", "wants": [want]}).to_string();
        post(&r, "/api/tiers/want", Some(&token), None, &body)
    };
    let (head, v) = ask(serde_json::json!({"tile": id, "tier": "huge"}));
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    assert!(
        v.to_string().contains("a tier is low, super-low or full"),
        "{v}"
    );
    let (head, v) = ask(serde_json::json!({"tile": id, "tier": "low", "system": "zz"}));
    assert!(head.starts_with("HTTP/1.1 400"), "{head}");
    assert!(v.to_string().contains("the system is db or ol"), "{v}");
    let (head, v) = ask(serde_json::json!({"tile": "nope", "tier": "low"}));
    assert!(head.starts_with("HTTP/1.1 404"), "{head}");
    assert!(v.to_string().contains("no such tile"), "{v}");
    // a field the contract does not have is refused, not ignored
    let (head, v) = ask(serde_json::json!({"tile": id, "tier": "low", "size": 64}));
    assert!(head.starts_with("HTTP/1.1 400"), "{head} {v}");
    assert!(v.to_string().contains("size"), "{v}");
}

/// A line that ends off a seam: `dt::seam`, twice.
const OFF_SEAM: &str = r#"dharmatiles version="1.0.0"

tile size="1x1" seed=97 {
    line shore wobble="5dmm" { through "(0sq, 0.62sq)" "(1sq, 0.58sq)" }
    region bank { boundedBy shore; fill "(0.5sq, 0.75sq)" }
    stack {
        dirt at=bank
    }
}
"#;

#[test]
fn a_listed_tile_with_a_document_mistake_reaches_every_picture_as_the_tiles_mistake_with_its_code()
{
    let dir = std::env::temp_dir().join(format!("dt-pic-mistake-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("first-plate.dtile");
    let broken = dir.join("broken.dtile");
    std::fs::write(&doc, dt_eval::Library.read(FLAT).unwrap()).unwrap();
    std::fs::write(&broken, OFF_SEAM).unwrap();
    let (dp, bp) = (
        doc.to_string_lossy().into_owned(),
        broken.to_string_lossy().into_owned(),
    );
    let r = start(&[&dp, &bp], true);
    wait_for(&r, "both listed", |_| true);
    let list = tiles(&r);
    let id_of = |name: &str| {
        list["tiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["path"].as_str().is_some_and(|p| p.ends_with(name)))
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let (doc_id, bad_id) = (id_of("first-plate.dtile"), id_of("broken.dtile"));
    let token = token_of(&r);
    let mut c = TcpStream::connect(r.addr).unwrap();
    write!(c, "GET /api/events HTTP/1.1\r\nHost: {}\r\n\r\n", r.addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    let body = serde_json::json!({"scope": "pics", "doc": doc_id, "wants": [
        {"tile": bad_id, "tier": "super-low"}
    ]})
    .to_string();
    let (head, _) = post(&r, "/api/tiers/want", Some(&token), None, &body);
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let mut seen = None;
    for l in BufReader::new(c).lines() {
        let l = l.unwrap();
        let Some(data) = l.strip_prefix("data: ") else {
            continue;
        };
        let v: Value = serde_json::from_str(data).unwrap();
        if v["type"] == "tier" && v["error"].is_string() {
            seen = Some(v);
            break;
        }
    }
    let ev = seen.expect("the tier's failure is announced");
    // the document's mistake, with its code: not the program's failure
    assert_eq!(ev["mistake"], true, "{ev}");
    assert_eq!(ev["code"], dt_lang::codes::SEAM, "{ev}");
    assert!(ev["error"].as_str().unwrap().contains("is not a seam"));
    // and the document that uses the picture is told as the tile's mistake, each with its place, file and help
    let said = messages(&r, &format!("tile={doc_id}&kind=error"));
    assert!(
        !said.is_empty(),
        "the mistake is a message of the document that wants the piece"
    );
    assert!(
        said.iter()
            .all(|m| m["code"] == dt_lang::codes::SEAM && m["kind"] == "error"),
        "{said:?}"
    );
    assert!(
        messages(&r, "kind=server")
            .iter()
            .all(|m| m["code"] != "tier" && m["code"] != "bug")
    );
    let first = &said[0];
    assert!(
        first["file"].as_str().unwrap().ends_with("broken.dtile"),
        "{first}"
    );
    assert!(
        first["doing"].as_str().unwrap().contains("super-low"),
        "{first}"
    );
    assert!(
        first["source"].as_str().unwrap().contains("through"),
        "{first}"
    );
    assert!(first["help"].as_str().unwrap().contains("`wsw`"));
    // one message, as the same text the whole list is copied in
    let (head, text) = get(
        &r,
        &format!("/api/messages?seq={}&format=text", first["seq"]),
    );
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let text = String::from_utf8(text).unwrap();
    for want in [
        dt_lang::codes::SEAM,
        "broken.dtile:4:",
        "file: ",
        "doing: ",
        "seen from: first-plate",
        "help: ",
        "  ^^^",
        "through",
        "kind: mistake",
    ] {
        assert!(text.contains(want), "{want} in\n{text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
