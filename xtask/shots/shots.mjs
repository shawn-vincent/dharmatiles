// Draws the editor's pictures for the docs.  Run by `cargo xtask shots` (xtask/src/shots.rs), which starts the editor
// and Chrome and passes their ports; read that file first.
//
//   node shots.mjs --editor http://127.0.0.1:PORT/ --chrome CDP_PORT --out DIR [--only name ...]
//
// It walks the editor through each state the docs describe, once for the light theme and once for the dark, and for each
// one writes DIR/<name>-<theme>.webp (1512 x 860) and records in DIR/shots.json where the parts of the page the docs
// call out are (the docs draw their numbered callouts from those boxes, so nothing is burned into a picture).
//
// Nothing here is swallowed: a state that does not come up, a callout whose part of the page is not there, or a page that
// shows its boot error stops the run with what was being waited for.  A callout that is missing means the editor changed:
// fix its line in ANCHORS, not the docs.

import fs from 'node:fs';
import path from 'node:path';

const WIDTH = 1512, HEIGHT = 860;
const args = process.argv.slice(2);
const opt = (name) => { const i = args.indexOf(`--${name}`); return i < 0 ? null : args[i + 1]; };
const editor = opt('editor'), chromePort = opt('chrome'), outDir = opt('out');
if (!editor || !chromePort || !outDir) throw new Error('usage: shots.mjs --editor URL --chrome PORT --out DIR [--only name ...]');
const oi = args.indexOf('--only');
const only = oi < 0 ? null : args.slice(oi + 1).filter((a) => !a.startsWith('--'));

// ---- the page, over the DevTools protocol ----------------------------------------------------------------------------
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = await (await fetch(`http://127.0.0.1:${chromePort}/json`)).json();
const pageTarget = targets.find((t) => t.type === 'page');
if (!pageTarget) throw new Error(`Chrome on port ${chromePort} has no page`);
const ws = new WebSocket(pageTarget.webSocketDebuggerUrl);
await new Promise((res, rej) => { ws.onopen = res; ws.onerror = () => rej(new Error('cannot connect to Chrome')); });
let nextId = 0;
const pending = new Map();
const pageErrors = [];
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  else if (m.method === 'Runtime.exceptionThrown') pageErrors.push(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text);
};
const send = (method, params = {}, timeout = 240000) => new Promise((resolve, reject) => {
  const id = ++nextId;
  const timer = setTimeout(() => { pending.delete(id); reject(new Error(`${method} did not answer in ${timeout / 1000} s`)); }, timeout);
  pending.set(id, (m) => { clearTimeout(timer); m.error ? reject(new Error(`${method}: ${m.error.message}`)) : resolve(m.result); });
  ws.send(JSON.stringify({ id, method, params }));
});
/** Run `expr` in the page and return its value; a thrown error stops the run. */
async function js(expr) {
  const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw new Error(`in the page: ${r.exceptionDetails.exception?.description ?? r.exceptionDetails.text}\n  ${expr}`);
  return r.result.value;
}
/** Poll `expr` until it is truthy; say what was being waited for if it never is. */
async function waitFor(what, expr, ms = 120000) {
  const t = Date.now();
  for (;;) {
    if (await js(expr)) return;
    if (Date.now() - t > ms) throw new Error(`waited ${ms / 1000} s for ${what}\n  ${expr}\n  the page says: ${await js("document.getElementById('sb-main')?.innerText ?? '(no status line)'")}`);
    await sleep(250);
  }
}

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', { width: WIDTH, height: HEIGHT, deviceScaleFactor: 1, mobile: false });

