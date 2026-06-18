# Orin Strategic Review — 2026-06-18

**Scope:** High-level, strategic — the shape of the design, not line-by-line findings.
**Prior reviews:** 2026-06-13 (DRY), 2026-06-18 (elegance/detail). This review does not repeat those findings.
**Reviewer:** Orin (claude-sonnet-4-6)

---

## The Shape of the Thing

DharmaTiles is a **pipeline with a language attached**. The pipeline is a linear sequence: heightmap → region masks → layers applied in order → terrain solid → base attachment → STL. The language is `.tile.py` files: live Python that instantiates the pipeline configuration directly. There is no intermediate representation, no serialization step, no compile phase. The spec is the object graph.

The central accumulator is `TileScene`, a thin struct carrying three mutable arrays (terrain_z, terrain_support_z, obstacle_mask) and a parts list. Every layer receives the scene, mutates what it needs to, and returns meshes. Ordering of this mutation is the tile author's responsibility: rocks before grass, lower regions before upper ones, etc.

Around this core, the design makes three structural choices worth examining:

1. **Region/Boundary/Layer** as the composition vocabulary
2. **Scatter + scatter-things** as a unified placement system
3. **Systems** as a late-binding scale/base parameterization

These are the load-bearing decisions. Everything else is implementation.

---

## What the Design Gets Right

**The scatter protocol is the best structural decision in the codebase.** `Rocks`, `Grass`, `Tree`, `Flowers` share a single duck-typed interface: `scatter(scene, *, placement_mask, layer_idx)` + `footprint_mm()`. The `Scatter` layer composes them by running them in order. Adding a new scatter thing requires implementing two methods and registering nothing — the `Scatter(*things)` variadic constructor handles the rest. This is the one place in the codebase where composition is fully achieved without any residual special-casing or inheritance. It sets the standard everything else should aspire to.

**The tile spec language is well-aimed.** Tile files are 20–80 lines of real Python. The vocabulary is small: `Tile`, `Region`, `Boundary`, `FloodFill`, `Edge`, layers, scatter things, `D[min:max].power(n)`. A designer who isn't a deep programmer can read and modify these files. The language IS the implementation — no parsing, no DSL compiler, no string-based type system. Layers hold real layer instances. This directness is correct and rare.

**The multi-system abstraction is right-sized.** `DungeonBlocks` and `OpenLOCK` are thin classes that specify a scale transformation and a base-attachment function. The pipeline core runs once at each needed scale; the cache in `build_tile_from_spec` shares the build when two systems coincide. This is the right boundary: content generation is scale-agnostic; base geometry is system-specific; the two are joined late.

**`derive_seed` is a genuinely principled solution.** Deterministic seeds via blake2s keyed on `(master, label, layer_idx)` means every sub-process has a unique, reproducible, human-readable derivation path. No magic XOR constants, no collision risk, no seed-management bugs. This is the kind of small infrastructure decision that prevents entire categories of subtle bugs.

**The `TileScene` accumulator level is correct.** Three arrays is the right amount of shared mutable state. More would create invisible coupling between layers; less would force layers to expensively recompute what prior layers already know. The `displace_terrain` / `set_terrain` helpers that sync `terrain_support_z` automatically are a good design: they make the invariant enforced rather than documented.

---

## Where the Shape Fights Itself

### The ordering problem is structural, not documentary

In the current design, the side-effect ordering of `Scatter(*things)` is load-bearing and invisible. Grass blades steer around rocks, but only if `Rocks` appears before `Grass` in the argument list. Water tiles comment their region ordering explicitly (`pool before meadow so rocks are stamped before grass grows`). The corridor tile comments its boundary placement (`top-margin before path`).

This is not a documentation problem. The pipeline is a series of stateful mutations with ordering constraints, and the spec language — which is otherwise declarative — gives the author no way to express those constraints except by relying on the implicit ordering of Python lists.

A declarative spec language with implicit temporal coupling is a category mismatch. You write `Scatter(Rocks(...), Grass(...))` which looks like "scatter rocks and grass together," but what it means is "scatter rocks, then scatter grass using the state left by rocks." The difference is invisible at the call site.

