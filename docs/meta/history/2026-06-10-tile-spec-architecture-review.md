# Tile-spec architecture review — 2026-06-10

A deep review of the `.tile.py` author experience and the architecture
behind it, after the recent migration from the static `.tile` (YAML-like)
file format to plain Python.

The proposals here are graded by the four design goals:

- **S** — Simplify the overall architecture
- **P** — Increase the power available to `.tile.py` authors
- **C** — Clean up the surface visible to authors (DRY, fewer concepts)
- **T** — Increase transparency, reduce magic

Each proposal lists which goals it serves and what would need to change.
None of them have been implemented yet; this is a design-direction doc.

---

## Where things stand today

Reading the six current specs in `src/tiles/` gives a clear feel for the
sweet spot of the format:

```python
tile = Tile(
    surface=SurfaceConfig(seed=42),
    regions=[
        Region(id='meadow', contains=(0.25, 0.5), layers=[
            GrassCarpetLayer(species=_species, groups_per_square=240),
            ScatterLayer(
                Grass(species=_species, groups_per_square=24),
            ),
        ]),
        Region(id='dirt', contains=(0.75, 0.5), layers=[
            SoilCarpetLayer(),
        ]),
    ],
    boundaries=[
        Boundary(id='margin', from_anchor=('top', 0.48),
                 to_anchor=('bottom', 0.52), path='organic',
                 amplitude_mm=5.0, wavelength_mm=10.0),
    ],
)
```

The shape — `Tile → Regions → layers (terrain texture + ScatterLayer of
things)` — is good. The friction lives in the details: hidden kwarg
splitting, hidden carpet-scaling defaults, hidden seed XOR constants,
hidden mutation of `scene.grass_mask`, vestigial `SceneConfig` plumbing,
and a few asymmetries that show through.

---

## A. Kill the magic kwarg-splitter on layer/thing constructors

### Problem

Four classes today silently split flat kwargs across multiple configs:

| Class | Splits across |
|---|---|
| `GrassCarpetLayer` | `GrassUnderlayConfig` + `SpeciesConfig` |
| `SoilCarpetLayer` | `SoilConfig` |
| `Rocks` | `RocksConfig` (+ implicit `ScatterConfig` derivation) |
| `Grass` | `SpeciesConfig` (+ implicit `ScatterConfig` derivation) |

```python
# layers/grass_carpet.py — 30 lines just for this
species_over = {k: v for k, v in kwargs.items() if k in _SPECIES_FIELDS}
carpet_over  = {k: v for k, v in kwargs.items() if k in _UNDERLAY_FIELDS}
unknown = set(kwargs) - _SPECIES_FIELDS - _UNDERLAY_FIELDS
```

Why this hurts:

- **Magic.** `GrassCarpetLayer(blade_length_min=10)` looks like it
  sets a layer parameter but actually overrides a `SpeciesConfig` field.
  The author has no way to tell from the call site.
- **Brittle.** If two configs happen to share a field name, the splitter
  silently picks one (alphabetical iteration order). Today they don't
  collide; a refactor could break that silently.
- **One-off.** Every layer/thing class invents its own copy of the same
  `_FIELDS = {...}` / `unknown` / `dataclasses.replace` boilerplate.
- **Hides composition.** The whole point of having `SpeciesConfig` as a
  separate dataclass is so it can be *shared* between the carpet and the
  3D `Grass`. The flat-kwargs API encourages the opposite.

### Proposal

Drop the flat-kwargs splitter entirely. Make every layer/thing take its
configs explicitly:

```python
# Before
GrassCarpetLayer(species=species, groups_per_square=240, noise_top_mm=0.4)

# After
GrassCarpetLayer(
    species=species,
    underlay=GrassUnderlayConfig(noise_top_mm=0.4),
)
# or, when the author wants to override one species field:
GrassCarpetLayer(species=replace(species, groups_per_square=240))
```

For the common one-line case, expose a tiny convenience:

```python
species = SpeciesConfig().override(groups_per_square=240, blade_length_min=15)
```

`override` is `dataclasses.replace` with a friendly name; we already use
`replace` in three places internally.

### Goals served

S, C, T

### Risk

Existing specs need a sweep — six files, mostly mechanical. The win is
that the constructor signature *tells the truth*: configs in, layer out.

---

## B. Stop the carpet from silently rescaling species geometry

### Problem

`GrassCarpetLayer` applies two hidden multipliers to the `SpeciesConfig`
it receives:

```python
_CARPET_LENGTH_SCALE: float = 0.75            # 75 % of species blade length
_CARPET_COUNT_SCALE:  float = 2.0 / 3.0       # 2/3 of species blade count
```

…unless the author overrode the relevant fields, in which case the
scaling is skipped per-field. So:

- `GrassCarpetLayer(species=s)` → carpet blades are 75 % of `s.blade_length_*`.
- `GrassCarpetLayer(species=s, blade_length_min=12)` → carpet uses 12 mm
  (no scale), but `blade_length_max` is still scaled. Subtle.
- The author who *wants* identical geometry between carpet and 3D blades
  (the documented purpose of sharing a `SpeciesConfig`!) gets it
  *only if* they override both length fields.

The CLAUDE.md guidance says: "Pass the same SpeciesConfig instance to
both so they share identical blade geometry." That promise is then broken
by the carpet layer.

### Proposal

Remove the in-layer scaling. If the carpet really wants shorter / sparser
blades than the 3D pass, expose that as an explicit knob on
`GrassUnderlayConfig`:

```python
@dataclass(frozen=True)
class GrassUnderlayConfig:
    ...
    blade_length_scale: float = 0.75   # multiply species blade lengths
    blade_count_scale:  float = 2/3    # multiply species blade density
```

Two changes:

1. The author can *see* the scaling at the call site (it appears in
   `GrassUnderlayConfig` defaults).
2. Reading the spec, you can predict what the carpet will look like
   without needing to know about a class constant.

Even better: drop the defaults to 1.0 and let the author override
explicitly if they want a difference. Today's behaviour assumes "the
carpet should look different from the upright blades," which is a design
judgment, not a layer responsibility.

### Goals served

T, C, P (authors gain explicit control over the relationship)

---

## C. Stop the temporary mutation of `scene.grass_mask`

### Problem

Two layers and one thing all do the same save-mutate-restore dance:

```python
# scatter/prototype.py (Rocks.scatter)
old_grass_mask = scene.grass_mask
if placement_mask is not None:
    scene.grass_mask = placement_mask
try:
    positions = scatter_positions(...)
finally:
    scene.grass_mask = old_grass_mask
```

Same pattern in `Grass.scatter` (with extra `&` intersection logic) and
`GrassCarpetLayer.apply`. The reason is that `scatter_positions` reads
`scene.grass_mask` for two unrelated jobs:

1. To **find candidate cells** for Voronoi groups.
2. To **scale the group count** (so a half-tile grass region gets half
   the groups of a full-tile one).

Both are placement queries, but the caller already *has* the placement
mask — it just got passed to `apply()` as `placement_mask`. The
roundabout-through-the-scene plumbing is purely historical.

### Proposal

Remove `scene.grass_mask` from the API. Thread the mask through
parameters:

```python
def scatter_positions(
    scatter_cfg: ScatterConfig,
    placement_mask: np.ndarray,
    scene, surface, rng,
) -> list[Position]:
    ...
```

…and pass the same `placement_mask` into the count-scaling helper. The
"intersect with prior mask" logic in `Grass.scatter` becomes a one-liner
the orchestrator does once, before calling layers.

Then strip `grass_mask` (and probably `rock_placement_mask`, which is
unused) from `TileScene`.

### Goals served

S, T, C

### Why this matters

Every save/restore is a code smell saying "this function should have
taken a parameter." We have three copies of it. Killing them removes a
non-obvious coupling: today, a future layer that calls `scatter_positions`
without remembering the save/restore dance would silently get the wrong
group count. The compiler can't catch that.