// ---- helpers living in the page -------------------------------------------------------------------------------------
const HELPERS = `
window.__q = (s) => document.querySelector(s);
window.__all = (s) => [...document.querySelectorAll(s)];
window.__text = (s, re) => __all(s).find((e) => re.test(e.textContent.trim()));
window.__row = (name) => __all('.orow').find((e) => e.querySelector('.nm')?.textContent.trim() === name);
// the part of an element that is in view: cut to every scrolling ancestor and to the window, so a long list's callout stops at the panel's edge
window.__vis = (e) => {
  let r = e.getBoundingClientRect();
  let l = r.left, t = r.top, rt = r.right, b = r.bottom;
  for (let a = e.parentElement; a; a = a.parentElement) {
    const o = getComputedStyle(a);
    if (o.overflowX === 'visible' && o.overflowY === 'visible') continue;
    const c = a.getBoundingClientRect();
    l = Math.max(l, c.left); t = Math.max(t, c.top); rt = Math.min(rt, c.right); b = Math.min(b, c.bottom);
  }
  l = Math.max(l, 0); t = Math.max(t, 0); rt = Math.min(rt, innerWidth); b = Math.min(b, innerHeight);
  return { left: l, top: t, right: rt, bottom: b, width: rt - l, height: b - t };
};
window.__R = (...els) => {
  const rs = els.flat().filter(Boolean).map(__vis).filter((r) => r.width > 0 && r.height > 0);
  if (!rs.length) return null;
  const x = Math.min(...rs.map((r) => r.left)), y = Math.min(...rs.map((r) => r.top));
  const x2 = Math.max(...rs.map((r) => r.right)), y2 = Math.max(...rs.map((r) => r.bottom));
  return { x: Math.round(x), y: Math.round(y), w: Math.round(x2 - x), h: Math.round(y2 - y) };
};`;

const idle = `(() => { const t = document.getElementById('sb-main')?.innerText ?? ''; return t.length > 0 && !/building|rebuilding|loading|reconnect|checking|opening|waiting/i.test(t); })()`;

async function press(what, expr) {
  const hit = await js(`(() => { ${HELPERS} const e = (${expr}); if (!e) return false; e.click(); return true; })()`);
  if (!hit) throw new Error(`nothing to press for ${what}: ${expr}`);
}

/** The Places level, fresh: the page loaded again with no address. */
async function places() {
  await send('Page.navigate', { url: editor });
  await waitFor('Places', `!!document.querySelector('.orow[data-kind=place]')`);
  await js(HELPERS);
}
/** Press a row of the folder level by its name and wait for what it opens. */
async function openRow(name, then) {
  await press(`the row ${name}`, `__row(${JSON.stringify(name)})`);
  await waitFor(then.what, then.expr);
}
const folderShows = (name) => ({ what: `the folder ${name}`, expr: `/${name}$/.test(document.getElementById('bcrumbs')?.innerText.trim() ?? '') || /${name}/.test(document.getElementById('bcrumbs')?.innerText ?? '')` });
const docOpen = (name) => ({ what: `${name} open and idle`, expr: `(document.getElementById('tilename')?.innerText ?? '').includes(${JSON.stringify(name)}) && ${idle}` });

