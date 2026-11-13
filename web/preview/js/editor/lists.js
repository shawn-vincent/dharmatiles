// Starting and filling a print list from the editor (docs/plates.html, "In the editor").
//
//   * New print list: a button in a folder you own; it asks a name and writes `<name>.dmap`, an empty map laid out
//     for the printer last chosen, and opens it on its plates.
//   * Add to print list: on a tile (its level, a row's menu) pick a list, or a new one.  One more copy goes in; a
//     piece already there at the same seed and system has its count raised.
//   * In the plates' panel a piece's count and its removal (plates.js calls `setCount` and `removePiece`).
//
// Every one is an edit of the list's text made by the program (`/api/lists/*`); each answers with the text it
// replaced and the text it wrote, which join the one history (editor/docs.js): Undo and Redo, the toast's too.

import { failed, showError } from '../problems.js';
import { openMenu } from '../menu.js';
import { host, toast } from './app.js';
import { nameField } from './rows.js';
import { say } from './inventory.js';
import { docs, undoToast } from './docs.js';
import { server, view } from './stores.js';

const el = (tag, cls, text) => {
  const x = document.createElement(tag);
  if (cls) x.className = cls;
  if (text != null) x.textContent = text;
  return x;
};

/** The print lists the program lists. */
export const lists = () => server.get().tiles.filter((t) => /\.dmap$/i.test(t.path));

/** Open a document by its id, on the editor page. */
async function openId(id, mode = null) {
  const { select } = await import('./open.js');
  if (mode) { const t = server.get().tiles.find((x) => x.id === id); if (t) (await import('./scene.js')).setChosen(t.path, mode); }
  if ((await select(id)) !== false) view.set({ page: 'edit' });
}

/** A call that edits a list: the program's refusal shown, the success returned; null when it did not go through. */
async function call(label, run) {
  try {
    const r = await run();
    if (r.status !== 200) { failed(label, r); return null; }
    if (r.body.ok === false) { say(r.body, label); return null; }
    return r.body;
  } catch (e) { showError(label, e); return null; }
}

/** Add one copy of tile `t` to the list `listId`, or to a new list when null. */
export async function addTo(t, listId) {
  const b = await call('Could not add to the print list', () => host.listsAdd({ tile: t.id, list: listId }));
  if (!b) return;
  const name = b.path.split(/[\\/]/).pop().replace(/\.dmap$/i, '');
  if (b.made) {
    toast(`Made ${name} with ${t.name}`, () => openId(b.list), { label: 'Open' });
    return;
  }
  const entry = docs.adopt(b.list, b.count > 1 ? `Raised ${t.name} to ${b.count}` : `Added ${t.name}`, b);
  toast(b.count > 1 ? `${t.name} is ${b.count} in ${name}` : `Added ${t.name} to ${name}`, entry ? () => undoToast(b.list, entry) : null);
}

/** The menu of lists a tile can go to: each existing one, and a new one. */
export function listMenu(t, anchor) {
  const items = lists().map((l) => ({ label: l.name, sub: 'add one copy', run: () => addTo(t, l.id) }));
  if (items.length) items.push({ sep: true });
  items.push({ label: 'New list…', sub: 'a new print list with this tile', run: () => addTo(t, null) });
  openMenu(anchor, [{ head: `Add ${t.name} to` }, ...items], { label: `Add ${t.name} to a print list`, cls: 'addmenu', align: 'right' });
}

/** The tile level's block: add the tile to a print list. */
export function listBlock(t) {
  const box = el('section', 'inv-block');
  box.append(el('h3', null, 'Print list'));
  const ask = el('div', 'inv-ask');
  const s = el('select', 'word-select');
  s.setAttribute('aria-label', 'Print list');
  for (const l of lists()) { const o = el('option', null, l.name); o.value = l.id; s.append(o); }
  const fresh = el('option', null, 'A new list'); fresh.value = '';
  s.append(fresh);
  const b = el('button', 'btn', 'Add to print list');
  b.type = 'button';
  b.title = 'One more copy of this tile in the list; a tile already there at the same seed has its count raised';
  b.onclick = () => addTo(t, s.value || null);
  ask.append(s, b);
  box.append(ask);
  return box;
}

/** New print list in folder `dir` of place `root`: asks a name, writes it, opens it. */
export function newList(root, dir, anchor, where, map = false) {
  nameField(anchor, {
    title: map ? 'New map' : 'New print list',
    value: map ? 'Map' : 'Print list',
    hint: map
      ? `A .dmap in ${where}: an empty map on a grid of squares. Open the catalog to put tiles on it.`
      : `A .dmap in ${where}: the tiles to print, laid out for the printer last chosen. Add tiles to it from their level.`,
    flaw: (v) => (v.trim().length > 80 ? 'At most 80 characters' : null),
    save: async (v) => {
      const r = await host.listsNew({ name: v, root, dir, map });
      if (r.status !== 200) { failed(map ? 'Could not make the map' : 'Could not make the print list', r); return; }
      if (r.body.id) { await openId(r.body.id, map ? 'map' : null); toast(`Made ${r.body.path.split(/[\\/]/).pop()}`); }
    },
  });
}

/** Set a piece (by id) of the list `listId` to `count` copies. */
export async function setCount(listId, id, count) {
  const r = await docs.edit(listId, () => ({ label: 'Could not change the count', note: `Now ${count}`, steps: [{ name: 'listsCount', body: { id, count } }] }));
  if (r) toast(r.note, () => undoToast(listId, r.entry));
}

/** Take a piece (by id) out of the list `listId`. */
export async function removePiece(listId, id, name) {
  const r = await docs.edit(listId, () => ({ label: 'Could not take the piece out', note: `Took ${name} out of the list`, steps: [{ name: 'listsRemove', body: { id } }] }));
  if (r) toast(r.note, () => undoToast(listId, r.entry));
}
