import random, html
def esc(s): return html.escape(s)

# ---------- SVG helpers (colours via CSS classes so both themes work)
def svg(w,h,body,label): 
    return f'<svg viewBox="0 0 {w} {h}" role="img" aria-label="{esc(label)}" class="dg">{body}</svg>'

def wall(spec, seed, W=220, H=130):
    """Front elevation schematic from joint sets. spec: dict."""
    r=random.Random(seed); out=[]
    base=H-8
    x=6; cols=[]
    cw=spec['cross']; 
    while x<W-6:
        w=r.uniform(*cw); w=min(w,W-6-x); cols.append((x,w)); x+=w
    sc=spec.get('scale',1.0)
    prevd=0
    for i,(x,w) in enumerate(cols):
        top=r.uniform(*spec['top'])*sc
        if 'ramp' in spec: top=top*(1-spec['ramp']*(x/W))+ r.uniform(-4,4)
        # bedding lines
        y=base; rows=[]
        bh=spec['bed']
        while True:
            h=r.uniform(*bh)*sc
            if base-y+h>=top-3: 
                rows.append((y,max(top-(base-y),6)));break
            rows.append((y,h)); y-=h
        d=spec.get('depth',0)
        shade=0
        for j,(yy,h) in enumerate(rows):
            off=0
            if spec.get('terrace'): off=j*spec['terrace']
            dx=off*0.0
            lvl=min(4,int(j*spec.get('lvl',0)+ (r.random()*spec.get('rnd',0))))
            if spec.get('colshade'): lvl=r.choice([0,1,2])
            y0=yy-h
            lean=spec.get('lean',0)
            xs=x+ (base-yy)*lean
            out.append(f'<rect x="{xs:.1f}" y="{y0:.1f}" width="{w:.1f}" height="{h:.1f}" class="b{lvl}"/>')
    return out

def S_idea1():
    b=[]
    # wall with joint sets drawn
    b.append('<rect x="10" y="14" width="200" height="100" class="b1"/>')
    for y in (36,60,86): b.append(f'<path d="M10 {y}H210" class="jn"/>')
    for x in (44,78,126,160,186): b.append(f'<path d="M{x} 14V114" class="jn2"/>')
    b.append('<path d="M10 100L120 14" class="jn3"/><path d="M60 114L170 28" class="jn3"/>')
    b.append('<text x="214" y="38" class="tx">bedding</text><text x="214" y="62" class="tx">cross</text><text x="214" y="86" class="tx">oblique</text>')
    return svg(280,124,''.join(b),'A wall cut by three families of planes: bedding, cross joints and an oblique set')

def S_idea2():
    b=['<path d="M20 110H260" class="gr"/>']
    # terraces profile
    pts="20,110 20,80 50,80 50,66 80,66 80,52 110,52 110,38 140,38 140,24"
    b.append(f'<path d="M{pts.replace(" "," L")} L140 110 Z" class="b1"/>')
    b.append('<text x="22" y="22" class="tx">setback > 0: terraces</text>')
    # cantilever
    b.append('<path d="M170 110 L170 60 L240 60 L240 40 L270 40 L270 110Z" class="b2"/><path d="M170 60L240 60" class="jn"/>')
    b.append('<path d="M180 110 L180 70 L240 70 L240 110Z" class="b0"/>')
    b.append('<text x="168" y="30" class="tx">setback &lt; 0: ledge</text>')
    return svg(290,122,''.join(b),'Side profiles: each course stepping back makes terraces; a negative setback makes a ledge that hangs out')

def S_idea3():
    b=['<path d="M10 112H270" class="gr"/>']
    xs=[10,52,98,140,186,226,270]; hs=[40,58,50,76,64,88]
    for i in range(6):
        b.append(f'<rect x="{xs[i]}" y="{112-hs[i]}" width="{xs[i+1]-xs[i]}" height="{hs[i]}" class="b{i%3}"/>')
    b.append('<path d="M10 90 C100 80 170 40 270 22" class="cv"/>')
    b.append('<text x="14" y="20" class="tx">top: a curve; blocks end on a bedding plane</text>')
    return svg(290,122,''.join(b),'Block tops follow a crest curve, snapped to bedding planes, so the crest is stepped')