// ---- where each callout points ---------------------------------------------------------------------------------------
// Each is a JS expression in the page (helpers above) giving the part of the page the callout is for.  The docs name them by key.
const ANCHORS = {
  folder: {
    trail: `__R(__q('#bcrumb'))`,
    filter: `__R(__q('#filter'), __q('#folder-actions'))`,
    tiles: `__R(__all('#folder-vp .orow[data-kind=tile]'))`,
    pills: `__R(__all('#folder-vp .ipill'))`,
    view: `__R(__q('#view'))`,
  },
  tile: {
    modes: `__R(__q('#modes'))`,
    trail: `__R(__q('#crumb'))`,
    face: `__R(__q('#face'))`,
    stack: `__R(__q('#lens-tile .cap.olab'), __all('#lens-tile .orow[data-kind=entry]'))`,
    view: `__R(__q('#view'))`,
    cameras: `__R(__q('#views'), __q('#dispbtn'))`,
    status: `__R(__q('#statusbar'))`,
    target: `__R(__q('#target'), __q('#export'))`,
  },
  entry: {
    trail: `__R(__q('#crumb'))`,
    title: `__R(__q('#ctx'))`,
    controls: `__R(__all('#lens .row'))`,
    more: `__R(__all('#lens .chev'))`,
    selected: `__R(__q('#view'))`,
    actions: `__R(__text('#lens button', /^Hide$/), __text('#lens button', /^Delete$/))`,
  },
  control: {
    trail: `__R(__q('#crumb'))`,
    body: `__R(__q('#lens-control'))`,
  },
  source: {
    face: `__R(__q('#face'))`,
    code: `__R(__q('#code'))`,
    view: `__R(__q('#view'))`,
  },
  messages: {
    panel: `__R(__q('#msgpanel'))`,
    filters: `__R(__q('#msg-filters'))`,
    verdict: `__R(__q('#msg-body > :nth-child(1)'))`,
    note: `__R(__q('#msg-body > :nth-child(2)'))`,
    status: `__R(__q('#statusbar'))`,
  },
  target: {
    card: `__R(__q('#target-card'))`,
    chip: `__R(__q('#target'))`,
  },
  map: {
    trail: `__R(__q('#crumb'))`,
    summary: `__R(__q('#lens-map .pl-sec:nth-child(1)'))`,
    sort: `__R(__all('#lens-map .pl-sec:nth-child(2) .pl-field'))`,
    pieces: `__R(__q('#lens-map .mp-list'))`,
    variants: `__R(__q('#lens-map .pl-sec:nth-child(3)'))`,
    view: `__R(__q('#view'))`,
    chip: `__R(__q('#target'))`,
    modes: `__R(__q('#modes'))`,
  },
  plates: {
    trail: `__R(__q('#crumb'))`,
    summary: `__R(__q('#ctx'))`,
    plates: `__R(__text('#lens-plates .cap', /^Plates$/)?.parentElement)`,
    export: `__R(__text('#lens-plates button', /^Export/))`,
    after: `__R(__text('#lens-plates .cap', /^After the print$/)?.parentElement)`,
    pieces: `__R(__text('#lens-plates .cap', /^Pieces$/)?.parentElement)`,
    view: `__R(__q('#view'))`,
    chip: `__R(__q('#target'))`,
  },
  inventory: {
    trail: `__R(__q('#crumb'))`,
    face: `__R(__q('#lens-inventory .inv-face'))`,
    filters: `__R(__q('#lens-inventory .inv-filters'))`,
    totals: `__R(__q('#lens-inventory .inv-totals'))`,
    pieces: `__R(__all('#lens-inventory .inv-group'))`,
    actions: `__R(__q('#lens-inventory .inv-acts'))`,
  },
};

// ---- taking a picture -------------------------------------------------------------------------------------------------
fs.mkdirSync(outDir, { recursive: true });
const jsonPath = path.join(outDir, 'shots.json');
const book = fs.existsSync(jsonPath) ? JSON.parse(fs.readFileSync(jsonPath, 'utf8')) : { width: WIDTH, height: HEIGHT, shots: {} };
let theme = 'light';
const written = [];

async function shoot(name) {
  if (only && !only.includes(name)) return;
  await waitFor('the page to be idle', idle);
  const boot = await js(`(() => { const b = document.getElementById('boot-error'); return b && !b.hidden ? b.innerText : ''; })()`);
  if (boot) throw new Error(`${name}: the page shows its boot error:\n${boot}`);
  await js(`document.activeElement?.blur?.(); getSelection()?.removeAllRanges?.(); 1`);
  await sleep(1500); // the view eases to its camera and draws
  const anchors = {};
  for (const [key, expr] of Object.entries(ANCHORS[name])) {
    const box = await js(`(() => { ${HELPERS} return ${expr}; })()`);
    if (!box) throw new Error(`${name}: callout "${key}" has no part of the page: ${expr}\nthe editor changed; fix ANCHORS in xtask/shots/shots.mjs`);
    anchors[key] = box;
  }
  const shot = await send('Page.captureScreenshot', { format: 'webp', quality: 80, clip: { x: 0, y: 0, width: WIDTH, height: HEIGHT, scale: 1 } });
  const file = path.join(outDir, `${name}-${theme}.webp`);
  fs.writeFileSync(file, Buffer.from(shot.data, 'base64'));
  book.shots[name] = { w: WIDTH, h: HEIGHT, anchors };
  written.push(file);
  console.log(`shots: ${name}-${theme}  ${(fs.statSync(file).size / 1024).toFixed(0)} KB`);
}

