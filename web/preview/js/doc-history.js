// The one history (docs/preview.html, "Undo and Redo"): for each document, the changes made to it, newest
// last, each `{label, before, after, docRev, ...}`: the text it replaced, the text it wrote and the revision
// of the file that write made.  Edits of a map or a print list, steps of the source text and the toast's Undo
// all read and write this and nothing else; the title bar's Undo and Redo and ⌘Z drive it.
//
// A document has `at`, how many of its changes are in force: Undo takes back entry `at - 1` (writes its
// `before`), Redo makes entry `at` again (writes its `after`).  A new change drops what was undone.
//
// For a document the program owns (a map, a print list) `head` is the revision the file has after the last
// change this history made or took back: the next Undo or Redo is made against it, so a file changed by
// anyone else since is refused by the program (and `clear` ends a history that can no longer be trusted).
// No DOM, no network: the callers write the texts (documents.js for a map, document.js for the source pane).

import { signal } from '@preact/signals-core';

const LIMIT = 400;

export function createHistory() {
  const docs = new Map(); // key -> {entries, at, head}
  // a signal that every change to any history bumps, and every read follows: what draws a history (the title bar's Undo and Redo and their menus) is drawn again when it changes
  const version = signal(0);
  const changed = () => { version.value += 1; };
  const of = (key) => {
    void version.value;
    let d = docs.get(key);
    if (!d) { d = { entries: [], at: 0, head: null }; docs.set(key, d); }
    return d;
  };
  return {
    /** The change just made: `{label, before, after, docRev?}`; what was undone is dropped. */
    push(key, entry) {
      const d = of(key);
      d.entries.length = d.at;
      d.entries.push(entry); // the object itself: a toast's Undo finds its change by it
      d.at = d.entries.length;
      if (d.entries.length > LIMIT) { d.entries.shift(); d.at -= 1; }
      if (entry.docRev != null) d.head = entry.docRev;
      changed();
    },
    /** The last change made is one more step of the same: its `after` (and revision) are now these (typing). */
    amend(key, patch) {
      const d = of(key);
      if (!d.at) return false;
      Object.assign(d.entries[d.at - 1], patch);
      if (patch.docRev != null) d.head = patch.docRev;
      changed();
      return true;
    },
    entries: (key) => of(key).entries,
    at: (key) => of(key).at,
    last: (key) => of(key).entries[of(key).at - 1] ?? null,
    canUndo: (key) => of(key).at > 0,
    canRedo: (key) => of(key).at < of(key).entries.length,
    /** The entry Undo takes back (null: none). */
    nextUndo: (key) => of(key).entries[of(key).at - 1] ?? null,
    /** The entry Redo makes again (null: none). */
    nextRedo: (key) => of(key).entries[of(key).at] ?? null,
    /** The revision the next Undo or Redo is made against (null: not a document the program owns). */
    head: (key) => of(key).head,
    /** Undo or Redo was written: the pointer moves and the file's revision is now `rev`. */
    moved(key, dir, rev = null) {
      const d = of(key);
      d.at = Math.max(0, Math.min(d.entries.length, d.at + dir));
      if (rev != null) d.head = rev;
      changed();
    },
    /** The text at state `i` (0: before the first change; `entries.length`: after the last). */
    stateText(key, i) {
      const { entries } = of(key);
      if (i < 0 || i > entries.length) return null;
      return i === entries.length ? (entries.at(-1)?.after ?? null) : entries[i].before;
    },
    /** The pointer goes to state `i` (after the caller wrote that state's text). */
    jump(key, i) { const d = of(key); d.at = Math.max(0, Math.min(d.entries.length, i)); changed(); },
    /** A history that can no longer be trusted (the file changed under it), or a document that is gone. */
    clear(key) { docs.delete(key); changed(); },
    /** A renamed document keeps its history. */
    rename(from, to) { if (docs.has(from)) { docs.set(to, docs.get(from)); docs.delete(from); changed(); } },
    has: (key) => { void version.value; return docs.has(key); },
  };
}
