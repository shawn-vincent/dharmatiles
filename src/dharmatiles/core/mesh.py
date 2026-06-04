"""Low-level mesh primitives: frame computation, blade tube, terrain solid,
logo emboss, and coloured STL export.
"""
from __future__ import annotations

import pathlib
import struct

import numpy as np
import trimesh

# ── Logo asset path ───────────────────────────────────────────────────────────
_ASSETS_DIR = pathlib.Path(__file__).parent.parent / 'assets'
_LOGO_PNG   = _ASSETS_DIR / 'dharmatiles-logo.png'


# ── Logo emboss ───────────────────────────────────────────────────────────────

def _logo_mask(size_px: int = 64) -> np.ndarray:
    """Load the pre-rendered logo PNG and return a (size_px × size_px) bool mask.

    True = logo pixel (dark), False = background (light).
    Row 0 of the returned array is the TOP of the logo image.
    """
    from PIL import Image as _Image   # PIL is a trimesh transitive dep — always present
    img = _Image.open(_LOGO_PNG).convert('L')
    img = img.resize((size_px, size_px), _Image.LANCZOS)
    return np.array(img) < 128


def make_logo_emboss(x0: float, y0: float,
                     w: float, h: float,
                     z_base: float,
                     depth_mm: float = 0.4,
                     px: int = 64) -> trimesh.Trimesh:
    """Raised logo mesh suitable for concatenation with a base mesh.

    The logo is rasterised from the pre-rendered PNG at *px* × *px* resolution
    and centred in the [x0 .. x0+w] × [y0 .. y0+h] rectangle.  Raised columns
    protrude *depth_mm* outward from *z_base* (i.e. to *z_base − depth_mm*,
    extending further from the tile interior).

    The existing flat cap at *z_base* is left intact; this mesh adds only the
    protruding column solids (bottom + side walls + top cap per connected cell
    group).  Co-planar top faces with the base cap are harmless to slicers.

    Parameters
    ----------
    x0, y0    : bottom-left corner of the emboss rectangle in tile mm.
    w, h      : width and height of the rectangle in mm.
    z_base    : z of the flat base cap (e.g. -WALL_HEIGHT for OpenLOCK).
    depth_mm  : protrusion depth beyond z_base (positive → more negative z).
    px        : raster resolution (px × px cells over the w × h area).
    """
    mask = _logo_mask(px)          # (px, px) bool; row 0 = top of image = +Y side
    H, W = mask.shape
    cw = w / W
    ch = h / H
    z_bot = z_base - depth_mm

    verts: list[list[float]] = []
    faces: list[list[int]] = []

    def add_quad(a, b, c, d) -> None:
        i = len(verts)
        verts.extend([a, b, c, d])
        faces.extend([[i, i + 1, i + 2], [i, i + 2, i + 3]])

    for jj in range(H):
        for ii in range(W):
            if not mask[jj, ii]:
                continue

            # PNG row 0 = top of logo = tile +Y: flip row index vertically.
            xl = x0 + ii * cw;              xr = x0 + (ii + 1) * cw
            yb = y0 + (H - 1 - jj) * ch;   yt = y0 + (H - jj) * ch

            # Outer (bottom) face at z_bot, normal −Z
            add_quad([xl, yb, z_bot], [xr, yb, z_bot],
                     [xr, yt, z_bot], [xl, yt, z_bot])
            # Inner (top) face at z_base, normal +Z
            add_quad([xl, yt, z_base], [xr, yt, z_base],
                     [xr, yb, z_base], [xl, yb, z_base])

            # South wall (−Y edge, at yb): neighbour is jj+1 in PNG (lower tile-Y)
            if jj == H - 1 or not mask[jj + 1, ii]:
                add_quad([xr, yb, z_bot], [xl, yb, z_bot],
                         [xl, yb, z_base], [xr, yb, z_base])
            # North wall (+Y edge, at yt): neighbour is jj−1 in PNG (higher tile-Y)
            if jj == 0 or not mask[jj - 1, ii]:
                add_quad([xl, yt, z_bot], [xr, yt, z_bot],
                         [xr, yt, z_base], [xl, yt, z_base])
            # West wall (−X edge)
            if ii == 0 or not mask[jj, ii - 1]:
                add_quad([xl, yb, z_bot], [xl, yt, z_bot],
                         [xl, yt, z_base], [xl, yb, z_base])
            # East wall (+X edge)
            if ii == W - 1 or not mask[jj, ii + 1]:
                add_quad([xr, yt, z_bot], [xr, yb, z_bot],
                         [xr, yb, z_base], [xr, yt, z_base])

    if not verts:
        return trimesh.Trimesh()

    mesh = trimesh.Trimesh(
        vertices=np.array(verts, dtype=float),
        faces=np.array(faces, dtype=np.int32),
        process=False,
    )
    mesh.fix_normals()
    return mesh


