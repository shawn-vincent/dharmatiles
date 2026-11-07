// The editor page (docs/design/editor.html): the tile menu, the text, the
// view and the property sheet, linked by one selection. Talks to the server only
// through `host` (js/host.js), so a desktop app can reuse this page and the
// viewer with a different host.
//
// The selection is a stack entry (an index into the plan's stack, the base
// last), optionally one piece of it (rock 2 of a scatter), a region or a
// line of the tile (by name), or the whole tile.  Clicking a thing selects
// it; clicking it again climbs: piece, entry, the region it is in, tile.

import { init as initChrome } from './editor/chrome.js';
import { init as initTiles } from './editor/tiles.js';
import { init as initOpen } from './editor/open.js';
import { init as initDocument } from './editor/document.js';
import { init as initSelection } from './editor/selection.js';
import { init as initSheet } from './editor/sheet.js';
import { init as initMap } from './editor/map.js';
import { init as initExport } from './editor/export.js';
import { init as initLive } from './editor/live.js';
import { viewer, state } from './editor/app.js';
import { tools } from './editor/map.js';

window.dtPreview = { state, viewer, get tools() { return tools; } }; // for tests and the console

// each part wires itself in the order the page was written in: the frame, the
// tile menu, the text, the selection, the sheet, the map, export, then the
// event stream, which starts everything moving
initChrome();
initTiles();
initOpen();
initDocument();
initSelection();
initSheet();
initMap();
initExport();
initLive();
