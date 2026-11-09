//! The sockets: a thread per connection, plain `std::net`.  Every
//! response closes the connection, except the event stream.  What a request
//! means is [`super::route`]'s; the text of HTTP is [`super::http`]'s.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use super::Shared;
use super::http::{Request, respond};
use serde_json::json;

use super::route::{Reply, Resume, route};

/// Accept connections on `listener` in a background thread.
pub fn spawn(listener: TcpListener, shared: Arc<Shared>) {
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || one(conn, &shared));
        }
    });
}

fn one(mut conn: TcpStream, shared: &Shared) {
    if let Err(e) = conn.set_read_timeout(Some(Duration::from_secs(10))) {
        tell!("error: preview: cannot set a read timeout on a connection: {e}");
        return;
    }
    let Some(req) = Request::parse(&conn) else {
        // not a request this server can read (malformed, too large, or the
        // browser sent nothing): say so to whoever is there, and log it
        tell!("error: preview: an unreadable request");
        respond(
            &mut conn,
            "400 Bad Request",
            "application/json",
            json!({ "error": "the request could not be read: malformed, too large or cut short" })
                .to_string()
                .as_bytes(),
        );
        return;
    };
    match answer(shared, &req) {
        Reply::Json(status, body) => respond(
            &mut conn,
            status,
            "application/json",
            body.to_string().as_bytes(),
        ),
        Reply::Bytes(b) => respond(&mut conn, "200 OK", "application/octet-stream", &b),
        Reply::Text(status, body) => respond(&mut conn, status, "text/plain", body.as_bytes()),
        Reply::Asset(ctype, body) => respond(&mut conn, "200 OK", ctype, &body),
        Reply::Events(from) => events(conn, shared, &from),
    }
}

/// The reply to `req`: [`route`]'s, with a panic in a route turned into a
/// 500 that names what failed.  Every 5xx is logged to stderr, so no
/// internal failure is only in the page.
pub(super) fn answer(shared: &Shared, req: &Request) -> Reply {
    let reply = match std::panic::catch_unwind(AssertUnwindSafe(|| route(shared, req))) {
        Ok(r) => r,
        Err(panic) => {
            let msg = super::panic_message(panic.as_ref());
            Reply::Json(
                "500 Internal Server Error",
                json!({ "error": format!("{} {} panicked: {msg}", req.method, req.path) }),
            )
        }
    };
    match &reply {
        Reply::Json(status, body) if status.starts_with('5') => {
            tell!(
                "error: preview: {} {}: {status}: {}",
                req.method,
                req.path,
                body["error"].as_str().unwrap_or("no message")
            );
        }
        Reply::Text(status, body) if status.starts_with('5') => {
            tell!(
                "error: preview: {} {}: {status}: {body}",
                req.method,
                req.path
            );
        }
        _ => {}
    }
    reply
}

/// Server-sent events: where the page left off (or the snapshot), then
/// every change, until the browser goes away.
fn events(mut conn: TcpStream, shared: &Shared, from: &Resume) {
    let _ = conn.set_read_timeout(None);
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n";
    if conn.write_all(head.as_bytes()).is_err() {
        return;
    }
    let rx = shared.subscribe(from);
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
