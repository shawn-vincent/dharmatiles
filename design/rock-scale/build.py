#!/usr/bin/env python3
"""Rebuilds the page "Rock at Every Scale" from src/ and the reference sheets.

    python3 build.py            writes out.html (three.js from cdnjs) from src/ and img/, and the Artifact `files` map
    python3 build.py --crops    first remakes img/ with crops.py (DT_REFS: reference/cliffs; DT_RENDERS: the wall render folders)
    python3 build.py --local    writes out.html with a local three.js copy (work/three.min.js) for headless checks

Sources (read only): the reference sheets in the repo's reference/cliffs/, and our comparison renders
from the two wall folders in this session's scratchpad (see crops.py). Nothing else is read.
"""
import os, sys, json
from PIL import Image
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
if '--crops' in sys.argv:  # remake img/ from the sheets and renders (needs DT_REFS and DT_RENDERS); img/ is kept as a source
    import crops
    crops.run()
used = set()
SZ = {}
def size(name):
    if name not in SZ:
        SZ[name] = Image.open('%s/img/%s.jpg' % (HERE, name)).size
    return SZ[name]
def img(name, cap='', alt=None, cls=''):
    used.add(name); w, h = size(name)
    return '<figure class="panel %s"><img src="img/%s.jpg" width="%d" height="%d" alt="%s" loading="lazy">%s</figure>' % (cls, name, w, h, alt or cap or name, ('<figcaption>%s</figcaption>' % cap) if cap else '')
def view(model, cap='', cls='', **d):
    attrs = ''.join(' data-%s="%s"' % (k, v) for k, v in d.items())
    return '<figure class="v3d %s" data-model="%s"%s><canvas aria-label="3D model: %s"></canvas><figcaption>%s</figcaption></figure>' % (cls, model, attrs, cap or model, cap)
def dl(rows):
    out = '<dl class="nums">'
    for k, v, *c in rows:
        out += '<dt>%s</dt><dd%s>%s</dd>' % (k, (' class="%s"' % c[0]) if c else '', v)
    return out + '</dl>'

# ---------------------------------------------------------------- primitives
def prim(pid, title, tag, intro, model, prof, rows, crops_html, cap, **vd):
    h = '<article class="prim" id="%s"><header><h3>%s</h3><span class="tag">%s</span></header>' % (pid, title, tag)
    h += '<p class="intro">%s</p>' % intro
    h += view(model, cap, **vd)
    h += '<div>'
    if prof:
        h += '<div data-prof="%s"></div>' % prof
    h += dl(rows) + '</div>'
    h += '<div class="panel stack">%s</div></article>' % crops_html
    return h

P = []
P.append(prim('planes', 'Planes and facet changes', 'primitive 1 of 9',
  'A plane is a broad, nearly flat face. A facet change is where one plane meets the next at a small angle. This is the structure of rock: the planes carry the form, and a crease between them is a line, not a gap. The model is one skin, so the planes can never separate.',
  'facet', 'facet',
  [('flat sheet ×1.4', 'change 1–5° (to 8°), span 7–70, height 0.3–1.4'),
   ('rock sheet ×1.4', 'plane 5–45°, size 7–70+, irregularity ±1.4–7'),
   ('this model', '', 'me'),
   ('rule', 'creases wander ±1.6 mm; no two neighbouring planes parallel')],
  img('t_facet', 'Rock sheet, 2. Surface detail: Facet (plane)') + img('f_facet', 'Flat-rock sheet, C: Facet Change'),
  'At the flat sheet’s defaults, three planes 2.2° to 3.4°. At these real angles the creases are faint; the plane-change slider makes them plain.'))
P.append(prim('breaks', 'Breaks and edges', 'primitive 2 of 9',
  'A break is where two planes meet. Its sharpness is a choice made edge by edge: seven in ten are eased, three in ten stay crisp. Sharp everywhere reads as a box, soft everywhere reads as a pebble. Left to right: sharp, crisp (0.45 mm), eased (2.4 mm in three tangent planes), eased with three corner chips.',
  'edges', None,
  [('angle between planes', '60–110° (rock sheet)'),
   ('chamfer, rock sheet', '0–2 mm, ×1.4 = 0–2.8 mm'),
   ('edge bevel, N and O sheet', '0.5–2.0 mm, ×1.4 = 0.7–2.8 mm'),
   ('top-front bevel (meso)', '2–6 mm, ×1.4 = 2.8–8.4; our O used 1.6–3.4'),
   ('mix', '70% soft or worn, 30% crisp'),
   ('this model', '', 'me')],
  img('t_edge', 'Rock sheet, 2: Edge (break), 60–110°, chamfer 0–2') + img('t_b', 'Rock sheet, 1B: Breaks (edges)'),
  'At the defaults, four blocks, 11 × 11 × 9 mm.', cls='wide', dist='1.2'))
