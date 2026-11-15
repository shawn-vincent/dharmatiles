// @ts-check
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
//
// What keeps a phone alive (docs/standards/performance.html, "The page's cards"):
//   - the result cache is bounded by bytes (limits.js), and a full build is never kept: the card that shows it holds it
//   - a fresh worker follows every full build, so the wasm heap (which never shrinks) goes back to its floor
//   - a draft builds through its card's step cache in the worker (dt-web `build_tile`'s card), dropped with the worker
//   - a build is stopped, and the card told, at three times its estimate (`runawayMs`), not at a fixed four minutes
//   - the files carry their hashes (engine.json) and are kept by the Cache API; an example's draft is kept between visits
//     in IndexedDB under the engine's hash (draft-store.js)

import { Queue } from './queue.js';
import { ByteCache, CACHE_BYTES, keptBytes, runawayMs, storeKey } from './limits.js';
import { DraftStore } from './draft-store.js';

// The messages to and from the worker are typed where worker.js makes and takes them.
/** @typedef {import('./worker.js').ToWorker} ToWorker */
/** @typedef {import('./worker.js').FromWorker} FromWorker */
/** @typedef {import('./worker.js').JobSpec} JobSpec */
/** @typedef {import('./worker.js').BuiltMsg} BuiltMsg */
/** @typedef {import('./engine-manifest').EngineManifest} EngineManifest */
/** What a card asks for: the source, the target and seed, and its own version; a full build also says the CPU seconds the check estimated it at (`Checked.fullCost`), which its runaway limit is made from. @typedef {{text: string, system: string, seed: number | null, version: number, cpuSecs?: number}} FullAsk */
/** An edit also says the cells and tier (a full build is the finest), and whether the text is the page's own (`persist`: its draft is kept between visits). @typedef {FullAsk & {cells: number, tier: string, persist?: boolean}} EditAsk */
/** An ask, made a job: the kind it is, the steps it takes, the key it is cached by, and the card it came from. @typedef {Omit<JobSpec, 'card'> & {kind: 'edit' | 'full', version: number, key?: string, cpuSecs?: number, persist?: boolean, card?: string | null}} Job */
/** The mesh of a build, as kept and as told: what the worker's `built` carries, less its job. @typedef {Pick<BuiltMsg, 'meta' | 'positions' | 'colours' | 'stl' | 'ms'>} Built */
/** What the engine keeps of a job's key: the check's answer and the build. @typedef {{checked?: import('./engine-wire').Checked, built?: Built}} Kept */
/**
 * What a card is told (each carries the `version` of the job it answers):
 * @typedef {{type: 'queued', version: number, kind: Job['kind'], behind: number}
 *   | {type: 'started', version: number, kind: Job['kind']}
 *   | {type: 'progress', version: number, fraction: number, label: string, done: number, total: number}
 *   | {type: 'checked', version: number, result: import('./engine-wire').Checked, ms?: number, cached?: boolean}
 *   | ({type: 'built', version: number, kind: Job['kind'], cached: boolean} & Built)
 *   | {type: 'stopped', version: number, kind: Job['kind']}
 *   | {type: 'idle', version: number, kind: Job['kind']}
 *   | {type: 'failed', version: number, kind: Job['kind'], message: string, stack: string}} CardEvent
 */

/**
 * What a card builds while it is edited, chosen by measuring (shore, strip and pond examples, one tile each): the program's `low`
 * tier at 24 cells a square takes 0.17 to 0.35 s and draws 7 000 to 15 000 triangles; its `draft` tier takes 1.0 to 2.3 s and
 * draws 400 000 to 900 000, and a full build 4.8 to 10 s.  Cells hardly matter to the other tiers (each draft cell count was
 * within 15 % of the others), so the page chooses the tier, not the cells.
 */
export const DRAFT_TIER = "low";
export const DRAFT_CELLS = 24;

