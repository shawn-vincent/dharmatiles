import base64
d=lambda f:open(f).read()
h=d('page.html')
ref='data:image/jpeg;base64,'+base64.b64encode(open('ref-O.jpg','rb').read()).decode()
h=h.replace('__REF__',ref).replace('__STEPS__',d('steps.html')).replace('__CARRY__',d('carry.html')).replace('__GEN__',d('gen.js')).replace('__VIEW__',d('view.js'))
open('out.html','w').write(h)
open('test.html','w').write(h.replace('https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js','three.min.js'))
