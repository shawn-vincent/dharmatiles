"""Off-screen PNG rendering for DharmaTiles terrain tiles.

Shared by ``src/extras/render_tile.py`` (CLI) and the ``generate-tile-stl``
batch pipeline (automatic PNG output after STL generation).
"""
from __future__ import annotations

import colorsys
import pathlib

import numpy as np
import trimesh
from PIL import Image, ImageDraw, ImageFilter


def _boost_saturation(rgba: np.ndarray, factor: float = 1.4) -> np.ndarray:
    h, s, v = colorsys.rgb_to_hsv(float(rgba[0]), float(rgba[1]), float(rgba[2]))
    r, g, b = colorsys.hsv_to_rgb(h, min(1.0, s * factor), v)
    return np.array([r, g, b, rgba[3]], dtype=np.float32)


def _split_by_color(mesh: trimesh.Trimesh) -> list[trimesh.Trimesh]:
    """Split a mesh into single-color submeshes so each gets its own material."""
    try:
        fc = mesh.visual.face_colors
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


def _load_label_font(size: int):
    from PIL import ImageFont
    candidates = [
        ("/System/Library/Fonts/HelveticaNeue.ttc", 0),   # Helvetica Neue Regular
        ("/System/Library/Fonts/SFNS.ttf",           0),
        ("/System/Library/Fonts/Helvetica.ttc",      0),
        ("/Library/Fonts/Arial.ttf",                 0),
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 0),
        ("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf", 0),
    ]
    for path, index in candidates:
        try:
            return ImageFont.truetype(path, size, index=index)
        except (OSError, IOError):
            continue
    return ImageFont.load_default()


def _load_label_font_bold(size: int):
    from PIL import ImageFont
    candidates = [
        ("/System/Library/Fonts/HelveticaNeue.ttc", 1),   # Helvetica Neue Bold
        ("/System/Library/Fonts/Avenir.ttc",         3),   # Avenir Heavy
        ("/Library/Fonts/Arial Bold.ttf",            0),
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 0),
    ]
    for path, index in candidates:
        try:
            return ImageFont.truetype(path, size, index=index)
        except (OSError, IOError):
            continue
    return _load_label_font(size)


def _load_label_font_light(size: int):
    from PIL import ImageFont
    candidates = [
        ("/System/Library/Fonts/HelveticaNeue.ttc", 7),   # Helvetica Neue Light
        ("/System/Library/Fonts/Avenir.ttc",         6),   # Avenir Light
    ]
    for path, index in candidates:
        try:
            return ImageFont.truetype(path, size, index=index)
        except (OSError, IOError):
            continue
    return _load_label_font(size)


def _load_logo_icon(size: int) -> Image.Image:
    """Dharmatiles logo resized to size×size, white-on-transparent, whitespace cropped."""
    import pathlib
    path = pathlib.Path(__file__).parent / 'assets' / 'dharmatiles-logo.png'
    src  = Image.open(path).convert('RGBA')
    arr  = np.array(src)
    lum  = arr[:, :, :3].mean(axis=2)
    # Crop to the bounding box of dark (logo) pixels so whitespace doesn't shrink it
    dark = lum < 200
    rows = np.any(dark, axis=1)
    cols = np.any(dark, axis=0)
    r0, r1 = np.where(rows)[0][[0, -1]]
    c0, c1 = np.where(cols)[0][[0, -1]]
    src  = src.crop((c0, r0, c1 + 1, r1 + 1))
    arr  = np.array(src)
    lum  = arr[:, :, :3].mean(axis=2)
    out  = np.zeros_like(arr)
    out[:, :, :3] = 255
    out[:, :, 3]  = np.clip(255 - lum, 0, 255).astype(np.uint8)
    return Image.fromarray(out, 'RGBA').resize((size, size), Image.LANCZOS)


