#!/usr/bin/env python3
"""Build Stone and Courses from this folder alone (plus ../rock/, the shared rock library):
    python3 build.py [out.html]      writes the page, decisions.html beside it, and MANIFEST.txt (every file the build read)
Reads: css/, text/ (nav.json, header, claim, the steps, the other sections; _order.txt says the order and the four groups), decisions/, js/, vendor/, img/, data/measured.json, ../rock/.
A {{name}} in a text file is filled from data/measured.json; a {{TABLE:name}} is a table made from it; a missing name stops the build. Nothing is patched in after the fact.
It needs python3 and node (node reads js/controls.js and js/presets.js, so the words, the panels and the preset names are written once)."""
import re,os,sys,json,base64,subprocess,html
H=os.path.dirname(os.path.abspath(__file__))
R=next(r for r in (os.path.join(H,'..','rock'),os.path.join(H,'..','..','rock','src')) if os.path.exists(os.path.join(r,'load.js')))   # the shared rock library: beside this folder, or in the repo at rock/src
READ=set()
def path(*p):
    f=os.path.normpath(os.path.join(H,*p));READ.add(f);return f
rd=lambda *p:open(path(*p)).read()
MAXTEXT=120_000   # a text file over this (without its images) is a mistake, not a page
M=json.load(open(path('data','measured.json')));M.update({k:v for k,v in json.load(open(path('data','external.json'))).items() if not k.startswith('_')})
_one=[r['stones'] for r in M['rows'] if r['tiles']==1];M['stones_min']=min(_one);M['stones_max']=max(_one)
def tab(*a): return ''.join(a)
# ---- the words, the panels and the preset names, read once from the live tables ----
def node_json(js):
    code=("global.window=global;global.document={getElementById(){return null},querySelectorAll(){return []}};var fs=require('fs');"
          "var src=fs.readFileSync(%r,'utf8')+';\\n'+fs.readFileSync(%r,'utf8')+';\\nconsole.log(JSON.stringify(%s))';new Function(src)()")%(path('js','controls.js'),path('js','presets.js'),js)
    return json.loads(subprocess.run(['node','-e',code],capture_output=True,text=True,check=True).stdout)
TBL=node_json("{panels:PANELS,groups:GROUPS,rows:ROWS,presets:Object.keys(PRESETS).map(function(k){return {key:k,name:PRESETS[k].name,refuses:!!PRESETS[k].refuses}})}")
PANEL={p['id']:p for p in TBL['panels']};GROUP={g['id']:g for g in TBL['groups']};PNAME={p['key']:p for p in TBL['presets']}
# the model's own constants, read from the engine so the text cannot disagree with the code
_c=subprocess.run(['node','-e',"var src=require('fs').readFileSync(%r,'utf8');console.log(JSON.stringify(new Function(src+';return {EDGE_MARGIN:EDGE_MARGIN,OVL:OVL,MOUTH:MOUTH,LINE:LINE,LAYER:LAYER,OVER:OVER,STEEP:STEEP,DIRT:DIRT,SQUARE:SQUARE,MINSTONE:MINSTONE,OLF:OLF}')()))"%path('js','engine.js')],capture_output=True,text=True,check=True).stdout
CONST=json.loads(_c);CONST['SQUARE_OL']=round(CONST['SQUARE']*CONST['OLF'],1);CONST['OLF']=round(CONST['OLF'],4);CONST={k:('%g'%v) for k,v in CONST.items()}
def fill(t,name):
    def f(m):
        k=m.group(1)
        if k in CONST: return CONST[k]
        if k not in M or isinstance(M[k],(dict,list)): sys.exit('build: %s uses {{%s}} and data/measured.json has no such value'%(name,k))
        return str(M[k])
    t=re.sub(r'\{\{TABLE:(\w+)\}\}',lambda m:TABLES[m.group(1)](),t)
    return re.sub(r'\{\{(\w+)\}\}',f,t)
def wall(k): return html.escape(PNAME[k]['name'])
def table_triangles():
    h='<div class="tw"><table><tr><th>Wall<th class="n">Stones<th class="n">Triangles written<th class="n">Triangles after the union<th class="n">Rock (ms)<th class="n">Union (ms)</tr>'
    for r in M['rows']: h+='<tr><td>%s<td class="n">%d<td class="n">%s<td class="n">%s<td class="n">%d<td class="n">%d</tr>'%(wall(r['key']),r['stones'],format(r['written'],','),format(r['union_tris'],','),r['rock_ms'],r['union_ms'])
    return h+'</table></div>'
