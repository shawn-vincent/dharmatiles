// The build worker: the engine in its own thread, so a build never freezes the page.
// The page hands it the compiled module and the library; a build is one message in,
// one message out (the mesh's arrays are moved, not copied).
import init, * as engine from "./dt_web.js";

let ready = null;
// a Rust panic reaches the console just before the trap: keep it, so the error says what it was
let lastPanic = "";
const consoleError = console.error.bind(console);
console.error = (...args) => { lastPanic = args.join(" "); consoleError(...args); };

self.onmessage = async (ev) => {
  const m = ev.data;
  try {
    if (m.type === "init") {
      await init({ module_or_path: m.module });
      engine.set_library(m.library);
      ready = true;
      self.postMessage({ type: "ready" });
    } else if (m.type === "build") {
      if (!ready) throw new Error("the worker was asked to build before it was ready");
      const t0 = performance.now();
      const tile = engine.build_tile(m.text, m.system, m.seed, m.cells);
      const ms = performance.now() - t0;
      const meta = JSON.parse(tile.meta);
      const positions = tile.positions, colours = tile.colours;
      tile.free();
      self.postMessage({ type: "built", id: m.id, ms, meta, positions, colours }, [positions.buffer, colours.buffer]);
    }
  } catch (e) {
    // a trap or a throw is loud: the page shows it, never a blank view
    self.postMessage({ type: "error", id: m.id, message: (lastPanic ? lastPanic + "\n" : "") + String(e && e.message ? e.message : e), stack: e && e.stack ? String(e.stack) : "" });
  }
};
