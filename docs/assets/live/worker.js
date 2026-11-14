// The one worker every card on a page shares: the engine (docs/engine/dt_web*.js|wasm, `cargo xtask wasm`) in its own
// thread, so a build never freezes the page.  The page compiles the module once and hands it over (and hands it over again to a
// fresh worker when a runaway build is stopped); a job is one message in, and the answers come back as messages:
//
//   ready                          the module is up and has the library
//   progress {id, fraction, label} each stage of a build, from the program's own progress events
//   checked  {id, result}          the check: mistakes, notes and the map (dt_eval::map, the program's one producer)
//   built    {id, ...}             a mesh, painted, with the gate's findings (and the STL of a full build)
//   error    {id, message, stack}  a trap or a throw, loud: the page shows it
//
// A job is `{id, steps: ['check', 'build'], text, system, seed, cells}`.  `cells` is cells per square: a number for a draft, 0
// for the build's own finest, the detail the printer is given.
import init, * as engine from '../../engine/dt_web.js';

let ready = false;
// a Rust panic reaches the console just before the trap: keep it, so the error says what it was
let lastPanic = '';
const consoleError = console.error.bind(console);
console.error = (...args) => { lastPanic = args.join(' '); consoleError(...args); };

let current = null;
globalThis.dtProgress = (fraction, label, done, total) => {
  if (current != null) self.postMessage({ type: 'progress', id: current, fraction, label, done, total });
};

self.onmessage = async (ev) => {
  const m = ev.data;
  try {
    if (m.type === 'init') {
      await init({ module_or_path: m.module });
      engine.set_library(m.library);
      ready = true;
      self.postMessage({ type: 'ready' });
      return;
    }
    if (m.type !== 'job') throw new Error(`the worker was sent a "${m.type}" message it does not know`);
    if (!ready) throw new Error('the worker was given a job before it was ready');
    current = m.id;
    lastPanic = '';
    if (m.steps.includes('check')) {
      const t0 = performance.now();
      const result = JSON.parse(engine.check_tile(m.text, m.system));
      self.postMessage({ type: 'checked', id: m.id, result, ms: performance.now() - t0 });
      if (!result.ok || !m.steps.includes('build')) { self.postMessage({ type: 'idle', id: m.id }); current = null; return; }
    }
    if (m.steps.includes('build')) {
      const t0 = performance.now();
      const tile = engine.build_tile(m.text, m.system, m.seed, m.cells, m.tier);
      const ms = performance.now() - t0;
      const meta = JSON.parse(tile.meta);
      const positions = tile.positions, colours = tile.colours, stl = tile.stl;
      tile.free();
      self.postMessage({ type: 'built', id: m.id, ms, meta, positions, colours, stl }, [positions.buffer, colours.buffer, stl.buffer]);
    }
    current = null;
  } catch (e) {
    current = null;
    // a trap or a throw is loud: the page shows it, never a blank view
    self.postMessage({ type: 'error', id: m.id, message: (lastPanic ? `${lastPanic}\n` : '') + String(e && e.message ? e.message : e), stack: e && e.stack ? String(e.stack) : '' });
  }
};
