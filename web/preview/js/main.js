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

import { installGlobalHandlers } from './problems.js';
import { init as initChrome } from './editor/chrome.js';
import { init as initBuild } from './editor/build.js';
import { init as initFraming } from './editor/framing.js';
import { init as initTiles } from './editor/tiles.js';
import { init as initOpen } from './editor/open.js';
import { init as initPlaces } from './editor/places.js';
import { init as initDocument } from './editor/document.js';
import { init as initSelection } from './editor/selection.js';
import { init as initSheet } from './editor/sheet.js';
import { init as initLevels } from './editor/levels.js';
import { init as initMap, tools } from './editor/map.js';
import { init as initExport } from './editor/export.js';
import { init as initPlates } from './editor/plates.js';
import { init as initScene } from './editor/scene.js';
import { init as initScenePanel } from './editor/scene-panel.js';
import { init as initAddress } from './editor/address.js';
import { init as initSceneDrag } from './editor/scene-drag.js';
import { init as initPlatesPin } from './editor/plates-pin.js';
import * as mapEdit from './editor/scene-edit.js';
import { onKey as sceneKey } from './editor/scene-edit.js';
import * as mapScene from './editor/scene.js';
import { init as initInventory } from './editor/inventory.js';
import { init as initTarget } from './editor/target.js';
import { init as initPanel } from './editor/panel.js';
import { init as initLive } from './editor/live.js';
import { init as initScrollShadows } from './scroll-shadows.js';
import { viewer } from './editor/app.js';
import { docs } from './editor/docs.js';
import { texts } from './editor/document.js';
import { doc, log, server, view } from './editor/stores.js';

installGlobalHandlers(); // a script error or an unhandled rejection lands in the error panel

window.dtPreview = { server, view, doc, viewer, log, texts, docs, mapEdit, mapScene, get tools() { return tools; } }; // for tests and the console

// each part wires itself in the order the page was written in: the frame, the
// build's camera, the tile menu, the view's tile, the text, the selection, the
// sheet, the map, export, then the event stream, which starts everything moving
initChrome();
initFraming();
initScrollShadows();
initBuild();
initTiles();
initOpen();
initDocument();
initSelection();
initSheet();
initLevels();
initPlaces();
initAddress();
initMap();
initExport();
initPlates();
initScene();
initScenePanel();
initSceneDrag();
initPlatesPin();
addEventListener('keydown', sceneKey, true);
initInventory();
initTarget();
initPanel();
initLive();
// everything is wired: from here on failures go to the problems panel, not the start-up banner
globalThis.dtStarted = true;
