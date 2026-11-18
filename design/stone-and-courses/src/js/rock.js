/* ===== the rock in the wall (steps 6 and 8): each stone's cell is handed to the shared rock library (Stone, loaded from w3/*.js, one copy) and its mesh comes back ===== */
/* the wall's rock before a preset says otherwise: cuts, a little grain and wear (chips cut a roof over themselves and fail the gate, and chunks set pieces back under each other: both are for the presets that ask) */
var WALLROCK={cuts:0.6,'cuts.bite':0.6,'cuts.aim':0.2,grain:0.1,wear:0.15};
function rockParams(over){return Object.assign(Stone.controls.defaults(),WALLROCK,over||{})}
var ROCK_ON=function(p){var r=p.rock;return (!!r&&Stone.controls.STAGES.some(function(s){return r[s.id]>0}))||GR.areaOn(p)};
/* the cell of one stone, handed to the rock as an AREA AND A TOP (w3/cell.js): for each piece of the stone (a stone on tilted row planes is cut along them), the area is the stone's outline in plan, the Voronoi cell of its column (an irregular polygon;
   cut by the seams between its pieces), each edge a free face or a joint a neighbour holds; the bed is the plane under it (the seat when the stone is on the ground, held when it rests on a stone); the top is the plane where the stone stops
   (held when a stone stands on it); and what else the earlier steps cut it by (step 9's chamfers) are its planes. The rock reads the strata's lean off the bed. All in the stone's own frame: origin at the footprint centre and the lowest bed.
   A held bed and top stand OVL further out, so neighbours overlap and the union has nothing coincident (the sides already do, in the cell's own polygon). */
/* the joint edges of a polygon P0 (the stone's cell, grown at its joints): the ones that lie on an edge of the cell the wall tagged a neighbour (n) or a merge (m) */
function jointSegs(b,P0){var poly=b.polyT,m=poly.length,sg=polyArea(poly)>=0?1:-1,so=polyArea(P0)>=0?1:-1,out=[],i,j;
  for(i=0;i<P0.length;i++){var a=P0[i],c=P0[(i+1)%P0.length],ex=c[0]-a[0],ey=c[1]-a[1],l=Math.hypot(ex,ey)||1,n=[so*ey/l,-so*ex/l],d=n[0]*a[0]+n[1]*a[1];
    for(j=0;j<m;j++){var a2=poly[j],c2=poly[(j+1)%m],ex2=c2[0]-a2[0],ey2=c2[1]-a2[1],l2=Math.hypot(ex2,ey2)||1,n2=[sg*ey2/l2,-sg*ex2/l2];
      if(n2[0]*n[0]+n2[1]*n[1]>0.999&&Math.abs(n[0]*a2[0]+n[1]*a2[1]-d)<0.6){if(b.tags[j]==='n'||b.tags[j]==='m')out.push([a,c]);break}}}
  return out}
/* the joint's lower mouth along one free edge a to b (stone frame, counter-clockwise, so the outward normal is (dy, -dx)): the plane engine.js's finish cuts along a cell's free edges, with no cell below to measure (need 0): 48 degrees from vertical, MOUTH deep, through the edge at the bed */
function mouthPlane(ax,ay,bx,by,bed){var ex=bx-ax,ey=by-ay,l=Math.hypot(ex,ey),n=[ey/l,-ex/l],db=MOUTH*Math.tan(rad(48)),hb=MOUTH,Lh=Math.hypot(hb,db),nn=[n[0]*hb/Lh,n[1]*hb/Lh,-db/Lh],mx=(ax+bx)/2,my=(ay+by)/2,
  zb=(bed.d-bed.n[0]*mx-bed.n[1]*my)/bed.n[2],gr=[-bed.n[0]/bed.n[2],-bed.n[1]/bed.n[2]],n2=[nn[0]-nn[2]*gr[0],nn[1]-nn[2]*gr[1],nn[2]];
  return {n:n2,d:n2[0]*mx+n2[1]*my+n2[2]*(zb+hb)}}
