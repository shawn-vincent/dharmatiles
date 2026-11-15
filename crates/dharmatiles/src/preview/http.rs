//! The HTTP text: reading a request and writing a response, nothing about
//! what a request means (that is [`super::route`]).  A request is parsed
//! from any reader, so routes can be tested without a socket.

use std::io::{BufRead, BufReader, Read, Write};

/// The request line and the headers: what a server reads before it decides whether to read the body.  The preview's
/// routes and the MCP server share this one reader ([`Head::read`]); neither parses HTTP itself.
pub(crate) struct Head {
    pub method: String,
    pub path: String,
    /// The query string's `key=value` pairs, percent-decoded.
    pub query: Vec<(String, String)>,
    pub host: String,
    /// `Origin`, when the browser sent one.
    pub origin: Option<String>,
    /// `X-DT-Token`.
    pub token: Option<String>,
    /// `Content-Type`, empty when none was sent.
    pub content_type: String,
    /// `Content-Length`, 0 when none was sent.
    pub length: usize,
}

/// Why a request could not be read.
#[derive(Debug)]
pub(crate) enum ReadError {
    /// The connection failed while reading `what`.
    Io {
        what: &'static str,
        error: std::io::Error,
    },
    /// The text is not a request.
    Malformed(String),
    /// The body is declared larger than [`MAX_BODY`].
    TooLarge(usize),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { what, error } => write!(f, "cannot read {what}: {error}"),
            Self::Malformed(why) => f.write_str(why),
            Self::TooLarge(len) => {
                write!(f, "the body is {len} bytes; the most read is {MAX_BODY}")
            }
        }
    }
}

impl Head {
    /// Read the line and the headers.  `Ok(None)` when the client connected and left without a word.
    pub(crate) fn read(r: &mut impl BufRead) -> Result<Option<Self>, ReadError> {
        let mut first = String::new();
        let got = r.read_line(&mut first).map_err(|error| ReadError::Io {
            what: "the request",
            error,
        })?;
        if got == 0 {
            return Ok(None);
        }
        let mut parts = first.split_whitespace();
        let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
            return Err(ReadError::Malformed(format!(
                "`{}` is not a request line",
                first.trim()
            )));
        };
        let method = method.to_owned();
        let target = target.split('#').next().unwrap_or(target);
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
        let mut head = Self {
            method,
            path,
            query,
            host: String::new(),
            origin: None,
            token: None,
            content_type: String::new(),
            length: 0,
        };
        for _ in 0..64 {
            let mut h = String::new();
            let got = r.read_line(&mut h).map_err(|error| ReadError::Io {
                what: "the request headers",
                error,
            })?;
            if got == 0 || h.trim().is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                let v = v.trim();
                match k.to_ascii_lowercase().as_str() {
                    "host" => v.clone_into(&mut head.host),
                    "origin" => head.origin = Some(v.to_owned()),
                    "x-dt-token" => head.token = Some(v.to_owned()),
                    "content-type" => v.clone_into(&mut head.content_type),
                    "content-length" => {
                        head.length = v.parse().map_err(|e| {
                            ReadError::Malformed(format!("bad Content-Length `{v}`: {e}"))
                        })?;
                    }
                    _ => {}
                }
            }
        }
        Ok(Some(head))
    }

    /// Read the body (`Content-Length` bytes, at most [`MAX_BODY`]), then the whole request.
    pub(crate) fn body(self, r: &mut impl Read) -> Result<Request, ReadError> {
        if self.length > MAX_BODY {
            return Err(ReadError::TooLarge(self.length));
        }
        let mut body = vec![0; self.length];
        r.read_exact(&mut body).map_err(|error| ReadError::Io {
            what: "the body (shorter than its Content-Length)",
            error,
        })?;
        Ok(Request {
            method: self.method,
            path: self.path,
            query: self.query,
            host: self.host,
            origin: self.origin,
            token: self.token,
            body,
        })
    }
}

pub(crate) struct Request {
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
pub(crate) const MAX_BODY: usize = 4 << 20;
/// The request line and headers, at most.
pub(crate) const MAX_HEAD: u64 = 16 << 10;

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
    /// for anything malformed, unreadable or too large: the connection is
    /// dropped, there is nobody to tell.
    #[expect(
        clippy::disallowed_methods,
        reason = "permitted silence: a malformed or unreadable request is dropped, None is the meaning (the MCP server, which answers refusals, reads Head itself)"
    )]
    pub fn parse(input: impl Read) -> Option<Self> {
        // bounded as a whole: a line with no end cannot grow without limit
        let mut r = BufReader::new(input.take(MAX_HEAD + MAX_BODY as u64));
        let head = Head::read(&mut r).ok()??;
        head.body(&mut r).ok()
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
pub(crate) fn loopback(host: &str) -> bool {
    let name = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or("")
    } else {
        host.rsplit_once(':').map_or(host, |(n, _)| n)
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

/// Is `origin` (an `Origin` header) a loopback page?  A browser sends one on
/// every cross-site `POST`; a program (an MCP client, `curl`) sends none.
pub(crate) fn loopback_origin(origin: &str) -> bool {
    origin.strip_prefix("http://").is_some_and(loopback)
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
