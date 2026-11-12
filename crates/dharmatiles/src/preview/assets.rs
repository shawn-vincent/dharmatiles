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
        "/js/editor/app.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/app.js"),
    ),
    (
        "/js/editor/build.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/build.js"),
    ),
    (
        "/js/editor/chrome.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/chrome.js"),
    ),
    (
        "/js/editor/document.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/document.js"),
    ),
    (
        "/js/editor/export.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/export.js"),
    ),
    (
        "/js/editor/live.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/live.js"),
    ),
    (
        "/js/editor/map.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/map.js"),
    ),
    (
        "/js/editor/target.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/target.js"),
    ),
    (
        "/js/editor/places.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/places.js"),
    ),
    (
        "/js/editor/open.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/open.js"),
    ),
    (
        "/js/editor/selection.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/selection.js"),
    ),
    (
        "/js/editor/levels.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/levels.js"),
    ),
    (
        "/js/editor/region-sheet.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/region-sheet.js"),
    ),
    (
        "/js/editor/rows.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/rows.js"),
    ),
    (
        "/js/editor/sheet.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/sheet.js"),
    ),
    (
        "/js/editor/tiles.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/tiles.js"),
    ),
    (
        "/js/editor/stores.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/stores.js"),
    ),
    (
        "/js/editor/title.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/title.js"),
    ),
    (
        "/js/crumbs.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/crumbs.js"),
    ),
    (
        "/js/folder-list.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/folder-list.js"),
    ),
    (
        "/js/folders.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/folders.js"),
    ),
    (
        "/js/menu.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/menu.js"),
    ),
    (
        "/js/nav.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/nav.js"),
    ),
    (
        "/js/outline.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/outline.js"),
    ),
    (
        "/js/property.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/property.js"),
    ),
    (
        "/js/problems.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/problems.js"),
    ),
    (
        "/js/rows.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/rows.js"),
    ),
    (
        "/js/server-error.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/server-error.js"),
    ),
    (
        "/js/viewer.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer.js"),
    ),
    (
        "/js/viewer/camera.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer/camera.js"),
    ),
    (
        "/js/viewer/consts.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer/consts.js"),
    ),
    (
        "/js/viewer/fade.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer/fade.js"),
    ),
    (
        "/js/viewer/layers.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer/layers.js"),
    ),
    (
        "/js/view-presets.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view-presets.js"),
    ),
    (
        "/js/copy.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/copy.js"),
    ),
    (
        "/js/scroll-shadows.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/scroll-shadows.js"),
    ),
    (
        "/js/layout.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/layout.js"),
    ),
    (
        "/js/history.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/history.js"),
    ),
    (
        "/js/payload.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/payload.js"),
    ),
    (
        "/js/gallery.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/gallery.js"),
    ),
    (
        "/js/lens.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/lens.js"),
    ),
    (
        "/js/size-grid.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/size-grid.js"),
    ),
    (
        "/js/value.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/value.js"),
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
        "/js/cloud.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/cloud.js"),
    ),
    (
        "/js/fatline.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/fatline.js"),
    ),
    (
        "/js/debug-layer.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/debug-layer.js"),
    ),
    (
        "/js/overlay.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/overlay.js"),
    ),
    (
        "/js/tokens.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/tokens.js"),
    ),
    (
        "/js/drafts.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/drafts.js"),
    ),
    (
        "/js/spans.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/spans.js"),
    ),
    (
        "/js/mine.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/mine.js"),
    ),
    (
        "/js/messages.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/messages.js"),
    ),
    (
        "/js/editor/panel.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/panel.js"),
    ),
    (
        "/js/points.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/points.js"),
    ),
    (
        "/js/place.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/place.js"),
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
        "/vendor/codemirror.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/codemirror.js"),
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
    (
        "/icons/undo-2.svg",
        "image/svg+xml",
        include_bytes!("../../../../web/preview/icons/undo-2.svg"),
    ),
    (
        "/icons/redo-2.svg",
        "image/svg+xml",
        include_bytes!("../../../../web/preview/icons/redo-2.svg"),
    ),
    (
        "/vendor/LICENSE-lucide",
        "text/plain; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/LICENSE-lucide"),
    ),
];

/// The file served at `path`.
#[must_use]
pub fn find(path: &str) -> Option<(&'static str, &'static [u8])> {
    FILES.iter().find(|f| f.0 == path).map(|f| (f.1, f.2))
}