# ── Coloured STL export ───────────────────────────────────────────────────────

def export_coloured_stl(mesh: trimesh.Trimesh, path: pathlib.Path) -> None:
    """Write *mesh* as a binary STL with VisCAM/SolidView per-face colours.

    Each explicitly coloured triangle's 2-byte attribute field encodes RGB in
    5-5-5 format::

        bit 15      = 1  (marks colour as valid)
        bits 14-10  = R  (0-31)
        bits  9-5   = G  (0-31)
        bits  4-0   = B  (0-31)

    Faces with alpha = 0 are exported with attribute 0, leaving their colour
    unspecified for viewers that honour the VisCAM/SolidView colour bit.
    Viewers that support the hack (PrusaSlicer, MeshLab, Windows 3D Builder …)
    will render each face in its assigned colour.  Viewers that ignore it see a
    normal single-colour STL.
    """
    faces   = mesh.faces                     # (N, 3) int
    verts   = mesh.vertices                  # (M, 3) float64
    normals = mesh.face_normals              # (N, 3) float64
    colours = mesh.visual.face_colors        # (N, 4) uint8  (RGBA)
    n       = len(faces)

    # Encode 8-bit channels to 5-bit and pack into uint16
    r5   = (colours[:, 0].astype(np.uint16) * 31 // 255)
    g5   = (colours[:, 1].astype(np.uint16) * 31 // 255)
    b5   = (colours[:, 2].astype(np.uint16) * 31 // 255)
    valid = colours[:, 3] > 0
    attr = np.where(
        valid,
        np.uint16(0x8000) | (r5 << 10) | (g5 << 5) | b5,
        np.uint16(0),
    )

    # Pack into a numpy structured array — one record per triangle (50 bytes)
    dtype = np.dtype([
        ('normal', '<f4', (3,)),
        ('v0',     '<f4', (3,)),
        ('v1',     '<f4', (3,)),
        ('v2',     '<f4', (3,)),
        ('attr',   '<u2'),
    ])
    tri_verts = verts[faces]                 # (N, 3, 3)
    data              = np.empty(n, dtype=dtype)
    data['normal']    = normals.astype('<f4')
    data['v0']        = tri_verts[:, 0].astype('<f4')
    data['v1']        = tri_verts[:, 1].astype('<f4')
    data['v2']        = tri_verts[:, 2].astype('<f4')
    data['attr']      = attr

    header = b'VisCAM coloured STL' + b'\x00' * 61   # must be exactly 80 bytes

    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, 'wb') as fh:
        fh.write(header)
        fh.write(struct.pack('<I', n))
        fh.write(data.tobytes())


# ── Frame computation ─────────────────────────────────────────────────────────

def blade_frame(path: np.ndarray):
    """Return (tangents, up_locs, down_locs) unit-vector arrays for each ring.

    ``down_locs`` points downward (away from the upper face), perpendicular to
    both the tangent and the world-horizontal up_loc.
    """
    path = np.asarray(path, dtype=float)
    tangs = np.empty_like(path)
    tangs[:-1] = path[1:] - path[:-1]
    tangs[-1]  = path[-1] - path[-2]
    t_norms    = np.linalg.norm(tangs, axis=1, keepdims=True) + 1e-9
    tangs     /= t_norms

    txy_norm = np.sqrt(tangs[:, 0]**2 + tangs[:, 1]**2)
    has_xy   = txy_norm > 1e-6
    up_locs  = np.zeros_like(path)
    up_locs[has_xy, 0] = -tangs[has_xy, 1] / txy_norm[has_xy]
    up_locs[has_xy, 1] =  tangs[has_xy, 0] / txy_norm[has_xy]
    up_locs[~has_xy]   = [1.0, 0.0, 0.0]

    down_locs = np.cross(up_locs, tangs)
    down_norms = np.linalg.norm(down_locs, axis=1, keepdims=True) + 1e-9
    down_locs /= down_norms
    down_locs[down_locs[:, 2] > 0.0] *= -1.0   # always point downward
    return tangs, up_locs, down_locs


# ── Blade tube mesh ───────────────────────────────────────────────────────────

def build_tube_mesh(spine_3d: np.ndarray, widths: np.ndarray,
                    thickness: float,
                    cross_section: str = 'triangle',
                    n_segs: int = 8,
                    diamond_equator: float = 0.75) -> trimesh.Trimesh:
    """Watertight tube mesh following *spine_3d*.

    cross_section='triangle' (default)
        3 verts / ring:
          V0 — lower hull apex  (spine + thickness * down_loc)
          V1 — right top edge   (spine + half_w * up_loc)
          V2 — left  top edge   (spine − half_w * up_loc)
        The top face (V1–V2 strip) sits on the support curve; the apex hangs
        *thickness* mm below it.

    cross_section='circle'
        n_segs verts / ring, uniformly distributed around the spine.
        Radius = half_width at each ring.  Vertex 0 is at the *up_loc* side
        (top), vertex n_segs//4 is at the *down_loc* side (bottom).
        The spine point is the tube centre, not the top surface.

    cross_section='diamond'
        4-vertex rhombus / diamond cross-section.
        Spine sits at the top apex; the blade widens at the equator then tapers
        back to a bottom keel apex, giving a ridge-backed, keel-bottomed blade.
    """
    path  = np.asarray(spine_3d, dtype=float)   # (n_pts, 3)
    W_arr = np.asarray(widths,   dtype=float)    # (n_pts,)
    n_pts = len(path)

    _, up_locs, down_locs = blade_frame(path)
    half_W = W_arr / 2.0                         # (n_pts,)

    if cross_section == 'triangle':
        n = 3
        ring_v = np.stack([
            path + thickness * down_locs,          # V0: lower apex
            path + half_W[:, None] * up_locs,      # V1: right edge
            path - half_W[:, None] * up_locs,      # V2: left  edge
        ], axis=1)                                 # (n_pts, 3, 3)

    elif cross_section == 'circle':
        n = max(3, int(n_segs))
        thetas = 2 * np.pi * np.arange(n) / n
        cos_t  = np.cos(thetas)
        sin_t  = np.sin(thetas)
        ring_v = (path[:, None, :] +
                  half_W[:, None, None] *
                  (cos_t[None, :, None] * up_locs[:, None, :] +
                   sin_t[None, :, None] * down_locs[:, None, :]))

    elif cross_section == 'diamond':
        n      = 4
        eq_d   = diamond_equator * thickness
        ring_v = np.stack([
            path,
            path + half_W[:, None] * up_locs   + eq_d * down_locs,
            path + thickness * down_locs,
            path - half_W[:, None] * up_locs   + eq_d * down_locs,
        ], axis=1)

    else:
        raise ValueError(
            f"Unknown cross_section {cross_section!r}; "
            "use 'triangle', 'circle', or 'diamond'"
        )

    nv = n * n_pts + 2
    nf = n + (n_pts - 1) * n * 2 + n
    verts = np.empty((nv, 3), dtype=float)
    faces = np.empty((nf, 3), dtype=np.int32)
    vi = fi = 0

    for i in range(n_pts):
        verts[vi:vi + n] = ring_v[i];  vi += n

    v_base = vi;  verts[vi] = path[0];   vi += 1
    v_tip  = vi;  verts[vi] = path[-1];  vi += 1

    for i in range(n):
        faces[fi] = [v_base, i, (i + 1) % n];  fi += 1

    for k in range(n_pts - 1):
        ra = k * n;  rb = (k + 1) * n
        for i in range(n):
            i1 = (i + 1) % n
            faces[fi]   = [ra + i,  rb + i,  ra + i1];  fi += 1
            faces[fi]   = [ra + i1, rb + i,  rb + i1];  fi += 1

    rl = (n_pts - 1) * n
    if cross_section == 'circle':
        # Circle side-face winding is already outward-pointing by construction.
        # Only the top cap needs a reversed winding to match:
        # [v_tip, rl+(i+1)%n, rl+i] gives normal along +tangent (outward at tip).
        for i in range(n):
            faces[fi] = [v_tip, rl + (i + 1) % n, rl + i];  fi += 1
    else:
        for i in range(n):
            faces[fi] = [rl + i, rl + (i + 1) % n, v_tip];  fi += 1

    mesh = trimesh.Trimesh(vertices=verts[:vi],
                           faces=faces[:fi].astype(int),
                           process=False)
    if cross_section != 'circle':
        # Triangle and diamond cross-sections have inconsistent face winding
        # from the generator and require a topology-based normal fix.
        # Circle is fully consistent after the cap correction above.
        mesh.fix_normals()
    return mesh


# ── Terrain solid ─────────────────────────────────────────────────────────────

def make_heightmap_solid(z_grid: np.ndarray, tile_w: float, tile_h: float,
                         base_h: float, subsample: int = 1,
                         omit_top_mask: np.ndarray | None = None,
                         ) -> trimesh.Trimesh:
    """Watertight solid: top = *z_grid* surface, bottom = flat at −*base_h*.

    Parameters
    ----------
    z_grid   : (grid_h, grid_w) terrain heights in mm.
    tile_w/h : tile dimensions in mm.
    base_h   : depth of the solid slab below terrain in mm (positive value).
    subsample: take every Nth grid sample for the mesh (1 = full resolution).
    omit_top_mask: optional bool grid. Top quads whose four sampled corners are
        True are omitted so another explicit layer, such as water, can cap that
        region instead of duplicating coplanar terrain faces.
    """
    nrows, ncols = z_grid.shape
    sr_arr = np.arange(0, ncols, subsample)
    if sr_arr[-1] != ncols - 1:
        sr_arr = np.append(sr_arr, ncols - 1)
    sc_arr = np.arange(0, nrows, subsample)
    if sc_arr[-1] != nrows - 1:
        sc_arr = np.append(sc_arr, nrows - 1)
    ns_c = len(sr_arr)   # sampled cols
    ns_r = len(sc_arr)   # sampled rows
    gx   = tile_w / max(ncols - 1, 1)
    gy   = tile_h / max(nrows - 1, 1)

    # ── Build all vertices in one shot ────────────────────────────────────────
    # Top: ns_r × ns_c vertices.  Bottom: same layout at z = -base_h.
    # Flat index: top[jj, ii] = jj*ns_c + ii;  bot[jj, ii] = ns_r*ns_c + jj*ns_c + ii
    n_top_v  = ns_r * ns_c
    n_verts  = 2 * n_top_v
    verts    = np.empty((n_verts, 3), dtype=float)

    JJ, II   = np.mgrid[0:ns_r, 0:ns_c]
    I_idx    = sr_arr[II]    # actual column indices into z_grid
    J_idx    = sc_arr[JJ]    # actual row indices

    verts[:n_top_v, 0] = (I_idx * gx).ravel()
    verts[:n_top_v, 1] = (J_idx * gy).ravel()
    verts[:n_top_v, 2] = z_grid[J_idx, I_idx].ravel()

    verts[n_top_v:, 0] = verts[:n_top_v, 0]
    verts[n_top_v:, 1] = verts[:n_top_v, 1]
    verts[n_top_v:, 2] = -base_h

    # Flat vertex index helpers
    def top_v(ii, jj): return jj * ns_c + ii
    def bot_v(ii, jj): return n_top_v + jj * ns_c + ii

    # ── Build all faces vectorised ────────────────────────────────────────────
    # Grid of quad corners: (ns_r-1) × (ns_c-1) quads
    q_r, q_c = np.mgrid[0:ns_r - 1, 0:ns_c - 1]   # quad row/col indices

    # Flat vertex indices for quad corners (broadcast over all quads at once)
    a_top = top_v(q_c,     q_r    )   # (ns_r-1, ns_c-1) each
    b_top = top_v(q_c + 1, q_r    )
    c_top = top_v(q_c,     q_r + 1)
    d_top = top_v(q_c + 1, q_r + 1)

    # ── Top surface ───────────────────────────────────────────────────────────
    if omit_top_mask is not None:
        # Omit quads where all four sampled corners are masked
        j0 = sc_arr[q_r];      j1 = sc_arr[q_r + 1]
        i0 = sr_arr[q_c];      i1 = sr_arr[q_c + 1]
        omit = (omit_top_mask[j0, i0] & omit_top_mask[j0, i1] &
                omit_top_mask[j1, i0] & omit_top_mask[j1, i1])
        keep = ~omit
        a_k, b_k, c_k, d_k = (a_top[keep], b_top[keep],
                                c_top[keep], d_top[keep])
    else:
        a_k, b_k, c_k, d_k = (a_top.ravel(), b_top.ravel(),
                                c_top.ravel(), d_top.ravel())

    n_top_quads = len(a_k)
    top_faces = np.empty((n_top_quads * 2, 3), dtype=np.int32)
    top_faces[0::2] = np.stack([a_k, b_k, d_k], axis=1)
    top_faces[1::2] = np.stack([a_k, d_k, c_k], axis=1)

    # ── Bottom surface ────────────────────────────────────────────────────────
    a_bot = bot_v(q_c,     q_r    ).ravel()
    b_bot = bot_v(q_c + 1, q_r    ).ravel()
    c_bot = bot_v(q_c,     q_r + 1).ravel()
    d_bot = bot_v(q_c + 1, q_r + 1).ravel()
    n_bot_quads = len(a_bot)
    bot_faces = np.empty((n_bot_quads * 2, 3), dtype=np.int32)
    bot_faces[0::2] = np.stack([a_bot, d_bot, b_bot], axis=1)
    bot_faces[1::2] = np.stack([a_bot, c_bot, d_bot], axis=1)

    # ── Side walls ────────────────────────────────────────────────────────────
    ii_sw = np.arange(ns_c - 1)
    jj_sw = np.arange(ns_r - 1)

    # South wall (jj=0): 2 triangles per column segment
    sw_s = np.empty((2 * (ns_c - 1), 3), dtype=np.int32)
    sw_s[0::2] = np.stack([top_v(ii_sw, 0),     bot_v(ii_sw, 0),     top_v(ii_sw + 1, 0)], axis=1)
    sw_s[1::2] = np.stack([top_v(ii_sw + 1, 0), bot_v(ii_sw, 0),     bot_v(ii_sw + 1, 0)], axis=1)

    # North wall (jj=ns_r-1)
    sw_n = np.empty((2 * (ns_c - 1), 3), dtype=np.int32)
    sw_n[0::2] = np.stack([top_v(ii_sw, ns_r-1), top_v(ii_sw+1, ns_r-1), bot_v(ii_sw, ns_r-1)],   axis=1)
    sw_n[1::2] = np.stack([top_v(ii_sw+1, ns_r-1),bot_v(ii_sw+1, ns_r-1),bot_v(ii_sw, ns_r-1)],   axis=1)

    # West wall (ii=0)
    sw_w = np.empty((2 * (ns_r - 1), 3), dtype=np.int32)
    sw_w[0::2] = np.stack([top_v(0, jj_sw),     top_v(0, jj_sw + 1), bot_v(0, jj_sw)],     axis=1)
    sw_w[1::2] = np.stack([top_v(0, jj_sw + 1), bot_v(0, jj_sw + 1), bot_v(0, jj_sw)],     axis=1)

    # East wall (ii=ns_c-1)
    sw_e = np.empty((2 * (ns_r - 1), 3), dtype=np.int32)
    sw_e[0::2] = np.stack([top_v(ns_c-1, jj_sw),  bot_v(ns_c-1, jj_sw),  top_v(ns_c-1, jj_sw+1)], axis=1)
    sw_e[1::2] = np.stack([top_v(ns_c-1, jj_sw+1),bot_v(ns_c-1, jj_sw),  bot_v(ns_c-1, jj_sw+1)], axis=1)

    faces = np.concatenate([top_faces, bot_faces, sw_s, sw_n, sw_w, sw_e])

    mesh = trimesh.Trimesh(vertices=verts,
                           faces=faces.astype(int),
                           process=False)
    mesh.metadata['top_face_count'] = n_top_quads * 2
    mesh.fix_normals()
    return mesh

