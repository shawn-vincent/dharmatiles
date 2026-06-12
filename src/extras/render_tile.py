#!/usr/bin/env python3
"""Render a tile spec to a PNG using pyrender (proper Z-buffer, smooth shading).

Usage:
    python src/extras/render_tile.py src/tiles/ground/soil+grass.tile.py
    python src/extras/render_tile.py src/tiles/water/water+grass.tile.py [OUTPUT.png]
    python src/extras/render_tile.py SPEC.tile.py --elev 35 --azim -135

OUTPUT defaults to /tmp/<spec-stem>.png.
"""
from __future__ import annotations

import argparse
import pathlib
import sys

import colorsys

import numpy as np
import trimesh
from PIL import Image, ImageDraw


def _boost_saturation(rgba: np.ndarray, factor: float = 1.4) -> np.ndarray:
    h, s, v = colorsys.rgb_to_hsv(float(rgba[0]), float(rgba[1]), float(rgba[2]))
    r, g, b = colorsys.hsv_to_rgb(h, min(1.0, s * factor), v)
    return np.array([r, g, b, rgba[3]], dtype=np.float32)


def _tight_crop(img: Image.Image, alpha: np.ndarray, padding: int = 48) -> Image.Image:
    rows = np.any(alpha > 0, axis=1)
    cols = np.any(alpha > 0, axis=0)
    r0, r1 = np.where(rows)[0][[0, -1]]
    c0, c1 = np.where(cols)[0][[0, -1]]
    r0 = max(0, r0 - padding)
    r1 = min(img.height - 1, r1 + padding)
    c0 = max(0, c0 - padding)
    c1 = min(img.width - 1, c1 + padding)
    return img.crop((c0, r0, c1 + 1, r1 + 1))


def _rounded_corners(img: Image.Image, radius: int = 52) -> Image.Image:
    mask = Image.new('L', img.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [(0, 0), (img.width - 1, img.height - 1)], radius=radius, fill=255,
    )
    out = img.convert('RGBA')
    out.putalpha(mask)
    return out


def _split_by_color(mesh: trimesh.Trimesh) -> list[trimesh.Trimesh]:
    """Split a mesh into single-color submeshes so each gets its own material."""
    try:
        fc = mesh.visual.face_colors  # (N, 4) uint8
    except Exception:
        return [mesh]
    unique = np.unique(fc, axis=0)
    if len(unique) == 1:
        return [mesh]
    parts = []
    for color in unique:
        idx = np.where(np.all(fc == color, axis=1))[0]
        sub = mesh.submesh([idx], append=True)
        n = len(sub.faces)
        sub.visual = trimesh.visual.ColorVisuals(
            mesh=sub,
            face_colors=np.tile(color, (n, 1)).astype(np.uint8),
        )
        parts.append(sub)
    return parts


def _make_sky_gradient(width: int, height: int) -> Image.Image:
    # Deep azure at top → pale warm horizon at bottom, tilted ~12° from vertical
    top    = np.array([38, 110, 200], dtype=np.float32)
    bottom = np.array([178, 220, 248], dtype=np.float32)
    angle  = np.radians(12)
    ys = np.linspace(0, 1, height)[:, None]
    xs = np.linspace(-0.5, 0.5, width)[None, :]
    t  = np.clip(ys * np.cos(angle) + xs * np.sin(angle), 0, 1)[:, :, None]
    px = ((1 - t) * top + t * bottom).astype(np.uint8)
    return Image.fromarray(px, mode='RGB')


# ── Camera helper ─────────────────────────────────────────────────────────────

def _camera_pose(center: np.ndarray, distance: float,
                 elev_deg: float, azim_deg: float) -> np.ndarray:
    e = np.radians(elev_deg)
    a = np.radians(azim_deg)
    cam_pos = center + distance * np.array([
        np.cos(e) * np.cos(a),
        np.cos(e) * np.sin(a),
        np.sin(e),
    ])
    fwd = center - cam_pos
    fwd /= np.linalg.norm(fwd)
    up = np.array([0.0, 0.0, 1.0])
    right = np.cross(fwd, up)
    if np.linalg.norm(right) < 1e-6:
        up = np.array([0.0, 1.0, 0.0])
        right = np.cross(fwd, up)
    right /= np.linalg.norm(right)
    up = np.cross(right, fwd)
    pose = np.eye(4)
    pose[:3, 0] =  right
    pose[:3, 1] =  up
    pose[:3, 2] = -fwd
    pose[:3, 3] =  cam_pos
    return pose


