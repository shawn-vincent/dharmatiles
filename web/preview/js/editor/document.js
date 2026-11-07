// The document: the text pane, one history, checking, saving and loading the
// text, drafts while a value is dragged, and completion.

import { suggest } from '../complete.js';
import { describeChange } from '../history.js';
import { parsePayload } from '../payload.js';
import { TextPane, byteToIndex } from '../text.js';
import { $, host, state, tile, viewer } from './app.js';
import { showBuild } from './build.js';
import { closePopovers } from './chrome.js';
import { renderTitle } from './export.js';
import { FAMILY, drawOverlay, surfaceHeights } from './map.js';
import { countPieces } from './open.js';
import { revealSel, setHov, setSel } from './selection.js';
import { lens, renderLens } from './sheet.js';
import { offerMine } from './tiles.js';

// the stack of the text now: the last clean check of it while editing, else the last build's
export const stackItems = () => ((doc.stack && doc.tile === state.selected ? doc.stack : tile()?.plan?.stack) || []).map((s) => (typeof s === 'string' ? { label: s } : s));

// ---- the document: the text, one history, checking and saving -----------------
// The text pane edits the file shown (the tile's document, or a file it
// uses).  Every change is checked at once against the server (the errors
// and the stack, without building) and saved soon after when it reads
// cleanly; the worker sees the saved file and rebuilds.  A text with a
// mistake stays here, unsaved, until it is fixed.

export let elements = [];
export const elementNamed = (n) => elements.find((e) => e.name === n);

export const doc = {
  tile: -1, file: null, base: null, writable: false,
  history: [], at: -1, lastKind: null, lastTime: 0,
  checkTimer: 0, saveTimer: 0, checkSeq: 0, saving: false,
  checked: null, ok: true, errors: [], stack: null, note: null, conflict: null,
  map: null, selectAt: null,
};
export const dirty = () => texts.text != null && doc.base != null && texts.text !== doc.base;

export const texts = new TextPane($('code'), {
  onCaretLine: (n) => { closeComp(); setSel(entryAtLine(n), { from: 'text' }); },
  onHoverLine: (n) => setHov(n == null ? null : entryAtLine(n)),
  onEdit: (text, kind) => {
    edited(text, kind);
    const last = texts.caret().before.slice(-1);
    if (/[\w=+\-.]/.test(last)) openComp(false); else closeComp();
  },
  onReadOnly: () => offerMine(),
  onKey: (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') { save(true); return true; }
    return compKey(e);
  },
  onHistory: (step) => undo(step),
});

/** The innermost entry written on line `n` of the file shown, else the tile (its own file) or nothing. */
function entryAtLine(line) {
  const n = line;
  let best = null, width = Infinity;
  stackItems().forEach((it, i) => {
    if (!it.lines || it.file !== texts.file) return;
    const [a, b] = it.lines;
    if (n >= a && n <= b && b - a < width) { best = i; width = b - a; }
  });
  if (best != null) return { entry: best, piece: null };
  if (texts.file !== tile()?.path) return null;
  for (const [kind, list] of [['region', doc.map?.regions], ['line', doc.map?.lines]]) {
    for (const node of list || []) {
      const l = node.span && spanLines(node.span);
      if (l && line >= l[0] && line <= l[1]) return { [kind]: node.name };
    }
  }
  return { tile: true };
}

/** The lines [first, last] (from 1) that byte span [offset, length] of the text covers. */
function spanLines([o, len]) {
  const t = texts.text || '';
  const a = byteToIndex(t, o), b = byteToIndex(t, o + Math.max(len, 1) - 1);
  return [t.slice(0, a).split('\n').length, t.slice(0, b).split('\n').length];
}
/** The map's region or line a selection names. */
export function mapNode(w) {
  if (!w || !doc.map) return null;
  if (w.region != null) return doc.map.regions.find((r) => r.name === w.region) || null;
  if (w.line != null) return doc.map.lines.find((l) => l.name === w.line) || null;
  return null;
}

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
  b.onclick = () => { goHistory(target); closePopovers(); };
  return b;
}

