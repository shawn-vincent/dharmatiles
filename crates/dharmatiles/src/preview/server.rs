//! The sockets: a thread per connection, plain `std::net`.  Every
//! response closes the connection, except the event stream.  What a request
//! means is [`super::route`]'s; the text of HTTP is [`super::http`]'s.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use super::Shared;
use super::http::{Request, respond};
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
    let _ = conn.set_read_timeout(Some(Duration::from_secs(10)));
    let Some(req) = Request::parse(&conn) else {
        return;
    };
    match route(shared, &req) {
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
