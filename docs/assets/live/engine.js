// The engine for a page of cards: one compiled module, one worker, one waiting line (queue.js).  This is the site's host, the
// third beside the program's server and the desktop app: it gives each card the events the editor's host gives (progress,
// messages, the built mesh, the map JSON), from the worker instead of the server.
//
//   a card calls    engine.register(id, onEvent)  ->  {edit(job), full(job), focus(), forget()}
//   it is told      queued {behind} · started {kind} · progress {fraction, label} · checked {result} · built {kind, meta, positions,
//                   colours, stl, cached, ms} · stopped {kind} · failed {message, stack}
//
// Every event carries the `version` of the job it answers; the card drops any that is older than its own (`isStale`), but a
// result that comes back late is kept in the cache, so the work is not lost.  Nothing is fetched until `start`/`prefetch`.

import { Queue } from './queue.js';

/**
 * What a card builds while it is edited, chosen by measuring (shore, strip and pond examples, one tile each): the program's `low`
 * tier at 24 cells a square takes 0.17 to 0.35 s and draws 7 000 to 15 000 triangles; its `draft` tier takes 1.0 to 2.3 s and
 * draws 400 000 to 900 000, and a full build 4.8 to 10 s.  Cells hardly matter to the other tiers (each draft cell count was
 * within 15 % of the others), so the page chooses the tier, not the cells.
 */
export const DRAFT_TIER = "low";
export const DRAFT_CELLS = 24;
/** Results kept for the visit: sliding back to a value is instant. */
const KEEP = 24;
/** A build that runs this long is stopped, loudly. */
const RUNAWAY_MS = 240_000;

export class Engine {
  /** @param {URL} root the docs folder: `engine/` and `assets/live/worker.js` are under it */
  constructor(root) {
    this.root = root;
    this.queue = new Queue();
    /** @type {Map<string, (ev: any) => void>} */
    this.cards = new Map();
    /** @type {Map<string, number>} the newest version each card asked for */
    this.versions = new Map();
    /** @type {Map<string, any>} */
    this.cache = new Map();
    this.serial = 0;
    /** @type {{ id: number, card: string, job: any, timer: any } | null} */
    this.running = null;
    this.worker = null;
    this.started = null;
    this.moduleBytes = null;
  }

  /** Fetch the module quietly (no compile, no worker), so the first touch is quick.  Skipped on a metered link. */
  prefetch() {
    const c = /** @type {any} */ (navigator).connection;
    if (c?.saveData || /(^|-)2g$/.test(c?.effectiveType ?? '')) return;
    this.moduleBytes ??= fetch(new URL('engine/dt_web_bg.wasm', this.root)).catch(() => null);
  }

  /** Compile the module once and bring up the worker.  Resolves when the worker has the library; rejects loudly. */
  start() {
    this.started ??= this.#start();
    return this.started;
  }

