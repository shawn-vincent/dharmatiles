// @ts-check
// The printer chooser: brand → model with a search, an AMS switch for a model that has an AMS
// variant, your own profiles in their own group, `verify` profiles marked unchecked, and an
// import from the slicers installed here.  One widget for every place a printer is chosen: the
// title bar's target card and the plates panel.

import { failed } from '../problems.js';
import { groupPrinters, selectionOf, sizeWords } from '../printers.js';
import { host, toast } from './app.js';

const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

const searches = new Map(); // what was typed, by chooser, so a redraw keeps it
let slicers = null; // the slicers installed here, asked once
let asking = false;
const waiting = new Set();

function askSlicers(done) {
  waiting.add(done);
  if (slicers || asking) { if (slicers) done(); return; }
  asking = true;
  host.printersScan().then((r) => {
    // no slicer, or a server that cannot say: the chooser simply has no import entry
    slicers = r.status === 200 && 'slicers' in r.body ? r.body.slicers : [];
    for (const f of waiting) f();
  }).catch((e) => { slicers = []; failed('Could not look for slicers', { status: 0, body: { error: String(e?.message || e) } }); })
    .finally(() => { asking = false; });
}

/**
 * `{printers, current, onPick, needBed, disabled, key}`: the chooser as an element.  `onPick(name)`
 * is called with the profile name (the AMS variant's when the switch is on).
 */
