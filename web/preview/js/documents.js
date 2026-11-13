// The document module (docs/preview.html, "One owner for each document"): for each map or print list the page
// shows, what the program last said it holds, and the only way to change what the page holds or the file.
//
//   state    `{id, docRev, plan}` per document: `docRev` is the revision of the file the program read (a number
//            that only rises), `plan` what the program made of that text (the map's `scene`, the plates).  Read
//            with `docs.get(id)`; set by nothing but `receive`, the one front door every state the program
//            sends goes through (the event stream's tile events and snapshots, the answer to an edit).
//   order    `receive` takes a state only if its `docRev` is newer than the one held; the same `docRev` is a
//            later part of the same plan (the plates after the scene) and is merged.  An answer that arrives
//            late, from a stream or a call, cannot put an older document back.
//   actions  `docs.edit(id, build)` is every change to a document: `build` is called when the action's turn
//            comes (actions on one document run one after another) with the state as it is then, and answers
//            the calls to make.  Each call names the document and the `docRev` held when it is made, and takes
//            the answer into the state before the next.  An action holds ids, never a piece: it looks the
//            piece up in the state it is given.  A call the program refuses because the file has moved on
//            (409) is shown, takes the program's current state, and ends the action and the document's history.
//   history  every change made is an entry of one history (doc-history.js); Undo and Redo are calls too.
//
// No DOM and no network of its own: `send(name, body)` is the host's call, `report` says what went wrong where
// the user is looking, `derive(id, state)` is what the page derives from a new state before anyone is told,
// and `subscribe` tells each listener once per state taken.

import { createHistory } from './doc-history.js';

/**
 * @param {{send: (name: string, body: object) => Promise<{status: number, body: any}>,
 *   report?: {refusal(label: string, r: object): void, failure(label: string, r: object): void, note(text: string): void},
 *   derive?: (id: string, state: object) => void,
 *   history?: ReturnType<typeof createHistory>}} deps
 * @returns {{docs: object, receive: (state: object) => boolean}} the readers and actions, and the setter apart from them
 */
/** A state is read, never written: frozen all the way down. */
function deepFreeze(v) {
  if (v && typeof v === 'object' && !Object.isFrozen(v)) {
    Object.freeze(v);
    for (const k of Object.keys(v)) deepFreeze(v[k]);
  }
  return v;
}

