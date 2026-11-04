#!/usr/bin/env python3
"""Parity harness, Python half (docs/analysis/rust-rewrite/plan.html, P0/P1).

Builds a lab at seeds 1–4 and, per seed, writes the STL, ``metrics.json``
and painted renders with overhang faces red (owner policy: every render).
Then writes one contact sheet: rows = seeds, columns = views.

Metrics are computed from the STL alone (``stl_metrics``), so the Rust half
can run the very same function on its own STL.  The lab's internal counts
go beside them under ``"lab"``, as a check on the STL estimators.

Usage::

    python tests/parity/run_lab.py                 # dirt, seeds 1 2 3 4
    python tests/parity/run_lab.py --seeds 1 --out /tmp/x
    python tests/parity/run_lab.py --look flat --beside out/parity/flat/rust
    python tests/parity/run_lab.py --look dirt-stones   # dirtlab as shipped, for reference

Looks: ``dirt`` is dirtlab with its stones off (stones are Phase 3, so the
Rust dirt has none); ``dirt-stones`` is dirtlab as approved, with stones;
``flat`` is the P1 flat 1×1.

``--beside DIR`` puts the Rust build of the same seeds next to each Python
view: DIR/s<seed>/ holds the Rust STL and its four ``--png`` views (as
``cargo xtask parity <look>`` writes them).  Rust metrics come from the
same ``stl_metrics`` run on the Rust STL, with the Rust build's own edge
level (``edge_mm`` in its build record).  The Rust ``"stats"`` go under
``"lab"`` in rust-metrics.json, and ``parity.json`` gives the verdict over
all seeds (``PARITY_BY_LOOK``).

Output: ``out/parity/<look>/`` (gitignored) unless ``--out`` is given.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys
import time

import numpy as np
import trimesh
from PIL import Image, ImageDraw, ImageFont
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parents[2]
for p in ("src", "dirtlab", "clifflab", "rocklab", "waterlab", "grasslab"):
    sys.path.insert(0, str(ROOT / p))

GRID_MM = 0.15                     # top-surface sampling (dirtlab's CELL_MM)
OVERHANG_NORMAL_DEG = 55.0 + 3.0   # the plain normal test, beside the layer audit
RED = (230, 30, 30)
ORANGE = (255, 140, 26)             # the 55–65° band
# Relief spectrum bands (wavelength, mm): grit, pebbles/clods, mounds, hills.
BANDS_MM = ((0.3, 1.0), (1.0, 3.0), (3.0, 10.0), (10.0, 40.0))
# Rock sizes (equivalent diameter, mm): dirtlab's small and medium pebbles, stones.
ROCK_BINS_MM = (0.5, 1.5, 4.0, 8.0, 1e9)
TOPHAT_MM, TOPHAT_RISE_MM = 6.0, 0.35   # a "rock" stands this far above its 6 mm surroundings
# The printable-texture rule (docs/standards/printing.html#texture).
TEX_WINDOW_MM, TEX_LAYER_MM, TEX_MIN_OK = 1.5, 0.2, 0.97
TEX_GRID_MM = 0.05                 # dense, so the measure does not depend on the mesh grid
TEX_RIM_MM = 1.0                   # dirtlab RIM_TEX_MM: the texture ramps in over the outer 1 mm
VIEWS = (("painted", 40, -120), ("top", 89, -90), ("low", 14, -150), ("close 20 mm", 38, -125))


# ── Metrics (STL in, numbers out; shared definition for Python and Rust) ─────
def overhang_mask(mesh: trimesh.Trimesh) -> np.ndarray:
    """Faces past the overhang limit: clifflab's layer-reach audit (55°, the
    owner's limit), as clifflab/iterate.py paints them."""
    import rockpile
    return rockpile.overhang_faces(mesh)


def overhang_over65(mesh: trimesh.Trimesh) -> np.ndarray:
    """Faces past the 65° hard limit (Shawn, 4 Nov 2026): the same layer-reach
    audit with the cone at 65°.  Between this and ``overhang_mask`` is the
    band (painted orange)."""
    import rockpile
    saved = rockpile.OVERHANG_DEG
    rockpile.OVERHANG_DEG = 65.0
    try:
        return rockpile.overhang_faces(mesh)
    finally:
        rockpile.OVERHANG_DEG = saved


def top_surface(mesh: trimesh.Trimesh, footprint) -> tuple[np.ndarray, np.ndarray]:
    """First hit of a vertical ray per GRID_MM cell inside the footprint:
    (Z, miss) with Z = NaN where the ray found nothing."""
    W, H = footprint
    xs = np.arange(GRID_MM / 2, W, GRID_MM)
    ys = np.arange(GRID_MM / 2, H, GRID_MM)
    X, Y = np.meshgrid(xs, ys)
    o = np.c_[X.ravel(), Y.ravel(), np.full(X.size, mesh.bounds[1, 2] + 5.0)]
    d = np.tile([0.0, 0.0, -1.0], (len(o), 1))
    loc, ri, _ = mesh.ray.intersects_location(o, d, multiple_hits=False)
    z = np.full(X.size, np.nan)
    z[ri] = loc[:, 2]
    z = z.reshape(X.shape)
    return z, np.isnan(z)


def texture_ok_fraction(mesh: trimesh.Trimesh, footprint) -> float:
    """The printable-texture rule, measured: the share of 1.5 mm windows with
    at least one layer (0.2 mm) of relief.  The rule holds at >= 0.97.

    Definition: the top surface is sampled by vertical rays on a dense
    TEX_GRID_MM (0.05 mm) lattice that includes the tile edges.  Every
    sample is the centre of a sliding square window spanning exactly
    TEX_WINDOW_MM (31 × 31 samples; clipped at the tile edge); its relief is
    max − min of Z inside it.  The interior is the window centres at least
    TEX_RIM_MM (1 mm, where dirtlab's texture ramps in) from the tile edge.
    Stones and pebbles count like any other relief: the STL cannot tell them
    apart.

    Calibration (the original texcheck.py is lost): dirtlab round 2 with
    stones, seeds 1–3, documented as 1.9% and "about 2–2.6%" of windows
    under one layer; this measure gives 2.10%, 1.92%, 2.66%.  Sampling on
    the 0.15 mm mesh vertices with node-aligned windows gives 1.83%, 1.65%,
    2.24% (and 1.90% for seed 1 on dirtlab's height field without the
    stones, likely texcheck's own definition) but depends on the mesh grid
    lining up with the samples, so the dense lattice is the shared one.
    Cell-centred 0.15 mm samples lose relief to interpolation (2.75% for
    seed 1) and are not used."""
    W, H = footprint
    nx, ny = int(round(W / TEX_GRID_MM)) + 1, int(round(H / TEX_GRID_MM)) + 1
    eps = 1e-3
    xs = np.clip(np.linspace(0.0, W, nx), eps, W - eps)
    ys = np.clip(np.linspace(0.0, H, ny), eps, H - eps)
    X, Y = np.meshgrid(xs, ys)
    o = np.c_[X.ravel(), Y.ravel(), np.full(X.size, mesh.bounds[1, 2] + 5.0)]
    loc, ri, _ = mesh.ray.intersects_location(o, np.tile([0.0, 0.0, -1.0], (len(o), 1)),
                                              multiple_hits=False)
    z = np.full(X.size, np.nan)
    z[ri] = loc[:, 2]
    Z = z.reshape(X.shape)
    Z = np.where(np.isnan(Z), np.nanmin(Z), Z)
    k = int(round(TEX_WINDOW_MM / (W / (nx - 1)))) + 1
    relief = ndimage.maximum_filter(Z, k, mode="nearest") - ndimage.minimum_filter(Z, k, mode="nearest")
    gx, gy = np.meshgrid(np.linspace(0.0, W, nx), np.linspace(0.0, H, ny))
    inner = np.minimum.reduce([gx, gy, W - gx, H - gy]) >= TEX_RIM_MM
    return float((relief[inner] >= TEX_LAYER_MM).mean())