def S_idea4():
    b=['<rect x="14" y="14" width="120" height="90" class="b1"/>']
    import random
    r=random.Random(4)
    for i in range(5):
        for j in range(4):
            x=24+i*22+r.uniform(-5,5); y=24+j*20+r.uniform(-5,5)
            b.append(f'<polygon points="{x:.0f},{y:.0f} {x+20:.0f},{y+r.uniform(-4,4):.0f} {x+8:.0f},{y+18:.0f}" class="b{r.choice([0,2])}"/>')
    b.append('<rect x="14" y="14" width="120" height="90" class="jn" fill="none"/>')
    b.append('<path d="M150 104H280" class="gr"/><rect x="156" y="62" width="40" height="42" class="b1"/><rect x="204" y="38" width="40" height="66" class="b1"/>')
    b.append('<path d="M156 62L196 62M204 38L244 38" class="jn3"/>')
    b.append('<text x="16" y="118" class="tx">hew: flat edge, facets in</text><text x="150" y="30" class="tx">edges stay square</text>')
    return svg(290,124,''.join(b),'A face with a flat square boundary and small flat facets inside it')

def S_idea5():
    b=[]
    for i in range(7):
        x=30+i*30
        b.append(f'<path d="M{x} 108L{135+(x-135)*0.45:.0f} 20" class="jn"/>')
    b.append('<circle cx="135" cy="14" r="4" class="pt"/><text x="144" y="16" class="tx">toward = a point</text>')
    b.append('<path d="M10 110H260" class="gr"/>')
    return svg(290,122,''.join(b),'Planes of one set leaning toward a point so columns lean together')

# example schematics
def S_M(): 
    b=wall(dict(cross=(18,28),top=(58,100),bed=(20,30),lvl=0,rnd=2.6),11,W=240,H=124); return svg(240,124,''.join(b),'Schematic of a skyline of columns of different heights')
def S_E():
    b=[]
    base=116
    for j in range(5):
        y=base-j*18; x=14+j*24
        b.append(f'<rect x="{x}" y="{y-16}" width="{200-j*20}" height="16" class="b{min(j,4)%3}"/>')
        for k in range(1,4): b.append(f'<path d="M{x+k*(200-j*20)/4:.0f} {y-16}V{y}" class="jn2"/>')
    return svg(240,124,''.join(b),'Schematic of five thin courses, each stepping back and up')
def S_P():
    b=['<path d="M10 116H230" class="gr"/>']
    cx=120
    for i,(x0,h,w) in enumerate([(30,70,16),(56,96,18),(84,106,18),(118,100,18),(148,84,18),(176,58,16)]):
        lean=(cx-x0)*0.22
        pts=f'{x0},116 {x0+w},116 {x0+w+lean:.0f},{116-h} {x0+lean+2:.0f},{116-h-6}'
        b.append(f'<polygon points="{pts}" class="b{i%3}"/>')
        b.append(f'<path d="M{x0+w*0.4+lean*0.4:.0f} {116-h*0.4:.0f}h{w*0.6:.0f}" class="jn2"/>')
    return svg(240,124,''.join(b),'Schematic of six tall columns leaning toward a centre')
def S_D():
    b=['<rect x="14" y="16" width="212" height="22" class="b2"/><rect x="26" y="38" width="190" height="80" class="b0"/>']
    for x,y in ((60,38),(110,38),(162,38)): b.append(f'<path d="M{x} {y}L{x-26} {y+80}" class="jn3"/>')
    b.append('<path d="M80 38L120 118M140 38L180 118" class="jn3"/><path d="M14 38H226" class="jn"/><text x="18" y="12" class="tx">cap, hangs out</text>')
    return svg(240,124,''.join(b),'Schematic of a cap course hanging over a face cut by oblique facets')

CSS = open('style.css').read()

