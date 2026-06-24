# Browser-Based Python App Viability

*Design exploration — 2026-06-24*

## The Core Question

Can dharmatiles (or a future project like it) be delivered as a web page that runs Python locally in the user's browser — no server, no install, just visit a URL?

Short answer: **partially viable for dharmatiles today; fully viable for a new project designed for it.** The limiting factor is `manifold3d` and, to a lesser extent, `trimesh`.

---

## Technology Landscape

### Pyodide

CPython compiled to WebAssembly. Ships a curated package set (~250+ packages) that includes:

- `numpy` ✓
- `scipy` ✓
- `Pillow` ✓
- `pyyaml` ✓ (pure Python, loads fine)
- `rich` ✓ (pure Python, terminal output would be suppressed in-browser anyway)

Pyodide is the most mature approach and the one to default to. PyScript is a higher-level wrapper around Pyodide; it doesn't add capability, just ergonomics.

### WebAssembly Constraints

The browser sandbox limits what Python can do:

- No OS-level filesystem. Files live in an in-memory virtual FS (Emscripten). The File System Access API (Chrome only) lets users grant folder access that gets mounted into that virtual FS.
- No threads by default. `SharedArrayBuffer` (needed for real threads) requires COOP/COEP HTTP headers. Pyodide supports it if the server sets those headers, but most static hosts don't.
- No subprocess. Can't call `openscad`, `prusa-slicer`, or any external binary.
- Memory cap. Browser tabs get ~2–4 GB in practice. Large STL operations are fine; huge terrain grids might hit limits.

---

## Dharmatiles Dependency Audit

| Package | Pyodide | Notes |
|---|---|---|
| `numpy` | ✓ | Core. Works fully. |
| `scipy` | ✓ | Voronoi, spatial trees — all fine. |
| `Pillow` | ✓ | Not heavily used; no issue. |
| `pyyaml` | ✓ | No issue. |
| `rich` | ✓ | Terminal colors will be no-ops; that's fine. |
| `trimesh` | ✗ | **Not in Pyodide's package list.** Uses optional C extensions (rtree, pyembree) but the base install is mostly pure Python + numpy. May be loadable via micropip if a pure-Python-compatible wheel exists, or a vendored subset could be written. |
| `manifold3d` | ✗ | **Hard blocker for Python.** Native C++ extension, no Pyodide build. Used for OpenLOCK base CSG and the logo inset. However, the same library ships as `manifold-3d` on npm (a WASM build) — available to JavaScript/TypeScript with full capability. |

### What Works Without Those Two

The entire core generation pipeline is pure Python + numpy + scipy:

- Heightmap construction (`core/mesh.py`, `core/region.py`, `core/grid.py`)
- Soil, grass carpet, water texture layers (`layers/`)
- 3D grass field simulation (`grass/`)
- Rock scatter (`scatter/`)
- Tree skeleton + mesh (`trees/`)
- DungeonBlocks base (`bases/dungeonblocks.py` — no manifold3d)

The heightmap solid (`make_heightmap_solid`) produces a plain numpy mesh, which is then assembled via trimesh and exported. If trimesh is unavailable, you'd need a fallback STL writer — but writing binary STL from numpy arrays is trivial (~20 lines).

**What breaks without manifold3d:**
- OpenLOCK base (T-slot CSG)
- Logo inset into the base

**What breaks without trimesh:**
- Mesh boolean operations (if used)
- The trimesh.Trimesh assembly and STL export

The trimesh dependency is softer: STL export is just packing numpy float32/int32 arrays into a 84-byte header + face records. A minimal in-browser STL writer could replace it.

---

## Architecture Options

### Option A: Pure Pyodide (Browser-only)

Load Pyodide, import numpy/scipy, run a stripped dharmatiles that skips manifold3d paths (OpenLOCK output only, no logo). Write a ~30-line STL writer to replace trimesh export.

**What you get:** DungeonBlocks STL generation entirely in the browser, no server, works offline after first load.

**What you lose:** OpenLOCK output, logo inset.

**Effort:** Medium. The codebase would need to be importable without manifold3d/trimesh. Today it imports them at module load time in a few places. Wrapping those in `try/except ImportError` and providing a minimal STL writer would mostly do it.

**Package load time:** Pyodide itself + numpy + scipy is ~20–40 MB. First load is slow (~5–15 seconds on good broadband). Subsequent loads are cached.

### Option B: Pure Pyodide + Micropip trimesh

`trimesh` is available on PyPI as a pure-Python-compatible package (the C extensions are optional). `micropip` (Pyodide's pip) can install pure-Python wheels at runtime.

```python
import micropip
await micropip.install("trimesh")
```

If trimesh's PyPI wheel has no compiled dependencies (its core does not — it's numpy/scipy), this likely works. Worth testing directly.

