// @ts-check
// The document: the text pane, its steps in the one history (editor/docs.js), checking,
// saving and loading the text, drafts while a value is dragged, and completion.  It keeps the doc
// store (the text and what is known of it) and follows the view store for
// which file to show; it knows nothing of the selection or the view.

import { suggest } from '../complete.js';
import { effect, signal } from '@preact/signals-core';
import { describeChange } from '../history.js';
import { html, render } from '../view/html.js';
import { HistoryMenu, SaveState } from '../view/title-bar.js';
import { parsePayload } from '../payload.js';
import { TextPane } from '../text.js';
import { patchOnCopy } from '../mine.js';
import { isOffline } from '../host.js';
import { createDrafter } from '../drafts.js';
import { STALE_NOTE, spansFresh } from '../spans.js';
import { failed, showError } from '../problems.js';
import { token } from '../tokens.js';
import { thumbs } from './scene-thumbs.js';
import { $, host, makeMine, toast } from './app.js';
import { dirty, doc, isPlates, server, sourceFile, sourceId, stem, tile, view } from './stores.js';
import { docs, planOf } from './docs.js';

// ---- the document: the text, one history, checking and saving -----------------
// The text pane edits the file shown (the tile's document, or a file it
// uses).  Every change is checked at once against the server (the errors
// and the stack, without building) and saved soon after when it reads
// cleanly; the worker sees the saved file and rebuilds.  A text with a
// mistake stays here, unsaved, until it is fixed.

// The disk's text and its revision are the documents module's (js/documents.js: a source is a document whose state the
// program's answers make, and whose saves go through it in turn).  `doc.base`, `doc.docRev`, `doc.tileId` and `doc.file` are
// that state, worked out from which source the pane shows (`shownSid`, the id of the source document, set by the load that
// put its text in the pane): derived, never copied, so they cannot disagree with the text held.
const shownSid = signal(/** @type {string | null} */ (null));
const heldSource = () => { const sid = shownSid.value; return sid ? docs.get(sid)?.source ?? null : null; };
doc.derive('base', () => heldSource()?.text ?? null);
doc.derive('docRev', () => heldSource()?.rev ?? null);
doc.derive('tileId', () => heldSource()?.tile ?? null);
doc.derive('file', () => heldSource()?.file ?? null);

const run = { lastKind: null, lastTime: 0, checkTimer: 0, saveTimer: 0, namesTimer: 0, saving: false, openText: '' };

// The history is the page's one (js/doc-history.js): a tile's source is a document whose steps are made here,
// typing and the lens, each `{before, after, label, change, caret}`; a print list or a map is a document the
// program owns, whose steps are the program's answers (docs.edit), so for one the source's saves join it as
// steps of their own and Undo and Redo go through the program (docs.step), whichever face is showing.
const history = docs.history;
const textKey = () => `text:${tile()?.id ?? doc.get().tileId ?? ''}`;
const owned = () => isPlates(tile());
/** The text at state `i` of the history of `key` (0: as opened). */
const stateTextOf = (key, i) => history.stateText(key, i) ?? run.openText;
/** The text the source is at now, by the history's account. */
const stateNow = (key) => stateTextOf(key, history.at(key));

export const texts = new TextPane($('code'), {
  onEdit: (text, kind) => {
    edited(text, kind);
    const last = texts.caret().before.slice(-1);
    if (/[\w=+\-.]/.test(last)) openComp(false); else closeComp();
  },
  onCaretLine: () => closeComp(),
  onReadOnly: (patch) => makeMine({ patch }), // the keystroke (or paste) is kept: the copy gets it
  onKey: (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') { save(true); return true; }
    return compKey(e);
  },
  onHistory: (step) => undo(step),
  // the colours are the program's (one set of rules for the editor, the terminal and the site)
  tokens: async (text) => {
    const r = await host.tokens(text);
    if (r.status === 0) return null; // the program is away, or a newer reading took its place: the marks it had stay
    if (failed('Could not colour the text', r)) return null;
    return r.body;
  },
});
// the selection part adds onCaretLine and onHoverLine to texts.cb

