//! The HTTP text: reading a request and writing a response, nothing about
//! what a request means (that is [`super::route`]).  A request is parsed
//! from any reader, so routes can be tested without a socket.

use std::io::{BufRead, BufReader, Read, Write};

pub(super) struct Request {
    pub method: String,
    pub path: String,
    /// The query string's `key=value` pairs, percent-decoded.
    pub query: Vec<(String, String)>,
    pub host: String,
    /// `Origin`, when the browser sent one.
    pub origin: Option<String>,
    /// `X-DT-Token`.
    pub token: Option<String>,
    /// The body (`Content-Length` bytes, at most [`MAX_BODY`]).
    pub body: Vec<u8>,
}

/// The largest request body read: a document, not a mesh.
pub(super) const MAX_BODY: usize = 4 << 20;
/// The request line and headers, at most.
const MAX_HEAD: u64 = 16 << 10;

impl Request {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// A request as the server would read it from the loopback page,
    /// carrying `token` when given (the tests' way into [`super::route`]).
    #[cfg(test)]
    #[allow(clippy::expect_used)]
    pub fn build(method: &str, target: &str, token: Option<&str>, body: &[u8]) -> Self {
        let mut head = format!("{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:1\r\n");
        if let Some(t) = token {
            head.push_str(&format!("X-DT-Token: {t}\r\n"));
        }
        head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(body);
        Self::parse(&bytes[..]).expect("a well-formed request")
    }

    /// Read one request: the line, the headers, then the body.  `None`
    /// for anything malformed or too large.
    pub fn parse(input: impl Read) -> Option<Self> {
        // bounded as a whole: a line with no end cannot grow without limit
        let mut r = BufReader::new(input.take(MAX_HEAD + MAX_BODY as u64));
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
        Some(Self {
            method,
            path,
            query,
            host,
            origin,
            token,
            body,
        })
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

/// Is `host` (a `Host` header, with its port) the loopback address?
pub(super) fn loopback(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.rsplit_once(':').map_or(host, |(n, _)| n)
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

/// Write a whole response; the connection closes after it.
pub(super) fn respond(out: &mut impl Write, status: &str, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = out
        .write_all(head.as_bytes())
        .and_then(|()| out.write_all(body));
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_read_from_bytes() {
        let raw = b"POST /api/x?a=1&b=%2Fq+r HTTP/1.1\r\nHost: localhost:9\r\nX-DT-Token: t\r\nOrigin: http://localhost:9\r\nContent-Length: 2\r\n\r\n{}";
        let r = Request::parse(&raw[..]).unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str()), ("POST", "/api/x"));
        assert_eq!(r.param("b"), Some("/q r"));
        assert_eq!(r.token.as_deref(), Some("t"));
        assert_eq!(r.body, b"{}");
        assert!(loopback(&r.host));
        assert!(!loopback("127.0.0.1.evil.com:80"));
        assert!(Request::parse(&b"GET"[..]).is_none());
        assert!(
            Request::parse(&b"POST / HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n"[..]).is_none()
        );
    }
}
