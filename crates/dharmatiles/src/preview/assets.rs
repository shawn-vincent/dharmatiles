//! The viewer's files, inside the program (so `brew install` users get it
//! offline).  They live in `web/preview/` at the repository root, a plain
//! HTML and JS-module front end with no build step that the Tauri app can
//! load unchanged.

/// `(url path, content type, bytes)`.
pub const FILES: &[(&str, &str, &[u8])] = &[
    (
        "/",
        "text/html; charset=utf-8",
        include_bytes!("../../../../web/preview/index.html"),
    ),
    (
        "/logo.svg",
        "image/svg+xml",
        include_bytes!("../../../../web/preview/logo.svg"),
    ),
    (
        "/favicon.svg",
        "image/svg+xml",
        include_bytes!("../../../../web/preview/favicon.svg"),
    ),
    (
        "/style.css",
        "text/css; charset=utf-8",
        include_bytes!("../../../../web/preview/style.css"),
    ),
    (
        "/js/main.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/main.js"),
    ),
    (
        "/js/viewer.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer.js"),
    ),
    (
        "/js/payload.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/payload.js"),
    ),
    (
        "/js/host.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/host.js"),
    ),
    (
        "/vendor/three.module.min.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/three.module.min.js"),
    ),
    (
        "/vendor/OrbitControls.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/OrbitControls.js"),
    ),
    (
        "/vendor/LICENSE-three",
        "text/plain; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/LICENSE-three"),
    ),
];

/// The file served at `path`.
#[must_use]
pub fn find(path: &str) -> Option<(&'static str, &'static [u8])> {
    FILES.iter().find(|f| f.0 == path).map(|f| (f.1, f.2))
}
