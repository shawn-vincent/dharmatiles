// The messages store: one stream of what happened (docs/standards/errors.html,
// crates/dharmatiles/src/message.rs).  The program sends every check, build,
// gate verdict, export and failure as a message on the event stream; this keeps
// them, and every surface (the status line, the messages panel, the marks in the
// source) draws from it and owns none.  Pure: no DOM here.
//
// A message is {seq, t, kind, severity, scope, code, tile, tile_name, node, span,
// text, help, data, state}; `state` is open until a later check or build of the
// same scope no longer reports it, then cleared.  Progress is not kept: the
// ticks are `progress` events, and the status line draws them.
//
// While a tile builds, the rows from the last build stay, dimmed and marked
// "from the last build".  When it finishes, what it no longer reports is
// cleared, shown as one quiet line ("1 cleared by this build"), and what is
// still true comes back to full strength.

import { ServerError } from './server-error.js';
import { detailsOf, tileErrorDetails } from './problems.js';

/** Is this a problem (something to fix or look at), rather than a record of what happened? */
export const isProblem = (m) => m.severity === 'error' || m.severity === 'warning' || m.severity === 'note';
/** The record of a build: its timing and the gate's verdict, shown at the top of the panel. */
export const isRecord = (m) => m.kind === 'build' || m.kind === 'gate';
const BUILD_KINDS = new Set(['build', 'gate', 'export', 'check', 'progress']);
const ORDER = { error: 0, warning: 1, note: 2, info: 3 };

/** The panel's filters: All, Errors, Notes, Build. */
export const FILTERS = [
  // the check's "read the tile" line is a record for the Build filter, not a row among the problems
  ['all', 'All', (m) => !(m.kind === 'check' && m.severity === 'info')],
  ['errors', 'Errors', (m) => m.severity === 'error'],
  ['notes', 'Notes', (m) => m.severity === 'note' || m.severity === 'warning'],
  ['build', 'Build', (m) => BUILD_KINDS.has(m.kind)],
];

/** `1 error · 2 notes`: the counts as the status line says them ('' when there is nothing). */
export function countsText({ errors, notes }) {
  const n = (k, one) => `${k} ${one}${k === 1 ? '' : 's'}`;
  return [errors && n(errors, 'error'), notes && n(notes, 'note')].filter(Boolean).join(' · ');
}

export function createLog() {
  const items = new Map();       // seq -> message
  const builds = new Map();      // tile id -> {startSeq}: the build under way, and the newest message before it
  const since = new Map();       // tile id -> messages cleared since the last build ended
  const last = new Map();        // tile id -> messages cleared by the last build that ended
  const dismissed = new Set();   // seqs the page put away (the program keeps them open)
  const listeners = new Set();
  let local = 0;
  const notify = () => { for (const fn of [...listeners]) fn(); };
  const maxSeq = () => { let n = 0; for (const k of items.keys()) n = Math.max(n, k); return n; };

  return {
    /** Take one message from the stream (the same message again replaces itself). */
    apply(m) {
      if (m.kind === 'progress') return; // not kept: the status line draws the ticks
      const old = items.get(m.seq);
      items.set(m.seq, m);
      // an open problem that is cleared now, seen from here (not one that was only replayed as cleared)
      if (m.state === 'cleared' && old?.state === 'open' && isProblem(m)) {
        since.set(m.tile, [...(since.get(m.tile) || []), m]);
      }
      notify();
    },
    /** A message of the page's own (the program could not be told): kept with a seq of its own, below zero. */
    addLocal(partial) {
      const m = {
        seq: --local, t: new Date().toISOString(), kind: 'server', severity: 'error', scope: 'server', code: 'failure',
        tile: '', tile_name: '', node: null, span: null, text: '', help: null, data: { origin: 'page' }, state: 'open', ...partial,
      };
      items.set(m.seq, m);
      notify();
      return m;
    },
    /** A build of `tile` begins: everything said so far is "from the last build". */
    beginBuild(tile) {
      if (builds.has(tile)) return;
      builds.set(tile, { startSeq: maxSeq() });
      notify();
    },
    /** The build of `tile` ended: what it cleared is kept to say so, and the rest is full strength again. */
    endBuild(tile) {
      if (!builds.delete(tile)) return;
      last.set(tile, since.get(tile) || []);
      since.delete(tile);
      notify();
    },
    dismiss(seq) { dismissed.add(seq); notify(); },
    /** A tile that left the list takes its messages with it. */
    forget(tile) {
      for (const [k, m] of [...items]) if (m.tile === tile) items.delete(k);
      builds.delete(tile); since.delete(tile); last.delete(tile);
      notify();
    },
    /** Tell `fn` after every change; returns the way to stop. */
    subscribe(fn) { listeners.add(fn); return () => listeners.delete(fn); },
    byId: (seq) => items.get(seq) ?? null,
    all: () => [...items.values()],

    /**
     * What the surfaces draw for `tile` ('' for the program's own, which belong to no tile):
     *   rows     the open messages that are not the build's record, errors first then newest first,
     *            each {m, dim}: dim while the tile builds and the message is from the last build
     *   record   {build, gate}: the last build's timing and verdict (open), or null
     *   cleared  the messages the last build cleared (shown as one line while no build runs)
     *   counts   {errors, notes} of the open problems (the gate's failure counts as an error)
     *   running  a build is under way
     */
    view(tile) {
      const run = builds.get(tile);
      const mine = [...items.values()]
        .filter((m) => (m.tile === tile || m.tile === '') && m.state === 'open' && !dismissed.has(m.seq));
      const dim = (m) => !!run && m.seq > 0 && m.seq <= run.startSeq;
      const rows = mine.filter((m) => !isRecord(m)).sort((a, b) => (ORDER[a.severity] - ORDER[b.severity]) || b.seq - a.seq).map((m) => ({ m, dim: dim(m) }));
      const newest = (kind) => mine.filter((m) => m.kind === kind).sort((a, b) => b.seq - a.seq)[0] ?? null;
      const build = newest('build'), gate = newest('gate');
      const record = build || gate ? { build, gate, dim: !!run } : null;
      const problems = mine.filter(isProblem);
      return {
        rows, record, running: !!run,
        cleared: run ? [] : last.get(tile) || [],
        counts: {
          errors: problems.filter((m) => m.severity === 'error').length,
          notes: problems.filter((m) => m.severity !== 'error').length,
        },
      };
    },
    /** The open messages of `tile` that point at stack entries, by entry key (the badges on a stack row read this). */
    byNode(tile) {
      const out = new Map();
      for (const m of items.values()) {
        if (m.tile !== tile || m.state !== 'open' || !m.node || dismissed.has(m.seq)) continue;
        out.set(m.node, [...(out.get(m.node) || []), m]);
      }
      return out;
    },
    /** The open messages of `tile` with a span in `file`: what the source marks in its gutter. */
    inFile(tile, file) {
      return [...items.values()].filter((m) => m.tile === tile && m.state === 'open' && m.span?.file === file && !dismissed.has(m.seq));
    },
  };
}

