#!/usr/bin/env python3
"""Turn the hand-traced joints (data/jn_ap.py, jn_t.py, jn_s.py) into data/joints.json, and measure each one.

A joint is the whole contact between two stones as it shows in the picture: a polyline in percent of the cell crop (x right, y down,
as in traces.json), cut into parts. Each part was judged by eye as
  v  a line shows: a groove, or a crease where two planes meet
  w  a faint line: just visible
  f  no line: the surface runs on from one stone to the other
The line's darkness is also measured from the picture along the same polyline (`prof`), so the eye's call can be checked.
Lengths are in figure heights as seen (fh), as everywhere in the atlas."""
import json, math, os, sys
import numpy as np
from PIL import Image
from shapely.geometry import Polygon, Point
from scipy import ndimage as ndi
here = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, here + '/data')
T = json.load(open(f'{here}/data/traces.json')); CROPS = json.load(open(f'{here}/data/crops.json'))
ME = json.load(open(f'{here}/data/measures.json')); CL = json.load(open(f'{here}/data/classes.json'))
KIND = {'col': 'column–column', 'course': 'course–course', 'cap': 'cap–support', 'lean': 'leaner–leaner', 'other': 'other'}
END = {'T': 'skyline', 'G': 'ground', 'S': 'side', 'J': 'junction', 'H': 'hidden'}
RANK = {'F': 'front', 'B': 'back', 'S': 'side'}
CONF = {'h': 'high', 'm': 'medium', 'l': 'low'}
NPROF = 16
LINE = 1.2      # the measured strength at which a line is taken to show

def line_map(cell):
    """how strongly a line shows at each pixel: a dark valley (the larger Hessian eigenvalue of the smoothed luminance) or a crease
    (the gradient), each divided by its own floor on plain faces, the larger of the two. 1 is the floor; a line is 2 and up."""
    g = np.asarray(Image.open(f'{here}/img/{cell["img"]}.jpg').convert('L')).astype(float)
    s = 1.3
    xx = ndi.gaussian_filter(g, s, order=(0, 2)); yy = ndi.gaussian_filter(g, s, order=(2, 0)); xy = ndi.gaussian_filter(g, s, order=(1, 1))
    val = np.clip((xx + yy) / 2 + np.sqrt(((xx - yy) / 2) ** 2 + xy ** 2), 0, None)
    gx = ndi.gaussian_filter(g, s, order=(0, 1)); gy = ndi.gaussian_filter(g, s, order=(1, 0)); grad = np.hypot(gx, gy)
    return val, grad

def sample(val, grad, P, reach=9):
    """the line's strength at each point of P (pixels). The hand-drawn polyline is good to 2 to 3 percent of the cell (6 to 9 pixels), so
    the line is looked for up to `reach` pixels either side: at each point one offset across the polyline is chosen, the chosen
    offsets changing slowly along it (dynamic programming), so the measure follows one line and does not hop to whatever is near."""
    offs = np.arange(-reach, reach + 1); n = len(P); S = np.zeros((n, len(offs)))
    for i, (x, y) in enumerate(P):
        a = P[max(i - 1, 0)]; b = P[min(i + 1, n - 1)]; dx, dy = b[0] - a[0], b[1] - a[1]; L = math.hypot(dx, dy) or 1; nx, ny = -dy / L, dx / L
        xs = x + offs * nx; ys = y + offs * ny
        S[i] = np.maximum(ndi.map_coordinates(val, [ys, xs], order=1, mode='nearest'), ndi.map_coordinates(grad, [ys, xs], order=1, mode='nearest') / 3.5)
    lam = 0.6; cost = S[0].copy(); back = np.zeros((n, len(offs)), int)
    pen = lam * np.abs(offs[:, None] - offs[None, :])
    for i in range(1, n):
        m = cost[None, :] - pen; back[i] = m.argmax(1); cost = m.max(1) + S[i]
    k = int(cost.argmax()); out = np.zeros(n)
    for i in range(n - 1, -1, -1): out[i] = S[i, k]; k = back[i, k]
    return out

