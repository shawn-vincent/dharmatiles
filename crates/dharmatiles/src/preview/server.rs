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

use super::api::Body;
use super::fail;
use super::route::{Reply, Resume, route};
use super::routes::tile_shape;

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
        let (status, body) = fail::refused(
            "400 Bad Request",
            "the request could not be read: malformed, too large or cut short",
        );
        send_json(&mut conn, status, &body);
        return;
    };
    match answer(shared, &req) {
        Reply::Json(status, body) => send_json(&mut conn, status, &body),
        Reply::Bytes(b) => respond(&mut conn, "200 OK", "application/octet-stream", &b),
        Reply::Body(status, ctype, body) => respond(&mut conn, status, ctype, body.as_bytes()),
        Reply::Asset(ctype, body) => respond(&mut conn, "200 OK", ctype, &body),
        Reply::Events(from) => events(conn, shared, &from),
    }
}

/// Write a JSON reply.  A body that cannot be written is the program's failure: said in the log and to the page as
/// a 500 (and, were even that body unwritable, as plain text), never an empty body.
fn send_json(conn: &mut TcpStream, status: &'static str, body: &Body) {
    match body.wire() {
        Ok(text) => respond(conn, status, "application/json", text.as_bytes()),
        Err(e) => {
            tell!("error: preview: cannot write a {status} reply: {e}");
            let (status, body) =
                fail::failure_from("500 Internal Server Error", "cannot write the reply", &e);
            match body.wire() {
                Ok(text) => respond(conn, status, "application/json", text.as_bytes()),
                Err(e) => {
                    tell!("error: preview: cannot write the error reply either: {e}");
                    respond(
                        conn,
                        status,
                        "text/plain; charset=utf-8",
                        e.to_string().as_bytes(),
                    );
                }
            }
        }
    }
}

/// The reply to `req`: [`route`]'s, with a panic in a route turned into a
/// 500 that names what failed.  Every 5xx is logged to stderr, so no
/// internal failure is only in the page.
pub(super) fn answer(shared: &Shared, req: &Request) -> Reply {
    let route_name = format!("{} {}", req.method, req.path);
    let tile = tile_shape(&req.path).map(|(id, _)| id.to_owned());
    let reply = fail::answering(route_name, tile, || {
        match std::panic::catch_unwind(AssertUnwindSafe(|| route(shared, req))) {
            Ok(r) => r,
            Err(panic) => {
                let e = fail::ServerError::bug(panic.as_ref(), None);
                let (status, body) = e.reply();
                Reply::Json(status, body)
            }
        }
    });
    match &reply {
        Reply::Json(status, body) if status.starts_with('5') => {
            // the one error path: a failure of the program's own is a message in the stream too
            let id = tile_shape(&req.path).map(|(id, _)| id.to_owned());
            let name = id.as_deref().and_then(|i| shared.name_of(i));
            // every 5xx is built by the one error path, so its body is an `ErrorBody`: say it when it is not
            let error = body.failed().map_or_else(
                || {
                    fail::ServerError::failure(
                        "500 Internal Server Error",
                        format!("a {status} reply did not carry an error body"),
                    )
                    .detail()
                },
                |b| b.error.clone(),
            );
            shared.say(super::messages::of_server_error(
                &error,
                (id.as_deref().unwrap_or(""), name.as_deref().unwrap_or("")),
                dt_service::message::Origin::Server,
            ));
            tell!(
                "error: preview: {} {}: {status}: {}",
                req.method,
                req.path,
                error.message
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
    let stream = shared.subscribe(from);
    loop {
        let sent = match stream.recv_timeout(Duration::from_secs(10)) {
            Ok(msg) => conn.write_all(format!("data: {msg}\n\n").as_bytes()),
            Err(RecvTimeoutError::Timeout) => conn.write_all(b": keep-alive\n\n"),
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if sent.and_then(|()| conn.flush()).is_err() {
            return;
        }
    }
}
