// Live updates: the event stream from the server, written into the server
// store and nowhere else.  Everything on the page that depends on the
// server's word subscribes to that store; this part knows no other.

import { showError } from '../problems.js';
import { host } from './app.js';
import { server } from './stores.js';

const withoutProgress = (t) => { const { progress, ...rest } = t; return rest; };
const progressOf = (tiles) => Object.fromEntries(tiles.filter((t) => t.progress).map((t) => [t.id, t.progress]));

/** Wire this part of the page (once, at load). */
export function init() {
  host.elements().then((elements) => server.set({ elements })).catch((e) => showError('Could not load the elements', e));
  host.connect({
    onState: (s) => {
      const live = s === 'live';
      server.set({ live, ever: server.get().ever || live });
    },
    onSnapshot: (m) => server.set({
      tiles: m.tiles.map(withoutProgress), progress: progressOf(m.tiles), roots: m.roots || [],
      system: m.system, printer: m.printer || 'default', printers: m.printers || [], overhangDeg: m.overhang_deg, steepestDeg: m.steepest_deg ?? m.overhang_deg + 10,
      epoch: m.epoch, ready: true,
    }),
    onProgress: (id, p) => server.set({ progress: { ...server.get().progress, [id]: p } }),
    onTile: (t) => {
      const { tiles, progress } = server.get();
      const i = tiles.findIndex((x) => x.id === t.id);
      const next = tiles.slice();
      if (i >= 0) next[i] = withoutProgress(t); else next.push(withoutProgress(t));
      const { [t.id]: _old, ...rest } = progress;
      server.set({ tiles: next, progress: t.progress ? { ...rest, [t.id]: t.progress } : rest });
    },
  });
}