// one history for the file shown: typing (a pause or another kind of change starts a new step), changes from the lens, and
// changes on disk.  The title bar's Undo and Redo and their menus are bound to it (`bindHistory`): they read the history's
// signal, the tile and the document, so they follow every change without being told.
function historyItems(key, mine) {
  const at = history.at(key), entries = history.entries(key);
  // a program-owned document goes back one change at a time (each is a call against the file's revision)
  const undo = [], redo = [];
  // (a step that is not there, at the ends, is not offered: the menus are drawn whatever the buttons say)
  for (let target = at - 1; target >= (mine ? at - 1 : 0); target--) if (entries[target]) undo.push({ entry: entries[target], target, distance: at - target, direction: 'Undo' });
  for (let target = at + 1; target <= (mine ? at + 1 : entries.length); target++) if (entries[target - 1]) redo.push({ entry: entries[target - 1], target, distance: target - at, direction: 'Redo' });
  return { undo, redo };
}
/** What a history row draws: the tile at the text it is at now and at the text a row leaves it at, each built low by the program. */
function historyPictures(id, file, key) {
  const { system, printer } = server.get();
  return {
    svc: thumbs,
    now: stateNow(key),
    spec: (/** @type {string} */ text) => ({ tile: id, text, file, system, printer, plate: token('--plate') || '#D9E0EB', w: 120, h: 84 }),
    // a text with a mistake in it is the document's to show (showError leaves it to the picture); anything else is the program's failure, shown loudly
    onError: (/** @type {any} */ e) => showError('Could not draw a step of the history', e),
  };
}
function pickHistory(direction, target) {
  if (owned()) undo(direction === 'Undo' ? -1 : 1); else goHistory(target);
  view.set({ popover: null });
}
function bindHistory() {
  effect(() => {
    const t = tile();
    const mine = owned();
    const key = mine ? t?.id : textKey();
    const writable = mine ? !!(t && !t.path.startsWith('lib:') && (planOf(t)?.writable ?? true)) : doc.get().writable;
    const canUndo = writable && !!key && history.canUndo(key);
    const canRedo = writable && !!key && history.canRedo(key);
    $('undo').disabled = !canUndo; $('undo-drop').disabled = !canUndo;
    $('redo').disabled = !canRedo; $('redo-drop').disabled = !canRedo;
    const items = key ? historyItems(key, mine) : { undo: [], redo: [] };
    // pictures of what a step does: of a tile's own text, and only while the menu is open (nothing is built for a closed menu)
    const open = view.get().popover;
    const own = !mine && !!t && doc.get().file === t.path && doc.get().text != null;
    const pictures = (menu) => (own && open === menu ? historyPictures(t.id, t.path, key) : null);
    const withText = (list) => list.map((it) => ({ ...it, text: own && open ? stateTextOf(key, it.target) : null }));
    render(html`<${HistoryMenu} items=${withText(items.undo)} onPick=${pickHistory} pictures=${pictures('undo-history')} />`, $('undo-history'));
    render(html`<${HistoryMenu} items=${withText(items.redo)} onPick=${pickHistory} pictures=${pictures('redo-history')} />`, $('redo-history'));
  });
}

function resetHistory(text) {
  history.clear(textKey());
  run.openText = text; run.lastKind = null;
}
function clearDocument() {
  texts.clear();
  loadKey = null;
  history.clear(textKey());
  run.lastKind = null;
  shownSid.value = null;
  doc.set({ text: null, writable: false, stack: null, map: null, nodes: null, nodesRev: null, names: null, conflict: null, note: null });
}
function record(text, kind, label = null) {
  // a document the program owns has its steps from the program: the source's saves (save() below)
  if (owned()) return;
  const key = textKey();
  const now = performance.now();
  const at = history.at(key);
  const join = kind === 'type' && run.lastKind === 'type' && now - run.lastTime < 1000 && at > 0;
  const prior = join ? history.entries(key)[at - 1].before : stateNow(key);
  const change = describeChange(prior, text, kind, label);
  if (join) history.amend(key, { after: text, caret: texts.selectionStart, kind, label: change.title, change });
  else history.push(key, { before: prior, after: text, caret: texts.selectionStart, kind, label: change.title, change });
  run.lastKind = kind; run.lastTime = now;
}
/** The source goes to state `i` of its history (the text of a tile; a program-owned document is stepped by docs.step). */
function goHistory(i) {
  const key = textKey();
  if (owned() || !doc.get().writable || i < 0 || i > history.entries(key).length) return;
  run.lastKind = null;
  const text = stateTextOf(key, i);
  texts.setText(text, i > 0 ? history.entries(key)[i - 1].caret : 0);
  history.jump(key, i);
  doc.set({ text: texts.text });
  closeComp();
  changed();
}
/** Undo (`step` -1) or Redo (1): the page's one, for whichever document is open. */
export function undo(step) {
  const t = tile();
  if (owned()) { if (t) docs.step(t.id, step); return; }
  goHistory(history.at(textKey()) + step);
}