def _add_label_overlay(img: Image.Image, label: str) -> Image.Image:
    """Single-line pill badge — equal gap left|icon↔text|right and vertically centered."""
    img = img.convert('RGBA')
    iw, ih = img.size

    slash = label.find('/')
    if slash >= 0:
        prefix = label[:slash + 1]
        name   = label[slash + 1:]
    else:
        prefix = ""
        name   = label

    font_size  = max(16, ih // 18)
    font       = _load_label_font_bold(font_size)
    font_light = _load_label_font_light(font_size)

    probe = ImageDraw.Draw(img)

    def _bb(text: str, f=None):
        return probe.textbbox((0, 0), text, font=f or font)

    def _w(text: str, f=None) -> int:
        b = _bb(text, f); return b[2] - b[0]

    def _h(text: str, f=None) -> int:
        b = _bb(text, f); return b[3] - b[1]

    pw    = _w(prefix, font_light) if prefix else 0
    nw    = _w(name)
    nh    = _h(name)
    top_y = _bb(name)[1]

    gap    = max(6, font_size // 5)
    margin = max(14, ih // 45)
    radius = max(6,  font_size // 4)

    icon_size = int(nh * 1.15)         # slightly taller than text
    rect_h    = icon_size + gap * 2    # pill height driven by icon + uniform padding
    try:
        icon = _load_logo_icon(icon_size)
    except Exception:
        icon = None

    icon_w = icon_size if icon is not None else 0
    text_w = pw + nw
    rect_w = gap + (icon_w + gap if icon is not None else 0) + text_w + gap

    x1 = margin
    y1 = ih - margin - rect_h
    x2 = x1 + rect_w
    y2 = y1 + rect_h

    # Frosted glass: blur the region behind the pill, then tint it
    region  = img.crop((x1, y1, x2, y2)).filter(ImageFilter.GaussianBlur(radius=12))
    tint    = Image.new('RGBA', (x2 - x1, y2 - y1), (6, 8, 14, 155))
    frosted = Image.alpha_composite(region.convert('RGBA'), tint)
    pmask   = Image.new('L', (x2 - x1, y2 - y1), 0)
    ImageDraw.Draw(pmask).rounded_rectangle(
        [(0, 0), (x2 - x1 - 1, y2 - y1 - 1)], radius=radius, fill=255)
    img.paste(frosted, (x1, y1), mask=pmask)

    if icon is not None:
        icon_y = y1 + (rect_h - icon_size) // 2
        img.alpha_composite(icon, dest=(x1 + gap, icon_y))

    draw = ImageDraw.Draw(img)
    tx   = x1 + gap + (icon_w + gap if icon is not None else 0)
    ty   = y1 + (rect_h - nh) // 2 - top_y + gap // 2
    if prefix:
        draw.text((tx, ty), prefix, font=font_light, fill=(255, 255, 255, 235))
        tx += pw
    draw.text((tx, ty), name, font=font, fill=(255, 255, 255, 235))
    return img


def _rounded_corners(img: Image.Image, radius: int = 52) -> Image.Image:
    mask = Image.new('L', img.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [(0, 0), (img.width - 1, img.height - 1)], radius=radius, fill=255,
    )
    out = img.convert('RGBA')
    out.putalpha(mask)
    return out


def _make_grid_mesh(x_min: float, x_max: float, y_min: float, y_max: float,
                    z: float, square_mm: float,
                    ex_xn: int = 2, ex_xp: int = 2,
                    ex_yn: int = 2, ex_yp: int = 2) -> trimesh.Trimesh:
    """Flat gray grid at *z*, lines spaced *square_mm*.

    ex_xn/ex_xp: extra squares in -X / +X direction.
    ex_yn/ex_yp: extra squares in -Y / +Y direction.
    """
    line_w = max(0.4, square_mm * 0.012)
    line_t = 0.08

    gx_min = x_min - ex_xn * square_mm
    gx_max = x_max + ex_xp * square_mm
    gy_min = y_min - ex_yn * square_mm
    gy_max = y_max + ex_yp * square_mm

    cols = int(round((x_max - x_min) / square_mm))
    rows = int(round((y_max - y_min) / square_mm))

    parts = []
    cx = (gx_min + gx_max) / 2
    cy = (gy_min + gy_max) / 2
    z_center = z + line_t / 2

    for i in range(-ex_xn, cols + ex_xp + 1):
        x = x_min + i * square_mm
        b = trimesh.creation.box(extents=[line_w, gy_max - gy_min + line_w, line_t])
        b.apply_translation([x, cy, z_center])
        parts.append(b)

    for j in range(-ex_yn, rows + ex_yp + 1):
        y = y_min + j * square_mm
        b = trimesh.creation.box(extents=[gx_max - gx_min + line_w, line_w, line_t])
        b.apply_translation([cx, y, z_center])
        parts.append(b)

    grid = trimesh.util.concatenate(parts)
    gray = np.array([150, 150, 150, 255], dtype=np.uint8)
    grid.visual.face_colors = np.tile(gray, (len(grid.faces), 1))
    return grid


def _make_sky_gradient(width: int, height: int) -> Image.Image:
    top    = np.array([38, 110, 200], dtype=np.float32)
    bottom = np.array([178, 220, 248], dtype=np.float32)
    angle  = np.radians(12)
    ys = np.linspace(0, 1, height)[:, None]
    xs = np.linspace(-0.5, 0.5, width)[None, :]
    t  = np.clip(ys * np.cos(angle) + xs * np.sin(angle), 0, 1)[:, :, None]
    px = ((1 - t) * top + t * bottom).astype(np.uint8)
    return Image.fromarray(px, mode='RGB')


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


def render(meshes: list[trimesh.Trimesh],
           output: pathlib.Path,
           elev: float = 45.0,
           azim: float = -135.0,
           resolution: tuple[int, int] = (1200, 1000),
           quiet: bool = False,
           grid_square_mm: float | None = None,
           label: str | None = None) -> None:
    import pyrender  # optional dependency

    all_v  = np.vstack([m.vertices for m in meshes if len(m.vertices)])
    bounds = np.array([all_v.min(axis=0), all_v.max(axis=0)])
    center = bounds.mean(axis=0)
    diag   = np.linalg.norm(bounds[1] - bounds[0])

    scene = pyrender.Scene(
        ambient_light=np.array([0.15, 0.15, 0.15]),
        bg_color=np.array([0.0, 0.0, 0.0, 0.0]),
    )

    if grid_square_mm is not None:
        # Far side (away from camera) gets more extension; sides/near also generous.
        fwd_x = -np.cos(np.radians(azim))
        fwd_y = -np.sin(np.radians(azim))
        near, far = 6, 20
        grid = _make_grid_mesh(
            bounds[0][0], bounds[1][0],
            bounds[0][1], bounds[1][1],
            0.0,
            grid_square_mm,
            ex_xn=far if fwd_x < 0 else near,
            ex_xp=far if fwd_x > 0 else near,
            ex_yn=far if fwd_y < 0 else near,
            ex_yp=far if fwd_y > 0 else near,
        )
        gmat = pyrender.MetallicRoughnessMaterial(
            baseColorFactor=np.array([0.60, 0.60, 0.60, 1.0], dtype=np.float32),
            metallicFactor=0.0,
            roughnessFactor=1.0,
        )
        scene.add(pyrender.Mesh.from_trimesh(grid, material=gmat, smooth=False))

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

    fg     = Image.fromarray(color, mode='RGBA')
    alpha  = np.array(fg)[:, :, 3]
    bg     = _make_sky_gradient(*resolution)
    bg.paste(fg, (0, 0), fg)
    bg     = _tight_crop(bg, alpha)
    result = _rounded_corners(bg)
    if label:
        result = _add_label_overlay(result, label)
    result.save(str(output))
    if not quiet:
        print(f'→ {output}')
