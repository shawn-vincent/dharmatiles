#!/usr/bin/env python3
"""The study: my decomposition of each of the 40 rocks of reference/cliffs/blocky-single-rocks.png (read by eye from crops at 2.4x),
joined to what can be measured from the picture (the silhouette's box in the 3/4 view). Writes data/rocks.json and cell crops in img/.
Run: python3 study.py   (needs PIL, numpy)"""
import json, os, numpy as np
from PIL import Image
here = os.path.dirname(os.path.abspath(__file__))
src = Image.open(os.path.join(here, '../../reference/cliffs/blocky-single-rocks.png')).convert('RGB')
Y = [0, 230, 441, 634, 824, 1024]
cells = json.load(open(os.path.join(here, 'work/cells.json')))
# decomposition. base: box | ramp.  lean: 0 none, 1 slight, 2 strong.  chamfer facets seen on the main top edge (1-3) and the ratio of its widest to narrowest width (by eye)
# features: (kind, face, position, height, extent, note)
#   kind  sp split (V-groove) | st step (a lowered corner or end) | bd bed line | ch chip (corner flake, V notch) | fc facet (a plane a few degrees off a face) | of offset (one side a hair lower across a split)
#   face  L left visible face | R right visible face | T top
#   pos   0..1 along that face, left to right as seen (T: 0..1 front to back)
#   z     height of the feature's middle, 0 ground .. 1 top of the rock
#   ext   how far it reaches: 'full' (ground to top) or a fraction of the rock's height (split from the top down), or a share of the face's width for a step/facet
D = {
 'R1C1': ('box', 0, 2, 1.8, [('st','T',.40,.62,.55,'seat cut into the top near the front edge'),('sp','R',.72,.5,.5,'short joint at the corner'),('fc','L',.40,.30,.6,'two planes on the lower left')]),
 'R1C2': ('box', 1, 2, 1.5, [('st','L',.12,.30,.30,'low shelf at the lower left'),('fc','L',.40,.50,.6,'face leans in two planes')]),
 'R1C3': ('ramp',0, 3, 2.0, [('sp','L',.55,.50,.6,'short groove on the slope'),('fc','L',.30,.30,.5,'front broken in facets')]),
 'R1C4': ('box', 0, 3, 1.6, [('sp','L',.70,.50,'full','wide groove, full height'),('of','L',.30,.60,.5,'the left half a hair lower'),('ch','L',.10,.25,.2,'chip at the lower left')]),
 'R1C5': ('box', 0, 2, 1.5, [('sp','L',.50,.80,.35,'short groove from the top'),('sp','R',.50,.80,.30,'second, on the other face'),('ch','R',.50,.30,.2,'flake at the lower edge')]),
 'R1C6': ('box', 1, 2, 1.7, [('fc','L',.40,.40,.9,'one big oblique plane from lower left to mid right'),('sp','R',.80,.50,'full','groove near the corner')]),
 'R1C7': ('ramp',0, 3, 1.8, [('st','L',.30,.35,.50,'flat shelf at the front, ramp behind'),('fc','L',.50,.50,.5,'slope in two planes')]),
 'R1C8': ('box', 0, 2, 1.6, [('sp','L',.35,.50,'full','groove'),('sp','L',.72,.50,.7,'second groove'),('bd','T',.5,.92,.9,'line across the top')]),
 'R2C1': ('box', 2, 2, 1.8, [('fc','L',.40,.30,.7,'oblique facet at lower left'),('ch','L',.20,.30,.2,'notch at the foot')]),
 'R2C2': ('box', 0, 2, 1.7, [('sp','L',.20,.50,'full','groove'),('sp','L',.75,.50,'full','groove'),('bd','T',.5,.92,.6,'line across the top')]),
 'R2C3': ('box', 0, 2, 1.4, [('sp','R',.70,.60,.7,'groove from the top'),('ch','L',.50,.20,.2,'small flake low')]),
 'R2C4': ('box', 1, 3, 1.4, [('st','L',.10,.40,.30,'big shelf at the lower left'),('fc','R',.50,.50,.5,'right face in two planes')]),
 'R2C5': ('box', 0, 2, 1.6, [('st','T',.30,.75,.40,'the top steps down to the left'),('sp','L',.60,.60,.7,'groove'),('sp','R',.50,.60,.4,'groove on the end')]),
 'R2C6': ('box', 0, 3, 1.6, [('st','L',.55,.40,.30,'notch at the front with a small block in it'),('sp','L',.50,.60,.6,'groove above it')]),
 'R2C7': ('ramp',1, 2, 1.7, [('st','L',.20,.40,.45,'lower step'),('st','L',.50,.70,.30,'second step above')]),
 'R2C8': ('box', 0, 2, 1.4, [('sp','L',.82,.50,'full','groove near the right edge'),('ch','L',.30,.15,.2,'flake at the foot')]),
 'R3C1': ('box', 0, 3, 1.3, [('ch','L',.50,.30,.3,'V notch, a triangle, on the left face'),('bd','T',.5,.90,.7,'line near the top'),('fc','L',.30,.50,.5,'face in planes')]),
 'R3C2': ('ramp',0, 2, 2.2, [('st','L',.10,.40,.35,'shelf at the lower left'),('fc','R',.50,.50,.6,'right face in planes')]),
 'R3C3': ('box', 0, 2, 1.7, [('sp','L',.58,.50,'full','wide groove, full height'),('st','L',.10,.50,.20,'low step left')]),
 'R3C4': ('box', 0, 2, 1.4, [('sp','L',.35,.50,'full','groove'),('sp','R',.40,.50,'full','groove')]),
 'R3C5': ('box', 1, 2, 1.7, [('fc','L',.20,.40,.5,'diagonal facet'),('ch','L',.45,.40,.2,'small V chip on the edge')]),
 'R3C6': ('ramp',0, 3, 1.9, [('ch','L',.70,.20,.2,'triangular chip at the lower front'),('sp','R',.40,.50,.5,'groove'),('fc','L',.30,.50,.5,'slope in planes')]),
 'R3C7': ('box', 0, 2, 1.5, [('sp','L',.20,.50,.8,'groove'),('sp','L',.72,.50,.8,'groove'),('ch','R',.50,.20,.2,'flake low right'),('bd','T',.5,.95,.8,'line across the top')]),
 'R3C8': ('ramp',1, 3, 1.8, [('st','L',.20,.50,.40,'notch'),('ch','L',.50,.30,.2,'chip'),('ch','L',.60,.15,.15,'chip')]),
 'R4C1': ('box', 0, 2, 1.7, [('sp','L',.70,.50,'full','groove'),('st','L',.10,.40,.50,'lower left shelf')]),
 'R4C2': ('box', 0, 3, 1.5, [('sp','L',.35,.50,.7,'notch-like groove'),('st','R',.70,.40,.30,'a foot at the lower right')]),
 'R4C3': ('box', 1, 2, 1.4, [('fc','L',.70,.40,.8,'one long diagonal plane'),('ch','L',.50,.50,.2,'small flake')]),
 'R4C4': ('ramp',0, 2, 1.8, [('sp','R',.40,.50,.6,'groove'),('fc','L',.50,.50,.5,'slope in planes')]),
 'R4C5': ('box', 0, 2, 1.4, [('sp','R',.55,.50,.8,'groove'),('ch','R',.55,.25,.2,'flake low')]),
 'R4C6': ('box', 1, 1, 1.3, [('ch','L',.50,.35,.15,'one small flake')]),
 'R4C7': ('box', 0, 2, 1.5, [('sp','L',.30,.50,.7,'groove'),('sp','L',.82,.50,.7,'groove')]),
 'R4C8': ('box', 0, 2, 1.6, [('st','L',.10,.45,.35,'big shelf at the lower left')]),
 'R5C1': ('box', 0, 2, 1.6, [('sp','R',.55,.50,.8,'groove'),('bd','T',.5,.90,.7,'line across the top')]),
 'R5C2': ('box', 0, 3, 1.6, [('st','T',.70,.80,.40,'the top steps down toward the left'),('ch','L',.50,.60,.3,'V chip'),('fc','L',.30,.40,.6,'planes')]),
 'R5C3': ('ramp',0, 2, 1.8, [('st','T',.40,.60,.50,'seat: shelf low, back high'),('bd','L',.50,.40,.6,'line on the shelf')]),
 'R5C4': ('box', 0, 2, 1.6, [('sp','L',.30,.50,'full','groove running slantwise'),('fc','L',.50,.50,.5,'planes')]),
 'R5C5': ('ramp',0, 2, 2.0, [('sp','R',.55,.50,.6,'groove'),('fc','L',.50,.50,.5,'slope in planes')]),
 'R5C6': ('box', 0, 2, 1.5, [('st','T',.40,.70,.35,'shelf in the top, back raised'),('sp','L',.30,.60,.5,'short groove')]),
 'R5C7': ('ramp',0, 3, 1.7, [('ch','L',.30,.20,.2,'chip at the lower left'),('fc','L',.40,.50,.7,'long slope in planes')]),
 'R5C8': ('box', 0, 2, 1.3, [('sp','L',.25,.50,.6,'groove'),('ch','R',.40,.40,.2,'V chip')]),
}
FIELDS = ['kind','face','pos','z','ext','note']
KIND = {'sp':'split','st':'step','bd':'bed','ch':'chip','fc':'facet','of':'offset'}
out = []
for k in sorted(D, key=lambda c: (int(c[1]), int(c[3]))):
    base, lean, nf, wr, feats = D[k]
    x0, y0, x1, y1 = cells[k]
    g = np.array(src.crop((x0, y0, x1, y1)).convert('L')).astype(float)
    g = g[4:-4, 4:-4]
    mask = g > 105                      # the rock is light grey, the stage 60-75
    ys, xs = np.where(mask)
    bx0, bx1, by0, by1 = xs.min(), xs.max(), ys.min(), ys.max()
    w, h = bx1 - bx0 + 1, by1 - by0 + 1
    asp = h / w
    # class from the measured silhouette (view height over view width) and the base
    cls = 'ramp' if base == 'ramp' else ('pillar' if asp >= 1.25 else ('slab' if asp < .75 else 'block'))
    out.append(dict(id=k, base=base, cls=cls, lean=lean, facets=nf, widthRatio=wr, view=dict(w=int(w), h=int(h), aspect=round(float(asp), 2), fill=round(float(mask[by0:by1+1, bx0:bx1+1].mean()), 2)),
                    features=[dict(zip(FIELDS, [KIND[f[0]]] + list(f[1:]))) for f in feats],
                    cell=[x0, y0, x1, y1]))
os.makedirs(os.path.join(here, 'data'), exist_ok=True)
json.dump(out, open(os.path.join(here, 'data/rocks.json'), 'w'), separators=(',', ':'))
from collections import Counter
print(Counter(r['cls'] for r in out), Counter(f['kind'] for r in out for f in r['features']))
print([ (r['id'], r['view']['aspect']) for r in out][:40])