/**
 * The user changed the text: `kind` is type, lens, view, or drag (a scrub
 * under way, kept out of the history until it ends).  `text` is put in the
 * pane unless typing put it there.
 */
export function edited(text, kind, label = null, caret = null) {
  if (text !== texts.text) texts.setText(text, caret);
  doc.set({ text });
  if (kind !== 'drag') record(text, kind, label);
  if (kind === 'lens') askDraft(); // the end of a scrub asks too: a draft for an earlier value must not stay (a step's own draft came with it)
  changed(kind === 'drag');
}

// drafts: while a value is dragged the view shows quick builds of the text
// as it is (marked draft), one in flight at a time, always the latest text;
// the full build follows when the hand lets go and the file is saved.  A
// draft is a plain request, answered with its mesh, and shown only if the
// text is still the text it was built from (drafts.js).
const drafter = createDrafter({
  text: () => {
    const t = tile(), d = doc.get();
    return !t || !d.file || d.text == null || d.tileId !== t.id ? null : d.text;
  },
  request: async (text) => {
    const t = tile(), d = doc.get();
    return { t, r: await host.draft(t.id, { text, file: d.file === t.path ? undefined : d.file, debug: view.get().debug }) };
  },
  accept: ({ t, r }) => {
    if (!failed(`Could not draft ${stem(t)}`, r) && r.buffer && t.id === view.get().tileId) view.set({ draft: { tileId: t.id, payload: parsePayload(r.buffer) } });
  },
  fail: (e) => showError('Could not draft', e),
});
export const askDraft = () => drafter.ask();
/** A full build of the text now is on the page: a draft still on its way is dropped. */
export function supersedeDrafts() { if (!dirty()) drafter.supersede(); }

/**
 * A change made at pointer speed: a map handle dragged, a value scrubbed or typed.  The page never makes the text of one:
 * each step names what it changes (`make(x)` is the edit's body for the value `x`: its `op`, the node's id and the value) and
 * the program makes the edit and drafts the text it made in one request (`host.step`), answering the new text, its revision
 * and the draft.  One request is in the air at a time, and what is wanted meanwhile is only the newest (a fast drag skips
 * steps rather than queueing them); each is made to the text the last answer made, at the revision it named, and a refusal
 * for a stale one (409) is answered by a new check, so the next step names the right node.  `end(x)` makes the last value an
 * ordinary edit: the one step of history the whole change is.  Null when the text has not been checked (the ids it would name
 * are not known yet; the reason is said).
 * @param {string} label the history step the change ends as
 * @param {(x: any) => Record<string, any>} make the edit's body for a value
 */