---

## D. Make scene-state invariants explicit; replace ad-hoc syncs

### Problem

Layers mutate `terrain_z`, `terrain_support_z`, `vegetation_support_z`,
`rock_mask` in an order that matters but is undocumented in code. Each
layer ends with whatever invariant *it* needs:

```python
# SoilCarpetLayer.apply ends with:
scene.terrain_support_z[:] = scene.terrain_z

# GrassCarpetLayer.apply ends with:
scene.terrain_support_z[:] = scene.terrain_z

# Rocks.scatter writes terrain_support_z + rock_mask
# Grass.scatter syncs vegetation_support_z from terrain_support_z, then
# lifts it to include rocks before growing
```

So the rule is roughly:
- *terrain-texturing layers* sync `terrain_support_z` to `terrain_z` at
  the end.
- *rock layers* stamp `terrain_support_z` upward.
- *grass layers* sync `vegetation_support_z`, then `max` it with
  `terrain_support_z`.

This is fine when you know it, but it's distributed knowledge — no
single place in the code states the invariant, and a new layer author
has to reverse-engineer it from existing layers.

### Proposal

Centralise the invariants in the orchestrator. Layers declare *what they
do* via a small enum (or, more Pythonically, by which method they
override on a base class), and the orchestrator handles the syncs:

```python
class TerrainTextureLayer:
    """Mutates terrain_z. Orchestrator re-syncs support fields after."""
    def texture(self, scene, *, placement_mask) -> list[Trimesh]: ...

class ScatterPass:
    """Stamps terrain_support_z / vegetation_support_z. Orchestrator
       does no post-sync."""
    def scatter(self, scene, *, placement_mask) -> list[Trimesh]: ...
```

Or — even simpler — make `TileScene` enforce the invariant directly:
when `terrain_z` is mutated through `scene.add_terrain_displacement(...)`,
`terrain_support_z` follows automatically. The current direct-mutation
API stays for layers that need it, but the high-frequency case is
encapsulated.

Either way, today's pattern of "every layer remembers to add `scene.
terrain_support_z[:] = scene.terrain_z` at the end" should go away.

### Goals served

T, S

---

## E. Decompose `ScatterConfig` placement strategies into composable pieces

### Problem

`ScatterConfig` is one dataclass that switches between two
distribution algorithms via a sentinel value:

```python
items_per_square:  int = 0      # > 0: hard count; 0 = area-based
groups_per_square: int = 0      # 0: uniform random; > 0: Voronoi groups
```

`scatter_positions` then branches:

```python
if scatter_cfg.groups_per_square > 0:
    return _voronoi_positions(...)
else:
    return _uniform_positions(...)
```

This works but:

- "0 means area-based" / "0 means uniform" are stringly-typed flags.
- A third placement strategy (Poisson disc, hex grid, jittered grid
  without Voronoi grouping) would mean a third branch and a third
  sentinel meaning.
- `gap_mm` is meaningful only when `items_per_square == 0`; in the
  Voronoi-with-hard-count branch the field is silently overridden by
  proportional distribution.

### Proposal

Promote each placement strategy to its own dataclass:

```python
@dataclass(frozen=True)
class Uniform:           # uniform random within mask
    count_per_square: int | None = None    # None = area-derived
    gap_mm:           float = 2.0

@dataclass(frozen=True)
class Grouped:           # Voronoi groups + jitter grid
    groups_per_square: int
    count_per_square:  int | None = None
    gap_mm:            float = 2.0
    group_dir:         str = 'random'      # 'random' | 'none' | callable

@dataclass(frozen=True)
class Hex:               # hex-lattice fixed positions (future)
    spacing_mm: float
    jitter:     float = 0.0