def table_breaks():
    h='<div class="tw"><table><tr><th>Wall<th class="n">DungeonBlocks: as the dot judges<th class="n">with bridges apart<th class="n">OpenLOCK: as the dot judges<th class="n">with bridges apart<th class="n">Worst past 65&deg;, bridges apart</tr>'
    for r in M['rows']:
        d,o=r['db'],r['ol'];h+='<tr><td>%s<td class="n">%d / %d<td class="n">%d / %d<td class="n">%d / %d<td class="n">%d / %d<td class="n">%.3f %%</tr>'%(wall(r['key']),d['counted'],d['of'],d['apart'],d['of'],o['counted'],o['of'],o['apart'],o['of'],r['worst_red_apart'])
    return h+'</table></div>'
def table_words():
    h='<div class="tw"><table><tr><th>Word<th>Panel<th>Meaning<th>Word of</tr>'
    for r in TBL['rows']:
        if not r.get('panel'): continue
        g=GROUP.get(r.get('group')) if r.get('group') else None
        word=r['label'] if (not g or r['label'].lower().startswith(g['title'].lower())) else g['title'].lower()+' '+r['label']
        text=r['text'].replace(' A word of this page; not yet in the tile language.','');meaning=text.split('. ')[0].rstrip('.')+'.'
        h+='<tr><td><code>%s</code><td>%s<td>%s<td>%s</tr>'%(html.escape(word),html.escape(PANEL[r['panel']]['title']),html.escape(meaning),'tile' if r['word']=='registry' else 'this page')
    return h+'</table></div>'
TABLES={'triangles':table_triangles,'breaks':table_breaks,'words':table_words}
def text(name):
    t=rd('text',name+'.html')
    if len(t)>MAXTEXT: sys.exit('build: text/%s.html is %d bytes of text; the limit is %d (a step file corrupted?)'%(name,len(t),MAXTEXT))
    return fill(t,name)
def inline_imgs(t):
    return re.sub(r'(src|data-unp)="img/([^"]+)"',lambda m:'%s="data:image/jpeg;base64,%s"'%(m.group(1),base64.b64encode(open(path('img',m.group(2)),'rb').read()).decode()),t)
# ---- the page's order, and the four groups the steps are read in ----
order=rd('text','_order.txt').split()
groups=[];cur=None
for k in order:
    if k.startswith('GROUP:'): cur={'id':k[6:],'steps':[]};groups.append(cur)
    elif k.startswith('steps/') and cur is not None: cur['steps'].append(k[6:])
def step_title(sid):
    t=rd('text','steps',sid+'.html');m=re.search(r'<h3><span class="snum">([^<]*)</span>\s*([^<]*)</h3>',t);return m.group(1),m.group(2).strip()
def label(sid):
    num,title=step_title(sid);return re.sub(r'^Step\s+','',num).split(' ')[0],title
def stepnav():
    h='<nav class="stepnav" aria-label="the steps"><a href="#a-viewer">Step 0</a>'
    for g in groups: h+=''.join('<a href="#%s">Step %s</a>'%(s,label(s)[0]) for s in g['steps'])
    return h+'</nav>\n'
def group_head(gid):
    p=PANEL[gid];steps=', '.join(label(s)[0] for s in next(g for g in groups if g['id']==gid)['steps'])
    return '<div class="agroup" id="g-%s"><h3><span class="gn">%d</span> %s</h3><p class="glead">%s</p><p class="small">Steps %s. Panel %d in the editor.</p></div>\n'%(gid,p['n'],html.escape(p['title']),html.escape(p['line']),steps,p['n'])
def nav():
    items=json.loads(rd('text','nav.json'));h='<nav class="toc" aria-label="Contents">\n  <div class="k">Stone and Courses</div>\n'
    for it in items:
        small='<small>%s</small>'%it['small'] if it.get('small') else ''
        h+='  <a href="#%s"%s>%s%s</a>'%(it['id'],' id="toc-algo"' if it.get('steps') else '',html.escape(it['label']).replace('&middot;','&middot;'),small)
        if it.get('steps'):
            h+='<div class="toc-sub" id="toc-steps"><a href="#a-viewer" data-step="a-viewer">0 &middot; The viewer</a>'
            for g in groups:
                p=PANEL[g['id']];h+='<span class="tgl">%d %s</span>'%(p['n'],html.escape(p['title']))
                h+=''.join('<a href="#%s" data-step="%s">%s &middot; %s</a>'%(s,s,label(s)[0],html.escape(label(s)[1])) for s in g['steps'])
            h+='</div>'
        h+='\n'
    return h+'</nav>'