export function steps(label, make) {
  if (doc.get().writable && !spansUsable()) return null;
  const base = doc.get().text;
  /** @type {{ x: any } | null} */ let want = null; // the newest value asked for, not yet sent
  /** @type {Promise<void> | null} */ let flight = null; // the request in the air
  /** @type {{ text: string, rev: string, key: string } | null} */ let last = null; // what the last answer made
  let over = false, mined = false, pending = /** @type {any} */ (null);
  const key = (/** @type {any} */ x) => JSON.stringify(make(x));
  /** The revision the ids are read at: the last answer's when the text is still the text it made, else the last check's. */
  const readAt = () => (last && last.text === doc.get().text ? last.rev : doc.get().nodesRev ?? undefined);

  async function send(/** @type {any} */ x) {
    const t = tile(), d = doc.get();
    if (!t || d.text == null || d.tileId !== t.id) return;
    const text = d.text;
    const read = readAt();
    const r = await host.step(t.id, { edit: /** @type {any} */ ({ ...make(x), text, read }), file: d.file === t.path ? undefined : d.file, debug: view.get().debug });
    if (r.status === 0) return; // the program is away: said by the chip
    if (r.status === 409) { check(); return; } // the text moved on past the nodes read: checked again, and the next step names them afresh
    if (r.status !== 200 || !('head' in r)) {
      doc.set({ note: /** @type {any} */ (r).body?.error || 'that change could not be made' });
      failed('Could not change the tile', /** @type {any} */ (r));
      return;
    }
    // typing moved the text on while the program answered: the answer is for the text it was sent
    if (doc.get().text !== text || t.id !== view.get().tileId) return;
    last = { text: r.head.text, rev: r.head.rev, key: key(x) };
    edited(r.head.text, 'drag');
    if (r.head.draft === 'mesh' && r.buffer.byteLength) view.set({ draft: { tileId: t.id, payload: parsePayload(r.buffer) } });
  }
  async function pump() {
    if (flight) return;
    flight = (async () => { while (want && !over) { const { x } = want; want = null; await send(x); } })();
    try { await flight; } finally { flight = null; }
  }
  /** The ordinary edit of `x` (the change as one step of history), once the ones in the air have landed. */
  async function settle(/** @type {any} */ x) {
    for (let tries = 0; tries < 2; tries++) {
      if (!(last && last.text === doc.get().text) && !spansUsable()) return false;
      const r = await editCall({ ...make(x), text: doc.get().text, ...(readAt() ? { read: readAt() } : {}) });
      if (r.status === 409) { await check(); continue; }
      if (r.status !== 200) { doc.set({ note: r.body?.error || 'that change could not be made' }); failed('Could not change the tile', r); return false; }
      edited(r.body.text, 'lens', label);
      return true;
    }
    toast(STALE_NOTE);
    return false;
  }
  const guard = (/** @type {Promise<any>} */ p) => p.catch((e) => showError('Could not change the tile', e));
  return {
    /** Ask for value `x` now (the newest wins). */
    to(/** @type {any} */ x) {
      if (over) return;
      // a library tile: the change copies it, and the copy gets the value the change ended at
      if (!doc.get().writable) {
        pending = x;
        if (!mined) { mined = true; makeMine({ run: () => guard(settle(pending)) }); }
        return;
      }
      want = { x };
      guard(pump());
    },
    /** The change is over, at `x`: one step of history, and a save. */
    end(/** @type {any} */ x) {
      over = true; want = null; pending = x;
      if (mined) return Promise.resolve();
      return guard((async () => {
        if (flight) await flight;
        if (last && last.text === doc.get().text && last.text === base) { askDraft(); edited(base, 'drag'); } // back where it began: no step to undo
        else if (last && last.key === key(x) && last.text === doc.get().text) edited(doc.get().text, 'lens', label); // the last answer made it
        else await settle(x);
      })());
    },
    /** The change is given up: the text as it was, kept out of the history. */
    cancel() {
      over = true; want = null;
      return guard((async () => {
        if (flight) await flight;
        if (last && last.text === doc.get().text) { edited(base, 'drag'); askDraft(); }
      })());
    },
  };
}

/**
 * Mend a mistake with the program's own fix (a message's `fix`, from `file`): the program lays it in, and the text goes through `edited`,
 * the document's one way in, as one step of the history named by the fix.  Only the text the fix was made for takes it: once the text
 * has moved past the check, or the place no longer reads what the program saw, it waits for the next check and says so.
 * @param {{ offset: number, len: number, was: string, replacement: string, label: string }} fix
 * @param {string | null} file the file the mistake is in (null: the tile's own)
 */
export async function applyFix(fix, file) {
  if (!doc.get().writable) { makeMine({ run: () => applyFix(fix, file) }); return false; }
  const d = doc.get();
  const waits = 'The text has changed since the program read it: the fix waits for the next check.';
  if (d.text == null || d.checked !== d.text || !(file == null || file === d.file)) { toast(waits); return false; }
  // the program lays the fix in (`Op::Fix`), and only where the text still reads what it saw
  const r = await host.edit({ op: 'fix', text: d.text, fix });
  if (failed('Could not make the fix', r)) return false;
  if (r.status !== 200) {
    const why = typeof r.body?.error === 'string' ? r.body.error : 'that fix could not be made';
    doc.set({ note: why });
    toast(`Could not make the fix: ${why}`);
    return false;
  }
  // the text moved on while the program answered (typing): the answer is for the text it was sent
  if (doc.get().text !== d.text) { toast(waits); return false; }
  edited(r.body.text, 'lens', fix.label);
  return true;
}