P.append(prim('steps', 'Steps and ledges', 'primitive 3 of 9',
  'Offsets and ledges come from fracture layers. A ledge is a tread (nearly level, tilted back a few degrees) and a riser (steep). The lip is never a straight line. Ledges give the silhouette its rhythm and give a base wide enough to stand on.',
  'steps', 'steps',
  [('rock sheet ×1.4', 'height 2.8–21, depth 7–56, H:D 1:2 to 1:6'),
   ('N and O sheets', 'step height .10–.30 H; ledge depth .10–.35 H (O)'),
   ('risers / treads', '70–90° / 0–10° (family guide)'),
   ('this model', '', 'me')],
  img('t_step', 'Rock sheet, 2: Step (ledge)') + img('t_c', 'Rock sheet, 1C: Steps (terraces)'),
  'At the rock sheet’s defaults, three risers of about 3 mm; the flat treads, about 8 mm deep, tilt back 3°.'))
P.append(prim('chips', 'Chips and notches', 'primitive 4 of 9',
  'A chip is a removal: a flat floor, a steep wall, an outline made of straight runs. It is one plane cut into the surface, so it stays one piece. Chips sit on edges and corners: every chip in the references takes a piece out of an edge or a corner, none sits in the middle of a plane (a removal there is a dish or a pit). The section runs along the front edge, through the notch.',
  'chip', 'chip',
  [('flat sheet ×1.4', 'depth 0.3–2.1, size 1.4–11, edge angle 15–45°'),
   ('rock sheet ×1.4', 'depth 1.4–7, width 2.8–21, wall 45–80°'),
   ('our corner chip', '0.12–0.3 of the piece’s narrow side, on 30% of top corners, 0–2 per block'),
   ('this model', '', 'me')],
  img('f_chip', 'Flat-rock sheet, C: Chip / Flake') + img('t_chip', 'Rock sheet, 2: Chip (notch)'),
  'At the flat sheet’s defaults, a chip 11 mm along the front edge and 5.5 mm into the top, its floor falling 3.4° toward the edge and its wall at 45°, and a small chip off the corner.'))
P.append(prim('dishes', 'Dishes', 'primitive 5a of 9',
  'A dish is a shallow, broad, irregular scoop. It must not be round or evenly spaced; Shawn found the explorer’s round pockets “not quite organic and irregular enough”. This one is elliptical, turned off the grid, deeper on one side, with a rim that wanders.',
  'dish', 'dish',
  [('flat sheet ×1.4', 'depth 0.3–1.4, width 7–42, concave and gentle'),
   ('rock sheet ×1.4', 'depth 0.7–7, width 4–35, depth to width 1:6 to 1:10'),
   ('this model', '', 'me')],
  img('f_dish', 'Flat-rock sheet, C: Shallow Dish / Scoop') + img('t_dish', 'Rock sheet, 2: Dish (depression)'),
  'At the flat sheet’s defaults, one elliptical dish, 25 × 16 mm.'))
P.append(prim('pits', 'Pits', 'primitive 5b of 9',
  'Pits are small, clustered, uneven in size. A lattice of identical pits is the mistake the sheets warn about (“avoid uniform pitting”). Nothing under 0.4 mm is printed, whatever the scale.',
  'pits', 'pits',
  [('flat sheet ×1.4', 'diameter 0.4 (print floor)–2.8, depth 0.15–0.85'),
   ('rock sheet ×1.4', 'pits and dishes 1.4–14 across'),
   ('share of a face', 'one patch in three or four, never the whole face'),
   ('this model', '', 'me')],
  img('f_pit', 'Flat-rock sheet, C: Pitting') + img('t_e', 'Rock sheet, 1E: Pits (dishes)'),
  'At the flat sheet’s defaults, pits in three clusters, diameter 0.8 to 2.8 mm.'))