export function renderHistory(buildMenus = false) {
  const canUndo = doc.writable && doc.at > 0;
  const canRedo = doc.writable && doc.at >= 0 && doc.at < doc.history.length - 1;
  $('undo').disabled = !canUndo; $('undo-drop').disabled = !canUndo;
  $('redo').disabled = !canRedo; $('redo-drop').disabled = !canRedo;
  if (!buildMenus) return;
  const undoMenu = $('undo-history'), redoMenu = $('redo-history');
  undoMenu.replaceChildren(); redoMenu.replaceChildren();
  for (let target = doc.at - 1; target >= 0; target--) {
    undoMenu.append(historyButton(doc.history[target + 1], target, doc.at - target, 'Undo'));
  }
  for (let target = doc.at + 1; target < doc.history.length; target++) {
    redoMenu.append(historyButton(doc.history[target], target, target - doc.at, 'Redo'));
  }
}

function resetHistory(text) {
  doc.history = [{ text, caret: 0, kind: 'open', label: 'Opened document' }];
  doc.at = 0; doc.lastKind = null;
  renderHistory();
}
export function clearDocumentHistory() {
  doc.writable = false; doc.base = null; doc.history = []; doc.at = -1; doc.lastKind = null;
  renderHistory();
}
function record(text, kind, label = null) {
  const now = performance.now();
  const join = kind === 'type' && doc.lastKind === 'type' && now - doc.lastTime < 1000 && doc.at > 0;
  doc.history.length = doc.at + 1;
  const before = doc.history[join ? doc.at - 1 : doc.at]?.text || '';
  const change = describeChange(before, text, kind, label);
  const h = { text, caret: texts.selectionStart, kind, label: change.title, change };
  if (join) doc.history[doc.at] = h; else { doc.history.push(h); doc.at++; }
  if (doc.history.length > 400) { doc.history.shift(); doc.at--; }
  doc.lastKind = kind; doc.lastTime = now;
  renderHistory();
}
function goHistory(i) {
  if (!doc.writable || i < 0 || i >= doc.history.length) return;
  doc.at = i; doc.lastKind = null;
  const h = doc.history[i];
  texts.setText(h.text, h.caret);
  closeComp();
  changed();
  renderHistory();
}
export function undo(step) { goHistory(doc.at + step); }

/** The user changed the text: `kind` is type, lens, or drag (a scrub under way, kept out of the history until it ends). */
export function edited(text, kind, label = null) {
  if (kind !== 'drag') record(text, kind, label);
  else askDraft();
  changed();
}

// drafts: while a value is dragged the view shows quick builds of the text
// as it is (marked draft), one in flight at a time, always the latest text;
// the full build follows when the hand lets go and the file is saved
export const drafts = { sent: 0, waiting: false, again: false, shown: false };
export async function askDraft() {
  const t = tile();
  if (!t || !texts.file) return;
  if (drafts.waiting) { drafts.again = true; return; }
  drafts.waiting = true; drafts.again = false;
  drafts.sent += 1;
  await host.draft(t.index, { text: texts.text, file: texts.file === t.path ? undefined : texts.file, id: drafts.sent });
  setTimeout(() => { if (drafts.waiting) { drafts.waiting = false; if (drafts.again) askDraft(); } }, 4000); // a draft that never came
}
export async function draftArrived(t) {
  drafts.waiting = false;
  if (drafts.again) askDraft();
  if (t.draft_id !== drafts.sent || t.index !== state.selected) return; // an older text
  try {
    const p = parsePayload(await host.fetchMesh(t.index, true));
    if (t.index !== state.selected || t.draft_id !== drafts.sent) return;
    viewer.setMesh(p);
    countPieces(p);
    surfaceHeights(p);
    drafts.shown = true;
    showBuild(state.tiles[t.index]);
    renderTitle();
  } catch (e) { console.error(e); }
}
export function changed() {
  clearTimeout(doc.checkTimer);
  doc.checkTimer = setTimeout(check, 120);
  clearTimeout(doc.saveTimer);
  if (!lens.dragging) doc.saveTimer = setTimeout(save, 500);
  renderTextBar();
}

async function check() {
  const t = tile();
  if (!t || texts.text == null) return false;
  const text = texts.text, seq = ++doc.checkSeq;
  const r = await host.check(t.index, { text, file: texts.file === t.path ? undefined : texts.file });
  if (r.status === 0) return false; // the server is away: not checked yet, and checked again when it is back
  if (seq !== doc.checkSeq || text !== texts.text || !r.body || t !== tile()) return false;
  doc.note = null;
  doc.checked = text;
  doc.ok = !!r.body.ok;
  doc.errors = r.body.errors || [];
  if (r.body.ok && r.body.stack) doc.stack = r.body.stack; // the last good stack stays while there are mistakes
  if (r.body.ok && r.body.map) doc.map = r.body.map;
  if (doc.selectAt != null && r.body.ok) {
    // the node a gesture just wrote: select it
    const at = doc.selectAt; doc.selectAt = null;
    const i = (doc.stack || []).findIndex((it) => it.span && it.span[0] === at && it.file === texts.file);
    if (i >= 0) { setSel({ entry: i, piece: null }, { from: 'view' }); }
  }
  markText(); renderTextBar(); renderLens(); drawOverlay();
  return true;
}