/**
 * May an edit that writes at a span (a stack entry's, the map's) go ahead?
 * Only when the text shown is the text last checked, cleanly: otherwise the
 * span is from older text.  Says why when it may not.
 */
export function spansUsable() {
  if (spansFresh(doc.get(), view.get().tileId)) return true;
  toast(STALE_NOTE);
  return false;
}

/**
 * The names the text makes (definitions, regions, lines, points), asked of the
 * program's parser a moment after the last change: completion reads them, so a
 * name typed seconds ago is offered before the check of it is back.  A text
 * that does not parse keeps the names it had; the check shows its mistakes.
 */
async function askNames() {
  const text = doc.get().text;
  if (text == null) return;
  const r = await host.names(text);
  if (r.status === 0) return; // the server is away, or a newer reading took its place: the names it had stay
  if (failed('Could not read the names in the text', r)) return;
  if (!r.body?.ok) return;
  const { defs, regions, lines, points } = r.body;
  doc.set({ names: { defs, regions, lines, points } });
}

function changed(dragging = false) {
  clearTimeout(run.namesTimer);
  run.namesTimer = setTimeout(askNames, 60);
  clearTimeout(run.checkTimer);
  run.checkTimer = setTimeout(check, 120);
  clearTimeout(run.saveTimer);
  if (!dragging) run.saveTimer = setTimeout(save, 500);
}

/**
 * An edit made through the server's edit route, naming nodes by the ids of the last check: the call carries the revision of the text
 * those ids were read at (`nodesRev`), and the program refuses it (409) when the text it is sent is another.  The refusal is shown where
 * the change was asked for, and the text is checked again, so the next try names the right node.
 * @param {Record<string, any>} body the edit (its `text` is the document's unless it says another)
 */
export async function editCall(body) {
  // the caller names the op and its fields; the contract's type (EditReq) is checked where the body is made whole, on the wire by the program
  const r = await host.edit(/** @type {import('../api').EditReq} */ ({ text: doc.get().text, read: doc.get().nodesRev ?? undefined, ...body }));
  if (r.status === 409) check();
  return r;
}

async function check() {
  const t = tile(), d = doc.get();
  if (!t || d.text == null) return false;
  const text = d.text;
  const r = await host.check(t.id, { text, file: d.file === t.path ? undefined : d.file });
  if (r.status === 0) return false; // the server is away (not checked yet, and checked again when it is back), or a newer check took its place
  if (failed(`Could not check ${stem(t)}`, r)) return false;
  if (text !== doc.get().text || !r.body || t.id !== view.get().tileId) return false;
  const patch = { note: null, checked: text, checkedFile: d.file, checkedTile: d.tileId, ok: !!r.body.ok, errors: r.body.errors || [] };
  if (r.body.ok && r.body.stack) patch.stack = r.body.stack; // the last good stack stays while there are mistakes
  if (r.body.ok && r.body.map) patch.map = r.body.map;
  if (r.body.ok && r.body.nodes) { patch.nodes = r.body.nodes; patch.nodesRev = r.body.nodesRev ?? null; }
  doc.set(patch);
  return true;
}

async function save(now = false) {
  let d = doc.get();
  if (!d.writable || !dirty()) return !dirty();
  if (run.saving) return false;
  if (!server.get().live) return false; // saved when the server is back
  if (d.checked !== d.text) {
    if (now) await check();
    else { run.saveTimer = setTimeout(save, 250); return false; }
  }
  d = doc.get();
  if (d.checked !== d.text || !d.ok) return false; // a mistake: kept here until it is fixed
  const t = tile(), text = d.text, file = d.file, sid = shownSid.value;
  if (!t || !file || !sid) return false;
  run.saving = true;
  doc.set({ saving: true });
  try {
    const r = await docs.saveSource(sid, text, `Could not save ${stem(t)}`);
    if (r.status === 'saved') {
      doc.set({ note: null, conflict: null });
      // a print list or a map: the source's save is a step of its history, like the program's own edits
      if (isPlates(t) && file === t.path) docs.adopt(t.id, 'Edited source', { before: d.base, text, docRev: r.docRev });
    } else if (r.status === 'conflict') doc.set({ note: null, conflict: r.disk });
    else if (r.status === 'failed') doc.set({ note: r.reply.body?.error || `could not save (${r.reply.status})` });
  } finally {
    run.saving = false;
    doc.set({ saving: false });
  }
  d = doc.get();
  if (dirty() && !d.conflict && !d.note) changed(); // typed while it saved
  return !dirty();
}