```

`Rocks(...)` and `Grass(...)` take a `placement=` arg:

```python
Rocks(rocks=RocksConfig(...), placement=Uniform(count_per_square=15))
Grass(species=s, placement=Grouped(groups_per_square=3))
```

Each strategy has its own `.positions(mask, scene, surface, rng) -> list`
method. `scatter_positions` becomes a one-line dispatch.

### Goals served

S (one less god-object), P (new strategies are easy to add), T
(each strategy's parameters are *only* the ones it uses).

### Cost

A bit more typing in specs. The current `ScatterConfig` could stay as a
deprecated alias for `Grouped` for one release.

---

## F. Stop deriving `Rocks` defaults from `RocksConfig.rocks_per_square`

### Problem

`Rocks` accepts `rocks_per_square` (a `RocksConfig` field), and then in
`__init__` copies it into a separately-constructed `ScatterConfig`:

```python
self.scatter_cfg = scatter or ScatterConfig(
    items_per_square  = self.rocks.rocks_per_square,
    ...
)
```

So `rocks_per_square` lives in `RocksConfig` but is semantically a
placement parameter, not a geometry parameter. If the author *does* pass
`scatter=ScatterConfig(items_per_square=X)`, the `RocksConfig` value is
ignored. Two sources of truth.

### Proposal

Once §E lands, `rocks_per_square` moves out of `RocksConfig` into the
placement strategy (`Uniform(count_per_square=15)`). `RocksConfig`
becomes purely geometry: `r_min`, `r_max`, `flat_min`, etc.

Same applies to `SpeciesConfig.groups_per_square` and
`SpeciesConfig.gap_mm` — they're placement, not geometry.

### Goals served

T (one source of truth), C

---

## G. Stop pretending `SceneConfig` is used

### Problem

`_scene_config_from_spec(tile)` builds a `SceneConfig` with default
`SoilConfig`, `RocksConfig`, `BaseConfig` even when those layers aren't
in the spec:

```python
return SceneConfig(
    surface = tile.surface,
    soil    = SoilConfig(),     # ignored unless a SoilCarpetLayer reads it
    rocks   = RocksConfig(),    # ditto
    base    = BaseConfig(),     # actually used by base export
    max_stack_height = 2.0,     # also used by Grass — duplicates Grass kwarg
)
```

Layers don't *read* `scene.config.soil` or `.rocks` — they each carry
their own config inside the layer instance, as they should. The only
config currently read from `scene.config` is `surface`. `SceneConfig` is
a fossil of the pre-spec architecture where one global config drove
everything.

### Proposal

Replace `SceneConfig` with just `surface: SurfaceConfig` (and `base:
BaseConfig`, since base is genuinely tile-level — but see §H).

```python
@dataclass
class TileScene:
    surface: SurfaceConfig
    terrain_z: np.ndarray
    terrain_support_z: np.ndarray
    vegetation_support_z: np.ndarray
    rock_mask: np.ndarray
    parts: list[Trimesh] = field(default_factory=list)
```

All references to `scene.config.surface` become `scene.surface`. Saves
one indirection across every layer.

### Goals served

S, T, C

---

## H. Make the base system part of the spec, not a global flag

### Problem

The "DungeonBlocks + OpenLOCK" export logic lives in the orchestrator and
is steered by `BaseConfig.style`. The OpenLOCK build runs the entire
pipeline a second time at `square_mm=25.4`. Today this is:

- Implicit (the spec author doesn't say "I want OL output").
- Asymmetric (DB is "primary," OL is "rebuilt").
- Hard to extend (a third system needs orchestrator changes).

### Proposal

Move system choice into the spec:

```python
from dharmatiles.systems import DungeonBlocks, OpenLOCK, BareSystem