async function save(now = false) {
  if (!doc.writable || !dirty()) return !dirty();
  if (doc.saving) return false;
  if (!state.live) { renderTextBar(); return false; } // saved when the server is back
  if (doc.checked !== texts.text) {
    if (now) await check();
    else { doc.saveTimer = setTimeout(save, 250); return false; }
  }
  if (doc.checked !== texts.text || !doc.ok) { renderTextBar(); return false; } // a mistake: kept here until it is fixed
  const t = tile(), text = texts.text, file = texts.file;
  if (!t || !file) return false;
  doc.saving = true;
  renderTextBar();
  try {
    const r = await host.save(t.index, { text, base: doc.base, file: file === t.path ? undefined : file });
    doc.note = null;
    if (r.status === 200) { doc.base = text; doc.conflict = null; }
    else if (r.status === 409) doc.conflict = r.body?.text ?? '';
    else doc.note = r.body?.error || `could not save (${r.status})`;
  } finally {
    doc.saving = false;
    renderTextBar();
  }
  if (dirty() && !doc.conflict && !doc.note) changed(); // typed while it saved
  return !dirty();
}

let leaving = null;
function confirmDiscard() {
  if (leaving) return leaving;
  const dlg = $('leave-dialog');
  $('leave-reason').textContent = doc.conflict != null
    ? 'The file changed on disk while your version was open. Choose which version to keep before leaving.'
    : !doc.ok ? 'This edit contains a mistake, so it has not been saved.'
      : doc.note || 'This edit has not been saved yet.';
  dlg.returnValue = ''; // showModal keeps the last choice: Esc must mean keep editing
  dlg.showModal();
  leaving = new Promise((resolve) => {
    dlg.addEventListener('close', () => {
      const discard = dlg.returnValue === 'discard';
      if (discard && doc.base != null) {
        texts.setText(doc.base);
        resetHistory(doc.base);
        doc.checked = doc.base; doc.ok = true; doc.errors = []; doc.conflict = null; doc.note = null;
      }
      leaving = null;
      resolve(discard);
    }, { once: true });
  });
  return leaving;
}

/** Save before leaving this file; if it cannot be saved, make discarding explicit. */
export async function canLeave() {
  if (!dirty() || !doc.writable) return true;
  clearTimeout(doc.checkTimer); clearTimeout(doc.saveTimer);
  if (await save(true)) return true;
  return confirmDiscard();
}

let textToken = 0;
let carried = null; // the undo history a rename brings to the renamed file
/** Bring the undo history along to the file a rename is about to make. */
export function carryHistory() { carried = { history: doc.history, at: doc.at }; }
export async function loadText(file) {
  const t = tile();
  if (!t || /\.stl$/i.test(t.path)) { texts.clear(); state.textFor = null; clearDocumentHistory(); renderTextBar(); return; }
  const want = file || t.path;
  if (dirty() && texts.file !== want && !(await canLeave())) return false;
  // unsaved changes to this file stay; a new build of the same file brings nothing new
  if (dirty() && texts.file === want && doc.tile === t.index) return true;
  const key = [t.index, want, t.mesh_version, t.status, t.errors.length];
  if (state.textFor && state.textFor.join('|') === key.join('|')) return;
  state.textFor = key;
  const token = ++textToken;
  try {
    const v = await host.text(t.index, want === t.path ? null : want);
    if (token !== textToken || !v) { if (!v) state.textFor = null; return false; }
    if (dirty() && texts.file === v.file && doc.tile === t.index) return true;
    const fresh = doc.tile !== t.index || texts.file !== v.file;
    doc.tile = t.index;
    doc.writable = v.writable;
    doc.base = v.text;
    doc.conflict = null; doc.note = null;
    texts.show(v.file, v.text, v.writable);
    if (fresh && carried && carried.history[carried.at]?.text === v.text) {
      doc.history = carried.history; doc.at = carried.at; renderHistory(); // a rename keeps its undo
    } else if (fresh) resetHistory(v.text);
    else if (doc.history[doc.at]?.text !== v.text) record(v.text, 'disk'); // changed on disk: one more step
    doc.checked = v.text; doc.ok = !t.errors.length; doc.errors = []; doc.stack = null; // the build's plan is this text's
    if (fresh) { doc.map = null; carried = null; }
    if (v.file === t.path && !doc.map) check(); // its map, for the overlay
    renderTextBar();
    markText();
    renderLens();
    return true;
  } catch (e) { console.error(e); state.textFor = null; return false; }
}

