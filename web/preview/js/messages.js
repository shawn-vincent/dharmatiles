// @ts-check
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

import { signal } from '@preact/signals-core';
import { ServerError } from './server-error.js';
import { detailsOf, tileErrorDetails } from './problems.js';

/** @typedef {import('./api').MessageJson} MessageJson */
/** @typedef {import('./api').GateData} GateData */
/** @typedef {import('./api').BuildData} BuildData */
/** @typedef {import('./api').ServerData} ServerData */

// A message's `data` is the numbers of its kind (api.rs, `MessageJson`): these are the readings that pair a kind with its type, once,
// here; the program makes each kind's data from the one type (crate::message), and a test holds the pairing.
/** @param {MessageJson | null | undefined} m @returns {GateData | null} */
export const gateOf = (m) => (m?.kind === 'gate' ? /** @type {GateData} */ (m.data) : null);
/** @param {MessageJson | null | undefined} m @returns {BuildData | null} */
export const buildOf = (m) => (m?.kind === 'build' && m.code == null ? /** @type {BuildData} */ (m.data) : null); // (a build's note about the cache has a code and no numbers)
/** @param {MessageJson} m @returns {ServerData | null} */
export const serverOf = (m) => (m.kind === 'server' ? /** @type {ServerData} */ (m.data) : null);

/**
 * Where a message's code is explained: the copy of its error page that the program serves from the docs it was installed with
 * (`/docs/e/CODE.html`, so it matches the installed program and needs no network), or null for a message with no `dt::` code.
 * The code's own page links on to the rule.
 * @param {string | null | undefined} code
 * @returns {string | null}
 */
export const explainHref = (code) => (code && /^dt::[a-z0-9-]+$/.test(code) ? `/docs/e/${code.slice(4)}.html` : null);

/** Is this a problem (something to fix or look at), rather than a record of what happened? */
/** @param {MessageJson} m */
export const isProblem = (m) => m.severity === 'error' || m.severity === 'warning' || m.severity === 'note';
/** The record of a build: its timing and the gate's verdict, shown at the top of the panel. */
/** @param {MessageJson} m */
export const isRecord = (m) => m.kind === 'build' || m.kind === 'gate';
const BUILD_KINDS = new Set(['build', 'gate', 'export', 'check', 'progress']);
const ORDER = { error: 0, warning: 1, note: 2, info: 3 };

/**
 * The panel's filters: All, Errors, Notes, Build.
 * @type {[string, string, (m: MessageJson) => boolean][]}
 */
export const FILTERS = [
  // the check's "read the tile" line is a record for the Build filter, not a row among the problems
  ['all', 'All', (m) => !(m.kind === 'check' && m.severity === 'info')],
  ['errors', 'Errors', (m) => m.severity === 'error'],
  ['notes', 'Notes', (m) => m.severity === 'note' || m.severity === 'warning'],
  ['build', 'Build', (m) => BUILD_KINDS.has(m.kind)],
];

/** `1 error · 1 warning · 2 notes`: the counts as the status line says them ('' when there is nothing). */
export function countsText({ errors, warnings = 0, notes }) {
  const n = (k, one) => `${k} ${one}${k === 1 ? '' : 's'}`;
  return [errors && n(errors, 'error'), warnings && n(warnings, 'warning'), notes && n(notes, 'note')].filter(Boolean).join(' · ');
}