REFS = [  # key, letter, name, massing, face, joints, crest, surface, family
 ('sL','L','Tall flat wall','one slab wall, a back plane','big flat slabs, thin cracks','3 or 4 irregular rows, few verticals','level, a little broken','planar, small chips','slab'),
 ('sJ','J','Corner','two walls meeting','wide flat slabs','rows and wide verticals','level','planar','slab'),
 ('sD','D','Cap over facets','wide mass, three sides','big oblique facets','diagonal, crossing','heavy cap hangs out','planar, sharp edges','slab'),
 ('sF','F','Sheer strips','one tall plane, almost no depth','narrow vertical strips','vertical, continuous','uniform','planar, fine cracks','slab'),
 ('sB','B','Ragged columns','tall narrow columns','vertical slivers','vertical, straight up','ragged, rising to one side','planar','columns'),
 ('sM','M','Skyline','columns of different heights, loose blocks in front','square blocks','vertical, some rows','each column its own top','planar','columns'),
 ('sP','P','Leaning spires','several tall columns leaning together','long tapering faces','steep and parallel','sloping, each its own','planar','columns'),
 ('sC','C','Tower','one narrow stack of small blocks','small blocks','rows and short verticals','ragged','planar','columns'),
 ('sE','E','Terraces','4 or 5 ledges stepping back and up','thin slab fronts','thin strata, few verticals','rises stepwise','flat tops, sharp lips','strata'),
 ('sG','G','Ramp','rows tilt, tall rear to low front','big blocks','tilted rows','falls on a diagonal','planar','strata'),
 ('sO','O','Chunky blocks','blocks at stepped depths','big flat faces','rows and verticals','stepped','planar, sharp cracks, flat tops','strata'),
 ('sA','A','Low massif','short, wide','wide flat blocks','few rows','flat plateau','planar','strata'),
 ('fN','N','Free-standing wall','a long wall of tall blocks, several squares','rectangular faces','running bond, vertical joints','flat, with small caps','planar, moss','strata'),
 ('fL','Lf','Ledges','thick strata, each hangs over the one below','horizontal strata','long horizontal, few verticals','stepped, big overhangs','rough, moss','strata'),
 ('sK','K','Bay and lip','a deep bay in the middle under a lip','flat slabs, larger at right','rows and verticals','lip over the bay','planar','strata'),
]
FAM={'slab':'Slab walls','columns':'Columns','strata':'Strata and ledges'}
FAMTXT={'slab':'Large flat faces cut by a few joints. Massing is one plane (or two). The crest is level or lightly broken. D differs: its joints are oblique.',
 'columns':'Steep joints dominate and run up the whole height. Each column ends where it likes, so the skyline is the picture. P and C lean or stack.',
 'strata':'Thin or thick horizontal courses. Each course sits at its own depth, so the front steps, hangs or terraces. N is the same strata made long and free-standing.'}
IDEAS=['Joint sets','Depth per cell','Tops are planes','Hewn faces','Lean and convergence']
COV={'sL':[1,0,1,1,0],'sJ':[1,0,1,1,0],'sD':[1,1,0,1,0],'sF':[1,0,1,1,0],'sB':[1,0,1,1,0],'sM':[1,1,1,0,0],'sP':[1,0,1,0,1],'sC':[1,1,1,0,0],'sE':[1,1,1,0,0],'sG':[1,1,1,0,1],'sO':[1,1,1,1,0],'sA':[1,0,1,0,0],'fN':[1,0,1,1,0],'fL':[1,1,1,0,0],'sK':[1,1,0,0,0]}

def fam_block(f):
    cards=''.join(f'<figure class="ref"><img src="img/{k}.jpg" alt="Reference {l}: {n}" loading="lazy" width="200" height="200"><figcaption><b>{l}</b> {n}</figcaption><dl><dt>massing</dt><dd>{m}</dd><dt>face</dt><dd>{fc}</dd><dt>joints</dt><dd>{j}</dd><dt>crest</dt><dd>{c}</dd><dt>surface</dt><dd>{s}</dd></dl></figure>' for k,l,n,m,fc,j,c,s,ff in REFS if ff==f)
    return f'<h3>{FAM[f]}</h3><p>{FAMTXT[f]}</p><div class="refs">{cards}</div>'