function cellOfBlock(g,b,pieces,ctx){
  var cx=b.cx[0],cy=b.cx[1],oz=1e9,poly=b.polyT,m=poly.length,sg=polyArea(poly)>=0?1:-1,i,j;
  pieces.forEach(function(Q){Q.faces.forEach(function(F){F.pts.forEach(function(q){if(q[2]<oz)oz=q[2]})})});
  var o=[cx,cy,oz],hasAbove=!!b.poly&&(g.byCol&&g.byCol[cE(b)+1]||[]).some(function(q){return polyOverlap(b.poly,q.poly)>0.02});
  var P0=b.poly,m0=P0.length,sg0=polyArea(P0)>=0?1:-1;
  /* the role of each edge of the outline: a neighbour edge or a merged edge is a joint, the rest are free */
  function roleOf(n,d){for(j=0;j<m;j++){var a=poly[j],c=poly[(j+1)%m],ex=c[0]-a[0],ey=c[1]-a[1],l=Math.hypot(ex,ey)||1,nx=sg*ey/l,ny=-sg*ex/l;
      if(nx*n[0]+ny*n[1]>0.999&&Math.abs(n[0]*a[0]+n[1]*a[1]-d)<0.6)return (b.tags[j]==='n'||b.tags[j]==='m')?'joint':'surface'}return 'surface'}
  /* graded areas (steps 3a and 7a): when a stage that draws an outline is on and changes this stone, its area is that outline's convex parts, each cut by the piece's seams; otherwise the cell, as before */
  /* tops are free faces (cut like any other); a stone stands on what is left of the tops below it (ctx.seq: courses are built in order and each hands the next its cut tops), cut to that area, or not built when too little is left */
  var G0=ctx&&GR.areaOn(ctx.p)?GR.stoneArea(ctx,b,jointSegs):null,parts=[],tops=[];
  if(G0&&G0.dropped)return null;
  pieces.forEach(function(Q){
    var outline=P0.map(function(q){return [q[0],q[1]]}),bed=null,top=null,planes=[],bA=-1,tA=-1,seams=[];
    Q.faces.forEach(function(F){var k=F.kind,n=F.n;
      if(k==='seam'){var d=dot(n,F.pts[0]);seams.push({n:n,d:d});outline=clipPoly(outline,n,d)}});
    Q.faces.forEach(function(F){var k=F.kind,n=F.n,d=dot(n,F.pts[0])-dot(n,o),A=areaOf(F.pts);
      if(k==='seam'||k==='side'||k==='front')return;
      if(k==='bed'&&n[2]<-0.5&&A>bA){if(bed)planes.push({n:bed.n,d:bed.d});bA=A;bed={n:n,d:d};return}
      if(k==='top'&&n[2]>0.5&&A>tA){if(top)planes.push({n:top.n,d:top.d});tA=A;top={n:n,d:d};return}
      planes.push({n:n,d:d})});
    var bedO={n:0,v:bed&&{n:bed.n,d:bed.d,seat:b.c===0&&bed.n[2]<-0.999,over:OVL+(ctx&&ctx.seq&&b.c>0&&!ctx.exact?SUPTOL+0.5*(ctx.vox||0):0)}},topO=top&&{n:top.n,d:top.d+(hasAbove?OVL:0),held:false,over:OVL};if(top)tops.push({n:top.n,d:topO.d+dot(top.n,o)});   /* a top is a free face; where a stone stands on it, it still stands OVL further out, so the two overlap and the union has nothing coincident */   // a piece whose bed (or top) is wholly taken by a chamfer has none: the chamfer closes it
    if(G0){G0.parts.forEach(function(pt){var tg=pt.area.map(function(q,i){return {p:q,e:pt.edges[i]==='joint'?'n':pt.edges[i]==='seam'?'s':'g'}}),si;
        for(si=0;si<seams.length&&tg.length>2;si++)tg=clipTagged(tg,seams[si].n,seams[si].d,'s');
        if(tg.length<3)return;var ar=tg.map(function(v){return v.p}),ax=0,ii;for(ii=0;ii<ar.length;ii++){var u1=ar[ii],u2=ar[(ii+1)%ar.length];ax+=u1[0]*u2[1]-u2[0]*u1[1]}if(ax/2<0.05)return;
        /* a stone on a stone: the joint's lower mouth (step 9's rule, at the part's own free edges): a 48 degree facet along every free edge the wall did not already chamfer, so a bed never starts in front of the rounded top edge under it */
        var pl=planes,bedPoly=ar.map(function(q){return [q[0],q[1]]});if(b.c>0&&bedO.v){pl=planes.slice();tg.forEach(function(v,ii2){if(v.e!=='g')return;var a1=v.p,b1=tg[(ii2+1)%tg.length].p,ex=b1[0]-a1[0],ey=b1[1]-a1[1],l=Math.hypot(ex,ey);if(l<0.8)return;
            var old=(b.fe||[]).some(function(fe){return Math.abs(fe.n[0]*ey/l*-1+fe.n[1]*ex/l)<0.02&&Math.abs(fe.n[0]*((a1[0]+b1[0])/2)+fe.n[1]*((a1[1]+b1[1])/2)-(fe.n[0]*fe.a[0]+fe.n[1]*fe.a[1]))<0.05});if(old)return;
            var cut=clipPoly(bedPoly,[ey/l,-ex/l],(ey/l)*a1[0]+(-ex/l)*a1[1]-MOUTH*Math.tan(rad(48)));if(cut.length<3||Math.abs(polyArea(cut))<1.0)return;bedPoly=cut;   /* a cut at the mouth may never take the bed: the bed keeps 1 mm2 */
            pl.push(mouthPlane(a1[0]-cx,a1[1]-cy,b1[0]-cx,b1[1]-cy,bedO.v))})}
        parts.push({area:ar.map(function(q){return [q[0]-cx,q[1]-cy]}),edges:tg.map(function(v){return v.e==='n'?'joint':'surface'}),bed:bedO.v,top:topO,planes:pl,seamTag:'gseam'})});return}
    var area=outline.map(function(q){return [q[0]-cx,q[1]-cy]}),edges=[],ol=outline.length,so=polyArea(outline)>=0?1:-1;
    for(i=0;i<ol;i++){var a=outline[i],c=outline[(i+1)%ol],ex=c[0]-a[0],ey=c[1]-a[1],l=Math.hypot(ex,ey)||1,n=[so*ey/l,-so*ex/l];edges.push(roleOf(n,n[0]*a[0]+n[1]*a[1]))}
    parts.push({area:area,edges:edges,bed:bedO.v,top:topO,planes:planes})});
  /* a stone on a stone: its bed must lie within what it stands on, SHRINK-WRAPPED: each part's bed carries `bear`, the convex hull of the cut tops below it (from their built meshes) intersected with the part. Holes and gaps between the supports inside the hull are bridged. A hull is convex, so there is no largest-piece limit: the bear is one convex polygon. The cuts stage cuts the bed to it; the part is not shrunk. A stone with nothing under it, or with less than the minimum hull area, is not built (the guard); a part the hull does not reach is left out of its stone. */
  if(ctx&&ctx.seq&&b.c>0){var sc=supportCS(ctx,b),cellA=Math.abs(polyArea(b.polyT)),tot=0,kept=[];
    if(sc){var HC=null;if(!sc.empty){var hp=[];sc.toPolygons().forEach(function(r){r.forEach(function(q){hp.push([q[0],q[1]])})});var hh=hp.length>=3?hull2(hp):null;if(hh&&hh.length>=3)HC=GR.CSof([hh])}
      parts.forEach(function(pt){var ring=GR.ccw(pt.area.map(function(q){return [q[0]+cx,q[1]+cy]})),A=0,best=null,full=false;
        if(HC){var Pc=GR.CSof([ring]),I=Pc.intersect(HC),pls=I.toPolygons().filter(function(r){return GR.area2(r)>0}),bA=0;
          pls.forEach(function(r){var a=GR.area2(r);A+=a;if(a>bA){bA=a;best=GR.simplify(GR.ccw(r))}});full=A>=0.995*GR.area2(ring);
          if(ctx.bridgeLog&&A>=1.0){var S=Pc.intersect(sc),gap=I.subtract(S),ga=gap.area();if(ga>0.05){var r0=0,r1=12,it;for(it=0;it<10;it++){var rm=(r0+r1)/2;if(gap.offset(-rm,'Round',2,8).area()>1e-6)r0=rm;else r1=rm}ctx.bridgeLog.push({k:b.c+'_'+b.i,c:b.c,zb:b.zb,area:+ga.toFixed(2),span:+(2*r0).toFixed(2),rings:gap.toPolygons().map(function(q){return q.map(function(v){return [+v[0].toFixed(2),+v[1].toFixed(2)]})})})}S.delete();gap.delete()}
          Pc.delete();I.delete()}
        if(A<1.0||!best)return;tot+=A;
        if(!full)pt=Object.assign({},pt,{bed:pt.bed&&Object.assign({},pt.bed,{bear:best.map(function(q){return [q[0]-cx,q[1]-cy]})})});kept.push(pt)});
      /* a stone the envelope has cut down to a thin piece must stand on a support under what is left of it, not only on the hull over a gap: the part of its footprint where it is at least the minimum stone height (4 mm at the 35 mm square) high (top under the envelope less its bed) must be over the real tops, by 15% of itself (and 1.5 mm2), else it is not built, so no stone is left floating */
      if(sc&&!sc.empty&&b.env&&tot>=minSupport(ctx,cellA)){var runs=[],xa=null,xs0=b.x0,xs1=b.x1,xq;for(xq=xs0;xq<=xs1+1e-9;xq+=0.25){var okx=Math.min(b.zt,g.crest(xq))-b.zb>=MINSTONE*g.f;if(okx&&xa===null)xa=xq;if((!okx||xq+0.25>xs1+1e-9)&&xa!==null){runs.push([xa,okx?xq:xq-0.25]);xa=null}}
        var remain=0,remSup=0;if(runs.length){var RC=GR.CSof(runs.map(function(rr){return [[rr[0]-0.12,-100],[rr[1]+0.12,-100],[rr[1]+0.12,100],[rr[0]-0.12,100]]}));
          kept.forEach(function(pt){var rg=GR.ccw(pt.area.map(function(q){return [q[0]+cx,q[1]+cy]})),Pc=GR.CSof([rg]),I1=Pc.intersect(RC),I2=I1.intersect(sc);remain+=I1.area();remSup+=I2.area();Pc.delete();I1.delete();I2.delete()});RC.delete()}
        if(remSup<Math.max(1.5,0.15*remain)){ctx.rep.dropped=(ctx.rep.dropped||0)+1;ctx.rep.floating=(ctx.rep.floating||0)+1;return null}}
      if(tot<minSupport(ctx,cellA)){ctx.rep.dropped=(ctx.rep.dropped||0)+1;return null}
      parts=kept}}
  var cell=Stone.cell.area(parts,'s');
  return {spec:cell.spec,origin:o,tops:tops,parts:parts}}
