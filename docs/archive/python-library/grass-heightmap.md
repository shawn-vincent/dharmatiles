# Grass Heightmap Design

## Purpose

Procedurally generate grayscale heightmap PNGs for use with OpenSCAD's `surface()` to emboss a grass/foliage texture into the top face of DungeonBlocks floor tiles.

Script: `scripts/generate-grass-heightmap.py`

---

## Tile Context

- DungeonBlocks tile footprint: 35×35mm
- Heightmap maps directly to the tile surface via `resize([total_w, total_d, texture_depth])` in OpenSCAD
- Output pixel count should be ≤256×256 to avoid OpenSCAD mesh performance issues
- `texture_depth` controls the physical emboss depth in mm (default 2mm for grass)
- Floor preset "Ground" (6mm) and peg height "Short" used for grass tiles

---

## Four-Layer Architecture

### Layer 1 — Ground (base)
- Low-frequency simplex noise
- Creates soil/dirt texture between blades
- Low height fraction (~25%) so blades read clearly above it
- Composited additively under the foreground layer

### Layer 2 — Edge (bottom foreground)
- Fills the perimeter strip (~38px at SIZE=512) with short inward-pointing blades and small weed leaves
- Perimeter walk: clockwise — top L→R, right T→B, bottom R→L, left B→T
- ~26px spacing between elements so they are non-overlapping at the base
- Blade direction: exactly inward ± gauss(0, 0.35 rad) variation
- Blade length: 28–65px (shorter than interior blades); base width 9px
- ~22% of slots become small leaves (r 11–24px) instead of blades
- Tip-outside-canvas check discards blades near corners whose angular variation would push the tip off-canvas (natural sparse corners)
- Composited via `np.maximum`

### Layer 3 — Blades (mid)
- Clumps of individual grass blades radiating from clump centers
- Each blade is a quadratic bezier curve with:
  - **Lateral curve**: control point offset perpendicular to blade direction (natural droop/lean)
  - **Tapered width**: `w(t) = base_w * (1 - t)^0.7` — wider at base, tapers to tip
  - **Ridge cross-section**: `cos(π/2 * dist/w)` across width — bright central spine, darker edges
  - **Sinusoidal height along length**: `sin(π * t^0.55) * (1 + amp * sin(freq * 2π * t + phase))` — blade rises from base, undulates, droops at tip
- Interior containment check (all bezier points must be within `[bw, S-bw]`) still applies
- Composited via `np.maximum`

### Layer 4 — Leaves (topmost)
- Scattered lobed ellipses representing broad weed/clover leaves
- Ellipse aspect ratio ~2.5:1 (elongated)
- Edge shape: sinusoidal lobe modulation `r(θ) = r_base * (1 + amp * sin(n*θ))`
- Height falloff: `(1 - r_norm)^0.7` — bright center, fades to zero at edge
- On top of blades so they read as overlying the grass
- Composited via `np.maximum`

---

## Compositing

```
foreground = max(edge, blades, leaves)   # no additive brightening at overlaps
composite  = ground + foreground         # ground shows through gaps
normalize to [0, 255]
```

Using `max()` for all foreground layers prevents stacked elements from creating unnaturally bright hotspots.

---

## Blur

- **Blades**: 0.0 — no blur, keep crisp ridge definition
- **Leaves**: 0.4px — minimal softening only
- Significant blur was tried and rejected: it merges blade edges and loses individual definition

---

## Placement Strategy

- **Clumps and leaves**: jittered grid (divide canvas into cells, one item per cell with random jitter within cell)
  - Prevents large dark voids and bright dense clusters
  - Looks random but distributes evenly
- **Blades**: radiate from clump center with gaussian spread + random direction bias
  - `blade_dir = atan2(p0-clump) + gauss(0, 0.6)` — mostly outward, some inward variation

---

## Containment (No Clipping)

All shapes must be entirely within the canvas — no partial shapes at edges.

- **Blades**: after generating bezier points, check all points are within `[base_w, S-base_w]` in both axes. Skip any blade that fails.
- **Leaves**: center must be at least `r_max * 1.7 + 2` px from each edge.

**Known issue**: skipping out-of-bounds blades creates a dark border because edge clumps produce few surviving blades. Options under consideration:
  - Dense short inward-pointing border blades
  - Allow partial blades at corners only (accept corner clipping as natural)
  - Separate pass of edge-hugging inward blades

---

## Key Parameters (at SIZE=512)

| Parameter | Value | Notes |
|---|---|---|
| `BLADE_LENGTH_MIN/MAX` | 80–170px | ~5.5–11.5mm on tile |
| `BLADE_BASE_W` | 14px | ~1mm on tile |
| `BLADE_UNDULATE` | 0.25 | height ripple amplitude |
| `CLUMP_COUNT` | 55 | jittered grid |
| `BLADES_PER_CLUMP` | 14 | |
| `CLUMP_RADIUS` | 20px | gaussian spread |
| `LEAF_COUNT` | 90 | jittered grid |
| `LEAF_R_MIN/MAX` | 22–55px | |
| `GROUND_HEIGHT` | 0.25 | fraction of range |
| `EDGE_STRIP_W` | 38px | depth of perimeter fill zone |
| `EDGE_SPACING` | 26px | spacing between edge elements (non-overlapping at base) |
| `EDGE_BLADE_LEN_MIN/MAX` | 28–65px | shorter than interior blades |
| `EDGE_BLADE_BASE_W` | 9px | narrower than interior blades |
| `EDGE_LEAF_PROB` | 0.22 | fraction of edge slots that become leaves |

---

## Open Issues

1. ~~**Dark border**~~ — resolved by the Edge fill layer (Layer 2).
2. **Density balance** — center tends to be brighter than edges due to blade overlap. May need per-region density control.
3. **Seed reproducibility** — each seed gives a unique but consistent result. The 9-variant pipeline uses random seeds per tile.
