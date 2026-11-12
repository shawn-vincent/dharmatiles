// The document: the text pane, one history, checking, saving and loading the
// text, drafts while a value is dragged, and completion.  It keeps the doc
// store (the text and what is known of it) and follows the view store for
// which file to show; it knows nothing of the selection or the view.

import { suggest } from '../complete.js';
import { describeChange } from '../history.js';
import { parsePayload } from '../payload.js';
import { TextPane, byteToIndex } from '../text.js';
import { isOffline } from '../host.js';
import { createDrafter } from '../drafts.js';
import { STALE_NOTE, spansFresh } from '../spans.js';
import { failed, showError } from '../problems.js';
import { $, FAMILY, host, offerMine, toast } from './app.js';
import { dirty, doc, server, stem, tile, view } from './stores.js';

// ---- the document: the text, one history, checking and saving -----------------
// The text pane edits the file shown (the tile's document, or a file it
// uses).  Every change is checked at once against the server (the errors
// and the stack, without building) and saved soon after when it reads
// cleanly; the worker sees the saved file and rebuilds.  A text with a
// mistake stays here, unsaved, until it is fixed.

const run = { history: [], at: -1, lastKind: null, lastTime: 0, checkTimer: 0, saveTimer: 0, scrubText: null, checkSeq: 0, saving: false };

export const texts = new TextPane($('code'), {
  onEdit: (text, kind) => {
    edited(text, kind);
    const last = texts.caret().before.slice(-1);
    if (/[\w=+\-.]/.test(last)) openComp(false); else closeComp();
  },
  onCaretLine: () => closeComp(),
  onReadOnly: offerMine,
  onKey: (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') { save(true); return true; }
    return compKey(e);
  },
  onHistory: (step) => undo(step),
});
// the selection part adds onCaretLine and onHoverLine to texts.cb

// one history for the file shown: typing (a pause or another kind of change
// starts a new step), changes from the lens, and changes on disk
function historyButton(entry, target, distance, direction) {
  const b = document.createElement('button');
  b.type = 'button';
  const action = document.createElement('span'); action.className = 'action'; action.textContent = entry.label;
  const away = document.createElement('span'); away.className = 'distance'; away.textContent = distance === 1 ? '1 step' : `${distance} steps`;
  b.append(action, away);
  if (entry.change?.removed || entry.change?.added) {
    const change = document.createElement('span'); change.className = 'change';
    if (entry.change.removed) {
      const removed = document.createElement('code'); removed.className = 'removed'; removed.textContent = `− ${entry.change.removed}`;
      change.append(removed);
    }
    if (entry.change.added) {
      const added = document.createElement('code'); added.className = 'added'; added.textContent = `+ ${entry.change.added}`;
      change.append(added);
    }
    b.append(change);
  }
  b.title = `${direction} to ${entry.label}`;
  b.onclick = () => { goHistory(target); view.set({ popover: null }); };
  return b;
}

function renderHistory(buildMenus = false) {
  const writable = doc.get().writable;
  const canUndo = writable && run.at > 0;
  const canRedo = writable && run.at >= 0 && run.at < run.history.length - 1;
  $('undo').disabled = !canUndo; $('undo-drop').disabled = !canUndo;
  $('redo').disabled = !canRedo; $('redo-drop').disabled = !canRedo;
  if (!buildMenus) return;
  const undoMenu = $('undo-history'), redoMenu = $('redo-history');
  undoMenu.replaceChildren(); redoMenu.replaceChildren();
  for (let target = run.at - 1; target >= 0; target--) {
    undoMenu.append(historyButton(run.history[target + 1], target, run.at - target, 'Undo'));
  }
  for (let target = run.at + 1; target < run.history.length; target++) {
    redoMenu.append(historyButton(run.history[target], target, target - run.at, 'Redo'));
  }
}