export function createLog() {
  // bumped by every change and followed by every read: what draws the messages is drawn again when they change
  const version = signal(0);
  /** @type {Map<number, MessageJson>} */
  const items = new Map();       // seq -> message
  const builds = new Map();      // tile id -> {startSeq}: the build under way, and the newest message before it
  const since = new Map();       // tile id -> messages cleared since the last build ended
  const last = new Map();        // tile id -> messages cleared by the last build that ended
  const dismissed = new Set();   // seqs the page put away (the program keeps them open)
  const listeners = new Set();
  let local = 0;
  const notify = () => { version.value += 1; for (const fn of [...listeners]) fn(); };
  const maxSeq = () => { let n = 0; for (const k of items.keys()) n = Math.max(n, k); return n; };

  return {
    /** Take one message from the stream (the same message again replaces itself). */
    /** @param {MessageJson} m */
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
    byId: (seq) => { void version.value; return items.get(seq) ?? null; },
    all: () => { void version.value; return [...items.values()]; },

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
      void version.value;
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
          warnings: problems.filter((m) => m.severity === 'warning').length,
          notes: problems.filter((m) => m.severity === 'note').length,
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

/**
 * The first mistake of a tile that did not build, as a message (`{code, text, help, span}`): the first open error of the
 * stream's view, else what the program said of the tile itself (`errors`, then `failure`), else null.  The one answer the
 * view's card and the left panel both read, so neither keeps a copy of it.
 * @param {any} t the tile's state from the program @param {{rows: Array<{m: any, dim?: boolean}>}} v the stream's view of it
 */
export function firstError(t, v) {
  if (!t || t.status !== 'failed') return null;
  const row = v.rows.map((r) => r.m).find((x) => x.severity === 'error' && !x.dim);
  if (row) return row;
  const e = t.errors?.[0];
  if (e) return { code: e.code, text: e.message, help: e.help, span: e.line ? { file: e.file ?? t.path, line: e.line } : null };
  if (t.failure) return { code: t.failure.kind, text: t.failure.message, help: t.failure.help, span: null };
  return null;
}

/**
 * What the tile level of the left panel is: 'none' (no tile is open), 'unread' (it is open and could not be read or built:
 * it carries its mistake, `firstError`), 'reading' (open, not yet described) or 'sheet'.  A tile that failed is never 'none'.
 * @param {any} t @param {any} model the sheet model, or null @param {any} first `firstError` of the tile
 */
export function tileLevelState(t, model, first) {
  if (!t) return 'none';
  if (model) return 'sheet';
  return t.status === 'failed' || first ? 'unread' : 'reading';
}

/** The rows `filter` ('all', 'errors', 'notes', 'build') keeps. */
export function filterRows(rows, filter) {
  const keep = FILTERS.find((f) => f[0] === filter)?.[2] ?? (() => true);
  return rows.filter((r) => keep(r.m));
}

/**
 * Work in flight for a document, in words: one record kind (`Progress`, api.rs) for a tile's build and a map's pieces
 * loading, so the status line and the running row say it from one place and nothing counts for itself.
 * `title` is the running row's line, `stage` its second line's start, `short` what the status line adds to its word.
 * @param {import('./api').Progress} p @param {string} name
 */
export function jobWords(p, name) {
  const loading = p.what === 'loading';
  const stage = !p.count ? p.label : p.count[2] === 0 ? `${p.label}: ${p.count[0]}` : `${p.label}: ${p.count[0]} ${p.count[1]}/${p.count[2]}`;
  return {
    verb: loading ? 'loading' : 'building',
    title: loading ? `Loading the pieces of ${name}` : `Building ${name}`,
    stage,
    short: loading && p.count ? `${p.count[1]} of ${p.count[2]} pieces · ${p.label}` : p.label,
  };
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
  if (t.status === 'building' || view.running || p) {
    const pct = p ? Math.floor(p.fraction * 100) : 0;
    const words = p ? jobWords(p, t.name) : null;
    // a stage with no words yet is just its percent: never a separator with nothing after it
    const stage = words ? [`${pct}%`, words.short].filter(Boolean).join(' · ') : 'starting';
    const what = words?.verb === 'loading' ? 'loading' : t.shown ? 'rebuilding' : 'building';
    // a draft on show (nothing earlier of the tile was kept) has not been through the gate: `unchecked`, said once by the status line
    return { verdict: `${what} ${stage}`, tone: 'busy', counts, progress: p ? p.fraction : 0, unchecked: t.shown?.checked === false };
  }
  if (t.status === 'idle') return { verdict: 'not built yet', tone: 'idle', counts, progress: null };
  const fails = view.record?.gate?.severity === 'error';
  if (t.status === 'failed') return { verdict: view.counts.errors ? 'no build' : 'build failed', tone: 'bad', counts, progress: null };
  if (fails) return { verdict: 'fails the gate', tone: 'bad', counts, progress: null };
  const data = buildOf(view.record?.build);
  const secs = data?.build_s;
  const warn = view.counts.errors > 0;
  const took = data?.cache === 'hit' ? ' · cached' : secs != null ? ` · ${secs.toFixed(2)} s` : '';
  return { verdict: `passes${took}`, tone: warn ? 'bad' : (view.counts.notes || view.counts.warnings) ? 'note' : 'ok', counts, progress: null };
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
/** @param {MessageJson} m @param {{ source?: string | null, env?: any }} [o] */
export function detailsFor(m, { source = null, env } = {}) {
  const opts = env ? { source, env } : { source };
  const d = serverOf(m);
  if (d) {
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

/**
 * One gauge of the gate's verdict: how much of an overhang budget was used.  `fill` is 0 to 1 (full at the budget), `over` the
 * multiple past it written in the bar (`16×`; `over` for a budget of nothing), null while within it, `tone` ok / warn / bad.
 * `null` when the gate did not send the area.  @param {number | null | undefined} used @param {number | null | undefined} budget
 */
export function gaugeOf(used, budget) {
  if (used == null) return null;
  const b = budget ?? 0;
  const past = used > b;
  return {
    used, budget: b,
    fill: past ? 1 : b > 0 ? used / b : 0,
    over: past ? (b > 0 ? `${Math.max(2, Math.round(used / b))}×` : 'over') : null,
    tone: past ? 'bad' : used > 0 ? 'warn' : 'ok',
  };
}

/**
 * The gate's numbers as rows for the messages panel: `[name, value, tone]`.  `data` is the gate message's data as the program
 * sends it (`crate::message`: snake_case names, as every field of a message is; tests/fixtures/gate-numbers.json is that data,
 * held to by a test in the program), `angles` the band's and the steepest angle.  A number the message does not carry is left
 * out of the row, never shown as NaN.
 */
/** @param {GateData} data @param {{ overhangDeg: number, steepestDeg: number }} angles */
export function gateRows(data, { overhangDeg, steepestDeg }) {
  const area = (mm2, budget) => (mm2 == null ? null : budget == null ? `${Number(mm2).toFixed(2)} mm²` : `${Number(mm2).toFixed(2)} / ${Number(budget).toFixed(2)} mm²`);
  const tone = (mm2, budget) => (mm2 > (budget ?? 0) ? 'bad' : mm2 > 0 ? 'warn' : 'ok');
  const rows = [
    ['Triangles', Number(data.triangles).toLocaleString(), ''],
    ['Watertight', data.watertight ? 'Yes' : 'No', data.watertight ? 'ok' : 'bad'],
    [`Band ${overhangDeg}–${steepestDeg}°`, area(data.band_mm2, data.band_budget_mm2), tone(data.band_mm2, data.band_budget_mm2)],
    [`Past ${steepestDeg}°`, area(data.overhang_mm2, data.overhang_budget_mm2), tone(data.overhang_mm2, data.overhang_budget_mm2)],
    ['Dropped bodies', data.dropped, data.dropped ? 'bad' : ''],
    ['Voids filled', data.voids_filled, ''],
    ['Holes', data.holes, data.holes ? 'bad' : ''],
  ];
  return rows.filter(([, v]) => v != null);
}