  async #start() {
    const wasm = new URL('engine/dt_web_bg.wasm', this.root);
    const lib = new URL('engine/library.json', this.root);
    const [response, library] = await Promise.all([
      (await this.moduleBytes) ?? fetch(wasm),
      fetch(lib).then((r) => {
        if (!r.ok) throw new Error(`cannot read ${lib}: ${r.status} ${r.statusText}`);
        return r.text();
      }),
    ]);
    if (!response.ok) throw new Error(`cannot read ${wasm}: ${response.status} ${response.statusText}`);
    this.module = await WebAssembly.compileStreaming(response);
    this.library = library;
    await this.#spawn();
  }

  /** A fresh worker from the already compiled module (tens of milliseconds: the engine's state is not kept). */
  #spawn() {
    this.worker?.terminate();
    const worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
    this.worker = worker;
    return new Promise((resolve, reject) => {
      worker.onmessage = (ev) => {
        if (ev.data.type === 'ready') { worker.onmessage = (e) => this.#heard(e.data); resolve(undefined); } else if (ev.data.type === 'error') reject(new Error(ev.data.message));
      };
      worker.onerror = (e) => {
        const err = new Error(`the engine's worker failed: ${e.message || 'it could not load'}`);
        reject(err);
        this.#failRunning(err.message, '');
      };
      worker.postMessage({ type: 'init', module: this.module, library: this.library });
    });
  }

  /** @param {string} id @param {(ev: any) => void} onEvent */
  register(id, onEvent) {
    this.cards.set(id, onEvent);
    return {
      edit: (/** @type {any} */ job) => this.#submit(id, { ...job, kind: 'edit', steps: ['check', 'build'] }),
      full: (/** @type {any} */ job) => this.#submit(id, { ...job, kind: "full", steps: ["build"], cells: 0, tier: "full" }),
      focus: () => this.#focus(id),
      forget: () => { this.cards.delete(id); this.queue.drop(id, 'edit'); this.queue.drop(id, 'full'); },
    };
  }

  #emit(card, ev) { this.cards.get(card)?.(ev); }

  static key(job) { return `${job.kind}|${job.tier}|${job.system}|${job.seed}|${job.cells ?? ''}|${job.text}`; }

  #remember(key, value) {
    this.cache.delete(key);
    this.cache.set(key, value);
    while (this.cache.size > KEEP) this.cache.delete(this.cache.keys().next().value);
  }

  #focus(card) {
    this.queue.focusOn(card);
    // a full build under way for another card cannot be paused: it is stopped when someone else is waiting
    const r = this.running;
    if (r && r.job.kind === 'full' && r.card !== card && this.queue.order().some((x) => x.card === card)) {
      this.#respawn(r, 'stopped');
    }
  }

  #submit(card, job) {
    this.versions.set(card, job.version);
    const key = Engine.key(job);
    const hit = this.cache.get(key);
    if (hit && (job.kind === 'full' ? hit.built : hit.checked)) {
      // sliding back to a value already built is instant: said as the events a build would, from the cache
      queueMicrotask(() => {
        if (this.versions.get(card) !== job.version) return;
        if (hit.checked) this.#emit(card, { type: 'checked', version: job.version, result: hit.checked, cached: true });
        if (hit.built) this.#emit(card, { type: 'built', version: job.version, kind: job.kind, ...hit.built, cached: true });
        this.#emit(card, { type: 'idle', version: job.version, kind: job.kind });
      });
      return;
    }
    this.queue.put(card, { ...job, key });
    this.#pump(card);
  }

  /** Run the next job if the worker is free; otherwise tell the card it is waiting, and where in the line. */
  #pump(asked) {
    if (!this.running) {
      const next = this.queue.next();
      if (next) this.#run(next.card, next.job);
    }
    for (const kind of ['edit', 'full']) {
      const n = this.queue.ahead(asked, kind);
      if (n >= 0) this.#emit(asked, { type: 'queued', version: this.queue.pending(asked, kind).version, kind, behind: n + (this.running ? 1 : 0) });
    }
  }

  async #run(card, job) {
    // the worker is taken from this moment, whether or not the engine is up yet: nothing else may start meanwhile
    const id = ++this.serial;
    const r = { id, card, job, timer: null };
    this.running = r;
    this.#emit(card, { type: 'started', version: job.version, kind: job.kind });
    try {
      await this.start();
    } catch (e) {
      this.running = null;
      this.#emit(card, { type: 'failed', version: job.version, kind: job.kind, message: `The engine did not start: ${e.message}`, stack: e.stack ?? '' });
      this.started = null;
      return;
    }
    // the build may have been stopped (and the worker replaced) while the engine was starting
    if (this.running !== r) return;
    r.timer = setTimeout(() => this.#failRunning(`The build took longer than ${Math.round(RUNAWAY_MS / 60000)} minutes and was stopped`, ''), RUNAWAY_MS);
    this.worker.postMessage({ type: "job", id, steps: job.steps, text: job.text, system: job.system, seed: job.seed, cells: job.cells, tier: job.tier });
  }

  /** Take what the worker said.  A message of a job that was stopped is not its card's any more. */
  #heard(m) {
    const r = this.running;
    if (!r || m.id !== r.id) return;
    const { card, job } = r;
    const current = this.versions.get(card) === job.version;
    const cached = this.cache.get(job.key) ?? {};
    if (m.type === 'progress') {
      if (current) this.#emit(card, { type: 'progress', version: job.version, fraction: m.fraction, label: m.label, done: m.done, total: m.total });
    } else if (m.type === 'checked') {
      this.#remember(job.key, { ...cached, checked: m.result });
      if (current) this.#emit(card, { type: 'checked', version: job.version, result: m.result, ms: m.ms });
    } else if (m.type === 'built') {
      const built = { meta: m.meta, positions: m.positions, colours: m.colours, stl: m.stl, ms: m.ms };
      // the cache keeps the arrays: a card that shows them copies what it needs
      this.#remember(job.key, { ...(this.cache.get(job.key) ?? {}), built });
      if (current) this.#emit(card, { type: 'built', version: job.version, kind: job.kind, ...built, cached: false });
      this.#finish(r, current);
    } else if (m.type === 'idle') {
      this.#finish(r, current);
    } else if (m.type === 'error') {
      if (current) this.#emit(card, { type: 'failed', version: job.version, kind: job.kind, message: m.message, stack: m.stack });
      // a trap leaves the module in an unknown state: a fresh worker, from the compiled module
      this.#respawn(r, null);
    }
  }

  #finish(r, current) {
    clearTimeout(r.timer);
    this.running = null;
    if (current) this.#emit(r.card, { type: 'idle', version: r.job.version, kind: r.job.kind });
    const next = this.queue.next();
    if (next) this.#run(next.card, next.job);
  }

  /** The running build is stopped: a new worker from the compiled module.  `why` 'stopped' tells its card it can be resumed. */
  #respawn(r, why) {
    clearTimeout(r.timer);
    this.running = null;
    if (why === 'stopped') this.#emit(r.card, { type: 'stopped', version: r.job.version, kind: r.job.kind });
    this.started = this.#spawn().then(() => undefined);
    this.started.then(() => {
      const next = this.queue.next();
      if (next && !this.running) this.#run(next.card, next.job);
    }, (e) => this.#emit(r.card, { type: 'failed', version: r.job.version, kind: r.job.kind, message: `The engine did not restart: ${e.message}`, stack: e.stack ?? '' }));
  }

  #failRunning(message, stack) {
    const r = this.running;
    if (!r) return;
    this.#emit(r.card, { type: 'failed', version: r.job.version, kind: r.job.kind, message, stack });
    this.#respawn(r, null);
  }
}