function resetHistory(text) {
  run.history = [{ text, caret: 0, kind: 'open', label: 'Opened document' }];
  run.at = 0; run.lastKind = null;
  renderHistory();
}
function clearDocument() {
  texts.clear();
  loadKey = null;
  run.history = []; run.at = -1; run.lastKind = null;
  doc.set({ tileId: null, file: null, base: null, text: null, writable: false, stack: null, map: null, format: 0, migrated: null, conflict: null, note: null });
  renderHistory();
}
function record(text, kind, label = null) {
  const now = performance.now();
  const join = kind === 'type' && run.lastKind === 'type' && now - run.lastTime < 1000 && run.at > 0;
  run.history.length = run.at + 1;
  const before = run.history[join ? run.at - 1 : run.at]?.text || '';
  const change = describeChange(before, text, kind, label);
  const h = { text, caret: texts.selectionStart, kind, label: change.title, change };
  if (join) run.history[run.at] = h; else { run.history.push(h); run.at++; }
  if (run.history.length > 400) { run.history.shift(); run.at--; }
  run.lastKind = kind; run.lastTime = now;
  renderHistory();
}
function goHistory(i) {
  if (!doc.get().writable || i < 0 || i >= run.history.length) return;
  run.at = i; run.lastKind = null;
  const h = run.history[i];
  texts.setText(h.text, h.caret);
  doc.set({ text: texts.text });
  closeComp();
  changed();
  renderHistory();
}
export function undo(step) { goHistory(run.at + step); }

/**
 * The user changed the text: `kind` is type, lens, view, or drag (a scrub
 * under way, kept out of the history until it ends).  `text` is put in the
 * pane unless typing put it there.
 */
