/* ===== the explorer: a sticky viewer, Setup, and the four panels (Region, Courses, Rock region, Rock), all made from the table in controls.js ===== */
function Explorer(root,REFS){
  var self=this,P=JSON.parse(JSON.stringify(DEFAULTS)),mode='shaded',stage=5,target=false,rowsOn=false,vmode='3d',cam=new Cam(32,22),cur=null,gate=null,thin=null,timer=0,presetKey='L',tier='draft',levelManual=false;
  var W=Widgets;
  /* ---- the markup: the viewer (canvas, its overlays, its strip), Setup, the four panels, Measure ---- */
  var h='<div class="xp sp"><div class="xview"><div class="xcv sp"><canvas id="xp-cv" width="900" height="820"></canvas><canvas id="xp-el" width="900" height="820" hidden></canvas><div class="zoomv"><input data-ctl="zoom" type="range" id="xp-zoom" min="0.35" max="4" step="0.01" value="1" aria-label="zoom" title="zoom"></div><button type="button" class="ovl ovl-reset" data-ctl="reset" id="xp-zreset" title="reset view" aria-label="reset view">&#8634;</button><button type="button" class="ovl gdot" data-ctl="gate" id="xp-gdot" aria-label="Printability: measuring" aria-haspopup="dialog" aria-expanded="false"><span class="gd wait">&middot;</span></button><div class="gpop" id="xp-gpop" role="dialog" aria-label="Printability details" hidden><button type="button" class="gclose" id="xp-gclose" aria-label="close">&times;</button><div id="xp-gpb"></div></div><button type="button" class="ovl ovl-reset ovl-spec" data-ctl="CopySpec" id="xp-spec" title="Copy spec" aria-label="Copy spec"><svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="8" y="3" width="10" height="4" rx="1"/><path d="M8 5H6a1 1 0 0 0-1 1v14a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V6a1 1 0 0 0-1-1h-2M8 12h8M8 16h6"/></svg></button><button type="button" class="ovl ovl-reset ovl-max" data-ctl="Maximize" id="xp-max" aria-pressed="false" title="Maximize" aria-label="Maximize the viewer"></button><div class="specbox" id="xp-specbox" role="dialog" aria-label="Spec" hidden><textarea id="xp-spectext" spellcheck="false" aria-label="Spec text"></textarea><div class="specrow"><button type="button" id="xp-speccopy">Copy</button><button type="button" id="xp-specload">Load this text</button><button type="button" id="xp-specclose">Close</button><span id="xp-specmsg" class="small"></span></div></div><div class="vsw" id="xp-vsw" role="group" aria-label="view of the model"></div><div class="xpanel" id="xp-costs" hidden><div class="xstats" id="xp-st"></div></div><div class="xpanel" id="xp-lines" hidden><div class="xlines"><div><canvas id="xp-plan" width="560" height="560"></canvas><label class="ctl">layer height z <output id="xp-zo"></output><input type="range" id="xp-z" min="3.6" max="60" step="0.2" value="20"></label></div><div><canvas id="xp-sec" width="560" height="560"></canvas><label class="ctl">section at x <output id="xp-xo"></output><input type="range" id="xp-x" min="-17" max="17" step="0.5" value="0"></label></div></div><p class="small" id="xp-lnote"></p></div></div>'
   +'<div class="xleg" id="xp-leg"></div></div>'
   +'<div class="pe" id="xp-pe" hidden><div><canvas id="xp-pl" width="560" height="420"></canvas><div class="small" id="xp-plc"></div></div><div><canvas id="xp-ev" width="560" height="420"></canvas><div class="small" id="xp-evc"></div></div></div>'
   +'<div class="xsetup" id="xp-setup"><figure class="xref"><img alt="reference panel" id="xp-ref"><figcaption id="xp-refc"></figcaption></figure><div class="xsetc" id="xp-setc"></div></div>'
   +'<div class="xpanels" id="xp-panels"></div>'
   +'<details class="xmeasure" id="xp-measure" data-ctl="Measure"><summary>Measure <small>the union by four routes, and the last builds</small></summary><div class="in" id="xp-mbody"></div></details></div>';
  root.innerHTML=h;
  var $=function(i){return document.getElementById(i)},cv=$('xp-cv'),el=$('xp-el');
  /* the canvas fills the whole viewer box at any aspect ratio: its pixel size follows the box (device pixel ratio, capped because the renderer runs on the CPU), so nothing is stretched */
  var xcv0=document.querySelector('#xp .xcv');
  function sizeCanvas(){var r=xcv0.getBoundingClientRect(),w=Math.max(120,r.width),h=Math.max(100,r.height),q=Math.max(0.6,Math.min(window.devicePixelRatio||1,Math.sqrt(1.4e6/(w*h)))),pw=Math.round(w*q),ph=Math.round(h*q);
    if(cv.width===pw&&cv.height===ph)return false;cv.width=pw;cv.height=ph;el.width=pw;el.height=ph;return true}
  var rzT=0;if(window.ResizeObserver)new ResizeObserver(function(){clearTimeout(rzT);rzT=setTimeout(function(){if(sizeCanvas()&&cur)draw()},40)}).observe(xcv0);
  sizeCanvas();
  /* ---- the rock: its params, switches and solo live here; the panel is the shared one (rock/panel.js); a change rebuilds the stones that it changes (their keys), and the page keeps the rest ---- */
  var RP={P:rockParams(null),off:{},solo:''},rockSum=null,rockBusy=false,rockErr=null,drawT=0,lastRockSolo='';
  /* ---- one Solo for the page: a graded stage or a rock stage; every other stage is then off ---- */
  var AP={on:{band:1,outl:1}},SOLO='';
  function areaSync(){var solo=SOLO;P.band.on=AP.on.band&&(!solo||solo==='band')?1:0;P.outl.on=AP.on.outl&&(!solo||solo==='outl')?1:0}
  function rockSolo(){return !SOLO?'':(Stone.controls.STAGES.some(function(s){return s.id===SOLO})?SOLO:'#none')}
  var ST={get:function(path){return path==='tier'?tier:W.getPath(P,path)},set:function(path,v){if(path==='tier')tier=v;else W.setPath(P,path,v)},change:function(){areaSync();go()}};
  var STAGE={on:function(id){return !!AP.on[id]},solo:function(){return SOLO},live:function(id){return !!AP.on[id]&&(!SOLO||SOLO===id)},toggle:function(id){AP.on[id]=AP.on[id]?0:1},
    setSolo:function(id){SOLO=id;RP.solo=Stone.controls.STAGES.some(function(s){return s.id===id})?id:'';lastRockSolo=RP.solo;pn.sync();W.refreshAll()},change:function(){areaSync();go()}};
  /* ---- Setup: the wall to start from, its scale, its seed ---- */
  var presetDD=W.dropdown({ctl:'Preset',label:'Preset',wide:true,opts:function(){var M=window.MEASURED&&MEASURED.presets||{};return PRESET_ORDER.filter(function(k){return PRESETS[k]}).map(function(k){var m=M[k];return {v:k,text:PRESETS[k].name,badge:PRESETS[k].refuses?'refuses':(m?'gate '+m.apart+'/'+m.of:''),img:'<img src="'+refSrc(LAST[k])+'" alt="">'}})},get:function(){return presetKey},set:function(k){setPreset(k)}});
  var setc=$('xp-setc');setc.appendChild(presetDD);
  var scaleC=W.choice({ctl:'Scale',label:'Scale',opts:function(){return [{v:'db',text:'35 mm',title:'DungeonBlocks'},{v:'ol',text:'25.4 mm',title:'OpenLOCK'}]},get:function(){return P.system},set:function(v){P.system=v;go()}});
  scaleC.title='The scale of a square: 35 mm is DungeonBlocks, 25.4 mm is OpenLOCK (the value the engine keys off as system).';setc.appendChild(scaleC);
  setc.appendChild(W.choice({ctl:'Quality',label:'Quality',opts:function(){return [{v:'draft',text:'Draft 0.5 mm'},{v:'fine',text:'Fine 0.3 mm'}]},get:function(){return tier},set:function(v){tier=v;W.refreshAll();go()}}));
  setc.appendChild(W.num({ctl:'seed',path:'seed',label:'seed',min:1,max:99,step:1,unit:''},ST));
  /* ---- the four panels ---- */
  var panelEl={},WSEL=GR.WORDS;
  function groupDef(id){return GROUPS.filter(function(g){return g.id===id})[0]}
  function weightingRows(r,g,rowsInto){   /* a weighting is a choice, its number (named by the weighting) and its centre: one group of three rows */
    var base=r.path.slice(0,r.path.length-2),pre=r.ctl.slice(0,r.ctl.length-10);
    var sel=W.dropdown({ctl:r.ctl,label:'weighting',opts:function(){return WSEL.map(function(w){return {v:w,text:w}})},get:function(){return W.getPath(P,base+'.w')},set:function(w){var d=GR.WDEF[w]||{};ST.set(base+'.w',w);if(d.a!=null)ST.set(base+'.a',d.a);if(d.c!=null)ST.set(base+'.c',d.c);W.refreshAll();ST.change()}});
    var nSpec=function(){var w=W.getPath(P,base+'.w'),rg=GR.WRANGE[w]||[0,1,0.01,'number'];return {ctl:pre+'-number',path:base+'.a',label:rg[3],min:rg[0],max:rg[1],step:rg[2],unit:''}};
    var numW=W.num(nSpec(),ST),cenW=W.num({ctl:pre+'-centre',path:base+'.c',label:'centre',min:0,max:1,step:0.05,unit:''},ST);
    function vis(){var w=W.getPath(P,base+'.w');numW.hidden=!GR.WRANGE[w];if(GR.WRANGE[w])numW.apply(nSpec());cenW.hidden=!GR.HASC[w]}
    W.reg(vis);vis();var wr=el0('div','wrow');[sel,numW,cenW].forEach(function(x){wr.appendChild(x)});rowsInto(wr)}
  function tryRow(g,id){   /* "try": read the weighting by eye, on this group only */
    var d=el0('div','allrow');d.innerHTML='<span class="sl">try</span><span class="seg"></span>';var seg=d.querySelector('.seg');
    ['low','high','ends','bell','off'].forEach(function(w){var b=el0('button');b.type='button';b.textContent=w;b.onclick=function(){var tg=P[groupDef(id).state],dd=GR.WDEF[w]||{};
      if(w==='off'){if(id==='band')tg.depth=0;else tg.s=0}else{tg.w=w;tg.a=dd.a!=null?dd.a:tg.a;tg.c=dd.c!=null?dd.c:tg.c;if(id==='band'){tg.depth=3;tg.bumps=6}else{tg.s=1;tg.bumps=6}}
      W.refreshAll();ST.change()};seg.appendChild(b)});return d}
  function el0(t,c){var e=document.createElement(t);if(c)e.className=c;return e}
  /* ---- summaries: one function from a set of rows to their one line, read from the table; what differs from the preset is marked ---- */
  var FRESH=null,FRESHROCK=null,rockPreset='';
  function changed(path){return JSON.stringify(W.getPath(P,path))!==JSON.stringify(W.getPath(FRESH,path))}
  /* a card's one line: values first, each with a tiny label or a symbol (from the table's `sh`), only the rows that define the card (`key`) and the ones that differ from the preset (bold); everything else is in the opened card and in the tooltip, which has the full names. */
  function token(r){var v=ST.get(r.path),txt,full,sh=r.sh||'',sf='';
    var num=function(x){return String(+(+x).toFixed(2))};
    if(r.kind==='range'){v=v||[0,0];txt=num(v[0])+'\u2013'+num(v[1])}
    else if(r.kind==='num')txt=num(v==null?0:v);
    else if(r.kind==='weighting'){var base=r.path.slice(0,r.path.length-2);txt=W.getPath(P,base+'.w');full=(r.group||'')+' weighting '+txt;return {t:txt,full:full,c:['w','a','c'].some(function(x){return changed(base+'.'+x)}),key:r.key}}
    else{var o=r.opts.filter(function(q){return q[0]===v})[0];txt=o?o[1]:String(v);txt=txt.replace(/^(\d)x(\d).*$/,'$1\u00d7$2').replace(/^2x2.*/,'2\u00d72').replace(/ \(.*\)$/,'').replace(/^Draft.*/,'draft').replace(/^Fine.*/,'fine');if(r.ctl==='band-stay')txt=v?'stays':'free';if(/^free-/.test(r.ctl))txt=v?'on':'off'}
    if(r.ctl==='crest-bumps'||r.ctl==='band-bumps'||r.ctl==='outl-bumps'){sf=sh;sh=''}
    var t=(sh?sh+' ':'')+txt+(sf?sf:'');
    return {t:t,full:r.label+' '+txt+(r.unit?' '+r.unit:''),c:changed(r.path),key:r.key}}
  /* a free-face switch with nothing to act on is greyed, with the reason (the checker and the page agree: the engine's faceOn and L0's seams say the same) */
  function deadWhy(ctl){if(ctl==='free-back')return P.wall?'On a wall the back is the front mirrored: it takes the front\u2019s amounts.':(P.foot?'':'The wall goes on behind this footprint (a seam), so it has no free back.');
    if(ctl==='free-sides')return (P.foot||P.wing)?'':'The sides of this footprint are seams (the wall goes on into the next tile), so it has no free sides.';return ''}
  function faceLive(){return !!(P.free&&(P.free.front||(P.free.back&&!deadWhy('free-back'))||(P.free.sides&&!deadWhy('free-sides'))))}
  /* an effect with an amount and no free face on does nothing: said, not silent */
  /* the row planes' clip, said: lean and tilt are limited to what keeps every row 0.6 of its own height thick; the build records the range asked and the one used */
  function planesNote(){var q=cur&&cur.rep&&cur.rep.planes;if(!q||!q.clipped)return '';function f(a){return (+a[0].toFixed(2))+'..'+(+a[1].toFixed(2))}var o=[];['lean','tilt'].forEach(function(k){var c=q.clipped[k];if(c&&(Math.abs(c.asked[0]-c.used[0])>1e-9||Math.abs(c.asked[1]-c.used[1])>1e-9))o.push(k+' limited to '+f(c.used)+' deg (asked '+f(c.asked)+')')});return o.length?o.join('; ')+': a row keeps its height in the middle and is never thinner than 0.2 of the thinnest row ('+(+q.floor.toFixed(1))+' mm)':''}
  function noFace(panel){if(faceLive())return false;return panel==='region'?(P.band&&P.band.depth>0):(((P.recess||[0,0])[1]>0)||((P.lip||[0,0])[1]>0))}
  function pick(rows){return rows.filter(function(r){return !r.more}).map(token).filter(function(k){return k.key||k.c})}
  function summaryOf(rows){return pick(rows).map(function(k){return k.c?'<b class="chg">'+k.t+'</b>':'<span>'+k.t+'</span>'}).join(' &middot; ')}
  function fullOf(rows){return rows.map(token).map(function(k){return k.full}).join(', ')}
  function badgeOf(state){var out='';if(!STAGE.on(state))out+='<b class="bg">off</b> ';else if(SOLO===state)out+='<b class="bg">solo</b> ';return out}
  function buildPanel(p){
    var sec=el0('section','pnl');sec.id='p-'+p.id;sec.setAttribute('data-level',p.level);
    var hd=el0('button','ph0');hd.type='button';hd.setAttribute('data-ctl','panel-'+p.id);hd.innerHTML='<span class="pn">'+p.n+'</span><span class="pt"><b>'+p.title+'</b><small>'+p.line+'</small><span class="psum"></span></span>'+(p.id==='region'||p.id==='rockregion'?'<span class="pth" aria-hidden="true"><canvas width="112" height="88"></canvas><canvas width="112" height="88"></canvas><canvas width="112" height="88"></canvas></span>':'')+'<span class="pc" aria-hidden="true">&#9662;</span>';
    var rs=el0('button','prs');rs.type='button';rs.textContent='reset';rs.title='Set this panel\'s controls back to the preset\'s values';rs.onclick=function(e){e.stopPropagation();resetPanel(p.id)};
    var top=el0('div','phd');top.appendChild(hd);top.appendChild(rs);sec.appendChild(top);
    var body=el0('div','pb');sec.appendChild(body);var groups={},prow=[],psum=hd.querySelector('.psum');
    hd.onclick=function(){var open=!sec.classList.contains('open');if(window.matchMedia('(max-width:899px)').matches)[].forEach.call(document.querySelectorAll('.pnl'),function(x){x.classList.remove('open')});sec.classList.toggle('open',open);hd.setAttribute('aria-expanded',String(open));if(open)followLevel(p.level)};
    sec.addEventListener('focusin',function(){followLevel(p.level)});
    function into(g,more){return function(x){if(g)g.add(x,more);else{(more?moreOf(body):body).appendChild(x)}}}
    var moreBody=null;function moreOf(b){if(!moreBody){var d=el0('details','more');d.innerHTML='<summary>more</summary>';moreBody=el0('div','gb');d.appendChild(moreBody);b.appendChild(d)}return moreBody}
    ROWS.filter(function(r){return r.panel===p.id}).forEach(function(r){
      var g=null;if(r.group){g=groups[r.group];if(!g){var gd=groupDef(r.group);g=groups[r.group]=W.group({id:gd.id,title:gd.title,line:gd.line});g.title=gd.title;g.rows=[];(function(g,gd){W.reg(function(){g.sum.innerHTML=(gd.onoff?badgeOf(gd.state==='band'?'band':'outl'):'')+summaryOf(g.rows);g.sum.title=fullOf(g.rows)})})(g,gd);g.el.querySelector('.gh').setAttribute('data-ctl','group-'+gd.id);if(gd.onoff)W.stageToggles(g,gd.state==='band'?'band':'outl',STAGE);body.appendChild(g.el)}}
      if(g)g.rows.push(r);else prow.push(r);
      var add=into(g,r.more);
      if(r.kind==='weighting')weightingRows(r,g,add);
      else if(r.kind==='num')add(W.num(r,ST));
      else if(r.kind==='range')add(W.range(r,ST));
      else if(r.kind==='choice'){var cw=W.choice({ctl:r.ctl,label:r.label,opts:function(){return r.opts.map(function(o){return {v:o[0],text:o[1]}})},get:function(){return ST.get(r.path)},set:function(v){ST.set(r.path,v);W.refreshAll();ST.change()},wide:r.ctl==='Quality'});add(cw);if(/^free-/.test(r.ctl))W.reg(function(){var why=deadWhy(r.ctl);[].forEach.call(cw.querySelectorAll('button'),function(bt){bt.disabled=!!why;bt.title=why||''});cw.title=why||'';cw.classList.toggle('dead',!!why)})}});
    if(groups.band)groups.band.body.appendChild(tryRow(groups.band,'band'));
    if(groups.outline)groups.outline.body.appendChild(tryRow(groups.outline,'outline'));
    W.reg(function(){var t=summaryOf(prow),gs=Object.keys(groups).map(function(k){var gd=groupDef(k),g=groups[k],ks=pick(g.rows),sh=gd.id==='band'?'':(gd.id==='crest'?'crest':gd.id==='outline'?'outline':gd.title.toLowerCase());return (sh?'<i>'+sh+'</i> ':'')+ks.map(function(q){return q.c?'<b class="chg">'+q.t+'</b>':'<span>'+q.t+'</span>'}).join(' ')});
      psum.innerHTML=[t].concat(gs).concat(noFace(p.id)?['<b class="chg">no free face is on</b>']:[]).concat(p.id==='courses'&&planesNote()?['<b class="chg">'+planesNote()+'</b>']:[]).filter(Boolean).join(' &middot; ')+(p.id==='rock'?(psum.innerHTML?' &middot; ':'')+rockSummary():'');
      psum.title=[fullOf(prow)].concat(Object.keys(groups).map(function(k){return groupDef(k).title.toLowerCase()+': '+fullOf(groups[k].rows)})).filter(Boolean).join('; ')});
    panelEl[p.id]=sec;return sec}
  /* the columns are the two pairs: wall scale (Region, Courses) and stone scale (Rock region, Rock) */
  var PN=$('xp-panels'),COLS={region:0,courses:0,rockregion:1,rock:1},cols=[el0('div','xcol'),el0('div','xcol')];cols.forEach(function(c){PN.appendChild(c)});PANELS.forEach(function(p){cols[COLS[p.id]].appendChild(buildPanel(p))});
  /* the rock panel's own markup (stages, all on/off, whole stone) goes in the Rock panel; the panel itself is the rock library's (rock/panel.js), shared with Weathered Stone */
  (function(){var pb=panelEl.rock.querySelector('.pb'),d=el0('div','in');d.id='xp-rock';d.innerHTML='<div class="allrow"><span class="seg"><button type="button" id="allon">All on</button><button type="button" id="alloff">All off</button></span><span class="sl">all stages</span></div><details class="whole"><summary>Whole stone: patch size</summary><div class="sset" id="stoneset"></div></details><div id="stages"></div><p class="small" id="xp-rnote"></p>';pb.appendChild(d);
    var rr=panelEl.rockregion.querySelector('.pb'),f=el0('div','facts');f.id='xp-facts';rr.appendChild(f)})();
  var pn=Stone.panel.create({stages:$('stages'),allOn:$('allon'),allOff:$('alloff'),whole:$('stoneset'),wholeWords:['patch'],state:RP,changed:function(){if(RP.solo!==lastRockSolo){lastRockSolo=RP.solo;SOLO=RP.solo}W.refreshAll();areaSync();go()},onStatusClick:function(id,a){pn.openPanel(rockStageRecord(id),a)}});
  /* the Rock panel's preset row: a preset is a named set of values for a group's controls, here the rock library's own table (Stone.controls.PRESETS, the one Weathered Stone reads); it sets only the Rock panel's controls */
  function rockPresetValues(k){var o=rockParams(PRESETS[presetKey].rock),q=Stone.controls.PRESETS[k].P;specTables().rockKeys.forEach(function(key){if(q[key]!==undefined)o[key]=q[key]});return o}
  function applyRockPreset(k){rockPreset=k||'';RP.P=k?rockPresetValues(k):rockParams(PRESETS[presetKey].rock);RP.off={};RP.solo='';SOLO='';lastRockSolo='';FRESHROCK=JSON.parse(JSON.stringify(RP.P));pn.sync();W.refreshAll();areaSync();go()}
  (function(){var host=panelEl.rock.querySelector('.pb'),dd=W.dropdown({ctl:'RockPreset',label:'Preset',wide:true,opts:function(){var RM=window.MEASURED&&MEASURED.rockPresets||{};function mark(k){var m=RM[k||'wall'];return m?' \u2014 '+(m.gate?'passes':'fails')+', '+m.bodies+(m.bodies===1?' body':' bodies')+', '+m.dropped+' dropped':''}return [{v:'',text:'the wall\'s own'+mark('')}].concat(Object.keys(Stone.controls.PRESETS).map(function(k){return {v:k,text:(Stone.controls.PRESETS[k].label||k)+mark(k)}}))},get:function(){return rockPreset},set:function(k){applyRockPreset(k)}});host.insertBefore(dd,host.firstChild);var nt=el0('p','rpnote');nt.textContent='The Weathered Stone looks were made for a lone stone, and many still break a wall. Each is marked with what it does on preset L (draft, seed 3).';host.insertBefore(nt,dd.nextSibling)})();

  /* the rock's stages are cards too: collapsed they show their strength and main settings; a click on the name opens them. The panel is the rock library's own; this only adds a summary line and a fold to what it made. */
  function stageSummary(sid){var C=Stone.controls,sr=C.strengthRow(sid),bits=[];function val(c){var v=RP.P[c.key];return c.step>=1?Math.round(v):String(+(+v).toFixed(2))}
    bits.push('<span>'+val(sr)+'</span>');C.settingsRows(sid).filter(function(c){return c.main&&FRESHROCK&&FRESHROCK[c.key]!==RP.P[c.key]}).forEach(function(c){bits.push('<b class="chg">'+(c.label||c.word).replace('preserve ','keep ')+' '+val(c)+'</b>')});
    var b=RP.off[sid]?'<b class="bg">off</b> ':(RP.solo===sid||SOLO===sid)?'<b class="bg">solo</b> ':'';return b+bits.join(' &middot; ')}
  function rockSummary(){var C=Stone.controls,on=C.STAGES.filter(function(s){return RP.P[s.id]>0&&!RP.off[s.id]}).map(function(s){return s.id});return (rockPreset?rockPreset+' \u00b7 ':'')+(on.length?on.join('+'):'none')}
  Stone.controls.STAGES.forEach(function(s){var st=$('st_'+s.id);if(!st)return;st.classList.add('collapsed');var nm=st.querySelector('.nm'),sum=el0('span','gsum');nm.appendChild(sum);nm.setAttribute('role','button');nm.tabIndex=0;nm.setAttribute('aria-expanded','false');
    /* one way to expand: the card's header. The rock panel's own caret (the library's, kept for Weathered Stone) is hidden here; it still carries the state, so opening the card opens all of the stage's settings, the 'more' ones too, and the status number opens the card as it opened them */
    var cb=st.querySelector('button.cb');function sync(){var open=cb.getAttribute('aria-expanded')==='true';st.classList.toggle('collapsed',!open);nm.setAttribute('aria-expanded',String(open))}
    cb.addEventListener('click',sync);
    function flip(){cb.click()}
    nm.addEventListener('click',function(e){if(e.target.closest('.info,.eye'))return;flip()});nm.addEventListener('keydown',function(e){if((e.key==='Enter'||e.key===' ')&&e.target===nm){e.preventDefault();flip()}});
    W.reg(function(){sum.innerHTML=stageSummary(s.id)})});
  function followLevel(l){if(levelManual||stage===l)return;stage=l;W.refreshAll();go()}
  /* a panel goes back to the preset's values: its rows, its stages' on/off, and (Rock) its stages */
  function resetPanel(id){var fresh=JSON.parse(JSON.stringify(DEFAULTS)),q=PRESETS[presetKey].p,i;for(i in q)fresh[i]=JSON.parse(JSON.stringify(q[i]));
    ROWS.forEach(function(r){if(r.panel!==id)return;
      if(r.kind==='weighting'){var b=r.path.slice(0,r.path.length-2);['w','a','c'].forEach(function(x){ST.set(b+'.'+x,W.getPath(fresh,b+'.'+x))});return}
      var v=W.getPath(fresh,r.path);if(v!==undefined)ST.set(r.path,JSON.parse(JSON.stringify(v)));else delete P[r.path]});
    GROUPS.forEach(function(g){if(g.panel===id&&g.onoff)AP.on[g.state==='band'?'band':'outl']=1});
    if(id==='rock'){RP.P=rockPreset?rockPresetValues(rockPreset):rockParams(PRESETS[presetKey].rock);RP.off={};RP.solo='';pn.sync()}
    SOLO='';RP.solo='';lastRockSolo='';areaSync();W.refreshAll();go()}
  /* ---- the view controls: View, Target, Rows, Projection, Level ---- */
  var targetW,rowsW,levelW,viewW;
  viewW=W.choice({ctl:'View',label:'View',list:true,optctl:'view-',opts:function(){return [['shaded','Shaded'],['over','Overhang'],['brush','Dry brush'],['lines','Lines']].map(function(o){return {v:o[0],text:o[1],img:W.iconFor('View',o[0])}})},get:function(){return mode},set:function(v){mode=v;draw()}});
  targetW=W.choice({ctl:'Target',label:'Target',opts:function(){return [{v:0,text:'Off'},{v:1,text:'On'}]},get:function(){return target||stage===0?1:0},set:function(v){target=!!v;draw()}});
  rowsW=W.choice({ctl:'Rows',label:'Rows',opts:function(){return [{v:0,text:'Off'},{v:1,text:'On'}]},get:function(){return rowsOn||stage===1?1:0},set:function(v){rowsOn=!!v;draw()}});
  var projW=W.choice({ctl:'Projection',label:'Projection',opts:function(){return [{v:1,text:'Perspective'},{v:0,text:'Orthographic'}]},get:function(){return PROJ.persp?1:0},set:function(v){PROJ.persp=!!v;draw()}});
  levelW=W.choice({ctl:'Level',label:'Level',strip:true,optctl:'level-',opts:function(){return [0,1,2,3,4,5].map(function(i){return {v:i,text:'L'+i,img:W.iconFor('Level',i),title:['L0 footprint and crest','L1 row planes','L2 blocks','L3 joints','L4 the rock','L5 union'][i]}})},get:function(){return stage},set:function(v){stage=v;levelManual=true;go()}});
  /* the view controls fold into one icon on the canvas (beside reset and the gate dot) and open as a small popover; the Level strip stays on the canvas as a compact chip, because it is used all the time */
  var xcv=document.querySelector('#xp .xcv'),vpop=el0('div','vpop'),vbtn=el0('button','ovl ovl-reset ovl-opts');vpop.id='xp-vpop';vpop.hidden=true;vpop.setAttribute('role','dialog');vpop.setAttribute('aria-label','View options');
  [viewW,targetW,rowsW,projW].forEach(function(w){vpop.appendChild(w)});
  vbtn.type='button';vbtn.setAttribute('data-ctl','ViewOptions');vbtn.setAttribute('aria-label','View options');vbtn.setAttribute('aria-haspopup','dialog');vbtn.setAttribute('aria-expanded','false');vbtn.title='View options';
  vbtn.innerHTML='<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/></svg>';
  function vpopShow(o){vpop.hidden=!o;vbtn.setAttribute('aria-expanded',String(o))}
  vbtn.onclick=function(e){e.stopPropagation();vpopShow(vpop.hidden)};
  document.addEventListener('click',function(e){if(!vpop.hidden&&!vpop.contains(e.target)&&!e.target.closest('.ddl'))vpopShow(false)});
  document.addEventListener('keydown',function(e){if(e.key==='Escape'&&!vpop.hidden){vpopShow(false);vbtn.focus()}});
  var vlev=el0('div','vlevel');vlev.appendChild(levelW);
  xcv.appendChild(vbtn);xcv.appendChild(vpop);xcv.appendChild(vlev);
  function setPreset(k){presetKey=k;levelManual=false;P=JSON.parse(JSON.stringify(DEFAULTS));var q=PRESETS[k].p;for(var i in q)P[i]=JSON.parse(JSON.stringify(q[i]));P.seed=3;rockPreset='';RP.P=rockParams(PRESETS[k].rock);RP.off={};RP.solo='';SOLO='';lastRockSolo='';AP.on={band:1,outl:1};FRESH=JSON.parse(JSON.stringify(P));FRESHROCK=JSON.parse(JSON.stringify(RP.P));pn.sync();sync();go()}
  var xrefLocal=null;function refSrc(k){var un=(xrefLocal!=null&&arguments[1])?xrefLocal:!!window.UNP;return (un&&window.REFSU&&window.REFSU[k])?window.REFSU[k]:REFS[k]}
  function sync(){W.refreshAll();areaSync();$('xp-ref').src=refSrc(LAST[presetKey],1);$('xp-refc').innerHTML=(['T','R','S','fHeight','fWear'].indexOf(presetKey)>=0?'No reference photo of its own; the nearest panel is shown. ':'Reference '+(presetKey==='N'?'forms N':presetKey==='Lf'?'forms L':'panel '+presetKey)+'. ')+PRESETS[presetKey].name+'.'}

  /* drag to orbit */
  bindOrbit(cv,cam,draw,function(z){var zs=$('xp-zoom');if(zs)zs.value=z});$('xp-zoom').oninput=function(){cam.zoom=+this.value;draw()};$('xp-zreset').onclick=function(){cam.reset()};
  function sceneTris(){var b=cur,tris=[];
    var show=function(list,tag){return trisOf(list,tag)};
    if(stage<2){return b.fig?figureTris(b.fig[0],b.fig[1],DIRT):figureTris(b.g.T/2+16*b.g.f,-b.g.W/2+14*b.g.f,0)}
    if(stage>=5&&gate){tris=gate.tris.slice();tris.forEach(function(t){t.tag=t.p[0][2]<DIRT+1e-3&&t.p[1][2]<DIRT+1e-3&&t.p[2][2]<DIRT+1e-3?'dirt':(t.n[2]>0.99&&Math.abs(t.p[0][2]-DIRT)<1e-3?'dirt':'stone')})}
    else{tris=show(b.polys,'stone');tris=tris.concat(show([b.core],'core'),show([b.dirt],'dirt'))}
    var f=b.g.f,fx=b.g.T/2+16*f;tris=tris.concat(b.fig?figureTris(b.fig[0],b.fig[1],DIRT):figureTris(fx,-b.g.W/2+14*f,0));
    return tris.filter(function(t){return t.tag!=='dirt'})}
  /* the view's height covers the whole crest: 82 mm as before for a low crest, more when top is high (the target and the wall are never cut off by the canvas) */
  function vspan(b){var f=b.g.f;return Math.max(82*f,b.g.topHi+DIRT+22*f,(b.g.T+34*f)*0.9)}
  /* the model is fitted to the canvas's shorter side (the frame was drawn for a canvas a little wider than tall), so a wide or tall viewer only adds space around it */
  function fitScale(span){return cam.zoom*(Math.min(cv.height,cv.width*0.91)/span)/cv.width}
  function drawRows(b){var f=b.g.f,sc=fitScale(b.view?b.view.span:vspan(b)),ce=b.view?b.view.center:[1.5*f,0,vspan(b)/2-16*f];drawRowsCore(cv,cam,b,sc,ce)}
  function drawTarget(b){var f=b.g.f,sc=fitScale(b.view?b.view.span:vspan(b)),ce=b.view?b.view.center:[1.5*f,0,vspan(b)/2-16*f];drawTargetCore(cv,cam,b,sc,ce)}
  function draw(){
    if(!cur)return;var b=cur,f=b.g.f;
    var vm=vmode;$('xp-costs').hidden=vm!=='costs';$('xp-lines').hidden=!(mode==='lines'&&vm==='3d');
    if((vm==='plan'||vm==='elev')){cv.hidden=false;el.hidden=true;drawPE();var src=$(vm==='plan'?'xp-pl':'xp-ev'),c2=cv.getContext('2d'),css0=getComputedStyle(document.body);c2.setTransform(1,0,0,1,0,0);c2.fillStyle=css0.backgroundColor||'#fff';c2.fillRect(0,0,cv.width,cv.height);var s0=Math.min(cv.width/src.width,cv.height/src.height)*0.96*cam.zoom,w0=src.width*s0,h0=src.height*s0;c2.drawImage(src,(cv.width-w0)/2,(cv.height-h0)/2,w0,h0);$('xp-leg').textContent=$(vm==='plan'?'xp-plc':'xp-evc').textContent;return}
    if(vm==='costs'){return}
    if((stage<2&&mode==='lines')){cv.hidden=true;el.hidden=false;drawElev(b);$('xp-leg').textContent=stage===0?'L0: the crest map, a height at every column across the face':'L1: the joints of every course, carried up or shifted';}
    else{cv.hidden=false;el.hidden=true;
      var tris=sceneTris(),big=Math.max(b.g.T+34*f,70*f);render(cv,tris,cam,fitScale(b.view?b.view.span:vspan(b)),b.view?b.view.center:[1.5*f+0,0,vspan(b)/2-16*f],pack(238,240,244),mode==='lines'?'shaded':mode,{dark:isDarkTheme(),px:cv.width/Math.max(1,cv.clientWidth),grid:mode!=='lines',z:DIRT-0.02,S:SQUARE*f,ox:b.fig?0:-b.g.T/2,oy:b.fig?0:-b.g.W/2,fx:b.fig?17.5*f:0,fy:b.fig?17.5*f:0,size:SQUARE*f*(b.fig?1:Math.max(b.g.T,b.g.W)/(SQUARE*f))});
      $('xp-leg').textContent=({shaded:'Drag to turn it. The 28 mm figure stands beside the tile.',over:'Green and grey: up to 55 degrees. Orange: 55 to 65. Red: past 65. Blue: a bridge, a flat underside over a gap between a stone\'s supports (the gate still counts it). '+(gate?'Judged on the union.':'Judged on the blocks; the union follows.'),brush:'Dry brush: a dark undercoat, a light coat on what looks up and on every worn edge.',lines:'Lines: see the slices below.'})[mode];if(stage===1)$('xp-leg').textContent='L1: the row planes, fixed for the whole wall before any block: each row between the height dial\'s min and max, tilted by dip, waved by wave. Drag to turn it.'}
    if(!cv.hidden&&(target||stage===0))drawTarget(b);
    if(!cv.hidden&&(rowsOn||stage===1))drawRows(b);
    if(mode==='lines')drawLines();
  }
  function drawPE(){var b=cur;if(!b)return;var css=getComputedStyle(document.body),ink=css.color,g=b.g,f=g.f,cells=true;
    var pl=$('xp-pl'),c=pl.getContext('2d'),W=pl.width,H=pl.height,ev=$('xp-ev'),d=ev.getContext('2d');
    c.clearRect(0,0,W,H);d.clearRect(0,0,W,H);c.fillStyle='rgba(120,130,150,.1)';c.fillRect(0,0,W,H);d.fillStyle='rgba(120,130,150,.1)';d.fillRect(0,0,W,H);
    var S=Math.min((W-30)/g.T,(H-30)/(g.yb-g.y0+2)),ox=15-g.x0*S,oy=15-(g.y0-1)*S;
    function PX(x){return ox+x*S}function PY(y){return oy+(y-0)*S*0+ (H-15)-(y-(g.y0-1))*S}
    var cols=['rgba(30,154,214,.9)','rgba(214,180,0,.9)','rgba(214,51,140,.9)','rgba(82,199,145,.9)','rgba(177,138,232,.9)','rgba(58,197,205,.9)'];
    {var shown=[0,1,2];shown.forEach(function(ci){var seen={};b.l2.blocks.filter(function(x){return x.c===ci}).forEach(function(x){var P=x.polyT;c.beginPath();P.forEach(function(q,i){if(i)c.lineTo(PX(q[0]),PY(q[1]));else c.moveTo(PX(q[0]),PY(q[1]))});c.closePath();
        if(ci===0){c.fillStyle='rgba(140,150,168,.35)';c.fill()}c.lineWidth=ci===0?2.2:1.6;c.setLineDash(ci===0?[]:ci===1?[7,5]:[2,4]);c.strokeStyle=ci===0?ink:cols[ci];c.stroke()})});c.setLineDash([]);
      $('xp-plc').innerHTML='Plan, from above: course 0 solid, course 1 dashed, course 2 dotted. A cell that is carried up is the same polygon in two lines; a shifted one has moved. The face is the bottom edge.'}

    /* elevation: every block's face as a box (cells: the box round the polygon) */
    var Se=Math.min((W-30)/g.T,(H-30)/(g.topHi+DIRT+4));
    function EX(x){return 15+(x-g.x0)*Se}function EY(z){return H-15-z*Se}
    d.fillStyle='rgba(150,121,92,.55)';d.fillRect(EX(g.x0),EY(DIRT),g.T*Se,DIRT*Se);d.lineWidth=1.5;
    b.l2.blocks.forEach(function(x){if(cells&&!x.free)return;d.strokeStyle=ink;d.fillStyle='rgba(140,150,168,'+(0.18+0.04*(x.c%3))+')';
      {(x.fe||[]).forEach(function(e){var xa=Math.min(e.a[0],e.b[0]),xb=Math.max(e.a[0],e.b[0]);d.beginPath();d.rect(EX(xa),EY(x.zt),(xb-xa)*Se,(x.zt-x.zb)*Se);d.fillStyle='rgba(140,150,168,'+(0.12+0.5*Math.abs(e.n[0])+0.1*(x.c%3))+')';d.fill();d.stroke()})}
      });
    $('xp-evc').innerHTML='Elevation, from the front: every face of every block (a cell shows two or three angled faces; the more it turns from the front the darker the box).'}
  function drawElev(b){var c=el.getContext('2d'),W=el.width,H=el.height,g=b.g,f=g.f,s=Math.min((W-60)/g.T,(H-60)/(g.topHi+DIRT+4)),ox=30-g.x0*s,oy=H-30,css=getComputedStyle(document.body),ink=css.color;
    c.clearRect(0,0,W,H);c.fillStyle='rgba(120,130,150,.12)';c.fillRect(0,0,W,H);
    function X(x){return ox+x*s}function Y(z){return oy-z*s}
    c.fillStyle='rgba(150,121,92,.55)';c.fillRect(X(g.x0),Y(DIRT),g.T*s,DIRT*s);
    c.lineWidth=2;c.strokeStyle=ink;
    if(stage===0){c.beginPath();for(var x=g.x0;x<=g.x1;x+=0.3){var z=g.crest(x);if(x===g.x0)c.moveTo(X(x),Y(z));else c.lineTo(X(x),Y(z))}c.stroke();
      c.setLineDash([6,5]);c.strokeStyle='rgba(200,60,50,.8)';[g.topLo+DIRT,g.topHi+DIRT].forEach(function(z){c.beginPath();c.moveTo(X(g.x0),Y(z));c.lineTo(X(g.x1),Y(z));c.stroke()});c.setLineDash([])}
    else{b.l1.planes.forEach(function(pl){c.strokeStyle='rgba(15,138,126,1)';c.lineWidth=2;c.beginPath();c.moveTo(X(g.x0),Y(pl.z+planeOff(pl,g.x0,g.y0)));c.lineTo(X(g.x1),Y(pl.z+planeOff(pl,g.x1,g.y0)));c.stroke()});
      c.strokeStyle='rgba(200,60,50,.9)';c.setLineDash([6,5]);c.beginPath();for(var x2=g.x0;x2<=g.x1;x2+=0.3){var zz=g.crest(x2);if(x2===g.x0)c.moveTo(X(x2),Y(zz));else c.lineTo(X(x2),Y(zz))}c.stroke();c.setLineDash([])}
    c.fillStyle=ink;c.font='600 22px system-ui';c.fillText(Math.round(g.T)+' mm',X(0)-30,H-6)}
  /* the Rock region's rules, as facts, and what this build did with them */
  function facts(){var b=cur,e=$('xp-facts');if(!e||!b)return;e.innerHTML='<b>Support</b> the convex hull of the cut tops below the stone (shrink-wrapped; a gap inside it is a bridge, blue in the Overhang view).<br><b>Top</b> its row\'s top, under the crest.<br><b>Head joints</b> never move.'+'<br>'+(rockSum?rockSum.stones+' stones; ':'')+(b.rep.areaParts||0)+' outline parts.'}
  /* the magenta views on the two area cards: Region draws the envelope; Rock region draws the stone the card picked (its cell, graded outline, support and top once the rock has it) */
  function thumbs(){if(!cur)return;var rc=panelEl.region&&panelEl.region.querySelectorAll('.pth canvas'),sc=panelEl.rockregion&&panelEl.rockregion.querySelectorAll('.pth canvas');
    try{if(rc&&rc.length===3&&cur.g.offs)regionThumbs(rc,cur);if(sc&&sc.length===3){var blk=pickStone(cur);if(blk){var R=RockRun.current,job=R&&R.b===cur?R.jobs.filter(function(j){return j.block===blk})[0]:null;if(job)stoneThumbs(sc,cur,job);else stoneCell(sc,cur,blk)}}}catch(e){console.error('[thumbs] '+e.message)}}
  /* ---- the spec: copy everything that is on screen as text, load it back from pasted text ---- */
  var SPECT=null;function specTables(){return SPECT||(SPECT={ROWS:ROWS,PANELS:PANELS,rockKeys:Stone.controls.CONTROLS.filter(function(c){return c.stage!=='stone'||c.word==='patch'}).map(function(c){return c.key})})}
  function snapshot(){var off=[];if(!AP.on.band)off.push('band');if(!AP.on.outl)off.push('outl');Stone.controls.STAGES.forEach(function(s){if(RP.off[s.id])off.push(s.id)});
    return {build:window.BUILD_ID||'dev',preset:presetKey,system:P.system,seed:P.seed,level:stage,mode:mode,target:target,rows:rowsOn,projection:PROJ.persp?'perspective':'orthographic',off:off,solo:SOLO,P:P,FP:FRESH,tier:tier,rock:RP.P,frock:FRESHROCK,rockPreset:rockPreset}}
  function specOnly(){return Spec.write(snapshot(),specTables())}
  function resultText(){var b=cur,L=[];if(!b)return 'no build yet';var r=b.rep,t=r.time,tr=r.tris;
    if(rockBusy)L.push('status building '+rockProg);else if(!gate)L.push('status '+(stage<2?'no verdict at this level':'measuring'));
    else{var ok=gate.status==='NoError'&&gate.bodies===1&&gate.bandPct<=0.1&&gate.redPct<=0.1,ap=bridgeStats(gate.tris);L.push('status '+(ok?'passes':'fails')+' the gate as counted; '+(ap.pass?'passes':'fails')+' with bridge faces apart');
      L.push('triangles '+gate.numTri+' bodies '+gate.bodies+' manifold '+gate.status);
      L.push('overhang past65 '+gate.redPct.toFixed(3)+'% band '+gate.bandPct.toFixed(3)+'% ; apart past65 '+ap.redApart.toFixed(3)+'% band '+ap.bandApart.toFixed(3)+'%')}
    if(planesNote())L.push('planes '+planesNote());if(r.planeGuard)L.push('planes DEFECT: the guard scaled a plane '+r.planeGuard+' times');
    L.push('stones '+(rockSum?rockSum.stones:'-')+' dropped '+(r.dropped||0)+(rockSum?' cached '+rockSum.cached+' checks '+((rockSum.bad||rockSum.islands)?'FAILED bad '+rockSum.bad+' islands '+rockSum.islands:'held'):''));
    var tm=['L0 '+(t.L0||0),'L1 '+(t.L1||0),'L2 '+((t.L2||0)+(t.L2b||0)),'L3 '+(t.L3||0)];if(rockSum)tm.push('L4 '+rockSum.wallMs);if(gate)tm.push('L6 '+(gate.tHull+gate.tUnion+gate.tMesh));
    L.push('ms '+tm.map(function(x){var p=x.split(' ');return p[0]+' '+(+p[1]).toFixed(1)}).join(' '));return L.join('\n')}
  function specText(){return specOnly()+'\n\n[result]\n'+resultText()+'\n\n[problems]\n'+((window.__problems&&window.__problems.length)?window.__problems.join('\n'):'none')+'\n'}
  function loadSpec(text){var S=Spec.read(text),h=S.header;if(!h.preset||!PRESETS[h.preset[0]])throw new Error('the spec names no preset this page knows');
    setPreset(h.preset[0]);if(h.system)P.system=h.system[0];if(h.seed)P.seed=+h.seed[0];
    if(h.quality)tier=h.quality[0];var rp=S.values['rock.preset']&&S.values['rock.preset'][0];if(rp&&rp!=='none'&&Stone.controls.PRESETS[rp]){rockPreset=rp;RP.P=rockPresetValues(rp);FRESHROCK=JSON.parse(JSON.stringify(RP.P))}Object.keys(S.values).forEach(function(k){if(k==='rock.preset')return;var f=Spec.get(FRESH,k);var v=S.values[k];Spec.set(P,k,Array.isArray(f)||(f===undefined&&v.length>1)?v.map(Number):typeof f==='number'||f===undefined?+v[0]:v[0])});
    Object.keys(S.rock).forEach(function(k){RP.P[k]=+S.rock[k]});
    AP.on={band:1,outl:1};RP.off={};(h.off||[]).forEach(function(id){if(id==='band'||id==='outl')AP.on[id]=0;else RP.off[id]=true});SOLO=(h.solo&&h.solo[0])||'';RP.solo=Stone.controls.STAGES.some(function(s){return s.id===SOLO})?SOLO:'';lastRockSolo=RP.solo;
    if(h.level){stage=+h.level[0];levelManual=true}if(h.view)mode=h.view[0];target=h.target?h.target[0]==='1':false;rowsOn=h.rows?h.rows[0]==='1':false;if(h.projection)PROJ.persp=h.projection[0]!=='orthographic';
    pn.sync();W.refreshAll();areaSync();go()}
  (function(){var btn=$('xp-spec'),box=$('xp-specbox'),ta=$('xp-spectext'),msg=$('xp-specmsg');
    function say(t){msg.textContent=t}
    function copy(){ta.focus();ta.select();var done=function(){say('Copied to the clipboard.')},fail=function(){ta.focus();ta.select();say('The browser did not allow the clipboard here: the text is selected, press Cmd-C (Ctrl-C).')};
      try{if(navigator.clipboard&&navigator.clipboard.writeText)navigator.clipboard.writeText(ta.value).then(done,fail);else{if(document.execCommand&&document.execCommand('copy'))done();else fail()}}catch(e){fail()}}
    btn.onclick=function(e){e.stopPropagation();if(!box.hidden){box.hidden=true;return}ta.value=specText();box.hidden=false;copy()};
    $('xp-speccopy').onclick=function(){ta.value=ta.value.indexOf('[result]')>=0&&ta.dataset.edited!=='1'?specText():ta.value;copy()};
    ta.addEventListener('input',function(){ta.dataset.edited='1';say('')});
    $('xp-specload').onclick=function(){try{loadSpec(ta.value);ta.dataset.edited='0';say('Loaded. The page now shows this spec.')}catch(e){say('Could not load: '+e.message)}};
    $('xp-specclose').onclick=function(){box.hidden=true};
    document.addEventListener('keydown',function(e){if(e.key==='Escape'&&!box.hidden)box.hidden=true});
    self.spec=specText;self.specOnly=specOnly;self.loadSpec=loadSpec})();
  /* ---- maximize: the viewer's box becomes a fixed layer over the page (no Fullscreen API: the artifact frame may refuse it, so it is never asked). The same canvas, camera, selection and build stay; only the box changes, and the canvas follows it. Esc or the same button comes back. ---- */
  (function(){var btn=$('xp-max'),xv=document.querySelector('#xp .xview'),on=false,svg=function(p){return '<svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">'+p+'</svg>'},
      EXPAND=svg('<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/>'),COLLAPSE=svg('<path d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5"/>');
    function set(o){on=o;xv.style.minHeight=o?xv.offsetHeight+'px':'';xcv0.classList.toggle('xmax',o);document.documentElement.classList.toggle('xmaxed',o);btn.setAttribute('aria-pressed',String(o));btn.title=o?'Back to the page':'Maximize';btn.setAttribute('aria-label',o?'Back to the page':'Maximize the viewer');btn.innerHTML=o?COLLAPSE:EXPAND;
      if(sizeCanvas()&&cur)draw()}
    btn.innerHTML=EXPAND;btn.onclick=function(e){e.stopPropagation();set(!on)};
    document.addEventListener('keydown',function(e){if(e.key!=='Escape'||!on)return;var open=['xp-vpop','xp-gpop','xp-specbox'].some(function(i){var n=$(i);return n&&!n.hidden});if(!open)set(false)},true);
    self.maximize=set})();
  function stats(){facts();thumbs();var b=cur,r=b.rep,t=r.time,tr=r.tris,e=$('xp-st'),rows=[],tot=0,k;
    function f2(v){return v==null?'-':(v<10?v.toFixed(2):v.toFixed(1))}
    var steps=[['L0','Footprint and crest',t.L0,0,'cell grid '+b.g.cells+' x 4 B'],['L1','Row planes and sites',t.L1,0,r.courses+' rows, '+r.joints+' sites'],['L2','Courses and blocks',(t.L2||0)+(t.L2b||0),tr.L2,b.l2.blocks.length+' blocks; '+r.raised+' raised, '+r.dropped+' dropped'],['L3','Joints and grooves',t.L3,tr.L3,''],['L4','Facets and chips',t.L4,tr.L4,'']];
    var h='<table><tr><th>Level<th class="n">ms (this browser)<th class="n">triangles added<th>note</tr>';
    steps.forEach(function(s,i){h+='<tr><td>'+s[0]+' '+s[1]+'<td class="n">'+f2(s[2])+'<td class="n">'+(s[3]==null?'-':s[3])+'<td>'+s[4]});
    var pt=0;b.polys.forEach(function(P){pt+=triCount(P)});h+='<tr><td><b>the cliff writes (L0 to L4)</b><td class="n"><b>'+f2((t.L0||0)+(t.L1||0)+(t.L2||0)+(t.L2b||0)+(t.L3||0)+(t.L4||0))+'</b><td class="n"><b>'+pt+'</b><td>'+Math.round((b.rep.bytes.polys||0)/1024)+' KB of faces (estimated)';
    h+='</table>';
    if(gate){var gt=gate;var lv=(t.L0||0)+(t.L1||0)+(t.L2||0)+(t.L2b||0)+(t.L3||0)+(t.L4||0),un=gt.tHull+gt.tUnion+gt.tMesh,ins=gt.ins||{};h+='<table><tr><th colspan=2>L5 &middot; The union (Manifold 3.5.3, in this page; measured here)</tr>'+
      '<tr><td>goes in: '+ins.blocks+' blocks ('+ins.pieces+' pieces; a stone on several facets is several pieces), 1 core, 1 ground plate = <b>'+gt.inputs+' solids</b>, '+gt.inTris+' triangles, '+(gt.pairs==null?'not counted for meshes':gt.pairs)+' touching pairs (hulls sharing volume or a face; '+(gt.pairsBox==null?'':gt.pairsBox+' with meeting boxes')+')<td class="n">in: '+gt.inTris+' triangles</tr>'+
      '<tr><td>union: hulls '+f2(gt.tHull)+' ms, union '+f2(gt.tUnion)+' ms, mesh '+f2(gt.tMesh)+' ms; total '+f2(un)+' ms, '+(lv>0&&un>0?(un/lv).toFixed(0)+' times the six levels ('+f2(lv)+' ms)':'')+'<td class="n">out: '+gt.numTri+' triangles</tr>'+
      '<tr><td>from '+pt+' written triangles: top '+gt.split.top+', side '+gt.split.side+', under '+gt.split.under+', bed '+gt.split.bed+'<td class="n">union minus written: '+(gt.numTri-pt>=0?'+':'')+(gt.numTri-pt)+' (the core and plate, splits where solids meet, less what is internal)</tr>'+
      '<tr><td>watertight (status), bodies (loose parts)<td class="'+(gt.status==='NoError'&&gt.bodies===1?'ok':'bad')+'">'+gt.status+', '+gt.bodies+' body'+(gt.bodies===1?'':'ies')+'</tr>'+
      '<tr><td>overhang 55 to 65 degrees, of the downward area ('+gt.downA.toFixed(0)+' mm2)<td class="'+(gt.bandPct<=0.1?'ok':'bad')+'">'+gt.bandPct.toFixed(3)+' % (budget 0.1)</tr>'+
      '<tr><td>overhang past 65 degrees<td class="'+(gt.redPct<=0.1?'ok':'bad')+'">'+gt.redPct.toFixed(3)+' % (budget 0.1)</tr>';
      if(thin)h+='<tr><td>parts that vanish under one 0.45 mm line, in '+thin.slices+' slices (knife edges of joints, attached to a body, reach '+(100*thin.lostFracMax).toFixed(1)+' % of a slice)<td class="'+(thin.lostParts===0?'ok':'bad')+'">'+thin.lostParts+' parts</tr><tr><td>gap under 0.2 mm that a slicer closes (parts over 0.15 mm2)<td class="'+(thin.gapParts===0?'ok':'bad')+'">'+thin.gapParts+' gaps ('+thin.t.toFixed(0)+' ms to check)</tr>';
      h+='</table>'}
    else h+='<p class="small">Measuring the union...</p>';
    var gpu=gate?gate.numTri:pt;h+='<p class="small">GPU: this view draws '+gpu+' triangles in '+(gate?2:3)+' draw calls (cliff, plate, figure) in the editor; this page draws them on the CPU.'+(performance&&performance.memory?' JS heap now '+Math.round(performance.memory.usedJSHeapSize/1048576)+' MB (Chrome, measured).':'')+'</p>';
    gateUI();
    e.innerHTML=h;$('xp-mbody').innerHTML=watch();var rb=document.getElementById('xp-routes');if(rb)rb.onclick=function(){runRoutes()};window.dispatchEvent(new CustomEvent('xp-cost',{detail:{rep:r,gate:gate,thin:thin,pt:pt}}))}
  var HIST=[],ROUTES=null;
  function watch(){var h='<table><tr><th colspan=8>Union watch: the last measurements, newest first (this browser; change the preset, the run, the system or a dial and watch it)</tr><tr><th>preset<th>run<th>system<th class="n">solids in<th class="n">touching pairs<th class="n">triangles in<th class="n">triangles out<th class="n">union ms (hulls+union+mesh)</tr>',mx=1;
    HIST.forEach(function(x){if(x.ms>mx)mx=x.ms});
    HIST.forEach(function(x){h+='<tr><td>'+x.k+'<td>'+x.tiles+'x1<td>'+x.sys+'<td class="n">'+x.n+'<td class="n">'+x.pairs+'<td class="n">'+x.bt+'<td class="n">'+x.tris+'<td class="n">'+x.ms.toFixed(1)+' <span class="bar" style="display:inline-block;height:8px;width:'+Math.round(90*x.ms/mx)+'px;background:var(--ochre);border-radius:2px"></span>'});
    h+='</table><p class="small"><button type="button" id="xp-routes" class="rbtn">Time the four routes on this build</button> '+(ROUTES?'sequential '+ROUTES.seqMs.toFixed(1)+' ms, batch '+ROUTES.batchMs.toFixed(1)+' ms, row by row '+ROUTES.rowsMs.toFixed(1)+' ms, touching groups composed '+ROUTES.groupsMs.toFixed(1)+' ms ('+ROUTES.groups+' group'+(ROUTES.groups===1?'':'s')+'); median of 3, on '+ROUTES.n+' solids, measured here.':'Batch, one at a time, course by course, and by touching groups: measured on the build in front of you.')+'</p>';return h}
  function runRoutes(){if(!MF||!cur)return;ROUTES=studyUnion(cur,3,false);stats()}
  /* printability: one dot over the canvas (green passes, red fails, grey is measuring), a symbol in it, and a popover with the numbers */
  /* ONE BUILD RECORD for one build: a status chip on the canvas (building, passes, fails) and one popover: the steps in build order with their times and counts (the region and courses steps, the rock course by course, the foot, the union) and the gate's verdict both ways. The rock library's own stage records (the number beside each stage) are unchanged. */
  var rockProg='';
  function fmtN(n){return n==null?'':(+n).toLocaleString()}
  function recordHTML(){var b=cur,r=b.rep,t=r.time,tr=r.tris,S=rockSum,rows='',row=function(a,ms,note,cls){return '<tr'+(cls?' class="'+cls+'"':'')+'><td>'+a+'<td class="n">'+(ms==null?'':fmtMs(ms))+'<td>'+(note||'')+'</tr>'};
    rows+=row('L0 footprint and crest',t.L0,'');rows+=row('L1 row planes and sites',t.L1,r.courses+' rows, '+r.joints+' sites');rows+=row('L2 courses and blocks',(t.L2||0)+(t.L2b||0),b.l2.blocks.length+' blocks; '+r.raised+' raised, '+r.dropped+' dropped; '+fmtN(tr.L2)+' triangles');rows+=row('L3 joints',t.L3,fmtN(tr.L3)+' triangles added');
    if(stage>=4&&ROCK_ON(b.p)){
      if(S){S.courseLog.forEach(function(c){rows+=row('L4 rock, course '+c.course,c.ms,c.stones+' stones'+(c.cuts?'; cuts '+fmtMs(c.cuts):'')+(c.chunks?'; chunks '+fmtMs(c.chunks):'')+(c.sampled?'; chips, pockets, grain, wear '+fmtMs(c.sampled):'')+' (cpu over the stones)')});
        rows+=row('L4 rock, all courses',S.wallMs,S.stones+' stones, '+S.cached+' kept from an earlier build, '+fmtN(S.tris)+' triangles, built in '+RockRun.mode+' at '+Stone.controls.TIERS[tier]+' mm; '+fmtMs(S.cpuMs)+' of work added up'+(S.dropped?'; '+S.dropped+' stones dropped':''))}
      else if(rockBusy)rows+=row('L4 rock',null,rockProg+'...')}
    if(gate)rows+=row('L5 union',gate.tHull+gate.tUnion+gate.tMesh,gate.inputs+' solids, '+fmtN(gate.numTri)+' triangles; '+gate.status+', '+gate.bodies+' body'+(gate.bodies===1?'':'ies'),gate.status==='NoError'&&gate.bodies===1?'':'bad');
    var h='<table><tr><th>Step<th class="n">Time<th>Count</tr>'+rows+'</table>';
    if(S){var ok=!S.bad&&!S.islands;h+='<p class="gline">Rock checks: <span class="'+(ok?'ok':'bad')+'">'+(ok?'every check held on every stone':(S.bad?S.bad+' stones failed a check':'')+(S.islands?(S.bad?'; ':'')+S.islands+' loose parts':''))+'</span>. Specks '+(S.specks||'none')+', voids sealed inside a stone '+(S.sealed||'none')+'.</p>'}
    return h}
  function gateUI(light){var dot=$('xp-gdot'),gd=dot.querySelector('.gd'),pb=$('xp-gpb'),v,txt,html='',head='';
    var running=rockBusy&&stage>=4;
    if(stage<2){v='na';txt='no verdict at this level (nothing is built to print yet)';gd.textContent='\u2013';head='<p><b>No verdict at this level.</b> L0 and L1 are the target and the row planes: there is no stone to print yet. The gate judges the union, from L2 on.</p>'}
    else if(running){v='run';txt='building: '+rockProg;gd.innerHTML='<span class="spin" aria-hidden="true"></span>';head='<p><b>Building: '+rockProg+'.</b> The gate judges the union when the last stone is in.</p>'}
    else if(!gate){v='wait';txt='measuring';gd.textContent='\u00b7';head='<p><b>Measuring the union...</b></p>'}
    else{var ok=gate.status==='NoError'&&gate.bodies===1&&gate.bandPct<=0.1&&gate.redPct<=0.1,ap=bridgeStats(gate.tris);v=ok?'ok':'fail';txt=ok?'passes the gate':'fails the gate';gd.textContent=ok?'\u2713':'\u2715';
      head='<p><b>'+(ok?'Passes':'Fails')+' the gate</b> as it counts, and <b>'+(ap.pass?'passes':'fails')+'</b> with the bridge faces counted apart.</p>'}
    dot.setAttribute('aria-label','Build: '+txt);dot.title='Build: '+txt+' (tap for the record)';gd.className='gd '+v;
    if(light&&$('xp-gpop').hidden)return;
    if(cur&&stage>=2)html+=recordHTML();
    if(gate&&stage>=2){var t=thin,row=function(a,b2,good){return '<tr><td>'+a+'<td class="n '+(good===true?'ok':good===false?'bad':'')+'">'+b2+'</tr>'},ba=0;gate.tris.forEach(function(q){if(q.bridge)ba+=q.area});var ap2=bridgeStats(gate.tris);
      html+='<h4>The gate, both ways</h4><table>'+
       row('overhang past 65&deg;, of the downward area ('+gate.downA.toFixed(0)+' mm&sup2;)',gate.redPct.toFixed(3)+' % (budget 0.1)',gate.redPct<=0.1)+
       row('overhang 55 to 65&deg;',gate.bandPct.toFixed(3)+' % (budget 0.1)',gate.bandPct<=0.1)+
       (ba?row('with the bridge faces counted apart (the gate counts them)',(ap2.pass?'passes':'fails')+': '+ap2.redApart.toFixed(3)+' % past 65&deg;, '+ap2.bandApart.toFixed(3)+' % at 55 to 65&deg;',ap2.pass)+row('bridges, painted blue: a flat underside over a gap between a stone\'s supports',ba.toFixed(0)+' mm&sup2; of the downward area',true):'')+
       row('watertight, loose bodies',gate.status+', '+gate.bodies,gate.status==='NoError'&&gate.bodies===1)+
       (t?row('parts that vanish under one printer line (shown, not part of the verdict)',t.lostParts+' parts')+row('gaps a slicer closes (shown, not part of the verdict)',t.gapParts+' gaps'):row('thin parts and gaps','checking...'))+'</table>'}
    pb.innerHTML=head+html+'<p class="gcl"><button type="button" id="xp-gcosts">Costs view: every number of this build</button></p>'}
  function gpop(open){var p=$('xp-gpop'),d=$('xp-gdot');p.hidden=!open;d.setAttribute('aria-expanded',String(open))}
  $('xp-gdot').onclick=function(e){e.stopPropagation();gpop($('xp-gpop').hidden)};$('xp-gclose').onclick=function(){gpop(false)};
  document.addEventListener('click',function(e){var p=$('xp-gpop');if(!p.hidden&&!p.contains(e.target)&&e.target!==$('xp-gdot'))gpop(false)});document.addEventListener('keydown',function(e){if(e.key==='Escape')gpop(false)});
  /* the build: the cells at once (steps 1 to 7, drawn as soon as they are made), then the rock's stone for each cell, placed as each arrives; the union and the gate run when the last has */
  function go(){var b;targetW.hidden=stage===0;rowsW.hidden=stage===1;W.refreshAll();
    P.rock=Stone.controls.effective(RP.P,RP.off,rockSolo());
    b=build(P,Math.max(stage,2)>=5?5:Math.max(stage,2));if(stage<2)b=build(P,2);
    freeGate(gate);cur=b;gate=null;thin=null;clearTimeout(timer);RockRun.cancel();rockBusy=false;rockSum=null;rockErr=null;pn.clearStages();rockUI();draw();stats();gateUI();W.refreshAll();
    if(stage>=4&&ROCK_ON(b.p)){startRock(b)}else{timer=setTimeout(measure,250)}}
  function startRock(b){var vox=Stone.controls.TIERS[tier];rockBusy=true;rockUI();
    Stone.controls.STAGES.forEach(function(s){if(b.p.rock[s.id]>0)pn.setStat(s.id,{mode:'run'})});
    RockRun.start(b,vox,{onPlaced:function(d,t){rockUI(d,t);if(!drawT)drawT=setTimeout(function(){drawT=0;if(cur===b)draw()},120)},
      onDone:function(sum){if(cur!==b)return;rockBusy=false;rockSum=sum;b.rep.time.L4=sum.wallMs;var t3=0;b.rep.tris.L4=sum.tris-(b.rep.tris.L2+(b.rep.tris.L3||0));rockStatus(sum,b);rockUI();draw();stats();gateUI();clearTimeout(timer);timer=setTimeout(measure,30)},
      onError:function(t){if(cur!==b)return;rockBusy=false;rockErr=t;rockUI()}})}
  function fmtMs(ms){return ms<1?'<1 ms':ms<1000?Math.round(ms)+' ms':(ms/1000).toFixed(2)+' s'}
  /* the number beside each stage: its time over every stone, and whether its checks held on every stone */
  var STEPOF={cuts:'cuts',chunks:'chunks',chips:'sampled',pockets:'sampled',grain:'sampled',wear:'sampled'};
  function rockStatus(sum,b){var Q=b.p.rock,ids=Stone.controls.STAGES.filter(function(s){return Q[s.id]>0}).map(function(s){return s.id}),samp=ids.filter(function(i){return STEPOF[i]==='sampled'});
    ids.forEach(function(id){var step=STEPOF[id],t=sum[step],bad=step==='sampled'?sum.islands:step==='chunks'?sum.bad+sum.sealed:sum.bad,names=samp.map(function(i){return pn.NAME[i]}).join(', ');
      pn.setStat(id,{mode:bad?'warn':'ok',ms:t,text:pn.NAME[id]+': '+(step==='sampled'&&samp.length>1?names+' share one sampling pass, ':'')+fmtMs(t)+' over '+sum.stones+' stones (cpu)'+(bad?'; '+bad+(step==='sampled'?' loose parts':' failed checks'):'; every check held on every stone')})})}
  function rockUI(d,t){var q=RockRun.current;rockProg=rockBusy?'stones '+(d!=null?d:(q?q.done:0))+' of '+(t!=null?t:(q?q.total:0)):'';
    $('xp-rnote').textContent=rockErr?'A stone failed to build: '+rockErr.split('\n')[0]:(stage<4?'The rock acts from L4 on: this level shows the plain cells.':P.rock&&ROCK_ON({rock:P.rock})?'':'Every stage is at 0 or off: each stone is its cell, as the steps before made it.');gateUI(true)}
  function rockStageRecord(id){var st=pn.stat[id],s=rockSum;return {key:'stage:'+id,title:pn.NAME[id]+' — '+(st&&st.mode==='warn'?'a check failed':'every check passed'),mode:st&&st.mode||'ok',html:'<p>'+(st?st.text:'')+'.</p><p class="note">'+Stone.controls.strengthRow(id).show(P.rock)+'. Every stone is built by the same rock, with its own seed.</p>'}}
  function measure(){if(!MF||!cur||rockBusy)return;var b=cur;try{freeGate(gate);gate=unionOf(b);markBridges(gate.tris,b.bridges);if(ROCK_ON(b.p)&&stage>=4){gate.pairs=null;gate.pairsBox=null}else{var h2=hullsOf(b,false),P2=pairsOf(h2.list);gate.pairs=P2.real;gate.pairsBox=P2.cand;freeAll(h2.list)}gate.inTris=0;b.polys.forEach(function(P){gate.inTris+=triCount(P)});gate.inTris+=b.core.faces.reduce(function(a,F){return a+F.pts.length-2},0)+b.dirt.faces.reduce(function(a,F){return a+F.pts.length-2},0);var bs={};b.polys.forEach(function(P){if(P.b)bs[P.b.c+'_'+P.b.i]=1});gate.ins={pieces:b.polys.length,blocks:Object.keys(bs).length};
      HIST.unshift({k:presetKey,tiles:P.tiles,sys:P.system,seed:P.seed,n:gate.inputs,pairs:gate.pairs,ms:gate.tHull+gate.tUnion+gate.tMesh,tris:gate.numTri,bt:gate.inTris});if(HIST.length>10)HIST.pop();var top=0;gate.tris.forEach(function(t){if(t.p[0][2]>top)top=t.p[0][2]});
      thin=window.__nothin?{lostParts:0,gapParts:0,lostFracMax:0,lostMax:0,t:0,slices:0}:thinCheck(gate.u,top);$('xp-z').max=String(Math.ceil(top));$('xp-x').min=String(Math.floor(b.g.xmin!=null?b.g.xmin:-b.g.W/2));$('xp-x').max=String(Math.ceil(b.g.xmax!=null?b.g.xmax:b.g.W/2));
      }catch(e){console.error(e);$('xp-st').innerHTML='<p class="bad">The mesh engine failed: '+String(e&&e.message||e)+'</p>';return}
    stats();draw()}
  self.cam=cam;self.measure=measure;self.setMF=function(){measure()};self.P=function(){return P};self.setPreset=setPreset;self.setMode=function(m){mode=m;draw()};self.get=function(){return {cur:cur,gate:gate,thin:thin,rock:rockSum,busy:rockBusy,tier:tier}};self.tweak=function(o){if(o.rock){Object.assign(RP.P,o.rock);pn.sync()}if(o.p){Object.assign(P,o.p);sync()}if(o.tier)tier=o.tier;if(o.stage!=null)stage=o.stage;W.refreshAll();go()};self.rockParams=function(){return RP.P};self.clearCache=function(){ROCKCACHE.clear()};self.setStage=function(s){stage=s;go()};
  /* Lines: a plan slice with 0.45 mm perimeters, and a side section stacked in 0.2 mm layers */
  function drawLines(){if(!gate)return;var u=gate.u,b=cur,f=b.g.f,z=+$('xp-z').value,xc=+$('xp-x').value;$('xp-zo').textContent=z.toFixed(1)+' mm';$('xp-xo').textContent=xc.toFixed(1)+' mm';
    var css=getComputedStyle(document.body),ink=css.color,pc=$('xp-plan'),c=pc.getContext('2d'),S=pc.width/(b.g.T+6),ox=pc.width/2,oy=pc.height/2;
    c.clearRect(0,0,pc.width,pc.height);c.fillStyle='rgba(120,130,150,.1)';c.fillRect(0,0,pc.width,pc.height);
    function PX(x){return ox+(x-(b.g.cx||0))*S}function PY(y){return oy+(y+(b.g.cy||0))*S}  /* y toward the viewer is up the page */
    function path(polys){c.beginPath();polys.forEach(function(P){P.forEach(function(q,i){if(i)c.lineTo(PX(q[0]),PY(-q[1]));else c.moveTo(PX(q[0]),PY(-q[1]))});c.closePath()})}
    var cs=u.slice(z);if(!cs.isEmpty()){
      path(cs.toPolygons());c.fillStyle='rgba(140,150,168,.45)';c.fill('evenodd');
      var o1=cs.offset(-LINE/2,'Miter',2,8),o2=cs.offset(-LINE*1.5,'Miter',2,8),op=o1.offset(LINE/2,'Miter',2,8),lost=cs.subtract(op),cl=cs.offset(GAPMIN/2,'Miter',2,8).offset(-GAPMIN/2,'Miter',2,8),gap=cl.subtract(cs);
      c.lineWidth=LINE*S;c.lineJoin='round';c.strokeStyle='rgba(60,80,110,.9)';path(o1.toPolygons());c.stroke();c.strokeStyle='rgba(60,80,110,.55)';path(o2.toPolygons());c.stroke();
      c.fillStyle='rgba(215,50,40,.95)';path(lost.toPolygons());c.fill('evenodd');c.fillStyle='rgba(240,150,40,.95)';path(gap.toPolygons());c.fill('evenodd');
      $('xp-lnote').innerHTML='Plan at z = '+z.toFixed(1)+' mm: grey is material, the dark loops are the first two 0.45 mm perimeters, red is material under one line (area '+lost.area().toFixed(2)+' mm2), orange is a gap under 0.2 mm that a slicer would close ('+gap.area().toFixed(2)+' mm2). Right: the side section at x, in 0.2 mm layers; a layer that steps out past the one below by over 0.29 mm is orange, over 0.43 red.'}
    var sc2=$('xp-sec'),d=sc2.getContext('2d');d.clearRect(0,0,sc2.width,sc2.height);d.fillStyle='rgba(120,130,150,.1)';d.fillRect(0,0,sc2.width,sc2.height);
    var rot=u.rotate([0,90,0]),sec=rot.slice(-xc),polys=sec.toPolygons(),top=0;polys.forEach(function(P){P.forEach(function(q){if(q[0]>top)top=q[0]})});
    var Sx=(sc2.width-40)/(b.g.W+4),Sz=Math.min(Sx,(sc2.height-40)/(top+2)),ox2=20+(b.g.W/2+2)*Sx,oz=sc2.height-20;
    function SX(y){return 20+(y+b.g.W/2+2)*Sx}function SZ(z){return oz-z*Sz}
    var prevIv=null,layers=Math.ceil(top/LAYER),k,steps={ok:0,band:0,red:0};
    for(k=0;k<layers;k++){var zc=(k+0.5)*LAYER,xs=[];
      polys.forEach(function(P){for(var i=0;i<P.length;i++){var a=P[i],bq=P[(i+1)%P.length];if((a[0]<=zc)!==(bq[0]<=zc)){xs.push(a[1]+(zc-a[0])/(bq[0]-a[0])*(bq[1]-a[1]))}}});
      xs.sort(function(p,q){return p-q});var iv=[];for(var m=0;m+1<xs.length;m+=2)iv.push([xs[m],xs[m+1]]);
      iv.forEach(function(I){var step=0;if(prevIv){var best=null;prevIv.forEach(function(J){if(J[1]>I[0]&&J[0]<I[1]){if(best===null||J[0]>best)best=J[0]}});if(best!==null)step=best-I[0]}
        d.fillStyle='rgba(140,150,168,.7)';d.fillRect(SX(I[0]),SZ((k+1)*LAYER),Math.max(1,(I[1]-I[0])*Sx),Math.max(1,LAYER*Sz));
        if(step>0.43){d.fillStyle='rgb(215,50,40)';d.fillRect(SX(I[0])-3,SZ((k+1)*LAYER),4,Math.max(2,LAYER*Sz));steps.red++}else if(step>0.286){d.fillStyle='rgb(240,150,40)';d.fillRect(SX(I[0])-3,SZ((k+1)*LAYER),4,Math.max(2,LAYER*Sz));steps.band++}else steps.ok++});
      prevIv=iv}
    d.fillStyle=ink;d.font='600 20px system-ui';d.fillText('face',SX(-b.g.W/2)+2,18);d.fillText('layers: '+layers+', steps orange '+steps.band+', red '+steps.red,24,sc2.height-4);
    window.__sec=steps}
  $('xp-z').oninput=$('xp-x').oninput=function(){drawLines()};
  window.matchMedia&&window.matchMedia('(prefers-color-scheme: dark)').addEventListener&&window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change',function(){draw()});
  function setView(v){vmode=v;[].forEach.call($('xp-vsw').children,function(x){x.setAttribute('aria-pressed',String(x.getAttribute('data-v')===v))});if(cur)draw()}
  (function(){var vs=$('xp-vsw');[['3d','3D'],['plan','Plan'],['elev','Elevation'],['costs','Costs']].forEach(function(o){var b=document.createElement('button');b.type='button';b.setAttribute('data-v',o[0]);b.setAttribute('data-ctl','v-'+o[0]);b.title=o[1];b.setAttribute('aria-label',o[1]);b.innerHTML=W.VIEWI[o[0]];b.onclick=function(){setView(o[0])};vs.appendChild(b)});setView('3d')})();
  document.addEventListener('click',function(e){var gc=e.target&&e.target.id==='xp-gcosts';if(gc){gpop(false);setView('costs')}});
  self.refreshRefs=function(){xrefLocal=null;presetDD.render();var k=LAST[presetKey];$('xp-ref').src=refSrc(k,1);var xr=$('xp-ref');xr.setAttribute('aria-pressed',String(!!window.UNP&&!!(window.REFSU&&window.REFSU[k])))};
  (function(){var xr=$('xp-ref');xr.setAttribute('role','button');xr.tabIndex=0;xr.setAttribute('aria-pressed','false');xr.setAttribute('aria-label','Show unpainted');xr.style.cursor='pointer';
    function flip(){var k=LAST[presetKey];if(!(window.REFSU&&window.REFSU[k]))return;var cur=xrefLocal!=null?xrefLocal:!!window.UNP;xrefLocal=!cur;xr.src=refSrc(k,1);xr.setAttribute('aria-pressed',String(xrefLocal));xr.setAttribute('aria-label',xrefLocal?'Show painted':'Show unpainted')}
    xr.onclick=flip;xr.onkeydown=function(e){if(e.key==='Enter'||e.key===' '){e.preventDefault();flip()}}})();
  /* the explainers: one eye per place, opened only on a click. The rock panel's own "i" explanations become eyes here; the viewer's overlay buttons share one eye on the canvas. */
  (function(){var xv=$('xp'),eo=makeEye('overlay',{name:'The viewer\'s buttons',html:['reset','zoom','ViewOptions','Level','gate','CopySpec','Maximize'].map(function(k){var r=CTL[k];return r?'<p><b>'+r.name+'</b> '+r.text+'</p>':''}).join('')});
    eo.className+=' ovl ovl-reset ovl-eyebtn';eo.innerHTML=eo.innerHTML.replace('width="14" height="14"','width="22" height="22"');xcv0.appendChild(eo);
    eyeRock($('xp-rock'));attachEyes(xv)})();
  setPreset('L');
}
