// @ts-check
// The browser host: how the page talks to `dharmatiles preview`.
// A desktop app supplies another object with the same methods; only this
// file knows the server's routes.  A tile is named by its id (the server's,
// never a position in a list).
//
// The contract is `js/api.d.ts`, generated from the server's Rust types
// (crates/dharmatiles/src/preview/api.rs, calls.rs): every call's request and
// answer.  Each method here takes its call's request type and answers its
// answer type, and `tsc` (npm test) checks this file, and the files that call
// it, against that file, so a field the server renames fails here and where it
// was used.  There is no other description of a route: no body is built by hand
// outside these methods.
//
// A call answers {status, body}, and for a failure the program's ServerError as
// `error` (body.error holds its words, beside what the refusal also says: the
// text a 409 lost to, the mistakes of a 422).  Status 0 is a lost connection.
// The reads answer their data or throw.
//
// The event stream: a snapshot, then one event per change, each with a
// revision.  When the stream drops it is opened again with the epoch (which
// run of the program) and the last revision seen: the program answers with
// what changed since, or a fresh snapshot.

import { serverErrorOf } from './server-error.js';
import { createRequests, SUPERSEDED } from './requests.js';

/** @typedef {import('./api').Calls} Calls */
/** @typedef {import('./api').Reads} Reads */
/** @template {keyof import('./api').Calls} R @typedef {import('./api').Req<R>} Req */
/** @template {keyof import('./api').Calls} R @typedef {import('./api').Res<R>} Res */
/** @typedef {import('./api').Snapshot} Snapshot */
/** @typedef {import('./api').StreamEvent} StreamEvent */
/** @typedef {import('./api').MessageJson} MessageJson */
/** @typedef {Extract<StreamEvent, { type: 'tier' }>} TierEvent */
/** @typedef {Extract<StreamEvent, { type: 'resource' }>} ResourceEvent */
/** @typedef {import('./api').TileSummary} TileSummary */

/**
 * What a call answers: the status, and the body (the answer, or for a failure
 * what the refusal says, with its words as `error`).
 * @template T
 * @typedef {{
 *   status: number,
 *   body: T & { error?: string, text?: string, errors?: import('./api').Mistake[], superseded?: boolean },
 *   error?: import('./server-error.js').ServerError,
 * }} Reply
 */

// fetch rejects with a TypeError when it cannot reach the program (down, or
// the connection dropped): that is a lost connection, shown by the chip, and
// the page repeats what it needs on reconnect.  Only that rejection, from
// fetch itself, is made an `Offline`; any other error, a TypeError from the
// page's own code included, is a failure and goes to the error panel.
export class Offline extends Error {
  /** @param {unknown} cause */
  constructor(cause) {
    super('the editor is disconnected', { cause });
    this.name = 'Offline';
  }
}
/** @param {unknown} e */
export const isOffline = (e) => e instanceof Offline;
const offline = isOffline;
/** `fetch`, whose network failure is an `Offline` and nothing else is.  Only `browserHost`'s `netFetch` calls it: it counts. */
/** @type {typeof fetch} */
async function rawFetch(...args) {
  try {
    return await fetch(...args);
  } catch (e) {
    if (e instanceof TypeError) throw new Offline(e);
    throw e;
  }
}

// What a failed reply says, as the page's own error for what it was doing
// (`what`), holding the program's ServerError as its cause: one error with
// both stacks.  A reply that is not the program's JSON has its words only.
/** @param {Response} r @param {string} what */
const replyError = async (r, what) => {
  const raw = await r.text();
  let body = null;
  try { body = JSON.parse(raw); } catch { /* not JSON: serverErrorOf makes the error from the words */ }
  const se = serverErrorOf(r.status, body, raw);
  return new Error(`${what}: ${se?.message}`, { cause: se });
};

// One reply to a call: {status, body}, and for a failure the program's
// ServerError as `error` (with body.error its words, for the places that say
// them beside what was asked).  Only a success may be something other than JSON.
/** @param {Response} r @param {string} path @returns {Promise<Reply<any>>} */
const reply = async (r, path) => {
  const raw = await r.text();
  let body = null;
  try { body = JSON.parse(raw); } catch (e) {
    if (r.ok) throw new Error(`${path}: the program answered ${r.status} with something that is not JSON`, { cause: e });
  }
  const error = serverErrorOf(r.status, body, raw);
  if (!error) return { status: r.status, body };
  return { status: r.status, body: { ...(body && typeof body === 'object' ? body : {}), error: error.message }, error };
};