let leaving = null;
function confirmDiscard() {
  if (leaving) return leaving;
  const dlg = $('leave-dialog'), d = doc.get();
  $('leave-reason').textContent = d.conflict != null
    ? 'The file changed on disk while your version was open. Choose which version to keep before leaving.'
    : !d.ok ? 'This edit contains a mistake, so it has not been saved.'
      : d.note || 'This edit has not been saved yet.';
  dlg.returnValue = ''; // showModal keeps the last choice: Esc must mean keep editing
  dlg.showModal();
  leaving = new Promise((resolve) => {
    dlg.addEventListener('close', () => {
      const discard = dlg.returnValue === 'discard';
      const base = doc.get().base;
      if (discard && base != null) {
        texts.setText(base);
        if (!owned()) resetHistory(base);
        doc.set({ text: base, checked: null, ok: true, errors: [], conflict: null, note: null });
        changed(); // checked afresh: the stack and map held are of the text just discarded
      }
      leaving = null;
      resolve(discard);
    }, { once: true });
  });
  return leaving;
}

/** Save before leaving this file; if it cannot be saved, make discarding explicit. */
export async function canLeave() {
  if (!dirty() || !doc.get().writable) return true;
  clearTimeout(run.checkTimer); clearTimeout(run.saveTimer);
  if (await save(true)) return true;
  return confirmDiscard();
}

let loadToken = 0, loadKey = null, loading = null;
let carried = null; // the key of the undo history a rename brings to the renamed file
/** Bring the undo history along to the file a rename is about to make. */
export function carryHistory() { carried = { key: textKey() }; }

/** Show `file` of the tile shown (null: its own document); resolves true once it is there. */
export async function loadText(file) {
  const t = tile();
  if (!t || /\.(stl|dinv)$/i.test(t.path)) { clearDocument(); return false; } // a model, or the inventory (its own view), has no tile text to edit here
  const want = file || t.path;
  if (dirty() && texts.file !== want && !(await canLeave())) return false;
  // unsaved changes to this file stay; a new build of the same file brings nothing new
  if (dirty() && texts.file === want && doc.get().tileId === t.id) return true;
  const key = [t.id, want, t.meshVersion, t.status, t.errors.length].join('|');
  if (loadKey === key) return loading ?? true;
  loadKey = key;
  const token = ++loadToken;
  loading = (async () => {
    try {
      const sid = sourceId(t, want);
      // taken unless the page has unsaved changes on this very text, or a later load has begun
      const mine = (v) => dirty() && texts.file === v.file && doc.get().tileId === t.id;
      let read = null;
      // the text, the pane and the source shown change in the same step as the held state is taken (no reader sees one without the others)
      let fresh = false;
      const apply = (state) => {
        const v = { file: state.source.file, text: state.source.text, writable: state.source.writable };
        fresh = doc.get().tileId !== t.id || texts.file !== v.file;
        texts.show(v.file, v.text, v.writable);
        shownSid.value = sid;
        doc.set({
          text: v.text, writable: v.writable,
          conflict: null, note: null,
          checked: null, ok: !t.errors.length, errors: [], stack: null, nodes: null, nodesRev: null, // the build's plan is this source's; not checked as shown until check() below says so (the sheet waits for it)
          ...(fresh ? { map: null, names: null } : {}),
        });
      };
      const st = await docs.loadSource(sid, { tile: t.id, own: want === t.path }, async () => (read = await host.text(t.id, want === t.path ? null : want)), (v) => token === loadToken && !mine(v), apply);
      if (!st?.source) {
        if (!read) loadKey = null;
        return !!read && token === loadToken && mine(read);
      }
      const v = { text: st.source.text };
      const key = `text:${t.id}`;
      if (fresh && carried && history.has(carried.key) && stateTextOf(carried.key, history.at(carried.key)) === v.text) {
        history.rename(carried.key, key); run.openText = v.text; // a rename keeps its undo
      } else if (fresh) resetHistory(v.text);
      else if (stateNow(key) !== v.text) record(v.text, 'disk'); // changed on disk: one more step (a document the program owns has its own steps)
      if (fresh) carried = null;
      askNames(); // the names it makes, for completion
      check(); // its stack and map, for the sheet and the overlay: spans of exactly this text
      return true;
    } catch (e) {
      loadKey = null;
      if (!isOffline(e)) showError(`Could not open ${want}`, e);
      return false;
    }
  })();
  return loading;
}