def relief_spectrum(Z: np.ndarray) -> dict:
    """RMS amplitude (mm) of the top surface per wavelength band."""
    z = Z - np.nanmean(Z)
    z = np.nan_to_num(z) * np.outer(np.hanning(z.shape[0]), np.hanning(z.shape[1]))
    P = np.abs(np.fft.fft2(z)) ** 2 / z.size
    fy = np.fft.fftfreq(z.shape[0], GRID_MM)
    fx = np.fft.fftfreq(z.shape[1], GRID_MM)
    f = np.hypot(*np.meshgrid(fy, fx, indexing="ij"))
    w = 1.0 / np.maximum(f, 1e-9)
    # Hann window keeps ~0.375 of the power in 2D (0.6124² per axis).
    return {f"{lo:g}-{hi:g}mm": float(np.sqrt(P[(w >= lo) & (w < hi)].sum() / z.size / 0.140625))
            for lo, hi in BANDS_MM}


def rocks(Z: np.ndarray) -> tuple[np.ndarray, list[float]]:
    """Rocks seen from above: connected areas standing TOPHAT_RISE_MM over a
    TOPHAT_MM-wide grey opening of the surface.  Returns (mask, diameters).

    An estimator, not the lab's truth: it also counts tall soil clods as
    cover and merges touching pebbles (dirt s1–s4: bare 0.77–0.79 against
    dirtlab's own 0.92–0.93; see ``lab`` in metrics.json).  It is the same
    function for Python and Rust STLs, which is what parity needs."""
    r = int(round(TOPHAT_MM / 2 / GRID_MM))
    yy, xx = np.mgrid[-r:r + 1, -r:r + 1]
    disc = xx ** 2 + yy ** 2 <= r * r
    z = np.nan_to_num(Z, nan=np.nanmin(Z))
    mask = (z - ndimage.grey_opening(z, footprint=disc)) > TOPHAT_RISE_MM
    lab, n = ndimage.label(mask)
    areas = ndimage.sum_labels(np.ones_like(z), lab, range(1, n + 1)) * GRID_MM ** 2
    diam = [float(2 * np.sqrt(a / np.pi)) for a in areas if a >= np.pi * 0.25 ** 2]
    return mask, diam


def stl_metrics(mesh: trimesh.Trimesh, footprint, edge_mm: float) -> dict:
    """The parity metrics of one tile, from its mesh only (plan.html,
    "The parity harness"; the ones that apply to dirt)."""
    Z, miss = top_surface(mesh, footprint)
    rmask, diam = rocks(Z)
    hist = np.histogram(diam, bins=ROCK_BINS_MM)[0]
    n = mesh.face_normals
    ov = overhang_mask(mesh)
    nrm = (n[:, 2] < -np.sin(np.radians(OVERHANG_NORMAL_DEG))) & (mesh.triangles_center[:, 2] > 0.05)
    rel = Z[~miss] - edge_mm
    return {
        "faces": int(len(mesh.faces)),
        "vertices": int(len(mesh.vertices)),
        "watertight": bool(mesh.is_watertight),
        "bodies": int(mesh.body_count),
        "volume_mm3": float(mesh.volume),
        "see_through_cells": int(miss.sum()),
        "bare_fraction": float(1.0 - rmask[~miss].mean()),
        "rock_count": len(diam),
        "rock_hist": {f"{lo:g}-{hi:g}mm": int(c) for lo, hi, c in
                      zip(ROCK_BINS_MM[:-1], ROCK_BINS_MM[1:], hist)},
        "relief_mm": {"p05": float(np.percentile(rel, 5)), "median": float(np.median(rel)),
                      "p95": float(np.percentile(rel, 95)), "max": float(rel.max())},
        "relief_spectrum_rms_mm": relief_spectrum(Z),
        "texture_ok_fraction": texture_ok_fraction(mesh, footprint),
        "overhang_mm2": float(mesh.area_faces[ov].sum()),
        "overhang_normal_mm2": float(mesh.area_faces[nrm].sum()),
    }, ov


ROCK_DIAM_BINS_MM = (0.0, 6.0, 9.0, 12.0, 16.0, 1e9)
ROCK_OPEN_MM, ROCK_RISE_MM = 20.0, 1.0   # a rock stands 1 mm over a 20 mm grey opening


def rock_metrics(mesh: trimesh.Trimesh, footprint, edge_mm: float) -> dict:
    """Rocks seen from above, from the STL alone: connected areas standing
    ROCK_RISE_MM over a ROCK_OPEN_MM-wide grey opening of the top surface.
    Clusters that touch count as one; pebbles and clods are under the rise."""
    Z, miss = top_surface(mesh, footprint)
    z = np.nan_to_num(Z, nan=np.nanmin(Z))
    r = int(round(ROCK_OPEN_MM / 2 / GRID_MM))
    # a disc opening of radius r is slow at this size: open with a square of
    # the same area as a fast stand-in (the same function for both sides)
    k = int(round(np.sqrt(np.pi) * r))
    opened = ndimage.grey_opening(z, size=(k, k))
    mask = (z - opened) > ROCK_RISE_MM
    lab, n = ndimage.label(mask)
    areas = ndimage.sum_labels(np.ones_like(z), lab, range(1, n + 1)) * GRID_MM ** 2
    keep = [i for i, a in enumerate(areas) if a >= np.pi * 1.0 ** 2]
    diam = [float(2 * np.sqrt(areas[i] / np.pi)) for i in keep]
    tops = ndimage.maximum(z, lab, [i + 1 for i in keep]) if keep else []
    hist = np.histogram(diam, bins=ROCK_DIAM_BINS_MM)[0]
    return {"count": len(diam),
            "hist": {f"{lo:g}-{hi:g}mm": int(c) for lo, hi, c in
                     zip(ROCK_DIAM_BINS_MM[:-1], ROCK_DIAM_BINS_MM[1:], hist)},
            "diam_mean_mm": float(np.mean(diam)) if diam else 0.0,
            "top_mm": float(np.max(tops) - edge_mm) if len(tops) else 0.0}


WATER_SAMPLE_MM, WATER_INSET_MM, WATER_WET_MM = 0.1, 0.02, 3.3
WATER_RIM_MM, WATER_BAND_MM = 4.0, 2.4
WATER_EDGE_TRIM_MM = 2.0   # edge samples this close (along the edge) to a dry one are shore
WATER_BAND_TRIM_MM = 4.0   # amplitude cells this close to land above the band are bank
TIDE_AMP_MM = 0.6


def wet_run(z: np.ndarray, step_mm: float) -> np.ndarray:
    """Edge samples (1-D, `step_mm` apart) that are open water: below the wet
    level and WATER_EDGE_TRIM_MM from any sample that is not.  At the shore
    the land rises through the troughs of the edge profile, so the samples
    next to it are bank, not water.  Used on the STL and on the lab grid."""
    dry = ~(np.isfinite(z) & (z < WATER_WET_MM))
    if not dry.any():
        return ~dry
    return ndimage.distance_transform_edt(~dry) * step_mm > WATER_EDGE_TRIM_MM


