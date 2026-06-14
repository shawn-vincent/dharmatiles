# Design Document: Bark Surface Pipeline

**Date:** 2026-06-14  
**Status:** Implemented for `ScaTree` — describes `trees/surface.py` as built  
**Scope:** Cross-section geometry, surface texture, frame propagation, mesh construction  
**Used by:** `ScaTree` (`scatter/sca_tree.py`); intended for `ConstTree` and future tree variants

---

## 1  Overview

The bark surface pipeline converts a **skeleton** (a directed tree of 3D nodes with
assigned radii) into a **trimesh**. It is independent of how the skeleton was grown —
SCA, constructive, or any future algorithm all produce the same `(nodes_xyz, parents,
radii, arc_dists)` arrays and pass them to the same builder.

```
skeleton arrays
  nodes_xyz   (N, 3)   — world positions
  parents     (N,)     — parent index; -1 for root
  radii       (N,)     — radius at each node (from radii.py)
  arc_dists   (N,)     — cumulative arc length from root to each node
        │
        ▼
  compute_frames(nodes_xyz, parents)
        │   parallel-transport normals, bisector tangents
        ▼
  build_tree_mesh(nodes_xyz, parents, radii, arc_dists, cfg, rng, tz, crown_base_z)
        │
        ├── one ring shape per node (elliptical, bark ridges, wrinkles, flare)
        ├── quad strips between adjacent rings (per edge)
        ├── base cap (flat disk sunk below terrain level)
        └── top fans at effective leaf nodes
        │
        ▼
  trimesh.Trimesh
```

**Configuration:** `BarkConfig` (in `core/config.py`) — all bark parameters live there.
`ScaTreeConfig` embeds a `bark: BarkConfig` field and forwards bark parameters to it
at construction time. Future tree configs should follow the same pattern.

---

## 2  Scale-Adaptive Detail

Surface detail is gated on the local node radius so fine twigs don't waste vertices
on invisible features:

| Node radius | Surface treatment |
|-------------|------------------|
| `r ≥ ridge_min_r_mm` | Full bark: elliptical cross-section + axial ridges + horizontal wrinkles + root flare |
| `r ≥ branch_min_r_mm` | Plain swept ring only (ridges below FDM resolution) |
| `r < branch_min_r_mm` | Node and all its edges skipped entirely |

Default thresholds: `ridge_min_r_mm = 1.2 mm`, `branch_min_r_mm = 0.20 mm`.

---

## 3  Cross-Section Ring

At each active node, one ring of `az_segs` vertices is built in the plane
perpendicular to the node's bisector tangent (see §5).

### 3.1  Elliptical cross-section

Each ring is scaled by a random aspect ratio `aspect ∈ [0.75, 1.0]` (sampled once per
tree) with a slow angular twist:

```
theta_i    = 2π * i / az_segs  +  twist_rate * arc_dist   (i = 0..az_segs-1)
r_i_base   = r_node * (1 if cos(theta_i) ≥ 0 else aspect)
```

The twist accumulates with arc distance from root, so the oval cross-section rotates
slowly from trunk to tips without any kink at junctions.

### 3.2  Axial bark ridges

Applied when `r ≥ ridge_min_r_mm`. Ridges run along the length of the trunk and
branches. Modelled as a sum of angular harmonics:

```
r_ridge(θ) = r_base(θ) * (1 + ridge_amp * Σ_{k=2}^{k+ridge_harmonics} A_k * cos(k*θ + φ_k))
```

- Harmonic amplitudes `A_k` fall off with `k` so low-order (coarse) ridges dominate.
- Phase offsets `φ_k` are random per tree (seeded from `rng`).
- **Phase drift with arc distance:** `φ_k` drifts slowly along the skeleton path
  so ridges run continuously from root to tip without phase jumps at bifurcations.
  The phase is indexed by **arc distance from root** (not global Z):

  ```
  φ_k(node) = φ_k_seed + arc_dist[node] * (2π / ridge_drift_mm)
  ```

  At a bifurcation, each child starts from its parent's `arc_dist` value, then
  advances independently — ridges diverge naturally at junctions, just as in real bark.

### 3.3  Fine bark grain