export function renderTextBar() {
  const t = tile();
  const f = texts.file;
  // the tile's own file is named in the crumb above; another file (a part) by its path
  $('textpath').textContent = !f || f === t?.path ? '' : `‎${f}`;
  $('textpath').title = f || '';
  const st = $('textstate');
  st.replaceChildren();
  const button = (label, fn, cls = 'back') => {
    const b = document.createElement('button');
    b.type = 'button'; b.className = cls; b.textContent = label; b.onclick = fn;
    return b;
  };
  if (f && f !== t?.path) st.append(button('Back to the tile', () => loadText(null).then(() => { markText(); revealSel(); })));
  else if (f && !doc.writable) {
    st.append(f.startsWith('lib:') ? 'library · ' : 'read only · ');
    if (f.startsWith('lib:')) st.append(button('Make it mine', () => offerMine()));
  } else if (doc.conflict != null) {
    st.append('changed on disk: ');
    st.append(button('take the disk’s', () => { const d = doc.conflict; doc.conflict = null; doc.base = d; texts.setText(d); record(d, 'disk'); changed(); }), ' or ',
      button('keep mine', () => { doc.base = doc.conflict; doc.conflict = null; changed(); }));
  } else if (doc.note) st.textContent = doc.note;
  else if (f) st.textContent = doc.saving ? 'saving…' : !dirty() ? 'saved' : !state.live ? 'not saved yet: waiting for dharmatiles preview' : doc.checked === texts.text && !doc.ok ? 'not saved: a mistake' : 'editing…';
  // the first mistake, in words, under the text
  const hint = $('codehint');
  const e = doc.checked === texts.text && !doc.ok ? doc.errors.find((x) => !x.file || x.file === texts.file) || doc.errors[0] : null;
  hint.classList.toggle('bad', !!e);
  // the hint shows until the first edit; a mistake always shows, on one line (all of it on hover)
  if (dirty()) { try { localStorage.setItem('dt-hint-seen', '1'); } catch { /* ignore */ } }
  let seen = false;
  try { seen = localStorage.getItem('dt-hint-seen') === '1'; } catch { /* ignore */ }
  hint.textContent = e ? `${e.line ? `Line ${e.line}: ` : ''}${e.message}.${e.help ? ` ${e.help[0].toUpperCase()}${e.help.slice(1)}` : ''}`
    : doc.writable ? 'Type to change the tile; it saves as you go. ⌃Space: what can go here; ? for keys.' : 'Click a line to select it in the view.';
  hint.title = hint.textContent;
  hint.hidden = !e && doc.writable && seen;
}

export function linesOf(w) {
  if (!w) return null;
  if (w.tile) return texts.file === tile()?.path ? [1, (texts.text || '').split('\n').length] : null;
  if (w.region != null || w.line != null) {
    const n = mapNode(w);
    return n?.span && texts.file === tile()?.path ? spanLines(n.span) : null;
  }
  const it = stackItems()[w.entry];
  return it?.lines && it.file === texts.file ? it.lines : null;
}

export function markText() {
  const t = tile();
  const live = doc.checked === texts.text && doc.tile === state.selected;
  const errs = (live ? doc.errors : t?.errors || []).filter((e) => (!e.file || e.file === texts.file) && e.line)
    .map((e) => ({ line: e.line, message: `${e.message}${e.help ? `. ${e.help}` : ''}` }));
  texts.mark(state.sel?.tile ? null : linesOf(state.sel), linesOf(state.hov), errs);
}

// ---- completion -------------------------------------------------------------

const comp = { s: null, i: 0 };
function openComp(force) {
  if (!doc.writable || !elements.length) return closeComp();
  const s = suggest(texts.caret(), texts.text, elements, force);
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
export function closeComp() { comp.s = null; $('comp').hidden = true; }
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
  host.elements().then((v) => { elements = v; renderLens(); }).catch(() => {});
  state.doc = doc;
  $('undo').onclick = () => undo(-1);
  $('redo').onclick = () => undo(1);
  addEventListener('beforeunload', (e) => {
    if (dirty() && doc.writable) { e.preventDefault(); e.returnValue = ''; }
  });
}