def _first_hits(mesh, xy):
    o = np.c_[xy, np.full(len(xy), mesh.bounds[1, 2] + 5.0)]
    loc, ri, _ = mesh.ray.intersects_location(o, np.tile([0.0, 0.0, -1.0], (len(o), 1)),
                                              multiple_hits=False)
    z = np.full(len(xy), np.nan)
    z[ri] = loc[:, 2]
    return z


def water_metrics(mesh: trimesh.Trimesh, footprint, wl_mm: float = 2.6,
                  square_mm: float = 35.0) -> dict:
    """The water's edge profile and amplitude, from the STL alone.

    Edge profile: the top surface sampled on each edge (inset 0.02 mm,
    every 0.1 mm); where it is wet (z < 3.3 mm) it is compared with the
    contract wl + 0.6·sin(2π(s − org)/λ), λ = square/2, org = λ − λ/4,
    s the tile coordinate along the edge, ``wet_run`` trimming the shore.
    Amplitude: percentiles of z − wl over top-surface cells ≥ 4 mm from the
    rim in the water band (z < wl + 2.4), 4 mm clear of anything above it
    (the bank and the rock)."""
    W, H = footprint
    lam = square_mm / 2
    org = square_mm * 0.5 - lam * 0.25
    s_x = np.arange(0.0, W + 1e-9, WATER_SAMPLE_MM).clip(WATER_INSET_MM, W - WATER_INSET_MM)
    s_y = np.arange(0.0, H + 1e-9, WATER_SAMPLE_MM).clip(WATER_INSET_MM, H - WATER_INSET_MM)
    edges = {"south": (np.c_[s_x, np.full_like(s_x, WATER_INSET_MM)], s_x),
             "north": (np.c_[s_x, np.full_like(s_x, H - WATER_INSET_MM)], s_x),
             "west": (np.c_[np.full_like(s_y, WATER_INSET_MM), s_y], s_y),
             "east": (np.c_[np.full_like(s_y, W - WATER_INSET_MM), s_y], s_y)}
    out_e, errs = {}, []
    for name, (xy, s) in edges.items():
        z = _first_hits(mesh, xy)
        wet = wet_run(z, WATER_SAMPLE_MM)
        want = wl_mm + TIDE_AMP_MM * np.sin(2 * np.pi * (s - org) / lam)
        e = np.abs(z[wet] - want[wet])
        errs.append(e)
        out_e[name] = {"wet": int(wet.sum()), "max_mm": float(e.max()) if len(e) else 0.0,
                       "mean_mm": float(e.mean()) if len(e) else 0.0}
    alle = np.concatenate(errs)
    Z, miss = top_surface(mesh, footprint)
    xs = np.arange(GRID_MM / 2, W, GRID_MM)
    ys = np.arange(GRID_MM / 2, H, GRID_MM)
    X, Y = np.meshgrid(xs, ys)
    rim = np.minimum.reduce([X, W - X, Y, H - Y])
    above = miss | (Z >= wl_mm + WATER_BAND_MM)
    bank = ndimage.distance_transform_edt(~above) * GRID_MM <= WATER_BAND_TRIM_MM
    band = (~above) & (~bank) & (rim >= WATER_RIM_MM)
    rel = Z[band] - wl_mm
    pct = (lambda q: float(np.percentile(rel, q))) if len(rel) else (lambda q: None)
    return {"edge_max_mm": float(alle.max()) if len(alle) else 0.0,
            "edge_mean_mm": float(alle.mean()) if len(alle) else 0.0,
            "edges": out_e,
            "p01": pct(1), "p05": pct(5), "p50": pct(50), "p95": pct(95), "p99": pct(99),
            "min": float(rel.min()) if len(rel) else None,
            "max": float(rel.max()) if len(rel) else None,
            "cells": int(band.sum())}


def grass_metrics(mesh, footprint, edge_mm: float) -> dict:
    """Grass from the STL: the top surface over the edge level (first hits
    of vertical rays, 2 mm clear of the rim), the same on both sides."""
    W, H = footprint
    Z, miss = top_surface(mesh, footprint)
    xs = np.arange(GRID_MM / 2, W, GRID_MM)
    ys = np.arange(GRID_MM / 2, H, GRID_MM)
    X, Y = np.meshgrid(xs, ys)
    inner = (np.minimum.reduce([X, W - X, Y, H - Y]) >= 2.0) & ~miss
    top = Z[inner] - edge_mm
    return {"grass": {"mean_mm": float(top.mean()), "p05_mm": float(np.percentile(top, 5)),
                      "p50_mm": float(np.percentile(top, 50)), "p95_mm": float(np.percentile(top, 95)),
                      "max_mm": float(top.max())}}


def look_metrics(look: str, mesh, footprint, edge_mm: float) -> dict:
    """The look-specific STL metrics, added to stl_metrics' dict."""
    if look == "grass":
        return grass_metrics(mesh, footprint, edge_mm)
    if look == "rocks":
        return {"rocks": rock_metrics(mesh, footprint, edge_mm)}
    if look == "water":
        return {"water": water_metrics(mesh, footprint)}   # the water level, not the ground edge
    return {}


# ── Labs ────────────────────────────────────────────────────────────────────
def build_dirt(seed: int, stones: bool = False):
    """dirtlab 2×2 at `seed`: (mesh, painted mesh, footprint, edge, lab stats).
    Stones off by default (``STONES_2X2 = (0, 0)`` for this build only):
    they come with rocks in Phase 3, so the Rust dirt has none."""
    import dirtlab as dl
    saved = dl.STONES_2X2
    if not stones:
        dl.STONES_2X2 = (0, 0)
    try:
        t0 = time.perf_counter()
        f, mesh = dl.build(seed)
        dt = time.perf_counter() - t0
    finally:
        dl.STONES_2X2 = saved
    st = f.stats
    lab = {"clods": st["clods"], "pebbles_small": st["small"], "pebbles_medium": st["medium"],
           "stones": st["stones"], "bodies": st["bodies"], "overhang_mm2_45deg": st["overhang"],
           "pebbles_rounded": st["rounded"], "pebble_cover": float(f.pebble.mean())}
    return mesh, dl.painted(f, mesh), (f.W, f.H), dl.EDGE_MM, lab, dt


EARTH_RGB = (128, 97, 64)        # the Rust renderer's paint (dt_kit::paint)
CONNECTOR_RGB = (158, 158, 163)