/** The rows `filter` ('all', 'errors', 'notes', 'build') keeps. */
export function filterRows(rows, filter) {
  const keep = FILTERS.find((f) => f[0] === filter)?.[2] ?? (() => true);
  return rows.filter((r) => keep(r.m));
}

/**
 * The status line: {verdict, tone, counts, progress}.  `verdict` is the one word that matters most,
 * worst first: building (with its percent and stage; "rebuilding" while the view is a build the
 * cache kept, `t.shown`), offline, an error, fails the gate, no build, passes (with the build's
 * time, or "cached" when the cache gave it).  `t` is the tile's state from the program (status,
 * record, shown); `p` its progress, if building.
 */
export function statusOf(view, { t = null, live = true, p = null } = {}) {
  const counts = countsText(view.counts);
  if (!live) return { verdict: 'reconnecting…', tone: 'busy', counts, progress: null };
  if (!t) return { verdict: 'no tile', tone: 'idle', counts, progress: null };
  if (t.status === 'building' || view.running) {
    const pct = p ? Math.floor(p.fraction * 100) : 0;
    const stage = p ? `${pct}% · ${p.label}` : 'starting';
    const what = t.shown ? 'rebuilding' : 'building';
    // a low build on show (nothing earlier of the tile was kept) has not been through the gate
    const low = t.shown?.checked === false ? ' · low · not yet checked' : '';
    return { verdict: `${what} ${stage}${low}`, tone: 'busy', counts, progress: p ? p.fraction : 0 };
  }
  if (t.status === 'idle') return { verdict: 'not built yet', tone: 'idle', counts, progress: null };
  const fails = view.record?.gate?.severity === 'error';
  if (t.status === 'failed') return { verdict: view.counts.errors ? 'no build' : 'build failed', tone: 'bad', counts, progress: null };
  if (fails) return { verdict: 'fails the gate', tone: 'bad', counts, progress: null };
  const data = view.record?.build?.data;
  const secs = data?.build_s;
  const warn = view.counts.errors > 0;
  const took = data?.cache === 'hit' ? ' · cached' : secs != null ? ` · ${secs.toFixed(2)} s` : '';
  return { verdict: `passes${took}`, tone: warn ? 'bad' : view.counts.notes ? 'note' : 'ok', counts, progress: null };
}

/** The error shape `tileErrorDetails` takes, from a message about the tile. */
const asTileError = (m) => ({
  code: m.code ?? m.kind, file: m.span?.file ?? null, line: m.span?.line ?? null, column: m.span?.col ?? null,
  message: m.text, help: m.help, unexpected: m.kind === 'server',
});

/**
 * The report for one row, to paste into a bug report or to an assistant: a mistake or note in the
 * tile is `tileErrorDetails` (its code, where, words, the source line); a failure of the program's
 * or the page's is `detailsOf`, with the whole chain and both stacks; a record says what it says.
 * `source` is the text of the file the span is in, when the page has it.
 */
export function detailsFor(m, { source = null, env } = {}) {
  const opts = env ? { source, env } : { source };
  if (m.kind === 'server') {
    const d = m.data || {};
    const se = new ServerError({
      kind: d.kind || 'failure', message: m.text, help: m.help, causes: d.causes || [], backtrace: d.backtrace || '',
      context: { route: d.route || null, tile: m.tile_name || m.tile || null, version: d.version || null },
    });
    return detailsOf(d.route || (d.origin === 'page' ? 'The page' : 'The server'), new Error(m.text, { cause: se }), ...(env ? [env] : []));
  }
  if (isProblem(m)) return tileErrorDetails(asTileError(m), opts);
  return `${m.text}${m.data && Object.keys(m.data).length ? `\n${JSON.stringify(m.data)}` : ''}`;
}

/** The messages panel's height, dragged to `h` px with `room` px above the status line: at least 96, and the view keeps 80 above it. */
export const MSG_MIN = 96;
export function fitMsgHeight(h, room) {
  return Math.round(Math.max(MSG_MIN, Math.min(h, room - 80)));
}
