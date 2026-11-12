// What the tile level holds, as rows: the lines and regions that shape the tile, and
// the stack in the order it builds.  Each row is one button that opens its thing's
// level.  A page here (it makes elements), but it reads no store: the caller gives it
// the data, and what a row does is the caller's.

import { badgeOf, placeOf } from './nav.js';
import { clauseTitle } from './editor/region-sheet.js';

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

const plural = (n, one, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** What a region says of itself in a few words: what bounds it, or its clauses. */
export function regionSummary(m) {
  const sh = m.sheet;
  if (!sh) return 'region';
  const parts = [];
  if (sh.bounded_by?.text) parts.push(`bounded by ${sh.bounded_by.text}`);
  for (const c of sh.clauses || []) parts.push(clauseTitle(c));
  return parts.join('; ') || 'region';
}

/** What a line says of itself: its points. */
export const lineSummary = (m) => (m.points?.length ? plural(m.points.length, 'point') : 'line');

/**
 * The rows.  `data`: `{lines, regions, stack, badges, selected}` (the map's lines and regions; the
 * stack items in written order; the messages by entry key; the entry key the level above had open);
 * `open(kind, id)` opens a row's level; `hover(w | null)` lights it in the view.
 * Returns the root element; arrow keys move between rows (`rows()` lists them).
 */
export function buildOutline(data, { open, hover }) {
  const root = el('div', 'outline');
  root.setAttribute('role', 'listbox');
  root.setAttribute('aria-label', 'What the tile holds');
  const row = (kind, id, cls) => {
    const b = el('button', `orow ${cls || ''}`);
    b.type = 'button'; b.setAttribute('role', 'option');
    b.dataset.kind = kind; b.dataset.id = id;
    b.onclick = () => open(kind, id);
    b.onmouseenter = () => hover?.({ kind, id });
    b.onmouseleave = () => hover?.(null);
    return b;
  };
  const text = (label, sub) => {
    const t = el('span', 'tx');
    t.append(el('span', 'nm', label));
    if (sub) t.append(el('span', 'sub', sub));
    return t;
  };
  const section = (title, hint) => {
    const h = el('div', 'olab');
    h.append(el('span', '', title));
    if (hint) h.append(el('small', '', hint));
    root.append(h);
  };
  section('Stack', 'builds top to bottom');
  const items = data.stack || [];
  if (!items.length) root.append(el('p', 'oempty', 'Nothing in the stack yet.'));
  items.forEach((it, i) => {
    const b = row('entry', it.key, `fam-${it.family || 'thing'}`);
    const badge = badgeOf(data.badges?.get?.(it.key));
    b.append(el('span', 'no', String(i + 1)), el('i', 'gl'), text(it.label, [it.what && it.what !== it.label ? it.what : '', placeOf(it)].filter(Boolean).join(' · ')));
    if (badge) {
      const x = el('span', `badge ${badge.tone}`, badge.n > 1 ? String(badge.n) : '');
      x.title = badge.title;
      x.setAttribute('role', 'img');
      x.setAttribute('aria-label', badge.n === 1 ? (badge.tone === 'error' ? 'a mistake' : 'a note') : `${badge.n} messages`);
      b.append(x);
    }
    b.append(el('span', 'chev', '›'));
    root.append(b);
  });
  const shaped = [...(data.lines || []).map((m) => ['line', m]), ...(data.regions || []).map((m) => ['region', m])];
  if (shaped.length) {
    section('Shape', 'lines and regions');
    for (const [kind, m] of shaped) {
      const b = row(kind, m.name);
      b.append(el('i', `gl ${kind}`), text(m.name, kind === 'line' ? lineSummary(m) : regionSummary(m)), el('span', 'chev', '›'));
      root.append(b);
    }
  }
  return root;
}

/** The rows of an outline, in order. */
export const rows = (root) => [...root.querySelectorAll('.orow')];
