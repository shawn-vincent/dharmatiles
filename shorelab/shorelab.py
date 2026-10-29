#!/usr/bin/env python3
"""shorelab — a meadow meeting swell-chaos water on one DungeonBlocks tile.

Composes grasslab (hills, grass), rocklab (rocks), meadowlab (rock/grass
interaction) and waterlab (swell-chaos water) around a real bank, plus
shore-only features: a variable wet-mud strip, shore-steered grass, reeds.
Design: shorelab/DESIGN.md.

Usage:
    python shorelab/shorelab.py [--seed N]
    python shorelab/shorelab.py --sheet --seeds 1 2 3     # timestamped contact sheet
"""
from __future__ import annotations

import argparse
import datetime
import pathlib
import sys
import tempfile
import time

import manifold3d as m3d
import numpy as np
import trimesh
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parent.parent
for lab in ('grasslab', 'rocklab', 'meadowlab', 'waterlab'):
    sys.path.insert(0, str(ROOT / lab))
import grasslab as gl    # noqa: E402
import rocklab as rl     # noqa: E402
import meadowlab as ml   # noqa: E402
import waterlab as wl    # noqa: E402

# ── Bank ─────────────────────────────────────────────────────────────────────
SHORE_X_MM      = 33.0         # mean shoreline position (land on the left)
SHORE_WOBBLE_MM = 11.0         # bays and headlands
SHORE_WOBBLE_SIGMA_MM = 14.0
SHORE_COVE_MM   = 5.0          # smaller coves
SHORE_COVE_SIGMA_MM = 5.5
BANK_W_MM       = 8.0          # bank ramp width
BANK_SHIFT_MM   = 3.0          # ramp centre offset into the water
BED_MM          = 0.4          # underwater bed height (≈ waterlab WL − BED_DEPTH)

# ── Mud strip ────────────────────────────────────────────────────────────────
MUD_MAX_MM      = 4.0          # strip width where it is widest
MUD_SIGMA_MM    = 9.0          # along-shore variation scale
MUD_BIAS        = 0.0        # < 0 → strip absent along part of the shore
FREE_LENSES_PER_SQ = (0.35, 0.6)

# ── Shore steering (grass leans to the water) ────────────────────────────────
SHORE_STEER_MM  = 9.0
SHORE_OK_DEG    = 80.0
SHORE_PULL      = 0.3
SHORE_SMOOTH_MM = 1.5

# ── Rocks ────────────────────────────────────────────────────────────────────
ROCK_PLACEMENTS = (6, 8)
WET_SURFACE_MM  = 0.6          # wet rocks must show above WL + this

# ── Reeds ────────────────────────────────────────────────────────────────────
REED_CLUMPS     = (3, 5)
REED_SEP_MM     = 12.0
REED_BAND_MM    = (-1.1, 0.2)  # ground height relative to WL where reeds root
REED_STEMS      = (5, 12)
REED_SPREAD_MM  = 0.35         # blades rise from a near-common base…
REED_FAN_DEG    = (14.0, 34.0) # …and curve outward from it (tip angle)
REED_H_MM       = (8.0, 13.0)
REED_HALF_W_MM  = (1.3, 1.8)   # blade half-width at the base (chunky)
REED_THICK      = (0.35, 0.45) # half-thickness / half-width (lemon section)
REED_TIP_SCALE  = (0.25, 0.35) # section scale at the tip: a blunt point
REED_LEAN_DEG   = 8.0          # extra lean of the whole clump toward the water

WATER_BURY_MM   = 0.3

# ── Grass look for the shore (overrides grasslab's module constants here only;
#    grasslab/meadowlab keep their own) ─────────────────────────────────────
GRASS_OVERRIDES = dict(
    FROND_LEN_MM=(3.5, 6.5),     # shorter blades (grasslab: 5.5–9.5)
    FROND_H_MM=(0.95, 1.35),     # blades stand prouder of the skirt…
    SKIRT_FACTOR=1.55,           # …and the skirt fills less between them
    DOME_EXP=1.5,                # steeper blade flanks → deeper crevasses
)
# Smoother (less sharp) ripples on the shore tile; waterlab keeps its own.
WATER_OVERRIDES = dict(SWELL_SHORT_SHARPEN=0.45)
WATER_SOFTEN_MM = 0.3            # extra light blur of the wave relief
for _k, _v in WATER_OVERRIDES.items():
    assert hasattr(wl, _k), _k
    setattr(wl, _k, _v)
