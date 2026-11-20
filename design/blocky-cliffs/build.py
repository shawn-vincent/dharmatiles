#!/usr/bin/env python3
"""Join src/*.html (in name order) into out.html, the one published page. data/*.json named in a
<!--data:NAME--> marker is inlined as `const NAME = …;` so the page carries its own measurements."""
import glob, json, os, re
here = os.path.dirname(os.path.abspath(__file__))
parts = [open(p).read() for p in sorted(glob.glob(os.path.join(here, 'src', '*.html')))]
page = '\n'.join(parts)
def data(m):
    name = m.group(1)
    d = json.load(open(os.path.join(here, 'data', name + '.json')))
    return 'const %s = %s;' % (name.upper().replace('-', '_'), json.dumps(d, separators=(',', ':')))
page = re.sub(r'/\*data:([a-z0-9-]+)\*/', data, page)
def js(m):
    return open(os.path.join(here, 'src', m.group(1))).read()
page = re.sub(r'/\*js:([a-z0-9.-]+)\*/', js, page)
open(os.path.join(here, 'out.html'), 'w').write(page)
local = page.replace('https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js', 'work/three.min.js')
open(os.path.join(here, 'work', 'local.html'), 'w').write('<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><base href="file://%s/"><style>:root{color-scheme:light}body{margin:0;font:14px system-ui}img{max-width:100%%}[hidden]{display:none!important}</style></head><body>%s</body></html>' % (here, local))
print('out.html', len(page))