// ---- the states, in the order the docs take them --------------------------------------------------------------------------
async function walk() {
  // folders: Places, My tiles, a folder of it
  await places();
  await openRow('My tiles', { what: 'My tiles', expr: `!!__row('crossing')` });
  await openRow('crossing', { what: 'the crossing folder', expr: `!!__row('pool')` });
  await shoot('folder');

  // a tile, its source, its messages, its build target
  await openRow('pool', docOpen('pool'));
  await shoot('tile');
  await press('Source', `__q('#face [data-face=source]')`);
  await waitFor('the source', `!!__q('#code') && __q('#code').getBoundingClientRect().width > 0`);
  await shoot('source');
  await press('Outline', `__q('#face [data-face=tile]')`);
  await waitFor('the outline', `!!__q('#lens-tile .orow[data-kind=entry]')`);
  await press('Messages', `__q('#msgbtn')`);
  await waitFor('the messages', `__q('#msgpanel')?.getBoundingClientRect().height > 100`);
  await shoot('messages');
  await press('close messages', `__q('#msg-close')`);
  await waitFor('the messages closed', `!(__q('#msgpanel')?.getBoundingClientRect().height > 0)`);
  await press('the build target', `__q('#target')`);
  await waitFor('the target card', `__q('#target-card')?.getBoundingClientRect().height > 50`);
  await shoot('target');
  await js(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); 1`);
  await waitFor('the target card closed', `!(__q('#target-card')?.getBoundingClientRect().height > 0)`);

  // an entry, and one of its controls
  await press('the grass entry', `__q('#lens-tile .orow[data-id="tile/grass/0"]')`);
  await waitFor('the grass sheet', `!!__text('#lens .nm', /^length$/)`);
  await shoot('entry');
  await press('the length control', `__q('#lens .chev[aria-label="More about length"]')`);
  await waitFor('the length level', `/length/.test(__q('#lens-control')?.innerText ?? '') && __q('#lens-control').getBoundingClientRect().width > 0`);
  await shoot('control');

  // a map
  await places();
  await openRow('My tiles', { what: 'My tiles', expr: `!!__row('crossing.dmap')` });
  await openRow('crossing.dmap', { what: 'the map', expr: `/\\d+ pieces/.test(__q('#sb-main')?.innerText ?? '') && ${idle}` });
  await shoot('map');

  // plates
  await places();
  await openRow('My tiles', { what: 'My tiles', expr: `!!__row('print-list.dmap')` });
  await openRow('print-list.dmap', docOpen('print-list.dmap'));
  if (await js(`!__q('#lens-plates')?.getBoundingClientRect().width`)) {
    await press('Plates', `__q('#modes [data-mode=plates]')`);
    await waitFor('the plates', `__q('#lens-plates')?.getBoundingClientRect().width > 0 && ${idle}`);
  }
  await shoot('plates');

  // the inventory
  await places();
  await openRow('My tiles', { what: 'My tiles', expr: `!!__row('inventory.dinv')` });
  await openRow('inventory.dinv', { what: 'the inventory', expr: `__q('#lens-inventory')?.getBoundingClientRect().width > 0` });
  await shoot('inventory');
}

try {
  for (theme of ['light', 'dark']) {
    await send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: theme }] });
    await send('Page.navigate', { url: editor });
    await sleep(1500);
    // a theme chosen by hand is remembered by the page: the run's own choice must be the system's
    await js(`try { for (const k of Object.keys(localStorage)) if (/theme/i.test(k)) localStorage.removeItem(k); } catch (e) { /* storage blocked: the system theme stands */ } 1`);
    await walk();
  }
  fs.writeFileSync(jsonPath, `${JSON.stringify(book, null, 1)}\n`);
  if (pageErrors.length) console.log(`shots: the page raised ${pageErrors.length} uncaught error(s), the first:\n${pageErrors[0]}`);
  console.log(`shots: ${written.length} pictures, ${(written.reduce((n, f) => n + fs.statSync(f).size, 0) / 1024).toFixed(0)} KB`);
} finally {
  ws.close();
}
process.exit(0);
