// @ts-check
// The page's documents (js/documents.js) wired to this page: the host's calls and the toasts.
//
// `docs` reads documents and makes every change to them.  `applyDocState` is the setter: the program's word on
// a document goes through it and only it, and only live.js (the event stream) imports it (tests/documents.test.js
// fails if another module does).

import { createDocuments } from '../documents.js';
import { failed } from '../problems.js';
import { host, toast } from './app.js';
import { say } from './inventory.js';
import { isPlates } from './stores.js';

/** @typedef {import('../host.js').Reply<any>} Reply  what a call answered */
/** @typedef {import('./stores.js').ServerTile} Tile */

/** The calls a document's steps name (js/steps.js), each made with the list and the revision the document supplies. @type {Record<string, (body: any) => Promise<Reply>>} (a step's body is typed by its step, js/steps.js) */
const calls = {
  listsCount: host.listsCount, listsRemove: host.listsRemove, listsRestore: host.listsRestore,
  mapsAdd: host.mapsAdd, mapsSet: host.mapsSet, mapsBatch: host.mapsBatch, mapsDup: host.mapsDup, mapsRemove: host.mapsRemove,
  mapsPin: host.mapsPin, mapsUnpin: host.mapsUnpin, mapsUnpinAll: host.mapsUnpinAll, platesSet: host.platesSet,
};
/** @param {string} name @param {any} body (typed by the step that made it, js/steps.js) */
const send = (name, body) => {
  if (name === 'save') return host.save(body.tile, { text: body.text, docRev: body.docRev, file: body.file });
  const call = calls[name];
  if (!call) throw new Error(`a document step named a call the page does not have: ${name}`);
  return call(body);
};

const report = {
  /** The file moved on under an edit: the refusal, in words, where the user is looking. */
  refusal: (/** @type {string} */ label, /** @type {Reply} */ r) => toast(`${label}: ${r.body?.error ?? 'the file changed'}`, null, { ms: 9000 }),
  failure: (/** @type {string} */ label, /** @type {Reply} */ r) => {
    if (r.status === 0) return; // the editor is disconnected: the chip says so, and nothing was written
    if (!failed(label, r)) say(r, label); // a mistake in the document (422), at its text, or the program's refusal in words
  },
  note: (/** @type {string} */ text) => toast(text, null, { ms: 7000 }),
};

export const { docs, receive: applyDocState } = createDocuments({ send, report });

/** A toast's Undo: the one history's, while the toast's change is still the last one; else it says so and writes nothing. */
/** @param {string} id @param {import('../documents.js').Entry} entry */
export async function undoToast(id, entry) {
  if (!(await docs.undoIf(id, entry))) toast('A later change came after this one: Undo in the title bar goes back one change at a time', null, { ms: 7000 });
}

/** @typedef {import('../api').TilePlan} TilePlan */
/**
 * The plan of tile `t`: for a print list or a map, its document's (what the program last said, js/documents.js); for a tile, the summary's own.
 * @param {Tile | null | undefined} t
 * @returns {TilePlan | null}
 */
export const planOf = (t) => (!t ? null : isPlates(t) ? docs.get(t.id)?.plan ?? null : t.plan ?? null);

/** Can the document be edited, as the program says: a tile or a model says nothing (it can), a print list or the inventory says. */
export const writableOf = (/** @type {Tile | null | undefined} */ t) => { const p = planOf(t); return p?.kind === 'plates' || p?.kind === 'inventory' ? p.writable : true; };

/** @typedef {import('../api').PlatesPanel} PlatesPanel */
/**
 * The plates' panel of a print list's plan: its settings and, once it plans, the layout (null for a document with no such plan).
 * @param {Tile | null | undefined} t
 * @returns {PlatesPanel | null}
 */
export const panelOf = (t) => { const p = planOf(t); return p?.kind === 'plates' ? p.plates : null; };

/** @typedef {import('../api').PlatesPlanned} PlatesPlanned */
/**
 * The plates' panel of a list the program has laid out (null while it has not, or the list does not plan).
 * @param {Tile | null | undefined} t
 * @returns {PlatesPlanned | null}
 */
export const plannedOf = (t) => { const p = panelOf(t); return p?.planned ? p : null; };
