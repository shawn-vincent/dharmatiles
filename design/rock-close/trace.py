#!/usr/bin/env python3
"""Traces the ridges of the reference picture (img/block-ref.jpg) and lays them on the three visible faces of the fitted block.

    python3 trace.py     writes the lines (polylines in mm on the block's faces) into out.html between its RIDGES marks, and work/trace.jpg (the lines over the picture)

The lines are found in the picture (an edge detector; double marks joined into one line, near misses carried on to the
line ahead, lines that end in space left out; then walked into polylines), not generated.
Each point is then sent back along the page's camera to the face of the block it falls on. BLOCK and CAM are the page's.
"""
import json, os
import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage as ndi
from scipy.spatial import cKDTree
from skimage import feature, measure, morphology
HERE = os.path.dirname(os.path.abspath(__file__))
BLOCK = dict(L=32.6, left=(14.8, 12.25), right=(16.9, 15.3))
CAM = dict(az=-33.75, el=22.87, dist=304.8, fov=8, target=(-4.3, -6.66, 10.53))
im = Image.open(HERE + '/img/block-ref.jpg').convert('L'); W, H = im.size; S = float(W); OFF = (W - H) / 2   # the picture letterboxed in a square
g = np.asarray(im, float) / 255

# ---- the page's camera
T = np.array(CAM['target'], float); a, e = np.radians(CAM['az']), np.radians(CAM['el'])
C = T + CAM['dist'] * np.array([np.cos(e) * np.sin(a), -np.cos(e) * np.cos(a), np.sin(e)])
f = (T - C) / np.linalg.norm(T - C); r = np.cross(f, [0, 0, 1]); r /= np.linalg.norm(r); u = np.cross(r, f)
K = (S / 2) / np.tan(np.radians(CAM['fov']) / 2)
def project(X): v = np.asarray(X, float) - C; z = v @ f; return np.array([S / 2 + K * (v @ r) / z, S / 2 - K * (v @ u) / z - OFF])
def ray(px, py): d = f * K + r * (px - S / 2) - u * (py + OFF - S / 2); return d / np.linalg.norm(d)

# ---- the block's three visible faces: corners (counter-clockwise from outside)
def end(x, wh): w, h = wh; return [np.array(p, float) for p in ([x, -w / 2, 0], [x, w / 2, 0], [x, w / 2, h], [x, -w / 2, h])]
A, B = end(-BLOCK['L'] / 2, BLOCK['left']), end(BLOCK['L'] / 2, BLOCK['right'])
FACES = {'top': [A[3], B[3], B[2], A[2]], 'near': [A[0], B[0], B[3], A[3]], 'left': [A[0], A[3], A[2], A[1]]}
PLANE = {}
for k, q in FACES.items(): n = np.cross(q[1] - q[0], q[2] - q[0]); n /= np.linalg.norm(n); PLANE[k] = (n, n @ q[0], np.array([project(p) for p in q]))
def inside(poly, p):
    s = 0
    for i in range(len(poly)):
        a1, b1 = poly[i], poly[(i + 1) % len(poly)]; c = (b1[0] - a1[0]) * (p[1] - a1[1]) - (b1[1] - a1[1]) * (p[0] - a1[0]); s += 1 if c > 0 else -1
    return abs(s) == len(poly)
def dist_poly(poly, p):
    d = 1e9
    for i in range(len(poly)):
        a1, b1 = poly[i], poly[(i + 1) % len(poly)]; t = np.clip(((p - a1) @ (b1 - a1)) / ((b1 - a1) @ (b1 - a1)), 0, 1); d = min(d, np.linalg.norm(a1 + t * (b1 - a1) - p))
    return d
def face_of(p):
    for k in FACES:
        if inside(PLANE[k][2], p): return k
    return min(FACES, key=lambda k: dist_poly(PLANE[k][2], p))
def lift(p, k): n, d0, _ = PLANE[k]; dr = ray(p[0], p[1]); t = (d0 - n @ C) / (n @ dr); return C + dr * t

