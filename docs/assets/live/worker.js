// @ts-check
// The one worker every card on a page shares: the engine (docs/engine/dt_web*.js|wasm, `cargo xtask wasm`) in its own
// thread, so a build never freezes the page.  The page compiles the module once and hands it over (and hands it over again to a
// fresh worker when a runaway build is stopped); a job is one message in, and the answers come back as messages:
//
//   ready                          the module is up and has the library
//   progress {id, fraction, label} each stage of a build, from the program's own progress events
//   checked  {id, result}          the check: mistakes, notes and the map (dt_eval::map, the program's one producer)
//   built    {id, ...}             a mesh, painted, with the gate's findings (and the STL of a full build)
//   fixed    {id, text}            the source with a mistake's fix laid in (dt_doc::ops Op::Fix, the program's one applier)
//   error    {id, message, stack}  a trap or a throw, loud: the page shows it
//
// A fix is `{type: 'fix', id, text, fix}` (`fix` is the check's mistake's `fix`); it answers `fixed`, or `error`.
// A job is `{id, steps: ['check', 'build'], text, system, seed, cells}`.  `cells` is cells per square: a number for a draft, 0
// for the build's own finest, the detail the printer is given.
// The compiled module (docs/engine/, `cargo xtask wasm`) is not in the repo: it is imported by a name tsc does not
// follow, and typed by the generated engine-wire.d.ts (`Engine`, `Answers`), the same whether a build is on disk or not.
const ENGINE = '../../engine/dt_web.js';
const engine = /** @type {import('./engine-wire').Engine} */ (await import(ENGINE));
const init = engine.default;
export {}; // a module worker: the engine is awaited at the top

/** @typedef {import('./engine-wire').Answers} Answers */

/** The worker's own scope, which the `DOM` lib types as a window. */
const scope = /** @type {{ postMessage(m: unknown, transfer?: Transferable[]): void, onmessage: ((ev: MessageEvent) => unknown) | null }} */ (
  /** @type {unknown} */ (self)
);

/** The JSON an engine function answered, as the type the engine's Rust code says it is (`Answers`). @template {keyof Answers} K @param {K} _fn @param {string} json @returns {Answers[K]} */
const answer = (_fn, json) => JSON.parse(json);

let ready = false;
// a Rust panic reaches the console just before the trap: keep it, so the error says what it was
let lastPanic = '';
const consoleError = console.error.bind(console);
console.error = (...args) => { lastPanic = args.join(' '); consoleError(...args); };

/** @type {string | null} the id of the job being built */
let current = null;
// the engine calls this at each stage with the JSON of a `StageTick`
// @ts-ignore a global the engine's glue looks up by name
globalThis.dtProgress = (/** @type {string} */ tick) => {
  if (current != null) scope.postMessage({ type: 'progress', id: current, ...answer('dtProgress', tick) });
};

scope.onmessage = async (ev) => {
  const m = ev.data;
  try {
    if (m.type === 'init') {
      await init({ module_or_path: m.module });
      engine.set_library(m.library);
      ready = true;
      scope.postMessage({ type: 'ready', version: engine.version() });
      return;
    }
    if (m.type === 'fix') {
      if (!ready) throw new Error('the worker was given a fix before it was ready');
      scope.postMessage({ type: 'fixed', id: m.id, text: engine.fix_tile(m.text, JSON.stringify(m.fix)) });
      return;
    }
    if (m.type !== 'job') throw new Error(`the worker was sent a "${m.type}" message it does not know`);
    if (!ready) throw new Error('the worker was given a job before it was ready');
    current = m.id;
    lastPanic = '';
    if (m.steps.includes('check')) {
      const t0 = performance.now();
      const result = answer('check_tile', engine.check_tile(m.text, m.system));
      scope.postMessage({ type: 'checked', id: m.id, result, ms: performance.now() - t0 });
      if (!result.ok || !m.steps.includes('build')) { scope.postMessage({ type: 'idle', id: m.id }); current = null; return; }
    }
    if (m.steps.includes('build')) {
      const t0 = performance.now();
      const tile = engine.build_tile(m.text, m.system, m.seed, m.cells, m.tier);
      const ms = performance.now() - t0;
      const meta = answer('build_tile().meta', tile.meta);
      const positions = tile.positions, colours = tile.colours, stl = tile.stl;
      tile.free();
      scope.postMessage({ type: 'built', id: m.id, ms, meta, positions, colours, stl }, [positions.buffer, colours.buffer, stl.buffer]);
    }
    current = null;
  } catch (e) {
    current = null;
    // a trap or a throw is loud: the page shows it, never a blank view
    scope.postMessage({ type: 'error', id: m.id, message: (lastPanic ? `${lastPanic}\n` : '') + String(e && e.message ? e.message : e), stack: e && e.stack ? String(e.stack) : '' });
  }
};
