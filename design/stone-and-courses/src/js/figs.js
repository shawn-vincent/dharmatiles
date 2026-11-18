/* The figures of the algorithm part. Each is drawn at load by this page's own engine, so a figure shows what the engine does, not a picture of it. A placeholder is <div class="dg" data-dg="name" data-a='{...}'> (an SVG diagram) or <canvas class="ex" data-a='{...}'> (a small render). */
(function(){
var COL=['#3b82c4','#d9a400','#c9348c','#3aa57a','#8e6fd6','#2aa3ae'];
function mk(p,ov){return Object.assign(JSON.parse(JSON.stringify(DEFAULTS)),JSON.parse(JSON.stringify(PRESETS[p].p)),ov||{},{seed:(ov&&ov.seed)||3,system:'db'})}
function pts(P,T){return P.map(function(q){return T(q[0],q[1]).join(',')}).join(' ')}
function svg(w,h,body,cls){return '<svg class="'+(cls||'dgsvg')+'" viewBox="0 0 '+w+' '+h+'" role="img">'+body+'</svg>'}
function txt(x,y,s,o){o=o||{};return '<text x="'+x+'" y="'+y+'" text-anchor="'+(o.a||'start')+'" class="dt'+(o.c?' '+o.c:'')+'">'+s+'</text>'}
var dg={};
dg.footprint=function(el,a){var b=build(mk(a.p||'J'),2),g=b.g,W=g.W,T=g.T,pad=6,k=420/(T+2*pad),w=420,h=(W+2*pad)*k;
  function X(x,y){return [(x+T/2+pad)*k,(W/2-y+pad)*k]}
  var s='<rect x="'+X(-T/2,W/2)[0]+'" y="'+X(-T/2,W/2)[1]+'" width="'+T*k+'" height="'+W*k+'" class="dtile"/>';
  s+='<polygon points="'+pts(g.fp,X)+'" class="dfp"/>';
  var m=g.m;s+='<rect x="'+X(-T/2+m,W/2-m)[0]+'" y="'+X(-T/2+m,W/2-m)[1]+'" width="'+(T-2*m)*k+'" height="'+(W-2*m)*k+'" class="dmargin"/>';
  s+=txt(X(0,W/2)[0],X(0,W/2)[1]-4,T.toFixed(0)+' mm tile edge',{a:'middle'});
  s+=txt(X(g.fp[0][0],g.fp[0][1])[0]+4,X(0,g.y0)[1]-5,'footprint (grey)');
  s+=txt(X(-T/2+m,W/2-m)[0]+3,X(0,W/2-m)[1]+13,'margin '+m.toFixed(1)+' mm (the fit beside the next tile)');
  var cw=360,ch=200,cx0=10,cz=function(x,z){return [cx0+(x+T/2)/T*(cw-20),ch-14-z*3]},e='<polyline class="dcrest" points="';
  for(var x=g.x0;x<=g.x1+1e-6;x+=1.2)e+=cz(x-g.x0-T/2,g.crest(x)).join(',')+' ';e+='"/>';
  var lo=cz(-T/2,DIRT+g.topLo)[1],hi=cz(-T/2,DIRT+g.topHi)[1],gr=cz(-T/2,DIRT)[1];
  e+='<line x1="10" x2="'+(cw-10)+'" y1="'+lo+'" y2="'+lo+'" class="dband"/><line x1="10" x2="'+(cw-10)+'" y1="'+hi+'" y2="'+hi+'" class="dband"/><rect x="10" y="'+gr+'" width="'+(cw-20)+'" height="10" class="ddirt"/>';
  e+=txt(14,lo+12,'top range '+g.topLo.toFixed(0)+' to '+g.topHi.toFixed(0)+' mm above the dirt')+txt(14,gr+22,'dirt, '+DIRT+' mm');
  el.innerHTML='<div class="dgrow">'+svg(w,h,s)+svg(cw,ch+14,e)+'</div>'};
dg.planes=function(el,a){function one(ov,title){var b=build(mk('L',Object.assign({wander:0},ov)),2),g=b.g,L=b.l1,w=420,h=250,k=(w-20)/g.T,y=g.y0+1;
    function XZ(x,z){return [10+(x-g.x0)*k,h-12-z*3.2]}var s='';
    L.planes.forEach(function(pl,i){var d='';for(var x=g.x0;x<=g.x1+1e-6;x+=1.5){var p=XZ(x,pl.z+planeOff(pl,x,y));d+=(d?' ':'')+p.join(',')}s+='<polyline points="'+d+'" class="dplane'+(i===0?' dground':'')+'"/>'});
    if(L.planes.length>2){var p0=XZ(g.x0,L.planes[2].z+planeOff(L.planes[2],g.x0,y));s+=txt(p0[0]+3,p0[1]-3,'row 2 plane')}
    s+=txt(12,18,title,{c:'dtb'});return svg(w,h,s)}
  el.innerHTML='<div class="dgrow">'+one({},'flat rows')+one({dip:4,wave:2},'dip 4, wave 2')+'</div>'};
function planPoly(b,cs,o){var g=b.g,T=g.T,W=g.W,pad=5,k=400/(T+2*pad),h=Math.round((g.yb-g.y0+2*pad+3)*k);function X(x,y){return [(x+T/2+pad)*k,(g.yb+pad-y)*k]}return {X:X,k:k,h:h,w:400}}
dg.subdivide=function(el,a){var b=build(mk(a.p||'L',Object.assign({wander:0},a.ov||{})),2),g=b.g,P=planPoly(b),X=P.X,s='';
  [0,1].forEach(function(c){b.l2.blocks.filter(function(x){return x.c===c}).forEach(function(o){s+='<polygon points="'+pts(o.polyT,X)+'" class="'+(c===0?'dcell':'dcell1')+'"/>'})});
  s+='<polygon points="'+pts(g.fp,X)+'" class="dfp0"/>';
  var sites=b.l1.pts[0];sites.forEach(function(q){var p=X(q[0],q[1]);s+='<circle cx="'+p[0]+'" cy="'+p[1]+'" r="3" class="dsite"/>'});
  s+=txt(8,14,'course 0: solid; course 1: dashed, set back')+txt(8,P.h-6,'dots: the sites; the outline is the footprint');
  el.innerHTML=svg(P.w,P.h,s)};
dg.faces=function(el,a){function one(free,label){var b=build(mk('C',{wander:0,recess:[0,4],free:free}),2),g=b.g,P=planPoly(b),X=P.X,s='';
    b.l2.blocks.filter(function(x){return x.c===0}).forEach(function(o){s+='<polygon points="'+pts(o.polyT,X)+'" class="dcell"/>'});
    s+='<polygon points="'+pts(g.fp,X)+'" class="dfp0"/>';s+=txt(8,14,label);return svg(P.w,P.h,s)}
  el.innerHTML='<div class="dgrow">'+one({front:1,back:0,sides:0},'front only')+one({front:1,back:1,sides:1},'front, back and sides')+'</div>'};
dg.recess=function(el,a){var b=build(mk(a.p||'L',Object.assign({wander:0,recess:[0,4]},a.ov||{})),2),g=b.g,P=planPoly(b),X=P.X,s='';
  b.l2.blocks.filter(function(x){return x.c===0}).forEach(function(o){s+='<polygon points="'+pts(o.polyT,X)+'" class="dcell"/>'});
  s+='<polygon points="'+pts(g.fp,X)+'" class="dfp0"/>';
  s+=txt(8,14,'course 0: each stone recessed')+txt(8,P.h-6,'outline: the footprint; a joined stone moves as one');
  el.innerHTML=svg(P.w,P.h,s)};
dg.area=function(el,a){var b=build(mk('T',{wander:0,seed:a.seed||4}),2),g=b.g,P=planPoly(b),X=P.X,s='',c=a.course||1;
  var tall=b.l2.blocks.filter(function(x){return x.c<c&&(x.c2||x.c)>=c});
  b.l2.blocks.filter(function(x){return x.c===c}).forEach(function(o){s+='<polygon points="'+pts(o.polyT,X)+'" class="dcell"/>'});
  tall.forEach(function(o){s+='<polygon points="'+pts(o.polyT,X)+'" class="dres"/>'});
  s+='<polygon points="'+pts(g.fp,X)+'" class="dfp0"/>'+txt(8,14,'row '+c+': hatched = footprints of stones that rise through it')+txt(8,P.h-6,'the other cells are cut round them (convex parts)');
  el.innerHTML=svg(P.w,P.h,s)};
dg.rise=function(el,a){var b=build(mk(a.p||'T',Object.assign({wander:0,seed:a.seed||3},a.ov||{})),2),g=b.g,w=440,h=300,k=(w-20)/g.T;function X(x){return 10+(x-g.x0)*k}function Z(z){return h-14-z*3.4}var s='<rect x="10" y="'+Z(DIRT)+'" width="'+(w-20)+'" height="'+(DIRT*3.4)+'" class="ddirt"/>';
  b.l2.blocks.forEach(function(o){var ext=(o.c2||o.c)-o.c,x0=Math.min.apply(null,o.polyT.map(function(q){return q[0]})),x1=Math.max.apply(null,o.polyT.map(function(q){return q[0]}));
    s+='<rect x="'+X(x0)+'" y="'+Z(o.zt)+'" width="'+(X(x1)-X(x0))+'" height="'+((o.zt-o.zb)*3.4)+'" class="'+(ext>0?'dtall':'dshort')+'" opacity="'+(ext>0?0.55:0.35)+'"/>'});
  s+=txt(12,16,'front view: solid = rose through 2 or more rows')+txt(12,h-2,'each rectangle is one stone from its bed to its top');el.innerHTML=svg(w,h,s)};
dg.span=function(el,a){var b=build(mk(a.p||'S',Object.assign({wander:0,seed:a.seed||3,rise:0,span:a.span!=null?a.span:0.7},a.ov||{})),2),g=b.g,P=planPoly(b),X=P.X,s='',c=0,grp=[],bl=b.l2.blocks.filter(function(x){return x.c===c});
  var par=bl.map(function(_,i){return i});function f(i){return par[i]===i?i:(par[i]=f(par[i]))}
  bl.forEach(function(A,i){A.polyT.forEach(function(q,ea){if(A.tags[ea]!=='m')return;var a1=q,a2=A.polyT[(ea+1)%A.polyT.length];bl.forEach(function(B,j){if(j===i)return;B.polyT.forEach(function(r,eb){var b2=B.polyT[(eb+1)%B.polyT.length];if(B.tags[eb]==='m'&&Math.hypot(a1[0]-b2[0],a1[1]-b2[1])<1e-3&&Math.hypot(a2[0]-r[0],a2[1]-r[1])<1e-3)par[f(i)]=f(j)})})})});
  bl.forEach(function(o,i){s+='<polygon points="'+pts(o.polyT,X)+'" fill="'+COL[f(i)%COL.length]+'" fill-opacity=".35" class="dcell2"/>';o.polyT.forEach(function(q,e){var p=X(q[0],q[1]),r=X(o.polyT[(e+1)%o.polyT.length][0],o.polyT[(e+1)%o.polyT.length][1]);s+='<line x1="'+p[0]+'" y1="'+p[1]+'" x2="'+r[0]+'" y2="'+r[1]+'" class="'+(o.tags[e]==='m'?'dmerged':o.tags[e]==='n'?'djoint':'dfree')+'"/>'})});
  s+=txt(8,14,'row 0, span '+(a.span!=null?a.span:0.7)+': one colour = one stone')+txt(8,P.h-6,'thick = joint; grey = free face');el.innerHTML=svg(P.w,P.h,s)};
function exTris(b,a){var out=[];b.polys.forEach(function(P){var hl=false,t=trisOf([P],'stone');if(a.hl&&!hl)t.forEach(function(q){if((a.hl==='groove'&&q.kind==='jbevel')||(a.hl==='joint'&&q.kind==='joint')||(a.hl==='facet'&&(q.kind==='facet'||q.kind==='chip')))q.tag='hl'});out=out.concat(t)});
  return out}
function exDraw(cv,a){var p=mk(a.p||'L',a.ov||{}),b=build(p,Math.max(2,a.st||4)),tris=(a.target||a.rows)?figureTris(b.g.T/2+16*b.g.f,-b.g.W/2+14*b.g.f,0):exTris(b,a),f=b.g.f,cam=new Cam(a.az==null?30:a.az,a.el==null?20:a.el),W=cv.width,H=cv.height,span=Math.max(82*f,(b.g.T+34*f)*0.9)*(a.span||1),sc=(a.zoom||1)*(H/span)/W,ctr=a.center||[1.5*f,0,25*f];
  if(!a.target&&!a.rows)tris=tris.concat(plateTris(b.g.W,b.g.T,DIRT));render(cv,tris,cam,sc,ctr,pack(238,240,244),a.mode||'shaded',{dark:isDarkTheme(),grid:true,z:DIRT-0.02,S:SQUARE*f,ox:-b.g.T/2,oy:-b.g.W/2,fx:0,fy:0,size:SQUARE*f,px:1});if(a.target)drawTargetCore(cv,cam,b,sc,ctr);if(a.rows)drawRowsCore(cv,cam,b,sc,ctr)}
window.drawFigures=function(){[].forEach.call(document.querySelectorAll('.dg'),function(el){var a={};try{a=JSON.parse(el.getAttribute('data-a')||'{}')}catch(e){console.error('[figs] bad data-a',el.getAttribute('data-dg'))}var f=dg[el.getAttribute('data-dg')];if(!f){console.error('[figs] no figure '+el.getAttribute('data-dg'));return}try{f(el,a)}catch(e){console.error('[figs] '+el.getAttribute('data-dg')+': '+e.message)}});
  [].forEach.call(document.querySelectorAll('canvas.ex'),function(cv){var a={};try{a=JSON.parse(cv.getAttribute('data-a')||'{}')}catch(e){}try{exDraw(cv,a)}catch(e){console.error('[figs] ex: '+e.message)}})};
window.addEventListener('load',function(){setTimeout(window.drawFigures,600)});
window.addEventListener('data-theme-changed',window.drawFigures);
})();