# ── Renderer ──────────────────────────────────────────────────────────────────

def render(meshes: list[trimesh.Trimesh],
           output: pathlib.Path,
           elev: float = 45.0,
           azim: float = -135.0,
           resolution: tuple[int, int] = (1200, 1000)) -> None:
    import pyrender

    all_v  = np.vstack([m.vertices for m in meshes if len(m.vertices)])
    bounds = np.array([all_v.min(axis=0), all_v.max(axis=0)])
    center = bounds.mean(axis=0)
    diag   = np.linalg.norm(bounds[1] - bounds[0])

    scene = pyrender.Scene(
        ambient_light=np.array([0.15, 0.15, 0.15]),
        bg_color=np.array([0.0, 0.0, 0.0, 0.0]),
    )

    flat_meshes = [s for m in meshes for s in _split_by_color(m)]
    for m in flat_meshes:
        try:
            fc   = m.visual.face_colors[0]
            base = np.array([fc[0], fc[1], fc[2], 255], dtype=np.float32) / 255.0
        except Exception:
            base = np.array([0.55, 0.35, 0.15, 1.0], dtype=np.float32)

        base = _boost_saturation(base)
        mat = pyrender.MetallicRoughnessMaterial(
            baseColorFactor=base,
            metallicFactor=0.0,
            roughnessFactor=0.85,
        )
        pm = pyrender.Mesh.from_trimesh(m, material=mat, smooth=True)
        scene.add(pm)

    fov   = np.radians(40.0)
    dist  = diag / (2.0 * np.tan(fov / 2.0)) * 1.1
    cam   = pyrender.PerspectiveCamera(yfov=fov, znear=0.1, zfar=dist * 10)
    pose  = _camera_pose(center, dist, elev, azim)
    scene.add(cam, pose=pose)

    key  = _camera_pose(center, dist, 18, azim - 20)
    scene.add(pyrender.DirectionalLight(color=np.ones(3), intensity=3.5), pose=key)
    fill = _camera_pose(center, dist, elev - 20, azim + 110)
    scene.add(pyrender.DirectionalLight(color=np.ones(3), intensity=0.7), pose=fill)

    r = pyrender.OffscreenRenderer(*resolution)
    color, _ = r.render(scene, flags=pyrender.RenderFlags.RGBA)
    r.delete()

    fg      = Image.fromarray(color, mode='RGBA')
    alpha   = np.array(fg)[:, :, 3]
    bg      = _make_sky_gradient(*resolution)
    bg.paste(fg, (0, 0), fg)
    bg      = _tight_crop(bg, alpha)
    result  = _rounded_corners(bg)
    result.save(str(output))
    print(f'→ {output}')


# ── CLI ───────────────────────────────────────────────────────────────────────

def main() -> None:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('spec',   type=pathlib.Path, help='.tile.py spec file')
    p.add_argument('output', type=pathlib.Path, nargs='?')
    p.add_argument('--elev',   type=float, default=45.0)
    p.add_argument('--azim',   type=float, default=-135.0)
    p.add_argument('--width',  type=int,   default=1200)
    p.add_argument('--height', type=int,   default=1000)
    args = p.parse_args()

    spec = args.spec
    if not spec.exists():
        print(f'not found: {spec}', file=sys.stderr)
        sys.exit(1)

    stem = spec.stem.removesuffix('.tile')
    out  = args.output or pathlib.Path('/tmp') / (stem + '.png')

    print(f'Building {spec.name} …')
    from dharmatiles.terrains.tile import build_meshes_for_render
    meshes = build_meshes_for_render(spec)
    total  = sum(len(m.faces) for m in meshes)
    print(f'  {len(meshes)} mesh parts, {total:,} faces')

    render(meshes, out, elev=args.elev, azim=args.azim,
           resolution=(args.width, args.height))


if __name__ == '__main__':
    main()