No separate per-vertex random grain layer is currently implemented. Fine-scale
surface variation comes from the higher bark harmonics plus the horizontal wrinkle
offset. A stochastic grain layer can be added later if printed bark reads too smooth.

### 3.4  Horizontal wrinkles

Applied when `r ≥ ridge_min_r_mm`. A low-amplitude sinusoidal Z offset models
growth rings and scar tissue:

```
z_noisy(node) = z(node) + wrinkle_amp * sin(2π * arc_dist[node] / wrinkle_period + φ_wrinkle)
```

`wrinkle_amp` and `wrinkle_period` are sampled once per tree from their distributions.
The independent variable is again arc distance (not global Z) for continuity across
junctions.

---

## 4  Root Flare

Rings within `flare_fraction * crown_base_z` of the terrain are widened to simulate
buttress roots and ground contact:

```
h = (z - terrain_z) / (flare_fraction * crown_base_z)
r_flare(h) = r_node * (1 + flare_amp * (1 - h) ^ flare_power)   for 0 <= h < 1
```

where `h` is normalized height through the flare zone: `0` at ground contact and
`1` at the top of the flare. The implementation applies this by node height, so any
low ring near the base receives the multiplier; ordinary branches are normally above
the flare zone and remain unchanged.

The multiplier blends smoothly back to `1.0` at the top of the flare zone.

---

## 5  Frame Propagation (Parallel-Transport Bisector)

Getting continuous bark ridges and stable ring orientation across the whole tree
requires propagating a coordinate frame from the root to every node.

### 5.1  Bisector tangent

Each node's canonical tangent **bisects** the incoming and outgoing edge directions:

- **Root node**: outgoing direction only.
- **Leaf node**: incoming direction only.
- **Internal node**: `normalize(t_in + mean(t_out_children))`.

For a symmetric Y-fork the bisector is the axis of the fork, so both child quad
strips are equally un-sheared. This is the "miter joint" approach: rings at junctions
split the bend angle equally.

### 5.2  Parallel-transport normal

A normal vector is propagated root-to-tips by parallel transport:

```
# At the root: seed an arbitrary normal ⊥ to the root direction
normal[0] = cross(root_dir, reference) / |...|

# At each child c with parent p, tangent t:
n = normal[p] - dot(normal[p], t) * t    # project parent normal onto child tangent plane
normal[c] = n / |n|
```

This ensures the angular orientation of the cross-section ring rotates smoothly
along any root-to-tip path. At bifurcations, both children inherit the same parent
normal independently — their rings will "open" naturally from the shared junction ring.

### 5.3  Why arc-distance, not Z

Using global Z for bark ridge phase fails at non-vertical branches: a nearly-horizontal
branch would show rapid ridge oscillation (many Z-cycles per mm of arc length) while
the trunk shows slow oscillation. Arc distance from root gives consistent ridge spacing
regardless of branch angle.

---

## 6  Swept-Ring Mesh Construction

### 6.1  One ring shape per node

All ring positions are pre-computed before any edge strips are built. Rings are indexed
by node index, so an edge `(parent → child)` can directly look up both endpoint rings
and receive the same bark phase and cross-section orientation as every adjacent edge.

The current implementation duplicates those ring vertices into each per-edge mesh strip
before concatenation, so the mesh is not topologically welded at junctions. The duplicated
vertices are geometrically coincident and generated from the same ring, so there is no
visible phase jump or cross-section mismatch between trunk and branch.

### 6.2  Edge quad strips

For each edge `(p → c)` both nodes are active (radius ≥ `branch_min_r_mm`):

```
for i in range(az_segs):
    j = (i + 1) % az_segs
    emit quad: ring_p[i], ring_p[j], ring_c[j], ring_c[i]
    → split into 2 triangles
```

Vertex normals are implicit from the ring geometry; `trimesh` computes face normals
from the triangle winding order.

### 6.3  Base cap

The root node's ring is closed with a flat disk fan:

```
center = root_pos - [0, 0, sink_mm]   # sunk below terrain for watertight seal
for i in range(az_segs):
    emit triangle: center, ring_root[i], ring_root[(i+1) % az_segs]
```

`sink_mm` (default 0.15) ensures the base is below `terrain_z` so no gap appears
between tree base and terrain mesh when the slicer unions them.