def build_flat(seed: int):
    """The Python twin of lib:tiles/test/1x1-flat.kdl: a flat 5 mm ground
    slab (core/mesh.make_heightmap_solid at 128 cells per square) on the
    DungeonBlocks base (bases/dungeonblocks, peg height from the content's
    top), unioned in Manifold.  The seed changes nothing on a flat tile."""
    import manifold3d as m3d
    from dharmatiles.bases import dungeonblocks as db
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.core.mesh import make_heightmap_solid
    t0 = time.perf_counter()
    surf = SurfaceConfig(cols=1, rows=1, seed=seed)
    z = np.full((surf.grid_h, surf.grid_w), 5.0)
    slab = make_heightmap_solid(z, surf.tile_w, surf.tile_h, 0.0)
    cfg = BaseConfig()
    peg = db.select_peg_height(z, cfg, float(slab.bounds[1, 2]))
    base = db.make_base(surf, peg, cfg)

    def solid(m):
        return m3d.Manifold(mesh=m3d.Mesh(vert_properties=np.asarray(m.vertices, np.float32),
                                          tri_verts=np.asarray(m.faces, np.uint32)))
    u = (solid(slab) + solid(base)).to_mesh()
    mesh = trimesh.Trimesh(np.asarray(u.vert_properties)[:, :3], np.asarray(u.tri_verts))
    dt = time.perf_counter() - t0
    painted = mesh.copy()
    fc = np.tile([*CONNECTOR_RGB, 255], (len(mesh.faces), 1))
    fc[mesh.triangles_center[:, 2] > 0.0, :3] = EARTH_RGB
    painted.visual.face_colors = fc
    return mesh, painted, (surf.tile_w, surf.tile_h), 5.0, {"peg_mm": peg}, dt


GRASS_RGB = (92, 143, 61)
ROCK_RGB = (150, 150, 155)
WATER_RGB = (60, 95, 140)


def build_rocks(seed: int):
    """rocklab 2×2 at `seed` (its own main, without the export)."""
    import rocklab as rl
    t0 = time.perf_counter()
    rng = np.random.default_rng(seed)
    solid, placed, peg, zmax = rl.build_tile(rng, 2, 2)
    mesh = rl.to_trimesh(solid)
    dt = time.perf_counter() - t0
    stones = [st for p in placed for st in p.stones]
    fac = [st["facets"] for st in stones]
    chips = [st["chips"] for st in stones]
    widths = [st["D"] for st in stones]
    lab = {"placements": len(placed), "clusters": sum(len(p.stones) > 1 for p in placed),
           "stones": len(stones), "facets_min": min(fac), "facets_max": max(fac),
           "facets_mean": float(np.mean(fac)), "chips_min": min(chips), "chips_max": max(chips),
           "width_min": float(min(widths)), "width_max": float(max(widths)),
           "chip_loss_max": rl.CHIP_MAX_LOSS, "peg_mm": peg, "zmax_mm": float(zmax)}
    painted = mesh.copy()
    c = mesh.triangles_center
    nz = mesh.face_normals[:, 2]
    fc = np.tile([*CONNECTOR_RGB, 255], (len(mesh.faces), 1))
    fc[c[:, 2] > 0.0, :3] = EARTH_RGB
    rock = (c[:, 2] > rl.SURFACE_MM + 0.45) | ((np.abs(nz) < 0.6) & (c[:, 2] > rl.SURFACE_MM - 0.3)
                                             & (c[:, 0] > 0.3) & (c[:, 0] < 69.7)
                                             & (c[:, 1] > 0.3) & (c[:, 1] < 69.7))
    fc[rock, :3] = ROCK_RGB
    painted.visual.face_colors = fc
    return mesh, painted, (70.0, 70.0), rl.SURFACE_MM, lab, dt


def build_grass(seed: int):
    """grasslab 2×2 at `seed` (its own main, without the export): the
    approved stroke blades on hills with eye-shaped clearings."""
    import grasslab as gl
    t0 = time.perf_counter()
    rng = np.random.default_rng(seed)
    W, H = 2 * gl.SQUARE_MM, 2 * gl.SQUARE_MM
    grid = gl.Grid(W, H, W / round(W / gl.VOX_MM))
    ground, _ = gl.make_ground(rng, W, H, True)          # flat: the Rust ground is the tile's own
    flow0 = gl.make_direction(rng, ground)
    sw = gl.Swaths(rng, W, H, flow0)
    lenses = gl.place_lenses(rng, ground, flow0, W, H)
    flow = gl.finish_flow(rng, flow0, lenses, W, H)
    g = gl.build_grass(rng, grid, ground, sw, lenses, flow, W, H)
    mesh, _, _ = gl.attach_base(g.solid, 2, 2, g.solid.bounding_box()[5])
    mesh = gl.stl_safe(mesh)
    dt = time.perf_counter() - t0
    gh = g.T - g.H0
    lab = {"blades": g.n_fronds, "strands": g.n_strands, "clearings": len(lenses.centre),
           "soil_frac": float((g.soil > 0.5).mean()), "gaps": g.gaps,
           "grass_mean_mm": float(gh.mean()), "grass_p95_mm": float(np.percentile(gh, 95)),
           "grass_max_mm": float(gh.max())}
    painted = mesh.copy()
    c = mesh.triangles_center
    fc = np.tile([*CONNECTOR_RGB, 255], (len(mesh.faces), 1))
    fc[c[:, 2] > 0.0, :3] = EARTH_RGB
    fc[c[:, 2] > gl.GROUND_EDGE_MM + 0.6, :3] = GRASS_RGB
    painted.visual.face_colors = fc
    return mesh, painted, (W, H), gl.GROUND_EDGE_MM, lab, dt


def build_water(seed: int):
    """waterlab swell-chaos, bank-rock scene, at `seed` (as its main)."""
    import waterlab as wl
    t0 = time.perf_counter()
    sc = wl.Scene(seed)
    relief = wl.gen_swell_chaos(wl.mode_rng(seed, "swell-chaos"), sc)
    mesh, z = wl.build(sc, relief, sc.tide)
    dt = time.perf_counter() - t0
    st = wl.stats(sc, relief, z)
    # the edge contract on the final height grid: open-water perimeter nodes
    lam = wl.TIDE_EDGE_LAMBDA_MM
    org = wl.SQUARE_MM * 0.5 - lam * 0.25
    X, Y = sc.X, sc.Y
    px = wl.TIDE_EDGE_AMP_MM * np.sin(2 * np.pi * (X - org) / lam)
    py = wl.TIDE_EDGE_AMP_MM * np.sin(2 * np.pi * (Y - org) / lam)
    dy, dx = np.minimum(Y, sc.H - Y), np.minimum(X, sc.W - X)
    wy, wx = 1 / np.maximum(dy, 1e-6), 1 / np.maximum(dx, 1e-6)
    edge = wl.WL_MM + (wy * px + wx * py) / (wy + wx)
    errs = [np.abs(z[sl] - edge[sl])[wet_run(z[sl], wl.CELL_MM)]
            for sl in ((0, slice(None)), (-1, slice(None)), (slice(None), 0), (slice(None), -1))]
    errs = np.concatenate(errs)
    err = float(errs.max()) if len(errs) else 0.0
    lab = {"relief_min": st["relief_min"], "relief_max": st["relief_max"],
           "relief_rms": st["relief_rms"], "max_slope_deg": st["max_slope"],
           "edge_err_mm": err}
    painted = mesh.copy()
    c = mesh.triangles_center
    nz = mesh.face_normals[:, 2]
    fc = np.tile([*CONNECTOR_RGB, 255], (len(mesh.faces), 1))
    top = c[:, 2] > 0.0
    fc[top, :3] = EARTH_RGB
    # land: the bank, by the scene's own land mask at the face's xy
    ix = np.clip(np.round(c[:, 0] / wl.CELL_MM).astype(int), 0, X.shape[1] - 1)
    iy = np.clip(np.round(c[:, 1] / wl.CELL_MM).astype(int), 0, X.shape[0] - 1)
    water = top & sc.water[iy, ix] & (c[:, 2] < wl.WL_MM + WATER_BAND_MM) \
        & ((nz > 0.3) | (c[:, 2] < wl.WL_MM + 0.5))
    fc[water, :3] = WATER_RGB
    rock = top & sc.rock_mask[iy, ix] & ~water
    fc[rock, :3] = ROCK_RGB
    painted.visual.face_colors = fc
    return mesh, painted, (sc.W, sc.H), wl.WL_MM, lab, dt


