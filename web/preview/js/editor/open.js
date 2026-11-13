// @ts-check
// Opening a tile: choosing it, and keeping the 3D view on it: its plan, its
// mesh (or a draft in its place), its camera.  Follows the server store (the
// tile's builds) and the view store (which tile).

import { meshIndex, parsePayload, piecesByKey } from '../payload.js';
import { isOffline } from '../host.js';
import { showError } from '../problems.js';
import { host, viewer } from './app.js';
import { canLeave, supersedeDrafts } from './document.js';
import { levelOfParams } from '../folders.js';
import { parseHash } from '../nav.js';
import { toast } from './app.js';
import { docs, planOf } from './docs.js';
import { isEntry, isMapView, placeAt, server, stackIndex, stem, tile, tileById, view } from './stores.js';

// ---- choosing a tile ---------------------------------------------------

let at = null;             // the tile the view is set up for
let started = false;       // the first list of tiles has been looked at

/** Show tile `id` (null: none); false if unsaved changes were kept instead. */
export async function select(id, guarded = false) {
  const cur = view.get();
  if (!guarded && id !== cur.tileId && !(await canLeave())) return false;
  const t = tileById(id);
  const to = t ? t.id : null;
  view.set(to === cur.tileId ? {} : { tileId: to, sel: null, control: null, hov: null, source: null });
  return true;
}

/** The view starts afresh on tile `t` (or on none): what the viewer held goes with the description (editor/framing.js). */
function reset(t) {
  view.set({ shown: null, draft: null });
  // no tile: the folder level shows, and writes its own address (places.js)
  if (!t) return;
  history.replaceState(null, '', `#tile=${encodeURIComponent(t.path)}`);
}

/**
 * The first list: the tile in the address; else the folder level in the address; else the first of
 * the files named on the command line; else no tile, and Places (the start with no argument).
 */
function pickFirst() {
  const { tiles, roots } = server.get();
  const want = decodeURIComponent((location.hash.match(/tile=([^&]+)/) || [])[1] || '');
  const wanted = want ? tiles.find((t) => t.path === want) : null;
  if (wanted) return select(wanted.id, true);
  const level = levelOfParams(roots, parseHash(location.hash));
  if (level?.missing) toast('That place is not on the list now');
  else if (level) view.set({ placeAt: placeAt(level), pageAt: 'browse' });
  else {
    const files = roots.findIndex((r) => r.kind === 'files');
    const named = files >= 0 ? tiles.find((t) => t.root === files) : null;
    if (named) return select(named.id, true);
  }
  return select(null, true);
}

/** The tile shown, kept in step with what the server says of it. */
function follow() {
  const { ready, epoch } = server.get(), { tileId } = view.get();
  if (!ready) return;
  // the first tile shows the editor page (the page the sidebar starts on): `view.page` says so, so the keys that read it agree with what is on screen
  if (!started) { started = true; pickFirst().then(() => { view.set({ page: view.get().tileId != null ? 'edit' : 'browse' }); }); return; }
  const t = tile();
  // the tile shown is gone (trashed, moved away by another program): none is shown, and the panel is on its folder
  if (tileId != null && !t) { select(null, true); return; }
  if (tileId !== at) { at = tileId; reset(t); }
  if (!t) return;
  // a rescan or a reconnect: keep the view, catch up on a newer mesh
  const shown = view.get().shown;
  // a map draws its pieces, not the plates' mesh: that comes when the plates are shown
  if (!isMapView() && t.meshVersion != null && !(shown && shown.id === t.id && shown.epoch === epoch && shown.version === t.meshVersion)) loadMesh(t);
}

let loadToken = 0;
async function loadMesh(t) {
  const token = ++loadToken, { epoch } = server.get(), version = t.meshVersion, docRev = t.docRev ?? 0;
  try {
    const buf = await host.fetchMesh(t.id, docRev);
    // the program's mesh is of another revision (null), the page moved on, or the tile did: not shown
    if (!buf || token !== loadToken || t.id !== view.get().tileId || (tile()?.docRev ?? 0) !== docRev) return;
    const p = parsePayload(buf);
    viewer.setMesh(p); // the new mesh swaps in place: the viewer decides whether its camera moves
    supersedeDrafts();
    view.set({ pieces: piecesByKey(p), shown: { id: t.id, epoch, version }, draft: null });
    // a selection that neither the text nor the new build has goes
    const sel = view.get().sel;
    if (isEntry(sel) && stackIndex(sel.id) < 0 && meshIndex(p, sel.id) < 0) view.set({ sel: null });
  } catch (e) {
    if (!isOffline(e)) showError(`Could not show ${stem(t)}`, e); // offline: the chip says so, and the mesh is fetched again
  }
}

/** A draft of the text the page holds stands in for the build's mesh. */
function showDraft() {
  const d = view.get().draft;
  if (!d || d.tileId !== view.get().tileId) return;
  viewer.setMesh(d.payload);
  view.set({ pieces: piecesByKey(d.payload) });
}

/** Wire this part of the page (once, at load). */
export function init() {
  view.subscribe(['tileId', 'dmode'], follow);
  server.subscribe(['tiles', 'ready', 'epoch'], follow);
  view.subscribe(['draft'], showDraft);
}