/** Show `file` (null: the tile's own) and, once it is there, scroll to `line`. */
export async function openFile(file, line) {
  const t = tile();
  view.set({ source: !file || file === t?.path ? null : sourceId(t, file) });
  if (await loadText(sourceFile(t, view.get().source)) && line) texts.reveal(line);
}

/**
 * What the title bar says of the text: saved, saving, why it is not, or (for a
 * part of the tile, which has a bar of its own) which file this is.  The tile
 * itself is named in the crumb above; a library tile has its chip there.
 */
function showTextBar() {
  const t = tile(), d = doc.get(), f = d.file;
  const part = !!f && f !== t?.path;
  $('partbar').hidden = !part;
  $('sourcepath').textContent = part ? `\u200e${f}` : '';
  $('sourcepath').title = f || '';
  $('rochip').hidden = !t?.path.startsWith('lib:');
  const st = $('savestate');
  const conflict = !!f && d.writable && d.conflict != null;
  const words = f && !conflict ? saveWords() : null;
  st.classList.toggle('bad', conflict || !!words?.bad);
  // the disk's text and revision are already the held state (js/documents.js): choosing says which text is the user's
  render(html`<${SaveState} words=${words} conflict=${conflict}
    onTake=${() => { const disk = docs.resolveConflict(shownSid.value) ?? d.conflict; doc.set({ conflict: null }); edited(disk, 'disk'); }}
    onKeep=${() => { docs.resolveConflict(shownSid.value); doc.set({ conflict: null }); changed(); }} />`, st);
}

/**
 * Where the text stands, in words: {text, bad} (bad: it needs the user), or null when there
 * is nothing to say (no file, or a library tile, which says so by its chip).  The title bar
 * says it; the status line repeats it only when it is not just "saved".
 */
export function saveWords() {
  const d = doc.get(), f = d.file, live = server.get().live;
  if (!f) return null;
  if (!d.writable) return f.startsWith('lib:') ? null : { text: 'read only', bad: false };
  if (d.conflict != null) return { text: 'changed on disk', bad: true };
  if (d.note) return { text: d.note, bad: true };
  if (d.saving) return { text: 'saving…', bad: false };
  if (!dirty()) return { text: 'saved', bad: false };
  if (!live) return { text: 'not saved yet: waiting for dharmatiles preview', bad: false };
  if (d.checked === d.text && !d.ok) return { text: 'not saved: a mistake', bad: true };
  return { text: 'editing…', bad: false };
}

/**
 * Resolves true once the text pane holds tile `id`'s own document and it has been
 * checked (the spans an edit writes at are of exactly this text); false after a few seconds.
 */
export function whenChecked(id) {
  const ready = () => { const d = doc.get(); return d.tileId === id && d.text != null && d.checked === d.text && d.file === tile()?.path; };
  if (ready()) return Promise.resolve(true);
  return new Promise((resolve) => {
    const stop = (ok) => { off(); offView(); clearTimeout(timer); resolve(ok); };
    const off = doc.subscribe(['checked', 'tileId', 'text'], () => { if (ready()) stop(true); });
    const offView = view.subscribe(['tileId'], () => { if (view.get().tileId !== id) stop(false); });
    const timer = setTimeout(() => stop(false), 6000);
  });
}

/**
 * Make `patch` ({from, to, insert}, over `lib`, the text of the tile the change was meant for)
 * on the text now shown, which is that tile's copy: typed, as one step of history.  False when
 * the copy changed the very text the patch touches.
 */
export function applyPatchOnCopy(patch, lib) {
  const r = patchOnCopy(patch, lib, doc.get().text);
  if (!r) return false;
  edited(r.text, 'type', null, r.caret);
  return true;
}