STONE_RGB = (186, 184, 176)


def build_masonry(family: str, seed: int):
    """A wall or floor layer (walls/) at `seed` on a flat 5 mm ground slab
    on the DungeonBlocks base, in memory, like the P5 lead's wall_py.py.
    Walls stand along the north edge (the Rust test tiles'
    ``along="north"``); the floor covers the tile.  Lab stats: units laid
    (``wall.units`` / ``floor.units``) and courses, as the Rust notes."""
    import manifold3d as m3d
    from dharmatiles.bases import dungeonblocks as db
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.core.mesh import make_heightmap_solid
    from dharmatiles.core.tile import TileScene
    from dharmatiles import walls as W
    surf = SurfaceConfig(cols=1, rows=1, seed=seed)
    z = np.full((surf.grid_h, surf.grid_w), 5.0)
    scene = TileScene(surface=surf, terrain_z=z, terrain_support_z=z.copy(),
                      obstacle_mask=np.zeros(z.shape, bool))
    north = [(1.0, 1.0), (0.0, 1.0)]
    cls, kw, kind = {
        "wall-cut-stone": (W.CutStoneWall, {"spine": north}, "wall"),
        "wall-brick": (W.BrickWall, {"spine": north}, "wall"),
        "wall-regular-brick": (W.RegularBrickWall, {"spine": north}, "wall"),
        "wall-fieldstone": (W.FieldstoneWall, {"spine": north}, "wall"),
        "floor-stone": (W.StoneFloor, {"texture": "dressed"}, "floor"),
        "floor-fieldstone": (W.FieldstoneWall, {"spine": [(0.0, 0.0), (1.0, 0.0)], "laid_flat": True,
                              "height_mm": 35.0, "thickness_mm": 7.4}, "floor"),
    }[family]
    count = {"units": 0, "courses": set()}

    class Counted(cls):
        def _cells(self, *a, **k):
            cells = super()._cells(*a, **k)
            count["courses"] = {c.key[0] for c in cells}
            return cells

        def _place_block(self, *a, **k):
            b = super()._place_block(*a, **k)
            count["units"] += b is not None
            return b

    t0 = time.perf_counter()
    parts = Counted(seed=seed, **kw).apply(scene)
    dt = time.perf_counter() - t0
    slab = make_heightmap_solid(z, surf.tile_w, surf.tile_h, 0.0)
    cfg = BaseConfig()
    peg = db.select_peg_height(z, cfg, float(slab.bounds[1, 2]))
    base = db.make_base(surf, peg, cfg)

    def solid(m):
        return m3d.Manifold(mesh=m3d.Mesh(vert_properties=np.asarray(m.vertices, np.float32),
                                          tri_verts=np.asarray(m.faces, np.uint32)))
    u = solid(slab) + solid(base)
    for p in parts:
        u = u + solid(p)
    u = u.to_mesh()
    mesh = trimesh.Trimesh(np.asarray(u.vert_properties)[:, :3], np.asarray(u.tri_verts))
    painted = mesh.copy()     # the ground is flat at 5 mm: above it is masonry
    zc = mesh.triangles_center[:, 2]
    fc = np.tile([*CONNECTOR_RGB, 255], (len(mesh.faces), 1))
    fc[zc > 0.0, :3] = EARTH_RGB
    fc[zc > 5.05, :3] = STONE_RGB
    painted.visual.face_colors = fc
    lab = {f"{kind}.units": count["units"], f"{kind}.courses": len(count["courses"]),
           f"{kind}.overhang_reach_mm2": float(mesh.area_faces[overhang_mask(mesh)].sum())}
    return mesh, painted, (surf.tile_w, surf.tile_h), 5.0, lab, dt


LEAF_RGB = (84, 140, 62)


def build_tree(seed: int):
    """The Python twin of lib:tiles/test/1x1-tree.kdl: the 1×1 grass-tree
    tile's Tree layer (src/tiles/ground/1x1-grass-tree.tile.py: height 35,
    canopy radius 12, base radius 11; n_attractors 32 as the Rust default)
    grown at the tile centre on a flat 5 mm slab with the DungeonBlocks
    base, unioned in Manifold.  Leaves OFF on both sides (the Rust test
    tile runs ``leaves clumps-only`` until its leaves pass the gate), so
    this is wood + foliage clumps."""
    import warnings
    import manifold3d as m3d
    from dharmatiles.bases import dungeonblocks as db
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.core.mesh import make_heightmap_solid
    from dharmatiles.trees.layer import Tree
    from dharmatiles.trees.mesh import build_branch_mesh
    from dharmatiles.trees.skeleton import grow_skeleton
    ground = 5.0
    t = Tree(height_mm=35.0, canopy_radius_mm=12.0, canopy_base_radius_mm=11.0,
             n_attractors=32, leaf_length_mm=4.5, leaf_width_mm=3.0)
    t0 = time.perf_counter()
    surf = SurfaceConfig(cols=1, rows=1, seed=seed)
    rng = np.random.default_rng(seed)
    env = t._sample_envelope(surf.tile_w / 2, surf.tile_h / 2, ground, rng)
    nodes, parents, radii, ind, outd, att, _ = grow_skeleton(
        env, rng, n_attractors=t.n_attractors, segment_length_mm=t.segment_length_mm,
        min_radius_mm=t.min_radius_mm, branch_split_angle_deg=t.branch_split_angle_deg,
        target_fdm_angle_deg=t.target_fdm_angle_deg, max_branches_per_step=t.max_branches_per_step,
        branch_exponent=t.branch_exponent, smoothing_alpha=t.smoothing_alpha,
        group_width_mm=t.group_width_mm, group_height_mm=t.group_height_mm,
        foliage_bulge_mm=t.foliage_bulge_mm, branch_split_eagerness=t.branch_split_eagerness,
        branch_target=t.branch_target, branch_fork_balance=t.branch_fork_balance)
    with warnings.catch_warnings(record=True):
        wood, fol, _, _ = build_branch_mesh(
            nodes, parents, radii, ind, outd, terrain_z=ground,
            strict_fdm_angle_deg=t.strict_fdm_angle_deg,
            foliage_cluster_radius_mm=t.foliage_cluster_radius_mm,
            foliage_cluster_length_mm=t.foliage_cluster_length_mm, bark=t.bark, bark_seed=seed,
            leaves=False)
    z = np.full((surf.grid_h, surf.grid_w), ground)
    slab = make_heightmap_solid(z, surf.tile_w, surf.tile_h, 0.0)
    cfg = BaseConfig()
    base = db.make_base(surf, db.select_peg_height(z, cfg, float(slab.bounds[1, 2])), cfg)

    def solid(m):
        return m3d.Manifold(mesh=m3d.Mesh(vert_properties=np.asarray(m.vertices, np.float32),
                                          tri_verts=np.asarray(m.faces, np.uint32)))
    acc = solid(slab) + solid(base)
    for part in (wood, fol):        # concatenations of closed shells
        if len(part.vertices):
            for b in part.split(only_watertight=False):
                acc = acc + solid(b)
    u = acc.to_mesh()
    mesh = trimesh.Trimesh(np.asarray(u.vert_properties)[:, :3], np.asarray(u.tri_verts))
    dt = time.perf_counter() - t0
    el = [np.degrees(np.arcsin(np.clip((nodes[i] - nodes[parents[i]])[2]
                                       / np.linalg.norm(nodes[i] - nodes[parents[i]]), -1, 1)))
          for i in range(1, len(nodes)) if np.linalg.norm(nodes[i] - nodes[parents[i]]) > 1e-9]
    ch = np.bincount(parents[1:], minlength=len(nodes))
    painted = mesh.copy()
    fc = np.tile([*CONNECTOR_RGB, 255], (len(mesh.faces), 1))
    fc[mesh.triangles_center[:, 2] > 0.0, :3] = EARTH_RGB
    fc[mesh.triangles_center[:, 2] > ground + 0.05, :3] = LEAF_RGB
    painted.visual.face_colors = fc
    lab = {"tree.attractors": len(att), "tree.terminals": int((ch == 0).sum()),
           "tree.nodes": len(nodes), "tree.min_branch_elevation_deg": float(min(el)),
           "tree.trunk_radius_mm": float(radii[0])}
    return mesh, painted, (surf.tile_w, surf.tile_h), ground, lab, dt


