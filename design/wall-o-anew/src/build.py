#!/usr/bin/env python3
"""Writes out.html for "Wall O, Anew" from engine.js, wall.js, page.js, rules.py and the crop.
    python3 build.py          out.html with three.js r128 from cdnjs (for the artifact)
    python3 build.py --local  out.html with the local three.js copy (for headless checks)
The geometry is wall.js; nothing else decides what the wall looks like."""
import base64, os, sys, json
HERE = os.path.dirname(os.path.abspath(__file__)); sys.path.insert(0, HERE)
import rules
rd = lambda n: open(os.path.join(HERE, n)).read()
local = '--local' in sys.argv
crop = 'data:image/jpeg;base64,' + base64.b64encode(open(HERE + '/ref-O-unpainted.jpg', 'rb').read()).decode()
three = ('<script>' + rd('three.local.js') + '</script>') if local else '<script src="https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js"></script>'
STAGES = rules.STAGES
def card(seed, stage, cam, cap, cls='sq', live=False):
    return '<figure class="v %s"><canvas data-seed="%d" data-stage="%d" data-cam="%s" %s aria-label="%s"></canvas><figcaption>%s <span class="tri num"></span></figcaption></figure>' % (cls, seed, stage, cam, 'data-live="1"' if live else '', cap, cap)