The fix is not to change the underlying mechanism — ordering is genuinely necessary — but to surface it. One option: a `before=` dependency marker on scatter things. Another: rename the composition to something that names its sequential nature (`Pipeline` instead of `Scatter`, `Then(Rocks(...), Grass(...))` instead of `Scatter(Rocks(...), Grass(...))`). The name `Scatter` implies spatial distribution, not temporal sequencing. These are two different things that happen to be bundled together, and the name lies about which one matters more.

The same problem exists at the `Tile.areas` level: region ordering in the list controls which region's rocks are stamped before which region's grass grows. This is documented in CLAUDE.md but not visible in any code the tile author touches.

### The heightmap is the whole world, but the API pretends otherwise

`terrain_z` is the physical ground. Every layer reads it. Some layers mutate it (SoilCarpet, GrassCarpet, WaterLayer). The `make_heightmap_solid` call at the end converts it to geometry.

But the layers' relationship to `terrain_z` is inconsistent. Some layers stamp `terrain_support_z` (the "ceiling" that later grass can't exceed). Some stamp `obstacle_mask` (which grass steers around). Some add to `terrain_z` directly. The water layer reshapes `terrain_z` entirely via `set_terrain`. These are three semantically distinct ways to interact with the ground, and the layer protocol (`apply(scene, *, placement_mask) -> list[Trimesh]`) doesn't distinguish between them.

This means a new layer author faces an invisible question: should my layer call `displace_terrain`, or stamp `terrain_support_z`, or write to `obstacle_mask`, or return geometry? The answer depends on what downstream layers need to know — and that's coupling that runs backward through the pipeline.

The deeper issue: the pipeline mutates a heightmap, but physical terrain is not a heightmap — it is a surface. The heightmap assumption is stated explicitly in `TileScene`'s docstring (`"All geometry layers treat the terrain surface as locally horizontal"`). For flat tiles this is fine. For anything with vertical features (cliffs, walls, overhangs), the entire architecture is the wrong primitive. The "future slope-normal API" documented as a comment in `TileScene` is not a surface issue; it is an acknowledgment that the heightmap model has a hard ceiling.

This is not a problem to fix today. But it is important to understand: the system's extensibility limit is the heightmap. Every new content type that fits within "feature sitting on a locally flat surface" is easy. Anything that doesn't (vertical rock faces, cave mouths, multi-level terrain) requires architectural work, not layer work.

### Region/Boundary is elegant for two regions, awkward for three

The `areas` list interleaves `Region` and `Boundary` objects. This is elegant for the two-region case (meadow / margin / dirt): the boundary lives between the two regions it divides, and the intent is readable.

For the three-region corridor tile (meadow-top / top-margin / path / bottom-margin / meadow-bottom), the ordering becomes a declaration of topology: the author must know that `FloodFill(0.5, 0.85)` will seed the top region and `FloodFill(0.5, 0.15)` will seed the bottom, and they must place the boundaries between the right regions in the right order. The flood-fill system doesn't enforce that the boundary separates the two regions it logically belongs to — it just draws a line and the BFS fills from wherever you said to fill from.

This works, but it's fragile. The tile author is doing manual topology management. If a flood fill seed lands in the wrong region (because the boundary didn't quite carve the space the author expected), the regions swap contents silently. There's no validation, no assertion that "meadow-top's FloodFill seed is actually on the top side of top-margin." The system trusts the author to get the spatial relationships right.

For a system intended to be extended by non-expert Python users, this is a design tax. The alternative — explicit adjacency declarations, or seed-point auto-assignment based on boundary geometry — would be more robust but also more complex to implement. The current approach is the right tradeoff for now, but its fragility should be acknowledged rather than obscured.

### The config class explosion is a symptom, not the disease

The prior elegance review documented the `SpeciesConfig / SoilConfig / RocksConfig` anti-pattern (frozen dataclass + custom `__init__` + values dict). That review is correct. But the deeper issue is why there are so many config classes at all.