for _k, _v in GRASS_OVERRIDES.items():
    assert hasattr(gl, _k), _k
    setattr(gl, _k, _v)


def smooth01(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


# ── Ground ───────────────────────────────────────────────────────────────────
def make_shore_ground(rng, W, H):
    """grasslab hills on the land, ramped down across a wobbly bank to the bed."""
    hills, gstats = gl.make_ground(rng, W, H)
    nx, ny = hills.data.shape
    xs = hills.x0 + np.arange(nx) * hills.res
    ys = hills.y0 + np.arange(ny) * hills.res
    X, Y = np.meshgrid(xs, ys, indexing='ij')
    def wobble(sigma):
        w = ndimage.gaussian_filter1d(rng.standard_normal(ny), sigma / hills.res)
        return np.clip(w / (2.0 * w.std() + 1e-9), -1, 1)   # typical swing, not one peak
    # Two scales: bays / headlands, plus smaller coves.
    xs_ = (SHORE_X_MM + SHORE_WOBBLE_MM * wobble(SHORE_WOBBLE_SIGMA_MM)
           + SHORE_COVE_MM * wobble(SHORE_COVE_SIGMA_MM))
    s = xs_[None, :] - X                                           # > 0 on land
    ramp = smooth01((s + BANK_SHIFT_MM) / BANK_W_MM)
    G = BED_MM + (hills.data - BED_MM) * ramp
    return gl.Field2D(G, hills.x0, hills.y0, hills.res), gstats


# ── Soil: free lenses on land + a variable wet-mud strip at the waterline ────
class ShoreSoil:
    def __init__(self, rng, ground, lenses):
        self.lenses = lenses
        G = ground.data
        res = ground.res
        land = G >= wl.WL_MM
        self.dist = gl.Field2D(ndimage.distance_transform_edt(land) * res,
                               ground.x0, ground.y0, res)       # land → waterline
        n = gl._smooth_noise(rng, G.shape, MUD_SIGMA_MM / res)
        self.width = gl.Field2D(MUD_MAX_MM * np.clip(n * 0.5 + MUD_BIAS, 0, 1) ** 0.8,
                                ground.x0, ground.y0, res)
        self.ground = ground

    def mud(self, X, Y):
        on_land = self.ground(X, Y) >= wl.WL_MM
        t = smooth01(0.5 + (self.width(X, Y) - self.dist(X, Y)) / gl.SOIL_EDGE_MM)
        return np.where(on_land, t * (self.width(X, Y) > 0.2), 0.0)

    def mask(self, X, Y):
        return np.maximum(self.lenses.mask(X, Y), self.mud(X, Y))


def land_only(ground):
    """Ground as seen by lens placement: water scores as high (never chosen)."""
    def f(x, y):
        g = ground(x, y)
        return np.where(g < wl.WL_MM + 0.6, 99.0, g)
    return f


# ── Grass flow: lean toward the water near the shore ────────────────────────
def steer_shore(flow, ground):
    X, Y = flow.grid()
    G = ground.data
    res = flow.res
    land = G >= wl.WL_MM
    dw = ndimage.distance_transform_edt(land) * res               # distance to water
    gy, gx = np.gradient(-dw, res, axis=1), np.gradient(-dw, res, axis=0)
    out = np.arctan2(gy, gx)                                       # toward the water
    th = np.arctan2(flow.vy, flow.vx)
    w = smooth01(1 - dw / SHORE_STEER_MM) * land
    phi = gl._wrap(th - out)
    a = np.abs(phi)
    ok = np.radians(SHORE_OK_DEG)
    a = np.where(a > ok, ok + (a - ok) * (1 - w), a) * (1 - SHORE_PULL * w)
    delta = gl._wrap(out + np.sign(phi) * a - th)
    delta = ndimage.gaussian_filter(delta, SHORE_SMOOTH_MM / res)
    th = th + delta
    return gl.Flow(np.cos(th), np.sin(th), flow.x0, flow.y0, res)


# ── Reeds ────────────────────────────────────────────────────────────────────
def make_reeds(rng, ground, W, H):
    """Clumps of tapered stems at the waterline, leaning toward the water."""
    cand = rng.uniform([4, 4], [W - 4, H - 4], size=(4000, 2))
    g = ground(cand[:, 0], cand[:, 1]) - wl.WL_MM
    cand = cand[(g > REED_BAND_MM[0]) & (g < REED_BAND_MM[1])]
    centres = []
    n = int(rng.integers(REED_CLUMPS[0], REED_CLUMPS[1] + 1))
    for c in cand[rng.permutation(len(cand))]:
        if all(np.hypot(*(c - q)) >= REED_SEP_MM for q in centres):
            centres.append(c)
        if len(centres) == n:
            break
    gx_f = lambda x, y, e=0.3: (ground(x + e, y) - ground(x - e, y)) / (2 * e)
    gy_f = lambda x, y, e=0.3: (ground(x, y + e) - ground(x, y - e)) / (2 * e)
    stems, bases = [], []
    for c in centres:
        k = int(rng.integers(REED_STEMS[0], REED_STEMS[1] + 1))
        dx, dy = -float(gx_f(*c)), -float(gy_f(*c))                    # downhill = to water
        nw = np.hypot(dx, dy) + 1e-9
        ang0 = rng.uniform(0, 2 * np.pi)
        for j in range(k):
            fan_a = ang0 + 2 * np.pi * j / k + rng.normal(0, 0.35)   # fan direction
            p = c + REED_SPREAD_MM * np.array([np.cos(fan_a), np.sin(fan_a)]) * rng.random()
            h = rng.uniform(*REED_H_MM)
            hw = rng.uniform(*REED_HALF_W_MM)                       # blade half-width
            ht = hw * rng.uniform(*REED_THICK)                      # half-thickness
            # Lemon cross-section (pointed at the two edges), tapering only
            # moderately to a blunt point: chunky so it won't snap off.
            t_ = np.linspace(0, 2 * np.pi, 24, endpoint=False)
            sec = np.c_[hw * np.cos(t_), ht * np.sign(np.sin(t_)) * np.abs(np.sin(t_)) ** 1.4]
            stem = m3d.Manifold.extrude(m3d.CrossSection([sec.tolist()]), h + 0.8,
                                        n_divisions=16, scale_top=REED_TIP_SCALE)
            # Flat face toward the clump centre (wide axis tangential).
            stem = stem.rotate([0.0, 0.0, float(np.degrees(fan_a) + 90 + rng.normal(0, 15))])
            fan = rng.uniform(*REED_FAN_DEG)
            ux = fan * np.cos(fan_a) + REED_LEAN_DEG * dx / nw
            uy = fan * np.sin(fan_a) + REED_LEAN_DEG * dy / nw
            # Gentle bend: vertical at the base (blades rise together), curving
            # out to the fan angle at the tip — offset ∝ z², tip slope = tan(fan).
            tip = np.radians(np.hypot(ux, uy))
            dirx, diry = ux / (np.hypot(ux, uy) + 1e-9), uy / (np.hypot(ux, uy) + 1e-9)
            kk = np.tan(tip) / (2 * (h + 0.8))
            stem = stem.warp_batch(lambda v, kk=kk, dx_=dirx, dy_=diry: np.c_[
                v[:, 0] + dx_ * kk * v[:, 2] ** 2, v[:, 1] + dy_ * kk * v[:, 2] ** 2, v[:, 2]])
            z0 = float(ground(*p)) - 0.8
            stems.append(stem.translate([float(p[0]), float(p[1]), z0]))
            bases.append((float(p[0]), float(p[1]), hw))
    reeds = m3d.Manifold.batch_boolean(stems, m3d.OpType.Add) if stems else None
    return reeds, len(centres), len(stems), np.array(bases).reshape(-1, 3)


# ── Water ────────────────────────────────────────────────────────────────────
def build_water(seed, ground, wet_rocks, reed_bases=None):
    """swell-chaos water over the bank; returns (Manifold, waterlab scene, z)."""
    from dharmatiles.core.mesh import make_heightmap_solid
    n = int(round(wl.COLS * wl.SQUARE_MM / wl.CELL_MM)) + 1
    x = np.linspace(0, wl.COLS * wl.SQUARE_MM, n)
    Xw, Yw = np.meshgrid(x, x)                                     # rows = y
    G = ground(Xw, Yw)
    sc = wl.Scene.from_fields(seed, G, wet_rocks, scenario='shore')
    if reed_bases is not None and len(reed_bases):
        rm = np.zeros(Xw.shape, bool)
        for bx, by, br in reed_bases:                       # stem footprints at the surface
            rm |= (Xw - bx) ** 2 + (Yw - by) ** 2 <= (br + 0.15) ** 2
        sc.reed_mask = rm
    relief = wl.gen_swell_chaos(wl.mode_rng(seed, 'swell-chaos'), sc)
    relief = ndimage.gaussian_filter(relief, WATER_SOFTEN_MM / wl.CELL_MM)
    zw = wl.WL_MM + relief * sc.rim * sc.water * wl.smooth01(sc.d / 0.4) + sc.tide
    # Water only in the basin (+ a thin margin up the bank): the tide must not
    # flood low spots on land into puddles.
    basin = ndimage.binary_dilation(sc.water, iterations=int(0.6 / wl.CELL_MM))
    z = np.where(basin & (zw > G), zw, G - WATER_BURY_MM)
    z = np.clip(ndimage.gaussian_filter(z, 0.6), 0.05, None)
    top = make_heightmap_solid(z, sc.W, sc.H, base_h=0.0, subsample=wl.MESH_SUBSAMPLE)
    return rl.to_manifold(top), sc, zw


# ── Build ────────────────────────────────────────────────────────────────────
class Parts:
    pass


def build(seed, cols=2, rows=2):
    t0 = time.perf_counter()
    P = Parts()
    rng = np.random.default_rng(seed)
    rrng = np.random.default_rng(seed + ml.ROCK_SEED_OFFSET)
    W, H = cols * gl.SQUARE_MM, rows * gl.SQUARE_MM
    grid = gl.Grid(W, H, W / round(W / gl.VOX_MM))
    P.W, P.H, P.grid = W, H, grid

    ground, gstats = make_shore_ground(rng, W, H)
    flow0 = gl.make_direction(rng, ground)
    sw = gl.Swaths(rng, W, H, flow0)
    posed = rl.pose(rrng, rl.scatter(rrng, W, H, ROCK_PLACEMENTS))
    masks = ml.footprint_masks(posed, grid)
    foot = ml.footprint_mask(masks, (grid.nx, grid.ny))
    free = gl.place_lenses(rng, land_only(ground), flow0, W, H, FREE_LENSES_PER_SQ)
    soil = ShoreSoil(rng, ground, free)
    flow = gl.finish_flow(rng, flow0, ml.combine(free, None, ml.rock_circles(flow0, posed)), W, H)
    flow = steer_shore(flow, ground)

    Xg, Yg = np.meshgrid(grid.xc, grid.yc, indexing='ij')
    water_g = ground(Xg, Yg) < wl.WL_MM
    blocker = water_g | foot
    g = gl.build_grass(rng, grid, ground, sw, soil, flow, W, H, blocker=blocker,
                       tuck_mm=ml.TUCK_MM, side_low=ml.SIDE_LOW, side_mm=ml.SIDE_MM)
    F = gl.make_field(grid, g.T, g.lips)
    gl.fdm_backstop(F, grid.vox)
    grass, _ = gl.mesh_field(F, grid.vox)

    T_eff = np.where(water_g, wl.WL_MM + WET_SURFACE_MM, g.T)
    wet = [water_g[min(int(p.x / grid.vox), grid.nx - 1), min(int(p.y / grid.vox), grid.ny - 1)]
           for p in posed]
    rocks_all, seats = ml.seat_rocks(posed, masks, g.H0, T_eff, grid)
    wet_posed = [p for p, w_ in zip(posed, wet) if w_]
    wet_rocks = (ml.seat_rocks(wet_posed, [m for m, w_ in zip(masks, wet) if w_],
                               g.H0, T_eff, grid)[0] if wet_posed else None)
    reeds, n_clumps, n_stems, reed_bases = make_reeds(rng, ground, W, H)
    water, wsc, zw = build_water(seed, ground, wet_rocks, reed_bases)

    P.grass, P.water, P.rocks, P.reeds = grass, water, rocks_all, reeds
    P.ground, P.soil, P.wsc, P.zw, P.g = ground, soil, wsc, zw, g
    P.stats = dict(rocks=len(posed), wet=int(sum(wet)), clumps=n_clumps, stems=n_stems,
                   mud=float(soil.mud(Xg, Yg)[~water_g].mean()),
                   fronds=g.n_fronds, t=time.perf_counter() - t0,
                   ground=(gstats['ground_min'], gstats['ground_max']))
    return P


def assemble(P, cols=2, rows=2):
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db
    slab = m3d.Manifold.cube([P.W, P.H, gl.SLAB_BOX_MM])
    top = (P.grass + slab).simplify(gl.SIMPLIFY_TOL_MM) + P.water
    for extra in (P.rocks, P.reeds):
        if extra is not None:
            top = top + extra
    zmax = top.bounding_box()[5]
    surface = SurfaceConfig(cols=cols, rows=rows, square_mm=gl.SQUARE_MM)
    base_cfg = BaseConfig()
    peg = db.select_peg_height(np.zeros((2, 2)), base_cfg, zmax)
    solid = top + gl.to_manifold(db.make_base(surface, peg, base_cfg))
    solid = max(solid.decompose(), key=lambda q: q.volume())
    out = solid.to_mesh()
    mesh = trimesh.Trimesh(np.asarray(out.vert_properties)[:, :3], np.asarray(out.tri_verts),
                           process=False)
    return gl.stl_safe(mesh), peg, zmax


# ── Painted preview ──────────────────────────────────────────────────────────
PAL = dict(
    water=[(18, 38, 92), (28, 86, 156), (58, 160, 178), (150, 220, 214)],
    grass=[(34, 70, 30), (62, 110, 44), (110, 150, 60), (170, 190, 90)],
    mud=(92, 70, 50), rock=(122, 126, 134), reed=(150, 150, 80), base=(42, 42, 46))


def _tm(man):
    o = man.to_mesh()
    return trimesh.Trimesh(np.asarray(o.vert_properties)[:, :3], np.asarray(o.tri_verts), process=False)


def _ramp(t, stops):
    stops = np.array(stops, float)
    i = np.clip(t * (len(stops) - 1), 0, len(stops) - 1 - 1e-9)
    k = i.astype(int)
    f = (i - k)[:, None]
    return stops[k] * (1 - f) + stops[k + 1] * f


def painted_meshes(P):
    """Separately coloured parts (z-buffer resolves overlaps; no union)."""
    out = []
    # water: colour by local relief
    m = _tm(P.water)
    c = m.triangles_center
    zw = P.zw
    rel = zw - ndimage.gaussian_filter(zw, 2.0 / wl.CELL_MM)
    ix = np.clip(np.round(c[:, 0] / wl.CELL_MM).astype(int), 0, zw.shape[1] - 1)
    iy = np.clip(np.round(c[:, 1] / wl.CELL_MM).astype(int), 0, zw.shape[0] - 1)
    t = np.clip(0.5 + rel[iy, ix] / 0.9, 0, 1)
    col = _ramp(t, PAL['water'])
    col[np.abs(m.face_normals[:, 2]) < 0.35] = PAL['base']
    m.visual.face_colors = np.c_[col, np.full(len(col), 255)].astype(np.uint8)
    out.append(m)
    # grass + mud
    m = _tm(P.grass)
    c = m.triangles_center
    g = P.grid
    i = np.clip((c[:, 0] / g.vox).astype(int), 0, g.nx - 1)
    j = np.clip((c[:, 1] / g.vox).astype(int), 0, g.ny - 1)
    T = P.g.T
    relg = T - ndimage.gaussian_filter(T, 1.5 / g.vox)
    t = np.clip(0.5 + relg[i, j] / 0.8, 0, 1)
    col = _ramp(t, PAL['grass'])
    bare = (P.g.soil[i, j] > 0.5) | ((T - P.g.H0)[i, j] < 0.25)
    col[bare] = PAL['mud']
    col[np.abs(m.face_normals[:, 2]) < 0.35] = PAL['base']
    m.visual.face_colors = np.c_[col, np.full(len(col), 255)].astype(np.uint8)
    out.append(m)
    for man, key in ((P.rocks, 'rock'), (P.reeds, 'reed')):
        if man is not None:
            mm = _tm(man)
            mm.visual.face_colors = np.tile(list(PAL[key]) + [255], (len(mm.faces), 1)).astype(np.uint8)
            out.append(mm)
    return out


VIEWS = [('painted', 40, -120), ('oblique', 40, -120), ('top', 89, -90), ('low', 18, -150)]


def contact_sheet(seeds):
    from PIL import Image, ImageDraw, ImageFont
    from dharmatiles.render import render
    cell = (760, 600)
    try:
        font = ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 30)
    except OSError:
        font = ImageFont.load_default()
    head, cap = 50, 50
    row_h = head + cell[1] + cap
    sheet = Image.new('RGB', (cell[0] * len(VIEWS), row_h * len(seeds)), (245, 245, 245))
    dr = ImageDraw.Draw(sheet)
    with tempfile.TemporaryDirectory() as td:
        tmp = pathlib.Path(td)
        for r, seed in enumerate(seeds):
            P = build(seed)
            mesh, peg, zmax = assemble(P)
            st = P.stats
            y = r * row_h
            dr.rectangle([0, y, sheet.width, y + head - 6], fill=(225, 228, 235))
            dr.text((14, y + 8), f"shore s{seed} — {st['rocks']} rocks ({st['wet']} wet), "
                    f"{st['clumps']} reed clumps / {st['stems']} stems, mud {st['mud']:.0%} of land, "
                    f"watertight={mesh.is_watertight}", fill=(25, 25, 25), font=font)
            clay = mesh.copy()
            clay.visual.face_colors = np.tile([150, 150, 150, 255], (len(clay.faces), 1))
            topv = clay.slice_plane([0, 0, 0.5], [0, 0, 1], cap=False)
            for k, (name, elev, azim) in enumerate(VIEWS):
                p = tmp / f's{seed}-{name}.png'
                meshes = painted_meshes(P) if name == 'painted' else [clay if name == 'oblique' else topv]
                render(meshes, p, elev=elev, azim=azim, resolution=(1200, 950), quiet=True, smooth=False)
                im = Image.open(p).convert('RGB')
                im.thumbnail((cell[0] - 20, cell[1] - 20))
                x = k * cell[0]
                sheet.paste(im, (x + (cell[0] - im.width) // 2, y + head + (cell[1] - im.height) // 2))
                dr.text((x + 18, y + head + cell[1] + 8), f"s{seed} · {name}", fill=(40, 40, 40), font=font)
            print(f"  seed {seed}: {st}")
    out = pathlib.Path(__file__).parent / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"{datetime.datetime.now():%Y-%m-%d-%H%M%S}-contact-sheet.png"
    sheet.save(path)
    print(f"→ {path}")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--sheet', action='store_true')
    ap.add_argument('--seeds', type=int, nargs='*', default=[1, 2, 3])
    args = ap.parse_args()
    if args.sheet:
        contact_sheet(args.seeds)
        return
    P = build(args.seed)
    mesh, peg, zmax = assemble(P)
    out = pathlib.Path(__file__).parent / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"2x2-shore-s{args.seed}.stl"
    mesh.export(path)
    print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB) {len(mesh.faces):,} faces, "
          f"watertight={mesh.is_watertight}, peg={peg} mm, top {zmax:.2f} mm")
    print(f"  {P.stats}")


if __name__ == '__main__':
    main()