tile = Tile(
    surface=SurfaceConfig(seed=42),
    systems=[DungeonBlocks(), OpenLOCK()],   # default in module
    regions=[...],
)
```

Each system declares:

- The scale (`square_mm`).
- The output suffix (`db`, `ol`, etc.).
- How to attach its base to the terrain mesh.

The orchestrator loops over `tile.systems`, runs the scene at each
system's scale, and exports. `BareSystem()` is today's `style='none'`.

For one-system tiles, `systems=[DungeonBlocks()]` is the only change.

### Goals served

P (extensibility — easy to add Adventurer's Realm, OpenForge, custom
bases), T (the spec spells out what gets generated), S (orchestrator
becomes a uniform loop).

### Cost

Touches `bases/` and `terrains/tile.py`. Worthwhile if a third system is
ever added; less compelling if we'll only ever have two.

---

## I. Move terrain-height construction into a `Terrain` layer

### Problem

Today the orchestrator runs `_build_spec_terrain` *before* layers, using
hardcoded IDW blending of `Region.effective_height_mm`. The author has
no hook to:

- Use a non-IDW height blend (linear ramp, hard step, curve).
- Add a base sinusoidal terrain under everything (the legacy "stand-in"
  is dead-code-only).
- Make region heights data-driven (e.g., a heightmap PNG).

`effective_height_mm` is itself awkward — it falls back to "first
layer's `height_default_mm`" which only makes sense for a small subset
of cases.

### Proposal

Make the terrain heightmap itself the output of an explicit `Terrain`
layer (or `Region.terrain=`) so authors can choose:

```python
Region(id='meadow', contains=(0.5, 0.5),
       terrain=FlatHeight(5.0),
       layers=[GrassCarpetLayer(), ...])

Region(id='hill', contains=(0.7, 0.7),
       terrain=GaussianMound(peak=12.0, sigma_mm=20.0),
       layers=[GrassCarpetLayer(), ScatterLayer(Grass(...))])
```

The orchestrator composes regional terrains (still via IDW by default)
into the master `terrain_z`. The IDW blend itself becomes a `BoundaryBlend`
strategy declared on `Boundary` (linear / quadratic / IDW), which solves
the today's *implicit* "quadratic slope into water zones" mentioned in
CLAUDE.md.

### Goals served

P (huge — slopes, hills, ramps, custom heightmaps all become tile-spec
features), T (no more `effective_height_mm` magic), C (one less property
on `Region`).

### Cost

The biggest change here. Probably worth phased rollout: keep
`height_mm` as a shortcut for `terrain=FlatHeight(...)`.

---

## J. Replace anchor tuples with a typed builder

### Problem

```python
Boundary(from_anchor=('left', 0.5), to_anchor=('bottom', 0.5), ...)
```

`('left', 0.5)` is a string + float that's not validated until runtime
(`_anchor_to_mm` raises if the edge name is wrong). The four valid edge
names live in `core/region.py`, far from the author's spec.

### Proposal

```python
from dharmatiles.spec import Edge

Boundary(
    from_anchor = Edge.LEFT(0.5),
    to_anchor   = Edge.BOTTOM(0.5),
    ...
)
```

`Edge` is an enum with `__call__` returning a small `Anchor` value object
(or `(edge, t)` tuple if we want to stay lightweight). Authors get
autocomplete; typos are caught at parse time.

A power-user benefit: a future `Edge.INTERIOR(x_norm, y_norm)` could
let boundaries terminate anywhere, not just at the perimeter.

### Goals served

C, T, P (interior anchors)

---

## K. Replace `Region.contains` with a region-selection predicate

### Problem

`Region.contains=(0.25, 0.5)` is a single flood-fill seed point.
Limitations:

- Two-region tiles with one region wrapping around the other are hard to
  express.
- Authors who think in shapes ("the lower half") rather than seed
  points have to do mental arithmetic.

### Proposal

Allow several seed shapes:

```python
Region(id='meadow', selector=Point(0.25, 0.5), layers=[...])
Region(id='meadow', selector=Rect(0.0, 0.0, 0.5, 1.0), layers=[...])
Region(id='donut',  selector=Points([(0.2, 0.5), (0.8, 0.5)]), layers=[...])
```

Selectors are tiny objects with a `seeds(surface) -> list[(row, col)]`
method. `Region(contains=...)` stays as a one-arg shortcut for `Point`.

### Goals served

P, C

---

## L. Sort out `Tile.sizes`

### Problem

`SurfaceConfig` already has `cols` and `rows`. But there's *also*
`Tile.sizes=[(1, 1), (3, 3)]` which means "emit at 1×1 and at 3×3".
Some specs use it; others don't. The CLI batch loop walks
`sizes`. When set, `surface.cols`/`rows` are rewritten on the fly.

The author has to remember which field controls what.

### Proposal

One field. Either:

- Drop `sizes`; let the author write multiple `Tile` instances in the
  spec file (e.g. `tiles = [tile_1x1, tile_3x3]`). The orchestrator
  picks up a `tile` *or* `tiles` binding.
- Keep `sizes` but rename `cols`/`rows` to a private internal field
  set only by the size-iterator.

I lean toward the multi-binding approach because it makes the spec read
the way the output reads (one entry per emitted file). A helper:

```python
def repeat_sizes(base: Tile, sizes: list[tuple[int, int]]) -> list[Tile]:
    return [replace(base, surface=replace(base.surface, cols=c, rows=r))
            for c, r in sizes]

