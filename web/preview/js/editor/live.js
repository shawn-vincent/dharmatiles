// Live updates: the event stream from the server, written into the server
// store and nowhere else.  Everything on the page that depends on the
// server's word subscribes to that store; this part knows no other.

import { setEnvironment, showError } from '../problems.js';
import { ServerError } from '../server-error.js';
import { host } from './app.js';
import { doc, server, tile } from './stores.js';

const withoutProgress = (t) => { const { progress, ...rest } = t; return rest; };
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
  host.connect({
    onState: (s) => {
      const live = s === 'live';
      server.set({ live, ever: server.get().ever || live });
    },
    onSnapshot: (m) => {
      m.tiles.forEach(reportFailure);
      server.set({
        tiles: m.tiles.map(withoutProgress), progress: progressOf(m.tiles), roots: m.roots || [],
        system: m.system, printer: m.printer || 'default', printers: m.printers || [], overhangDeg: m.overhang_deg, steepestDeg: m.steepest_deg ?? m.overhang_deg + 10,
        epoch: m.epoch, ready: true,
      });
    },
    onProgress: (id, p) => server.set({ progress: { ...server.get().progress, [id]: p } }),
    onTile: (t) => {
      reportFailure(t);
      const { tiles, progress } = server.get();
      const i = tiles.findIndex((x) => x.id === t.id);
      const next = tiles.slice();
      if (i >= 0) next[i] = withoutProgress(t); else next.push(withoutProgress(t));
      const { [t.id]: _old, ...rest } = progress;
      server.set({ tiles: next, progress: t.progress ? { ...rest, [t.id]: t.progress } : rest });
    },
  });
}
