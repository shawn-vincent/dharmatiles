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
//   edited   {id, made}            the source with an edit made (dt_service::edit::apply, the editor's one applier): `made` is the new text, the node and the text's fingerprint
//   keyed    {id, key}             the key a build of the text is kept under between visits (dt-store's key function, compiled into the engine: the program's disk cache's key maker); null when the text does not read
//   error    {id, message, stack}  a trap or a throw, loud: the page shows it
//
// A fix is `{type: 'fix', id, text, fix}` (`fix` is the check's mistake's `fix`); it answers `fixed`, or `error`.
// An edit is `{type: 'edit', id, req, system}` (`req` is the typed `EditReq`: an op, the node it names by id, and the fingerprint
// of the text the ids were read at); it answers `edited`, or `error` with the program's refusal.  The page never writes tile text.
// A job is `{id, steps: ['check', 'build'], text, system, seed, cells}`.  `cells` is cells per square: a number for a draft, 0
// for the build's own finest, the detail the printer is given.
// The compiled module (docs/engine/, `cargo xtask wasm`) is not in the repo and its files carry their hashes in their names: the page
// reads engine.json and hands the glue's URL over with the module (`init`), so a glue file is always the one its module was made with.
// It is imported by that URL, which tsc does not follow, and typed by the generated engine-wire.d.ts (`Engine`, `Answers`).
/** @type {import('./engine-wire').Engine | null} */
let engine = null;
export {}; // a module worker

/** @typedef {import('./engine-wire').Answers} Answers */

// The protocol, once, where the worker makes and takes the messages (engine.js imports these).  What the answers carry is the
// generated engine-wire.d.ts's: `Answers`.
/** @typedef {'check' | 'build'} Step */
/** What a job asks: its steps, the source, the target, the seed (null for the document's own), the cells a square (0: the finest), the tier, and the card whose step cache a draft builds through (null: a full build keeps none). @typedef {{steps: Step[], text: string, system: string, seed: number | null, cells: number, tier: string, card: string | null}} JobSpec */
/** @typedef {{type: 'init', module: WebAssembly.Module, library: string, glue: string}} InitMsg  to the worker: the compiled module, the library as JSON, and the URL of the glue made with the module */
/** @typedef {{type: 'forget', card: string}} ForgetMsg  to the worker: a card left the page, drop its step cache */
/** @typedef {{type: 'fix', id: string, text: string, fix: import('./engine-wire').FixJson}} FixMsg  to the worker: lay a mistake's fix into the source */
/** @typedef {{type: 'edit', id: string, req: import('./engine-wire').EditReq, system: string}} EditMsg  to the worker: make an edit (the program's `edit_tile`) */
/** @typedef {{type: 'key', id: string, engine: string} & Omit<JobSpec, 'steps' | 'card'>} KeyMsg  to the worker: the key a build of this job would be kept under, for the engine named (the page's hash of it) */
/** @typedef {{type: 'job', id: number} & JobSpec} JobMsg  to the worker: one job */
/** @typedef {InitMsg | FixMsg | EditMsg | KeyMsg | JobMsg | ForgetMsg} ToWorker */
/** @typedef {{type: 'ready', version: string}} ReadyMsg */
/** @typedef {{type: 'progress', id: number} & Answers['dtProgress']} ProgressMsg */
/** @typedef {{type: 'checked', id: number, result: Answers['check_tile'], ms: number}} CheckedMsg */
/** @typedef {{type: 'built', id: number, ms: number, meta: Answers['build_tile().meta'], positions: Float32Array, colours: Float32Array, stl: Uint8Array}} BuiltMsg */
/** @typedef {{type: 'fixed', id: string, text: string}} FixedMsg */
/** @typedef {{type: 'edited', id: string, made: Answers['edit_tile']}} EditedMsg */
/** @typedef {{type: 'keyed', id: string, key: string | null}} KeyedMsg */
/** @typedef {{type: 'idle', id: number}} IdleMsg  the job is done and nothing was built (the check failed, or only the check was asked) */
/** @typedef {{type: 'error', id: number | string | undefined, message: string, stack: string}} ErrorMsg */
/** @typedef {ReadyMsg | ProgressMsg | CheckedMsg | BuiltMsg | FixedMsg | EditedMsg | KeyedMsg | IdleMsg | ErrorMsg} FromWorker */

/** The worker's own scope, which the `DOM` lib types as a window. */
const scope = /** @type {{ postMessage(m: FromWorker, transfer?: Transferable[]): void, onmessage: ((ev: MessageEvent<ToWorker>) => unknown) | null }} */ (
  /** @type {unknown} */ (self)
);

/** The JSON an engine function answered, as the type the engine's Rust code says it is (`Answers`). @template {keyof Answers} K @param {K} _fn @param {string} json @returns {Answers[K]} */
const answer = (_fn, json) => JSON.parse(json);

let ready = false;
// a Rust panic reaches the console just before the trap: keep it, so the error says what it was
let lastPanic = '';
const consoleError = console.error.bind(console);
console.error = (...args) => { lastPanic = args.join(' '); consoleError(...args); };

/** @type {number | null} the id of the job being built */
let current = null;
// the engine calls this at each stage with the JSON of a `StageTick`
// @ts-ignore a global the engine's glue looks up by name
globalThis.dtProgress = (/** @type {string} */ tick) => {
  if (current != null) scope.postMessage({ type: 'progress', id: current, ...answer('dtProgress', tick) });
};

scope.onmessage = async (/** @type {MessageEvent<ToWorker>} */ ev) => {
  const m = ev.data;
  try {
    if (m.type === 'init') {
      engine = /** @type {import('./engine-wire').Engine} */ (await import(m.glue));
      await engine.default({ module_or_path: m.module });
      engine.set_library(m.library);
      ready = true;
      scope.postMessage({ type: 'ready', version: engine.version() });
      return;
    }
    if (!ready || !engine) throw new Error(`the worker was sent a "${m.type}" message before it was ready`);
    if (m.type === 'forget') {
      engine.forget_card(m.card);
      return;
    }
    if (m.type === 'fix') {
      scope.postMessage({ type: 'fixed', id: m.id, text: engine.fix_tile(m.text, JSON.stringify(m.fix)) });
      return;
    }
    if (m.type === 'edit') {
      scope.postMessage({ type: 'edited', id: m.id, made: answer('edit_tile', engine.edit_tile(JSON.stringify(m.req), m.system)) });
      return;
    }
    if (m.type === 'key') {
      scope.postMessage({ type: 'keyed', id: m.id, key: engine.store_key(m.text, m.engine, m.system, m.seed, m.cells, m.tier) ?? null });
      return;
    }
    if (m.type !== 'job') throw new Error(`the worker was sent a "${/** @type {{ type: string }} */ (m).type}" message it does not know`);
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
      const tile = engine.build_tile(m.text, m.system, m.seed, m.cells, m.tier, m.card);
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
    scope.postMessage({ type: 'error', id: 'id' in m ? m.id : undefined, message: (lastPanic ? `${lastPanic}\n` : '') + (e instanceof Error ? e.message : typeof e === 'object' && e && 'message' in e ? String(e.message) : String(e)), stack: e instanceof Error && e.stack ? e.stack : '' });
  }
};