tiles = repeat_sizes(tile, [(1, 1), (3, 3)])
```

The author still writes one tile, and explicitly opts in to the multi-
emit. The hidden mutation in `_sized_spec` goes away.

### Goals served

T, C

---

## M. Make the layer order rule for regions/boundaries explicit

### Problem

CLAUDE.md documents:

> Boundaries always run after every region… so grass blades growing
> into a boundary strip will plow through its rocks — documented
> behaviour: put rocks on grass, you get rocks on grass.

That's the orchestrator's fixed two-phase loop. It's a rule the author
has to know but can't see in the spec.

### Proposal

Run regions and boundaries in *spec order*, not always-regions-first.
Each `Region` / `Boundary` is just an "area." Author writes:

```python
tile = Tile(
    areas=[
        Region(id='pool', ...),          # runs 1st
        Boundary(id='shore', ...),       # runs 2nd
        Region(id='meadow', ...),        # runs 3rd → its grass steers
                                         # around the shore's rocks
    ],
)
```

The current "regions before boundaries" is a sensible default but
shouldn't be the only option. With this change, the rocks-on-grass
caveat goes away — you put the rock-bearing area *before* the
grass-bearing one.

### Goals served

P, T

### Cost

`Tile.regions` and `Tile.boundaries` become a single `areas` list. We
could also keep both as inputs and just process them in
"interleave by declaration order" using insertion order or an
explicit `priority` field.

---

## N. Per-layer seeds out of XOR-constant magic

### Problem

```python
# soil.py
rng = np.random.default_rng(seed ^ 0xC01D_50_11)
# grass_carpet.py
rng = np.random.default_rng(surface.seed ^ 0x554E_4445)   # "UNDE"
# rocks via scatter
rng_seed = surface.seed ^ 0x726F636B ^ self.scatter_cfg.seed ^ (layer_idx * 65537)
# grass via scatter
seed = surface.seed ^ 0x47524F57 ^ self.scatter_cfg.seed ^ (layer_idx * 65537)
```

The magic constants ensure independent RNG streams per layer. The author
who wants to reproduce a tile *exactly* — say, debug "why does this
soil clump look weird?" — has no clean way to vary just one layer's
seed.

### Proposal

Move seed derivation into a single `scene.derive_seed(label, layer_idx)`
helper that hashes deterministically:

```python
def derive_seed(self, label: str, layer_idx: int = 0) -> int:
    h = hashlib.blake2s(digest_size=4)
    h.update(self.surface.seed.to_bytes(4, 'big'))
    h.update(label.encode())
    h.update(layer_idx.to_bytes(4, 'big'))
    return int.from_bytes(h.digest(), 'big')
