// @ts-check
// Drafts kept between visits (IndexedDB), so a second visit shows the page's examples without starting the engine.
//
//   what      the low-tier build and the check's answer of an example as the page wrote it (never an edited text, never a full
//             build: a full build is tens of MB and a phone may not make one)
//   key       the engine's hash and the text (limits.js `storeKey`): a mesh an older engine made is a wrong tile, so an entry
//             of another engine is never found, and is deleted when the store is opened
//   bound     `MAX_ENTRIES` entries, the oldest used first out
//
// The store is a convenience: when IndexedDB is missing or fails (a private window, blocked site data, a full disk) the page
// works without it.  That is the one silence here, and each place says so.

/** How many drafts are kept: about 1 MB each, so about 40 MB at most. */
export const MAX_ENTRIES = 40;
const DB = 'dt-drafts';
const STORE = 'drafts';

/** @typedef {import('./engine.js').Kept} Kept */
/** @typedef {{key: string, engine: string, at: number, kept: Kept}} Entry */

/** @template T @param {IDBRequest<T>} req @returns {Promise<T>} */
const done = (req) => new Promise((resolve, reject) => { req.onsuccess = () => resolve(req.result); req.onerror = () => reject(req.error); });

/** @param {IDBTransaction} tx @returns {Promise<void>} */
const finished = (tx) => new Promise((resolve, reject) => { tx.oncomplete = () => resolve(); tx.onerror = () => reject(tx.error); tx.onabort = () => reject(tx.error); });

export class DraftStore {
  /** @param {string} engine the engine's hash @param {IDBFactory | undefined} [idb] */
  constructor(engine, idb = globalThis.indexedDB) {
    this.engine = engine;
    this.idb = idb;
    /** @type {Promise<IDBDatabase | null> | null} */
    this.opened = null;
  }

  /** The database, opened once and with the entries of other engines removed; null when there is none. */
  #open() {
    return (this.opened ??= this.#openNow().catch(() => null)); // a store that cannot open is no store: the page builds as it would have
  }

  async #openNow() {
    const idb = this.idb;
    if (!idb) return null;
    const req = idb.open(DB, 1);
    req.onupgradeneeded = () => { req.result.createObjectStore(STORE, { keyPath: 'key' }); };
    const db = await done(req);
    await this.#purge(db);
    return db;
  }

  /** Delete what another engine made. @param {IDBDatabase} db */
  async #purge(db) {
    const tx = db.transaction(STORE, 'readwrite');
    const store = tx.objectStore(STORE);
    const all = /** @type {Entry[]} */ (await done(store.getAll()));
    for (const e of all) if (e.engine !== this.engine) store.delete(e.key);
    await finished(tx);
  }

  /** The draft kept under `key`, or null. @param {string} key @returns {Promise<Kept | null>} */
  async get(key) {
    const db = await this.#open();
    if (!db) return null;
    try {
      const store = db.transaction(STORE, 'readonly').objectStore(STORE);
      const e = /** @type {Entry | undefined} */ (await done(store.get(key)));
      return e && e.engine === this.engine ? e.kept : null;
    } catch {
      return null; // a read that fails is a miss: the page builds it
    }
  }

  /** Keep `kept` under `key`; the least recently put go when there are too many.  Never fails the page. @param {string} key @param {Kept} kept */
  async put(key, kept) {
    const db = await this.#open();
    if (!db) return;
    try {
      const tx = db.transaction(STORE, 'readwrite');
      const store = tx.objectStore(STORE);
      store.put({ key, engine: this.engine, at: Date.now(), kept });
      const all = /** @type {Entry[]} */ (await done(store.getAll()));
      for (const e of oldestOver(all, MAX_ENTRIES)) store.delete(e.key);
      await finished(tx);
    } catch {
      // a write that fails (quota, a private window) only means the next visit builds the example again
    }
  }
}

/** The entries to delete so that `max` remain: the oldest. @param {Entry[]} all @param {number} max */
export const oldestOver = (all, max) => [...all].sort((a, b) => a.at - b.at).slice(0, Math.max(0, all.length - max));
