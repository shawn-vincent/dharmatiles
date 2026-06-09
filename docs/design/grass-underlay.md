# Grass Underlay Layer

## What It Is

A flat embossed surface texture that lives **under the 3D grass blades**.  It
modifies `terrain_z` (like `SoilLayer`) rather than generating freestanding
geometry.  The result is a compressed-grass carpet: short bumps and blade-shaped
ridges pressed into the ground, visible in gaps between the upright 3D blades.

Layer type string: `grass_underlay`

---

## Pipeline Position

```
SoilLayer           → modifies terrain_z  (soil region only)
GrassUnderlayLayer  → modifies terrain_z  (grass region only)  ◄ new
terrain_support_z sync
StonesLayer         → adds stone geometry + stone_mask
GrassLayer (3D)     → grows upright blades on top
```

It runs after soil (so the soil texture is the base it sits on top of) and before
stones and 3D grass (so both can sit on the already-textured surface).

`terrain_support_z` is synced **after** both soil and grass underlay, so stones
and 3D grass correctly see the fully textured surface height.

---

## Layer Separation

| Region  | Layer(s)                        | What modifies `terrain_z` |
|---------|----------------------------------|---------------------------|
| `grass` | `grass_underlay` + `grass` (3D) | GrassUnderlayLayer        |
| `soil`  | `soil`                          | SoilLayer                 |

Soil does NOT bleed into the grass region: `_collect_soil_layers` already
attaches a `placement_mask = (region_mask == idx)` to every soil layer, so each
layer is confined to its declared region.  The grass underlay is symmetric —
it gets the grass-region mask at collection time.

---

## Algorithm

### 1. Base noise

Gaussian-filtered white noise, clipped to ≥ 0, giving a smooth random bumpy
background that resembles compressed, matted grass seen from above.

```
sigma  = noise_scale_mm / cell_w     (correlation length in cells)
noise  = gaussian_filter(rng.standard_normal((gh, gw)), sigma)
noise  = clip(noise / noise.std() * noise_amp, 0, ∞)
field  = max(field, noise)
```

Default: `noise_amp = 0.20 mm`, `noise_scale_mm = 3.0 mm`.

### 2. Blade stamp footprints

Seeds are planted using the same Voronoi-group seeding logic as the 3D grass
layer (`plant_seeds` from `grass.grow`), with the same `SpeciesConfig`
parameters (groups_per_square, gap_mm, blade width/length distributions,
curl).  This draws seeds from the same spatial distribution as the 3D blades,
so the flat stamps are visually coherent with the blades that grow on top.

For each seed, the blade trajectory is traced in 2D (same curl / taper as
the 3D seed) and a cross-section stamp is rasterised at each step:

```
for step in 0 … blade_n_steps:
    taper   = seed.distance_taper(step × segment_len, total_len)
    width   = seed.blade_width × taper
    peak_h  = blade_thickness × stamp_height_scale × taper

    for each cell in bounding box of this cross-section:
        lat     = signed lateral distance from blade axis
        along   = signed along-blade distance from step centre
        if |lat| ≤ width/2  and  |along| ≤ segment_len/2 + cell_w:
            h = peak_h × sin(π × (lat/half_width + 1) / 2)
            field = max(field, h)

    advance (x, y, direction) by one segment
```

Profile shape: `sin(π × x)` for x ∈ [0, 1] across the blade width — zero at
both edges, peak at centre.  This matches the top-profile cross-section used
in the 3D blade mesh (`blade_top_facets = 1` case → flat; `= N` → arced;
here always arced regardless of blade_top_facets).

`stamp_height_scale` controls how tall the pressed-flat silhouette is relative
to the 3D blade thickness.  Default 0.40 (40 % of blade_thickness).

### 3. Apply with placement mask

```python
if placement_mask is None:
    scene.terrain_z += field
else:
    scene.terrain_z[placement_mask] += field[placement_mask]
```

---

## Configuration (`GrassUnderlayConfig`)

```python
@dataclass
class GrassUnderlayConfig:
    # ── Noise base ────────────────────────────────────────────────────────────
    noise_amp:       float = 0.20   # mm — peak amplitude of background noise
    noise_scale_mm:  float = 3.0    # mm — Gaussian blur sigma (feature scale)

    # ── Blade stamp geometry (mirror of SpeciesConfig fields used in stamping)
    blade_width_min: float = 1.2
    blade_width_max: float = 1.2
    blade_length_min: float = 10.0
    blade_length_max: float = 10.0
    blade_segment_length: float = 0.5
    blade_taper:      float = 1.0
    blade_base_width: float = 1.0
    blade_base_taper: float = 0.0
    blade_curl_min:   float = 0.2
    blade_curl_max:   float = 0.45
    blade_thickness:  float = 0.6   # used as stamp peak before scaling

    # ── Stamp-specific ────────────────────────────────────────────────────────
    stamp_height_scale: float = 0.40  # fraction of blade_thickness → stamp peak

    # ── Placement ─────────────────────────────────────────────────────────────
    groups_per_square: int = 3     # match companion grass layer for visual coherence
    gap_mm:            float = 0.3
```

---

## Tile Spec Format

```yaml
regions:
  meadow:
    contains: [0.25, 0.5]
    layers:
      - type: grass_underlay     # ← embossed texture (runs first, modifies terrain_z)
        groups_per_square: 240   # match the companion grass layer
        noise_amp: 0.20
        stamp_height_scale: 0.40
      - type: grass              # ← 3D upright blades (runs after)
        groups_per_square: 240

  dirt:
    contains: [0.75, 0.5]
    layers:
      - type: soil               # ← soil blobs, masked to this region only
```

`groups_per_square` controls stamp density.  Matching it to the companion
`grass` layer means roughly the same population of blade footprints gets
pressed flat, then the same seeds sprout upright on top.

---

## Implementation Files

| File | Role |
|---|---|
| `core/config.py` | Add `GrassUnderlayConfig` |
| `layers/grass_underlay.py` | `GrassUnderlayLayer` — build() + stamp helpers |
| `terrains/tile.py` | `_collect_grass_underlay_layers()` + pass in `_build_mesh()` |
| `src/tiles/soil+grass.tile` | Add `grass_underlay` to `meadow` region |

---

## Design Notes

- **No 3-D mesh output.** The layer only modifies `terrain_z`; it adds no
  trimesh objects to `parts`.  The embossed texture becomes part of the
  terrain solid.

- **Seeding vs. exact correspondence.** `plant_seeds` is called with the same
  RNG seed base as the 3D grass layer (`surface.seed ^ 0x554E4445`).  The
  seeding algorithm (Voronoi groups + jitter grid) is stochastic and the two
  layers' RNG streams are independent, so the flat stamps and the upright blades
  are drawn from the same distribution but are not pixel-perfect coincident.
  This is intentional — exact coincidence would require a shared planting step
  that adds pipeline coupling, and the visual result is the same.

- **`blade_top_facets` is not used.** The stamp profile is always the sin(π x)
  arc regardless of the 3D blade's facet count.  The embossed shape is a smooth
  ridge across each blade width; a faceted stamp would not read at print scale.

- **Performance.** 240 groups/square at ~14 blades/group = ~3 360 blades,
  each 10 mm / 0.5 mm step = 20 stamps.  Each stamp touches a bounding box of
  ≈ (hw + 1 cell) × 2 = ~10 × 5 cells.  Total rasterisation: ~3 M cell ops,
  vectorised with NumPy per step.  Expected wall-clock: < 1 s.
