/* ---------- the page: tables from the study, the plates, the explorer ---------- */
const $ = (s, r = document) => r.querySelector(s);
const el = (tag, attrs = {}, ...kids) => { const e = document.createElement(tag); for (const k in attrs) { if (k === 'html') e.innerHTML = attrs[k]; else if (k === 'text') e.textContent = attrs[k]; else if (k.startsWith('on')) e.addEventListener(k.slice(2), attrs[k]); else if (attrs[k] != null) e.setAttribute(k, attrs[k]); } for (const c of kids) if (c) e.append(c); return e; };
const q = (arr, p) => { const a = arr.slice().sort((x, y) => x - y), i = (a.length - 1) * p, lo = Math.floor(i), hi = Math.ceil(i); return a[lo] + (a[hi] - a[lo]) * (i - lo); };
const f2 = (v) => (+v).toFixed(2), f1 = (v) => (+v).toFixed(1);
const CLASSES = ['slab', 'block', 'pillar', 'ramp'], KINDS6 = ['split', 'step', 'chip', 'facet', 'bed', 'offset'];
const table = (head, rows, cls) => `<table${cls ? ` class="${cls}"` : ''}><thead><tr>${head.map((x, i) => `<th${i ? ' class="num"' : ''}>${x}</th>`).join('')}</tr></thead><tbody>${rows.map((r) => `<tr>${r.map((x, i) => `<td${i ? ' class="num"' : ''}>${x}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
const hasF = (r, k) => r.features.some((f) => f.kind === k);
const count = (r, k) => r.features.filter((f) => f.kind === k).length;

function tables() {
  const R = ROCKS, N = R.length, by = (c) => R.filter((r) => r.cls === c);
  /* base shapes */
  const prop = { slab: 'H 0.3–0.4 of W', block: 'H 0.6–1 of W', pillar: 'H 1.7–2.5 of W', ramp: 'back 0.6–0.95 of W, front 0.25–0.45 of that' };
  $('#t-base').innerHTML = table(['Base shape', 'Rocks', 'Share', 'Silhouette h/w (min · median · max)', 'Steps', 'Splits', 'Features a rock', 'Proportions I use'],
    CLASSES.map((c) => { const rs = by(c), a = rs.map((r) => r.view.aspect); return [c, rs.length, f2(rs.length / N), `${f2(Math.min(...a))} · ${f2(q(a, .5))} · ${f2(Math.max(...a))}`, rs.filter((r) => hasF(r, 'step')).length, rs.filter((r) => hasF(r, 'split')).length, f1(rs.reduce((s, r) => s + r.features.length, 0) / rs.length), prop[c]]; })
      .concat([['all', N, '1.00', `${f2(Math.min(...R.map((r) => r.view.aspect)))} · ${f2(q(R.map((r) => r.view.aspect), .5))} · ${f2(Math.max(...R.map((r) => r.view.aspect)))}`, R.filter((r) => hasF(r, 'step')).length, R.filter((r) => hasF(r, 'split')).length, f1(R.reduce((s, r) => s + r.features.length, 0) / N), '']]));
  /* size ranges of features */
  const unit = { split: 'of the height, from the top (11 of 32 full)', step: 'of the face width', chip: 'of the face width', facet: 'of the face width', bed: 'of the face width', offset: 'of the height' };
  const rowsS = KINDS6.map((k) => { const fs = R.flatMap((r) => r.features.filter((f) => f.kind === k)), e = fs.filter((f) => f.ext !== 'full').map((f) => f.ext); return [k, fs.length, e.length ? `${f2(Math.min(...e))} · ${f2(q(e, .5))} · ${f2(Math.max(...e))}` : '–', unit[k]]; });
  $('#t-size').innerHTML = table(['Feature', 'Seen', 'Size (min · median · max)', 'Size is a share'], rowsS);
  /* features by class */
  $('#t-feat').innerHTML = table(['Feature', 'Rocks', 'Features', 'a rock', ...CLASSES.map((c) => c + ' (' + by(c).length + ')')],
    KINDS6.map((k) => { const rs = R.filter((r) => hasF(r, k)), n = R.reduce((s, r) => s + count(r, k), 0); return [k, rs.length, n, f2(n / N), ...CLASSES.map((c) => by(c).filter((r) => hasF(r, k)).length)]; }));
  /* where */
  const where = KINDS6.map((k) => { const fs = R.flatMap((r) => r.features.filter((f) => f.kind === k)), z = fs.map((f) => f.z), n = (cond) => fs.filter(cond).length; return [k, n((f) => f.face === 'L'), n((f) => f.face === 'R'), n((f) => f.face === 'T'), k === 'split' ? '–' : n((f) => f.z < .35), k === 'split' ? '–' : n((f) => f.z >= .35 && f.z <= .65), k === 'split' ? '–' : n((f) => f.z > .65), n((f) => f.ext === 'full'), k === 'split' ? '–' : f2(q(z, .5))]; });
  $('#t-where').innerHTML = table(['Feature', 'Left face', 'Right face', 'Top', 'Low <.35', 'Mid', 'High >.65', 'Full height', 'Median z'], where);
  /* co-occurrence */
  const kk = KINDS6.slice(0, 5), mx = kk.map((a) => kk.map((b) => R.filter((r) => hasF(r, a) && hasF(r, b)).length));
  $('#t-co').innerHTML = `<table class="heat"><thead><tr><th></th>${kk.map((k) => `<th class="num">${k}</th>`).join('')}</tr></thead><tbody>${kk.map((a, i) => `<tr><th style="text-transform:none;letter-spacing:0;font-size:13px;color:var(--fg)">${a}</th>${kk.map((b, j) => `<td class="h" style="background:color-mix(in srgb,var(--accent) ${Math.round(mx[i][j] / (mx[i][i] || 1) * (i === j ? 60 : 85))}%,transparent)">${mx[i][j]}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
  /* chamfers */
  const fc = (c) => by(c).reduce((a, r) => { a[r.facets]++; return a; }, [0, 0, 0, 0]);
  $('#t-chamfer').innerHTML = table(['', 'Rocks with 1 plane', '2 planes', '3 planes', 'Widest over narrowest (min · median · max)'],
    CLASSES.map((c) => { const f = fc(c), w = by(c).map((r) => r.widthRatio); return [c, f[1], f[2], f[3], `${f1(Math.min(...w))} · ${f1(q(w, .5))} · ${f1(Math.max(...w))}`]; })
      .concat([['all', ...(() => { const f = [1, 2, 3].map((n) => R.filter((r) => r.facets === n).length), w = R.map((r) => r.widthRatio); return [...f, `${f1(Math.min(...w))} · ${f1(q(w, .5))} · ${f1(Math.max(...w))}`]; })()]]));
}

/* ---------- plates: reference cells enlarged, features marked ---------- */
const PLATES = [
  ['R1C4', [[.62, .55, 'sp', 'split, full'], [.2, .62, 'ch', 'chip, low']]],
  ['R1C7', [[.36, .68, 'st', 'step: shelf'], [.6, .35, 'fc', 'top in 2 planes']]],
  ['R1C8', [[.38, .62, 'sp', 'split'], [.75, .62, 'sp', 'split'], [.66, .26, 'bd', 'line across top']]],
  ['R2C4', [[.28, .6, 'st', 'step, lower left'], [.62, .2, 'fc', 'chamfer in planes']]],
  ['R3C3', [[.58, .46, 'sp', 'split, full'], [.2, .52, 'st', 'low step']]],
  ['R3C1', [[.5, .66, 'ch', 'V chip'], [.5, .22, 'bd', 'line near top']]],
  ['R1C6', [[.44, .62, 'fc', 'oblique facet'], [.76, .5, 'sp', 'split']]],
  ['R5C3', [[.3, .62, 'st', 'seat, shelf'], [.62, .3, 'fc', 'slope in planes']]],
];
function plates(img) {
  const host = $('#plates');
  for (const [id, marks] of PLATES) {
    const r = ROCKS.find((x) => x.id === id), [x0, y0, x1, y1] = r.cell, w = x1 - x0, h = y1 - y0, S = 3;
    const cv = el('canvas', { width: w * S, height: h * S }), c = cv.getContext('2d'); c.imageSmoothingQuality = 'high'; c.drawImage(img, x0, y0, w, h, 0, 0, w * S, h * S);
    const cell = el('div', { class: 'cell', style: `aspect-ratio:${w}/${h}` }, cv, el('span', { class: 'tag', text: id }));
    cell.style.display = 'block'; cv.style.cssText = 'position:absolute;inset:0;width:100%;height:100%';
    for (const [x, y, kd, t] of marks) { const m = el('span', { class: 'mark ' + kd, text: t }); m.style.left = x * 100 + '%'; m.style.top = y * 100 + '%'; cell.append(m); }
    const fs = r.features.map((f) => f.kind).join(', ');
    host.append(el('div', {}, cell, el('div', { class: 'note', style: 'margin-top:5px;border:0;padding:0', html: `<b>${r.cls}</b>${r.lean ? ', leaning' : ''}: ${fs}. ${r.features.map((f) => f.note).slice(0, 2).join('; ')}.` })));
  }
}

/* ---------- the explorer ---------- */
let seed = 20, kind = 'any', rock = null, scene, cam, ctl, GLR, shot = 0;
const live = () => $('#live');
function dirOf() { const d = cam.position.clone().sub(ctl.target); return d.length() ? d.normalize() : new THREE.Vector3(.65, .35, .67); }
function frame(first) {
  const d = rock.dimsOut, size = Math.max(d[0], d[1], d[2]), cy = d[2] * .48, dir = first ? dirOf() : dirOf();
  ctl.target.set(0, cy, 0); cam.position.copy(dir.multiplyScalar(size * 6.2)).add(ctl.target); ctl.minDistance = size * 2.2; ctl.maxDistance = size * 14; ctl.update();
}
function draw() { if (!GLR || !scene) return; GLR.render(scene, cam); }
function show() {
  rock = ROCK.makeRock(seed, { kind });
  scene = ROCKVIEW.buildScene(rock); frame(); draw();
  const d = rock.dimsOut, c = rock.check, fe = rock.feats.map((f) => f.k).reduce((a, k) => (a[k] = (a[k] || 0) + 1, a), {});
  $('#read').innerHTML = `<dt>Base</dt><dd>${rock.kind}${rock.forceStep ? ' (stepped)' : ''}</dd><dt>Size</dt><dd>${f2(d[0])} × ${f2(d[1])} × ${f2(d[2])} fh (x, y, up)</dd><dt>Cutters</dt><dd>${rock.cutters.length}</dd><dt>Features</dt><dd>${Object.entries(fe).map(([k, n]) => k + (n > 1 ? ' ×' + n : '')).join(', ') || 'none'}</dd><dt>Triangles</dt><dd>${rock.T.length / 3}</dd><dt>Built in</dt><dd>${f1(rock.ms)} ms</dd><dt>Closed</dt><dd>${c.open === 0 && c.many === 0 ? 'yes: ' + c.edges + ' edges, each in two triangles' : 'NO: ' + c.open + ' open, ' + c.many + ' shared by more'}</dd><dt>Steepest underside</dt><dd>${f1(c.overhangDeg)}° from vertical (limit 55°)${rock.attempt > 1 ? '; seed redrawn ' + (rock.attempt - 1) + '×' : ''}</dd>`;
  $('#seed').value = seed; try { history.replaceState(null, '', '#s=' + seed + '&k=' + kind); } catch (e) { /* the preview may not allow it: harmless */ }
}
function go(s) { seed = Math.max(1, Math.min(999999, Math.round(s))); show(); }
function startExplorer(img) {
  const box = live(); GLR = new THREE.WebGLRenderer({ antialias: true }); GLR.setPixelRatio(Math.min(2, window.devicePixelRatio || 1)); box.append(GLR.domElement);
  cam = new THREE.PerspectiveCamera(15, 4 / 3, .05, 100); cam.position.set(.65, .35, .67).multiplyScalar(2);
  ctl = new THREE.OrbitControls(cam, GLR.domElement); ctl.enablePan = false; ctl.enableDamping = false; ctl.addEventListener('change', draw);
  const fit = () => { const w = box.clientWidth, h = box.clientHeight; if (!w) return; GLR.setSize(w, h, false); cam.aspect = w / h; cam.updateProjectionMatrix(); draw(); };
  new ResizeObserver(fit).observe(box);
  const m = /s=(\d+)(?:&k=(\w+))?/.exec(location.hash); if (m) { seed = +m[1]; if (m[2] && $('#kind [value=' + m[2] + ']')) { kind = m[2]; $('#kind').value = kind; } } else seed = 20;
  $('#prev').onclick = () => go(seed - 1); $('#next').onclick = () => go(seed + 1); $('#new').onclick = () => go(1 + Math.floor(Math.random() * 99999));
  $('#view').onclick = () => { ctl.target.set(0, 0, 0); cam.position.set(.65, .35, .67); frame(); };
  $('#seed').onchange = (e) => go(+e.target.value || 1); $('#kind').onchange = (e) => { kind = e.target.value; show(); };
  fit(); show();
  const sel = $('#refcell'); for (const r of ROCKS.slice().sort((x, y) => x.id.localeCompare(y.id))) sel.append(el('option', { value: r.id, text: r.id + ' · ' + r.cls }));
  const showRef = () => { const r = ROCKS.find((x) => x.id === sel.value), [x0, y0, x1, y1] = r.cell, w = x1 - x0, h = y1 - y0, cv = $('#refcrop'); cv.width = w * 3; cv.height = h * 3; const c = cv.getContext('2d'); c.imageSmoothingQuality = 'high'; c.drawImage(img, x0, y0, w, h, 0, 0, cv.width, cv.height); };
  sel.value = 'R2C2'; sel.onchange = showRef; showRef();
}

/* ---------- how a rock is built: the stages of one seed, and the timing ---------- */
function stages() {
  let s0 = 1, best = null;
  for (; s0 < 800; s0++) { const r = ROCK.makeRock(s0, { kind: 'any' }), k = new Set(r.feats.map((f) => f.k)), nsp = r.feats.filter((f) => f.k === 'split').length; if (k.has('step') && nsp && k.has('chip') && k.has('facet') && r.kind !== 'pillar') { best = r; break; } }
  if (!best) { s0 = 20; best = ROCK.makeRock(20, { kind: 'any' }); }
  const host = $('#stages'), o = { kind: 'any', attempt: best.attempt };
  const items = [['1 · the mass', ROCK.makeRock(s0, { ...o, stage: 0 }), 'One solid: sides, top, flat bottom.'], ['2 · chamfers', ROCK.makeRock(s0, { ...o, stage: 1 }), 'Every edge cut by several planes of changing width.'], ['3 · facets, chips', ROCK.makeRock(s0, { ...o, stage: 2 }), 'A few more planes on the same solid.'], ['4 · cuts', best, 'Grooves and a bite taken out: ' + best.T.length / 3 + ' triangles, one closed mesh (seed ' + s0 + ').']];
  for (const [t, r, sub] of items) { const cv = ROCKVIEW.still(r, 330, 300); host.append(el('div', {}, el('div', { class: 'cell' }, cv, el('span', { class: 'tag', text: t })), el('div', { class: 'note', style: 'margin-top:5px;border:0;padding:0', text: sub }))); }
}
function timing() {
  const rows = [], kinds = ['slab', 'block', 'pillar', 'ramp', 'stepped'];
  const all = [];
  for (const k of kinds) {
    const ms = [], tri = []; let again = 0, bad = 0, over = 0; const N = 120;
    for (let i = 0; i < N; i++) { const r = ROCK.makeRock(5000 + i, { kind: k }); ms.push(r.ms); tri.push(r.T.length / 3); if (r.attempt > 1) again++; if (r.check.open || r.check.many) bad++; over = Math.max(over, r.check.overhangDeg); }
    all.push(...ms); rows.push([k, f1(ms.reduce((a, b) => a + b) / N), f1(q(ms, .95)), f1(Math.max(...ms)), Math.round(tri.reduce((a, b) => a + b) / N), Math.round(again / N * 100) + '%', bad, f1(over)]);
  }
  $('#t-time').innerHTML = table(['Base shape', 'mean ms', 'p95 ms', 'max ms', 'triangles', 'redrawn', 'open', 'worst underside °'], rows);
  $('#timenote').textContent = 'Measured just now in this browser: 120 seeds a row, a rock built, joined, welded and checked each time; the picture is not in it. "Redrawn" is the share of seeds that needed a second draw because the first had a face over 55 degrees or an open edge; the seed still gives the same rock every time.';
}

window.addEventListener('DOMContentLoaded', () => {
  tables();
  const img = new Image(); img.onload = () => {
    plates(img); startExplorer(img);
    setTimeout(() => { stages(); timing(); }, 60);
  }; img.src = $('#sheetfig img').src;
});