function hullOfRings(rings,cx,cy){var pts=[];rings.forEach(function(R){R.forEach(function(q){pts.push([q[0]-cx,q[1]-cy])})});return pts.length>=3?hull2(pts):null}
/* THE SUPPORT of a stone: where the stones it rests on really are, at the height its bed stands: the upward faces of their built meshes within SUPTOL of their top planes, as one area (Manifold's CrossSection). A stone on the ground has none to look at. */
var SUPTOL=0.12;
function supportCS(ctx,b){if(!ctx.seq||b.c<1)return null;var sups=ctx.blocks.filter(function(o){return cE(o)===b.c-1&&polyOverlap(b.poly,o.poly)>0.02});if(!sups.length)return null;
  var acc=null;sups.forEach(function(o){var k=o.c+'_'+o.i,cs=ctx.supMemo[k];if(cs===undefined){var S0=ctx.built[k];cs=null;
      if(S0&&S0.tops){var m=S0.mesh,P=m.pos,ix=m.idx,tp=S0.tops,rings=[],t,v,pl;
        for(t=0;t<ix.length;t+=3){var V=[ix[t]*3,ix[t+1]*3,ix[t+2]*3],A=[P[V[0]],P[V[0]+1],P[V[0]+2]],B=[P[V[1]],P[V[1]+1],P[V[1]+2]],C=[P[V[2]],P[V[2]+1],P[V[2]+2]],n=cross(sub(B,A),sub(C,A)),l=Math.sqrt(dot(n,n));if(l<1e-12||n[2]/l<0.8)continue;
          var near=false,tol=ctx.exact?SUPTOL:SUPTOL+0.5*(ctx.vox||0);for(pl=0;pl<tp.length&&!near;pl++){near=[A,B,C].every(function(q){var d=dot(tp[pl].n,q)-tp[pl].d;return d>-tol&&d<0.06+(ctx.exact?0:0.25*(ctx.vox||0))})}
          if(near){var ring=[[A[0],A[1]],[B[0],B[1]],[C[0],C[1]]];rings.push(ring)}}
        if(rings.length)cs=MF.CrossSection.ofPolygons(rings)}
      ctx.supMemo[k]=cs}
    if(cs)acc=acc?acc.add(cs):cs});
  /* the support is smoothed: opened by 0.3 mm (thin slivers and jags go, and it only ever shrinks) and simplified, so the stone above gets a clean outline, not the sampled mesh's */
  if(!acc)return {empty:true};var sm=acc.offset(-0.3,'Round',2,8).offset(0.3,'Round',2,8).simplify(0.2);return sm.area()>1e-6?sm:{empty:true}}