stages = ''.join('<div class="st">%s<p><b>%s</b> %s</p></div>' % (card(1, s[0], 'stage', 'Stage %d' % s[0]), s[1], s[2]) for s in STAGES)
seeds = ''.join(card(s, 7, 'seed', 'Seed %d' % s, 'sq') for s in (1, 2, 3, 4))
rows = ''.join('<tr><td>%s</td><td class="num">%s</td><td class="%s">%s</td><td>%s</td></tr>' % (a, b, 'ok' if c == 'as written' else 'chg', c, d) for a, b, c, d in rules.TABLE)
deps = ''.join('<li><b>%s</b> %s</li>' % (a, b) for a, b in rules.DEPARTURES)
html = '''<title>Wall O, Anew</title>
<meta name="claude:viewport-fit-sides" content="cover">
<style>
/* Layout: a bench sheet. The model beside the photo it chases, then the build one rule at a time. */
:root{--bg:#eceef1;--surface:#f8f9fb;--ink:#1b2027;--ink2:#47515e;--ink3:#6d7786;--rule:#cbd1d9;--dim:#0d6e86;--ok:#2e7d58;--chg:#a55a12;--disp:"Barlow Condensed","Arial Narrow",system-ui,sans-serif;--body:"Source Sans 3","Segoe UI",system-ui,sans-serif;--mono:"IBM Plex Mono",ui-monospace,Menlo,monospace;color-scheme:light}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){--bg:#12161b;--surface:#1a2027;--ink:#e7ebf0;--ink2:#a9b4c1;--ink3:#7f8b99;--rule:#303945;--dim:#5fc4dd;--ok:#6cc79a;--chg:#e1a35d;color-scheme:dark}}
:root[data-theme="dark"]{--bg:#12161b;--surface:#1a2027;--ink:#e7ebf0;--ink2:#a9b4c1;--ink3:#7f8b99;--rule:#303945;--dim:#5fc4dd;--ok:#6cc79a;--chg:#e1a35d;color-scheme:dark}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);font:400 16px/1.55 var(--body);padding-left:max(16px,env(safe-area-inset-left,0px));padding-right:max(16px,env(safe-area-inset-right,0px))}
.wrap{max-width:1180px;margin:0 auto;padding-block:36px 70px}
h1,h2{font-family:var(--disp);font-weight:600;letter-spacing:.01em;line-height:1.1;margin:0;text-wrap:balance}
h1{font-size:clamp(2.6rem,7vw,4.4rem);font-weight:700}h2{font-size:clamp(1.7rem,4vw,2.3rem);margin:0 0 .3em}
.kick{font:600 .8rem var(--mono);letter-spacing:.14em;text-transform:uppercase;color:var(--ink3);margin-bottom:12px}
p{max-width:70ch;margin:.5em 0 .8em}.lede{font-size:1.15rem;color:var(--ink2)}
section{border-top:1px solid var(--rule);margin-top:34px;padding-top:26px}
.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}
.seeds{display:grid;grid-template-columns:repeat(4,1fr);gap:12px}
.stages{display:grid;grid-template-columns:repeat(auto-fill,minmax(250px,1fr));gap:16px}
@media(max-width:700px){.pair{grid-template-columns:1fr}.seeds{grid-template-columns:1fr 1fr}}
figure{margin:0;min-width:0}figure img,figure canvas{display:block;width:100%;aspect-ratio:1;background:#55575d;touch-action:pan-y}
figcaption{font:500 .8rem var(--mono);color:var(--ink2);padding:6px 0}
.st p{font-size:.9rem;margin:.1em 0;color:var(--ink2)}.st b{color:var(--ink);font-family:var(--disp);letter-spacing:.04em}
.crop{background:#55575d;aspect-ratio:1;background-repeat:no-repeat}
.scroll{overflow-x:auto}table{border-collapse:collapse;width:100%;font-size:.92rem;min-width:620px}
th,td{text-align:left;padding:7px 10px;border-bottom:1px solid var(--rule);vertical-align:top}th{font:600 .75rem var(--mono);letter-spacing:.1em;text-transform:uppercase;color:var(--ink3)}
.num{font-variant-numeric:tabular-nums}td.ok{color:var(--ok);font-weight:600;white-space:nowrap}td.chg{color:var(--chg);font-weight:600;white-space:nowrap}
ol.dep{padding-left:1.2em;max-width:80ch}ol.dep li{margin:.5em 0}
[data-failed]::after{content:"model failed: " attr(data-failed);color:#b3392d;font:500 .8rem var(--mono)}
body.comp{padding:0;background:#55575d}body.comp .wrap{max-width:none;padding:0}body.comp section,body.comp header,body.comp .nocomp{display:none}body.comp #comp{display:flex}
#comp{display:none;width:1164px;height:582px}#comp>*{width:582px;height:582px;flex:none}#comp canvas{width:582px;height:582px;display:block}
</style>
<div class="wrap">
<header class="nocomp"><div class="kick">Wall O &middot; rebuilt from the recipe &middot; 35 mm square</div><h1>Wall O, Anew</h1>
<p class="lede">The blocky stacked cliff, built again from section 7 of Rock at Every Scale and nothing else: half-spaces cut into cells, coarse to fine, judged in grey beside the unpainted photo. Drag any model to turn it; double-click to reset.</p></header>
<section id="whole"><h2>The photo and the model</h2><p>Left, the unpainted panel (O). Right, the wall at the panel&rsquo;s camera: azimuth &minus;22&deg;, elevation 40&deg;, 146 mm. The figure is a 28 mm stand-in.</p>
<div class="pair"><figure><img src="@@crop@@" alt="Unpainted reference panel O"><figcaption>Reference, panel O</figcaption></figure>@@whole@@</div></section>
<section id="close"><h2>Close up</h2><p>The same region of the photo and the model, upper right of the wall.</p>
<div class="pair"><figure><div class="crop" id="cropc" role="img" aria-label="Reference close-up"></div><figcaption>Reference, close</figcaption></figure>@@close@@</div></section>
<section id="seeds"><h2>Seeds 1 to 4</h2><p>The seed changes where the joints fall, the pushes, the splits, the edges and chips. The courses and tiers stay.</p><div class="seeds">@@seeds@@</div></section>
<section id="stages"><h2>The build, one recipe step at a time</h2><p>Each model adds the named step to the one before it. Seed 1.</p><div class="stages">@@stages@@</div></section>
<section id="table"><h2>Each rule, the number used, what happened</h2><div class="scroll"><table><thead><tr><th>Recipe rule</th><th>Number used</th><th>Result</th><th>Note</th></tr></thead><tbody>@@rows@@</tbody></table></div></section>
<section id="dep"><h2>Where the recipe had to change</h2><ol class="dep">@@deps@@</ol></section>
<div id="comp"><div id="comp-l"></div><div id="comp-r"></div></div>
</div>
@@three@@
<script>@@eng@@</script><script>window.OVER=@@over@@;</script><script>@@wall@@</script><script>@@pg@@</script>
<script>
(function(){var Q=new URLSearchParams(location.search),m=Q.get('comp');
var crop="@@crop@@";
document.getElementById('cropc').style.backgroundImage='url('+crop+')';
document.getElementById('cropc').style.backgroundSize='@@csz@@';document.getElementById('cropc').style.backgroundPosition='@@cpos@@';
if(!m)return;document.body.classList.add('comp');
if(m==='seeds'){var C=document.getElementById('comp');C.innerHTML='';C.style.cssText='display:flex;width:1164px;height:291px';[1,2,3,4].forEach(function(k){var q=document.createElement('canvas');q.width=291;q.height=291;q.style.cssText='width:291px;height:291px;flex:none';q.setAttribute('data-seed',k);q.setAttribute('data-stage',7);q.setAttribute('data-cam','seed');C.appendChild(q)});return;}
var L=document.getElementById('comp-l'),R=document.getElementById('comp-r');
var c=document.createElement('canvas');c.width=582;c.height=582;c.setAttribute('data-seed',Q.get('seed')||1);c.setAttribute('data-stage',Q.get('stage')||7);c.setAttribute('data-cam',m==='close'?'close':'whole');R.appendChild(c);
var d=document.createElement('div');d.style.cssText='width:582px;height:582px;background-image:url('+crop+');background-repeat:no-repeat;background-size:'+(m==='close'?'@@csz@@':'582px 582px')+';background-position:'+(m==='close'?'@@cpos@@':'0 0');L.appendChild(d);
document.querySelectorAll('.crop').forEach(function(x){x.style.display='none'});
})();
</script>'''
close_sz = rules.CLOSE_CROP   # (x, y, size) in crop pixels
csz = '%.1f%% %.1f%%' % (400 / close_sz[2] * 100, 400 / close_sz[2] * 100)
cpos = '%.2f%% %.2f%%' % (close_sz[0] / (400 - close_sz[2]) * 100, close_sz[1] / (400 - close_sz[2]) * 100)
over = json.dumps(rules.OVER)
D = dict(crop=crop, whole=card(1, 7, 'whole', 'Wall O, seed 1', 'sq', True), close=card(1, 7, 'close', 'Wall O, close', 'sq', True), seeds=seeds, stages=stages, rows=rows, deps=deps,
                  three=three, eng=rd('engine.js'), wall=rd('wall.js'), pg=rd('page.js'), over=over, csz=csz, cpos=cpos)
out = html
for k, v in D.items(): out = out.replace('@@%s@@' % k, v)
# the comp page reuses canvas[data-seed]; the page's own canvases are display:none in comp mode but still painted only when shown
open(os.path.join(HERE, 'out.html'), 'w').write(out)
print('out.html', len(out))