export class Engine {
  /** @param {URL} root the docs folder: `engine/` and `assets/live/worker.js` are under it */
  constructor(root) {
    this.root = root;
    this.queue = new Queue();
    /** @type {Map<string, (ev: CardEvent) => void>} */
    this.cards = new Map();
    /** @type {Map<string, number>} the newest version each card asked for */
    this.versions = new Map();
    /** Results kept for the visit, by bytes: sliding back to a value is instant.  A full build is not kept. @type {ByteCache<Kept>} */
    this.cache = new ByteCache(CACHE_BYTES, /** @param {Kept} k */ (k) => keptBytes(k));
    /** @type {Promise<DraftStore | null> | null} the between-visits store of drafts, once the manifest says which engine it is */
    this.store = null;
    /** @type {Promise<EngineManifest> | null} */
    this.manifestAsked = null;
    this.serial = 0;
    /** @type {string | null} the engine's version, once its worker has said it (`ready`) */
    this.version = null;
    /** @type {string | null} the engine's hash, from the manifest */
    this.hash = null;
    /** @type {{ id: number, card: string, job: Job, timer: ReturnType<typeof setTimeout> | null } | null} */
    this.running = null;
    /** @type {Worker | null} */
    this.worker = null;
    /** @type {Promise<void> | null} */
    this.started = null;
    /** @type {WebAssembly.Module | null} the compiled module, once `start` has it */
    this.module = null;
    /** @type {string | null} the library's JSON, once `start` has it */
    this.library = null;
    /** @type {string | null} the URL of the glue made with the module, once `start` has the manifest */
    this.glue = null;
    /** @type {Promise<Response | null> | null} the module's bytes, fetched ahead of the first touch (null: the fetch failed, `start` fetches again) */
    this.moduleBytes = null;
    /** @type {Map<string, {resolve: (text: string) => void, reject: (e: Error) => void}>} fixes asked of the worker, by id */
    this.fixes = new Map();
  }

  /** The engine's manifest (`engine.json`: the hashed names of its files and its hash), read fresh each visit, once. */
  manifest() {
    return (this.manifestAsked ??= (async () => {
      const url = new URL('engine/engine.json', this.root);
      const r = await fetch(url, { cache: 'no-cache' });
      if (!r.ok) throw new Error(`cannot read ${url}: ${r.status} ${r.statusText}`);
      const m = /** @type {EngineManifest} */ (await r.json());
      this.hash = m.hash;
      return m;
    })());
  }