def matrix():
    h='<table class="mx"><thead><tr><th></th>'+''.join(f'<th><span>{i}</span></th>' for i in IDEAS)+'</tr></thead><tbody>'
    for k,l,n,*_ in REFS:
        h+=f'<tr><th scope="row"><img src="img/{k}.jpg" alt="" width="28" height="28"> {l}</th>'+''.join(f'<td>{"<i class=on></i>" if v else ""}</td>' for v in COV[k])+'</tr>'
    return h+'</tbody></table>'

EX=[ ('M','sM','cur_M','Skyline of columns',S_M(),
'''cliff at=crag top="[34dmm..62dmm]" bumpSize=14dmm {
  cleave dip=90deg facing=0deg
         spacing="[10dmm..14dmm]" carry=1
  cleave dip=0deg spacing="[10dmm..16dmm]"
         carry=0 wander=0.4 setback="[-3dmm..3dmm]"
}''','Vertical joints run the full height (carry 1), so columns are separate. The bedding set is independent per column (carry 0), so each column has its own rows and its own top. Setback steps each block in or out from its neighbour along the wall, so some stand in front.'),
 ('E','sE','cur_E','Terraces',S_E(),
'''cliff at=crag top="$edge + 8dmm + 24dmm * $y / 35" {
  cleave dip=2deg facing=0deg
         spacing="[6.5dmm..10dmm]" carry=1
         setback="[4dmm..7dmm]"
  cleave dip=90deg facing=0deg
         spacing="[12dmm..22dmm]" carry=0.3
}''','Thin strata, each standing back from the one below: that is the whole terrace. The crest rises with depth into the wall, and each block ends on a bedding plane, so it rises in steps.'),
 ('P','sP','cur_P','Leaning spires',S_P(),
'''cliff at=spires top="[56dmm..68dmm]" {
  cleave dip="[72deg..80deg]" toward="0.5,0.5"
         spacing="[4dmm..7dmm]" carry=1
  cleave dip=0deg spacing="[10dmm..14dmm]" carry=0
}''','A steep set whose planes lean toward a point, so every column leans in toward the middle. The region is three summits, as today. The word toward names a point; facing stays an angle.'),
 ('D','sD','cur_D','Oblique facets and a cap',S_D(),
'''cliff at=crag top="[44dmm..56dmm]" {
  cleave dip=0deg spacing="[12dmm..18dmm]" carry=0.9
         setback="[-4dmm..-2.5dmm]"
  cleave dip=60deg facing=30deg
         spacing="[8dmm..15dmm]" carry=0.5
  cleave dip=90deg facing=0deg
         spacing="[8dmm..15dmm]"
}''','A third set at 60 degrees makes the big slanted facets that no row-and-column wall can. The bedding set steps each course forward (negative setback), so the top course hangs out as the cap.')]

def ex_block():
    o=''
    for l,k,cur,t,sv,code,why in EX:
        o+=f'''<article class="ex"><h3>{l}. {t}</h3>
<div class="trio"><figure><img src="img/{k}.jpg" alt="Reference {l}" width="200" height="200"><figcaption>reference</figcaption></figure>
<figure><img src="img/{cur}.jpg" alt="Best today for {l}" width="200" height="160"><figcaption>best today</figcaption></figure>
<figure class="sk">{sv}<figcaption>what the sets cut (schematic, not a render)</figcaption></figure></div>
<pre><code>{esc(code)}</code></pre><p>{why}</p></article>'''
    return o

def main():
    body=open('body.html').read()
    body=body.replace('{{FAMS}}',''.join(fam_block(f) for f in ('slab','columns','strata')))
    body=body.replace('{{MATRIX}}',matrix()).replace('{{EX}}',ex_block())
    for i,f in enumerate((S_idea1,S_idea2,S_idea3,S_idea4,S_idea5),1): body=body.replace(f'{{{{S{i}}}}}',f())
    open('page.html','w').write('<title>Cliff Cleaving</title>\n<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Archivo:wght@400;600&family=Archivo+Narrow:wght@600;700&family=JetBrains+Mono:wght@400;600&display=swap">\n<meta name="claude:viewport-fit-sides" content="cover">\n<style>'+CSS+'</style>\n'+body)
main()