**Upside:** Closer to current codebase, less adaptation needed.
**Downside:** trimesh wheel is ~15 MB; adds to load time.

manifold3d remains blocked regardless — there is no WASM build.

### Option C: Browser UI + Local Helper

A small installed Python service (`dharmatiles-server`) exposes a `localhost:PORT` HTTP API. The web UI is a static page that sends generation requests to it and gets back an STL blob.

```
Browser (HTML/JS/CSS)
  └── POST /generate { tile_spec: "..." }
        └── dharmatiles-server (native Python, full capabilities)
              └── returns STL bytes
```

**What you get:** Full current capabilities, no code changes, OpenLOCK + logo.  
**What you lose:** The "no install" magic. User runs `pip install dharmatiles && dharmatiles-server`.  
**Effort:** Low on the Python side (FastAPI in ~50 lines). Higher on the UI side (build a web frontend).

This is the "browser butterfly on top, Python badger underneath" architecture from the ChatGPT conversation. It's pragmatic and probably the right choice if the UI is the goal rather than the zero-install delivery.

### Option D: Desktop App Shell (Tauri/Electron)

Package the web UI + Python backend together as a signed desktop app. Users download and install once; after that it behaves like a local app.

- **Tauri** (Rust shell): lean, fast, native file dialogs. Python would run as a sidecar process.
- **Electron**: heavier but well-trodden path.
- **PyWebView**: Python-native, simpler model — Python is the host, WebView is the UI.

For dharmatiles specifically, **PyWebView** might be the cleanest fit: Python process owns the generation, a WebView renders the UI, and `window.evaluate_js()` / `pywebview.api.generate()` bridges them. Total added dependency: `pywebview`.

**Effort:** Medium. Requires a UI build (HTML/JS/CSS for the interface) but no architecture change to the generator.

---

## Browser-Native Python Design

If you're designing a Python browser app from scratch rather than porting dharmatiles, the constraints become design constraints instead of blockers.

**The key insight:** trimesh is not needed. The real product of the generation pipeline is `(vertices: float32[N,3], faces: int32[M,3])` — plain numpy arrays. You can:

- Pass those arrays directly to JavaScript as typed arrays with **zero copy** via Pyodide's JS bridge. Three.js renders from `BufferGeometry` built from those arrays, giving you an interactive 3D viewport.
- Write a ~20-line binary STL serializer directly in numpy — no library needed.
- Let `scipy.spatial` handle Voronoi, KDTree, Delaunay — all available and fast.
- Let `scipy.interpolate` handle IDW/RBF terrain blending.

**The clean architecture:**

```
Python/Pyodide (Web Worker)          JavaScript (main thread)
  numpy mesh math              →      Three.js BufferGeometry → render
  scipy distributions          →      File System Access API → save STL
  heightmap, scatter, trees    →      UI (HTML/CSS/events)
```

Python never needs to know trimesh exists. CSG operations (OpenLOCK base, logo inset) are either skipped or delegated to a JS CSG library on the JavaScript side.

**Rules for designing to this model:**
- Use only packages in Pyodide's built-in set (numpy, scipy, shapely, Pillow, networkx).
- No native extensions. `manifold3d`, `pyembree`, `rtree`, `vtk` are blocked.
- Use OPFS for persistence (universally available). File System Access API (user-chosen folder) is Chrome/Edge-only.
- Run generation in a Web Worker (Pyodide supports this) to keep the UI responsive.

---

## TypeScript Alternative

TypeScript (or plain JavaScript) is a serious alternative worth comparing directly. The landscape looks different from the JS side.

### The JS 3D Ecosystem

| Need | Library | Notes |
|---|---|---|
| 3D rendering + mesh | Three.js v0.184 | BufferGeometry from typed arrays; STLExporter built in; icosphere/box/cylinder primitives |
| CSG / boolean ops | `manifold-3d` v3.5.1 | **The same manifold library**, compiled to WASM for JS. Full CSG: union, difference, intersection. This is unavailable to Python/Pyodide but fully available here. |
| Full declarative CSG | `@jscad/modeling` v2.13.0 | OpenJSCAD's modeling library — primitives, transforms, boolean ops, pure JS |
| Voronoi / Delaunay | `d3-delaunay` | Fast, well-tested; wraps the Delaunator algorithm |
| Spatial KDTree | `rbush` | R-tree for 2D spatial queries; no 3D KDTree equivalent to scipy's |
| Matrix / vector math | `gl-matrix` | Typed-array backed, fast |
| Polygon triangulation | `earcut` | Used by Three.js internally |
| Seeded random / noise | `alea`, `simplex-noise` | Reproducible seeds for procedural generation |
| STL export | Three.js STLExporter | Or ~20 lines of DataView |

