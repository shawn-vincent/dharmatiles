//! The HTTP side: a thread per connection, plain `std::net`.
//!
//! Only `GET` and one `POST`; every response closes the connection.  The
//! `Host` header must name the loopback address, so a web page on another
//! site cannot reach the server through DNS rebinding.

use std::io::{BufRead, BufReader, Write};
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
    host: String,
}

fn read_request(conn: &TcpStream) -> Option<Request> {
    let mut r = BufReader::new(conn.try_clone().ok()?);
    let mut first = String::new();
    r.read_line(&mut first).ok()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?.to_owned();
    let target = parts.next()?;
    let path = target.split(['?', '#']).next()?.to_owned();
    let mut host = String::new();
    for _ in 0..64 {
        let mut h = String::new();
        if r.read_line(&mut h).ok()? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':')
            && k.eq_ignore_ascii_case("host")
        {
            v.trim().clone_into(&mut host);
        }
    }
    Some(Request { method, path, host })
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
    let _ = conn.write_all(head.as_bytes()).and_then(|()| conn.write_all(body));
}

fn json(conn: &mut TcpStream, status: &str, body: &serde_json::Value) {
    respond(conn, status, "application/json", body.to_string().as_bytes());
}

/// `/api/tiles/N/rest` → `(N, rest)`.
fn tile_route(path: &str) -> Option<(usize, &str)> {
    let rest = path.strip_prefix("/api/tiles/")?;
    let (n, what) = rest.split_once('/')?;
    Some((n.parse().ok()?, what))
}

fn one(mut conn: TcpStream, shared: &Shared) {
    let _ = conn.set_read_timeout(Some(Duration::from_secs(10)));
    let Some(req) = read_request(&conn) else { return };
    if !loopback(&req.host) {
        return respond(&mut conn, "403 Forbidden", "text/plain", b"loopback only");
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/api/events") => events(conn, shared),
        ("GET", "/api/tiles") => json(&mut conn, "200 OK", &shared.snapshot()),
        ("GET", p) if tile_route(p).is_some_and(|(_, w)| w == "mesh") => {
            let mesh = tile_route(p).and_then(|(i, _)| shared.mesh(i));
            match mesh {
                Some(m) => respond(&mut conn, "200 OK", "application/octet-stream", &m),
                None => respond(&mut conn, "404 Not Found", "text/plain", b"no mesh yet"),
            }
        }
        ("POST", p) if tile_route(p).is_some_and(|(_, w)| w == "build") => {
            let known = tile_route(p).is_some_and(|(i, _)| shared.want(i));
            let status = if known { "202 Accepted" } else { "404 Not Found" };
            json(&mut conn, status, &serde_json::json!({"ok": known}));
        }
        ("GET", p) => match assets::find(p) {
            Some((ctype, body)) => respond(&mut conn, "200 OK", ctype, body),
            None => respond(&mut conn, "404 Not Found", "text/plain", b"not found"),
        },
        _ => respond(&mut conn, "405 Method Not Allowed", "text/plain", b"GET only"),
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
