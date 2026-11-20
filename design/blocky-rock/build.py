#!/usr/bin/env python3
"""Join src/*.html (in name order) into out.html, the one published page. /*data:NAME*/ becomes `const NAME = …;` from data/NAME.json,
/*js:FILE*/ the file from src/, @@IMG:name@@ a data: URI of img/name.jpg. work/local.html is the same page with the libraries read from work/."""
import base64, glob, json, os, re
here = os.path.dirname(os.path.abspath(__file__))
page = '\n'.join(open(p).read() for p in sorted(glob.glob(os.path.join(here, 'src', '*.html'))))
def data(m):
    d = json.load(open(os.path.join(here, 'data', m.group(1) + '.json')))
    return 'const %s = %s;' % (m.group(1).upper(), json.dumps(d, separators=(',', ':')))
page = re.sub(r'/\*data:([a-z0-9-]+)\*/', data, page)
page = re.sub(r'/\*js:([a-z0-9.-]+)\*/', lambda m: open(os.path.join(here, 'src', m.group(1))).read(), page)
page = re.sub(r'@@IMG:([a-z0-9-]+)@@', lambda m: 'data:image/jpeg;base64,' + base64.b64encode(open(os.path.join(here, 'img', m.group(1) + '.jpg'), 'rb').read()).decode(), page)
open(os.path.join(here, 'out.html'), 'w').write(page)
local = page.replace('https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js', 'three.min.js').replace('https://cdn.jsdelivr.net/npm/three@0.128.0/examples/js/controls/OrbitControls.js', 'OrbitControls.js')
open(os.path.join(here, 'work', 'local.html'), 'w').write('<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"></head><body>%s</body></html>' % local)
print('out.html', len(page))
