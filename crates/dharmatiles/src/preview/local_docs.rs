//! The docs, served from the folder they are installed in (or the checkout's `docs/`) under `/docs/`: what the
//! editor's Explain link opens, so an error's page matches the installed program and needs no network.  The files
//! are the site's own; nothing is generated here.

use std::borrow::Cow;
use std::path::{Component, Path};

use super::fail;
use super::route::Reply;

/// `GET /docs/PATH`: the file under the docs folder.  A folder is its `index.html`.
pub(super) fn serve(path: &str) -> Reply {
    let Some(rel) = path.strip_prefix("/docs/") else {
        return fail::refused("404 Not Found", "not found").into();
    };
    let rel = rel.split(['?', '#']).next().unwrap_or("");
    // the editor's own scripts and libraries, at the address the site's pages import them from (`<site>/editor/js`, `/editor/vendor`):
    // the files the program embeds for its editor, so the cards on a page run the same code and there is no second copy of it
    if let Some(file) = rel.strip_prefix("editor/")
        && (file.starts_with("js/") || file.starts_with("vendor/"))
        && !file.contains("..")
    {
        return match super::assets::find(&format!("/{file}")) {
            Some((ctype, body)) => Reply::Asset(ctype, Cow::Borrowed(body)),
            None => fail::refused("404 Not Found", "not found").into(),
        };
    }
    let Some(dir) = dt_eval::source::docs_dir() else {
        return fail::failure(
            "404 Not Found",
            "no docs folder is installed with this program (set $DT_DOCS to the checkout's docs/)",
        )
        .into();
    };
    // only plain relative paths: no `..`, no root, no escapes
    let plain = Path::new(rel)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
        && !rel.contains(['%', '\\']);
    if !plain {
        return fail::refused("404 Not Found", "not found").into();
    }
    let mut file = dir.join(rel);
    if rel.is_empty() || file.is_dir() {
        file = file.join("index.html");
    }
    match std::fs::read(&file) {
        Ok(body) => Reply::Asset(content_type(&file), Cow::Owned(body)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fail::refused("404 Not Found", "not found").into()
        }
        Err(e) => fail::failure(
            "500 Internal Server Error",
            format!("cannot read {}: {e}", file.display()),
        )
        .into(),
    }
}

fn content_type(file: &Path) -> &'static str {
    match file.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        // the engine module: the browser compiles it as it arrives only when it is told it is WebAssembly
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}