/* the stones of a build, grouped by course (the course a stone's bed is on): a stone's pieces share a block. */
function rockPlan(b){var by={},order=[];b.polys.slice(0,b.n0).forEach(function(Q,i){if(!Q.b)return;var k=Q.b.c+'_'+Q.b.i;if(!by[k]){by[k]={k:k,b:Q.b,pieces:[],idx:[]};order.push(k)}by[k].pieces.push(Q);by[k].idx.push(i)});
  var cs={};order.forEach(function(k){var c=by[k].b.c;(cs[c]=cs[c]||[]).push(by[k])});
  return {groups:order.map(function(k){return by[k]}),courses:Object.keys(cs).map(Number).sort(function(x,y){return x-y}).map(function(c){return cs[c]})}}
/* the least a stone may stand on: 15% of its cell, and 4 mm2 for a cell that can spare it; a small cell (under 8 mm2) needs half of itself, so a stone small enough to be fully supported is not dropped for being small */
function minSupport(ctx,cellA){return Math.max(ctx.minShare*cellA,Math.min(ctx.minArea,0.5*cellA))}
function rockCtx(b,p,seq){var c={p:p,g:b.g,blocks:b.l2.blocks,memo:{},rep:b.rep,seq:!!seq,built:{},supMemo:{},minArea:4,minShare:0.15};c.exact=!(p.rock&&['chips','pockets','grain','wear'].some(function(k){return p.rock[k]>0}));c.supportCS=function(bb){return supportCS(c,bb)};return c}
/* the rock's lengths are reference mm at the 35 mm square, like the wall's dmm: under OpenLOCK (f = 0.7257) every stage's size shrinks with the stone, and a density per cm grows (the same number of chips along an edge). Hash change for the ol builds only; db has f = 1. */
function rockScaled(P,f){if(f===1)return P;var Q=Object.assign({},P);
  Stone.controls.CONTROLS.forEach(function(c){if(c.stage==='stone'&&c.word!=='patch')return;if(c.unit==='mm')Q[c.key]=P[c.key]*f;else if(c.unit==='per cm')Q[c.key]=P[c.key]/f});return Q}