P.append(prim('ridges', 'Ridges and swells', 'primitive 6 of 9',
  'A ridge is a raised band (a vein, a weathering line, a fracture pillar). Its crest is rounded or angular, and it fades out instead of ending in a cap. A swell is the same idea broad and round.',
  'ridge', 'ridge',
  [('flat sheet ×1.4', 'height 0.3–1.4, width 4–28, crest slope 5–20°'),
   ('rock sheet ×1.4', 'height 1.4–8.4, width 4.2–28'),
   ('this model', '', 'me')],
  img('f_ridge', 'Flat-rock sheet, C: Low Ridge / Swell') + img('t_ridge', 'Rock sheet, 2: Ridge'),
  'At the flat sheet’s defaults, an angular ridge that fades, and a round swell.'))
P.append(prim('cracks', 'Cracks and fissures', 'primitive 7 of 9',
  'A crack is a narrow groove that wanders and stops. It stops because it lives inside one block (persistence). A hairline of 0.1 mm cannot be printed, so a crack is at least 0.4 mm wide and reads by its dark shadow, with a small step across it where one side sits a little proud.',
  'crack', 'crack',
  [('flat sheet ×1.4', 'width 0.15–0.7 (print floor 0.4), depth 0.15–1.1, length 7–70+'),
   ('persistence', 'a crack ends inside a block; it never crosses a joint of the level above'),
   ('this model', '', 'me')],
  img('f_crack', 'Flat-rock sheet, C: Crack / Fissure') + img('f_close', 'Flat-rock sheet, B: the close-up, hairline crack at right'),
  'At the defaults, a crack 0.7 mm wide, 1 mm deep, with a branch and a small step.'))
P.append(prim('undulation', 'Undulation', 'primitive 8 of 9',
  'Undulation is the lowest, broadest relief: no feature in it stands out, and it is what stops a flat from looking machined. It is the macro layer of the flat-rock sheet.',
  'undul', 'undul',
  [('flat sheet ×1.4', 'amplitude 0.7–2.8, width 14–56, slope 1–4°'),
   ('total relief of a flat', '1.4–2.8 mm peak to low (to 4.2)'),
   ('this model', '', 'me')],
  img('f_profile', 'Flat-rock sheet, D: surface profile') + img('f_multi', 'Flat-rock sheet, F: macro, meso, micro'),
  'At the defaults, value noise, wavelength 20 mm, relief about 2 mm.'))
P.append(prim('grain', 'Grain and striation', 'primitive 9 of 9',
  'Grain is the finest layer. It is stretched along the bedding (striations follow the slab), and it is the last thing added, because it is only worth anything under strong detail. Below 0.4 mm the printer will not follow.',
  'grain', 'grain',
  [('flat sheet ×1.4', 'micro 0.15–0.85 deep; tool floor 0.4 mm (not scaled)'),
   ('rock sheet ×1.4', 'micro texture 0.4–2.1'),
   ('share of the read', '5–10% (family guide)'),
   ('this model', '', 'me')],
  img('t_f', 'Rock sheet, 1F and the micro layer') + img('ap_tex', 'Family guide, 7: texture language (striated slabs)'),
  'At the flat sheet’s defaults, striations at 14° and a fine grain, 20 × 20 mm.'))

# ---------------------------------------------------------------- forms
FORMS = [
 ('flat', 'Flat faceted', '10–30°', 'form0'),
 ('layered', 'Layered / bedded', '0–15°', 'form1'),
 ('blocky', 'Fractured blocky', '70–110°', 'form2'),
 ('sheared', 'Sheared / inclined', '25–45°', 'form3'),
 ('weathered', 'Weathered / rounded', 'R 7–35 mm', 'form4'),
 ('pitted', 'Pitted / eroded', 'D:W 1:6–1:10', 'form5'),
 ('cracked', 'Cracked network', '30–120°', 'form6'),
 ('columnar', 'Columnar / vertical', '80–95°', 'form7'),
]
forms_html = '<div class="cols4">'
for k, name, ang, crop in FORMS:
    used.add(crop)
    forms_html += '<div class="card"><h4>%s</h4>%s<p class="small mono" style="margin:6px 0 4px">%s</p>%s</div>' % (name, view('form_' + k, '22 × 22 mm patch'), ang, img(crop, 'Rock sheet, 5'))
forms_html += '</div>'

