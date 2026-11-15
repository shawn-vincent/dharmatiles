// @ts-check
// The browser's keeper of kept builds: the one file that touches IndexedDB for them, for both workers that build in the page
// (the editor's, js/editor-worker.js, and the site's cards', docs/assets/live/worker.js).
//
// What it keeps is opaque.  The engine's `KeptBuilds` (dt-web kept.rs, a `dt_store::MemStore`) decides everything about a kept
// build: the key it is under, the bytes it is encoded as (`dt_store`'s own entry), how much is kept and which entry goes first.
// This file only stores what the engine tells it to and gives it back at the next start:
//
//   open     `persist()` first (so what the engine drops while loading is told to this keeper), then each stored record goes to
//            `kept.load(bytes, used)`; an entry another engine made, or one that is damaged, is dropped there and deleted here
//   follow   the engine calls `globalThis.dtKeptPut(id, bytes, used)` for an entry kept and `dtKeptDrop(id)` for one gone; each is
//            one write, in order
//
// A record is `{id, bytes, used}`: nothing in it is made or read here.  IndexedDB is async and a build is not, which is why the
// engine's memory store is in front: a build never waits on this file.
//
// It is a cache: when IndexedDB is missing or fails (a private window, blocked site data, a full disk) the worker keeps its builds
// in memory alone, and each place says so on the console.  That is the one silence here: nothing is ever lost but a build the
// next visit makes again.

const DB = 'dt-kept';
const STORE = 'builds';
/** The database the site's drafts were kept in before the engine kept them (records of another shape): removed once, it is only a cache. */
const OLD_DB = 'dt-drafts';

/** What of the engine's `KeptBuilds` this file uses. @typedef {{ load(bytes: Uint8Array, used: number): string, persist(): void }} Kept */
/** @typedef {{ id: string, bytes: Uint8Array, used: number }} KeptRecord */
/** What `openKept` gives back: whether the builds are kept between visits, and a promise of the writes made so far (a test waits on it). @typedef {{ persistent: boolean, flush: () => Promise<void> }} Opened */

/** @template T @param {IDBRequest<T>} req @returns {Promise<T>} */
const done = (req) => new Promise((resolve, reject) => { req.onsuccess = () => resolve(req.result); req.onerror = () => reject(req.error); });

/** @param {IDBTransaction} tx @returns {Promise<void>} */
const finished = (tx) => new Promise((resolve, reject) => { tx.oncomplete = () => resolve(); tx.onerror = () => reject(tx.error); tx.onabort = () => reject(tx.error); });

/**
 * Open the keeper for `kept`: load what an earlier visit kept and follow what the engine keeps from now on.
 * @param {Kept} kept the engine's store
 * @param {{ idb?: IDBFactory | undefined, say?: (words: string) => void }} [host] IndexedDB and where a failure is said
 * @returns {Promise<Opened>}
 */
export async function openKept(kept, { idb = globalThis.indexedDB, say = (w) => console.warn(w) } = {}) {
  const memoryOnly = { persistent: false, flush: () => Promise.resolve() };
  if (!idb) return memoryOnly; // no IndexedDB (a worker without it): memory alone, as the cache it is
  /** @type {IDBDatabase} */
  let db;
  try {
    const req = idb.open(DB, 1);
    req.onupgradeneeded = () => { req.result.createObjectStore(STORE, { keyPath: 'id' }); };
    db = await done(req);
  } catch (e) {
    say(`kept builds are not kept between visits (IndexedDB did not open: ${e instanceof Error ? e.message : String(e)})`);
    return memoryOnly;
  }
  // the old store of drafts goes; failing to delete it only leaves a cache nobody reads
  try { idb.deleteDatabase(OLD_DB); } catch (e) { say(`the old draft store was not removed: ${e instanceof Error ? e.message : String(e)}`); }

  /** Writes go one after another, in the order the engine told them; a failed one is said and the next goes on. @type {Promise<void>} */
  let writing = Promise.resolve();
  /** @param {string} what @param {(store: IDBObjectStore) => void} write */
  const queue = (what, write) => {
    writing = writing.then(async () => {
      try {
        const tx = db.transaction(STORE, 'readwrite');
        write(tx.objectStore(STORE));
        await finished(tx);
      } catch (e) {
        say(`a kept build was not ${what} (${e instanceof Error ? e.message : String(e)}); the next visit builds it again`);
      }
    });
  };
  // the engine's calls (kept.rs names them): the bytes are a view into the engine's memory, valid only until the call returns
  /** @type {any} */ (globalThis).dtKeptPut = (/** @type {string} */ id, /** @type {Uint8Array} */ bytes, /** @type {number} */ used) => {
    const record = { id, bytes: bytes.slice(), used };
    queue('kept', (store) => { store.put(record); });
  };
  /** @type {any} */ (globalThis).dtKeptDrop = (/** @type {string} */ id) => { queue('forgotten', (store) => { store.delete(id); }); };
  // told first: what the engine drops while it loads (another engine's entries) is deleted here too
  kept.persist();
  try {
    const records = /** @type {KeptRecord[]} */ (await done(db.transaction(STORE, 'readonly').objectStore(STORE).getAll()));
    for (const r of records) {
      const why = kept.load(r.bytes, r.used);
      if (why) {
        say(why);
        const id = r.id;
        queue('forgotten', (store) => { store.delete(id); }); // a damaged record would be said again at every start
      }
    }
  } catch (e) {
    say(`the kept builds of earlier visits could not be read (${e instanceof Error ? e.message : String(e)}); they are built again`);
  }
  return { persistent: true, flush: () => writing };
}
