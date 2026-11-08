// The editor page (docs/design/editor.html): the tile menu, the text, the
// view and the property sheet, linked by one selection. Talks to the server only
// through `host` (js/host.js), so a desktop app can reuse this page and the
// viewer with a different host.
//
// The page's state is three stores (editor/stores.js): `server` (what the
// program says), `view` (what you are looking at) and `doc` (the text being
// edited).  Each part draws from them by subscribing; only live.js writes
// `server`.
//
// The selection is one shape, {kind, id, piece?}: a stack entry (id: its
// index in the plan's stack, the base last; piece: one piece of it, rock 2
// of a scatter), a region or a line of the tile (id: its name), or the
// whole tile.  Clicking a thing selects it; clicking it again climbs: piece,
// entry, the region it is in, tile.

import { init as initChrome } from './editor/chrome.js';
import { init as initBuild } from './editor/build.js';
import { init as initTiles } from './editor/tiles.js';
import { init as initOpen } from './editor/open.js';
import { init as initDocument } from './editor/document.js';
import { init as initSelection } from './editor/selection.js';
import { init as initSheet } from './editor/sheet.js';
import { init as initMap, tools } from './editor/map.js';
import { init as initExport } from './editor/export.js';
import { init as initLive } from './editor/live.js';
import { viewer } from './editor/app.js';
import { doc, server, view } from './editor/stores.js';

window.dtPreview = { server, view, doc, viewer, get tools() { return tools; } }; // for tests and the console

// each part wires itself in the order the page was written in: the frame, the
// build's camera, the tile menu, the view's tile, the text, the selection, the
// sheet, the map, export, then the event stream, which starts everything moving
initChrome();
initBuild();
initTiles();
initOpen();
initDocument();
initSelection();
initSheet();
initMap();
initExport();
initLive();