### 6.4  Top fan

Each effective leaf node (an active node with no active children) is capped:

```
apex = leaf_pos                               # fan center at the node
for i in range(az_segs):
    emit triangle: apex, ring_leaf[i], ring_leaf[(i+1) % az_segs]
```

### 6.5  Junction geometry

At bifurcations the parent and child swept meshes meet on the same generated junction
ring. This is the implemented simple junction strategy: no trim surface, no CSG, and
no parent widening beyond the pipe-model radius. The geometry is appropriate for FDM
slicers, which will union coincident/overlapping shells in the printed output.

An alternative (**Option C**) would widen the parent tip ring to `sqrt(Σ r_child²)` so
children emerge flush. Deferred: the implemented simple junction is sufficient for
printed output.

### 6.6  Assembly

```python
parts = [base_cap, side_strip_0, side_strip_1, ..., leaf_cap_0, ...]
mesh = trimesh.util.concatenate(parts)
mesh.fix_normals()
```

---

## 7  BarkConfig Reference

```python
@dataclass(init=False)
class BarkConfig:
    """Bark and surface parameters.

    Shared by ScaTreeConfig and any future tree variant that uses trees/surface.py.
    """

    # ── Cross-section ─────────────────────────────────────────────────────────
    r_base_mm:         Sample[float] = D[2.5:4.5]    # root node radius (overrides pipe model)
    aspect:            Sample[float] = D[0.75:1.0]   # minor/major axis ratio of cross-section
    twist_rate:        float         = 0.018          # radians of twist per mm of arc distance
    az_segs:           int           = 16             # azimuth vertices per ring

    # ── Axial bark ridges ─────────────────────────────────────────────────────
    ridge_harmonics:   int           = 5              # number of angular harmonics (k = 2..k+1)
    ridge_amp:         float         = 0.10           # amplitude fraction of local radius
    ridge_drift_mm:    float         = 60.0           # arc-distance period of phase drift

    # ── Horizontal wrinkles ───────────────────────────────────────────────────
    wrinkle_amp:       Sample[float] = D[0.25:0.55]  # mm amplitude
    wrinkle_period:    Sample[float] = D[4.0:8.0]    # mm period

    # ── Root flare ────────────────────────────────────────────────────────────
    flare_amp:         float         = 0.55           # boost factor at ground (fraction of r_base)
    flare_fraction:    float         = 0.22           # fraction of trunk height covered by flare
    flare_power:       float         = 2.5            # sharpness of flare taper

    # ── Scale-adaptive detail thresholds ─────────────────────────────────────
    ridge_min_r_mm:    float         = 1.2            # full bark detail below this radius: off
    branch_min_r_mm:   float         = 0.20           # skip segment entirely below this radius

    # ── Radius assignment ─────────────────────────────────────────────────────
    branch_r_tip_mm:   float         = 0.30           # leaf-node starting radius for da Vinci pass

    # ── Base seal ─────────────────────────────────────────────────────────────
    sink:              float         = 0.15           # mm below terrain_z for watertight base cap
```

---

## 8  Module Layout

```
src/dharmatiles/
  core/
    config.py         BarkConfig dataclass
  trees/
    surface.py        compute_frames(), build_tree_mesh()
    radii.py          assign_radii(parents, r_tip_mm, r_root_mm)
```

`radii.py` is logically part of the bark pipeline: it converts the skeleton topology
into per-node radii that `surface.py` consumes. The current implementation uses the
classic square-law pipe model; `ConstTree` may extend this with a configurable
`pipe_model_exp`.

---

## 9  Key References

- Runions et al., [*Modeling Trees with a Space Colonization Algorithm*](https://algorithmicbotany.org/papers/colonization.egwnp2007.large.pdf) (2007) — parallel-transport frame derivation, §4
- Weber & Penn, [*Creation and Rendering of Realistic Trees*](https://courses.cs.duke.edu/cps124/fall01/resources/p119-weber.pdf) (1995) — taper formula, bark undulation parameters
- Eloy / PLOS ONE, [*Tree Branching: Leonardo da Vinci's Rule versus Biomechanical Models*](https://journals.plosone.org/plosone/article?id=10.1371/journal.pone.0093535) (2014) — pipe-model exponent
