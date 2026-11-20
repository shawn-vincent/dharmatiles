#!/usr/bin/env python3
"""Measure the traced stones (data/traces.json) and write data/measures.json.
Everything here is an image-plane measure: lengths are in figure heights as seen (the figure's foot-to-head distance in the same cell),
so a number does not depend on the cell's size on the sheet. Nothing is turned into 3D here."""
import json, math, os
import numpy as np
from shapely.geometry import Polygon, MultiPolygon
from shapely.ops import unary_union
here = os.path.dirname(os.path.abspath(__file__))
T = json.load(open(f'{here}/data/traces.json'))
CROPS = json.load(open(f'{here}/data/crops.json'))   # each cell's crop on its sheet, in pixels (the sheet of twenty's cells are not square)

def poly(pts, sx, sy):
    p = Polygon([(x * sx, y * sy) for x, y in pts])
    return p if p.is_valid else p.buffer(0)

def pca_axis(p):
    """long axis of a polygon's area: angle from vertical in degrees (positive = top to the right), elongation (sd ratio)"""
    minx, miny, maxx, maxy = p.bounds
    xs, ys = np.meshgrid(np.linspace(minx, maxx, 60), np.linspace(miny, maxy, 60))
    from shapely import contains_xy
    m = contains_xy(p, xs, ys)
    X = np.stack([xs[m], ys[m]], 1); X = X - X.mean(0)
    w, v = np.linalg.eigh(np.cov(X.T))
    ax = v[:, 1]
    if ax[1] > 0: ax = -ax              # point up (image y is down)
    return math.degrees(math.atan2(ax[0], -ax[1])), math.sqrt(w[1] / max(w[0], 1e-9))

def widths(p, n=9):
    """width of the polygon along horizontal cuts at n heights, top to bottom"""
    from shapely.geometry import LineString
    minx, miny, maxx, maxy = p.bounds; out = []
    for i in range(n):
        y = miny + (i + .5) / n * (maxy - miny)
        c = p.intersection(LineString([(minx - 1, y), (maxx + 1, y)]))
        out.append(c.length)
    return out