/** Drop the unsaved changes and any save or check still waiting (the file is about to be deleted). */
export function abandonChanges() {
  clearTimeout(run.checkTimer); clearTimeout(run.saveTimer);
  const base = doc.get().base;
  if (base != null && doc.get().text !== base) { texts.setText(base); doc.set({ text: base }); }
}

// ---- completion -------------------------------------------------------------

const comp = { s: null, i: 0 };
function openComp(force) {
  const elements = server.get().elements;
  if (!doc.get().writable || !elements.length) return closeComp();
  const s = suggest(texts.caret(), elements, force, doc.get().map?.language, doc.get().names ?? undefined);
  if (!s) return closeComp();
  comp.s = s; comp.i = 0;
  renderComp();
}
function renderComp() {
  const box = $('comp');
  box.replaceChildren();
  comp.s.items.forEach((it, i) => {
    const d = document.createElement('div');
    d.className = `it fam-${it.family || 'doc'}${i === comp.i ? ' on' : ''}`;
    d.setAttribute('role', 'option');
    d.setAttribute('aria-selected', String(i === comp.i));
    d.innerHTML = '<i class="sw"></i><span class="nm"></span><span class="ds"></span>';
    d.querySelector('.nm').textContent = it.label;
    d.querySelector('.ds').textContent = it.about || it.kind;
    d.onpointerdown = (e) => { e.preventDefault(); comp.i = i; acceptComp(); };
    box.append(d);
  });
  const xy = texts.caretXY();
  box.hidden = false;
  if (xy) {
    const h = box.offsetHeight, w = box.offsetWidth;
    box.style.left = `${Math.max(8, Math.min(xy.x - 6, innerWidth - w - 8))}px`;
    box.style.top = `${xy.y + h + 4 > innerHeight ? xy.y - h - 26 : xy.y + 2}px`;
  }
  box.children[comp.i]?.scrollIntoView({ block: 'nearest' });
}
function closeComp() { comp.s = null; $('comp').hidden = true; }
function acceptComp() {
  const it = comp.s?.items[comp.i];
  if (!it) return closeComp();
  const s = comp.s;
  closeComp();
  texts.insert(it.insert, s.from, s.to);
}
function compKey(e) {
  if ((e.ctrlKey || e.metaKey) && e.key === ' ') { openComp(true); return true; }
  if (!comp.s) return false;
  const n = comp.s.items.length;
  if (e.key === 'ArrowDown') { comp.i = (comp.i + 1) % n; renderComp(); return true; }
  if (e.key === 'ArrowUp') { comp.i = (comp.i + n - 1) % n; renderComp(); return true; }
  if (e.key === 'Enter' || e.key === 'Tab') { acceptComp(); return true; }
  if (e.key === 'Escape') { closeComp(); return true; }
  return false;
}

/** Wire this part of the page (once, at load). */
export function init() {
  // what the text pane shows follows the tile, the file wanted, and the tile's builds
  const follow = () => { if (view.get().tileId != null || tile()) loadText(sourceFile(tile(), view.get().source)); else if (server.get().ready) clearDocument(); };
  view.subscribe(['tileId', 'source'], follow);
  server.subscribe(['tiles'], follow);
  view.subscribe(['tileId'], closeComp);
  effect(showTextBar);
  bindHistory();
  // the server was away and is back: an edit it never got is checked and saved now
  let wasLive = server.get().live;
  server.subscribe(['live'], ({ live }) => {
    if (live && !wasLive && tile() && dirty()) { changed(); askDraft(); }
    wasLive = live;
  });
  $('part-back').onclick = () => openFile(null);
  $('undo').onclick = () => undo(-1);
  $('redo').onclick = () => undo(1);
  // the history is one: what it holds changes when a document's state or its steps do
  addEventListener('beforeunload', (e) => {
    if (dirty() && doc.get().writable) { e.preventDefault(); e.returnValue = ''; }
  });
  // one undo for the whole page: typing and changes made in the view
  addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && !e.altKey && /** @type {HTMLElement} */ (e.target).tagName !== 'INPUT' && (e.key === 'z' || e.key === 'Z' || e.key === 'y')) {
      e.preventDefault();
      undo(e.key === 'y' || e.shiftKey ? 1 : -1);
    }
  });
}
