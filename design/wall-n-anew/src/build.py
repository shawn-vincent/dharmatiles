#!/usr/bin/env python3
"""Rebuilds out.html ("Wall N, Anew") from the sources in this folder.
   python3 build.py          out.html with three.js r128 from cdnjs
   python3 build.py --local  out.html with the local three.min.js (for headless stills)
Reads: page.html, page.js, engine.js, wall.js, view.js, content.json (text and table), ref-N-unpainted.jpg and ref-close.jpg."""
import os, sys, json, base64
H = os.path.dirname(os.path.abspath(__file__))
def rd(n): return open(os.path.join(H, n), encoding='utf-8').read()
def uri(n): return 'data:image/jpeg;base64,' + base64.b64encode(open(os.path.join(H, n), 'rb').read()).decode()
C = json.load(open(os.path.join(H, 'content.json'), encoding='utf-8'))
local = '--local' in sys.argv
three = ('<script>' + rd('three.min.js') + '</script>') if local else '<script src="https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js"></script>'
scripts = three + ''.join('<script>' + rd(f) + '</script>' for f in ['engine.js', 'wall.js', 'view.js', 'page.js'])
rows = ''.join('<tr><td>%s</td><td>%s</td><td class="%s">%s</td></tr>' % (r[0], r[1], {'held': 'ok', 'changed': 'chg', 'silent': 'bad', 'unmet': 'bad'}[r[2]], {'held': 'held', 'changed': 'changed', 'silent': 'recipe silent', 'unmet': 'not reached'}[r[2]] + (': ' + r[3] if len(r) > 3 else '')) for r in C['table'])
stages = ''.join('<p><strong>%s</strong> %s</p>' % (n, t) for n, t in C['stages'])
h = rd('page.html')
for k, v in [('{{CROP}}', uri('ref-N-unpainted.jpg')), ('{{CROPCLOSE}}', uri('ref-close.jpg')), ('{{SUMMARY}}', C['summary']), ('{{STAGES}}', stages), ('{{TABLE}}', rows),
             ('{{DEPARTURES}}', ''.join('<li>%s</li>' % d for d in C['departures'])), ('{{STAGENAMES}}', json.dumps([n + ' ' + t.split('.')[0] for n, t in C['stages']])), ('{{SCRIPTS}}', scripts)]:
    h = h.replace(k, v)
open(os.path.join(H, 'out.html'), 'w', encoding='utf-8').write(h)
print('wrote out.html', len(h) // 1024, 'KB')