### What TypeScript Gains Over Python/Pyodide

**`manifold-3d` works.** This is the single biggest structural difference. The CSG operations that are hard-blocked in Pyodide are fully available in TypeScript via the same underlying library.

**No runtime loading penalty.** Pyodide + numpy + scipy is a ~30–50 MB download with a 5–15 second cold-start. TypeScript bundles start instantly; even a Three.js + manifold-3d bundle is <5 MB and loads in under a second.

**No Python/JS bridge.** In Pyodide, passing geometry between Python and Three.js requires explicit array transfer. In TypeScript, the mesh math and the rendering are in the same language — no marshalling layer.

**Web Workers are native.** Background generation in a Worker is a first-class JS pattern with no extra setup.

**Full browser API access.** IndexedDB, File System Access, WebRTC, WebGPU — all available without wrappers or emulation.

### What TypeScript Loses

**numpy ergonomics.** This is the real cost. numpy's vectorised array operations — broadcasting, slicing, in-place mutation across millions of elements — have no direct equivalent in JS. You'd use typed arrays (`Float32Array`, `Int32Array`) with explicit loops or hand-written SIMD-style operations. For a heightmap touching 200k cells, this means the code is more verbose and more error-prone.

**scipy maturity.** `d3-delaunay` is excellent, but scipy's spatial module is broader and more polished. 3D KDTree queries (used for closest-point lookups), RBF interpolation, and morphological operations have JS equivalents but they're patchwork rather than a unified library.

**Python conciseness for math-heavy code.** The grass field simulation, tree skeleton algorithm, scatter distributions — these are written as readable, dense Python. The TypeScript equivalents would be longer and more imperative.

### Honest Comparison

| Dimension | Python/Pyodide | TypeScript |
|---|---|---|
| CSG (manifold) | ✗ blocked | ✓ `manifold-3d` |
| Startup time | 5–15 s cold | <1 s |
| Array math ergonomics | ✓ numpy | ✗ verbose typed arrays |
| 3D rendering | bridge to Three.js | ✓ native Three.js |
| Spatial algorithms | ✓ scipy | partial (d3-delaunay, rbush) |
| Package ecosystem for browser | Pyodide set (~250 pkgs) | npm (vast) |
| Language you write | Python | TypeScript |

**If the goal is "browser-first, full feature set, no install, CSG included":** TypeScript wins. The `manifold-3d` availability closes the biggest gap, and the startup-time difference is material for a tool people use repeatedly.

**If the goal is "I want to write Python and have it run in a browser":** Pyodide + browser-native design works well for a feature set that stays within numpy/scipy and skips CSG. The code is more concise and the algorithms translate directly from the Python mental model.

**The sharpest version of the tradeoff:** you're choosing between numpy's expressive power (Python) and manifold's CSG power (TypeScript). For terrain generation specifically — where you spend more time on distributions, heightmaps, and mesh construction than on boolean ops — Python's array ergonomics are the bigger daily advantage. For a tool that does heavy CSG (interlocking parts, slots, insets), TypeScript's manifold access tips the balance.

---

## Recommendation

**For dharmatiles now:** Option B (Pyodide + micropip trimesh) is worth a quick experiment — create a single `src/scripts/browser_demo.html` that imports Pyodide, installs trimesh via micropip, imports the generator (with manifold3d paths guarded), and exports a DungeonBlocks STL. If trimesh installs cleanly, you'd have a working in-browser DB generator in a day or two of adaptation work.

**For a richer experience with full capabilities:** Option C (local helper) gets you a polished web UI talking to native Python, with zero capability loss. The web UI can be as fancy as you like while the backend stays exactly as it is.

**For something you could ship to strangers:** Option D (PyWebView or Tauri desktop app) is the clearest path to a distributable tool. The user gets a native app experience; you get full Python. The gap between "a web page" and "a signed installable" is real friction, but it's a one-time hurdle for users.

---

## Quick Experiment Steps (Option B)

To test Pyodide viability concretely:

1. Create `src/scripts/browser_test.html` — a minimal Pyodide bootstrap.
2. Load numpy + scipy + micropip, then `await micropip.install("trimesh")`.
3. Add `dharmatiles` source to the virtual FS (pack into a JS object or load as a zip).
4. Call `build_tile_from_spec()` on the simplest tile (`1x1-soil+grass`).
5. Inspect whether trimesh installs and the generation completes.

If it works, the main remaining tasks are: guard the manifold3d import, replace OpenLOCK with a stub or just skip it, and wire up the File System Access API for STL download.

The load time penalty and Chrome-centric file access are real usability trade-offs. For a personal tool or a demo, that's fine. For something you'd share widely, the local helper or desktop app is more robust.
