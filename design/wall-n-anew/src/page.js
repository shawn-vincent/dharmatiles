/* page.js: mounts the views of Wall N, anew, and exports the comparison stills for headless runs. */
(function () {
  'use strict';
  const CAM = window.CAM = {
    whole: { az: 35, el: 27, dist: 122, fov: 28, target: [17.5, 17.5, 13] },
    close: { az: 35, el: 27, dist: 78, fov: 28, target: [24, 21, 22] },
    seed: { az: 35, el: 27, dist: 122, fov: 28, target: [17.5, 17.5, 13] }
  };
  const FINALCFG = 'FINAL';
  function draw(el) {
    const c = el.querySelector('canvas'), d = el.dataset;
    const m = WV.modelFor(+d.seed || 1, d.cfg || FINALCFG, +d.stage || 7, { figure: d.figure !== '0' });
    const base = Object.assign({}, CAM[d.cam || 'whole']);
    if (el._az != null) { base.az = el._az; base.el = el._el; }
    WV.drawTo(c, m, base);
    const cap = el.querySelector('.stat'); if (cap) cap.textContent = m.stats.pieces + ' pieces, ' + m.tris + ' triangles';
  }
  function mount(el) {
    if (el._m) return; el._m = 1;
    try { draw(el); } catch (e) { el.querySelector('figcaption').textContent = 'failed: ' + e.message; console.error(e); return; }
    const c = el.querySelector('canvas'); let drag = null;
    const base = CAM[el.dataset.cam || 'whole']; el._az = base.az; el._el = base.el;
    c.addEventListener('pointerdown', e => { drag = { x: e.clientX, y: e.clientY, az: el._az, el: el._el }; try { c.setPointerCapture(e.pointerId); } catch (x) { /* best effort */ } });
    c.addEventListener('pointermove', e => { if (!drag) return; el._az = drag.az - (e.clientX - drag.x) * .45; el._el = Math.min(80, Math.max(5, drag.el + (e.clientY - drag.y) * .35)); requestAnimationFrame(() => draw(el)); });
    c.addEventListener('pointerup', () => { drag = null; }); c.addEventListener('dblclick', () => { el._az = base.az; el._el = base.el; draw(el); });
  }
  const els = [].slice.call(document.querySelectorAll('.v3d'));
  if (!window.THREE) els.forEach(e => { e.querySelector('figcaption').textContent = '3D needs three.js (blocked or offline)'; });
  else if (window.IntersectionObserver && !location.search.includes('eager')) {
    const io = new IntersectionObserver(es => es.forEach(en => { if (en.isIntersecting) { io.unobserve(en.target); setTimeout(() => mount(en.target), 0); } }), { rootMargin: '300px 0px' });
    els.forEach(e => io.observe(e));
  } else els.forEach(mount);
  /* stages: one at a time */
  const sb = [].slice.call(document.querySelectorAll('.stages button')), sv = document.getElementById('stagev');
  const names = window.STAGE_NAMES || [];
  sb.forEach(b => b.addEventListener('click', () => { sb.forEach(x => x.setAttribute('aria-pressed', String(x === b))); sv.dataset.stage = b.dataset.stage; sv._m = 0; mount(sv); document.getElementById('stagecap').textContent = names[+b.dataset.stage - 1] || ''; }));
  const cfgb = [].slice.call(document.querySelectorAll('.cfgsw button'));
  cfgb.forEach(b => b.addEventListener('click', () => { cfgb.forEach(x => x.setAttribute('aria-pressed', String(x === b))); sv.dataset.cfg = b.dataset.cfg; sv._m = 0; mount(sv); }));
  window.addEventListener('resize', () => els.forEach(e => { if (e._m) draw(e); }));
  /* headless: draw a still at a size, from a camera and config */
  window.still = function (o) {
    const c = document.createElement('canvas'); c.width = o.w; c.height = o.h;
    const m = WV.modelFor(o.seed || 1, o.cfg, o.stage || 7, {});
    WV.drawTo(c, m, Object.assign({}, CAM[o.cam || 'whole'], o.camOver || {}));
    return c.toDataURL('image/png').slice(22);
  };
})();
