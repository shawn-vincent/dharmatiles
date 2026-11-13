//! What a request means: [`route`] turns a parsed [`Request`] into a
//! [`Reply`], with no socket in sight, so every call can be tested by
//! building a request and looking at the reply.  The only thing it cannot
//! hand back whole is the event stream, which is a [`Reply::Events`] for
//! `server` to run.
//!
//! The `Host` header must name the loopback address, so a web page on
//! another site cannot reach the server through DNS rebinding.  An
//! editor call (a `POST`) must also carry this run's token in
//! `X-DT-Token`, which only a page served from here can read
//! (`/api/session`), and an `Origin`, when sent, must be the loopback.

use std::borrow::Cow;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use super::api::ok;
use super::http::{Request, loopback};
use super::{Shared, assets, calls, fail};

/// `GET /api/session`: this run's token, which every `POST` must carry.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SessionRes {
    pub token: String,
}

/// `GET /api/about`: the program's version and the tile formats it reads.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AboutRes {
    pub version: String,
    pub formats: String,
}

/// `GET /api/tiles/ID/text`: a document's text for the text pane.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TextRes {
    pub file: String,
    pub text: String,
    pub writable: bool,
    /// The revision of the file the text is of; 0 for a library file.
    pub doc_rev: u64,
}

/// `POST /api/tiles/ID/build`: asked for.
#[derive(Serialize)]
pub struct Accepted {}

/// What to send back.
pub(super) enum Reply {
    Json(&'static str, Value),
    /// A mesh buffer (a tile's, a sketch's or a draft's), 200.
    Bytes(Arc<Vec<u8>>),
    /// A body made now, with its content type (the messages as text).
    Body(&'static str, &'static str, String),
    Asset(&'static str, Cow<'static, [u8]>),
    /// The event stream, resuming from where the page left off.
    Events(Resume),
}

/// Where a returning page's last stream ended: this run's epoch, and the
/// last revision it saw.
pub(super) struct Resume {
    pub epoch: Option<String>,
    pub since: Option<u64>,
}

impl From<(&'static str, Value)> for Reply {
    fn from((status, body): (&'static str, Value)) -> Self {
        Self::Json(status, body)
    }
}

const OK: &str = "200 OK";

/// `/api/tiles/ID/rest` → `(ID, rest)`.
pub(super) fn tile_route(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("/api/tiles/")?;
    let (id, what) = rest.split_once('/')?;
    (!id.is_empty()).then_some((id, what))
}

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
    // Developer aid only (`DT_WEB` is never set for a user): a file not
    // there is served from the copy in the program, which is what a user sees.
    std::fs::read(std::path::Path::new(&dir).join(rel))
        .ok()
        .map(|b| (ctype, b))
}

/// The reply to one request.
pub(super) fn route(shared: &Shared, req: &Request) -> Reply {
    if !loopback(&req.host) {
        return fail::refused(
            "403 Forbidden",
            "the editor answers the loopback address only",
        )
        .into();
    }
    let tile = tile_route(&req.path);
    // every POST: this run's token, from a loopback page (the build call too:
    // a page of any other site must not make the editor build)
    if req.path.starts_with("/api/") && req.method == "POST" {
        let origin_ok = req
            .origin
            .as_deref()
            .is_none_or(super::http::loopback_origin);
        if !origin_ok || req.token.as_deref() != Some(shared.token.as_str()) {
            return fail::refused(
                "403 Forbidden",
                "this call is not from the page this program served (the session token or the origin is wrong)",
            )
            .into();
        }
        if !matches!(tile, Some((_, "build"))) {
            return calls::dispatch(shared, &req.path, &req.body);
        }
    }
    let bytes = |b: Option<Arc<Vec<u8>>>, missing: &'static str| -> Reply {
        b.map_or_else(
            || fail::refused("404 Not Found", missing).into(),
            Reply::Bytes,
        )
    };
    match (req.method.as_str(), req.path.as_str(), tile) {
        ("GET", "/api/events", _) => Reply::Events(Resume {
            epoch: req.param("epoch").map(str::to_owned),
            since: req.param("since").and_then(|s| s.parse().ok()),
        }),
        #[cfg(any(test, feature = "test-routes"))]
        ("GET", "/api/test-panic", _) => panic!("a test panic in a route"),
        #[cfg(any(test, feature = "test-routes"))]
        ("GET", "/api/test-failure", _) => {
            let e = fail::Layered::demo();
            let (status, body) = fail::failure_from(
                "500 Internal Server Error",
                "cannot write the demo file",
                &e,
            );
            Reply::Json(status, body)
        }
        ("GET", "/api/messages", _) => {
            super::messages::read(shared, &|k| req.param(k).map(str::to_owned))
        }
        ("GET", "/api/session", _) => ok(&SessionRes {
            token: shared.token.clone(),
        })
        .into(),
        ("GET", "/api/about", _) => ok(&AboutRes {
            version: env!("DT_ENGINE_VERSION").to_owned(),
            formats: dt_doc::header::formats(),
        })
        .into(),
        ("GET", "/api/elements", _) => Reply::Json(OK, crate::elements::all_json()),
        ("GET", "/api/language", _) => Reply::Json(OK, crate::elements::language_json()),
        ("GET", "/api/weightings", _) => {
            let (status, body) = crate::weightings::answer(&|k| req.param(k).map(str::to_owned));
            Reply::Json(status, body)
        }
        ("GET", "/api/tiles", _) => ok(&shared.snapshot()).into(),
        ("GET", _, Some((id, "mesh"))) => bytes(shared.mesh(id), "no mesh yet"),
        // `?tier=low` or `?tier=super-low`: the tile's mesh at that tier, from the
        // cache or built, for a map that draws by distance.  Never checked.
        ("GET", _, Some((id, "tier"))) => shared.tier_mesh(id, &|k| req.param(k)),
        ("GET", _, Some((id, "sketch"))) => bytes(
            req.param("id")
                .and_then(|k| k.parse().ok())
                .and_then(|k| shared.sketch_mesh(id, k)),
            "no such sketch",
        ),
        ("GET", _, Some((id, "text"))) => match shared.text(id, req.param("file")) {
            Ok(Some(v)) => ok(&v).into(),
            Ok(None) => fail::refused("404 Not Found", "no such file").into(),
            Err(e) => {
                let (status, body) = fail::failure(
                    "500 Internal Server Error",
                    format!("cannot read the file: {e}"),
                );
                Reply::Json(status, body)
            }
        },
        ("POST", _, Some((id, "build"))) => {
            // `?debug=1` or `?debug=0`: the page has the debug layer on or off for this tile
            if let Some(on) = req.param("debug") {
                shared.set_marks(id, on == "1");
            }
            if shared.want(id) {
                Reply::Json("202 Accepted", super::api::ok(&Accepted {}).1)
            } else {
                fail::refused("404 Not Found", "no such tile").into()
            }
        }
        ("GET", p, _) => match dev_asset(p) {
            Some((ctype, body)) => Reply::Asset(ctype, Cow::Owned(body)),
            None => match assets::find(p) {
                Some((ctype, body)) => Reply::Asset(ctype, Cow::Borrowed(body)),
                None => fail::refused("404 Not Found", "not found").into(),
            },
        },
        _ => fail::refused("405 Method Not Allowed", "no such call").into(),
    }
}
