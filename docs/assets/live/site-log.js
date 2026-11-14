// A card's messages: what the engine's checks and builds say, made into the messages the editor draws
// (web/preview/js/messages.js: `createLog`, one stream; the status line, the list and the marks in the source all read it).
// The program's server makes these messages in crates/dharmatiles/src/message.rs; the browser build makes them here from the
// diagnostics of dt-web (`check_tile`, `build_tile`), with the same fields and words, so the one list draws both.
//
// A check replaces the last check's messages: what it no longer reports is cleared, never left to go stale.

import { createLog } from '../../editor/js/messages.js';

const FILE = 'tile.dtile';

/** @param {any} d a diagnostic of dt-web @param {string} tile @returns {object} */
function fromDiagnostic(d, tile, note) {
  return {
    kind: note ? 'note' : 'error', severity: note ? 'note' : 'error', scope: 'check', code: d.code, tile, tile_name: tile,
    node: null, text: d.message, help: d.help ?? null, fix: d.fix ?? null, data: {},
    span: d.file === FILE || !d.file ? { file: FILE, line: d.line, col: d.col, offset: d.span[0], len: d.span[1] } : null,
  };
}

const keyOf = (m) => `${m.scope}|${m.kind}|${m.code}|${m.text}|${m.span?.offset ?? ''}`;

export class CardLog {
  /** @param {string} tile the card's id, which is the tile the log is about */
  constructor(tile) {
    this.tile = tile;
    this.log = createLog();
    this.seq = 0;
    /** @type {Map<string, any>} the open messages by what makes them the same message */
    this.open = new Map();
  }

  /** Say `partials` as the whole of what `scope` reports now: the new ones open, the ones no longer reported cleared. */
  report(scope, partials) {
    const next = new Map();
    for (const p of partials) {
      const m = { seq: 0, t: new Date().toISOString(), state: 'open', tile: this.tile, ...p };
      const key = keyOf(m);
      const old = this.open.get(key);
      // the same message again keeps its place in the stream (a changed span moves it)
      m.seq = old ? old.seq : ++this.seq;
      next.set(key, m);
      this.log.apply(m);
    }
    for (const [key, m] of this.open) {
      if (m.scope === scope && !next.has(key)) this.log.apply({ ...m, state: 'cleared' });
    }
    for (const [key, m] of this.open) if (m.scope !== scope) next.set(key, m);
    this.open = next;
  }

  /** A check's result (`check_tile`): its mistakes, then its notes. */
  checked(result) {
    this.report('check', [...result.errors.map((d) => fromDiagnostic(d, this.tile, false)), ...result.notes.map((d) => fromDiagnostic(d, this.tile, true))]);
  }

  /** A finished build.  A draft has not been through the gate and says so; a full build reports the gate's verdict. */
  built(meta, { draft, cached, ms }) {
    const secs = ms / 1000;
    const out = [{
      kind: 'build', severity: 'info', scope: 'build', code: null, node: null, span: null, help: null, fix: null,
      text: cached ? `From the cache (${draft ? 'draft' : 'full'} detail)` : `Built at ${draft ? 'draft' : 'full'} detail in ${secs.toFixed(2)} s`,
      data: { build_s: secs, triangles: meta.triangles, draft, cache: cached ? 'hit' : 'miss' },
    }];
    if (!draft) {
      const fails = !meta.passes;
      out.push({
        kind: 'gate', severity: fails ? 'error' : 'info', scope: 'build', code: 'gate', node: null, span: null, help: null, fix: null,
        text: fails ? `Fails the printability gate: ${meta.failures.join('; ')}` : 'Passes the printability gate',
        data: {
          triangles: meta.triangles, watertight: meta.watertight, band_mm2: meta.band_mm2, band_budget_mm2: meta.band_budget_mm2,
          overhang_mm2: meta.overhang_mm2, overhang_budget_mm2: meta.overhang_budget_mm2, dropped: 0, holes: meta.holes, failures: meta.failures,
        },
      });
    }
    this.report('build', out);
  }

  /** The engine failed, not the document: loud, and kept until a later build replaces it. */
  failed(message, stack) {
    this.report('build', [{
      kind: 'server', severity: 'error', scope: 'build', code: 'failure', node: null, span: null, help: null, fix: null,
      text: message, data: { origin: 'page', kind: 'failure', backtrace: stack ?? '' },
    }]);
  }

  /** A build is under way: the rows from the last one are shown as such until it ends. */
  begin() { this.log.beginBuild(this.tile); }
  end() { this.log.endBuild(this.tile); }
  view() { return this.log.view(this.tile); }
}