# ---- the ridges in the picture
hull = Image.new('L', (W, H), 0); dr = ImageDraw.Draw(hull)
for k in FACES: dr.polygon([tuple(p) for p in PLANE[k][2]], fill=255)
mask = morphology.dilation(np.asarray(hull) > 0, morphology.disk(34))
edges = feature.canny(g, sigma=2.2, low_threshold=.012, high_threshold=.045) & mask
# one line for each ridge: the detector marks both sides of a bright ridge, so the marks are thickened until the pair joins,
# the slivers between them are filled, and the middle line is kept
def thin(e, cell=260): return morphology.skeletonize(morphology.remove_small_holes(morphology.dilation(e, morphology.disk(4)), max_size=cell))
def degree(e): return ndi.convolve(e.astype(int), np.ones((3, 3), int), mode='constant') - 1
edges = thin(edges)
# a line that stops just short of another is carried on to it (up to REACH px, roughly straight ahead)
REACH = 24
ys, xs = np.nonzero(edges); tree = cKDTree(np.c_[xs, ys]); ends = np.argwhere((degree(edges) == 1) & edges)
bridge = Image.fromarray((edges * 255).astype('uint8')); bd = ImageDraw.Draw(bridge)
for y, x in ends:
    # the way the line is heading: from the pixels of the same line 6 to 14 px back
    near = tree.query_ball_point([x, y], 14); back = np.array([[xs[k], ys[k]] for k in near if np.hypot(xs[k] - x, ys[k] - y) > 6])
    if not len(back): continue
    d = np.array([x, y], float) - back.mean(0); d /= np.linalg.norm(d) or 1; best = None
    for k in tree.query_ball_point([x, y], REACH):
        v = np.array([xs[k] - x, ys[k] - y], float); n = np.linalg.norm(v)
        if n > 3 and (v @ d) / n > .55 and (best is None or n < best[0]): best = (n, xs[k], ys[k])
    if best: bd.line([(int(x), int(y)), (int(best[1]), int(best[2]))], fill=255, width=1)
edges = thin(np.asarray(bridge) > 0, 1500)   # a cell smaller than about 40 px across is not a dish: its walls become one line
# lines that end in space are left out: ends are eaten back until none is left, so what stays is the connected net
while True:
    e1 = edges & (degree(edges) <= 1)
    if not e1.any(): break
    edges &= ~e1
ys, xs = np.nonzero(edges); pix = set(zip(xs.tolist(), ys.tolist()))
nb = lambda p: [(p[0] + i, p[1] + j) for i in (-1, 0, 1) for j in (-1, 0, 1) if (i or j) and (p[0] + i, p[1] + j) in pix]
seen, lines = set(), []
def walk(p):
    path = [p]; seen.add(p)
    while True:
        nx = [q for q in nb(path[-1]) if q not in seen]
        if not nx: return path
        q = min(nx, key=lambda q: abs(q[0] - path[-1][0]) + abs(q[1] - path[-1][1])); seen.add(q); path.append(q)
for p in sorted(pix, key=lambda p: len(nb(p))):   # ends first, so a line is walked from its end
    if p not in seen:
        path = walk(p)
        if len(path) >= 2 and len(nb(path[0])) > 1:   # started mid-line: walk the other way too
            back = walk(path[0])[1:]; path = back[::-1] + path
        lines.append(np.array(path, float))
MINLEN = 6
out, draw_im = [], Image.open(HERE + '/img/block-ref.jpg').convert('RGB'); dd = ImageDraw.Draw(draw_im)
for path in lines:
    if len(path) < MINLEN: continue
    simp = measure.approximate_polygon(path, 2.2)
    dd.line([tuple(p) for p in simp], fill=(230, 40, 30), width=2)
    # split where the line passes from one face to another
    cur, kcur = [], None
    for p in simp:
        k = face_of(p)
        if k != kcur and cur: cur.append(lift(p, kcur)); out.append(cur); cur = []
        kcur = k; cur.append(lift(p, k))
    if len(cur) > 1: out.append(cur)
os.makedirs(HERE + '/work', exist_ok=True); draw_im.save(HERE + '/work/trace.jpg', quality=90)
data = json.dumps([[[round(float(v), 2) for v in p] for p in ln] for ln in out], separators=(',', ':'))
# the page keeps the lines between two marks
page = open(HERE + '/out.html').read(); i = page.index('/*RIDGES*/') + 10; j = page.index('/*END*/')
open(HERE + '/out.html', 'w').write(page[:i] + data + page[j:])
print(len(out), 'lines,', sum(len(l) for l in out), 'points')
