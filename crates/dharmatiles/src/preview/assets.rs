//! The viewer's files, inside the program (so `brew install` users get it
//! offline).  They live in `web/preview/` at the repository root, a plain
//! HTML and JS-module front end with no build step that the Tauri app can
//! load unchanged.  The visual system (`theme.css`, its fonts and the
//! lotus) is the docs site's, from `docs/assets/`, so both look alike.

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
        include_bytes!("../../../../docs/assets/logo.svg"),
    ),
    (
        "/favicon.svg",
        "image/svg+xml",
        include_bytes!("../../../../web/preview/favicon.svg"),
    ),
    (
        "/theme.css",
        "text/css; charset=utf-8",
        include_bytes!("../../../../docs/assets/theme.css"),
    ),
    (
        "/fonts/big-shoulders-display.woff2",
        "font/woff2",
        include_bytes!("../../../../docs/assets/fonts/big-shoulders-display.woff2"),
    ),
    (
        "/fonts/plex-sans.woff2",
        "font/woff2",
        include_bytes!("../../../../docs/assets/fonts/plex-sans.woff2"),
    ),
    (
        "/fonts/plex-sans-italic.woff2",
        "font/woff2",
        include_bytes!("../../../../docs/assets/fonts/plex-sans-italic.woff2"),
    ),
    (
        "/fonts/jetbrains-mono.woff2",
        "font/woff2",
        include_bytes!("../../../../docs/assets/fonts/jetbrains-mono.woff2"),
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
        "/js/lens.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/lens.js"),
    ),
    (
        "/js/complete.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/complete.js"),
    ),
    (
        "/js/text.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/text.js"),
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