```

Layers call `scene.derive_seed('soil-blobs')`, `scene.derive_seed('grass-
carpet-noise')`. Magic constants gone. Authors can also override per-layer
seeds via an optional `seed=` kwarg on layers (already supported in some
places via `ScatterConfig.seed`, but not uniformly).

### Goals served

T, P, C

---

## O. Clean up vestigial code

A short list of things to delete now that the spec architecture has
settled:

- `TileScene.from_config` and `_make_sinusoidal_terrain` — only used by
  legacy entry points that no longer exist in the production pipeline.
- `surface.flat_terrain` — boolean only consulted by the dead
  `from_config` path.
- `TileScene.rock_placement_mask` — declared, never set or read.
- `SoilConfig.blob_h_min` / `blob_h_max` and `small_h_min` / `small_h_max`
  — referenced only on the `perturb=False` branch of `_accumulate_blob`,
  which the docstring describes as "unused in default pipeline."
- `GrassUnderlayConfig.stamp_min_taper` — present but no caller reads it
  in `grass_carpet.py`. (Worth a closer check; might be live.)

### Goals served

S, T

---

## P. Rename for symmetry: `*CarpetLayer` → `*Layer`

### Problem

The public layer names are slightly asymmetric:

| Class | Affects |
|---|---|
| `SoilCarpetLayer` | terrain texture |
| `GrassCarpetLayer` | terrain texture + flat blade meshes |
| `WaterLayer` | water volume mesh + terrain reshape |
| `ScatterLayer` | container of `Rocks` / `Grass` things |

The "Carpet" suffix exists to disambiguate the *texturing* grass layer
from the *3D* `Grass` thing. But the disambiguation already happens via
the namespace: `layers.GrassCarpetLayer` vs `scatter.Grass`. And no
similar suffix exists for soil (there is no 3D `Soil` thing).

### Proposal

Drop the `Carpet` suffix:

```python
from dharmatiles.layers import Soil, Grass as GrassCarpet, Water, Scatter
from dharmatiles.scatter import Rocks, Grass
```

Hmm — now `Grass` collides. Pick one of:

- Layers package owns `Soil`, `Water`, `Scatter`, plus `GrassTexture`
  (no "Carpet" but no collision).
- Or, keep `Carpet` only on `GrassCarpet` and rename `SoilCarpetLayer` →
  `Soil`.

Either way: drop `Layer` suffix (every public class in `layers/` is a
layer; the suffix is noise once you import from `dharmatiles.layers`).

### Goals served

C

### Cost

Mechanical sweep of `src/tiles/*.tile.py` + CLAUDE.md.

---

## Q. Make the spec loader transparent

### Problem

`load_spec()` uses `exec()` on the file contents into a fresh namespace.
That works but:

- The file isn't loaded as a Python module, so it doesn't appear in
  `sys.modules` and can't be re-imported by helpers.
- Spec files that try `from . import shared_helpers` get an ImportError
  because there is no `__package__`.
- Stack traces don't show the spec filename in a `Module` line.

### Proposal

Load specs via `importlib.util.spec_from_file_location` so they're
real modules. Documented, transparent, and unlocks "shared helpers in
`src/tiles/_lib/`" for power users.

### Goals served

P, T

---

## Suggested rollout order

If most of the above is acceptable, a sensible sequence:

1. **C** (drop the grass_mask save/restore dance), **G** (drop
   `SceneConfig`), **O** (cleanup) — pure refactors, no API change.
2. **A** (kill the kwarg-splitter), **B** (un-magic carpet scaling),
   **N** (named seed derivation), **P** (rename for symmetry) — small
   spec-author-visible churn, big clarity win.
3. **F** (move `rocks_per_square` out of geometry), **E** (decompose
   placement strategies), **J** (typed anchors), **K** (region
   selectors) — moderate API expansion.
4. **D** (centralise scene invariants), **M** (interleaved area order),
   **L** (move multi-size out of `Tile.sizes`) — structural moves.
5. **H** (systems-as-spec), **I** (terrain as a layer), **Q** (importlib
   loader) — the powerful and expensive ones.

Items 1–2 alone would meaningfully reduce magic without expanding the
surface area at all.