export function printerChooser({ printers, current, onPick, needBed = false, disabled = false, key = 'pc' }) {
  const root = el('div', 'pc');
  let mode = 'pick';
  /** @type {{slicer: string, label: string, title?: string, models: any[] | null, problems: string[], chosen: Set<string>, query: string, busy: boolean, note: string} | null} */
  let importing = null; // {slicer, label, models, problems, chosen:Set, query, busy}

  const draw = () => { root.replaceChildren(mode === 'pick' ? pick() : importView()); };

  function pick() {
    const box = el('div', 'pc-pick');
    const q = el('input');
    q.type = 'search'; q.placeholder = 'Search printers'; q.setAttribute('aria-label', 'Search printers');
    q.value = searches.get(key) || '';
    box.append(q);
    const list = el('div', 'pc-list');
    list.setAttribute('role', 'listbox');
    const sel = selectionOf(printers, current);
    const fill = () => {
      const groups = groupPrinters(printers, { needBed, query: q.value });
      list.replaceChildren();
      if (!groups.length) list.append(el('p', 'pc-none', 'No printer matches.'));
      for (const g of groups) {
        const h = el('div', `pc-group ${g.kind}`, g.label);
        list.append(h);
        for (const m of g.models) list.append(row(m));
      }
    };
    const row = (m) => {
      const b = el('button', `pc-row${m.name === sel.model ? ' on' : ''}`);
      b.type = 'button'; b.disabled = disabled || !!m.p.broken;
      b.setAttribute('role', 'option'); b.setAttribute('aria-selected', String(m.name === sel.model));
      b.dataset.printer = m.name;
      const top = el('span', 'pc-top');
      top.append(el('span', 'pc-name', m.title));
      if (m.p.verify) { const v = el('span', 'verify', 'unchecked'); v.title = `Not yet checked against the maker: ${m.p.verify}`; top.append(v); }
      if (m.p.fromSlicer) { const v = el('span', 'own', 'imported'); v.title = `From ${m.p.fromSlicer}`; top.append(v); }
      b.append(top, el('span', 'pc-meta', m.p.broken ? `does not read: ${m.p.broken}` : `${sizeWords(m.p)}${m.ams ? ' · AMS available' : ''}`));
      // choosing a model keeps the AMS switch where it was, when the new model has one
      b.onclick = () => onPick(sel.ams && m.ams ? m.ams.name : m.name);
      return b;
    };
    q.oninput = () => { searches.set(key, q.value); fill(); };
    fill();
    // the chosen model is in view when the list opens (the list scrolls itself, not the panel round it)
    requestAnimationFrame(() => {
      const on = list.querySelector('.pc-row.on');
      if (on && !q.value) list.scrollTop = Math.max(0, on.offsetTop - list.clientHeight / 2 + on.offsetHeight / 2);
    });
    box.append(list);
    // the AMS question: a switch on the model, not a model of its own
    const cur = printers.find((p) => p.name === sel.model);
    const withAms = cur && printers.find((p) => p.name === `${cur.name}-ams`);
    if (withAms) {
      const s = el('div', 'seg pc-ams');
      s.setAttribute('role', 'group'); s.setAttribute('aria-label', 'AMS');
      for (const [name, text, title] of [[cur.name, 'No AMS', 'One filament at a time'], [withAms.name, 'AMS', `${withAms.slots} filaments`]]) {
        const b = el('button', null, text);
        b.type = 'button'; b.title = title; b.disabled = disabled;
        b.setAttribute('aria-pressed', String(name === current));
        b.onclick = () => { if (name !== current) onPick(name); };
        s.append(b);
      }
      box.append(s);
    }
    // the importers
    const foot = el('div', 'pc-foot');
    const addImports = () => {
      foot.replaceChildren();
      for (const s of slicers || []) {
        const b = el('button', 'btn pc-import', `Import from ${s.title}…`);
        b.type = 'button'; b.title = `Read ${s.label}'s machine profiles into your printers`; b.dataset.slicer = s.slicer;
        b.onclick = () => startImport(s);
        foot.append(b);
      }
    };
    addImports();
    askSlicers(addImports);
    box.append(foot);
    return box;
  }

  async function startImport(s) {
    importing = { slicer: s.slicer, label: s.label, title: s.title, models: null, problems: [], chosen: new Set(), query: '', busy: false, note: '' };
    mode = 'import';
    draw();
    try {
      const r = await host.printersScan(s.slicer);
      if (r.status !== 200) { failed(`Could not read ${s.title}`, r); mode = 'pick'; draw(); return; }
      if (!('models' in r.body)) throw new Error('the scan of a slicer answered no models');
      importing.models = r.body.models; importing.problems = r.body.problems; importing.label = r.body.label;
      draw();
    } catch (e) { failed(`Could not read ${s.title}`, { status: 0, body: { error: String(e?.message || e) } }); mode = 'pick'; draw(); }
  }

  function importView() {
    const im = importing;
    const box = el('div', 'pc-importview');
    const head = el('div', 'pc-ihead');
    const back = el('button', 'btn', '‹ Back');
    back.type = 'button'; back.onclick = () => { mode = 'pick'; draw(); };
    head.append(back, el('span', 'pc-ititle', `Import from ${im.label}`));
    box.append(head);
    if (!im.models) { box.append(el('p', 'pc-none', `Reading ${im.title}'s profiles…`)); return box; }
    box.append(el('p', 'note', 'Your own copies of the bed, height, nozzle, zones and filaments, in your printers folder. The slicer is not changed.'));
    const q = el('input');
    q.type = 'search'; q.placeholder = `Search ${im.models.length} models`; q.value = im.query; q.setAttribute('aria-label', 'Search the slicer\'s models');
    box.append(q);
    const list = el('div', 'pc-list');
    const go = el('button', 'btn on pc-go');
    go.type = 'button';
    const refresh = () => {
      go.textContent = im.busy ? 'Importing…' : im.chosen.size ? `Import ${im.chosen.size} model${im.chosen.size === 1 ? '' : 's'}` : 'Choose models to import';
      go.disabled = im.busy || !im.chosen.size;
    };
    const fill = () => {
      list.replaceChildren();
      const words = im.query.toLowerCase().split(/\s+/).filter(Boolean);
      let brand = null, n = 0;
      for (const m of im.models) {
        const hay = `${m.brand} ${m.model} ${m.name}`.toLowerCase();
        if (!words.every((w) => hay.includes(w))) continue;
        n += 1;
        if (m.brand !== brand) { brand = m.brand; list.append(el('div', 'pc-group brand', brand)); }
        const lab = el('label', `pc-irow${m.have ? ' have' : ''}`);
        const c = el('input');
        c.type = 'checkbox'; c.checked = im.chosen.has(m.name); c.disabled = m.have || im.busy;
        c.onchange = () => { if (c.checked) im.chosen.add(m.name); else im.chosen.delete(m.name); refresh(); };
        const t = el('span', 'pc-top');
        t.append(el('span', 'pc-name', m.model));
        if (m.have) t.append(el('span', 'own', 'in yours'));
        const meta = `${m.bed[0]} × ${m.bed[1]} mm${m.height ? ` · ${m.height} mm tall` : ''} · ${m.slots === 1 ? '1 filament' : `${m.slots} filaments`}${m.zones ? ` · ${m.zones} zone${m.zones === 1 ? '' : 's'} kept clear` : ''}`;
        lab.append(c, el('span', 'pc-itext'));
        lab.lastChild.append(t, el('span', 'pc-meta', meta));
        list.append(lab);
      }
      if (!n) list.append(el('p', 'pc-none', 'No model matches.'));
    };
    q.oninput = () => { im.query = q.value; fill(); };
    fill(); refresh();
    box.append(list);
    if (im.problems.length) {
      const d = el('details', 'pc-problems');
      d.append(el('summary', null, `${im.problems.length} profile${im.problems.length === 1 ? '' : 's'} could not be used`));
      const ul = el('ul');
      for (const p of im.problems.slice(0, 40)) ul.append(el('li', null, p));
      if (im.problems.length > 40) ul.append(el('li', null, `… and ${im.problems.length - 40} more`));
      d.append(ul);
      box.append(d);
    }
    go.onclick = async () => {
      im.busy = true; refresh();
      try {
        const r = await host.printersImport({ slicer: im.slicer, names: [...im.chosen] });
        if (r.status !== 200) { failed('Could not import the printers', r); im.busy = false; refresh(); return; }
        const done = r.body.written;
        const kept = done.filter((w) => w.kept).length;
        toast(`Imported ${done.length - kept} profile${done.length - kept === 1 ? '' : 's'}${kept ? `, ${kept} already yours` : ''}`);
        mode = 'pick'; draw();
      } catch (e) { failed('Could not import the printers', { status: 0, body: { error: String(e?.message || e) } }); im.busy = false; refresh(); }
    };
    box.append(go);
    return box;
  }

  draw();
  return root;
}