PATTERNS = [('pat_subtle', 'Subtle and worn', 'undulation, a few small pits and one soft chip off the front edge, almost no cracks'),
            ('pat_plates', 'Fractured plates', 'large plates, shallow steps and 3° tilts between them, chips at the plate edges'),
            ('pat_pitted', 'Pitted and weathered', 'clustered pits of every size, a few shallow dishes, soft rounded edges'),
            ('pat_layered', 'Layered and veined', 'low flowing ridges and layer lines, one raised vein crossing them'),
            ('pat_network', 'Cracked network', 'domed plates of 6–10 mm cut all round by crack lines, 30–120° corners')]
pat_html = '<div class="cols3">'
for k, name, d in PATTERNS:
    pat_html += '<div class="card"><h4>%s</h4>%s<p class="small" style="margin:6px 0 0">%s</p></div>' % (name, view(k, '30 × 30 mm patch'), d)
pat_html += '<div class="card">%s</div></div>' % img('f_gallery', 'Flat-rock sheet, H: the five variations at the same scale; and G, the patterns top down')

# ---------------------------------------------------------------- hero overlays
def marks(items, sil):
    s = '<svg viewBox="0 0 400 400" aria-hidden="true"><polygon class="sil" points="%s"/>' % sil
    for n, x, y, k in items:
        s += '<g class="mk"><circle class="k%d" cx="%d" cy="%d" r="11"/><text x="%d" y="%d">%s</text></g>' % (k, x, y, x, y, n)
    return s + '</svg>'
O_M = [('1', 258, 22, 1), ('2', 170, 140, 2), ('3', 262, 77, 2), ('4', 350, 205, 2), ('5', 300, 262, 2), ('6', 302, 58, 3), ('7', 250, 200, 3), ('8', 338, 82, 3)]
N_M = [('1', 300, 66, 1), ('2', 250, 152, 2), ('3', 300, 190, 2), ('4', 228, 215, 2), ('5', 345, 82, 3), ('6', 330, 232, 3), ('7', 288, 126, 3)]
O_SIL = '128,95 192,28 265,20 345,32 386,88 388,300 330,334 270,300 236,300 190,268 180,215 132,170'
N_SIL = '155,190 205,125 245,95 300,65 345,52 372,38 378,98 385,150 386,300 330,262 270,262 215,272 170,250'
hero = '<div class="hero"><figure><div class="photo">%s%s</div><figcaption class="small">Panel O, unpainted. Dashed outline: form. Numbers: where structure and surface show.</figcaption></figure>' % (
    '<img src="img/ref_o.jpg" width="%d" height="%d" alt="Reference panel O, unpainted">' % size('ref_o'), marks(O_M, O_SIL))
hero += '<figure><div class="photo">%s%s</div><figcaption class="small">Panel N, unpainted.</figcaption></figure></div>' % (
    '<img src="img/ref_n.jpg" width="%d" height="%d" alt="Reference panel N, unpainted">' % size('ref_n'), marks(N_M, N_SIL))
used.update(['ref_o', 'ref_n'])

# ---------------------------------------------------------------- mistakes
def mistake(imgname, title, text, extra=''):
    used.add(imgname)
    return '<div class="mistake"><img src="img/%s.jpg" alt="%s" loading="lazy"><div class="b"><h4>%s</h4><p>%s</p>%s</div></div>' % (imgname, title, title, text, extra)
def mistake3d(model, title, text, **d):
    return '<div class="mistake">%s<div class="b"><h4>%s</h4><p>%s</p></div></div>' % (view(model, title, **d), title, text)
