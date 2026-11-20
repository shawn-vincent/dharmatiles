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
print('out.html', len(page))
