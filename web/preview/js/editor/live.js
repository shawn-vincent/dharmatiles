// Live updates: the event stream from the server, written into the server
// store and nowhere else.  Everything on the page that depends on the
// server's word subscribes to that store; this part knows no other.

import { setEnvironment, showError } from '../problems.js';
import { ServerError } from '../server-error.js';
import { setUnits } from '../value.js';
import { host } from './app.js';
import { doc, log, server, tile } from './stores.js';
import { applyDocState } from './docs.js';

const isList = (t) => /\.dmap$/i.test(t.path);
/**
 * A tile as the server store keeps it: without its progress (kept apart) and, for a print list or a map, without
 * its plan, which is the document's state: it goes to the document module (the one place that holds it, and
 * takes it only if it is newer than what it holds).
 */
const serverTile = (t) => {
  const { progress, plan, ...rest } = t;
  if (!isList(t)) return plan === undefined ? rest : { ...rest, plan };
  if (plan?.docRev != null) applyDocState({ id: t.id, docRev: plan.docRev, plan });
  return rest;
};
const progressOf = (tiles) => Object.fromEntries(tiles.filter((t) => t.progress).map((t) => [t.id, t.progress]));

// A build that failed with a bug or a failure the program reports
// ({kind, message, causes, backtrace, ...}, the tile's `failure`) goes to the
// error panel once, with the server's error as the cause of the page's.
const reported = new Set();
function reportFailure(t) {
  const f = t.failure;
  if (!f) return;
  const key = `${t.id}:${f.context?.at_ms}`;
  if (reported.has(key)) return;
  reported.add(key);
  showError(`The build of ${t.name} failed`, new Error(`The build of ${t.name} failed`, { cause: new ServerError(f) }));
}

/** The messages store is told when a tile's build begins and ends: what it shows of the last build follows. */
const building = new Set();
function followBuild(t) {
  const now = t.status === 'building';
  if (now && !building.has(t.id)) { building.add(t.id); log.beginBuild(t.id); }
  else if (!now && building.delete(t.id)) log.endBuild(t.id);
}

let lastRev = -1; // the newest revision the stream has told

/**
 * The program's list, taken: the stream's snapshot, or the one a call's answer carries (a folder made,
 * a tile moved), which is the list as it is when the call ends.  A call's answer older than what the
 * stream has already told is not taken: it would put the list back.
 */
export function applySnapshot(m, fromStream = false) {
  if (!fromStream && (m.epoch !== server.get().epoch || m.rev < lastRev)) return;
  lastRev = fromStream ? (m.rev ?? -1) : Math.max(lastRev, m.rev ?? -1);
  m.tiles.forEach(reportFailure);
  const ids = new Set(m.tiles.map((t) => t.id));
  for (const id of [...building]) if (!ids.has(id)) { building.delete(id); log.forget(id); }
  for (const t of log.all()) if (t.tile && !ids.has(t.tile)) log.forget(t.tile);
  m.tiles.forEach(followBuild);
  server.set({
    tiles: m.tiles.map(serverTile), progress: progressOf(m.tiles), roots: m.roots || [], folders: m.folders || [], home: m.home ?? null,
    system: m.system, printer: m.printer || 'default', printers: m.printers || [], overhangDeg: m.overhang_deg, steepestDeg: m.steepest_deg ?? m.overhang_deg + 10,
    inventory: m.inventory ?? null, epoch: m.epoch, ready: true,
  });
}

/** Wire this part of the page (once, at load). */
export function init() {
  // what a bug report should say about this run: the version is asked of the program once
  let version = null;
  host.about().then((a) => { version = a.version; }).catch((e) => showError('Could not read the version', e));
  setEnvironment(() => {
    const s = server.get(), t = tile();
    return { version, system: s.system, printer: s.printer, tile: t?.path ?? null, file: doc.get().file ?? t?.path ?? null };
  });
  host.elements().then((elements) => server.set({ elements })).catch((e) => showError('Could not load the elements', e));
  host.language().then((language) => { setUnits(language.units); server.set({ language }); }).catch((e) => showError('Could not load the language', e));
  host.connect({
    onState: (s) => {
      const live = s === 'live';
      server.set({ live, ever: server.get().ever || live });
    },
    onSnapshot: (m) => applySnapshot(m, true),
    onProgress: (id, p) => server.set({ progress: { ...server.get().progress, [id]: p } }),
    onMessage: (m) => log.apply(m),
    onTile: (t) => {
      reportFailure(t);
      followBuild(t);
      const { tiles, progress } = server.get();
      const i = tiles.findIndex((x) => x.id === t.id);
      const next = tiles.slice();
      if (i >= 0) next[i] = serverTile(t); else next.push(serverTile(t));
      const { [t.id]: _old, ...rest } = progress;
      server.set({ tiles: next, progress: t.progress ? { ...rest, [t.id]: t.progress } : rest });
    },
  });
}
