/* page.js: draws Wall O's models. Lighting is our own: grey, a hemisphere fill, a soft key, cavity shading baked per vertex (engine.js). */
(function () {
  'use strict';
  const T = THREE, W = window.WallO, E = window.Eng;
  const Q = new URLSearchParams(location.search);
  const VER = Q.get('v') || 'cur';
  const OV = window.OVER || {}, LIT = Q.get('literal') === '1';
  let renderer = null;
  function getR() { if (!renderer) { renderer = new T.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true }); renderer.setClearColor(0, 0); } return renderer; }
  const CAM = {   /* the panel's camera */
    whole: { az: -22, el: 40, dist: 156, fov: 20, tgt: [19, 15, 14] },
    close: { az: -22, el: 36, dist: 80, fov: 20, tgt: [24, 22, 22] },
    seed: { az: -22, el: 40, dist: 156, fov: 20, tgt: [19, 15, 14] },
    stage: { az: -22, el: 40, dist: 156, fov: 20, tgt: [19, 15, 14] }
  };
  function makeModel(seed, stage) {
    const o = W.build(seed, stage, Object.assign({}, OV, LIT ? { literal: true } : {})), parts = o.parts.slice();
    if (stage >= 7) W.figure(parts, o.fig);
    const m = E.solidsMesh(parts, { ground: [0, 0, 35, 35], pitch: 2.2, aoK: 1.6, seed: seed, facetTone: .03, maxd: 7, grey: .8 });
    m.info = o.info; return m;
  }
  function paint(canvas, model, camName, extra) {
    const cm = Object.assign({}, CAM[camName], extra || {}), r = getR(), w = canvas.clientWidth || canvas.width, h = canvas.clientHeight || canvas.height, dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr);
    const ctx = canvas.getContext('2d'), scene = new T.Scene(), cam = new T.PerspectiveCamera(cm.fov, w / h, 1, 1000);
    cam.up.set(0, 0, 1);
    model.meshes.forEach(m => scene.add(m));
    scene.add(new T.HemisphereLight(0xf1f3f8, 0x7a7c84, .62));
    const key = new T.DirectionalLight(0xffffff, .5); scene.add(key); scene.add(key.target);
    const a = cm.az * Math.PI / 180, e = cm.el * Math.PI / 180, t = cm.tgt, d = cm.dist;
    cam.position.set(t[0] + d * Math.cos(e) * Math.sin(a), t[1] - d * Math.cos(e) * Math.cos(a), t[2] + d * Math.sin(e)); cam.lookAt(t[0], t[1], t[2]);
    const ka = a - 55 * Math.PI / 180, ke = 42 * Math.PI / 180; key.position.set(t[0] + 100 * Math.cos(ke) * Math.sin(ka), t[1] - 100 * Math.cos(ke) * Math.cos(ka), t[2] + 100 * Math.sin(ke)); key.target.position.set(t[0], t[1], t[2]);
    r.setPixelRatio(1); r.setSize(canvas.width, canvas.height, false); r.render(scene, cam);
    const gr = ctx.createRadialGradient(canvas.width * .5, canvas.height * .45, 10, canvas.width * .5, canvas.height * .5, canvas.width * .8); gr.addColorStop(0, '#7a7c82'); gr.addColorStop(1, '#4a4c52');
    ctx.fillStyle = gr; ctx.fillRect(0, 0, canvas.width, canvas.height); ctx.drawImage(r.domElement, 0, 0);
    model.meshes.forEach(m => scene.remove(m));
    return cam;
  }
  /* orbit on drag; the model is kept so a redraw is cheap */
  function live(canvas, model, camName, extra) {
    const cm = Object.assign({}, CAM[camName], extra || {}), st = { az: cm.az, el: cm.el }; let drag = null, raf = 0;
    const go = () => { raf = 0; paint(canvas, model, camName, Object.assign({}, extra || {}, st)); };
    canvas.addEventListener('pointerdown', e => { drag = { x: e.clientX, y: e.clientY, az: st.az, el: st.el }; try { canvas.setPointerCapture(e.pointerId); } catch (x) { /* best effort */ } });
    canvas.addEventListener('pointermove', e => { if (!drag) return; st.az = drag.az - (e.clientX - drag.x) * .45; st.el = Math.max(6, Math.min(86, drag.el + (e.clientY - drag.y) * .35)); if (!raf) raf = requestAnimationFrame(go); });
    const up = () => { drag = null; }; canvas.addEventListener('pointerup', up); canvas.addEventListener('pointercancel', up);
    canvas.addEventListener('dblclick', () => { st.az = cm.az; st.el = cm.el; go(); });
  }
  const cache = {};
  function get(seed, stage) { const k = seed + ':' + stage; return cache[k] || (cache[k] = makeModel(seed, stage)); }
  function paintAll() {
    const jobs = [];
    document.querySelectorAll(Q.get('comp') ? '#comp canvas[data-seed]' : 'canvas[data-seed]').forEach(c => jobs.push(c));
    let i = 0;
    (function step() {
      if (i >= jobs.length) { document.documentElement.setAttribute('data-ready', '1'); return; }
      const c = jobs[i++], seed = +c.dataset.seed, stage = +(c.dataset.stage || 7), cam = c.dataset.cam || 'whole';
      try { const m = get(seed, stage); paint(c, m, cam); if (c.dataset.live) live(c, m, cam); const n = c.parentElement.querySelector('.tri'); if (n) n.textContent = m.tris + ' tri'; } catch (e) { console.error('model failed', e.stack || e); c.parentElement.setAttribute('data-failed', e.message); }
      setTimeout(step, 10);
    })();
  }
  window.WallPage = { paintAll, get, paint, CAM };
  window.addEventListener('load', paintAll);
})();