function makeJob(b,p,grp,ctx){var c=cellOfBlock(b.g,grp.b,grp.pieces,ctx);if(!c)return null;
  return {k:grp.k,block:grp.b,idx:grp.idx,origin:c.origin,spec:c.spec,tops:c.tops,parts:c.parts,P:rockScaled(Object.assign({},p.rock,{seed:hash(p.seed,777,grp.b.c,grp.b.i)|0}),b.g.f)}}
/* a stone that is not built: its places are emptied (and its mirror's) */
function dropRock(b,grp){grp.idx.forEach(function(ix){b.polys[ix]={faces:[],b:grp.b,empty:true};if(b.ym!=null)b.polys[ix+b.n0]={faces:[],b:grp.b,empty:true,mirror:true}})}
/* the mesh of a job, in the wall's frame, as a solid the page draws and the union takes */
function rockSolid(job,res){var m=res.mesh,n=m.pos.length,pos=new Float32Array(n),o=job.origin,i;for(i=0;i<n;i+=3){pos[i]=m.pos[i]+o[0];pos[i+1]=m.pos[i+1]+o[1];pos[i+2]=m.pos[i+2]+o[2]}
  return {mesh:{pos:pos,idx:m.idxM||m.idx,tris:m.tris,path:m.path},faces:[],b:job.block,rock:true,report:res.report,tops:job.tops}}
