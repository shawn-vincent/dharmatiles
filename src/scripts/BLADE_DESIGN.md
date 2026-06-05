# Grass Blade Renderer — Design Document

## What We're Rendering

A single grass blade as a 2D image:
- **Base cap:** bulbous semi-ellipse at bottom, tangent to body sides
- **Body:** constant-width parallel sides
- **Tip:** cosine-tapered point, tangent to sides, fixed arc-length
- **Curve:** the spine follows a circular arc over a configurable fraction of the blade
- **Shading:** bright center ridge, dark edges — models cross-section illumination
- **Base fade:** blade emerges from black ground
- **Tip fade:** slightly dimmed near the point

---

## Root Cause of Current Problems

The renderer uses a **row-by-row scanline model**: for each image row y, compute
`center_x(y)` and `hw_eff(y)` (horizontal half-width), then shade based on
horizontal distance from `center_x`.

This model has a fundamental assumption: **the spine is roughly vertical**.

When the spine tilts toward horizontal, two things go wrong:

1. **Tilt correction blows up.** Horizontal projected width = physical_width / cos(tilt).
   At 90° tilt, cos(tilt) = 0 → the correction factor hits the 1000× clamp → the
   tip visually "spreads" into a flat bar instead of a point.

2. **Arc endpoint doesn't reach the body.** With `theta_max = 90°` and
   `R = curve_span / theta_max`, the arc only drops `R·sin(90°) = R` pixels
   vertically — but `y_body` can be *further up* than that. At `curve=1` in the
   current grid, the body starts at y=140 but the arc ends at y=162. The code
   clips `theta_body` to π/2 and computes a negative tip extent. **Geometry is
   completely broken.**

3. **Gradients use vertical distance.** The tip fade is computed from `ys - y_apex`
   (image-vertical). When the tip arc goes mostly sideways, it occupies very few
   image rows, so the fade snaps rather than transitioning smoothly.

The scanline model is fine for gentle curves (≤ ~50°), but breaks at extreme angles.

---

## The Correct Mental Model

A grass blade is a **flat ribbon** (like a leaf) lying in the image plane. Its spine
curves within that plane. The physical width at any spine point is always `width(s)`,
and it's always oriented **perpendicular to the spine** at that point.

All measurements of "distance across the blade" should be in the spine-normal
direction, not the image-x direction.

---

## Implementation Options

---

### Option A — Scanline, Cap Max Angle (Quick Fix)

**What:** Keep the current scanline approach but cap `theta_max` at ~50°. At 50°,
`cos_t_min ≈ 0.64`, so `hw_eff_factor ≤ 1.56×` — manageable. Remap `curve = ±1` to
±50° and restore the original `R = curve_span / sin(theta_max)` formula (not constant
arc-length, just same vertical height for all curves).

**Pros:**
- Minimal code change (~3 lines)
- Fast (same O(bbox) performance)
- No new dependencies
- Shading artifacts are small at ≤50° tilt

**Cons:**
- Curves look gentle even at `curve=±1` — heavy drooping is impossible
- The scanline shading is still *slightly* wrong at 50° (measuring horizontally instead
  of perpendicularly), but visually unnoticeable
- Doesn't solve the fundamental model mismatch

---

### Option B — Scanline with Perpendicular-Distance Shading

**What:** Keep scanline for the *silhouette* (center_x, hw_eff per row), but for
*shading*, measure distance perpendicular to the spine rather than horizontally.

For each row, the spine tangent is `d(center_x)/dy` in image space. The perpendicular
direction `n = normalize(1, -d center_x/dy)`. Shade by the component of `(px - cx)`
along `n`.

This fixes the shading orientation but NOT the arc-geometry breakdown at extreme angles.

**Pros:**
- Correct shading at all angles
- Still O(bbox) fast
- Moderate code change

**Cons:**
- The arc endpoint geometry still breaks at extreme curves (same problem as now)
- Requires capping curve to angles where the arc geometry stays valid (≤ ~70° or so)
- Combining with constant-arc-length is still tricky

---

### Option C — KDTree Nearest-Spine-Point  *(Recommended)*

