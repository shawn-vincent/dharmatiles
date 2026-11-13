// @ts-check
// Live updates: the event stream from the server, written into the server
// store and nowhere else.  Everything on the page that depends on the
// server's word subscribes to that store; this part knows no other.

import { signal } from '@preact/signals-core';
import { tierReady, tiersReconnected } from '../tiers.js';
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
  const key = `${t.id}:${f.context?.atMs}`;
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

/** The newest revision of the program's stream the page has taken (a call's answer names the revision its change was told at). */
const streamRev = signal(-1);

/**
 * Resolves true once the stream has reached revision `rev`: the page then holds what an answer's change told.  False
 * after a few seconds (the stream is down or behind): the caller says so, loudly.
 */
export function reached(rev) {
  if (streamRev.peek() >= rev) return Promise.resolve(true);
  return new Promise((resolve) => {
    /** @type {() => void} */
    let off = () => {};
    const timer = setTimeout(() => { off(); resolve(false); }, 5000);
    off = streamRev.subscribe((v) => { if (v >= rev) { clearTimeout(timer); queueMicrotask(off); resolve(true); } });
  });
}

/**
 * The program's whole state, taken: the first event of a stream (a page that connects, or one whose stream cannot
 * resume).  Nothing else carries the whole: a call's answer says only the revision its change was told at (`reached`).
 */
export function applySnapshot(m) {
  m.tiles.forEach(reportFailure);
  const ids = new Set(m.tiles.map((t) => t.id));
  for (const id of [...building]) if (!ids.has(id)) { building.delete(id); log.forget(id); }
  for (const t of log.all()) if (t.tile && !ids.has(t.tile)) log.forget(t.tile);
  m.tiles.forEach(followBuild);
  server.set({
    tiles: m.tiles.map(serverTile),
    progress: progressOf(m.tiles),
    roots: m.roots || [], folders: m.folders || [], home: m.home ?? null,
    system: m.system, printer: m.printer || 'default', printers: m.printers || [],
    overhangDeg: m.overhangDeg, steepestDeg: m.steepestDeg ?? m.overhangDeg + 10,
    inventory: m.inventory ?? null, epoch: m.epoch, ready: true,
  });
}

/**
 * One resource the program says changed (crates/dharmatiles/src/preview/resources.rs): the places, the target, the
 * printers, the inventory or the list of tiles.  Each goes into its own part of the server store and the rest is left
 * as it is, so a new folder does not touch the inventory and an inventory change does not touch the tiles.
 */
/** @param {import('../api').StreamEvent & { type: 'resource' }} m */
export function applyResource(m) {
  switch (m.name) {
    case 'places': server.set({ roots: m.value.roots, folders: m.value.folders, home: m.value.home ?? null }); break;
    case 'target': server.set({ system: m.value.system, printer: m.value.printer, overhangDeg: m.value.overhangDeg, steepestDeg: m.value.steepestDeg }); break;
    case 'printers': server.set({ printers: m.value }); break;
    case 'inventory': server.set({ inventory: m.value }); break;
    case 'list': {
      // the tiles as they are, in the order the program says; a tile that joined comes with its state
      const have = new Map(server.get().tiles.map((t) => [t.id, t]));
      for (const t of m.value.added) { reportFailure(t); followBuild(t); have.set(t.id, serverTile(t)); }
      const ids = new Set(m.value.ids);
      for (const id of [...building]) if (!ids.has(id)) { building.delete(id); log.forget(id); }
      for (const t of log.all()) if (t.tile && !ids.has(t.tile)) log.forget(t.tile);
      const { progress } = server.get();
      server.set({ tiles: m.value.ids.map((id) => have.get(id)).filter(Boolean), progress: Object.fromEntries(Object.entries(progress).filter(([id]) => ids.has(id))) });
      break;
    }
    default: {
      // every resource the program can tell is handled above: one added in Rust and not here fails tsc on this line
      /** @type {never} */
      const unknown = m;
      throw new Error(`the program told a resource the page does not know: ${JSON.stringify(unknown)}`);
    }
  }
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
      if (live) tiersReconnected(); // what was announced while the stream was away is asked for again
    },
    onSnapshot: applySnapshot,
    onRev: (rev, fresh) => { streamRev.value = fresh ? rev : Math.max(streamRev.peek(), rev); },
    onProgress: (id, p) => server.set({ progress: { ...server.get().progress, [id]: p } }),
    onMessage: (m) => log.apply(m),
    onTier: tierReady,
    onResource: applyResource,
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