M = [
 mistake('m_o_slits', 'Plates open slits', 'Splitting a surface into tilted polygons makes plates. Plates overlap, and where they do not, a slit opens to the inside. O rounds 3 to 5. The method was wrong, not the settings. Fix: everything stays solid; a piece moves only its share of the face.'),
 mistake('m_n_shards', 'Crystal shards', 'Faces turned at many angles read as quartz. N rounds 1 to 3. Slab faces within a few degrees of one another read as rock; the variety comes from where the lips start and stop. Cross cuts: off square by up to 12° is chipped, over 25° is a shard.'),
 mistake('m_o_rubble', 'Rubble', 'Too many pieces, or offsets too large, read as a dry-stone wall. O pass 3 (every block skinned into stones). Offsets per level are a tenth to 0.4 of the level above, and fewer, bigger faces carry the look.'),
 mistake('m_o_foil', 'Triangulated relief', 'Jittered points under a Delaunay triangulation made crumpled foil, not hewn stone. A face is bent by two or three planes and cut by a few straight cracks; points are not scattered.'),
 '<div class="mistake"><div data-diagram="pitwrong"></div><div class="b"><h4>A pit as a cell inside a cell</h4><p>Subtracting a convex pit cell from a convex cell leaves up to six thin pieces and the face reads shattered (O pass 5). A pit is one plane cut into a face: one piece, no slivers.</p></div></div>',
 mistake3d('dish_round', 'Round, even dishes', 'A lattice of identical round pockets is the first thing the eye finds. Compare the irregular dish and pits above: different sizes, off the grid, clustered, wandering rims.', cls=''),
 mistake('m_n_black', 'Black recesses', 'One flat colour hides fine relief and turns every recess black (N comp 9). Grey with a hemisphere fill, a soft key and cavity shading is fair. A printed tile has no shadow map, so the model needs real depth in its joints.'),
 mistake3d('uniform', 'Uniform noise', 'The same amplitude everywhere is texture with no structure. The sheets say it twice: avoid dense noisy texture, keep surface to 5–10% of the read.'),
 mistake('m_o_boxes', 'Boxes only', 'Courses, joints and depth push give O’s massing and the strongest single lever, but bare boxes read as toys. The face needs broken planes, eased edges and chips on top of the massing.'),
]
used.add('m_o_last'); used.add('m_n_last')

CSS = open(HERE + '/src/style.css').read()
JS = ''.join(open(HERE + '/src/' + f).read() + '\n' for f in ['engine.js', 'models.js', 'page.js'])

body = open(HERE + '/src/body.html').read()
repl = {'{{PLANES_CROPS}}': img('ap_angles', 'Family guide, 5: angle families') + img('ns_planeN', 'Study: plane-family analysis, N') + img('ns_planeO', 'Study: plane-family analysis, O'),
        '{{FRAC_CROPS}}': img('ns_fracN', 'Study: fracture system, N (slope 30\u201335\u00b0, majors 30\u201340\u00b0, cross joints 60\u201375\u00b0)') + img('ns_fracO', 'Study: fracture system, O (verticals 80\u201390\u00b0, secondary 60\u201375\u00b0, bedding 0\u201310\u00b0)'),
        '{{N_CROPS}}': img('n_dims', 'N and O sheet: key dimensions, N') + img('n_mass', 'N and O sheet: massing and ratios, N') + img('ns_carveN', 'Study: carving sequence, N'),
        '{{O_CROPS}}': img('o_dims', 'N and O sheet: key dimensions, O') + img('o_mass', 'N and O sheet: massing and ratios, O') + img('ns_carveO', 'Study: carving sequence, O'),
        '{{PRIMS}}': '\n'.join(P), '{{FORMS}}': forms_html, '{{PATTERNS}}': pat_html, '{{HERO}}': hero, '{{MISTAKES}}': '\n'.join(M),
        '{{LAST}}': img('m_o_last', 'Wall O, our latest model beside panel O (left). Light grey, hemisphere fill, one soft key, cavity shading baked per vertex.') + img('m_n_last', 'Wall N, our latest model beside panel N (left). Same light.')}
for k, v in repl.items():
    body = body.replace(k, v)
# images used by body.html itself
import re
for n in re.findall(r'img/([A-Za-z0-9_]+)\.jpg', body):
    used.add(n)

local = '--local' in sys.argv
three = ('<script src="%s/work/three.min.js"></script>' % ('file://' + HERE)) if local else '<script src="https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js"></script>'
html = '''<title>Rock at Every Scale</title>
<meta name="claude:viewport-fit-sides" content="cover">
<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Barlow+Condensed:wght@500;600;700&family=IBM+Plex+Mono:wght@400;500;600&family=Source+Sans+3:ital,wght@0,400;0,600;0,700;1,400&display=swap">
<style>
%s
</style>
%s
<div class="wrap">
%s
</div>
%s
<script>
%s
</script>
''' % (CSS, '', body, three, JS)
open(HERE + '/out.html', 'w').write(html)
files = {'img/%s.jpg' % n: 'img/%s.jpg' % n for n in sorted(used)}
json.dump(files, open(HERE + '/files.json', 'w'), indent=0)
tot = sum(os.path.getsize('%s/img/%s.jpg' % (HERE, n)) for n in used)
print('out.html', len(html), 'bytes;', len(used), 'images,', tot // 1024, 'KB')
