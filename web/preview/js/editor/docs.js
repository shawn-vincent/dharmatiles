// The page's documents (js/documents.js) wired to this page: the host's calls, the toasts, and the one thing
// the page derives from a document before anyone is told (the mode a .dmap is shown in).
//
// `docs` reads documents and makes every change to them.  `applyDocState` is the setter: the program's word on
// a document goes through it and only it, and only live.js (the event stream) imports it (tests/documents.test.js
// fails if another module does).

import { createDocuments } from '../documents.js';
import { failed } from '../problems.js';
import { host, toast } from './app.js';
import { say } from './inventory.js';
import { derivedMode } from './mode.js';
import { isPlates } from './stores.js';

const send = (name, body) => {
  const call = host[name];
  if (typeof call !== 'function') throw new Error(`the host has no call ${name}`);
  return call(body);
};

const report = {
  /** The file moved on under an edit: the refusal, in words, where the user is looking. */
  refusal: (label, r) => toast(`${label}: ${r.body?.error ?? 'the file changed'}`, null, { ms: 9000 }),
  failure: (label, r) => {
    if (r.status === 0) return; // the editor is disconnected: the chip says so, and nothing was written
    if (r.status === 200) { say(r.body, label); return; } // a mistake in the document, at its text
    if (!failed(label, r)) toast(`${label}: ${r.body?.error ?? 'refused'}`, null, { ms: 9000 });
  },
  note: (text) => toast(text, null, { ms: 7000 }),
};

export const { docs, receive: applyDocState } = createDocuments({ send, report, derive: (id) => derivedMode(id) });

/** A toast's Undo: the one history's, while the toast's change is still the last one; else it says so and writes nothing. */
export async function undoToast(id, entry) {
  if (!(await docs.undoIf(id, entry))) toast('A later change came after this one: Undo in the title bar goes back one change at a time', null, { ms: 7000 });
}

/** The plan of tile `t`: for a print list or a map, its document's (what the program last said, js/documents.js); for a tile, the summary's own. */
export const planOf = (t) => (!t ? null : isPlates(t) ? docs.get(t.id)?.plan ?? null : t.plan ?? null);
