// @ts-check
// The editor's service in a web worker: the engine (`cargo xtask wasm`: the files `engine.json` names) in its own thread, with
// the same `Service` the program answers over HTTP (dt-web's service.rs): the page's `workerHost` (js/host.js) sends a call by
// its verb and gets the answer back, with no server.  The page never writes tile text and never reads a build: it asks.
//
//   to the worker    {id, op: 'init', glue, module, library}  the engine's glue (a URL), its compiled module and the library as JSON: the page
//                                                              fetched them (js/host.js is the one place the page reaches the network)
//                    {id, op: 'call', verb, tile, request}     one verb; `tile` is a tile's id or its path, `request` the verb's JSON
//                    {id, op: 'bytes', verb, tile, request}    a verb that answers bytes (`draft`, `step`)
//                    {id, op: 'mesh', tile, docRev}            the page's mesh of the revision, or none
//                    {id, op: 'snapshot'}                      the first message of a stream
//   from the worker  {id, ok: true, json | bytes | mesh, events}   the answer, and what the stream said while it was made
//                    {id, ok: false, refusal | message, events}    a refusal (the JSON of dt_service's RefusalReply) or a trap or a throw
//                    {type: 'stream', json}                         one stream event as it happens (a build's progress)
//                    {type: 'ready', version}                       the engine is up and has the library
//
// A trap or a throw is loud: it is answered with its words, never an empty answer.
/** @typedef {{ id: number, op: 'init', glue: string, module: WebAssembly.Module, library: string }} InitMsg */
/** @typedef {{ id: number, op: 'call' | 'bytes', verb: string, tile: string, request: string }} CallMsg */
/** @typedef {{ id: number, op: 'mesh', tile: string, docRev: number }} MeshMsg */
/** @typedef {{ id: number, op: 'snapshot' }} SnapshotMsg */
/** @typedef {InitMsg | CallMsg | MeshMsg | SnapshotMsg} ToWorker */
/** @typedef {{ id: number, ok: true, events: string, json?: string, bytes?: Uint8Array, mesh?: Uint8Array | null }} Answered  `events` is the JSON array of the stream events the call made */
/** @typedef {{ id: number, ok: false, events: string, refusal?: string, message?: string }} Failed */
/** @typedef {{ type: 'stream', json: string }} Streamed */
/** @typedef {{ type: 'ready', version: string }} Ready */
/** @typedef {Answered | Failed | Streamed | Ready} FromWorker */

/**
 * The engine's glue, as the page's TypeScript knows it: only what this worker calls.
 * @typedef {{
 *   default: (init: { module_or_path: BufferSource | WebAssembly.Module }) => Promise<unknown>,
 *   set_library: (json: string) => void,
 *   version: () => string,
 *   Service: new (entropy: Uint8Array) => {
 *     call(verb: string, tile: string, request: string): string,
 *     call_bytes(verb: string, tile: string, request: string): Uint8Array,
 *     mesh(tile: string, docRev: number): Uint8Array | undefined,
 *     snapshot(): string,
 *     events(): string,
 *   },
 * }} Engine
 */

export {}; // a module worker

const scope = /** @type {{ postMessage(m: FromWorker, transfer?: Transferable[]): void, onmessage: ((ev: MessageEvent<ToWorker>) => unknown) | null }} */ (
  /** @type {unknown} */ (self)
);

// a Rust panic reaches the console just before the trap: keep it, so the error says what it was
let lastPanic = '';
const consoleError = console.error.bind(console);
console.error = (...args) => { lastPanic = args.join(' '); consoleError(...args); };

// the engine calls this with the JSON of one stream event as it happens (a build's progress, while the call is still running)
// @ts-ignore a global the engine's glue looks up by name
globalThis.dtStream = (/** @type {string} */ json) => scope.postMessage({ type: 'stream', json });

/** @type {{ call(verb: string, tile: string, request: string): string, call_bytes(verb: string, tile: string, request: string): Uint8Array, mesh(tile: string, docRev: number): Uint8Array | undefined, snapshot(): string, events(): string } | null} */
let service = null;

/** What the stream said since the last pull: the JSON array of events the service wrote. */
const pulled = () => need().events();

const need = () => {
  if (!service) throw new Error('the editor worker was sent a call before it was ready');
  return service;
};

/** The JSON the service threw for a refusal (its message is a RefusalReply), or null when it is a trap or a throw of the engine's own. @param {unknown} e */
const refusalOf = (e) => {
  const words = e instanceof Error ? e.message : typeof e === 'object' && e && 'message' in e ? String(e.message) : String(e);
  try {
    const r = JSON.parse(words);
    return r && typeof r === 'object' && typeof r.status === 'number' && r.body?.error ? words : null;
  } catch { return null; /* not JSON: a trap or a throw of the engine's own, answered with its words */ }
};

scope.onmessage = async (/** @type {MessageEvent<ToWorker>} */ ev) => {
  const m = ev.data;
  try {
    if (m.op === 'init') {
      const engine = /** @type {Engine} */ (await import(m.glue));
      await engine.default({ module_or_path: m.module });
      engine.set_library(m.library);
      // a worker has no system to ask for a number nobody chose: the page gives real randomness, a fresh seed is drawn from it
      service = new engine.Service(crypto.getRandomValues(new Uint8Array(32)));
      scope.postMessage({ type: 'ready', version: engine.version() });
      scope.postMessage({ id: m.id, ok: true, events: '[]' });
      return;
    }
    const s = need();
    lastPanic = '';
    if (m.op === 'call') {
      const json = s.call(m.verb, m.tile, m.request);
      scope.postMessage({ id: m.id, ok: true, json, events: pulled() });
    } else if (m.op === 'bytes') {
      const bytes = s.call_bytes(m.verb, m.tile, m.request);
      scope.postMessage({ id: m.id, ok: true, bytes, events: pulled() }, [bytes.buffer]);
    } else if (m.op === 'mesh') {
      const mesh = s.mesh(m.tile, m.docRev) ?? null;
      scope.postMessage({ id: m.id, ok: true, mesh, events: '[]' }, mesh ? [mesh.buffer] : []);
    } else if (m.op === 'snapshot') {
      scope.postMessage({ id: m.id, ok: true, json: s.snapshot(), events: '[]' });
    } else {
      throw new Error(`the editor worker was sent a "${/** @type {{ op: string }} */ (m).op}" message it does not know`);
    }
  } catch (e) {
    const refusal = refusalOf(e);
    // events the failed call made (a check's messages) are still told
    let events = '[]';
    try { events = service ? pulled() : '[]'; } catch (again) { consoleError('the editor worker could not pull the stream after a failure', again); }
    if (refusal) { scope.postMessage({ id: m.id, ok: false, refusal, events }); return; }
    const words = e instanceof Error ? e.message : typeof e === 'object' && e && 'message' in e ? String(e.message) : String(e);
    scope.postMessage({ id: m.id, ok: false, message: (lastPanic ? `${lastPanic}\n` : '') + words, events });
  }
};