parts={'ALGOHEAD':fill(rd('text','10-algo-head.html').replace('{{STEPNAV}}',stepnav()),'algo head'),'ALGOTAIL':rd('text','19-algo-tail.html')}
body=''
for k in order:
    if k.startswith('GROUP:'): body+=group_head(k[6:])
    else: body+=parts.get(k) or text(k)
body=inline_imgs(body)
css=rd('css','page.css')+rd('css','extra.css')+rd('css','panel.css')+rd('css','explorer.css')
modules=re.search(r"MODULES = \[(.*?)\]",open(path(os.path.relpath(R,H),'load.js')).read()).group(1).replace("'",'').replace(' ','').split(',')
stonesrc='\n;\n'.join(open(path(os.path.relpath(R,H),m+'.js')).read() for m in modules)
live=['problems','engine','graded','rock','union','ui','thumbs','controls','widgets','presets','spec','explorer','popups','figs','flips']
mine='\n'.join(rd('js',n+'.js') for n in live)
def refs(pre):
    d={}
    for f in sorted(os.listdir(os.path.join(H,'img','ref'))):
        if f.startswith('u_')!=(pre=='u_'): continue
        k=f[2:-4] if pre=='u_' else f[:-4]
        d[k]='data:image/jpeg;base64,'+base64.b64encode(open(path('img','ref',f),'rb').read()).decode()
    return json.dumps(d)
import hashlib
BUILD_ID=hashlib.sha1((mine+css+body).encode()).hexdigest()[:8]   # names this build in a copied spec
MEASURED='window.BUILD_ID="'+BUILD_ID+'";window.MEASURED='+json.dumps({'presets':M['presets_measured'],'rockPresets':json.load(open(path('data','rockpresets.json')))})+';\n'
mount=MEASURED+rd('js','mount.js').replace('{{REFS}}',refs('')).replace('{{REFSU}}',refs('u_'))
out=('<meta charset="utf-8"><title>Stone and Courses</title><meta name="claude:viewport-fit-sides" content="cover">\n<style>'+css+'</style>\n<div class="wrap">\n'+nav()+'\n<main>\n'+body+'\n</main></div>\n'
 +'<script id="stonesrc">'+stonesrc+'</script>\n<script>'+open(path(os.path.relpath(R,H),'panel.js')).read()+'</script>\n'
 +'<script>'+rd('vendor','manifold.js')+'</script>\n<script type="application/octet-stream" id="mfwasm">'+rd('vendor','mfwasm.b64')+'</script>\n'
 +'<script>(function(){\n'+mine+'\n'+mount+'\n})();</script>\n<script>'+rd('js','disclose.js')+'</script>\n')
dst=sys.argv[1] if len(sys.argv)>1 else os.path.join(H,'..','index.html')
open(dst,'w').write(out)
# ---- the decisions log: one page of dated entries, off the main page ----
dec=rd('decisions','entries.html')
for f in sorted(os.listdir(os.path.join(H,'decisions'))):
    if f.startswith('verbatim-'): dec=dec.replace('{{%s}}'%f[:-5],rd('decisions',f))
dec=fill(dec,'decisions')
dpage=('<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><title>Stone and Courses: decisions</title><meta name="claude:viewport-fit-sides" content="cover">\n<style>'+rd('css','page.css')+rd('css','decisions.css')+'</style>\n<main class="dpage">\n'+dec+'\n</main>\n')
open(os.path.join(os.path.dirname(os.path.abspath(dst)),'decisions.html'),'w').write(dpage)
READ.add(os.path.join(H,'build.py'))
man=sorted(os.path.relpath(f,os.path.join(H,'..')) for f in READ)
open(os.path.join(H,'MANIFEST.txt'),'w').write('\n'.join(man)+'\n')
print('built',dst,len(out),'bytes; decisions.html',len(dpage),'bytes;',len(man),'files read')