export function edited(text, kind, label = null, caret = null) {
  if (text !== texts.text) texts.setText(text, caret);
  doc.set({ text });
  if (kind !== 'drag') record(text, kind, label);
  if (kind === 'drag' || kind === 'lens') askDraft(); // the last step of a scrub asks too: a draft for an earlier value must not stay
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

/** "Update this file": the text carried to the newest format (by the program's own migration), as an edit that saves like any other. */
async function updateFile() {
  const t = tile(), d = doc.get();
  if (!t || d.text == null || !d.writable) return;
  try {
    const r = await host.migrate(t.id, { text: d.text, file: d.file === t.path ? undefined : d.file });
    if (failed(`Could not update ${stem(t)}`, r)) return;
    if (r.body?.ok && typeof r.body.text === 'string') edited(r.body.text, 'update');
    else doc.set({ note: r.body?.errors?.[0]?.message || 'could not update this file: it needs a hand fix', ok: false, errors: r.body?.errors || [] });
  } catch (e) { showError(`Could not update ${stem(t)}`, e); }
}

/** Replace bytes [start, start+len) of the text; `end` (the scrub is over) makes it a step of history and a save. */
export function replaceBytes(start, len, text, end) {
  const t = doc.get().text;
  // a scrub's later steps follow the text its own steps made (the caller tracks the span); the first needs a clean check of this text
  if (t !== run.scrubText && !spansUsable()) return false;
  const i = byteToIndex(t, start), j = byteToIndex(t, start + len);
  const next = t.slice(0, i) + text + t.slice(j);
  run.scrubText = end ? null : next;
  edited(next, end ? 'lens' : 'drag');
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

function changed(dragging = false) {
  clearTimeout(run.checkTimer);
  run.checkTimer = setTimeout(check, 120);
  clearTimeout(run.saveTimer);
  if (!dragging) run.saveTimer = setTimeout(save, 500);
}

async function check() {
  const t = tile(), d = doc.get();
  if (!t || d.text == null) return false;
  const text = d.text, seq = ++run.checkSeq;
  const r = await host.check(t.id, { text, file: d.file === t.path ? undefined : d.file });
  if (r.status === 0) return false; // the server is away: not checked yet, and checked again when it is back
  if (failed(`Could not check ${stem(t)}`, r)) return false;
  if (seq !== run.checkSeq || text !== doc.get().text || !r.body || t.id !== view.get().tileId) return false;
  const patch = { note: null, checked: text, checkedFile: d.file, checkedTile: d.tileId, ok: !!r.body.ok, errors: r.body.errors || [] };
  if (r.body.ok && r.body.stack) patch.stack = r.body.stack; // the last good stack stays while there are mistakes
  if (r.body.ok && r.body.map) patch.map = r.body.map;
  if (typeof r.body.format === 'number') patch.format = r.body.format; // known even while there are mistakes
  if (r.body.ok) patch.migrated = (r.body.migrated || []).find((m) => m.path === d.file)?.from ?? null;
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
  const t = tile(), text = d.text, file = d.file;
  if (!t || !file) return false;
  run.saving = true;
  doc.set({ saving: true });
  try {
    const r = await host.save(t.id, { text, base: d.base, file: file === t.path ? undefined : file });
    if (r.status === 200) doc.set({ note: null, base: text, conflict: null });
    else if (r.status === 409) doc.set({ note: null, conflict: r.body?.text ?? '' });
    else {
      doc.set({ note: r.body?.error || `could not save (${r.status})` });
      failed(`Could not save ${stem(t)}`, r);
    }
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
        resetHistory(base);
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
let carried = null; // the undo history a rename brings to the renamed file
/** Bring the undo history along to the file a rename is about to make. */
export function carryHistory() { carried = { history: run.history, at: run.at }; }

/** Show `file` of the tile shown (null: its own document); resolves true once it is there. */
export async function loadText(file) {
  const t = tile();
  if (!t || /\.stl$/i.test(t.path)) { clearDocument(); return false; }
  const want = file || t.path;
  if (dirty() && texts.file !== want && !(await canLeave())) return false;
  // unsaved changes to this file stay; a new build of the same file brings nothing new
  if (dirty() && texts.file === want && doc.get().tileId === t.id) return true;
  const key = [t.id, want, t.mesh_version, t.status, t.errors.length].join('|');
  if (loadKey === key) return loading ?? true;
  loadKey = key;
  const token = ++loadToken;
  loading = (async () => {
    try {
      const v = await host.text(t.id, want === t.path ? null : want);
      if (token !== loadToken || !v) { if (!v) loadKey = null; return false; }
      if (dirty() && texts.file === v.file && doc.get().tileId === t.id) return true;
      const fresh = doc.get().tileId !== t.id || texts.file !== v.file;
      texts.show(v.file, v.text, v.writable);
      doc.set({
        tileId: t.id, file: v.file, text: v.text, base: v.text, writable: v.writable,
        conflict: null, note: null,
        checked: null, ok: !t.errors.length, errors: [], stack: null, // the build's plan is this source's; not checked as shown until check() below says so (the sheet waits for it)
        ...(fresh ? { map: null, format: 0 } : {}),
      });
      if (fresh && carried && carried.history[carried.at]?.text === v.text) {
        run.history = carried.history; run.at = carried.at; renderHistory(); // a rename keeps its undo
      } else if (fresh) resetHistory(v.text);
      else if (run.history[run.at]?.text !== v.text) record(v.text, 'disk'); // changed on disk: one more step
      renderHistory();
      if (fresh) carried = null;
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
  view.set({ textFile: !file || file === t?.path ? null : file });
  if (await loadText(view.get().textFile) && line) texts.reveal(line);
}

function renderTextBar() {
  const t = tile(), d = doc.get(), f = d.file, live = server.get().live;
  // the tile's own file is named in the crumb above; another file (a part) by its path
  $('sourcepath').textContent = !f || f === t?.path ? '' : `‎${f}`;
  $('sourcepath').title = f || '';
  const st = $('sourcestate');
  st.replaceChildren();
  const button = (label, fn, cls = 'back') => {
    const b = document.createElement('button');
    b.type = 'button'; b.className = cls; b.textContent = label; b.onclick = fn;
    return b;
  };
  if (f && f !== t?.path) st.append(button('Back to the tile', () => openFile(null)));
  else if (f && !d.writable) {
    st.append(f.startsWith('lib:') ? 'library · ' : 'read only · ');
    if (f.startsWith('lib:')) st.append(button('Make it mine', () => offerMine()));
  } else if (d.conflict != null) {
    st.append('changed on disk: ');
    st.append(button('take the disk’s', () => { const disk = d.conflict; doc.set({ conflict: null, base: disk }); edited(disk, 'disk'); }), ' or ',
      button('keep mine', () => { doc.set({ base: d.conflict, conflict: null }); changed(); }));
  } else if (d.note) st.textContent = d.note;
  else if (d.migrated != null && d.writable && !dirty()) {
    // an older format read as the newest in memory: the file is as it was until this
    st.append(`format ${d.migrated}, read as the newest · `, button('Update this file', () => updateFile()));
  } else if (f) st.textContent = d.saving ? 'saving…' : !dirty() ? 'saved' : !live ? 'not saved yet: waiting for dharmatiles preview' : d.checked === d.text && !d.ok ? 'not saved: a mistake' : 'editing…';
  // the first mistake, in words, under the text
  const hint = $('codehint');
  const e = d.checked === d.text && !d.ok ? d.errors.find((x) => !x.file || x.file === f) || d.errors[0] : null;
  hint.classList.toggle('bad', !!e);
  // the hint shows until the first edit; a mistake always shows, on one line (all of it on hover)
  if (dirty()) { try { localStorage.setItem('dt-hint-seen', '1'); } catch { /* private mode: the default stands */ } }
  let seen = false;
  try { seen = localStorage.getItem('dt-hint-seen') === '1'; } catch { /* private mode: the default stands */ }
  hint.textContent = e ? `${e.line ? `Line ${e.line}: ` : ''}${e.message}.${e.help ? ` ${e.help[0].toUpperCase()}${e.help.slice(1)}` : ''}`
    : d.writable ? 'Type to change the tile; it saves as you go. ⌃Space: what can go here; ? for keys.' : 'Click a line to select it in the view.';
  hint.title = hint.textContent;
  hint.hidden = !e && d.writable && seen;
}

// ---- completion -------------------------------------------------------------

const comp = { s: null, i: 0 };
function openComp(force) {
  const elements = server.get().elements;
  if (!doc.get().writable || !elements.length) return closeComp();
  const s = suggest(texts.caret(), texts.text, elements, force, doc.get().format >= 1 ? doc.get().map?.language : null);
  if (!s) return closeComp();
  comp.s = s; comp.i = 0;
  renderComp();
}
function renderComp() {
  const box = $('comp');
  box.replaceChildren();
  comp.s.items.forEach((it, i) => {
    const d = document.createElement('div');
    d.className = `it fam-${FAMILY[it.label] || (it.kind === 'definition' ? 'plant' : 'doc')}${i === comp.i ? ' on' : ''}`;
    d.setAttribute('role', 'option');
    d.setAttribute('aria-selected', i === comp.i);
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
  const follow = () => { if (view.get().tileId != null || tile()) loadText(view.get().textFile); else if (server.get().ready) clearDocument(); };
  view.subscribe(['tileId', 'textFile'], follow);
  server.subscribe(['tiles'], follow);
  view.subscribe(['tileId'], closeComp);
  doc.subscribe(null, renderTextBar);
  server.subscribe(['live', 'tiles'], renderTextBar);
  // the server was away and is back: an edit it never got is checked and saved now
  let wasLive = server.get().live;
  server.subscribe(['live'], ({ live }) => {
    if (live && !wasLive && tile() && dirty()) { changed(); askDraft(); }
    wasLive = live;
  });
  $('undo').onclick = () => undo(-1);
  $('redo').onclick = () => undo(1);
  view.subscribe(['popover'], ({ popover }) => { if (popover === 'undo-history' || popover === 'redo-history') renderHistory(true); });
  addEventListener('beforeunload', (e) => {
    if (dirty() && doc.get().writable) { e.preventDefault(); e.returnValue = ''; }
  });
  // one undo for the whole page: typing and changes made in the view
  addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && !e.altKey && e.target.tagName !== 'INPUT' && (e.key === 'z' || e.key === 'Z' || e.key === 'y')) {
      e.preventDefault();
      undo(e.key === 'y' || e.shiftKey ? 1 : -1);
    }
  });
  renderTextBar();
}