  /**
   * A file of the engine, from the Cache API when this engine's copy is there, else fetched and kept (the name carries the file's hash,
   * so a kept copy is never the wrong one; caches of other engines are removed).  The cache is a convenience: when it cannot be used the
   * file is fetched as it always was.
   * @param {string} name @returns {Promise<Response>}
   */
  async #file(name) {
    const m = await this.manifest();
    const url = new URL(`engine/${name}`, this.root);
    /** @type {Cache | null} */
    let kept = null;
    try {
      kept = await globalThis.caches?.open(`dt-engine-${m.hash}`) ?? null;
      const hit = await kept?.match(url);
      if (hit) return hit;
    } catch {
      kept = null; // no Cache API here (an insecure page, blocked site data): fetch as usual
    }
    const r = await fetch(url);
    if (r.ok && kept) {
      try {
        await kept.put(url, r.clone());
        for (const k of await globalThis.caches.keys()) if (k.startsWith('dt-engine-') && k !== `dt-engine-${m.hash}`) await globalThis.caches.delete(k);
      } catch {
        // a copy that cannot be kept (quota) only means the next visit fetches it again
      }
    }
    return r;
  }

  /** The between-visits store of drafts, for this engine; null when the manifest cannot be read (`start` says so, loudly) or IndexedDB is missing. */
  #draftStore() {
    return (this.store ??= this.manifest().then((m) => new DraftStore(m.hash), () => null));
  }

  /** Fetch the module quietly (no compile, no worker), so the first touch is quick.  Skipped on a metered link. */
  prefetch() {
    // boundary: the Network Information API is in no DOM lib
    const c = /** @type {{saveData?: boolean, effectiveType?: string} | undefined} */ (/** @type {{connection?: unknown}} */ (navigator).connection);
    if (c?.saveData || /(^|-)2g$/.test(c?.effectiveType ?? '')) return;
    // a prefetch that fails is only a prefetch: `start` fetches again, and says what went wrong
    this.moduleBytes ??= this.manifest().then((m) => this.#file(m.wasm)).catch(() => null);
  }

  /** Compile the module once and bring up the worker.  Resolves when the worker has the library; rejects loudly. */
  start() {
    return (this.started ??= this.#start());
  }

  async #start() {
    const m = await this.manifest();
    const [response, library] = await Promise.all([
      (await this.moduleBytes) ?? this.#file(m.wasm),
      this.#file(m.library).then((r) => {
        if (!r.ok) throw new Error(`cannot read ${m.library}: ${r.status} ${r.statusText}`);
        return r.text();
      }),
    ]);
    if (!response.ok) throw new Error(`cannot read ${m.wasm}: ${response.status} ${response.statusText}`);
    this.module = await WebAssembly.compileStreaming(response);
    this.library = library;
    this.glue = new URL(`engine/${m.js}`, this.root).href;
    await this.#spawn();
  }

  /** A fresh worker from the already compiled module (tens of milliseconds: the engine's state is not kept). */
  #spawn() {
    const { module, library, glue } = this;
    if (!module || library === null || glue === null) throw new Error('The engine has no compiled module yet, so no worker can be started.');
    this.worker?.terminate();
    // a fix waiting on the worker that is stopped is lost with it: said, not left hanging
    for (const f of this.fixes.values()) f.reject(new Error('the engine was restarted before the fix was made'));
    this.fixes.clear();
    const worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
    this.worker = worker;
    return new Promise((resolve, reject) => {
      /** @param {MessageEvent<FromWorker>} ev */
      worker.onmessage = (ev) => {
        if (ev.data.type === 'ready') { this.version = ev.data.version; worker.onmessage = (/** @type {MessageEvent<FromWorker>} */ e) => this.#heard(e.data); resolve(undefined); } else if (ev.data.type === 'error') reject(new Error(ev.data.message));
      };
      worker.onerror = (e) => {
        const err = new Error(`the engine's worker failed: ${e.message || 'it could not load'}`);
        reject(err);
        this.#failRunning(err.message, '');
      };
      Engine.post(worker, { type: 'init', module, library, glue });
    });
  }

  /**
   * The source with a mistake's `fix` (from a `checked` result) laid in, by the program's own applier (dt_doc::ops Op::Fix, in the
   * worker).  It waits behind a build the worker is running; rejects loudly when the text is not the one the fix was made for.
   * @param {string} text @param {import('./engine-wire').FixJson} fix @returns {Promise<string>}
   */
  async fix(text, fix) {
    await this.start();
    const id = `fix-${++this.serial}`;
    const worker = this.worker;
    if (!worker) throw new Error('The engine has no worker to make the fix.');
    return new Promise((resolve, reject) => {
      this.fixes.set(id, { resolve, reject });
      Engine.post(worker, { type: 'fix', id, text, fix });
    });
  }

  /** The one place a message goes to the worker, so each is checked against the protocol. @param {Worker} worker @param {ToWorker} message */
  static post(worker, message) { worker.postMessage(message); }

  /** @param {string} id @param {(ev: CardEvent) => void} onEvent */
  register(id, onEvent) {
    this.cards.set(id, onEvent);
    return {
      edit: (/** @type {EditAsk} */ job) => this.#submit(id, { ...job, kind: 'edit', steps: ['check', 'build'], card: id }),
      full: (/** @type {FullAsk} */ job) => this.#submit(id, { ...job, kind: "full", steps: ["build"], cells: 0, tier: "full", card: null }),
      focus: () => this.#focus(id),
      forget: () => {
        this.cards.delete(id); this.queue.drop(id, 'edit'); this.queue.drop(id, 'full');
        if (this.worker && this.version !== null) Engine.post(this.worker, { type: 'forget', card: id });
      },
    };
  }

  /** @param {string} card @param {CardEvent} ev */
  #emit(card, ev) { this.cards.get(card)?.(ev); }

  /** @param {Job} job */
  static key(job) { return `${job.kind}|${job.tier}|${job.system}|${job.seed}|${job.cells ?? ''}|${job.text}`; }

  /** A draft is kept (a full build never is: the card that shows it holds it). @param {string} key @param {Kept} value */
  #remember(key, value) {
    if (key.startsWith('full|')) return;
    this.cache.set(key, value);
  }

  /** @param {string} card */
  #focus(card) {
    this.queue.focusOn(card);
    // a full build under way for another card cannot be paused: it is stopped when someone else is waiting
    const r = this.running;
    if (r && r.job.kind === 'full' && r.card !== card && this.queue.order().some((x) => x.card === card)) {
      this.#respawn(r, 'stopped');
    }
  }

  /** @param {string} card @param {Job} job */
  #submit(card, job) {
    this.versions.set(card, job.version);
    const key = Engine.key(job);
    const hit = job.kind === 'edit' ? this.cache.get(key) : undefined;
    if (hit?.checked) {
      // sliding back to a value already built is instant: said as the events a build would, from the cache
      this.#serve(card, job, hit);
      return;
    }
    if (job.kind === 'edit' && job.persist) {
      // the page's own example: a draft kept from an earlier visit (made by this engine) is shown without waking the engine
      this.#draftStore().then((store) => store?.get(storeKey(this.manifestHash(), job.system, job.text)) ?? null).then((kept) => {
        if (this.versions.get(card) !== job.version) return; // edited again while the store was read
        if (kept?.checked && kept.built) { this.cache.set(key, kept); this.#serve(card, job, kept); return; }
        this.queue.put(card, { ...job, key });
        this.#pump(card);
      });
      return;
    }
    this.queue.put(card, { ...job, key });
    this.#pump(card);
  }

  /** The engine's hash, once its manifest is read (a store only exists after that). */
  manifestHash() {
    if (!this.hash) throw new Error('The engine\'s manifest was not read before a draft was looked up.');
    return this.hash;
  }

  /** Tell `card` what `kept` holds, as the events a build would. @param {string} card @param {Job} job @param {Kept} kept */
  #serve(card, job, kept) {
    queueMicrotask(() => {
      if (this.versions.get(card) !== job.version) return;
      if (kept.checked) this.#emit(card, { type: 'checked', version: job.version, result: kept.checked, cached: true });
      if (kept.built) this.#emit(card, { type: 'built', version: job.version, kind: job.kind, ...kept.built, cached: true });
      this.#emit(card, { type: 'idle', version: job.version, kind: job.kind });
    });
  }

  /** Run the next job if the worker is free; otherwise tell the card it is waiting, and where in the line. */
  /** @param {string} asked */
  #pump(asked) {
    if (!this.running) {
      const next = this.queue.next();
      if (next) this.#run(next.card, next.job);
    }
    /** @type {Array<Job['kind']>} */
    const kinds = ['edit', 'full'];
    for (const kind of kinds) {
      const n = this.queue.ahead(asked, kind);
      if (n >= 0) {
        const waiting = this.queue.pending(asked, kind);
        if (!waiting) throw new Error(`the queue places a ${kind} job of ${asked} in line but holds none`);
        this.#emit(asked, { type: 'queued', version: waiting.version, kind, behind: n + (this.running ? 1 : 0) });
      }
    }
  }

  /** @param {string} card @param {Job} job */
  async #run(card, job) {
    // the worker is taken from this moment, whether or not the engine is up yet: nothing else may start meanwhile
    const id = ++this.serial;
    /** @type {NonNullable<Engine['running']>} */
    const r = { id, card, job, timer: null };
    this.running = r;
    this.#emit(card, { type: 'started', version: job.version, kind: job.kind });
    try {
      await this.start();
    } catch (e) {
      this.running = null;
      this.#emit(card, { type: 'failed', version: job.version, kind: job.kind, message: `The engine did not start: ${e instanceof Error ? e.message : String(e)}`, stack: e instanceof Error ? e.stack ?? '' : '' });
      this.started = null;
      // the line goes on: each waiting card is told in its turn (it tries the start again), none is left waiting for a worker that is not coming
      const next = this.queue.next();
      if (next) this.#run(next.card, next.job);
      return;
    }
    // the build may have been stopped (and the worker replaced) while the engine was starting
    if (this.running !== r) return;
    const limit = job.kind === 'full' ? runawayMs(job.cpuSecs) : runawayMs(null);
    r.timer = setTimeout(() => this.#failRunning(
      `The build took longer than ${Math.round(limit / 1000)} seconds${job.cpuSecs ? ` (three times its estimate of ${Math.round(job.cpuSecs)} s)` : ''} and was stopped; this device may be too small for it`, ''), limit);
    const worker = this.worker;
    if (!worker) throw new Error('The engine started but has no worker to run the build.');
    Engine.post(worker, { type: "job", id, steps: job.steps, text: job.text, system: job.system, seed: job.seed, cells: job.cells, tier: job.tier, card: job.card ?? null });
  }

  /** Take what the worker said.  A message of a job that was stopped is not its card's any more. */
  /** @param {FromWorker} m */
  #heard(m) {
    if (m.type === 'ready') return; // said once, to `start`
    const fixId = typeof m.id === 'string' ? m.id : undefined; // a fix's id is a string, a job's a number
    const fix = fixId === undefined ? undefined : this.fixes.get(fixId);
    if (fixId !== undefined && fix) {
      this.fixes.delete(fixId);
      if (m.type === 'fixed') fix.resolve(m.text);
      else if (m.type === 'error') fix.reject(new Error(m.message));
      else fix.reject(new Error(`the worker answered a fix with a "${m.type}" message`));
      return;
    }
    const r = this.running;
    if (!r || m.id !== r.id) return;
    const { card, job } = r;
    const key = job.key;
    if (key === undefined) throw new Error('A running job has no cache key (Engine.ask gives every job one).');
    const current = this.versions.get(card) === job.version;
    const cached = this.cache.get(key) ?? {};
    if (m.type === 'progress') {
      if (current) this.#emit(card, { type: 'progress', version: job.version, fraction: m.fraction, label: m.label, done: m.done, total: m.total });
    } else if (m.type === 'checked') {
      this.#remember(key, { ...cached, checked: m.result });
      if (current) this.#emit(card, { type: 'checked', version: job.version, result: m.result, ms: m.ms });
    } else if (m.type === 'built') {
      const built = { meta: m.meta, positions: m.positions, colours: m.colours, stl: m.stl, ms: m.ms };
      // the cache keeps the arrays: a card that shows them copies what it needs
      this.#remember(key, { ...(this.cache.get(key) ?? {}), built });
      if (current) this.#emit(card, { type: 'built', version: job.version, kind: job.kind, ...built, cached: false });
      if (job.persist && job.kind === 'edit') {
        const kept = this.cache.get(key);
        if (kept) this.#draftStore().then((store) => store?.put(storeKey(this.manifestHash(), job.system, job.text), kept));
      }
      if (job.kind === 'full') this.#finishFresh(r, current); else this.#finish(r, current);
    } else if (m.type === 'idle') {
      this.#finish(r, current);
    } else if (m.type === 'error') {
      if (current) this.#emit(card, { type: 'failed', version: job.version, kind: job.kind, message: m.message, stack: m.stack });
      // a trap leaves the module in an unknown state: a fresh worker, from the compiled module
      this.#respawn(r, null);
    }
  }

  /** @param {NonNullable<Engine['running']>} r @param {boolean} current */
  #finish(r, current) {
    if (r.timer !== null) clearTimeout(r.timer);
    this.running = null;
    if (current) this.#emit(r.card, { type: 'idle', version: r.job.version, kind: r.job.kind });
    const next = this.queue.next();
    if (next) this.#run(next.card, next.job);
  }

  /** A full build is done and shown: its worker's heap is as large as the build was and never shrinks, so a fresh worker follows it (and the waiting line goes on with it). */
  /** @param {NonNullable<Engine['running']>} r @param {boolean} current */
  #finishFresh(r, current) {
    if (current) this.#emit(r.card, { type: 'idle', version: r.job.version, kind: r.job.kind });
    this.#respawn(r, null);
  }

  /** The running build is stopped: a new worker from the compiled module.  `why` 'stopped' tells its card it can be resumed. */
  /** @param {NonNullable<Engine['running']>} r @param {string | null} why */
  #respawn(r, why) {
    if (r.timer !== null) clearTimeout(r.timer);
    this.running = null;
    if (why === 'stopped') this.#emit(r.card, { type: 'stopped', version: r.job.version, kind: r.job.kind });
    this.started = this.#spawn().then(() => undefined);
    this.started.then(() => {
      const next = this.queue.next();
      if (next && !this.running) this.#run(next.card, next.job);
    }, (e) => this.#emit(r.card, { type: 'failed', version: r.job.version, kind: r.job.kind, message: `The engine did not restart: ${e.message}`, stack: e.stack ?? '' }));
  }

  /** @param {string} message @param {string} stack */
  #failRunning(message, stack) {
    const r = this.running;
    if (!r) return;
    this.#emit(r.card, { type: 'failed', version: r.job.version, kind: r.job.kind, message, stack });
    this.#respawn(r, null);
  }
}
