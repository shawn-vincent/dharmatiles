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

def sides(p):
    """the stone's two sides: the outline's edges within 40 degrees of upright, left and right of its middle.
    Returns each side's angle from upright (positive = the top is to the right), weighted by edge length, or None."""
    c = list(p.exterior.coords); cx = p.centroid.x; acc = {'L': [0, 0], 'R': [0, 0]}
    for (x0, y0), (x1, y1) in zip(c[:-1], c[1:]):
        if y1 > y0: x0, y0, x1, y1 = x1, y1, x0, y0      # make the edge point up (y smaller)
        L = math.hypot(x1 - x0, y1 - y0); a = math.degrees(math.atan2(x1 - x0, y0 - y1))
        if L < 1e-6 or abs(a) > 40: continue
        k = 'L' if (x0 + x1) / 2 < cx else 'R'; acc[k][0] += a * L; acc[k][1] += L
    return [acc[k][0] / acc[k][1] if acc[k][1] > 0 else None for k in 'LR'] + [acc['L'][1], acc['R'][1]]

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
        # is the stone's foot in view? a ground stone whose lower edge has no other traced stone just under it
        from shapely.affinity import translate
        under = translate(p, 0, 4).difference(p)
        others = unary_union([P[o['id']] for o in c['stones'] if o['id'] != s['id']])
        footvis = (len(s['rests']) == 0) and (under.is_empty or under.intersection(others).area < 0.25 * under.area)
        r = dict(id=s['id'], cell=cid, sheet=c['sheet'], kind=s['kind'], ground=(len(s['rests']) == 0), nrest=len(s['rests']), footvis=bool(footvis), hidden=('?' in s['rests']),
                 w=(x1 - x0) / fh, h=(y1 - y0) / fh, hb=hb / fh, wb=(bx1 - bx0) / fh, area=p.area / fh ** 2, share=p.area / mass.area,
                 lean=ang, elong=elong, convex=p.area / p.convex_hull.area,
                 nv=len(s['out']), topw=None, toph=None, topshare=None,
                 cx=(p.centroid.x - mx0) / (mx1 - mx0), cy=(my1 - p.centroid.y) / (my1 - my0),
                 topy=(my1 - y0) / fh, boty=(my1 - y1) / fh, left=(x0 - mx0) / fh, right=(x1 - mx0) / fh)
        if t is not None:
            tx0, ty0, tx1, ty1 = t.bounds
            r.update(topw=(tx1 - tx0) / fh, toph=(ty1 - ty0) / fh, topshare=t.area / p.area)
            # the top's dip across the picture: slope of the line through its leftmost and rightmost points
            pts = sorted(t.exterior.coords[:-1]); r['topdip'] = math.degrees(math.atan2(-(pts[-1][1] - pts[0][1]), pts[-1][0] - pts[0][0]))
        r['slender'] = r['hb'] / r['wb']
        r['zt'] = (r['hb'] / 0.95) if footvis else None      # true height, where the foot shows: face height over the camera's foreshortening
        aL, aR, lL, lR = sides(p)
        r['axis'] = r['lean']; r['sideL'] = aL; r['sideR'] = aR
        # tilt: the mean of the two sides; closing: how much they close toward the top (positive = narrower above)
        ok = aL is not None and aR is not None and min(lL, lR) > 0.25 * hb
        r['lean'] = (aL + aR) / 2 if ok else None
        r['close'] = (aL - aR) if ok else None
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
            # which is in front: on the ground the nearer stone's foot is lower in the picture (no use of the order of tracing)
            fr = None
            if abs(A['boty'] - B['boty']) > 0.04: fr = a if A['boty'] < B['boty'] else b
            lowfront_foot = None if fr is None else (fr == lo['id'])
            dz = abs(A['zt'] - B['zt']) if (A['zt'] is not None and B['zt'] is not None) else None
            pairs.append(dict(cell=cid, a=a, b=b, stacked=stacked, joint=jl, jang=jang, dtop=abs(A['topy'] - B['topy']), dz=dz, lowfront_foot=lowfront_foot,
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
    nearest = min(rec, key=lambda r: r['boty'])                 # the stone whose foot is lowest in the picture
    leans = [r['lean'] for r in rec if r['lean'] is not None]; ws_ = [r['wb'] for r in rec]
    # grain: the stones' long axes, weighted by area and by how long each stone is
    wts = np.array([r['area'] * max(r['elong'] - 1, 0.05) for r in rec]); angs = np.radians([r['axis'] for r in rec])
    # axes are undirected: average on the doubled angle; 0 = upright, +-90 = level
    C = (wts * np.cos(2 * angs)).sum() / wts.sum(); S = (wts * np.sin(2 * angs)).sum() / wts.sum()
    simp = mass.simplify(0.8)
    cells[cid] = dict(id=cid, sheet=c['sheet'], n=len(rec), W=(mx1 - mx0) / fh, H=(my1 - my0) / fh, area=mass.area / fh ** 2,
                      fill=mass.area / ((mx1 - mx0) * (my1 - my0)), sky=[round(v, 3) for v in sky], low=[round(v, 3) for v in low],
                      peak=(int(np.argmax(sky)) + .5) / 48, cx=(mass.centroid.x - mx0) / (mx1 - mx0), cy=(my1 - mass.centroid.y) / (my1 - my0),
                      big=areas[0] / sum(areas), big2=(areas[1] / areas[0] if len(areas) > 1 else None),
                      footshare=nearest['topy'] / ((my1 - my0) / fh), footid=nearest['id'], tiltrange=(max(leans) - min(leans)) if leans else 0.0, wratio=max(ws_) / min(ws_),
                      farend=float(np.mean(sky[:4]) / max(sky)),
                      tilt=float(np.median([r['lean'] for r in rec if r['lean'] is not None] or [0])),
                      grain=math.degrees(math.atan2(S, C)) / 2, graincoh=math.hypot(C, S),
                      fh=fh / sy, bd=bd / sx, figratio=fh / bd,
                      mass=[[round(x / sx, 1), round(y / sy, 1)] for x, y in simp.exterior.coords[:-1]],
                      kinds={k: sum(1 for r in rec if r['kind'] == k) for k in ('slab', 'pillar', 'lean', 'cap', 'toe', 'block')})
rnd = lambda d: {k: (round(v, 3) if isinstance(v, float) else v) for k, v in d.items()}
json.dump(dict(cells={k: rnd(v) for k, v in cells.items()}, stones=[rnd(s) for s in stones], pairs=[rnd(p) for p in pairs]),
          open(f'{here}/data/measures.json', 'w'), separators=(',', ':'))
print(len(cells), 'cells', len(stones), 'stones', len(pairs), 'pairs', os.path.getsize(f'{here}/data/measures.json'))