/** The URL of a call: `/api/tiles/:id/save` with the tile's id put in. */
const urlOf = (/** @type {string} */ route, /** @type {string | undefined} */ id) => {
  const path = route.slice(route.indexOf(' ') + 1);
  return id === undefined ? path : path.replace(':id', encodeURIComponent(id));
};

// The calls whose answer is only worth taking if it is of the newest one asked: what a text says (its names, its
// check, what may be added at a place, a typed value's reading, a draft) and which plate or target is wanted.  The
// key says which requests replace which.  Every other call is made as it is asked: a write is answered into the
// document module by its revision (js/documents.js), never dropped.
/** @type {Record<string, (body: any, id?: string) => string>} */
const LATEST = {
  'POST /api/names': () => 'names',
  'POST /api/tokens': () => 'tokens',
  'POST /api/tiles/:id/check': (_b, id) => `check:${id}`,
  'POST /api/tiles/:id/offers': (_b, id) => `offers:${id}`,
  'POST /api/tiles/:id/draft': (_b, id) => `draft:${id}`,
  'POST /api/check-value': (b) => `check-value:${b.element}.${b.control}`,
  'POST /api/tiles/:id/printed': (_b, id) => `printed:${id}`,
  'POST /api/plates/show': (b) => `plates-show:${b.list}`,
  'POST /api/target': () => 'target',
};

/** What a call answers when a newer one of its key took its place: nothing happened, and nothing is said. */
const superseded = () => /** @type {any} */ ({ status: 0, body: { error: 'a newer request took its place', superseded: true } });