**What:** Precompute N dense spine samples with known arc-length `t`, spine tangents,
normal vectors, and radius profile. For each pixel in the bounding box, find the
nearest spine point (via KDTree), then:
- `d_perp` = offset in spine-normal direction → drives alpha and shading
- `t_nearest` → drives base/tip fades and radius

```
For each pixel (px, py) in bbox:
    i = nearest_spine_index(px, py)          # O(log N) via KDTree
    n = normal[i]                             # perpendicular to spine
    d_perp = dot((px, py) - spine[i], n)
    r = radius_profile[t[i]]                  # includes tip taper
    alpha = softclip(r - |d_perp| + 0.5)
    shade = (1 - (d_perp/r)^2)^power
    brightness = shade * alpha * base_fade(t[i]) * tip_fade(t[i])
```

**Pros:**
- Mathematically correct for **any** spine shape — no angle limits, no blow-up
- Shading always perpendicular to spine; tip looks pointy from any angle
- Base/tip fades work in arc-length space, always smooth
- No tilt correction needed — it's implicit and exact
- Handles 90°, 180°, banana loops, anything
- The spine shape can be anything (Bézier, arc, free-form) — no special-casing
- Same pattern used successfully in `generate-grass-blade-heightmap.py`

**Cons:**
- O(W×H) per blade — no easy bounding-box speedup for the inner loop
  *(At 200×380 = 76 000 pixels, cKDTree query is ~10–20 ms — fine for a utility)*
- Needs scipy (already a project dependency)
- ~30–50 more lines of code

---

### Option D — Spine Splatting (Painter's Algorithm)

**What:** Step along the spine in arc-length steps of ~0.5 px. At each step, "stamp"
a short perpendicular line segment with anti-aliased coverage and shading, accumulating
into the canvas.

**Pros:**
- No KDTree needed (pure numpy)
- Correct orientation at all angles

**Cons:**
- Double-coverage on inside of sharp curves → visible brightening artifacts
- Under-coverage on outside of tight curves → visible gaps
- Needs careful normalization per pixel which is expensive or approximate
- Much harder to get right than Option C

---

## Recommendation

**Use Option C (KDTree).**

The KDTree approach is the canonical solution to this class of problem. It's the same
model as `generate-grass-blade-heightmap.py`, just adapted to 2D alpha-rendering
instead of 3D heightmap. The result will be correct for any curve value, the tip will
always taper cleanly, the fades will always be smooth, and there's nothing to break as
parameters change.

The performance cost (10–20 ms per blade in a utility script) is irrelevant here.

If you need faster rendering as a primitive in a larger system, Option A + capping at
50° is a pragmatic quick fix that would look fine for moderate grass curves.

---

## Proposed KDTree Implementation Sketch

```python
import numpy as np
from scipy.spatial import cKDTree

def render_blade_kdtree(canvas_w, canvas_h, width, length, ...):
    # 1. Build spine: circular arc from base through body to tip
    #    Parameterize by arc-length t ∈ [0, 1]
    N = 2000  # spine samples
    t = np.linspace(0, 1, N)
    
    # spine_xy[i] = (x, y) position of spine at arc-length t[i]
    # radius[i]   = half-width profile at t[i] (body=a, tip taper, base taper)
    # normal[i]   = unit vector perpendicular to spine at t[i]
    
    # 2. KDTree
    tree = cKDTree(spine_xy)
    
    # 3. Query all pixels in bounding box
    ys, xs = np.mgrid[y0:y1, x0:x1].astype(float)
    pixels = np.column_stack([xs.ravel(), ys.ravel()])
    _, idx = tree.query(pixels)
    
    # 4. Per-pixel values
    d_perp = (pixels - spine_xy[idx]) @ normals_indexed  # perpendicular offset
    r = radius[idx]
    alpha = np.clip(r - np.abs(d_perp) + 0.5, 0, 1)
    shade = (1 - np.clip(d_perp/r, -1, 1)**2) ** power
    fade = base_fade(t[idx]) * tip_fade(t[idx])
    
    brightness = shade * alpha * fade
    canvas[y0:y1, x0:x1] = (brightness.reshape(...) * 255).astype(np.uint8)
```

The spine geometry (arc computation, radius profile, fades) is largely the same
as the current code — only the per-pixel measurement changes.