def tree_metrics(mesh: trimesh.Trimesh, ground: float) -> dict:
    """What stands on the ground: height, crown width (the widest of the
    x and y extents above the trunk's lower third) and volume above it."""
    v = mesh.vertices
    above = v[v[:, 2] > ground + 0.05]
    h = float(above[:, 2].max() - ground) if len(above) else 0.0
    crown = above[above[:, 2] > ground + h / 3] if len(above) else above
    w = float(max(np.ptp(crown[:, 0]), np.ptp(crown[:, 1]))) if len(crown) else 0.0
    try:
        vol = float(mesh.slice_plane([0, 0, ground + 0.05], [0, 0, 1], cap=True).volume)
    except Exception:  # noqa: BLE001
        vol = float("nan")
    return {"tree_height_mm": h, "crown_width_mm": w, "tree_volume_mm3": vol}


MASONRY = ("wall-cut-stone", "wall-brick", "wall-regular-brick", "wall-fieldstone", "floor-stone",
           "floor-fieldstone")
LABS = {"dirt": build_dirt, "dirt-stones": lambda seed: build_dirt(seed, stones=True),
        "flat": build_flat, "rocks": build_rocks, "water": build_water, "tree": build_tree,
        "grass": build_grass,
        **{m: (lambda seed, m=m: build_masonry(m, seed)) for m in MASONRY}}
# What the Python column is, in the captions.
LAB_NOTE = {"dirt": "dirtlab, stones off (P3)", "dirt-stones": "dirtlab with stones",
            "flat": "flat 1×1 twin", "rocks": "rocklab", "grass": "grasslab, flat hills",
            "water": "waterlab swell-chaos, bank-rock", "tree": "library Tree, 1×1 tile params, leaves off",
            **{m: f"walls/ {m} on a flat 5 mm slab" for m in MASONRY}}


# ── Parity verdict (plan.html, "The parity harness") ────────────────────────
# (name, where in the metrics, test, tolerance).  "rel": |Rust − Py| ≤ tol ×
# Python mean; "abs": |Rust − Py| ≤ tol (mm); "min": Rust mean ≥ tol;
# "max": every Rust seed ≤ tol; "info": reported, not judged.
_DIRT_ROWS = [
    ("clods", ("lab", "clods"), "rel", 0.15),
    ("pebbles_small", ("lab", "pebbles_small"), "rel", 0.15),
    ("pebbles_medium", ("lab", "pebbles_medium"), "rel", 0.15),
    ("rock_count", ("rock_count",), "rel", 0.15),
    *[(f"spectrum {lo:g}-{hi:g}mm", ("relief_spectrum_rms_mm", f"{lo:g}-{hi:g}mm"), "rel", 0.20)
      for lo, hi in BANDS_MM],
    ("texture_ok_fraction", ("texture_ok_fraction",), "min", TEX_MIN_OK),
    ("relief p05 (mm over edge)", ("relief_mm", "p05"), "info", None),
    ("relief median (mm over edge)", ("relief_mm", "median"), "abs", 0.3),
    ("relief p95 (mm over edge)", ("relief_mm", "p95"), "abs", 0.3),
]

PARITY_BY_LOOK = {
    "dirt": _DIRT_ROWS,
    "flat": _DIRT_ROWS,
    "dirt-stones": _DIRT_ROWS + [
        ("stones", ("lab", "stones"), "rel", 0.3),
    ],
    "rocks": [
        ("placements", ("lab", "placements"), "rel", 0.15),
        ("stones", ("lab", "stones"), "rel", 0.2),
        ("facets_mean", ("lab", "facets_mean"), "rel", 0.15),
        ("facets_min", ("lab", "facets_min"), "info", None),
        ("facets_max", ("lab", "facets_max"), "info", None),
        ("chip_loss_max", ("lab", "chip_loss_max"), "max", 0.03),
        ("STL rocks count", ("rocks", "count"), "rel", 0.2),
        *[(f"STL rocks {k}", ("rocks", "hist", k), "info", None)
          for k in (f"{lo:g}-{hi:g}mm" for lo, hi in zip(ROCK_DIAM_BINS_MM[:-1], ROCK_DIAM_BINS_MM[1:]))],
        ("STL rock top (mm over edge)", ("rocks", "top_mm"), "abs", 1.5),
    ],
    "grass": [
        ("blades", ("lab", "blades"), "rel", 0.15),
        ("strands", ("lab", "strands"), "rel", 0.2),
        ("clearings", ("lab", "clearings"), "rel", 0.35),
        ("bare share (soil)", ("lab", "soil_frac"), "abs", 0.05),
        ("grass top mean (mm over edge)", ("grass", "mean_mm"), "abs", 0.5),
        ("grass top p95 (mm over edge)", ("grass", "p95_mm"), "abs", 0.6),
        ("grass top max (mm over edge)", ("grass", "max_mm"), "abs", 1.0),
        ("overhang (mm2)", ("overhang_mm2",), "info", None),
        ("faces (whole tile)", ("faces",), "info", None),
        ("build (s)", ("build_s",), "info", None),
    ],
    "water": [
        ("relief_min", ("lab", "relief_min"), "abs", 0.15),
        ("relief_max", ("lab", "relief_max"), "abs", 0.3),
        ("relief_rms", ("lab", "relief_rms"), "rel", 0.15),
        ("STL water p05 (mm over wl)", ("water", "p05"), "abs", 0.2),
        ("STL water p95 (mm over wl)", ("water", "p95"), "abs", 0.2),
        ("STL edge max error (mm)", ("water", "edge_max_mm"), "max", 0.05),
        ("edge_err_mm", ("lab", "edge_err_mm"), "max", 0.05),
    ],
}


