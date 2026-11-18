/* panel.js — owns the rock's controls, once: the six stage rows (a number that shows the stage's status, a switch, Solo, a strength slider, folded settings), All on and All off,
   the whole-stone rows, the build icon with its panel, and the small record panel a status opens. Built from Stone.controls.CONTROLS only; no stone logic lives here.
   The weathered-stone page and the wall page both call it: each holds its own params and its own records, and tells the panel when something changed.
     const pn = Stone.panel.create({ stages, allOn, allOff, whole, state, changed, ids })
       state: { P, off, solo }  (the panel reads and writes these; P may be replaced by the page, the panel reads it fresh)
       whole: element for the whole-stone rows (or null) and `wholeWords`: the words shown there
       changed(): called after any control moves
       onStatusClick(id, anchor): called when a stage's number is clicked with a status to show
     pn.sync() puts the state on the controls; pn.paint(id)/pn.setStat(id, s) show a stage's status; pn.openPanel(rec, anchor), pn.closePanel(). */
var Stone = Stone || {};
(function (S) {
  const C = S.controls, $ = (id) => document.getElementById(id);
  const fmt = (c, v) => (c.step >= 1 ? Math.round(v) : (+v).toFixed(c.step < 0.1 ? 2 : 1)) + (c.unit ? ' ' + c.unit : '');
  const infoBtn = (name, doc) => '<span class="info" tabindex="0" role="button" aria-label="about ' + name + '">i<span class="pop">' + doc + '</span></span>';
  const slider = (c) => '<label><span>' + c.label + ' ' + infoBtn(c.label, c.doc) + '</span><output id="o_' + c.key + '"></output><input type="range" id="r_' + c.key + '" min="' + c.min + '" max="' + c.max + '" step="' + c.step + '"></label>';
  const ICON = { ok: '<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M3 8.5l3.2 3.2L13 4.8" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>', warn: '<svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M8 2.2L14.6 13.6H1.4z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round"/><path d="M8 6.4v3.4M8 11.6v.4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>' };
  const NAME = {}; C.STAGES.forEach((s) => { NAME[s.id] = s.name; });

  function create(cfg) {
    const st = cfg.state, stat = {}, stagesEl = cfg.stages;
    const showVal = (c) => { const o = $('o_' + c.key); if (o) o.textContent = c.show ? c.show(st.P) : fmt(c, st.P[c.key]); };
    C.STAGES.forEach((s, i) => {
      const sr = C.strengthRow(s.id), rows = C.settingsRows(s.id), d = document.createElement('div'); d.className = 'stg'; d.id = 'st_' + s.id;
      d.innerHTML = '<div class="shead"><span class="num"><button type="button" class="cb" aria-expanded="false" aria-controls="set_' + s.id + '" aria-label="' + s.name + ' settings"><i class="car">▸</i></button><button type="button" class="nn" id="nn_' + s.id + '" aria-label="' + s.name + ', stage ' + (i + 1) + '"><span class="n">' + (i + 1) + '</span></button></span>' +
        '<span class="seg tri" id="tri_' + s.id + '" role="group" aria-label="' + s.name + ' switches"><button type="button" class="sw" data-m="toggle" role="switch" aria-checked="true" aria-label="' + s.name + '"><i></i></button><button type="button" data-m="solo" aria-pressed="false">Solo</button></span>' +
        '<span class="nm"><b>' + s.name + '</b> ' + infoBtn(s.name, s.doc) + '</span><output id="o_' + s.id + '"></output>' +
        '<input type="range" class="sl" id="r_' + s.id + '" min="' + sr.min + '" max="' + sr.max + '" step="' + sr.step + '" aria-label="' + s.name + ' strength"></div>' +
        (rows.some((c) => c.main) ? '<div class="sset mset" id="main_' + s.id + '">' + rows.filter((c) => c.main).map(slider).join('') + '</div>' : '') + '<div class="sset" id="set_' + s.id + '" hidden>' + rows.filter((c) => !c.main).map(slider).join('') + '</div>';
      stagesEl.appendChild(d);
      const nb = d.querySelector('button.cb');
      nb.addEventListener('click', () => { const open = nb.getAttribute('aria-expanded') !== 'true'; nb.setAttribute('aria-expanded', String(open)); $('set_' + s.id).hidden = !open; d.classList.toggle('open', open); nb.querySelector('.car').textContent = open ? '▾' : '▸'; });
      $('nn_' + s.id).addEventListener('click', (ev) => { ev.stopPropagation(); if (stat[s.id] && stat[s.id].mode !== 'run') cfg.onStatusClick(s.id, $('nn_' + s.id)); else nb.click(); });   // a status opens its record; a plain number opens the settings
      $('r_' + s.id).addEventListener('input', (e) => { st.P[s.id] = +e.target.value; if (cfg.touched) cfg.touched(); showVal(sr); changed(); });
      $('tri_' + s.id).addEventListener('click', (ev) => { const b = ev.target.closest('button'); if (!b) return; if (b.dataset.m === 'solo') { st.solo = st.solo === s.id ? '' : s.id; if (st.solo) st.off[s.id] = false; } else { st.off[s.id] = !st.off[s.id]; if (st.off[s.id] && st.solo === s.id) st.solo = ''; } changed(); });
      rows.forEach((c) => $('r_' + c.key).addEventListener('input', (e) => { st.P[c.key] = +e.target.value; if (cfg.touched) cfg.touched(); $('o_' + c.key).textContent = fmt(c, st.P[c.key]); changed(); }));
    });
    const wholeRows = (cfg.wholeWords || []).map((w) => C.BY_KEY[w]);
    if (cfg.whole) { cfg.whole.innerHTML = wholeRows.map(slider).join(''); wholeRows.forEach((c) => $('r_' + c.key).addEventListener('input', (e) => { st.P[c.key] = +e.target.value; if (cfg.touched) cfg.touched(); $('o_' + c.key).textContent = fmt(c, st.P[c.key]); changed(); })); }
    cfg.allOn.addEventListener('click', () => { st.off = {}; st.solo = ''; changed(); });
    cfg.allOff.addEventListener('click', () => { C.STAGES.forEach((s) => { st.off[s.id] = true; }); st.solo = ''; changed(); });
    // whether All on / All off are lit is derived from the switches, never set by a click
    function syncSwitches() {
      const allOn = !st.solo && C.STAGES.every((s) => !st.off[s.id]), allOff = C.STAGES.every((s) => st.off[s.id]);
      [[cfg.allOn, allOn], [cfg.allOff, allOff]].forEach(([el, on]) => { el.classList.toggle('on', on); el.setAttribute('aria-pressed', String(on)); });
      C.STAGES.forEach((s) => {
        const e = $('st_' + s.id), mode = st.solo === s.id ? 'solo' : st.off[s.id] ? 'off' : 'on', tb = $('tri_' + s.id).querySelector('[data-m=toggle]'), sb = $('tri_' + s.id).querySelector('[data-m=solo]');
        e.classList.toggle('solo', mode === 'solo'); e.classList.toggle('isoff', mode === 'off'); e.classList.toggle('dim', !!st.solo && st.solo !== s.id);
        tb.classList.toggle('on', !st.off[s.id]); tb.setAttribute('aria-checked', String(!st.off[s.id])); sb.classList.toggle('on', st.solo === s.id); sb.setAttribute('aria-pressed', String(st.solo === s.id));
      });
    }
    function syncControls() {
      C.CONTROLS.forEach((c) => { const el = $('r_' + c.key); if (el) el.value = st.P[c.key]; const o = $('o_' + c.key); if (o) o.textContent = c.strength ? (c.show ? c.show(st.P) : fmt(c, st.P[c.key])) : fmt(c, st.P[c.key]); });
    }
    function changed() { syncSwitches(); C.STAGES.forEach((s) => showVal(C.strengthRow(s.id))); cfg.changed(); }
    function paint(id) {
      const b = $('nn_' + id), s = stat[id], n = C.STAGES.findIndex((x) => x.id === id) + 1; if (!b) return;
      b.className = 'nn' + (s ? ' ' + s.mode : ''); b.removeAttribute('title');
      if (!s || s.mode === 'run') { b.innerHTML = '<span class="n">' + n + '</span>'; b.setAttribute('aria-label', NAME[id] + ', stage ' + n + (s ? ', computing' : '')); return; }
      b.innerHTML = ICON[s.mode === 'warn' ? 'warn' : 'ok']; b.title = s.text; b.setAttribute('aria-label', NAME[id] + ': ' + s.text + '. Click for details');
    }
    const setStat = (id, s) => { stat[id] = s; paint(id); };
    const clearStages = () => C.STAGES.forEach((s) => setStat(s.id, null));
    /* a small panel by the icon that was clicked: the details behind a status. Closes on Escape or a click elsewhere. */
    let panelEl = null;
    function closePanel() { if (panelEl) { panelEl.remove(); panelEl = null; } }
    function openPanel(rec, anchor) {
      const was = panelEl && panelEl.dataset.k === rec.key; closePanel(); if (was) return;
      const p = document.createElement('div'); p.className = 'bpanel'; p.dataset.k = rec.key; p.setAttribute('role', 'dialog'); p.setAttribute('aria-label', rec.title);
      p.innerHTML = '<button type="button" class="bx" aria-label="Close">×</button><h4><span class="st ' + rec.mode + '">' + ICON[rec.mode === 'warn' ? 'warn' : 'ok'] + '</span>' + rec.title + '</h4>' + rec.html;
      document.body.appendChild(p); panelEl = p; p.addEventListener('click', (ev) => ev.stopPropagation()); p.querySelector('.bx').addEventListener('click', closePanel);
      const r = anchor.getBoundingClientRect(), w = Math.min(p.offsetWidth, innerWidth - 16); let left = r.left + scrollX; if (left + w > scrollX + innerWidth - 8) left = scrollX + innerWidth - w - 8; if (left < scrollX + 8) left = scrollX + 8;
      p.style.left = left + 'px'; p.style.top = (r.bottom + scrollY + 6) + 'px'; p.querySelector('.bx').focus();
    }
    document.addEventListener('click', closePanel); document.addEventListener('keydown', (ev) => { if (ev.key === 'Escape') closePanel(); });
    document.addEventListener('click', () => document.querySelectorAll('.info.open').forEach((x) => x.classList.remove('open')));
    stagesEl.closest('.sp, body').querySelectorAll('.info').forEach((i) => i.addEventListener('click', (ev) => { ev.preventDefault(); ev.stopPropagation(); const was = i.classList.contains('open'); document.querySelectorAll('.info.open').forEach((x) => x.classList.remove('open')); if (!was) i.classList.add('open'); }));
    const sync = () => { syncControls(); syncSwitches(); C.STAGES.forEach((s) => showVal(C.strengthRow(s.id))); };
    const showVals = () => C.STAGES.forEach((s) => showVal(C.strengthRow(s.id)));
    return { sync, syncSwitches, showVals, paint, setStat, clearStages, openPanel, closePanel, get stat() { return stat; }, ICON, NAME, panelOpen: () => panelEl };
  }
  S.panel = { create, ICON, fmt };
})(Stone);