/* the same stone on the back of a two-faced wall: mirrored in y about the wall's middle, the winding turned */
function mirrorSolid(S0,ym){var m=S0.mesh,pos=new Float32Array(m.pos),idx=new Uint32Array(m.idx),i,q;for(i=1;i<pos.length;i+=3)pos[i]=2*ym-pos[i];for(i=0;i<idx.length;i+=3){q=idx[i+1];idx[i+1]=idx[i+2];idx[i+2]=q}return {mesh:{pos:pos,idx:idx,tris:m.tris,path:m.path},faces:[],b:S0.b,rock:true,report:S0.report,mirror:true}}
/* put a stone's mesh in the build's list of solids: its first piece's place takes it (and its mirror), the other pieces' places are emptied */
function placeRock(b,job,res){var S0=rockSolid(job,res);job.idx.forEach(function(ix,q){b.polys[ix]=q?{faces:[],b:job.block,empty:true}:S0;if(b.ym!=null)b.polys[ix+b.n0]=q?{faces:[],b:job.block,empty:true,mirror:true}:mirrorSolid(S0,b.ym)});return S0}
/* the mesh path of triangles for the page's renderer */
function meshTris(S0,tag){var m=S0.mesh,p=m.pos,ix=m.idx,out=[],t,a,b2,c;
  for(t=0;t<ix.length;t+=3){a=ix[t]*3;b2=ix[t+1]*3;c=ix[t+2]*3;var A=[p[a],p[a+1],p[a+2]],B=[p[b2],p[b2+1],p[b2+2]],C=[p[c],p[c+1],p[c+2]],n=unit(cross(sub(B,A),sub(C,A)));
    out.push({p:[A,B,C],n:n,tag:tag||'stone',kind:'rock',id:(S0.b?S0.b.c*131+S0.b.i*7:0),ang:overAng(n)})}
  return out}
/* the rock's meshes by a stone's own key: a build that changed one stone keeps the rest. Keys hold everything a stone reads: its cell, its settings, its seed, the tier. */
var ROCKCACHE=new Map();
function rockKey(job,vox){job.ck=Stone.cell.make(job.spec).key;return JSON.stringify([job.ck,Stone.stone.stagesOf(job.P),job.P.seed,job.P.patch,vox])}

/* ===== the pool: stones are built in workers, up to hardwareConcurrency - 1, a stone always in the same worker (so its model is kept there); the page asks, and each mesh is placed as it lands ===== */
/* one stone from a job: the single function the workers, the page-thread fallback and the one-thread build all call. checks=true also runs the stone's invariants (the page shows them; the sweep does not need them). It depends on Stone alone, so a worker gets it as text. */
function makeStone(P,vox,cell,checks){var t=Date.now(),r=Stone.stone.mesh(P,vox,null,null,cell),mm=r.mesh,rp=r.report,res={mesh:{pos:mm.pos,idx:mm.idxM||mm.idx,tris:mm.tris,path:mm.path},report:{steps:rp.steps,islands:rp.islands.length,specks:rp.specks.length,sealed:rp.sealed,nPieces:rp.nPieces,refused:rp.refused,overhang:rp.overhang,tris:rp.tris,ms:0}};
  if(checks){var inv=Stone.checks.invariants(Stone.stone.build(P,null,cell));res.report.inv={ok:inv.ok,stray:inv.stray,outside:inv.outside,unreached:inv.unreached}}
  res.report.ms=Date.now()-t;return {res:res,raw:r.mesh}}