/** @param {ReturnType<typeof createRequests>} [requests] */
export function browserHost(requests = createRequests()) {
  /** Every request of the page is made here, so the layer counts it. @type {typeof fetch} */
  const netFetch = (...args) => requests.direct(() => rawFetch(...args));
  /** @type {string | null} */
  let token = null;
  const session = async () => {
    if (!token) {
      const r = await netFetch('/api/session', { cache: 'no-store' });
      if (!r.ok) throw await replyError(r, 'session');
      token = /** @type {import('./api').SessionRes} */ (await r.json()).token;
    }
    return token;
  };
  /**
   * One call of the editor: JSON in, {status, body} out, with the session token.
   * A reply that is not JSON has its words as the body's error.  Only a lost
   * connection is status 0.
   * @template {keyof Calls} R
   * @param {R} route @param {Req<R>} body @param {string} [id] the tile, for a call under `/api/tiles/:id/`
   * @returns {Promise<Reply<Res<R>>>}
   */
  const call = async (route, body, id) => {
    const key = LATEST[route]?.(body, id);
    if (key === undefined) return send(route, body, id);
    const r = await requests.latest(key, () => send(route, body, id));
    return r === SUPERSEDED ? superseded() : r;
  };
  /** @template {keyof Calls} R @param {R} route @param {Req<R>} body @param {string} [id] @returns {Promise<Reply<Res<R>>>} */
  const send = async (route, body, id) => {
    const path = urlOf(route, id);
    try {
      const r = await netFetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-DT-Token': await session() },
        body: JSON.stringify(body),
      });
      return await reply(r, path);
    } catch (e) {
      if (offline(e)) return /** @type {any} */ ({ status: 0, body: { error: 'the editor is disconnected' } });
      throw e;
    }
  };
  /** One read of the program with a query: {status, body}, as a call is. */
  const read = async (/** @type {string} */ path, /** @type {Record<string, string>} */ query) => {
    try {
      const r = await netFetch(`${path}?${new URLSearchParams(query)}`, { cache: 'no-store' });
      return await reply(r, path);
    } catch (e) {
      if (offline(e)) return { status: 0, body: { error: 'the editor is disconnected' } };
      throw e;
    }
  };
  /** @param {string} id @param {Req<'POST /api/tiles/:id/draft'>} body @returns {Promise<{status: number, buffer: ArrayBuffer} | Reply<{}>>} */
  const sendDraft = async (id, body) => {
    const path = urlOf('POST /api/tiles/:id/draft', id);
    try {
      const r = await netFetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-DT-Token': await session() },
        body: JSON.stringify(body),
      });
      if (r.ok) return { status: r.status, buffer: await r.arrayBuffer() };
      return await reply(r, path);
    } catch (e) {
      if (offline(e)) return { status: 0, body: { error: 'the editor is disconnected' } };
      throw e;
    }
  };
  /**
   * One step of a drag or a scrub: an edit and the draft of its text in one request.  The answer is the head's JSON after its
   * length (four bytes, big-endian), then the mesh (empty when the text could not be drafted: `head.draft` says why).
   * @param {string} id @param {Req<'POST /api/tiles/:id/step'>} body
   * @returns {Promise<{status: number, head: import('./api').StepHead, buffer: ArrayBuffer} | Reply<{}>>}
   */
  const sendStep = async (id, body) => {
    const path = urlOf('POST /api/tiles/:id/step', id);
    try {
      const r = await netFetch(path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'X-DT-Token': await session() },
        body: JSON.stringify(body),
      });
      if (!r.ok) return await reply(r, path);
      const all = await r.arrayBuffer();
      const n = new DataView(all).getUint32(0);
      const head = JSON.parse(new TextDecoder().decode(new Uint8Array(all, 4, n)));
      return { status: r.status, head, buffer: all.slice(4 + n) };
    } catch (e) {
      if (offline(e)) return { status: 0, body: { error: 'the editor is disconnected' } };
      throw e;
    }
  };
  /** @param {string} id @param {string} what */
  const tile = (id, what) => `/api/tiles/${encodeURIComponent(id)}/${what}`;
  return {
    // One loop: the stream is live once its first message is in; when it
    // drops or is refused, it is closed and opened again a second later, for
    // as long as the page is open.  The server back may be a new one, with a
    // new token: it is fetched afresh.
    /**
     * @param {{
     *   onSnapshot: (s: Snapshot) => void, onTile: (t: TileSummary) => void,
     *   onProgress: (id: string, p: import('./api').Progress) => void,
     *   onMessage: (m: MessageJson) => void, onState: (s: 'live' | 'offline') => void,
     *   onTier?: (m: TierEvent) => void,
     *   onResource?: (m: ResourceEvent) => void,
     *   onRev?: (rev: number, fresh: boolean) => void,
     * }} handlers
     */
    connect({ onSnapshot, onTile, onProgress, onMessage, onState, onTier, onResource, onRev }) {
      /** @type {string | null} */
      let epoch = null;
      /** @type {number | null} */
      let rev = null;
      const open = () => {
        const q = epoch != null && rev != null ? `?epoch=${epoch}&since=${rev}` : '';
        const es = new EventSource(`/api/events${q}`);
        es.onmessage = (e) => {
          // the stream's one boundary: what the program sends is its StreamEvent type (api.rs), and the page reads it as that
          const m = /** @type {StreamEvent} */ (JSON.parse(e.data));
          if (m.type !== 'progress' && m.type !== 'message' && m.type !== 'tier' && m.rev != null) rev = m.rev;
          if (m.type === 'snapshot') { epoch = m.epoch; onState('live'); onSnapshot(m); }
          else if (m.type === 'resumed') onState('live');
          else if (m.type === 'tile') onTile(m.tile);
          else if (m.type === 'progress') onProgress(m.id, m.progress);
          else if (m.type === 'message') onMessage(m.message);
          else if (m.type === 'tier') onTier?.(m);
          else if (m.type === 'resource') onResource?.(m);
          // what the page holds now includes this revision (a snapshot starts the count again: another run's revisions restart)
          if (m.type !== 'progress' && m.type !== 'message' && m.type !== 'tier' && m.rev != null) onRev?.(m.rev, m.type === 'snapshot');
        };
        es.onerror = () => { es.close(); token = null; onState('offline'); setTimeout(open, 1000); };
      };
      open();
    },
    /**
     * The tile's binary mesh, of the revision of its document the page asks for (its summary's `docRev`); null when the
     * program's mesh is of another revision (a build is on its way: the next summary says which), never that mesh.
     * @param {string} id @param {number} docRev @returns {Promise<ArrayBuffer | null>}
     */
    async fetchMesh(id, docRev) {
      const r = await netFetch(`${tile(id, 'mesh')}?docRev=${docRev}`, { cache: 'no-store' });
      if (r.status === 409) return null;
      if (!r.ok) throw await replyError(r, 'mesh');
      return r.arrayBuffer();
    },
    /**
     * The tiers the page wants (`super-low`, `low`, `full` of a tile as a map's piece builds it), the whole set of a
     * scope: each post replaces the last, and the program cancels what is no longer wanted (js/tiers.js).  Answers
     * the program's key for each, in order.
     * @param {Req<'POST /api/tiers/want'>} body
     */
    tiersWant: (body) => call('POST /api/tiers/want', body),
    /**
     * A built tier, by the key the program announced and the revision it said: null when what it holds is of another
     * revision (never that one).
     * @param {string} key @param {number} docRev @returns {Promise<ArrayBuffer | null>}
     */
    async tierResult(key, docRev) {
      const r = await netFetch(`/api/tiers/result?${new URLSearchParams({ key, docRev: String(docRev) })}`, { cache: 'no-store' });
      if (r.status === 409 || r.status === 404) return null;
      if (!r.ok) throw await replyError(r, 'the tier');
      return r.arrayBuffer();
    },
    /** One stage's sketch while it builds (null once the build has moved on). @param {string} id @param {number} sketch */
    async fetchSketch(id, sketch) {
      const r = await netFetch(tile(id, `sketch?id=${sketch}`), { cache: 'no-store' });
      if (r.status === 404) return null; // the build moved on: its sketches are gone
      if (!r.ok) throw await replyError(r, 'sketch');
      return r.arrayBuffer();
    },
    /**
     * Build a tile not built yet; `debug` says whether the debug layer is on for it (its marks are collected only then).
     * @param {string} id @param {{debug?: boolean}} [opts]
     */
    async build(id, { debug } = {}) {
      let r;
      const q = typeof debug === 'boolean' ? `?debug=${debug ? 1 : 0}` : '';
      try { r = await netFetch(tile(id, `build${q}`), { method: 'POST', headers: { 'X-DT-Token': await session() } }); } catch (e) {
        if (offline(e)) return; // offline: asked again on reconnect
        throw e;
      }
      if (r.status >= 500) throw await replyError(r, 'build');
    },
    /** The text of a document of a tile, with its revision (null: no such tile or file any more). @param {string} id @param {string} [file] @returns {Promise<import('./api').TextRes | null>} */
    async text(id, file) {
      const q = file ? `?file=${encodeURIComponent(file)}` : '';
      const r = await netFetch(tile(id, `text${q}`), { cache: 'no-store' });
      if (r.status === 404) return null; // no such tile or file (any more)
      if (!r.ok) throw await replyError(r, 'text');
      return r.json();
    },
    /** @returns {Promise<import('./api').AboutRes>} */
    async about() {
      const r = await netFetch('/api/about', { cache: 'no-store' });
      if (!r.ok) throw await replyError(r, 'about');
      return r.json();
    },
    /** The element schema. */
    async elements() {
      const r = await netFetch('/api/elements', { cache: 'force-cache' });
      if (!r.ok) throw await replyError(r, 'elements');
      return r.json();
    },
    /** The tile's nodes with their controls, and the language's words. */
    async language() {
      const r = await netFetch('/api/language', { cache: 'force-cache' });
      if (!r.ok) throw await replyError(r, 'language');
      return r.json();
    },
    /** The names the text makes, by the program's parser. @param {string} text */
    names: (text) => call('POST /api/names', { text }),
    /** How the text is coloured, by the language's one set of rules (dt-lang's highlighter). @param {string} text */
    tokens: (text) => call('POST /api/tokens', { text }),
    /** What the text would build. @param {string} id @param {Req<'POST /api/tiles/:id/check'>} body */
    check: (id, body) => call('POST /api/tiles/:id/check', body, id),
    /** What the Add menu may offer at a place of the stack. @param {string} id @param {Req<'POST /api/tiles/:id/offers'>} body */
    offers: (id, body) => call('POST /api/tiles/:id/offers', body, id),
    /** What the reader refuses in a typed value. @param {string} element @param {string} control @param {string} typed */
    checkValue: (element, control, typed) => call('POST /api/check-value', { element, control, typed }),
    /** What each written length prints on the build target. @param {string} id @param {Req<'POST /api/tiles/:id/printed'>} body */
    printed: (id, body) => call('POST /api/tiles/:id/printed', body, id),
    /** A length with one end set so that it prints `mm`, written in its own unit. @param {Req<'POST /api/printed-set'>} body */
    printedSet: (body) => call('POST /api/printed-set', body),
    /**
     * A quick build of the text (with the debug layer's marks when `debug`): {status, buffer} (the mesh),
     * 409 when a newer draft took its place.
     * @param {string} id @param {Req<'POST /api/tiles/:id/draft'>} body
     * @returns {Promise<{status: number, buffer: ArrayBuffer} | Reply<{}>>}
     */
    async draft(id, body) {
      const r = await requests.latest(`draft:${id}`, () => sendDraft(id, body));
      return r === SUPERSEDED ? superseded() : r;
    },
    /**
     * A step of a drag or a scrub: the edit made to the text it carries and the draft of what it made, in one request.
     * {status, head, buffer}; 409 when the text moved on since its nodes were read, 422 when the edit cannot be made.
     * @param {string} id @param {Req<'POST /api/tiles/:id/step'>} body
     * @returns {Promise<{status: number, head: import('./api').StepHead, buffer: ArrayBuffer} | Reply<{}>>}
     */
    step: (id, body) => sendStep(id, body),
    /** The requests in flight, for a test or a page that waits for quiet. */
    requests,
    /** Write the text to its file, if the file is still at `docRev` (else 409 with its text and docRev). @param {string} id @param {Req<'POST /api/tiles/:id/save'>} body */
    save: (id, body) => call('POST /api/tiles/:id/save', body, id),
    /** Rename a tile's file. @param {string} id @param {string} name */
    rename: (id, name) => call('POST /api/tiles/:id/rename', { name }, id),
    /** Copy a library tile to My tiles. @param {string} id */
    mine: (id) => call('POST /api/tiles/:id/mine', {}, id),
    /** Delete a tile's file from a folder the preview lists (Undo of that copy). @param {string} id */
    deleteTile: (id) => call('POST /api/tiles/:id/delete', {}, id),
    /** A failure of the page's own, to join the stream. @param {Req<'POST /api/messages'>} body */
    reportMessage: (body) => call('POST /api/messages', body),
    /** The tile's open messages as text, the program's one renderer (Copy all). @param {string} id */
    async messagesText(id) {
      const r = await netFetch(`/api/messages?${new URLSearchParams({ tile: id, state: 'open', format: 'text' })}`, { cache: 'no-store' });
      if (!r.ok) throw await replyError(r, 'messages');
      return r.text();
    },
    /** One message as text, the program's one renderer (a row's details). @param {number} seq */
    async messageText(seq) {
      const r = await netFetch(`/api/messages?${new URLSearchParams({ seq: String(seq), format: 'text' })}`, { cache: 'no-store' });
      if (!r.ok) throw await replyError(r, 'messages');
      return r.text();
    },
    /** The filled-in new-issue form for a message that means the program went wrong, made by the program from the message's own text (Report this defect). @param {number} seq @param {{ env: string, leaveSourceOut: boolean }} o @returns {Promise<import('./api').Report>} */
    async messageReport(seq, { env, leaveSourceOut }) {
      const q = new URLSearchParams({ seq: String(seq), env, ...(leaveSourceOut ? { source: '0' } : {}) });
      const r = await netFetch(`/api/messages/report?${q}`, { cache: 'no-store' });
      if (!r.ok) throw await replyError(r, 'report');
      return r.json();
    },
    /** A new tile in My tiles (or in the folder `into`). @param {string} name @param {string} size @param {{root: number, dir: string}} [into] */
    newTile: (name, size, into) => call('POST /api/new', { name, size, ...(into ? { root: into.root, dir: into.dir } : {}) }),
    /** Show the program's folder chooser; the folder chosen is a place from now on. */
    addPlace: () => call('POST /api/places/add', {}),
    /** Take an added place off the list (its folder is left alone). @param {number} root */
    removePlace: (root) => call('POST /api/places/remove', { root }),
    /** A folder of place `root`, named by its path below it. @param {number} root @param {string} dir @param {string} name */
    newFolder: (root, dir, name) => call('POST /api/folders/new', { root, dir, name }),
    /** @param {number} root @param {string} dir @param {string} name */
    renameFolder: (root, dir, name) => call('POST /api/folders/rename', { root, dir, name }),
    /** @param {number} root @param {string} dir */
    trashFolder: (root, dir) => call('POST /api/folders/trash', { root, dir }),
    /** A tile into a folder. @param {string} id @param {{root: number, dir: string}} into */
    moveTile: (id, into) => call('POST /api/tiles/:id/move', { root: into.root, dir: into.dir }, id),
    /** A copy beside the tile. @param {string} id */
    duplicateTile: (id) => call('POST /api/tiles/:id/duplicate', {}, id),
    /** To the Trash. @param {string} id */
    trashTile: (id) => call('POST /api/tiles/:id/trash', {}, id),
    /** Put back what a trash answered `undo: n` for. @param {number} undo */
    undoTrash: (undo) => call('POST /api/trash/undo', { undo }),
    /** A text edit (a control set, a node inserted or removed). @param {Req<'POST /api/edit'>} body */
    edit: (body) => call('POST /api/edit', body),
    /** Choose the build target (base system, printer profile) for every build. @param {Req<'POST /api/target'>} body */
    setTarget: (body) => call('POST /api/target', body),
    /** What the installed slicers have (none named), or one slicer's models. @param {string} [slicer] */
    printersScan: (slicer) => call('POST /api/printers/scan', slicer ? { slicer } : {}),
    /** @param {Req<'POST /api/printers/import'>} body */
    printersImport: (body) => call('POST /api/printers/import', body),
    /** Write STL / 3MF ({where: true} asks where it would go). @param {string} id @param {Req<'POST /api/tiles/:id/export'>} body */
    exportTile: (id, body) => call('POST /api/tiles/:id/export', body, id),
    /** Show a written file in the file manager. @param {string} path */
    reveal: (path) => call('POST /api/reveal', { path }),
    /** The plate of a print list the view shows (from 0). @param {string} list @param {number} plate */
    platesShow: (list, plate) => call('POST /api/plates/show', { list, plate }),
    /** Set a word of the list's `plates` line; value null removes it. @param {Req<'POST /api/plates/set'>} body */
    platesSet: (body) => call('POST /api/plates/set', body),
    /** Build every piece in full and write the plates. @param {string} list */
    platesExport: (list) => call('POST /api/plates/export', { list }),
    /** @param {Req<'POST /api/lists/new'>} body */
    listsNew: (body) => call('POST /api/lists/new', body),
    /** One more copy of a tile in a list (a new list when none is named). @param {Req<'POST /api/lists/add'>} body */
    listsAdd: (body) => call('POST /api/lists/add', body),
    /** @param {Req<'POST /api/lists/count'>} body */
    listsCount: (body) => call('POST /api/lists/count', body),
    /** @param {Req<'POST /api/lists/remove'>} body */
    listsRemove: (body) => call('POST /api/lists/remove', body),
    /** Undo or Redo: a text written back, if the file is still at docRev. @param {Req<'POST /api/lists/restore'>} body */
    listsRestore: (body) => call('POST /api/lists/restore', body),
    /** @param {Req<'POST /api/maps/add'>} body */
    mapsAdd: (body) => call('POST /api/maps/add', body),
    /** @param {Req<'POST /api/maps/set'>} body */
    mapsSet: (body) => call('POST /api/maps/set', body),
    /** @param {Req<'POST /api/maps/batch'>} body */
    mapsBatch: (body) => call('POST /api/maps/batch', body),
    /** @param {Req<'POST /api/maps/dup'>} body */
    mapsDup: (body) => call('POST /api/maps/dup', body),
    /** @param {Req<'POST /api/maps/remove'>} body */
    mapsRemove: (body) => call('POST /api/maps/remove', body),
    /** @param {Req<'POST /api/maps/pin'>} body */
    mapsPin: (body) => call('POST /api/maps/pin', body),
    /** @param {Req<'POST /api/maps/unpin'>} body */
    mapsUnpin: (body) => call('POST /api/maps/unpin', body),
    /** @param {Req<'POST /api/maps/unpin-all'>} body */
    mapsUnpinAll: (body) => call('POST /api/maps/unpin-all', body),
    /** One good print of a tile. @param {Req<'POST /api/inventory/have'>} body */
    inventoryHave: (body) => call('POST /api/inventory/have', body),
    /** The sheet after a print. @param {Req<'POST /api/inventory/record'>} body */
    inventoryRecord: (body) => call('POST /api/inventory/record', body),
    /** One of a piece lost. @param {Req<'POST /api/inventory/gone'>} body */
    inventoryGone: (body) => call('POST /api/inventory/gone', body),
    /** A tile has another path. @param {Req<'POST /api/inventory/relink'>} body */
    inventoryRelink: (body) => call('POST /api/inventory/relink', body),
    /** A piece, with its recorded seed, into a print list. @param {Req<'POST /api/inventory/reprint'>} body */
    inventoryReprint: (body) => call('POST /api/inventory/reprint', body),
  };
}