export function createDocuments({ send, report = {}, derive = () => {}, history = createHistory() }) {
  const held = new Map();
  const subs = new Set();
  const queues = new Map();
  const say = {
    refusal: report.refusal ?? (() => {}),
    failure: report.failure ?? (() => {}),
    note: report.note ?? (() => {}),
  };

  const notify = (id) => { for (const s of [...subs]) s(id); };

  function commit(id, state) {
    held.set(id, deepFreeze(state));
    derive(id, state);
    notify(id);
  }

  /**
   * Take a state the program sent: `{id, docRev, plan?, scene?, source?}`.  False when it is older than what is
   * held.  `source` is the text of the file at that revision (`{tile, own, file, text, writable}`): the source of
   * a tile or a part of it, or of a map or print list, is a document like the rest, and what is on disk is
   * what the program last said, here and nowhere else.  A library file has no revision (0) and is read only:
   * what it says now replaces what was held.
   */
  function receive(state) {
    const { id, docRev } = state;
    if (typeof id !== 'string' || !Number.isInteger(docRev)) throw new Error(`a document state needs an id and a docRev: ${JSON.stringify({ id, docRev })}`);
    const h = held.get(id);
    if (h && docRev < h.docRev) return false;
    let plan = state.plan ?? h?.plan ?? null; // null until the program has planned the document: a source alone is not a plan
    // the same text, planned further: the plates come after the scene
    if (h && docRev === h.docRev && state.plan) plan = { ...h.plan, ...state.plan };
    if (state.scene) plan = { ...plan, scene: state.scene };
    // the source carries the revision its text is of (`rev`): the state's own docRev may be later (the program says an
    // outside change happened) without the text having been read, and a save is made against the text's revision
    let source = h?.source;
    const got = state.source;
    if (got) {
      const old = h?.source;
      const newer = !old || docRev > old.rev || docRev === 0;
      const same = old && docRev === old.rev && got.text === old.text && got.file === old.file && got.writable === old.writable && got.tile === old.tile && got.own === old.own;
      if (newer) source = { ...got, rev: docRev };
      else if (!same && docRev < old.rev) source = old; // an older reading of the file: not taken
    }
    if (h && docRev === h.docRev && plan === h.plan && source === h.source) return true;
    commit(id, { id, docRev, plan, ...(source ? { source } : {}) });
    return true;
  }

  /** Run `fn` after the actions already waiting on document `id`. */
  function enqueue(id, fn) {
    const run = (queues.get(id) ?? Promise.resolve()).then(fn);
    queues.set(id, run.catch(() => {}));
    return run;
  }

  /** One call's answer: its body when it went through (taken into the state), else null after saying why. */
  async function call(id, label, name, body) {
    const s = held.get(id);
    const r = await send(name, { ...body, list: id, docRev: s?.docRev });
    if (r.status === 200) {
      if (r.body?.docRev != null) receive({ id, docRev: r.body.docRev, scene: r.body.scene ?? undefined });
      if (r.body?.note) say.note(r.body.note);
      return r.body;
    }
    if (r.status === 409 && r.body?.docRev != null) {
      // the file moved on under this edit: the program's current state is taken, the refusal shown
      receive({ id, docRev: r.body.docRev, scene: r.body.scene ?? undefined });
      history.clear(id);
      notify(id);
      say.refusal(label, r);
      return null;
    }
    say.failure(label, r);
    return null;
  }

  const docs = {
    /** The state of document `id` as it is now (frozen), or null. */
    get: (id) => held.get(id) ?? null,
    /** The scene of document `id` as it is now: what a map draws. */
    scene: (id) => held.get(id)?.plan?.scene ?? null,
    /** `fn(id)` after each state taken; returns the way to stop. */
    subscribe(fn) { subs.add(fn); return () => subs.delete(fn); },
    history,

    /**
     * One change to document `id`.  `build(state)` runs when the change's turn comes and answers `{label,
     * steps: [{name, body}]}` (a body may be a function of the state as it is then) or null for nothing to do.
     * Resolves `{last, entry, note}` (the last answer, the history entry made, and the action's own `note`), or null when nothing was made.
     */
    edit(id, build) {
      return enqueue(id, async () => {
        const plan = build(held.get(id) ?? null);
        if (!plan?.steps?.length) return null;
        let first = null, last = null;
        for (const step of plan.steps) {
          const body = typeof step.body === 'function' ? step.body(held.get(id)) : step.body;
          if (body === null) break; // the step found nothing to do in the state it was given
          const b = await call(id, plan.label, step.name, body);
          if (!b) break;
          first ??= b;
          last = b;
        }
        if (!last) return null;
        const entry = { label: plan.label, before: first.before, after: last.text, docRev: last.docRev };
        history.push(id, entry);
        notify(id);
        return { last, entry, note: plan.note };
      });
    },

    /**
     * Read the source of document `id` (`read()` is the program's text of the file: `{file, text, writable, docRev}`)
     * and take it in its turn.  `meta` says whose it is: `{tile, own}` (own: the tile's own file, not one it uses).
     * `accept(v)` is asked when it has been read and says whether to take it (unsaved changes on the text held say no).
     * Resolves the state held after it, or null when nothing was read or taken.
     */
    loadSource(id, meta, read, accept = () => true) {
      return enqueue(id, async () => {
        const v = await read();
        if (!v || !accept(v)) return null; // nothing there, or the page has changes of its own on the text it holds: what was read is not taken
        receive({ id, docRev: v.docRev, source: { tile: meta.tile, own: meta.own, file: v.file, text: v.text, writable: v.writable } });
        notify(id);
        return held.get(id) ?? null;
      });
    },

    /**
     * Write `text` as the source of document `id`, made against the revision held, in its turn after the other
     * actions on it.  Resolves `{status}`: `saved` (the new state is held), `conflict` (the file moved on: the
     * disk's text and revision are held, and `disk` is its text, for the user to choose), `unwritable` (nothing
     * held, or a read only file), or `failed` (said).
     */
    saveSource(id, text, label = 'Could not save') {
      return enqueue(id, async () => {
        const h = held.get(id);
        const src = h?.source;
        if (!src?.writable) return { status: 'unwritable' };
        const r = await send('save', { tile: src.tile, file: src.own ? undefined : src.file, text, docRev: src.rev });
        if (r.status === 200) {
          receive({ id, docRev: r.body.docRev, source: { ...src, text } });
          notify(id);
          return { status: 'saved', docRev: r.body.docRev };
        }
        if (r.status === 409 && r.body?.docRev != null) {
          receive({ id, docRev: r.body.docRev, source: { ...src, text: r.body.text ?? '' } });
          notify(id);
          return { status: 'conflict', disk: r.body.text ?? '', docRev: r.body.docRev };
        }
        say.failure(label, r);
        return { status: 'failed', reply: r };
      });
    },

    /** A change made by a call outside the document's own page (a tile put on a list): its answer joins the history. */
    adopt(id, label, body) {
      if (body.docRev == null || body.before == null) return null;
      const entry = { label, before: body.before, after: body.text, docRev: body.docRev };
      history.push(id, entry);
      if (held.has(id) && body.scene) receive({ id, docRev: body.docRev, scene: body.scene });
      notify(id);
      return entry;
    },

    /** Undo (`dir` -1) or Redo (1) of document `id`: the text written back, against the revision this history left the file at. */
    step(id, dir) {
      return enqueue(id, async () => {
        const entry = dir < 0 ? history.nextUndo(id) : history.nextRedo(id);
        if (!entry) return false;
        const label = dir < 0 ? 'Could not undo' : 'Could not redo';
        const r = await send('listsRestore', { list: id, text: dir < 0 ? entry.before : entry.after, docRev: history.head(id) });
        if (r.status === 200) {
          receive({ id, docRev: r.body.docRev, scene: r.body.scene ?? undefined });
          history.moved(id, dir, r.body.docRev);
          notify(id);
          return true;
        }
        if (r.status === 409 && r.body?.docRev != null) {
          receive({ id, docRev: r.body.docRev, scene: r.body.scene ?? undefined });
          history.clear(id);
          notify(id);
          say.refusal(label, r);
          return false;
        }
        say.failure(label, r);
        return false;
      });
    },
    undo: (id) => docs.step(id, -1),
    redo: (id) => docs.step(id, 1),
    /** A toast's Undo: only while its change is still the last one; false (and nothing written) otherwise. */
    undoIf(id, entry) {
      if (history.nextUndo(id) !== entry) return Promise.resolve(false);
      return docs.undo(id);
    },
    canUndo: (id) => history.canUndo(id),
    canRedo: (id) => history.canRedo(id),
    /** Resolves when every action waiting on `id` is done (tests, and the editor's own waits). */
    idle: (id) => queues.get(id) ?? Promise.resolve(),
  };
  return { docs, receive };
}