def resample(P, step=0.5):
    out = [P[0]]; 
    for a, b in zip(P[:-1], P[1:]):
        L = math.hypot(b[0] - a[0], b[1] - a[1]); n = max(1, int(round(L / step)))
        for k in range(1, n + 1): out.append((a[0] + (b[0] - a[0]) * k / n, a[1] + (b[1] - a[1]) * k / n))
    return out

def parts(sx):
    out = []; last = None
    for chunk in sx.split('|'):
        tok = chunk.split(); st = tok[0]; pts = [[float(v) for v in t.split(',')] for t in tok[1:]]
        if last is not None: pts = [last] + pts
        assert st in 'vwf' and len(pts) >= 2, sx
        out.append({'s': st, 'p': pts}); last = pts[-1]
    return out

def suggest(contact, st, P, sx, sy):
    """the parts the measure alone would give: v at 2.5 and up, w from LINE, f under it; runs shorter than a tenth of the contact
    (or 5 pixels) are joined to a neighbour. Printed beside each joint as a second opinion on the eye's call."""
    k = np.array([2 if v >= 2.5 else 1 if v >= LINE else 0 for v in ndi.uniform_filter1d(st, 7, mode='nearest')]); n = len(k); mn = max(10, n // 10)
    runs = []
    for i, v in enumerate(k):
        if runs and runs[-1][0] == v: runs[-1][2] = i + 1
        else: runs.append([v, i, i + 1])
    while len(runs) > 1:
        i = min(range(len(runs)), key=lambda q: runs[q][2] - runs[q][1])
        if runs[i][2] - runs[i][1] >= mn: break
        j = i - 1 if i > 0 and (i == len(runs) - 1 or runs[i - 1][2] - runs[i - 1][1] >= runs[i + 1][2] - runs[i + 1][1]) else i + 1
        runs[j][1] = min(runs[j][1], runs[i][1]); runs[j][2] = max(runs[j][2], runs[i][2]); runs.pop(i)
        m = []
        for r in runs:
            if m and m[-1][0] == r[0]: m[-1][2] = r[2]
            else: m.append(r)
        runs = m
    # vertices of the contact as sample indices
    idx = []; 
    for c in contact:
        d = [math.hypot(c[0] * sx - q[0], c[1] * sy - q[1]) for q in P]; idx.append(int(np.argmin(d)))
    out = []
    for r in runs:
        pts = ([] if out else [P[0]]) + [P[i] for i in idx if r[1] < i < r[2] - 1] + [P[r[2] - 1]]
        out.append('vwf'[2 - r[0]] + ' ' + ' '.join('%g,%g' % (round(q[0] / sx * 2) / 2, round(q[1] / sy * 2) / 2) for q in pts))
    return ' | '.join(out)

DTOP = {frozenset((p['a'], p['b'])): p['dtop'] for p in ME['pairs'] if p.get('dtop') is not None}

def build():
    joints = []; added = []; floors = {}
    for mod in ('jn_ap', 'jn_t', 'jn_s'):
        if not os.path.exists(f'{here}/data/{mod}.py'): continue
        m = __import__(mod)
        for a in getattr(m, 'ADDED', []): added.append({'id': a[0], 'of': a[1], 'part': a[2], 'note': a[3]})
        for cid, rows in m.J.items():
            c = T[cid]; W, H = CROPS[c['img']]['size']; sx, sy = W / 100, H / 100
            fh = ME['cells'][cid]['fh'] * sy                       # the figure's height in pixels
            mass = ME['cells'][cid]['mass']; mx0 = min(p[0] for p in mass); mx1 = max(p[0] for p in mass); my0 = min(p[1] for p in mass); my1 = max(p[1] for p in mass)
            val, grad = line_map(c)
            # the floor: the median of each map inside the traced stones, away from their outlines
            from shapely.geometry import Polygon, Point
            polys = [Polygon([(x * sx, y * sy) for x, y in s['out']]).buffer(0).buffer(-5) for s in c['stones']]
            ys, xs = np.mgrid[0:H:3, 0:W:3]; inside = np.zeros(xs.shape, bool)
            from shapely import contains_xy
            for p in polys:
                if not p.is_empty: inside |= contains_xy(p, xs, ys)
            fv = float(np.percentile(val[ys[inside], xs[inside]], 75)) + .15; fg = float(np.percentile(grad[ys[inside], xs[inside]], 75)) + .3
            floors[cid] = [round(fv, 3), round(fg, 3)]
            ids = {s['id'] for s in c['stones']} | {a['id'] for a in added}
            for n, r in enumerate(rows):
                if len(r) <= 6:      # the short form: a, b, kind, contact, then optional flags and note
                    r = (r[0], r[1], r[2], '?', '?', r[3], r[4] if len(r) > 4 and r[4] else '? ? ? ? m', r[5] if len(r) > 5 else '')
                a, b, kind, e0, e1, px, flags = r[:7]; note = r[7] if len(r) > 7 else ''
                for q in (a, b):     # a stone named with a letter (2a, 2b) is part of the traced mass 2
                    if cid + q not in ids:
                        of = cid + q.rstrip('abcdefgh'); assert of in ids and of != cid + q, (cid, q)
                        added.append({'id': cid + q, 'of': of, 'part': '', 'note': ''}); ids.add(cid + q)
                rank, fade, dies, two, conf = flags.split()
                massP = Polygon(mass).exterior
                A, B = cid + a, cid + b
                assert A in ids and B in ids, (cid, a, b)
                auto = px.split()[0] not in ('v', 'w', 'f')
                if auto:        # no call by eye: the measure's own parts stand
                    contact = [[float(v) for v in t.split(',')] for t in px.split()]
                    P = resample([(x * sx, y * sy) for x, y in contact]); st = sample(val, grad, P)
                    px = suggest(contact, st, P, sx, sy)
                ps = parts(px)
                seg = []; tot = 0
                for p in ps:
                    L = sum(math.hypot((q[0] - o[0]) * sx, (q[1] - o[1]) * sy) for o, q in zip(p['p'][:-1], p['p'][1:])); seg.append(L); tot += L
                contact = ps[0]['p'][:] + [q for p in ps[1:] for q in p['p'][1:]]
                share = {k: sum(L for p, L in zip(ps, seg) if p['s'] == k) / tot for k in 'vwf'}
                shown = share['v'] + share['w']
                # the ends of the contact: does a line reach each?
                r0 = ps[0]['s'] != 'f'; r1 = ps[-1]['s'] != 'f'
                state = 'fused' if shown == 0 else 'open' if share['f'] == 0 else 'both' if (not r0 and not r1) else 'one' if (r0 != r1) else 'gap'
                # measured: the line's strength along the contact
                P = resample([(x * sx, y * sy) for x, y in contact])
                st = sample(val, grad, P)   # luminance units: about 0.3 on a plain face, 1.2 a faint line, 4 a crisp groove
                prof = [round(float(st[int(i * len(st) / NPROF):max(int((i + 1) * len(st) / NPROF), int(i * len(st) / NPROF) + 1)].mean()), 2) for i in range(NPROF)]
                mshare = float((st >= LINE).mean()); sug = suggest(contact, st, P, sx, sy)
                # where the line stops, from the end it reaches (fraction of the contact), when it dies at one end
                cx = float(np.mean([p[0] for p in P])) / sx; cy = float(np.mean([p[1] for p in P])) / sy
                # what was left to be derived ('?'): the ends from where they fall on the formation's outline, the rank from height,
                # how the line fades from the measured strength, where it dies from the end the merged part touches
                def endkind(pt):
                    if massP.distance(Point(pt)) > 2.2: return 'J'
                    xs = [q for q in mass if abs(q[0] - pt[0]) < 6] or mass
                    lo = max(q[1] for q in xs); hi = min(q[1] for q in xs)
                    return 'G' if lo - pt[1] < 4 and pt[1] > my0 + .5 * (my1 - my0) else 'T' if pt[1] - hi < 4 else 'S'
                if e0 == '?': e0 = endkind(contact[0])
                if e1 == '?': e1 = endkind(contact[-1])
                up = (my1 - cy) / (my1 - my0)
                if rank == '?': rank = 'F' if 'G' in (e0, e1) or up < .42 else 'B'
                if state == 'open': fade = dies = two = '-'
                elif state == 'fused': fade = dies = '-'; two = 'y' if two == '?' else two
                else:
                    if fade == '?':
                        sm = ndi.uniform_filter1d(st, 5, mode='nearest'); hi_i = np.where(sm >= 2.5)[0]; lo_i = np.where(sm < LINE)[0]
                        d = min((abs(a - b) for a in hi_i for b in lo_i), default=0) * .5 / fh
                        fade = 'g' if d > .08 or share['w'] * tot / fh > .08 else 's'
                    if dies == '?':
                        ek = e0 if not r0 else e1 if not r1 else 'J'
                        dies = {'G': 'f', 'T': 't'}.get(ek) or ('e' if ek == 'S' and (cx - mx0) / (mx1 - mx0) > .8 else 'b' if rank == 'B' and ek != 'J' else 'm')
                    if two == '?': two = 'y' if DTOP.get(frozenset((A, B)), 1) > .05 else 'n'
                j = {'id': f'{cid}-j{n + 1}', 'cell': cid, 'a': A, 'b': B, 'kind': KIND[kind], 'ends': None,
                     'parts': ps, 'state': state, 'reach': [r0, r1], 'shown': round(shown, 3), 'faint': round(share['w'], 3),
                     'len': round(tot / fh, 3), 'rank': None, 'fade': None, 'dies': None, 'two': None, 'conf': CONF[conf], 'note': note, 'auto': auto,
                     'along': round((cx - mx0) / (mx1 - mx0), 3), 'up': round((my1 - cy) / (my1 - my0), 3), 'upfh': round((my1 - cy) * sy / fh, 3),
                     'prof': prof, 'mshown': round(mshare, 3), 'msays': sug}
                j['ends'] = [END[e0], END[e1]]; j['rank'] = RANK[rank]; j['fade'] = {'g': 'gradual', 's': 'stops', '-': None}[fade]
                j['dies'] = {'f': 'foot', 'b': 'back', 'm': 'mid-face', 't': 'top', 'e': 'cut end', '-': None}[dies]; j['two'] = {'y': True, 'n': False, '-': None}[two]
                if state in ('one', 'both', 'gap'): assert j['dies'] and j['fade'], (j['id'], 'a dying joint needs where and how')
                if state == 'open': assert not j['dies'], j['id']
                joints.append(j)
    out = {'about': 'Joints traced by hand over the reference cells; see joints.py. Points are percent of the cell crop.', 'added': added, 'floors': floors, 'joints': joints}
    json.dump(out, open(f'{here}/data/joints.json', 'w'), separators=(',', ':'))
    return out

if __name__ == '__main__':
    o = build(); J = o['joints']
    from collections import Counter
    if len(sys.argv) > 1:
        for j in J:
            if j['cell'] in sys.argv[1:]: print(j['id'].ljust(8), j['a'][len(j['cell']):] + '|' + j['b'][len(j['cell']):], j['kind'][:6].ljust(6), j['state'].ljust(5), ''.join(p['s'] * max(1, round(16 * (len(p['p']) - 1) / (sum(len(q['p']) - 1 for q in j['parts'])))) for p in j['parts']).ljust(18), ''.join(str(min(9, int(round(2 * v)))) for v in j['prof']), j['mshown'], '\n            ' + j['msays'] if '|' in j['msays'] or j['msays'][0] != j['parts'][0]['s'] or len(j['parts']) > 1 else '')
    print(len(J), 'joints in', len({j['cell'] for j in J}), 'cells;', dict(Counter(j['state'] for j in J)))
