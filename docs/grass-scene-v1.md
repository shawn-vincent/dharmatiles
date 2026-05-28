# Grass Scene v1 — Generation Notes

This documents the exact parameters and pipeline used to generate the
`grass_scene.png` / `stl/grass-scene.stl` at the point of the first
commit of `scripts/generate-grass-scene.py`.

---

## Pipeline

```
scripts/generate-grass-scene.py   →   grass_scene.png   (700 × 700 px greyscale)
scripts/prepare-heightmap.py      →   textures/grass-scene-faded.png   (256 × 256 px)
scad/dungeonblocks-blank.scad     →   stl/grass-scene.stl
```

### Step 1 — Scene render (`generate-grass-scene.py`)

Two-pass jittered-grid placement using `scripts/blade.py` as the blade renderer.
Painter's algorithm (back-to-front by `base_y`).

#### Canvas
| Parameter | Value |
|-----------|-------|
| `CANVAS_W / CANVAS_H` | 700 px (= 35 mm at 20 px/mm) |
| `SEED` | 42 |

#### Tall blades  (`N_BLADES = 125`)
| Parameter | Range | Notes |
|-----------|-------|-------|
| `width` | 14 – 32 px | |
| `length` | 100 – 200 px | |
| `tip_length` | 14 – 28 px | |
| `power` | 0.40 – 0.75 | cross-section falloff |
| `curve_start` | 0.08 – 0.22 | fraction of blade where bend begins |
| Grid | 12 × 11 cells over y ∈ [118, 700] | y range derived from `MIN_TOT` |

#### Fill blades  (`N_FILL = 300`)
Shorter blades that fill the ground layer between tall blade bases.

| Parameter | Range |
|-----------|-------|
| `width` | 8 – 20 px |
| `length` | 40 – 90 px |
| `tip_length` | 10 – 20 px |
| Grid | 18 × 17 cells over full y ∈ [~27, 700] |

#### Flow field — **swirl**
| Parameter | Value | Notes |
|-----------|-------|-------|
| `FLOW_TYPE` | `'swirl'` | CW rotation around canvas centre |
| `FLOW_STRENGTH` | 0.45 | 0 = pure vertical, 1 = full tangential |
| `DIR_SPREAD` | 8° | per-blade random jitter added on top of flow angle |

**Direction** at each blade base: `θ_flow(bx, by) + uniform(−8°, +8°)`

Flow vector for swirl: `(fx, fy) = (yn/r, −xn/r)` (CW tangential, image coords),
blended with straight-up `(0, −1)` at weight `(1 − FLOW_STRENGTH)` before
converting to angle: `θ = atan2(bfx/mag, −bfy/mag)`.

#### Curvature-constrained blade curve
The blade `curve` parameter is no longer random in sign — it is derived from
the local signed curvature of the flow streamline:

```
κ  =  ∇θ · f̂   (directional derivative of flow angle along flow direction)
     normalised to [−1, 1] via 95th-percentile of |κ|

cur  =  κ × uniform(0.3 × MAX_CURVE, MAX_CURVE)
```

`MAX_CURVE = 0.70`.  The sign of `cur` therefore matches whether the
streamline bends CW or CCW at that position; magnitude is random.
Near the swirl centre curvature is highest → blades curve most strongly.
Near the canvas edges curvature is near zero → blades are nearly straight.

#### Fitting / rejection
`fits()` rejects placements whose bounding box (exact blade geometry, no extra
margin) falls outside the canvas.  Each grid cell allows up to 50 attempts.

---

### Step 2 — Heightmap prep (`prepare-heightmap.py`)

```
python scripts/prepare-heightmap.py grass_scene.png textures/grass-scene-faded.png \
    1.0   \  # crop_fraction  — full image, no crop
    0.05  \  # fade_fraction  — 5 % edge fade ramp
    0.0   \  # fade_floor     — fade goes to black
    1.5   \  # blur_radius    — Gaussian blur px
    256      # output_size    — 256 × 256 px output
```

---

### Step 3 — STL export (`scad/dungeonblocks-blank.scad`)

| Parameter | Value |
|-----------|-------|
| `cols / rows` | 1 × 1 |
| `floor_height` | 9.5 mm (Manmade preset) |
| `texture_depth` | 1.5 mm |
| `texture_zoom` | 1.0 |
| `texture_file` | `../textures/grass-scene-faded.png` |
| `peg_height` | 11.4 mm (Normal) |

Output: `stl/grass-scene.stl` — 35 × 35 mm DungeonBlocks floor tile.