The answer is: each class exists to be shareable. `SpeciesConfig` is passed to both `GrassCarpet` and `Grass` so they share blade geometry. `RocksConfig` is separate from `Rocks.placement` so the rock geometry can be reused without the placement policy. This is good intent — sharing is correct — but it has produced a config surface that is larger than the problem.

Count the distinct config objects in a complex tile: `SurfaceConfig`, `SpeciesConfig`, `SoilConfig` (implicit inside `SoilCarpet`), `GrassUnderlayConfig` (implicit inside `GrassCarpet`), `RocksConfig` (wrapped by `Rocks`), `ScatterConfig` (wrapped by `Uniform` / `Grouped`). A tile author who wants to understand what parameters are available for a grass region has to understand the distinction between `SpeciesConfig` (blade geometry), `GrassUnderlayConfig` (carpet rendering), `GrassConfig` (internal runtime wrapper), `Grouped` (placement strategy). These are four config objects in the path from "I want grass" to "I have grass."

The issue is that the configuration taxonomy was designed around implementation modules (one config per module), not around user intent (one config per thing a user wants to control). A tile author doesn't think in terms of "GrassUnderlayConfig" — they think in terms of "the carpet layer." The config classes should probably be collapsed toward the thing they configure, not kept separate for reusability that in practice is used in only one way.

### The pipeline is not composable in the way tile authors actually want

The current model: one tile, one pipeline run, one set of outputs. If you want a 1x1 and a 3x3 version of the same tile, `repeat_sizes` provides a shortcut — but it's a post-hoc helper, not a capability of the pipeline itself.

More importantly: there is no way to reuse regions across tiles, no way to define "a meadow region" once and apply it to multiple tile layouts. Each `.tile.py` file is standalone. The corridor tile duplicates the meadow region factory with a helper function (`_meadow(region_id, x, y)`) — which is fine, but it's Python-level composition, not pipeline-level composition.

This is not necessarily a problem. Python-level composition (shared `_species` objects, helper functions, imported region builders) may be exactly the right level. But it means the compositional unit is the Python module, not the spec dataclass. The spec objects are not really composable — you can't take a `Region` from one tile and embed it in another because `FloodFill` positions are tile-relative and `layer_idx` seeds depend on position in the spec list.

The lack of named, reusable region templates means that as the tile library grows, tile files will gradually diverge in style even when they intend to share the same content. This is already visible: the meadow region in the corridor tile and the meadow region in the water+grass tile configure `GrassCarpet` with different `groups_per_square` values (240 vs 2) for no documented reason.

---

## The Central Question

**Should the pipeline be aware of intent, or only of effect?**

Currently, the pipeline is purely effect-based. Layers run in order. They read and write shared state. The orchestrator doesn't know that `Rocks` exists to create obstacles for `Grass` — it just runs them in order and trusts that the state left by each layer is what the next layer needs.

This works. But it creates two compounding problems:

First, the ordering constraint is invisible and load-bearing. Tile authors must know the causal chain (rocks before grass, pool before shoreline) without any help from the type system or the API. The current code comments make this explicit, but comments are documentation, not contracts.

Second, as the scatter vocabulary grows (Rocks, Grass, Tree, Flowers, and presumably more), the causal graph of "who needs whose state" grows with it. Today it's a simple chain: rocks → grass. Tomorrow it might be: rocks → grass, flowers → grass, trees → (rocks + grass). The pipeline has no way to express or validate this graph — it just runs the list.

The question isn't whether to add a dependency graph now. It's whether the current design — pure temporal ordering via Python list position — is the right model as the system grows, or whether there's a simpler alternative that makes the intent visible without adding a dependency engine.

The simplest alternative: **name the phases explicitly**. Instead of `Scatter(Rocks(...), Grass(...))` where ordering is implicit, something like:

```python
Scatter(obstacles=[Rocks(...)], plants=[Grass(...)])
```