cells = {}; stones = []; pairs = []
for cid, c in T.items():
    W, H = CROPS[c['img']]['size']; sx, sy = W / 100, H / 100
    fig = c['fig']; fh = (fig['foot'][1] - fig['head'][1]) * sy; bd = 2 * fig['base'][2] * sx
    P = {}; top = {}
    for s in c['stones']:
        P[s['id']] = poly(s['out'], sx, sy)
        top[s['id']] = poly(s['top'], sx, sy) if s['top'] else None
    mass = unary_union(list(P.values()))
    mass = mass.buffer(2).buffer(-2)        # close hairline gaps between traced outlines
    if isinstance(mass, MultiPolygon): mass = max(mass.geoms, key=lambda g: g.area)
    mx0, my0, mx1, my1 = mass.bounds
    # the visible part of each stone: stones are numbered front first, so a stone hides the later ones where they overlap
    vis = {}; cover = None
    for s in c['stones']:
        v = P[s['id']] if cover is None else P[s['id']].difference(cover)
        vis[s['id']] = v; cover = P[s['id']] if cover is None else cover.union(P[s['id']])
    rec = []
    for s in c['stones']:
        p = P[s['id']]; t = top[s['id']]
        x0, y0, x1, y1 = p.bounds
        body = p.difference(t) if t is not None else p
        if body.is_empty or body.area < 0.2 * p.area: body = p
        if isinstance(body, MultiPolygon): body = max(body.geoms, key=lambda g: g.area)
        bx0, by0, bx1, by1 = body.bounds
        ang, elong = pca_axis(body)
        ws = widths(body)
        hb = by1 - by0
        # body height measured down the middle: the face under the top
        r = dict(id=s['id'], cell=cid, sheet=c['sheet'], kind=s['kind'], ground=(len(s['rests']) == 0), nrest=len(s['rests']),
                 w=(x1 - x0) / fh, h=(y1 - y0) / fh, hb=hb / fh, wb=(bx1 - bx0) / fh, area=p.area / fh ** 2, share=p.area / mass.area,
                 lean=ang, elong=elong, taper=(np.mean(ws[1:3]) / max(np.mean(ws[6:8]), 1e-6)), convex=p.area / p.convex_hull.area,
                 nv=len(s['out']), topw=None, toph=None, topshare=None,
                 cx=(p.centroid.x - mx0) / (mx1 - mx0), cy=(my1 - p.centroid.y) / (my1 - my0),
                 topy=(my1 - y0) / fh, boty=(my1 - y1) / fh, left=(x0 - mx0) / fh, right=(x1 - mx0) / fh)
        if t is not None:
            tx0, ty0, tx1, ty1 = t.bounds
            r.update(topw=(tx1 - tx0) / fh, toph=(ty1 - ty0) / fh, topshare=t.area / p.area)
            # the top's dip across the picture: slope of the line through its leftmost and rightmost points
            pts = sorted(t.exterior.coords[:-1]); r['topdip'] = math.degrees(math.atan2(-(pts[-1][1] - pts[0][1]), pts[-1][0] - pts[0][0]))
        r['slender'] = r['hb'] / r['wb']
        rec.append(r); stones.append(r)
    # neighbours: stones whose visible parts share a boundary
    ids = [s['id'] for s in c['stones']]; R = {r['id']: r for r in rec}
    restmap = {s['id']: s['rests'] for s in c['stones']}
    for i, a in enumerate(ids):
        for b in ids[i + 1:]:
            if vis[a].is_empty or vis[b].is_empty: continue
            shared = vis[a].buffer(1.2).intersection(vis[b].buffer(1.2))
            if shared.is_empty or shared.area < 6: continue
            sx0, sy0, sx1, sy1 = shared.bounds
            jl = max(sx1 - sx0, sy1 - sy0) / fh
            jang = math.degrees(math.atan2(sy1 - sy0, sx1 - sx0))    # 0 = a level joint, 90 = an upright joint (box of the shared band)
            stacked = (b in restmap[a]) or (a in restmap[b])
            A, B = R[a], R[b]
            lo, hi = (A, B) if A['topy'] < B['topy'] else (B, A)
            pairs.append(dict(cell=cid, a=a, b=b, stacked=stacked, joint=jl, jang=jang, dtop=abs(A['topy'] - B['topy']),
                              ratio=min(A['area'], B['area']) / max(A['area'], B['area']),
                              lowfront=(ids.index(lo['id']) < ids.index(hi['id'])), dx=(hi['cx'] - lo['cx'])))
    # the skyline: height of the mass's top edge above its lowest point, at 48 places across its width
    from shapely.geometry import LineString
    sky = []
    for i in range(48):
        x = mx0 + (i + .5) / 48 * (mx1 - mx0)
        cut = mass.intersection(LineString([(x, my0 - 1), (x, my1 + 1)]))
        sky.append(0 if cut.is_empty else (my1 - cut.bounds[1]) / fh)
    low = []
    for i in range(48):
        x = mx0 + (i + .5) / 48 * (mx1 - mx0)
        cut = mass.intersection(LineString([(x, my0 - 1), (x, my1 + 1)]))
        low.append(0 if cut.is_empty else (my1 - cut.bounds[3]) / fh)
    areas = sorted((r['area'] for r in rec), reverse=True)
    # grain: the stones' long axes, weighted by area and by how long each stone is
    wts = np.array([r['area'] * max(r['elong'] - 1, 0.05) for r in rec]); angs = np.radians([r['lean'] for r in rec])
    # axes are undirected: average on the doubled angle; 0 = upright, +-90 = level
    C = (wts * np.cos(2 * angs)).sum() / wts.sum(); S = (wts * np.sin(2 * angs)).sum() / wts.sum()
    simp = mass.simplify(0.8)
    cells[cid] = dict(id=cid, sheet=c['sheet'], n=len(rec), W=(mx1 - mx0) / fh, H=(my1 - my0) / fh, area=mass.area / fh ** 2,
                      fill=mass.area / ((mx1 - mx0) * (my1 - my0)), sky=[round(v, 3) for v in sky], low=[round(v, 3) for v in low],
                      peak=(int(np.argmax(sky)) + .5) / 48, cx=(mass.centroid.x - mx0) / (mx1 - mx0), cy=(my1 - mass.centroid.y) / (my1 - my0),
                      big=areas[0] / sum(areas), big2=(areas[1] / areas[0] if len(areas) > 1 else None),
                      grain=math.degrees(math.atan2(S, C)) / 2, graincoh=math.hypot(C, S),
                      fh=fh / sy, bd=bd / sx, figratio=fh / bd,
                      mass=[[round(x / sx, 1), round(y / sy, 1)] for x, y in simp.exterior.coords[:-1]],
                      kinds={k: sum(1 for r in rec if r['kind'] == k) for k in ('slab', 'pillar', 'lean', 'cap', 'toe', 'block')})
rnd = lambda d: {k: (round(v, 3) if isinstance(v, float) else v) for k, v in d.items()}
json.dump(dict(cells={k: rnd(v) for k, v in cells.items()}, stones=[rnd(s) for s in stones], pairs=[rnd(p) for p in pairs]),
          open(f'{here}/data/measures.json', 'w'), separators=(',', ':'))
print(len(cells), 'cells', len(stones), 'stones', len(pairs), 'pairs', os.path.getsize(f'{here}/data/measures.json'))