# The tree's rows: same look and statistics, not RNG-identical (P5).
TREE_PARITY = [
    ("terminals", ("lab", "tree.terminals"), "rel", 0.15),
    ("skeleton nodes", ("lab", "tree.nodes"), "info", None),
    ("min branch elevation (deg)", ("lab", "tree.min_branch_elevation_deg"), "min", 26.0),
    ("trunk radius (mm)", ("lab", "tree.trunk_radius_mm"), "rel", 0.25),
    ("height (mm)", ("tree_height_mm",), "rel", 0.15),
    ("crown width (mm)", ("crown_width_mm",), "rel", 0.20),
    ("volume (mm3)", ("tree_volume_mm3",), "rel", 0.30),
    ("overhang (mm2)", ("overhang_mm2",), "info", None),
    ("build (s)", ("build_s",), "info", None),
]
PARITY_OF = {"tree": TREE_PARITY}


def _dig(d, path):
    for k in path:
        if not isinstance(d, dict) or d.get(k) is None:
            return None
        d = d[k]
    return float(d)


def _masonry_parity(kind: str) -> list:
    """Walls and floors: the masonry statistics (the Rust tile stands on
    dirt, the Python one on a flat slab, so whole-tile volume and faces
    are reported, not judged).  Rust must pass its own audit (0 mm²)."""
    return [
        ("units laid", ("lab", f"{kind}.units"), "rel", 0.25),
        ("courses", ("lab", f"{kind}.courses"), "rel", 0.25),
        ("faces (whole tile)", ("faces",), "info", None),
        ("volume mm³ (whole tile)", ("volume_mm3",), "info", None),
        ("overhang mm² (layer reach)", ("overhang_mm2",), "info", None),
        ("build s", ("build_s",), "info", None),
    ]


def parity_verdict(look: str, py: dict, rs: dict) -> dict:
    """Python vs Rust means over the seeds, per PARITY row; writes nothing."""
    rows = []
    table = (_masonry_parity(look.split("-")[0]) if look in MASONRY
             else PARITY_OF.get(look) or PARITY_BY_LOOK[look])
    for name, path, test, tol in table:
        pv = [v for v in (_dig(py[s], path) for s in py) if v is not None]
        rv = [v for v in (_dig(rs[s], path) for s in rs) if v is not None]
        pm = float(np.mean(pv)) if pv else None
        rm = float(np.mean(rv)) if len(rv) == len(rs) else None   # every seed, or missing
        ratio = rm / pm if (pm and rm is not None) else None
        if test == "info":
            ok = None
        elif rm is None or (pm is None and test not in ("min", "max")):
            ok = False
        elif test == "rel":
            ok = abs(rm - pm) <= tol * abs(pm)
        elif test == "abs":
            ok = abs(rm - pm) <= tol
        elif test == "max":
            ok = max(rv) <= tol            # a ceiling: the worst seed, not the mean
        else:
            ok = rm >= tol
        rows.append({"metric": name, "python_mean": pm, "rust_mean": rm, "ratio": ratio,
                     "test": test, "tolerance": tol, "pass": ok,
                     **({} if rm is not None else {"note": "missing from the Rust side"})})
    judged = [r["pass"] for r in rows if r["pass"] is not None]
    return {"look": look, "seeds": sorted(py), "pass": all(judged), "metrics": rows}


def print_verdict(v: dict) -> None:
    f = lambda x: "—" if x is None else f"{x:.4g}"  # noqa: E731
    tol = {"rel": lambda t: f"±{t:.0%}", "abs": lambda t: f"±{t:g} mm",
           "min": lambda t: f"≥ {t:g}", "max": lambda t: f"≤ {t:g}",
           "info": lambda t: "info"}
    print(f"parity {v['look']} seeds {v['seeds']}: {'PASS' if v['pass'] else 'FAIL'}")
    print(f"  {'metric':30} {'python':>10} {'rust':>10} {'ratio':>7}  {'tolerance':>10}  verdict")
    for r in v["metrics"]:
        verdict = {True: "pass", False: "FAIL", None: ""}[r["pass"]]
        print(f"  {r['metric']:30} {f(r['python_mean']):>10} {f(r['rust_mean']):>10} "
              f"{f(r['ratio']):>7}  {tol[r['test']](r['tolerance']):>10}  {verdict}")


# ── Rendering ───────────────────────────────────────────────────────────────
_WARNED: list = []


def render(meshes, path, elev, azim):
    """The repo's renderer; the CPU Embree stand-in when pyrender has no
    screen (same camera and light directions, flat shading)."""
    try:
        from dharmatiles.render import render as gl
        gl(meshes, path, elev=elev, azim=azim, resolution=(1200, 950), quiet=True, smooth=False)
        return "pyrender"
    except Exception as e:  # noqa: BLE001 — no GL context: fall back
        if not _WARNED:
            _WARNED.append(e)
            print(f"  pyrender failed ({type(e).__name__}: {e}); using the CPU renderer")
        cpu_render(meshes, path, elev, azim)
        return "cpu"


def cpu_render(meshes, path, elev, azim, res=(900, 720)):
    m = trimesh.util.concatenate(meshes)
    fc = np.asarray(m.visual.face_colors)[:, :3] / 255.0

    def dirv(e, a):
        e, a = np.radians(e), np.radians(a)
        return np.array([np.cos(e) * np.cos(a), np.cos(e) * np.sin(a), np.sin(e)])

    c = m.bounds.mean(0)
    fov = np.radians(40.0)
    cam = c + np.linalg.norm(np.ptp(m.bounds, 0)) / (2 * np.tan(fov / 2)) * 1.1 * dirv(elev, azim)
    fwd = (c - cam) / np.linalg.norm(c - cam)
    right = np.cross(fwd, [0, 0, 1.0])
    right /= np.linalg.norm(right)
    up = np.cross(right, fwd)
    W, H = res
    th = np.tan(fov / 2)
    X, Y = np.meshgrid((np.arange(W) + 0.5 - W / 2) / (H / 2) * th,
                       -(np.arange(H) + 0.5 - H / 2) / (H / 2) * th)
    D = (fwd + X[..., None] * right + Y[..., None] * up).reshape(-1, 3)
    D /= np.linalg.norm(D, axis=1, keepdims=True)
    tri = m.ray.intersects_first(np.tile(cam, (len(D), 1)), D)
    hit = tri >= 0
    n = m.face_normals[tri[hit]]
    n[(n * D[hit]).sum(1) > 0] *= -1
    shade = 0.3 + np.clip(n @ dirv(18, azim - 20), 0, None) + 0.35 * np.clip(n @ dirv(elev - 20, azim + 110), 0, None)
    img = np.tile([0.85, 0.88, 0.92], (len(D), 1))
    img[hit] = np.clip(fc[tri[hit]] * shade[:, None] * 0.95, 0, 1) ** (1 / 1.25)
    im = Image.fromarray((img.reshape(H, W, 3) * 255).astype(np.uint8))
    ii, jj = np.nonzero(hit.reshape(H, W))
    if len(ii):
        im = im.crop((max(jj.min() - 20, 0), max(ii.min() - 20, 0),
                      min(jj.max() + 20, W), min(ii.max() + 20, H)))
    im.save(path)


def paint_overhang(painted: trimesh.Trimesh, ov: np.ndarray, hard: np.ndarray | None = None) -> trimesh.Trimesh:
    """Orange: past 55° (``ov``); red: past 65° (``hard``, else all of ``ov``)."""
    m = painted.copy()
    fc = np.asarray(m.visual.face_colors).copy()
    fc[ov, :3] = ORANGE if hard is not None else RED
    if hard is not None:
        fc[hard, :3] = RED
    m.visual.face_colors = fc
    return m


