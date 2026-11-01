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
for p in ("src", "dirtlab", "clifflab"):
    sys.path.insert(0, str(ROOT / p))

GRID_MM = 0.15                     # top-surface sampling (dirtlab's CELL_MM)
OVERHANG_NORMAL_DEG = 55.0 + 3.0   # the plain normal test, beside the layer audit
RED = (230, 30, 30)
# Relief spectrum bands (wavelength, mm): grit, pebbles/clods, mounds, hills.
BANDS_MM = ((0.3, 1.0), (1.0, 3.0), (3.0, 10.0), (10.0, 40.0))
# Rock sizes (equivalent diameter, mm): dirtlab's small and medium pebbles, stones.
ROCK_BINS_MM = (0.5, 1.5, 4.0, 8.0, 1e9)
TOPHAT_MM, TOPHAT_RISE_MM = 6.0, 0.35   # a "rock" stands this far above its 6 mm surroundings
VIEWS = (("painted", 40, -120), ("top", 89, -90), ("low", 14, -150), ("close 20 mm", 38, -125))


# ── Metrics (STL in, numbers out; shared definition for Python and Rust) ─────
def overhang_mask(mesh: trimesh.Trimesh) -> np.ndarray:
    """Faces past the overhang limit: clifflab's layer-reach audit (55°, the
    owner's limit), as clifflab/iterate.py paints them."""
    import rockpile
    return rockpile.overhang_faces(mesh)


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
        "overhang_mm2": float(mesh.area_faces[ov].sum()),
        "overhang_normal_mm2": float(mesh.area_faces[nrm].sum()),
    }, ov


# ── Labs ────────────────────────────────────────────────────────────────────
def build_dirt(seed: int):
    """dirtlab 2×2 at `seed`: (mesh, painted mesh, footprint, edge, lab stats)."""
    import dirtlab as dl
    t0 = time.perf_counter()
    f, mesh = dl.build(seed)
    dt = time.perf_counter() - t0
    st = f.stats
    lab = {"clods": st["clods"], "pebbles_small": st["small"], "pebbles_medium": st["medium"],
           "stones": st["stones"], "bodies": st["bodies"], "overhang_mm2_45deg": st["overhang"],
           "pebble_cover": float(f.pebble.mean())}
    return mesh, dl.painted(f, mesh), (f.W, f.H), dl.EDGE_MM, lab, dt


LABS = {"dirt": build_dirt}


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


def paint_overhang(painted: trimesh.Trimesh, ov: np.ndarray) -> trimesh.Trimesh:
    m = painted.copy()
    fc = np.asarray(m.visual.face_colors).copy()
    fc[ov, :3] = RED
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


def run(look: str, seeds: list[int], out: pathlib.Path) -> pathlib.Path:
    out.mkdir(parents=True, exist_ok=True)
    cell, head, cap = (620, 500), 44, 34
    sheet = Image.new("RGB", (cell[0] * len(VIEWS), (head + cell[1] + cap) * len(seeds)), (245, 245, 242))
    dr = ImageDraw.Draw(sheet)
    summary = {}
    for r, seed in enumerate(seeds):
        sd = out / f"s{seed}"
        sd.mkdir(exist_ok=True)
        mesh, painted, fp, edge, lab, build_s = LABS[look](seed)
        mesh.export(sd / f"{look}-s{seed}.stl")
        t0 = time.perf_counter()
        met, ov = stl_metrics(mesh, fp, edge)
        met.update(build_s=build_s, metrics_s=time.perf_counter() - t0, lab=lab, look=look, seed=seed)
        (sd / "metrics.json").write_text(json.dumps(met, indent=2))
        summary[seed] = met
        pm = paint_overhang(painted, ov)
        c0 = fp[0] / 2 - 10
        y = r * (head + cell[1] + cap)
        dr.rectangle([0, y, sheet.width, y + head - 6], fill=(225, 228, 235))
        dr.text((12, y + 9),
                f"{look} s{seed}: bare {met['bare_fraction']:.3f} · rocks {met['rock_count']} "
                f"{list(met['rock_hist'].values())} · {met['faces']:,} faces · bodies {met['bodies']} · "
                f"watertight {met['watertight']} · see-through {met['see_through_cells']} · "
                f"overhang {met['overhang_mm2']:.2f} mm² (red) · build {build_s:.1f}s",
                fill=(25, 25, 25), font=font(19))
        for k, (name, elev, azim) in enumerate(VIEWS):
            p = sd / f"{k}-{name.split()[0]}.png"
            meshes = [crop(pm, (c0, c0 + 20, c0, c0 + 20))] if name.startswith("close") else [pm]
            how = render(meshes, p, elev, azim)
            im = Image.open(p).convert("RGB")
            im.thumbnail((cell[0] - 16, cell[1] - 16))
            x = k * cell[0]
            sheet.paste(im, (x + (cell[0] - im.width) // 2, y + head + (cell[1] - im.height) // 2))
            dr.text((x + 14, y + head + cell[1] + 6), f"s{seed} · {name}" + (" (cpu)" if how == "cpu" else ""),
                    fill=(40, 40, 40), font=font(20))
        print(f"  {look} s{seed}: bare {met['bare_fraction']:.3f}, rocks {met['rock_count']}, "
              f"overhang {met['overhang_mm2']:.2f} mm², build {build_s:.1f}s")
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    path = out / "contact-sheet.png"
    sheet.save(path)
    print(f"→ {path}")
    return path


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--look", default="dirt", choices=sorted(LABS))
    ap.add_argument("--seeds", type=int, nargs="*", default=[1, 2, 3, 4])
    ap.add_argument("--out", type=pathlib.Path)
    a = ap.parse_args()
    run(a.look, a.seeds, a.out or ROOT / "out" / "parity" / a.look)


if __name__ == "__main__":
    main()