This makes the causal relationship structurally visible — "obstacles first, plants second, plants steer around obstacles" — without adding a dependency graph. The two phases run in fixed order. A tile author can't accidentally put grass before rocks because the API doesn't accept `plants=` and `obstacles=` as interchangeable.

This is the single most important architectural question for the tile spec language: how to make temporal constraints explicit in a declaration.

---

## Recommendations

These are structural shifts, not line edits. They are ordered by impact.

**1. Name the ordering constraint in the Scatter API**

The most important user-facing change. Introduce explicit phases in `Scatter`: ground-stampers (rocks, flowers, trees) before vegetation (grass). The API should make it impossible to accidentally put grass before rocks. One path: `Scatter(obstacles=[...], vegetation=[...])` with fixed phase order. Another: a `before()` / `after()` declarative API on scatter things. The exact spelling matters less than the principle: ordering constraints should be structural, not conventional.

This is a breaking API change for existing tile files, but the existing tile files are few and the refactor is mechanical.

**2. Collapse config toward the thing being configured**

`GrassUnderlayConfig` should not be a separate class from `GrassCarpet`. `RocksConfig` should not be a separate class from `Rocks`. The shareability argument for keeping them separate is theoretical — in practice, `RocksConfig` is never shared between two `Rocks` instances. When shareability is actually needed (as with `SpeciesConfig` shared between `GrassCarpet` and `Grass`), the config class is justified. Otherwise, flatten it: put the params directly on the layer class, accept keyword arguments.

This reduces the number of named concepts in the public API and makes the documentation surface smaller for tile authors.

**3. Make the pipeline's boundary explicit**

The boundary between "content pipeline" and "export pipeline" is currently implicit. The content pipeline ends with a list of colored meshes. The export pipeline takes that list, attaches a base, and writes an STL. These are the two phases of `build_tile_from_spec`, and they have different extension points (a new content type extends the first; a new base system extends the second).

Currently they're interleaved in `build_tile_from_spec` and `_build_tile_mesh`. The internal function boundary (`_build_tile_mesh` returns `(colored_meshes, scene)`, then `system.export()` takes over) is close to the right cut, but the function naming and organization don't emphasize it. Making this split explicit — perhaps as two documented stages with a clean data handoff type — would make it easier to test, debug, and extend each half independently.

**4. Establish a tile template / region library pattern**

As the tile library grows, the lack of shared region definitions will produce drift. Establish a canonical `src/tiles/shared/` module (or a `dharmatiles.templates` package) with well-tuned, named region factories: `meadow(seed, groups_per_square)`, `shoreline(amplitude_mm, rock_density)`, `water_pool(depth_mm)`. Tile files import and compose these rather than redefining them.

This is not a change to the pipeline — it's a change to the tile-authoring convention. But it prevents the tile library from becoming 30 slightly-different-but-inconsistent meadows.

**5. Treat `TileScene` as the system's documented API contract**

`TileScene` is the one object that every layer, every scatter thing, and every base system touches. It is the integration surface. Currently its docstring describes what it is, but not what layers are allowed to do to it. The mutation contract (`displace_terrain` yes, `terrain_z +=` no; `obstacle_mask` is for stamping, not reading by content layers; etc.) should be explicit.

This is not about adding enforcement. It is about making the implicit contract visible so that new layer authors know what they can safely do and what they should not do. The boundary between "safe to mutate from a layer" and "private to the orchestrator" is currently undocumented at the code level. A short `# Layer contract:` block in the `TileScene` docstring would suffice.

---

## Architecture Verdict: CLEAN (core), DRIFTING (surface)

The pipeline spine is correct. The scatter protocol is a genuine design success. The tile spec language is well-aimed. The mutable accumulator model is the right level of coupling for this problem.

The drift is at the user-facing surface: ordering constraints that are implicit and load-bearing, config classes that multiply without clear justification, and a growing tile library that lacks shared vocabulary. None of this breaks the system. All of it makes the system harder to grow correctly.

The central investment opportunity is making temporal ordering visible in the Scatter API. Everything else is maintenance. That one change would make the design match its intent.