/* the jobs of one course, in order; a stone with too little to stand on is not built (its places are emptied) and counted */
function courseJobs(b,ctx,grps){var jobs=[],dropped=0;grps.forEach(function(grp){var j=makeJob(b,b.p,grp,ctx);if(!j){dropRock(b,grp);dropped++;return}jobs.push(j)});return {jobs:jobs,dropped:dropped}}
var RockRun=(function(){
  var workers=[],run=0,fallback=false,cur=null;
  var HANDLER=';var CELLS={},nc=0;'+makeStone.toString()+';onmessage=function(e){var d=e.data,c=CELLS[d.ck];if(!c){if(nc>400){CELLS={};nc=0}c=CELLS[d.ck]=Stone.cell.make(d.spec);nc++}var m,err=null;try{m=makeStone(d.P,d.vox,c,true)}catch(x){err=String(x&&x.stack||x)}if(err){postMessage({run:d.run,jid:d.jid,key:d.key,err:err});return}postMessage({run:d.run,jid:d.jid,key:d.key,res:m.res},[m.res.mesh.pos.buffer,m.res.mesh.idx.buffer])};';
  var ROCKCACHE_MAX=900;
  function ensure(){
    if(workers.length||fallback)return;
    try{var n=Math.max(1,Math.min(12,(navigator.hardwareConcurrency||4)-1)),url=URL.createObjectURL(new Blob([document.getElementById('stonesrc').textContent+HANDLER],{type:'text/javascript'})),i;
      for(i=0;i<n;i++){var w=new Worker(url);w.onmessage=onmsg;w.onerror=function(ev){console.error('[rock] worker failed, building on the page thread',ev&&ev.message);dropWorkers()};workers.push(w)}}
    catch(e){console.error('[rock] no workers, building on the page thread',e);dropWorkers()}}
  function dropWorkers(){workers.forEach(function(w){try{w.terminate()}catch(e){}});workers=[];if(!fallback){fallback=true;if(cur&&cur.done<cur.total)runMain(cur,cur.jobs.map(function(j,i){return i}).filter(function(i){return !cur.landed[i]}))}}   /* the stones still to land, in the page thread */
  function cacheSet(k,v){ROCKCACHE.delete(k);ROCKCACHE.set(k,v);if(ROCKCACHE.size>ROCKCACHE_MAX)ROCKCACHE.delete(ROCKCACHE.keys().next().value)}
  function land(R,i,r){if(R.landed[i])return;R.landed[i]=1;var S0=placeRock(R.b,R.jobs[i],r);R.ctx.built[R.jobs[i].k]=S0;R.reports.push(r.report);if(R.cc){var sp=r.report.steps||{};R.cc.cuts+=sp.cuts?sp.cuts.ms:0;R.cc.chunks+=sp.chunks?sp.chunks.ms:0;R.cc.sampled+=sp.sampled?sp.sampled.ms:0}R.done++;R.pendingC--;R.cb.onPlaced(R.done,R.total);if(R.pendingC<=0)setTimeout(function(){if(R===cur)nextCourse(R)},0)}
  function finish(R){var sum={stones:R.total,cached:R.cached,wallMs:now()-R.t0,cuts:0,chunks:0,sampled:0,tris:0,islands:0,specks:0,sealed:0,refused:0,bad:0,cpuMs:0,workers:fallback?0:workers.length,down:0,band:0,red:0,dropped:R.dropped,courses:R.plan.courses.length,areaMs:R.areaMs,courseLog:R.courseLog};
    R.reports.forEach(function(r){var s=r.steps||{};sum.cuts+=s.cuts?s.cuts.ms:0;sum.chunks+=s.chunks?s.chunks.ms:0;sum.sampled+=s.sampled?s.sampled.ms:0;sum.tris+=r.tris;sum.islands+=r.islands;sum.specks+=r.specks;sum.sealed+=r.sealed;sum.refused+=r.refused;sum.cpuMs+=r.ms;if(!r.inv.ok)sum.bad++;sum.down+=r.overhang.down;sum.band+=r.overhang.band;sum.red+=r.overhang.red});
    R.b.rep.dropped=R.dropped;R.b.bridges=R.ctx.bridgeLog;R.cb.onDone(sum)}
  function onmsg(e){var d=e.data;if(d.err){console.error('[rock] a stone failed: '+d.err);if(cur&&d.run===cur.run)cur.cb.onError(d.err);return}
    cacheSet(d.key,d.res);if(!cur||d.run!==cur.run)return;land(cur,d.jid,d.res)}
  function runMain(R,list){var i=0;(function step(){if(R!==cur)return;while(i<list.length&&R.landed[list[i]])i++;if(i>=list.length)return;var k=list[i],j=R.jobs[k];
      setTimeout(function(){if(R!==cur)return;var res=makeStone(j.P,R.vox,Stone.cell.make(j.spec),true).res;
        cacheSet(j.key,res);land(R,k,res);i++;step()},0)})()}
  /* one course at a time (a course stands on the cut tops of the one below), the stones of a course in parallel: the cells of the next course are made from the meshes just landed; a stone with too little to stand on is not built */
  function nextCourse(R){if(R.cc){R.courseLog.push({course:R.cc.c,stones:R.cc.n,ms:now()-R.cc.t0,cuts:R.cc.cuts,chunks:R.cc.chunks,sampled:R.cc.sampled});R.cc=null}
    if(R.ci>=R.plan.courses.length){finish(R);return}
    var grps=R.plan.courses[R.ci++],todo=[],hits=[],t0=now();R.cc={c:grps[0].b.c,n:grps.length,t0:t0,cuts:0,chunks:0,sampled:0};
    var cj=courseJobs(R.b,R.ctx,grps);if(cj.dropped){R.dropped+=cj.dropped;R.done+=cj.dropped;R.cb.onPlaced(R.done,R.total)}
    cj.jobs.forEach(function(j){j.key=rockKey(j,R.vox);R.jobs.push(j);var idx=R.jobs.length-1,hit=ROCKCACHE.get(j.key);if(hit){R.cached++;hits.push([idx,hit])}else todo.push(idx)});
    R.areaMs+=now()-t0;R.pendingC=hits.length+todo.length;
    if(!R.pendingC){nextCourse(R);return}
    hits.forEach(function(h){land(R,h[0],h[1])});
    if(!todo.length)return;
    if(fallback||!workers.length){runMain(R,todo);return}
    todo.forEach(function(i){var j=R.jobs[i],w=workers[(Stone.rand.fnv(j.ck)>>>0)%workers.length];w.postMessage({run:R.run,jid:i,key:j.key,ck:j.ck,spec:j.spec,P:j.P,vox:R.vox})})}
  /* cb: { onPlaced(done,total), onDone(summary), onError(text) } */
  function start(b,vox,cb){
    run++;var plan=rockPlan(b),R={run:run,plan:plan,ctx:Object.assign(rockCtx(b,b.p,true),{vox:vox,bridgeLog:[]}),jobs:[],total:plan.groups.length,done:0,cached:0,dropped:0,areaMs:0,ci:0,pendingC:0,t0:now(),reports:[],landed:[],courseLog:[],cc:null,cb:cb,b:b,vox:vox};cur=R;ensure();
    if(!R.total){finish(R);return R}
    nextCourse(R);return R}
  function cancel(){run++;cur=null}
  return {start:start,cancel:cancel,get current(){return cur},get mode(){return fallback?'page thread':workers.length+' workers'}}})();

/* the same build on one thread, course by course (the harness and anything that wants the meshes without workers) */
function rockBuildSync(b,vox){var plan=rockPlan(b),ctx=rockCtx(b,b.p,true),out={jobs:[],tris:0,dropped:0,hashes:[],bridges:[]};ctx.vox=vox;ctx.bridgeLog=out.bridges;
  plan.courses.forEach(function(grps){var cj=courseJobs(b,ctx,grps);out.dropped+=cj.dropped;cj.jobs.forEach(function(j){var m=makeStone(j.P,vox,Stone.cell.make(j.spec),false),S0=placeRock(b,j,m.res);ctx.built[j.k]=S0;out.tris+=S0.mesh.tris;out.jobs.push(j);out.hashes.push(Stone.checks.hashMesh(m.raw))})});
  out.total=plan.groups.length;return out}
