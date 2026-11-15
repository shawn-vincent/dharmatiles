// @ts-check
// A card's messages: what the engine's checks and builds say, made into the messages the editor draws
// (web/preview/js/messages.js: `createLog`, one stream; the status line, the list and the marks in the source all read it).
// The program's server makes these messages in crates/dharmatiles/src/message.rs; the browser build makes them here from the
// diagnostics of dt-web (`check_tile`, `build_tile`), as the program's `MessageJson` (the page's own type), so the one list
// draws both.
//
// A check replaces the last check's messages: what it no longer reports is cleared, never left to go stale.

import { createLog } from '../../editor/js/messages.js';

/** @typedef {import('../../../web/preview/js/api').MessageJson} MessageJson (the repo's path: the site serves the page's code at `../../editor/js`, where api.d.ts is not published, so `tsc` reads it from here) */
/** @typedef {import('./engine-wire').Diagnostic} Diagnostic */
/** @typedef {import('./engine-wire').Checked} Checked */
/** @typedef {import('./engine-wire').BuildMeta} BuildMeta */
/**
 * What a message says: the rest of a `MessageJson` is the log's (its place in the stream, its time, its state, its tile)
 * or what the browser has nothing for (`said` makes those null).
 * @typedef {Omit<MessageJson, 'seq' | 't' | 'state' | 'tile' | 'tile_name' | 'file' | 'doing' | 'source' | 'reportable'>} Said
 */

const FILE = 'tile.dtile';

/** The one place the page completes a message: the program's fields it has nothing for are null. @param {Said} m */
const said = (m) => ({ ...m, file: null, doing: null, source: null, reportable: false });

/** @param {Diagnostic} d @param {boolean} note */
function fromDiagnostic(d, note) {
  return said({
    kind: note ? 'note' : 'error', severity: note ? 'note' : 'error', scope: 'check', code: d.code,
    node: null, text: d.message, help: d.help, fix: d.fix, data: {},
    span: d.file === FILE || !d.file ? { file: FILE, line: d.line, col: d.col, offset: d.span[0], len: d.span[1] } : null,
  });
}

/** @param {{ scope: string, kind: string, code: string | null, text: string, span: { offset: number } | null }} m */
const keyOf = (m) => `${m.scope}|${m.kind}|${m.code}|${m.text}|${m.span?.offset ?? ''}`;

export class CardLog {
  /** @param {string} tile the card's id, which is the tile the log is about */
  constructor(tile) {
    this.tile = tile;
    this.log = createLog();
    this.seq = 0;
    /** @type {Map<string, MessageJson>} the open messages by what makes them the same message */
    this.open = new Map();
  }

  /**
   * Say `partials` as the whole of what `scope` reports now: the new ones open, the ones no longer reported cleared.
   * @param {'check' | 'build'} scope @param {ReturnType<typeof said>[]} partials
   */
  report(scope, partials) {
    /** @type {Map<string, MessageJson>} */
    const next = new Map();
    for (const p of partials) {
      /** @type {MessageJson} */
      const m = { seq: 0, t: new Date().toISOString(), state: 'open', tile: this.tile, tile_name: this.tile, ...p };
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

  /** A check's result (`check_tile`): its mistakes, then its notes. @param {Checked} result */
  checked(result) {
    this.report('check', [...result.errors.map((d) => fromDiagnostic(d, false)), ...result.notes.map((d) => fromDiagnostic(d, true))]);
  }

  /**
   * A finished build.  A draft has not been through the gate and says so; a full build reports the gate's verdict: the
   * gate message's data is the build record's audit (the program's `AuditSummary`), as the editor's is.
   * @param {BuildMeta} meta @param {{ draft: boolean, cached: boolean, ms: number }} how
   */
  built(meta, { draft, cached, ms }) {
    const secs = ms / 1000;
    if (!meta.record) throw new Error('the engine said a build was built, with no record of it');
    const audit = meta.record.audit;
    /** @type {ReturnType<typeof said>[]} */
    const out = [said({
      kind: 'build', severity: 'info', scope: 'build', code: null, node: null, span: null, help: null, fix: null,
      text: cached ? `From the cache (${draft ? 'draft' : 'full'} detail)` : `Built at ${draft ? 'draft' : 'full'} detail in ${secs.toFixed(2)} s`,
      data: { build_s: secs, triangles: audit.triangles, draft, cache: cached ? 'hit' : 'miss' },
    })];
    if (!draft) {
      const fails = !meta.record.checked || audit.failures.length > 0;
      out.push(said({
        kind: 'gate', severity: fails ? 'error' : 'info', scope: 'build', code: 'gate', node: null, span: null, help: null, fix: null,
        text: fails ? `Fails the printability gate: ${audit.failures.join('; ')}` : 'Passes the printability gate',
        data: audit,
      }));
    }
    this.report('build', out);
  }

  /** The engine failed, not the document: loud, and kept until a later build replaces it. @param {string} message @param {string} [stack] */
  failed(message, stack) {
    this.report('build', [said({
      kind: 'server', severity: 'error', scope: 'build', code: 'failure', node: null, span: null, help: null, fix: null,
      text: message, data: { origin: 'page', kind: 'failure', backtrace: stack ?? '' },
    })]);
  }

  /** A build is under way: the rows from the last one are shown as such until it ends. */
  begin() { this.log.beginBuild(this.tile); }
  end() { this.log.endBuild(this.tile); }
  view() { return this.log.view(this.tile); }
}
