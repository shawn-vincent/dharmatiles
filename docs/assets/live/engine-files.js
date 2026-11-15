// @ts-check
// One file of the engine, through the Cache API (engine.js asks for each: the module, the library).  The engine's files carry their
// hashes in their names, and the cache that keeps them is named by the manifest's hash (`dt-engine-<hash>`), so a copy kept for
// another engine is never looked in: when the manifest names a new hash the file is fetched, kept under the new name, and every
// other `dt-engine-` cache is removed.  The Cache API is a convenience, not a requirement: where it cannot be used (an insecure page,
// blocked site data) the file is fetched as it always was, and that is the one silence here, with its comment below.  A copy that
// cannot be kept (quota, a full disk) is said, once per file, through `say`: it costs the reader a fetch on every visit, which is
// worth knowing.  Pure of the page: the Cache API, `fetch` and the reporting are given.

/** The cache a manifest hash keeps its files in. @param {string} hash */
export const cacheName = (hash) => `dt-engine-${hash}`;

/**
 * @typedef {{
 *   caches: CacheStorage | undefined,
 *   fetch: (url: URL) => Promise<Response>,
 *   say: (message: string) => void,
 * }} Host  the page's Cache API (none where it is missing), its `fetch`, and where a failure to keep a copy is said
 */

/**
 * The file at `url`, from this engine's cache when its copy is there, else fetched (and kept, with the other engines' caches removed).
 * @param {Host} host @param {string} hash the manifest's hash @param {URL} url @returns {Promise<Response>}
 */
export async function engineFile(host, hash, url) {
  /** @type {Cache | null} */
  let kept = null;
  try {
    kept = (await host.caches?.open(cacheName(hash))) ?? null;
    const hit = await kept?.match(url);
    if (hit) return hit;
  } catch {
    // permitted silence: no Cache API here (an insecure page, blocked site data), so the file is fetched as usual
    kept = null;
  }
  const r = await host.fetch(url);
  if (r.ok && kept && host.caches) {
    try {
      await kept.put(url, r.clone());
      for (const k of await host.caches.keys()) if (k.startsWith('dt-engine-') && k !== cacheName(hash)) await host.caches.delete(k);
    } catch (e) {
      host.say(`The engine's file ${url.pathname.split('/').pop()} could not be kept for the next visit (${e instanceof Error ? e.message : String(e)}): it will be fetched again each time.`);
    }
  }
  return r;
}
