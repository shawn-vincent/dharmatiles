//! The HTTP side: a thread per connection, plain `std::net`.
//!
//! Every response closes the connection.  The `Host` header must name the
//! loopback address, so a web page on another site cannot reach the server
//! through DNS rebinding.  A request that changes or reads a draft (check,
//! edit, save, make it mine) must also carry this run's token in
//! `X-DT-Token`, which only a page served from here can read
//! (`/api/session`), and an `Origin`, when sent, must be the loopback.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use super::{Shared, assets};

/// Accept connections on `listener` in a background thread.
pub fn spawn(listener: TcpListener, shared: Arc<Shared>) {
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || one(conn, &shared));
        }
    });
}

struct Request {
    method: String,
    path: String,
    /// The query string's `key=value` pairs, percent-decoded.
    query: Vec<(String, String)>,
    host: String,
    /// `Origin`, when the browser sent one.
    origin: Option<String>,
    /// `X-DT-Token`.
    token: Option<String>,
    /// The body (`Content-Length` bytes, at most [`MAX_BODY`]).
    body: Vec<u8>,
}

/// The largest request body read: a document, not a mesh.
const MAX_BODY: usize = 4 << 20;

impl Request {
    fn param(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// `%41` and `+` decoded; a bad escape is kept as written.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() && s.is_char_boundary(i + 3) => {
                match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn read_request(conn: &TcpStream) -> Option<Request> {
    let mut r = BufReader::new(conn.try_clone().ok()?);
    let mut first = String::new();
    r.read_line(&mut first).ok()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?.to_owned();
    let target = parts.next()?;
    let target = target.split('#').next()?;
    let (path, q) = target.split_once('?').unwrap_or((target, ""));
    let path = path.to_owned();
    let query = q
        .split('&')
        .filter(|x| !x.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (unescape(k), unescape(v))
        })
        .collect();
    let mut host = String::new();
    let (mut origin, mut token, mut length) = (None, None, 0usize);
    for _ in 0..64 {
        let mut h = String::new();
        if r.read_line(&mut h).ok()? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let v = v.trim();
            match k.to_ascii_lowercase().as_str() {
                "host" => v.clone_into(&mut host),
                "origin" => origin = Some(v.to_owned()),
                "x-dt-token" => token = Some(v.to_owned()),
                "content-length" => length = v.parse().ok()?,
                _ => {}
            }
        }
    }
    if length > MAX_BODY {
        return None;
    }
    let mut body = vec![0; length];
    r.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        query,
        host,
        origin,
        token,
        body,
    })
}

/// Is `host` (a `Host` header, with its port) the loopback address?
fn loopback(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.rsplit_once(':').map_or(host, |(n, _)| n)
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

fn respond(conn: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = conn
        .write_all(head.as_bytes())
        .and_then(|()| conn.write_all(body));
}

fn json(conn: &mut TcpStream, status: &str, body: &serde_json::Value) {
    respond(
        conn,
        status,
        "application/json",
        body.to_string().as_bytes(),
    );
}

/// `/api/tiles/N/rest` → `(N, rest)`.
/// With `DT_WEB` set to `web/preview`, the page's own files (only the
/// paths the program serves) are read from there on every request, so the
/// page can be worked on without rebuilding the program.
fn dev_asset(path: &str) -> Option<(&'static str, Vec<u8>)> {
    let dir = std::env::var_os("DT_WEB")?;
    let (ctype, _) = assets::find(path)?;
    let rel = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    if !(rel == "index.html" || rel == "style.css" || rel.starts_with("js/")) {
        return None;
    }
    std::fs::read(std::path::Path::new(&dir).join(rel))
        .ok()
        .map(|b| (ctype, b))
}

fn tile_route(path: &str) -> Option<(usize, &str)> {
    let rest = path.strip_prefix("/api/tiles/")?;
    let (n, what) = rest.split_once('/')?;
    Some((n.parse().ok()?, what))
}

fn one(mut conn: TcpStream, shared: &Shared) {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(10)));
    let Some(req) = read_request(&conn) else {
        return;
    };
    if !loopback(&req.host) {
        return respond(&mut conn, "403 Forbidden", "text/plain", b"loopback only");
    }
    // the editor's calls: this run's token, from a loopback page
    if req.path.starts_with("/api/") && req.method == "POST" && !req.path.ends_with("/build") {
        let origin_ok = req
            .origin
            .as_deref()
            .is_none_or(|o| o.strip_prefix("http://").is_some_and(loopback));
        if !origin_ok || req.token.as_deref() != Some(shared.token.as_str()) {
            return respond(&mut conn, "403 Forbidden", "text/plain", b"not this page");
        }
        let (status, v) = super::edit::post(shared, &req.path, &req.body);
        return json(&mut conn, status, &v);
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/events") => events(conn, shared),
        ("GET", "/api/session") => json(
            &mut conn,
            "200 OK",
            &serde_json::json!({ "token": shared.token }),
        ),
        ("GET", "/api/elements") => json(&mut conn, "200 OK", &crate::elements::all_json()),
        ("GET", "/api/tiles") => json(&mut conn, "200 OK", &shared.snapshot()),
        ("GET", p) if tile_route(p).is_some_and(|(_, w)| w == "mesh") => {
            let draft = req.param("draft").is_some();
            let mesh = tile_route(p).and_then(|(i, _)| {
                if draft {
                    shared.draft_mesh(i)
                } else {
                    shared.mesh(i)
                }
            });
            match mesh {
                Some(m) => respond(&mut conn, "200 OK", "application/octet-stream", &m),
                None => respond(&mut conn, "404 Not Found", "text/plain", b"no mesh yet"),
            }
        }
        ("GET", p) if tile_route(p).is_some_and(|(_, w)| w == "text") => {
            match tile_route(p).and_then(|(i, _)| shared.text(i, req.param("file"))) {
                Some(v) => json(&mut conn, "200 OK", &v),
                None => respond(&mut conn, "404 Not Found", "text/plain", b"no such file"),
            }
        }
        ("POST", p) if tile_route(p).is_some_and(|(_, w)| w == "build") => {
            let known = tile_route(p).is_some_and(|(i, _)| shared.want(i));
            let status = if known {
                "202 Accepted"
            } else {
                "404 Not Found"
            };
            json(&mut conn, status, &serde_json::json!({"ok": known}));
        }
        ("GET", p) if dev_asset(p).is_some() => {
            let (ctype, body) = dev_asset(p).unwrap_or_default();
            respond(&mut conn, "200 OK", ctype, &body);
        }
        ("GET", p) => match assets::find(p) {
            Some((ctype, body)) => respond(&mut conn, "200 OK", ctype, body),
            None => respond(&mut conn, "404 Not Found", "text/plain", b"not found"),
        },
        _ => respond(
            &mut conn,
            "405 Method Not Allowed",
            "text/plain",
            b"GET only",
        ),
    }
}

/// Server-sent events: the snapshot, then every change, until the browser
/// goes away.
fn events(mut conn: TcpStream, shared: &Shared) {
    let _ = conn.set_read_timeout(None);
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    if conn.write_all(head.as_bytes()).is_err() {
        return;
    }
    let rx = shared.subscribe();
    loop {
        let sent = match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(msg) => conn.write_all(format!("data: {msg}\n\n").as_bytes()),
            Err(RecvTimeoutError::Timeout) => conn.write_all(b": keep-alive\n\n"),
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if sent.and_then(|()| conn.flush()).is_err() {
            return;
        }
    }
}
