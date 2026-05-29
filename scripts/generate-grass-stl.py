#!/usr/bin/env python3
"""
Generate a 3D grass-tile STL directly from Python using trimesh.

Geometry
────────
  • Flat rectangular base (box).
  • One cylinder-with-cosine-tip per blade, placed using the same jittered-
    grid positions as generate-grass-scene.py (same SEED).
  • All blades stand straight up — no lean or curve for this first experiment.
  • Every mesh is hand-triangulated for a guaranteed-watertight result.

Scale
─────
  • Virtual canvas: CANVAS_W × CANVAS_H px  (matching the 2-D scene).
  • Physical tile:  TILE_W   × TILE_H  mm.
  • 1 pixel = TILE_W / CANVAS_W mm.

Usage
─────
  python3 scripts/generate-grass-stl.py [output.stl]
"""

import numpy as np
import trimesh
import pathlib, sys, subprocess

# ── Config ────────────────────────────────────────────────────────────────────
CANVAS_W = CANVAS_H = 700          # must match generate-grass-scene.py
TILE_W   = TILE_H   = 35.0         # mm — DungeonBlocks 1×1 tile
BASE_H   =  3.0                    # mm — base slab height
N_BLADES =  62
N_FILL   = 150
SEED     =  42
CAP_RATIO = 0.6
N_SECTIONS = 14                    # polygon sides around each blade cylinder

OUTPUT = pathlib.Path("stl/grass-blades.stl")

SCALE = TILE_W / CANVAS_W          # px → mm

# ── Blade placement (identical to the 2-D scene generator) ───────────────────
rng = np.random.default_rng(SEED)

def place_blades(n, w_min, w_max, l_min, l_max, tl_min, tl_max):
    """Jittered-grid blade placement; returns list of dicts with px coords."""
    grid_cols = int(np.ceil(np.sqrt(n)))
    grid_rows = int(np.ceil(n / grid_cols))
    cell_w = CANVAS_W / grid_cols
    cell_h = CANVAS_H / grid_rows

    cells = [(c, r) for c in range(grid_cols) for r in range(grid_rows)]
    rng.shuffle(cells)

    blades = []
    for cell_c, cell_r in cells:
        if len(blades) >= n:
            break
        w  = rng.uniform(w_min,  w_max)
        L  = rng.uniform(l_min,  l_max)
        tl = rng.uniform(tl_min, tl_max)
        mx = (cell_c + rng.uniform(0, 1)) * cell_w
        my = (cell_r + rng.uniform(0, 1)) * cell_h
        blades.append(dict(base_x=mx, base_y=my, width=w, length=L, tip_length=tl))
    return blades

tall  = place_blades(N_BLADES, 17, 38, 100, 200, 28, 56)
fills = place_blades(N_FILL,   10, 24,  40,  90, 20, 40)
blades = tall + fills
print(f"Placed {len(blades)} blades total")

# ── Mesh builders ─────────────────────────────────────────────────────────────

def make_blade_mesh(radius_mm, body_h_mm, tip_h_mm, n=N_SECTIONS):
    """
    Watertight trimesh for one upright blade centred at (0, 0, 0):
      • flat circular base cap at z = 0
      • cylinder walls up to z = body_h_mm
      • cosine-tapered tip from z = body_h_mm  to  z = body_h_mm + tip_h_mm
    All face normals point outward (verified by fix_normals).
    """
    angles = np.linspace(0, 2 * np.pi, n, endpoint=False)

    # ── tip-ring profile: N_TIP levels from body top to apex ─────────────────
    N_TIP = 10
    t      = np.linspace(0, np.pi / 2, N_TIP + 1)   # 0 … π/2
    tip_rs = radius_mm * np.cos(t)                   # radius → 0
    tip_zs = tip_h_mm  * t / (np.pi / 2)             # 0 → tip_h_mm

    verts, faces = [], []

    def add_point(x, y, z):
        idx = len(verts); verts.append([x, y, z]); return idx

    def add_ring(r, z):
        idx = len(verts)
        for a in angles:
            verts.append([r * np.cos(a), r * np.sin(a), z])
        return idx

    def quad(a0, a1, b0, b1):
        """Two CCW triangles for a quad (outward = right-hand rule)."""
        faces.append([a0, b0, a1])
        faces.append([a1, b0, b1])

    # Vertices
    v_bot   = add_point(0, 0, 0)
    v_bring = add_ring(radius_mm, 0)
    v_tring = add_ring(radius_mm, body_h_mm)

    tip_rings = [v_tring]
    for j in range(1, N_TIP):
        tip_rings.append(add_ring(tip_rs[j], body_h_mm + tip_zs[j]))
    v_apex = add_point(0, 0, body_h_mm + tip_h_mm)

    # Bottom disk (normal → −z)
    for i in range(n):
        i1 = (i + 1) % n
        faces.append([v_bot, v_bring + i1, v_bring + i])  # CW from below = outward

    # Cylinder walls (normal → +r)
    for i in range(n):
        i1 = (i + 1) % n
        quad(v_bring + i, v_bring + i1, v_tring + i, v_tring + i1)

    # Tip rings
    for j in range(len(tip_rings) - 1):
        ra, rb = tip_rings[j], tip_rings[j + 1]
        for i in range(n):
            i1 = (i + 1) % n
            quad(ra + i, ra + i1, rb + i, rb + i1)

    # Apex fan (normal → outward from tip)
    last = tip_rings[-1]
    for i in range(n):
        i1 = (i + 1) % n
        faces.append([last + i, last + i1, v_apex])

    mesh = trimesh.Trimesh(
        vertices=np.array(verts, dtype=float),
        faces=np.array(faces,    dtype=int),
        process=False,
    )
    mesh.fix_normals()
    return mesh


def make_box_mesh(w, d, h):
    """Watertight box with one corner at origin, top face at z=h."""
    return trimesh.creation.box(extents=[w, d, h],
                                transform=trimesh.transformations.translation_matrix(
                                    [w / 2, d / 2, h / 2]))


# ── Assemble ──────────────────────────────────────────────────────────────────
parts = [make_box_mesh(TILE_W, TILE_H, BASE_H)]

for i, bl in enumerate(blades):
    r  = (bl['width']      / 2)   * SCALE
    bh =  bl['length']            * SCALE
    th =  bl['tip_length']        * SCALE

    blade_mesh = make_blade_mesh(r, bh, th)

    # Position: canvas x→tile x, canvas y→tile y, base of blade sits on top of box
    x_mm = bl['base_x'] * SCALE
    y_mm = bl['base_y'] * SCALE
    blade_mesh.apply_translation([x_mm, y_mm, BASE_H])

    parts.append(blade_mesh)
    if (i + 1) % 50 == 0:
        print(f"  built {i+1}/{len(blades)} blades…")

print("Concatenating meshes…")
scene = trimesh.util.concatenate(parts)
print(f"  vertices: {len(scene.vertices):,}   faces: {len(scene.faces):,}")
print(f"  watertight: {scene.is_watertight}")

OUTPUT.parent.mkdir(parents=True, exist_ok=True)
scene.export(str(OUTPUT))
print(f"Saved {OUTPUT}")

# ── Preview ───────────────────────────────────────────────────────────────────
subprocess.Popen(["open", str(OUTPUT)])