def crop(mesh, box, zmin=0.5):
    x0, x1, y0, y1 = box
    c = mesh.triangles_center
    keep = (c[:, 0] > x0) & (c[:, 0] < x1) & (c[:, 1] > y0) & (c[:, 1] < y1) & (c[:, 2] > zmin)
    return mesh.submesh([np.nonzero(keep)[0]], append=True)


def font(n):
    for f in ("/System/Library/Fonts/Helvetica.ttc", "/Library/Fonts/Arial.ttf",
              "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(f, n)
        except OSError:
            pass
    return ImageFont.load_default()


def caption(who, met, build_s):
    return (_caption_base(met, who, build_s) + _caption_look(met))


def _caption_look(met):
    """The look's own numbers, after the shared ones."""
    if "rocks" in met:
        r = met["rocks"]
        return (f" · STL rocks {r['count']} {list(r['hist'].values())} "
                f"top {r['top_mm']:+.2f} mm")
    if "water" in met:
        w = met["water"]
        return (f" · water p05 {w['p05']:+.2f} p95 {w['p95']:+.2f} mm · "
                f"edge err {w['edge_max_mm']:.3f} mm")
    return ""


def _caption_base(met, who, build_s):
    lab = met.get("lab") or {}
    units = next((k for k in lab if k.endswith(".units")), None)
    what = (f"units {lab[units]:g} · courses {lab.get(units.replace('units', 'courses'), 0):g}"
            if units else
            f"bare {met['bare_fraction']:.3f} · rocks {met['rock_count']} {list(met['rock_hist'].values())}")
    return (f"{who}: {what} · {met['faces']:,} faces · bodies {met['bodies']} · "
            f"watertight {met['watertight']} · vol {met['volume_mm3']:.1f} mm³ · "
            f"see-through {met['see_through_cells']} · texture ok {met['texture_ok_fraction']:.1%} · "
            f"overhang {met['overhang_mm2']:.2f} mm² (red) · build {build_s:.2f}s")


def run(look: str, seeds: list[int], out: pathlib.Path, beside: pathlib.Path | None = None) -> pathlib.Path:
    out.mkdir(parents=True, exist_ok=True)
    cols = len(VIEWS) * (2 if beside else 1)
    cell, head, cap = (620, 500), (70 if beside else 44), 34
    sheet = Image.new("RGB", (cell[0] * cols, (head + cell[1] + cap) * len(seeds)), (245, 245, 242))
    dr = ImageDraw.Draw(sheet)
    summary, py_all, rs_all = {}, {}, {}
    note = LAB_NOTE.get(look, look)
    for r, seed in enumerate(seeds):
        sd = out / f"s{seed}"
        sd.mkdir(exist_ok=True)
        mesh, painted, fp, edge, lab, build_s = LABS[look](seed)
        mesh.export(sd / f"{look}-s{seed}.stl")
        t0 = time.perf_counter()
        met, ov = stl_metrics(mesh, fp, edge)
        met.update(look_metrics(look, mesh, fp, edge))
        met.update(build_s=build_s, metrics_s=time.perf_counter() - t0, edge_mm=edge, lab=lab, look=look,
                   seed=seed)
        if look == "tree":
            met.update(tree_metrics(mesh, edge))
        (sd / "metrics.json").write_text(json.dumps(met, indent=2))
        summary[seed] = py_all[seed] = met
        pm = paint_overhang(painted, ov, overhang_over65(mesh))
        c0 = fp[0] / 2 - 10
        y = r * (head + cell[1] + cap)
        dr.rectangle([0, y, sheet.width, y + head - 6], fill=(225, 228, 235))
        dr.text((12, y + 9), caption(f"{look} s{seed} Python ({note})", met, build_s), fill=(25, 25, 25), font=font(19))
        rust = None
        if beside:
            rd = beside / f"s{seed}"
            stl = next(iter(sorted(rd.rglob("*.stl"))))
            rmesh = trimesh.load(stl)
            rec = json.loads(stl.with_suffix(".build.json").read_text())
            redge = float(rec.get("edge_mm", edge))   # Rust dirt sits at 3.4 mm, dirtlab at 4.0
            rmet, _ = stl_metrics(rmesh, fp, redge)
            rmet.update(look_metrics(look, rmesh, fp, redge))
            rmet.update(build_s=rec["build_s"], edge_mm=redge, lab=rec.get("stats", {}), look=look,
                        seed=seed, rust_audit=rec["audit"])
            if look == "tree":
                rmet.update(tree_metrics(rmesh, redge))
            if look == "rocks" and "stones#2" in rmet["lab"]:
                # the tile's bare dirt notes `stones` (0) first; the rocks' own is the second
                rmet["lab"]["stones"] = rmet["lab"]["stones#2"]
            (sd / "rust-metrics.json").write_text(json.dumps(rmet, indent=2))
            summary[seed] = {"python": met, "rust": rmet}
            rs_all[seed] = rmet
            dr.text((12, y + 36), caption(f"{look} s{seed} Rust  ", rmet, rec["build_s"]), fill=(25, 25, 25), font=font(19))
            rust = {p.stem.rsplit("-", 1)[-1]: p for p in rd.rglob("*.png")}
        for k, (name, elev, azim) in enumerate(VIEWS):
            p = sd / f"{k}-{name.split()[0]}.png"
            meshes = [crop(pm, (c0, c0 + 20, c0, c0 + 20))] if name.startswith("close") else [pm]
            how = render(meshes, p, elev, azim)
            im = Image.open(p).convert("RGB")
            im.thumbnail((cell[0] - 16, cell[1] - 16))
            x = k * cell[0] * (2 if beside else 1)
            sheet.paste(im, (x + (cell[0] - im.width) // 2, y + head + (cell[1] - im.height) // 2))
            dr.text((x + 14, y + head + cell[1] + 6),
                    f"s{seed} · {name} · Python" + (" (cpu)" if how == "cpu" else ""),
                    fill=(40, 40, 40), font=font(20))
            if rust is not None:
                rp = rust.get(name.split()[0])
                x += cell[0]
                if rp is not None:
                    rim = Image.open(rp).convert("RGB")
                    rim.thumbnail((cell[0] - 16, cell[1] - 16))
                    sheet.paste(rim, (x + (cell[0] - rim.width) // 2, y + head + (cell[1] - rim.height) // 2))
                dr.text((x + 14, y + head + cell[1] + 6), f"s{seed} · {name} · Rust",
                        fill=(140, 40, 20), font=font(20))
        print(f"  {look} s{seed}: bare {met['bare_fraction']:.3f}, rocks {met['rock_count']}, "
              f"texture ok {met['texture_ok_fraction']:.1%}, "
              f"overhang {met['overhang_mm2']:.2f} mm², build {build_s:.1f}s")
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    if beside:
        v = parity_verdict(look, py_all, rs_all)
        (out / "parity.json").write_text(json.dumps(v, indent=2))
        print_verdict(v)
    path = out / "contact-sheet.png"
    sheet.save(path)
    print(f"→ {path}")
    return path


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--look", default="dirt", choices=sorted(LABS))
    ap.add_argument("--seeds", type=int, nargs="*", default=[1, 2, 3, 4])
    ap.add_argument("--out", type=pathlib.Path)
    ap.add_argument("--beside", type=pathlib.Path, help="Rust build of the same seeds (xtask parity)")
    a = ap.parse_args()
    run(a.look, a.seeds, a.out or ROOT / "out" / "parity" / a.look, a.beside)


if __name__ == "__main__":
    main()
