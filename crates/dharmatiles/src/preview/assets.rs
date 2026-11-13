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
        "/js/doc-history.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/doc-history.js"),
    ),
    (
        "/js/doc-views.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/doc-views.js"),
    ),
    (
        "/js/documents.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/documents.js"),
    ),
    (
        "/js/editor/docs.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/docs.js"),
    ),
    (
        "/js/editor/mode.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/mode.js"),
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
        "/js/editor/framing.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/framing.js"),
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
        "/js/editor/inventory.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/inventory.js"),
    ),
    (
        "/js/editor/controls.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/controls.js"),
    ),
    (
        "/js/editor/lists.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/lists.js"),
    ),
    (
        "/js/inventory.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/inventory.js"),
    ),
    (
        "/js/printers.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/printers.js"),
    ),
    (
        "/js/editor/printer-chooser.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/printer-chooser.js"),
    ),
    (
        "/js/editor/plates.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/plates.js"),
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
        "/js/crumbs.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/crumbs.js"),
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
        "/js/map-list.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/map-list.js"),
    ),
    (
        "/js/map-model.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/map-model.js"),
    ),
    (
        "/js/editor/list-keep.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/list-keep.js"),
    ),
    (
        "/js/editor/address.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/address.js"),
    ),
    (
        "/js/editor/scene.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/scene.js"),
    ),
    (
        "/js/editor/scene-panel.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/scene-panel.js"),
    ),
    (
        "/js/editor/scene-edit.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/scene-edit.js"),
    ),
    (
        "/js/viewer/scene.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer/scene.js"),
    ),
    (
        "/js/editor/scene-drag.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/scene-drag.js"),
    ),
    (
        "/js/editor/scene-thumbs.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/scene-thumbs.js"),
    ),
    (
        "/js/viewer/backdrop.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer/backdrop.js"),
    ),
    (
        "/js/editor/plates-pin.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/plates-pin.js"),
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
        "/js/viewer/crossfade.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer/crossfade.js"),
    ),
    (
        "/js/viewer/fade.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer/fade.js"),
    ),
    (
        "/js/viewer/framing.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/viewer/framing.js"),
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
        "/js/slide.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/slide.js"),
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
        "/js/thumbs.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/thumbs.js"),
    ),
    (
        "/js/tiers.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/tiers.js"),
    ),
    (
        "/js/steps.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/steps.js"),
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
        "/js/hooks.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/hooks.js"),
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
        "/vendor/LICENSE-lucide",
        "text/plain; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/LICENSE-lucide"),
    ),
    (
        "/vendor/preact.mjs",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/preact.mjs"),
    ),
    (
        "/vendor/preact-hooks.mjs",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/preact-hooks.mjs"),
    ),
    (
        "/vendor/htm.mjs",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/htm.mjs"),
    ),
    (
        "/vendor/preact-signals.mjs",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/preact-signals.mjs"),
    ),
    (
        "/vendor/preact-signals-core.mjs",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/preact-signals-core.mjs"),
    ),
    (
        "/vendor/LICENSE-preact",
        "text/plain; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/LICENSE-preact"),
    ),
    (
        "/vendor/LICENSE-htm",
        "text/plain; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/LICENSE-htm"),
    ),
    (
        "/vendor/LICENSE-preact-signals",
        "text/plain; charset=utf-8",
        include_bytes!("../../../../web/preview/vendor/LICENSE-preact-signals"),
    ),
    (
        "/js/requests.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/requests.js"),
    ),
    (
        "/js/editor/model.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/editor/model.js"),
    ),
    (
        "/js/view/html.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/html.js"),
    ),
    (
        "/js/view/field.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/field.js"),
    ),
    (
        "/js/view/map-panel.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/map-panel.js"),
    ),
    (
        "/js/view/widgets.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/widgets.js"),
    ),
    (
        "/js/view/printer-chooser.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/printer-chooser.js"),
    ),
    (
        "/js/view/plates-panel.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/plates-panel.js"),
    ),
    (
        "/js/view/plates-panel.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/plates-panel.js"),
    ),
    (
        "/js/view/popup.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/popup.js"),
    ),
    (
        "/js/view/crumbs.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/crumbs.js"),
    ),
    (
        "/js/view/places-panel.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/places-panel.js"),
    ),
    (
        "/js/view/inventory-panel.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/inventory-panel.js"),
    ),
    (
        "/js/view/sheet.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/sheet.js"),
    ),
    (
        "/js/view/outline.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/outline.js"),
    ),
    (
        "/js/view/property.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/property.js"),
    ),
    (
        "/js/view/size-grid.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/size-grid.js"),
    ),
    (
        "/js/view/value-pop.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/value-pop.js"),
    ),
    (
        "/js/view/list-block.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/list-block.js"),
    ),
    (
        "/js/view/title-bar.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/title-bar.js"),
    ),
    (
        "/js/view/icons.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/icons.js"),
    ),
    (
        "/js/view/copy-button.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/copy-button.js"),
    ),
    (
        "/js/view/messages-panel.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/messages-panel.js"),
    ),
    (
        "/js/view/view-controls.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../../../web/preview/js/view/view-controls.js"),
    ),
];

/// The file served at `path`.
#[must_use]
pub fn find(path: &str) -> Option<(&'static str, &'static [u8])> {
    FILES.iter().find(|f| f.0 == path).map(|f| (f.1, f.2))
}
