/* ===== the stone engine of this page: L0 to L4, convex polytopes written straight, one union at the end ===== */
var OVL=0.15;
/* the tile's edge constants: the wall stands this far inside the tile edge (the fit of a tile beside its neighbour), and the joint's lower mouth is chamfered this deep so a stone over a stone prints (step 7) */
var EDGE_MARGIN=0.6,MOUTH=0.72;
/* the tile's square, and the least a stone may be tall, both in mm at the 35 mm square (scaled with the system by `f`) */
var SQUARE=35,MINSTONE=4,MINCELLW=2;   /* the least a cell may be across (mm at the 35 mm square): a needle of a cell cannot hold the rock's wear and grain, which break its tip off as a loose speck */
var LINE=0.45,LAYER=0.2,GAPMIN=0.2,OVER=55,STEEP=65,OLF=0.7257142857142856,DIRT=3.4,PI=Math.PI;
function mulberry(a){return function(){a|=0;a=a+0x6D2B79F5|0;var t=Math.imul(a^a>>>15,1|a);t=t+Math.imul(t^t>>>7,61|t)^t;return((t^t>>>14)>>>0)/4294967296}}
/* THE FREE FACES. A footprint edge faces front, back, left or right (the dominant axis of its normal); it is a seam when the wall goes on into the neighbouring tile or the hill is behind it (L0 tags it; the cell edge keeps the tag 'x'). Recess, the face band and the lip act on a free face (not a seam) whose switch p.free is on: front, back, or sides (left and right are two draws of the one switch). */
var FACES=['front','back','left','right'];
function faceOf(n){return Math.abs(n[0])>Math.abs(n[1])?(n[0]<0?'left':'right'):(n[1]<0?'front':'back')}
var FN=[[0,-1],[0,1],[-1,0],[1,0]];
/* an edge (sloped ones too, as support outlines are) faces a switched-on face when its normal points that way: a lip stands out over it */
function toward(p,n){return FACES.some(function(fc,d){return faceOn(p,fc)&&n[0]*FN[d][0]+n[1]*FN[d][1]>0.3})}
function faceD(face){return FACES.indexOf(face)}
function faceOn(p,face){var F=p.free||{front:1,back:0,sides:0};return face==='front'?!!F.front:face==='back'?!!F.back&&!p.wall:!!F.sides}
/* the one key a face's line is drawn from: the front's is the legacy key, every other face adds its direction */
function faceKey(seed,tag,c,face){var d=faceD(face);return d>0?hash(seed,tag,c,d):hash(seed,tag,c)}
function hash(){var h=2166136261,i;for(i=0;i<arguments.length;i++){h^=(arguments[i]|0)+0x9e3779b9;h=Math.imul(h,16777619);h^=h>>>13;}return h>>>0}
function R(){var r=mulberry(hash.apply(null,arguments));r();r();return r}
function dot(a,b){return a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
function sub(a,b){return [a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
function add(a,b){return [a[0]+b[0],a[1]+b[1],a[2]+b[2]]}
function mul(a,s){return [a[0]*s,a[1]*s,a[2]*s]}
function cross(a,b){return [a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
function unit(a){var l=Math.sqrt(dot(a,a))||1;return [a[0]/l,a[1]/l,a[2]/l]}
function rad(d){return d*PI/180}
function areaVec(p){var s=[0,0,0],i;for(i=1;i<p.length-1;i++)s=add(s,cross(sub(p[i],p[0]),sub(p[i+1],p[0])));return s}
function areaOf(p){var v=areaVec(p);return Math.sqrt(dot(v,v))/2}
/* a weighted range: low(lo..hi) leans to the low end by `w` (0 flat); drawn from the caller's stream */
function draw(r,rg){if(typeof rg==='number')return rg;var lo=rg[0],hi=rg[1],w=rg[2]||0,u=r();return lo+(hi-lo)*Math.pow(u,1+w)}
/* ---- polytope: faces with outward normals; clip keeps n.p <= d ---- */
function mkFace(n,pts,kind){if(dot(areaVec(pts),n)<0)pts=pts.slice().reverse();return {n:n,pts:pts,kind:kind}}
function box(x0,x1,y0,y1,z0,z1){
  var F=[];function q(n,a,b,c,d){F.push(mkFace(n,[a,b,c,d],'side'))}
  q([-1,0,0],[x0,y0,z0],[x0,y1,z0],[x0,y1,z1],[x0,y0,z1]);q([1,0,0],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1],[x1,y0,z1]);
  q([0,-1,0],[x0,y0,z0],[x1,y0,z0],[x1,y0,z1],[x0,y0,z1]);q([0,1,0],[x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]);
  q([0,0,-1],[x0,y0,z0],[x1,y0,z0],[x1,y1,z0],[x0,y1,z0]);q([0,0,1],[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]);
  F[5].kind='top';F[4].kind='bed';
  return {faces:F};
}
function clip(P,n,d,kind){
  var E=1e-7,out=[],cut=[],any=false,f,i,m,s,a,b,sa,sb,np,allin,allout,t,q;
  for(f=0;f<P.faces.length;f++){
    var F=P.faces[f],pts=F.pts;m=pts.length;s=[];allin=true;allout=true;
    for(i=0;i<m;i++){s[i]=dot(n,pts[i])-d;if(s[i]>E)allin=false;if(s[i]<-E)allout=false}
    if(allin){out.push(F);for(i=0;i<m;i++)if(Math.abs(s[i])<=E)cut.push(pts[i]);continue}
    any=true;
    if(allout){for(i=0;i<m;i++)if(Math.abs(s[i])<=E)cut.push(pts[i]);continue}
    np=[];
    for(i=0;i<m;i++){a=pts[i];b=pts[(i+1)%m];sa=s[i];sb=s[(i+1)%m];
      if(sa<=E){np.push(a);if(sa>=-E)cut.push(a)}
      if((sa<-E&&sb>E)||(sa>E&&sb<-E)){t=sa/(sa-sb);q=[a[0]+(b[0]-a[0])*t,a[1]+(b[1]-a[1])*t,a[2]+(b[2]-a[2])*t];np.push(q);cut.push(q)}}
    if(np.length>=3)out.push({n:F.n,pts:np,kind:F.kind});
  }
  if(!any)return P;if(!out.length)return null;
  var u=[],j,dup,c=[0,0,0];
  for(i=0;i<cut.length;i++){dup=false;for(j=0;j<u.length;j++)if(Math.abs(u[j][0]-cut[i][0])+Math.abs(u[j][1]-cut[i][1])+Math.abs(u[j][2]-cut[i][2])<1e-5){dup=true;break}if(!dup)u.push(cut[i])}
  if(u.length<3)return P;
  for(i=0;i<u.length;i++)c=add(c,u[i]);c=mul(c,1/u.length);
  var ax=Math.abs(n[0])<0.8?[1,0,0]:[0,1,0],e1=unit(cross(n,ax)),e2=cross(n,e1);
  u.sort(function(p,q2){var dp=sub(p,c),dq=sub(q2,c);return Math.atan2(dot(dp,e2),dot(dp,e1))-Math.atan2(dot(dq,e2),dot(dq,e1))});
  out.push({n:n,pts:u,kind:kind||'cut'});
  return {faces:out};
}
function triCount(P){if(P.mesh)return P.mesh.tris;var s=0,i;for(i=0;i<P.faces.length;i++)s+=P.faces[i].pts.length-2;return s}
function verts(P){var seen={},o=[],i,j,p,k;for(i=0;i<P.faces.length;i++)for(j=0;j<P.faces[i].pts.length;j++){p=P.faces[i].pts[j];k=Math.round(p[0]*1e4)+','+Math.round(p[1]*1e4)+','+Math.round(p[2]*1e4);if(!seen[k]){seen[k]=1;o.push(p)}}return o}
/* the width of a convex polygon in plan: the least, over its edges, of the farthest vertex from that edge */
function minWidth2(pts){var m=1e9,i,j,a,b,ex,ey,l,mx,d;for(i=0;i<pts.length;i++){a=pts[i];b=pts[(i+1)%pts.length];ex=b[0]-a[0];ey=b[1]-a[1];l=Math.hypot(ex,ey);if(l<1e-9)continue;mx=0;for(j=0;j<pts.length;j++){d=Math.abs(ex*(pts[j][1]-a[1])-ey*(pts[j][0]-a[0]))/l;if(d>mx)mx=d}if(mx<m)m=mx}return m}
function minWidth(pts){var m=1e9,i,j,a,b,e,l,mx,d,c;
  for(i=0;i<pts.length;i++){a=pts[i];b=pts[(i+1)%pts.length];e=sub(b,a);l=Math.sqrt(dot(e,e));if(l<1e-9)continue;mx=0;
    for(j=0;j<pts.length;j++){c=cross(e,sub(pts[j],a));d=Math.sqrt(dot(c,c))/l;if(d>mx)mx=d}if(mx<m)m=mx}
  return m}
/* ---- a smooth 1D value noise for the crest ---- */
function noise1(seed,x,wl){var u=x/wl,i=Math.floor(u),f=u-i,a=R(seed,i)(),b=R(seed,i+1)(),s=f*f*(3-2*f);return a+(b-a)*s}

/* ===== the parameters: lengths in dmm (scale with the system), angles in degrees ===== */
var DEFAULTS={
  system:'db',tiles:1,seed:3,wall:0,free:{front:1,back:0,sides:0},
  depth:22,                       /* region depth behind the face, dmm */
  top:[44,58],                    /* crest band above the dirt, dmm (L0) */
  width:[6,14,1.2],height:[10,16,1.2],bond:[0.05,0.6],shift:[0.25,0.5],recess:[0,1.2],wander:0.15,
  rise:0,span:0,dip:0,wave:0,
  regularity:0.3,
  /* graded areas (steps 3a and 7a): both off at 0, so the wall is exactly what it was */
  sbw:{w:'flat',bumps:2.5},       /* the recess's weighting along a course (step 5a), like the crest's */
  crest:{w:'flat',bumps:2.5},   /* the crest's own wobble: bumps a square and the weighting of its band (step 1) */
  band:{on:1,depth:0,w:'bell',a:0.12,c:0.5,bumps:4,stay:1},
  outl:{on:1,s:0,w:'bell',a:0.12,c:0.5,bumps:4,core:0.35}
};
/* THE ENVELOPE: the cliff is the footprint extruded up to the crest, and a stone never rises above it. A stone's top is its row's top plane (or, for a stone that rises, the top plane of the last row it reaches) intersected with the envelope: the crest is handed to the stone as planes it must lie under, chords of the crest across its footprint (about every 2 mm), lowered together by however far the chords stand above the crest anywhere on it. The stone is built inside that top; nothing is cut off it afterwards. */
function envOf(g,b){var x0=b.x0,x1=b.x1,K=Math.max(3,Math.ceil((x1-x0)/(2*g.f))),k;
  function planesFor(pts){var pl=[],ex=0,j,k2;for(k2=0;k2<pts.length-1;k2++){var a=pts[k2],c=pts[k2+1],s2=(c[1]-a[1])/Math.max(1e-6,c[0]-a[0]);pl.push({s:s2,c:a[1]-s2*a[0]})}
    for(k2=0;k2<=40;k2++){var x=x0+(x1-x0)*k2/40,t=1e9;for(j=0;j<pl.length;j++){var v=pl[j].s*x+pl[j].c;if(v<t)t=v}var e=t-g.crest(x);if(e>ex)ex=e}
    pl.forEach(function(q){q.c-=ex});return pl}
  function deficit(pl){var sum=0,k2,j;for(k2=0;k2<=40;k2++){var x=x0+(x1-x0)*k2/40,t=1e9;for(j=0;j<pl.length;j++){var v=pl[j].s*x+pl[j].c;if(v<t)t=v}sum+=Math.min(b.zt,g.crest(x))-Math.min(b.zt,t)}return sum/41}
  var many=[],two=[[x0,g.crest(x0)],[x1,g.crest(x1)]];for(k=0;k<=K;k++){var x=x0+(x1-x0)*k/K;many.push([x,g.crest(x)])}
  /* chords across the footprint follow a hump closely, but in a steep valley their extensions cut below the crest and can leave nothing of the stone: the one chord between the ends (lowered to the crest) and a flat plane at the lowest crest are the fallbacks; of the three, the one that takes least off the stone (its top, below the row's top, against the crest) is used */
  var mnc=1e9,kk;for(kk=0;kk<=40;kk++){var cv=g.crest(x0+(x1-x0)*kk/40);if(cv<mnc)mnc=cv}
  var A=planesFor(many),B=planesFor(two),C=[{s:0,c:mnc}],pl=A,dA=deficit(A),dB=deficit(B),dC=deficit(C);if(dB<dA){pl=B;dA=dB}if(dC<dA)pl=C;
  return pl.map(function(q){var L=Math.sqrt(1+q.s*q.s);return {n:[-q.s/L,0,1/L],d:q.c/L}})}
/* the height a stone is built to: its top (the row's top under the envelope) less its bed, averaged over its footprint's length; under the cliff's minimum stone height (4 mm at the 35 mm square: the least a row's height may be) it is not built */
function envHeight(g,b){var x0=b.x0,x1=b.x1,k,i,sum=0,N=12,T0=1e9,env=null;
  /* the top under the same crest planes the stone is cut to (crestCut), not under the crest itself: where the planes lie below the crest a stone can be built to a sliver, so the guard must read what will really stand. Computed fresh: the stone's top is not final here. */
  for(k=0;k<=8;k++){var v=g.crest(x0+(x1-x0)*k/8);if(v<T0)T0=v}
  if(b.zt>T0-1e-6)env=envOf(g,b);
  for(k=0;k<N;k++){var x=x0+(x1-x0)*(k+.5)/N,top=Math.min(b.zt,g.crest(x));if(env)for(i=0;i<env.length;i++)top=Math.min(top,(env[i].d-env[i].n[0]*x)/env[i].n[2]);sum+=Math.max(0,top-b.zb)}return sum/N}
function sc(p){return p.system==='ol'?OLF:1}
/* ===== L0 footprint and crest ===== */
function L0(p){
  var f=sc(p),W0=SQUARE*f,W=W0*(p.rows||1),T=W0*p.tiles,fr=(p.frontOffset==null?9:p.frontOffset),y0=-W/2+fr*f,m=EDGE_MARGIN,D=Math.min(p.depth*f,W/2-m-y0),topLo=p.top[0]*f,topHi=p.top[1]*f;
  /* THE CREST is graded, the one rule of steps 3a and 7a along the wall: a smooth wobble along x (bumps a square), ranked, each rank (i + 0.5) / N through the weighting's quantile to t in 0..1, the crest DIRT + low + (high - low) x t. The band is used whole, ends included, whatever the seed; the wobble only decides where the highs and lows fall. */
  var CR=p.crest||{},cb=CR.bumps||2.5,cw=GR.wOf({w:CR.w||'flat',a:CR.a,c:CR.c}),cN=Math.max(24,Math.ceil(T/W0*cb*12)),cwob=GR.lineWobble(hash(p.seed,11,3),cb,W0),cxs=[],cvs=[],ck;
  for(ck=0;ck<cN;ck++){var cx0=-T/2+T*ck/(cN-1);cxs.push(cx0);cvs.push(cwob(cx0))}
  var ctt=GR.ranked(cvs,cw);
  function crest(x){var u=(x+T/2)/T*(cN-1);if(u<=0)return DIRT+topLo+(topHi-topLo)*ctt[0];if(u>=cN-1)return DIRT+topLo+(topHi-topLo)*ctt[cN-1];var i=Math.floor(u),w2=u-i;return DIRT+topLo+(topHi-topLo)*(ctt[i]+(ctt[i+1]-ctt[i])*w2)}
  var fx=p.foot?{xl:-p.foot[0]*f/2,xr:p.foot[0]*f/2,y0:-p.foot[1]*f/2,yb:p.foot[1]*f/2}:null;
  var X0=fx?fx.xl:-T/2+m,X1=fx?fx.xr:T/2-m,Y0=fx?fx.y0:y0,Y1=fx?fx.yb:y0+D,fp=[[X0,Y0],[X1,Y0],[X1,Y1],[X0,Y1]],seam=fx?[0,0,0,0]:[0,1,1,1];
  /* a region that is not a rectangle: the J corner is an L, a rectangle with a wing standing out of its end toward the front */
  if(p.wing){var ww=p.wing[0]*f,wl=Math.min(p.wing[1]*f,Y0-(-W/2+m)-1);fp=[[X0,Y0],[X1-ww,Y0],[X1-ww,Y0-wl],[X1,Y0-wl],[X1,Y1],[X0,Y1]];seam=[0,0,0,1,1,1]}
  return {fp:fp,seam:seam.map(Boolean),f:f,T:T,W:W,x0:-T/2,x1:T/2,xl:fx?fx.xl:-T/2+m,xr:fx?fx.xr:T/2-m,m:m,y0:fx?fx.y0:y0,D:fx?fx.yb-fx.y0:D,yb:fx?fx.yb:y0+D,free:!!fx,crest:crest,topLo:topLo,topHi:topHi,cells:Math.round(W*T/0.25)};
}
/* ===== the polytope of one block, at stage 2 (boxes), 3 (joints) or 4 (facets) ===== */
function blockPoly(p,g,b,stage,rep){
  var f=g.f,yf=g.y0+b.off,yb=g.yb+0.4,zb0=b.zb,r=R(p.seed,400,b.c,b.i),zb=b.zb,zt=b.zt,s2=Math.SQRT1_2;
  /* a block begins and ends on the row planes: its prism is cut by the plane under it and the plane over it (the crest cuts flat) */
  /* RULE (a stone's bed and the top under it are one surface): each row plane is a mesh of flat facets, the same mesh for every stone. A stone is cut apart along the mesh into pieces, one for each facet it stands on, each piece bounded below and above by that facet's own plane; the stone above is bounded by the same facets, so its bed is the top below it, exactly, and no overhang, however the planes tilt or wave. (The pieces overlap by 0.15 mm at the seams.) */
  function splitOne(Q){if(!b.poly||!g.planes)return [Q];
    var pb=g.planes[b.c],pt=g.planes[cE(b)+1],useB=!!(pb&&pb.s),useT=!!(pt&&pt.s&&!b.crest);if(!useB&&!useT)return [Q];
    var G=(useB?pb:pt).G,dEdge=0,x0=-90,y0=-90,xs=b.polyT.map(function(q){return q[0]}),ys=b.polyT.map(function(q){return q[1]}),i0=Math.floor((Math.min.apply(null,xs)-x0)/G),i1=Math.floor((Math.max.apply(null,xs)-x0)/G),j0=Math.floor((Math.min.apply(null,ys)-y0)/G),j1=Math.floor((Math.max.apply(null,ys)-y0)/G),out=[],ii,jj,tk;
    for(ii=i0;ii<=i1;ii++)for(jj=j0;jj<=j1;jj++){var ax=x0+ii*G,ay=y0+jj*G,tris=[[[ax,ay],[ax+G,ay],[ax,ay+G]],[[ax+G,ay+G],[ax,ay+G],[ax+G,ay]]];
      for(tk=0;tk<2;tk++){var tr=tris[tk];if(polyOverlap(b.polyT,tr)<0.02)continue;var Qt=Q,e3,ok=true;
        for(e3=0;e3<3&&ok;e3++){var A3=tr[e3],B3=tr[(e3+1)%3],ex=B3[0]-A3[0],ey=B3[1]-A3[1],l3=Math.hypot(ex,ey),n3=[ey/l3,-ex/l3,0],R2=clip(Qt,n3,n3[0]*A3[0]+n3[1]*A3[1],'seam');if(R2)Qt=R2;else ok=false}
        if(!ok)continue;var mx=(tr[0][0]+tr[1][0]+tr[2][0])/3,my=(tr[0][1]+tr[1][1]+tr[2][1])/3;
        if(useB){var tb=planeTri(pb,mx,my),ab=tb.z-tb.gx*mx-tb.gy*my,R4=clip(Qt,[tb.gx,tb.gy,-1],-(zb0+ab-dEdge),'bed');if(R4)Qt=R4;else continue}
        if(useT){var tt=planeTri(pt,mx,my),at=tt.z-tt.gx*mx-tt.gy*my,R5=clip(Qt,[-tt.gx,-tt.gy,1],zt+at+dEdge,'top');if(R5)Qt=R5;else continue}
        out.push(Qt)}}
    return out.length?out:[Q]}
  function splitAll(Qs){var out=[];Qs.forEach(function(Q0){splitOne(Q0).forEach(function(q){q.b=b;out.push(q)})});return out}
  var dl=g.planes?Math.max.apply(null,g.planes.map(function(q){return q.s?(q.amp*1.6+Math.hypot(q.gx,q.gy)*Math.max(g.T,g.W)/2):0}))+0.8:0;
  var pbX=g.planes&&g.planes[b.c],ptX=g.planes&&g.planes[cE(b)+1],dlB=(pbX&&pbX.s)?dl:0,dlT=(ptX&&ptX.s&&!b.crest)?dl:0;
  function crestCut(Q){var T0=1e9,k;for(k=0;k<=8;k++){var v=g.crest(b.x0+(b.x1-b.x0)*k/8);if(v<T0)T0=v}if(b.zt<=T0-1e-6)return Q;b.env=b.env||envOf(g,b);var R2,i;for(i=0;i<b.env.length;i++){R2=clip(Q,b.env[i].n,b.env[i].d,'env');if(!R2)return null;Q=R2}return Q}
  if(stage<3){var Q=crestCut(prism(b.poly,zb0-dlB,zt+dlT));if(!Q)return [];Q.b=b;return splitAll([Q])}
  /* neighbours touch at the joint plane and overlap 0.15 mm, so the union has nothing coincident to repair. A stone's weathering is the rock's (step 8): the cell stays the plain prism, cut at its row planes, with the joints' lower mouth (finish) */
  var Q=crestCut(prism(b.poly,zb0-dlB,zt+dlT));if(!Q)return [];Q.b=b;
  return finish(p,g,b,splitAll([Q]),stage,rep);
}
/* the joint's lower mouth, made last: a facet at 50 degrees, sized so the flat underside never starts in front of where the block below stops being a bevel.
   A projection past the block below is the same rule at 55 degrees (a corbel 0.70 of its size tall). Nothing is guessed: it is measured on the faceted blocks. */
/* RULE: a cut at the joint's mouth may never take the bed: the bed of a cell keeps at least 1 mm2 */
function hull2(P){var pts=P.slice().sort(function(a,b){return a[0]-b[0]||a[1]-b[1]}),cr=function(o,a,b){return (a[0]-o[0])*(b[1]-o[1])-(a[1]-o[1])*(b[0]-o[0])},lo=[],up=[],i;for(i=0;i<pts.length;i++){while(lo.length>1&&cr(lo[lo.length-2],lo[lo.length-1],pts[i])<=0)lo.pop();lo.push(pts[i])}for(i=pts.length-1;i>=0;i--){while(up.length>1&&cr(up[up.length-2],up[up.length-1],pts[i])<=0)up.pop();up.push(pts[i])}lo.pop();up.pop();return lo.concat(up)}
function bedOf(Q){var A=0;Q.faces.forEach(function(F){if(F.kind==='bed')A+=areaOf(F.pts)});return A}
function finish(p,g,b,pieces,stage,rep){
  var zb=b.zb,out=[];function bz(x,y){var pb=g.planes&&g.planes[b.c];return zb+(pb&&pb.s?planeOff(pb,x,y):0)}
  /* a chamfer plane follows the bed's slope: it is tilted by the bed's gradient and passes through the same point */
  /* a chamfer's angle is reduced by the bed's own slope so it stays within the overhang band on the side where the bed rises */
  function chamAng(base,x,y){var sd=slopeDeg(x,y);return base+(51-base)*Math.min(1,sd/1.5)-1.1*sd}
  function slopeDeg(x,y){var pb=g.planes&&g.planes[b.c];if(!(pb&&pb.s))return 0;var gr=planeGrad(pb,x,y);return Math.atan(Math.hypot(gr[0],gr[1]))*180/PI}
  function tiltPlane(nn,px,py,pz){var pb=g.planes&&g.planes[b.c];if(!(pb&&pb.s))return {n:nn,d:nn[0]*px+nn[1]*py+nn[2]*pz};var gr=planeGrad(pb,px,py),n2=[nn[0]-nn[2]*gr[0],nn[1]-nn[2]*gr[1],nn[2]];return {n:n2,d:n2[0]*px+n2[1]*py+n2[2]*pz}}
  var lowerStart=-1e9,k;
  if(b.c>0)(g.tsList[b.c-1]||[]).forEach(function(o){if((b.poly&&o.poly?polyOverlap(b.polyT,o.poly)>0.4:(o.x1>b.x0+1e-6&&o.x0<b.x1-1e-6))&&o.ts>lowerStart)lowerStart=o.ts});
  pieces.forEach(function(Q,qi){
    var yfront=1e9,top=1e9;
    Q.faces.forEach(function(F){F.pts.forEach(function(q){if(q[2]<zb+0.6&&q[1]<yfront)yfront=q[1]});if(F.n[2]>0.9)F.pts.forEach(function(q){if(q[1]<top)top=q[1]})});
    if(b.cap&&b.supC&&b.c>0){
      /* RULE R8 (a projection is chamfered over its whole unsupported part): a lip or a ledge cut over one supporting cell is flat only above that cell's top and slopes up from the outline of that top, at 51 degrees from vertical, everywhere else. Material is kept only above the sloped plane of every edge of the top's outline. Pieces over neighbouring supports overlap in front, so the union takes the lower of the surfaces. */
      var ent=(g.tsList[b.c-1]||[]).filter(function(o){return o.poly===b.supC.polyT})[0];
      if(ent&&ent.tv&&ent.tv.length>2){var T=hull2(ent.tv),mT=T.length,ti;
        for(ti=0;ti<mT;ti++){var ta=T[ti],tb=T[(ti+1)%mT],te=[tb[0]-ta[0],tb[1]-ta[1]],tl=Math.hypot(te[0],te[1]);if(tl<1e-6)continue;var tn=[te[1]/tl,-te[0]/tl];
          /* the hull is counter-clockwise: the outward normal of an edge is (dy,-dx) */
          var an8=rad(chamAng(38.76,ta[0],ta[1])),nn3=[tn[0]*Math.sin(an8),tn[1]*Math.sin(an8),-Math.cos(an8)],tq3=tiltPlane(nn3,ta[0],ta[1],bz(ta[0],ta[1])),Q3=clip(Q,tq3.n,tq3.d,'joint');if(Q3&&Q3!==Q&&bedOf(Q3)>=0.5)Q=Q3}}}
    else if(b.poly&&b.fe&&b.fe.length&&b.c>0){
      /* the joint's lower mouth, edge by edge: along each face edge's normal, sized so the flat underside never starts in front of where the cell below stops being a bevel */
      var lows=(g.tsList[b.c-1]||[]).filter(function(o){return o.poly&&polyOverlap(b.polyT,o.poly)>0.4});
      b.fe.forEach(function(e){var ed=[(e.b[0]-e.a[0])/e.l,(e.b[1]-e.a[1])/e.l],sU=e.n[0]*e.a[0]+e.n[1]*e.a[1],sL=-1e9;
        /* the cell below stops being a top at its own front; take the most recessed of the cells under this edge, so the bed starts behind every one of them */
        sL=1e9;lows.forEach(function(o){var mo=-1e9,ma=-1e9;(o.tv||[]).forEach(function(v){var sv=e.n[0]*v[0]+e.n[1]*v[1],t=(v[0]-e.a[0])*ed[0]+(v[1]-e.a[1])*ed[1];if(sv>ma)ma=sv;if(t>-1&&t<e.l+1&&sv>mo)mo=sv});if(mo<-1e8)mo=ma;if(mo>-1e8&&mo<sL)sL=mo});if(sL>1e8)sL=-1e9;
        var need=sL>-1e8?Math.max(0,sU-sL):0;
        /* RULE (set back first): where the cell below stops being a top more than 0.6 mm behind this edge, the edge is cut back to it (a vertical face), not chamfered; a chamfer that deep would take the whole bed */
        if(need>0.6&&!b.cap){var Qs=clip(Q,e.n,sL-0.25,'setback');if(Qs&&Qs!==Q&&bedOf(Qs)>=1.0){Q=Qs;rep.trimmed=(rep.trimmed||0)+1;return}/* RULE (a short overhang is chamfered): when the set-back cut would take the bed, an overhang up to 1.2 mm falls through to the chamfer, not left flat over the cell below */if(need>1.2)return}
        var db=Math.max(MOUTH*Math.tan(rad(48)),need+0.05),hb=db/Math.tan(rad((need>0.6?53:48)-1.1*slopeDeg((e.a[0]+e.b[0])/2,(e.a[1]+e.b[1])/2))),Lh=Math.hypot(hb,db),nn=[e.n[0]*hb/Lh,e.n[1]*hb/Lh,-db/Lh],emx=(e.a[0]+e.b[0])/2,emy=(e.a[1]+e.b[1])/2,tq=tiltPlane(nn,emx,emy,bz(emx,emy)+hb),Q2=clip(Q,tq.n,tq.d,'joint');if(Q2&&(b.cap||bedOf(Q2)>=1.0))Q=Q2})
      /* RULE R9 (one reader of a lower cell's top outline): wherever the outline of the top a cell stands on (after its facets and chips) has no other cell's top beyond it, the bed is chamfered from that outline at 51 degrees from vertical, as R8 does for a lip, so a lip taken from the top edge by weathering never leaves a flat ceiling */
      lows.forEach(function(o){if(!o.tv||o.tv.length<3)return;var T=hull2(o.tv),mT=T.length,ti;if(mT<3)return;
        for(ti=0;ti<mT;ti++){var ta=T[ti],tb=T[(ti+1)%mT],te=[tb[0]-ta[0],tb[1]-ta[1]],tl=Math.hypot(te[0],te[1]);if(tl<0.3)continue;var tn=[te[1]/tl,-te[0]/tl],M=[(ta[0]+tb[0])/2+tn[0]*0.6,(ta[1]+tb[1])/2+tn[1]*0.6];
          if(!inPolyAny(b.polyT,M[0],M[1]))continue;if(lows.some(function(o2){return o2!==o&&inPolyAny(o2.poly,M[0],M[1])}))continue;
          var an9=rad(chamAng(38.76,ta[0],ta[1])),nn3=[tn[0]*Math.sin(an9),tn[1]*Math.sin(an9),-Math.cos(an9)],tq3=tiltPlane(nn3,ta[0],ta[1],bz(ta[0],ta[1])),Q3=clip(Q,tq3.n,tq3.d,'joint');if(Q3&&Q3!==Q&&bedOf(Q3)>=0.5){Q=Q3;rep.r9=(rep.r9||0)+1}}})}
    else if(b.c>0&&lowerStart>-1e8&&yfront<1e8){
      var need=Math.max(0,lowerStart-yfront),db=Math.max(MOUTH*Math.tan(rad(48)),need+0.05);
      var hb=db/Math.tan(rad(need>0.6?53:48)),Lh=Math.hypot(hb,db),nn=[0,-hb/Lh,-db/Lh],d=nn[1]*yfront+nn[2]*(zb+hb);
      var Q2=clip(Q,nn,d,'joint');if(Q2)Q=Q2}
    Q.b=b;pieces[qi]=Q});
  /* the block's own top-face start, for the block above (a tilted or waved top is read within the planes' reach of its level) */
  var tmg=(g.planes&&g.planes[cE(b)+1]&&g.planes[cE(b)+1].s&&!b.crest)?(g.dl||0)+0.05:0.05,ts=1e9;pieces.forEach(function(Q){Q.faces.forEach(function(F){if(F.n[2]>(tmg>1?0.9:0.5))F.pts.forEach(function(q){if(q[2]>b.zt-tmg&&q[1]<ts)ts=q[1]})})});
  if(ts>1e8)ts=g.y0+b.off+MOUTH;
  var tv=[];pieces.forEach(function(Q){Q.faces.forEach(function(F){if(F.n[2]>(tmg>1?0.9:0.5))F.pts.forEach(function(q){if(q[2]>b.zt-tmg)tv.push([q[0],q[1]])})})});
  (g.tsList[cE(b)]=g.tsList[cE(b)]||[]).push({x0:b.x0,x1:b.x1,ts:ts,poly:b.polyT,tv:tv});
  return pieces;
}
/* a joint that no block above carries up is roofed: a lean-to wedge filling the top of its V, its underside at 48 degrees from vertical, hanging from one wall.
   The block that bridges the joint then has no flat ceiling, and no tip is printed in air. */
function mirrorY(P,ym){return {b:P.b,faces:P.faces.map(function(F){return {n:[F.n[0],-F.n[1],F.n[2]],kind:F.kind,pts:F.pts.map(function(q){return [q[0],2*ym-q[1],q[2]]}).reverse()}})}}
/* ===== the whole build, timed by level; returns polytopes and a record ===== */
function now(){return (typeof performance!=='undefined'?performance:Date).now()}
function build(p,stage){
  stage=stage==null?5:stage;
  var rep={shiftFail:0,raised:0,dropped:0,cutsL3:0,time:{},tris:{},bytes:{}},t0,g,j,l2,polys=[],i;
  t0=now();g=L0(p);rep.time.L0=now()-t0;
  t0=now();j=L1c(p,g,rep);rep.time.L1=now()-t0;rep.tris.L1=0;
  var nj=0;j.pts.forEach(function(P){nj+=P.length});rep.joints=nj;rep.courses=j.pts.length;
  {var FPo=g.fp,ar=0,ii;for(ii=0;ii<FPo.length;ii++){var qa=FPo[ii],qb=FPo[(ii+1)%FPo.length];ar+=qa[0]*qb[1]-qb[0]*qa[1]}if(ar<0){var nn0=FPo.length,sm0=g.seam;g.fp=FPo.slice().reverse();g.seam=g.fp.map(function(q,i3){return sm0[(nn0-2-i3+nn0)%nn0]})}
    /* the one table of the footprint's edges: its face, whether it is a seam, and how far its normal runs to the tile's margin (a lip may stand out that far) */
    g.edges=g.fp.map(function(a,ii){var b2=g.fp[(ii+1)%g.fp.length],e=[b2[0]-a[0],b2[1]-a[1]],l=Math.hypot(e[0],e[1])||1,n=[e[1]/l,-e[0]/l],my=n[1]<0?(a[1]+g.W/2-g.m-0.6)/(-n[1]):n[1]>0?(g.W/2-g.m-0.6-a[1])/n[1]:1e9,mx=n[0]<0?(a[0]-g.xl-0.6)/(-n[0]):n[0]>0?(g.xr-0.6-a[0])/n[0]:1e9;return {a:a,b:b2,l:l,t:[e[0]/l,e[1]/l],n:n,face:faceOf(n),seam:g.seam[ii],maxOut:Math.min(my,mx)}})}
  t0=now();l2=L2c(p,g,j,rep);rep.time.L2=now()-t0;
  /* the face of the block below, for a projection's corbel */
  var byCol={};l2.blocks.forEach(function(b){(byCol[b.c]=byCol[b.c]||[]).push(b)});
  g.planes=j.planes;g.dl=g.planes?Math.max.apply(null,g.planes.map(function(q){return q.s?(q.amp*1.6+Math.hypot(q.gx,q.gy)*Math.max(g.T,g.W)/2):0}))+0.8:0;g.pcx=j.cx;g.pcy=j.cy;g.byCol=byCol;g.faceBelow=function(b){var m=0;(byCol[b.c-1]||[]).forEach(function(o){if(o.x1>b.x0+1e-6&&o.x0<b.x1-1e-6)m=Math.max(m,o.off-b.off)});return m};
  var tri={2:0,3:0},tm={2:0,3:0};
  /* build each stage's polytopes separately so each is timed and counted; only the last kept */
  var kept=null;
  [2,3].forEach(function(s){
    if(s>Math.min(3,Math.max(2,stage)))return;
    t0=now();var list=[],tt=0;g.tsList={};
    l2.blocks.forEach(function(b){blockPoly(p,g,b,s,rep).forEach(function(P){list.push(P);tt+=triCount(P)})});
    tm[s]=now()-t0;tri[s]=tt;kept=list});
  rep.time.L2b=tm[2];rep.tris.L2=tri[2];
  if(stage>=3){rep.time.L3=tm[3]-tm[2];rep.tris.L3=tri[3]-tri[2]}
  /* L4 is the rock's (rock.js): its time and triangles are written when the stones arrive */
  polys=kept;var n0=polys.length,ym=g.y0+g.D/2;
  if(p.wall===1){var m2=polys.map(function(P){return mirrorY(P,ym)});polys=polys.concat(m2)}
  rep.blocks=polys.length;
  var core=box(-1,1,g.y0+8,g.y0+9,DIRT-1.5,DIRT-0.5);core.core=true;
  var dirt=box(g.x0,g.x1,-g.W/2,g.W/2,0,DIRT);dirt.dirt=true;
  var bytes=0;polys.forEach(function(P){bytes+=verts(P).length*12+P.faces.length*16});rep.bytes.polys=bytes;
  return {n0:n0,ym:p.wall===1?ym:null,g:g,l1:j,l2:l2,polys:polys,core:core,dirt:dirt,rep:rep,p:p};
}
/* ===== L1 and L2 as cells: a convex partition of the footprint, a convex prism for each cell ===== */
/* clip a convex polygon (a list of [x,y]) to n.p <= d */
function clipPoly(P,n,d){var out=[],i,a,b,sa,sb,t;for(i=0;i<P.length;i++){a=P[i];b=P[(i+1)%P.length];sa=n[0]*a[0]+n[1]*a[1]-d;sb=n[0]*b[0]+n[1]*b[1]-d;
  if(sa<=1e-9)out.push(a);if((sa<-1e-9&&sb>1e-9)||(sa>1e-9&&sb<-1e-9)){t=sa/(sa-sb);out.push([a[0]+(b[0]-a[0])*t,a[1]+(b[1]-a[1])*t])}}return out}
function inPolyAny(P,x,y){var c=false,i,j;for(i=0,j=P.length-1;i<P.length;j=i++){if(((P[i][1]>y)!==(P[j][1]>y))&&(x<(P[j][0]-P[i][0])*(y-P[i][1])/(P[j][1]-P[i][1])+P[i][0]))c=!c}return c}
function inPoly(P,x,y){var i;for(i=0;i<P.length;i++){var a=P[i],b=P[(i+1)%P.length];if((b[0]-a[0])*(y-a[1])-(b[1]-a[1])*(x-a[0])<-1e-9)return false}return true}
function polyArea(P){var s=0,i;for(i=0;i<P.length;i++){var a=P[i],b=P[(i+1)%P.length];s+=a[0]*b[1]-b[0]*a[1]}return s/2}
function polyCentroid(P){var x=0,y=0,i;for(i=0;i<P.length;i++){x+=P[i][0];y+=P[i][1]}return [x/P.length,y/P.length]}
function polyInter(A,B){var C=A.slice(),i;for(i=0;i<B.length&&C.length>2;i++){var a=B[i],b=B[(i+1)%B.length],e=[b[0]-a[0],b[1]-a[1]],n=[e[1],-e[0]],l=Math.hypot(n[0],n[1]);if(l<1e-9)continue;n=[n[0]/l,n[1]/l];C=clipPoly(C,n,n[0]*a[0]+n[1]*a[1])}return C.length>2?C:[]}
function polyOverlap(A,B){var C=A.slice(),i,m;for(i=0;i<B.length&&C.length>2;i++){var a=B[i],b=B[(i+1)%B.length],e=[b[0]-a[0],b[1]-a[1]],n=[e[1],-e[0]],l=Math.hypot(n[0],n[1]);if(l<1e-9)continue;n=[n[0]/l,n[1]/l];C=clipPoly(C,n,n[0]*a[0]+n[1]*a[1])}return C.length>2?Math.abs(polyArea(C)):0}
/* a tagged clip: every vertex carries the tag of the edge that leaves it (b boundary, e end, k back, n a neighbour cell, g a ghost: the open air in front of the face) */
function clipTagged(P,n,d,tag){var out=[],i,a,b,sa,sb,t,q;
  for(i=0;i<P.length;i++){a=P[i];b=P[(i+1)%P.length];sa=n[0]*a.p[0]+n[1]*a.p[1]-d;sb=n[0]*b.p[0]+n[1]*b.p[1]-d;
    if(sa<=1e-9){out.push({p:a.p,e:a.e});if(sb>1e-9){t=sa/(sa-sb);out.push({p:[a.p[0]+(b.p[0]-a.p[0])*t,a.p[1]+(b.p[1]-a.p[1])*t],e:tag})}}
    else if(sb<=1e-9){t=sa/(sa-sb);out.push({p:[a.p[0]+(b.p[0]-a.p[0])*t,a.p[1]+(b.p[1]-a.p[1])*t],e:a.e})}}
  return out}
/* a cell clipped to another cell's outline: its joints stay joints (grown by the overlap), its free edges stay free */
function clipByCell(C,o,grow,fg,p){var R2=C,i,m=o.polyT.length;for(i=0;i<m&&R2;i++){var a=o.polyT[i],b=o.polyT[(i+1)%m],e=[b[0]-a[0],b[1]-a[1]],l=Math.hypot(e[0],e[1])||1,n=[e[1]/l,-e[0]/l],joint=o.tags[i]==='n'||o.tags[i]==='m';R2=clipCellTagged(R2,n,n[0]*a[0]+n[1]*a[1]+(joint?grow:(fg&&o.tags[i]!=='x'&&toward(p,n)?fg:0)),joint?'n':'g')}return R2}
function clipCellTagged(C,n,d,tag){var P=C.pts.map(function(q,i){return {p:q,e:C.tags[i]}}),R2=clipTagged(P,n,d+1e-6,tag||'g');if(R2.length<3)return null;return {pts:R2.map(function(v){return v.p}),tags:R2.map(function(v){return v.e})}}
/* the Voronoi cell of site k among sites (real points first, then ghosts); grow moves the bisectors of real neighbours outward (the overlap that keeps the union free of coincident faces) */
function voronoiCell(sites,k,nReal,X0,X1,Y0,Y1,grow,reach){var p=sites[k],C=[{p:[X0,Y0],e:'b'},{p:[X1,Y0],e:'e'},{p:[X1,Y1],e:'k'},{p:[X0,Y1],e:'e'}],j;
  for(j=0;j<sites.length;j++){if(j===k)continue;var q=sites[j],dx=q[0]-p[0],dy=q[1]-p[1],dd=Math.hypot(dx,dy);if(dd>reach||dd<1e-9)continue;var n=[dx/dd,dy/dd],real=j<nReal,d=n[0]*(p[0]+q[0])/2+n[1]*(p[1]+q[1])/2+(real?grow:0);C=clipTagged(C,n,d,real?'n':'g');if(C.length<3)return null}
  /* drop repeated vertices (a zero-length edge has no direction, and every overlap test reads edge directions) */
  var D=[],ii;for(ii=0;ii<C.length;ii++){var nx2=C[(ii+1)%C.length];if(Math.abs(C[ii].p[0]-nx2.p[0])+Math.abs(C[ii].p[1]-nx2.p[1])<1e-7)continue;D.push(C[ii])}
  if(D.length<3)return null;
  return {pts:D.map(function(v){return v.p}),tags:D.map(function(v){return v.e})}}
/* the height of a row plane above its level at (x,y): tilt, waves and noise, laid on a grid of triangles (12 mm) so that the plane is made of flat facets and every block that stands on one facet shares it exactly */
function planeSmooth(pl,x,y){var v=pl.gx*(x-pl.cx)+pl.gy*(y-pl.cy),k;for(k=0;k<pl.waves.length;k++){var w=pl.waves[k];v+=w.a*Math.sin(2*PI*((x-pl.cx)*w.c+(y-pl.cy)*w.s)/w.l+w.ph)}
  if(pl.amp)v+=pl.amp*0.35*2*(noise1(pl.ns,(x-pl.cx)*0.9+(y-pl.cy)*0.4,40)-0.5);return v*pl.s}
function planeTri(pl,x,y){var G=pl.G,x0=-90,y0=-90,i=Math.floor((x-x0)/G),j=Math.floor((y-y0)/G),u=(x-x0)/G-i,v=(y-y0)/G-j,ax=x0+i*G,ay=y0+j*G,
    z00=planeSmooth(pl,ax,ay),z10=planeSmooth(pl,ax+G,ay),z01=planeSmooth(pl,ax,ay+G),z11=planeSmooth(pl,ax+G,ay+G);
  if(u+v<=1){var gx=(z10-z00)/G,gy=(z01-z00)/G;return {gx:gx,gy:gy,z:z00+gx*(x-ax)+gy*(y-ay)}}
  var gx2=(z11-z01)/G,gy2=(z11-z10)/G;return {gx:gx2,gy:gy2,z:z11+gx2*(x-ax-G)+gy2*(y-ay-G)}}
function planeOff(pl,x,y){if(!pl.s)return 0;return planeTri(pl,x,y).z}
function planeGrad(pl,x,y){if(!pl.s)return [0,0];var t=planeTri(pl,x,y);return [t.gx,t.gy]}
function L1c(p,g,rep){
  var f=g.f,r=R(p.seed,1),reg=p.regularity==null?0.3:p.regularity,Ht=g.topHi,whi=p.width[1]*f,wlo=p.width[0]*f,w=(wlo+whi)/2,hs=[],sum=0;
  while(sum<Ht-3*f){var h=Math.max(MINSTONE*f,draw(r,[p.height[0]*f,p.height[1]*f,p.height[2]]));hs.push(h);sum+=h}
  if(hs.length>1){var sl=sum-hs[hs.length-1];if(Math.abs(Ht/sl-1)<Math.abs(Ht/sum-1)){hs.pop();sum=sl}}
  var fit=Ht/sum;hs=hs.map(function(h){return h*fit});rep.fit=fit;
  var minY=Math.min.apply(null,g.fp.map(function(q){return q[1]})),T=g.xr-g.xl,Dp=g.yb-minY,nx=Math.max(1,Math.round(T/w)),ny=Math.max(1,Math.round(Dp/w)),sx=T/nx,sy=Dp/ny,pts=[],i,j,c;
  var jit=(1-reg)*0.45;
  for(j=0;j<ny;j++)for(i=0;i<nx;i++){var off=(j%2)*0.5*sx*reg,q0=[Math.min(g.xr-0.35*sx,g.xl+(i+0.5)*sx+off+(r()*2-1)*jit*sx),minY+(j+0.5)*sy+(r()*2-1)*jit*sy,0];if(inPolyAny(g.fp,q0[0],q0[1]))pts.push(q0)}
  var courses=[],prev=null,prevPts=null;
  for(c=0;c<hs.length;c++){
    var rr=R(p.seed,100,c),bondHere=p.bond[0]+(p.bond[1]-p.bond[0])*(hs.length>1?c/(hs.length-1):0),cur;
    if(!prevPts)cur=pts.map(function(q,k){return [q[0],q[1],k]});
    else{cur=[];var shiftAmt=draw(rr,p.shift)*w;
      prevPts.forEach(function(q,k){var carried=rr()<Math.max(0,Math.min(1,(1-bondHere)*(0.4+1.2*rr())));
        if(carried){cur.push([q[0],q[1],q[2]]);return}
        var ang=reg>=0.999?(rr()<0.5?0:PI):rr()*2*PI,nq=[Math.max(g.xl+0.3*sx,Math.min(g.xr-0.3*sx,q[0]+Math.cos(ang)*shiftAmt)),Math.max(minY+0.3*sy,Math.min(g.yb-0.3*sy,q[1]+Math.sin(ang)*shiftAmt)),-1-k];if(!inPolyAny(g.fp,nq[0],nq[1])){nq[0]=q[0];nq[1]=q[1]}
        var ok=true;cur.forEach(function(o){if(Math.hypot(o[0]-nq[0],o[1]-nq[1])<0.5*Math.min(sx,sy))ok=false});if(ok)cur.push(nq);else{cur.push([q[0],q[1],q[2]]);rep.shiftFail++}})}
    courses.push(cur);prevPts=cur}
  /* RULE (the row planes come first): every boundary between rows is fixed for the whole wall before any block exists. A plane is its height, a tilt (up to dip degrees), and waves (sinusoids of up to wave mm across the wall, random in direction and wavelength, with value noise a third as strong); the bottom plane is the ground and stays flat. Any two planes are held at least 0.6 of the row's own height apart everywhere in the wall (a row is never thinner than its minimum). Blocks begin and end on these planes. */
  var planes=[],zc0=DIRT-0.4,cxw=0,cyw=g.y0+g.D/2,rp=R(p.seed,500),tn=Math.tan(rad(p.dip||0)),amp=(p.wave||0)*f,pi2,k2;
  for(pi2=0;pi2<=hs.length;pi2++){var an=rp()*2*PI,mg=tn*rp(),pl={z:zc0,gx:Math.cos(an)*mg,gy:Math.sin(an)*mg,s:(pi2===0||(!tn&&!amp))?0:1,waves:[],ns:p.seed*31+pi2,amp:amp,cx:cxw,cy:cyw,G:12*f};
    for(k2=0;k2<2;k2++){var th=rp()*2*PI;pl.waves.push({a:amp*(k2===0?0.7:0.4)*(0.5+rp()),c:Math.cos(th),s:Math.sin(th),l:(30+60*rp())*f,ph:rp()*2*PI})}
    planes.push(pl);if(pi2<hs.length)zc0+=hs[pi2]}
  var xs0=g.fp.map(function(q){return q[0]}),ys0=g.fp.map(function(q){return q[1]}),gx0=Math.min.apply(null,xs0),gx1=Math.max.apply(null,xs0),gy0=Math.min.apply(null,ys0),gy1=Math.max.apply(null,ys0);
  for(pi2=1;pi2<planes.length;pi2++){var need=0.6*hs[pi2-1],guard2=0;
    while(guard2++<8){var dmin=1e9,ix,iy;for(ix=0;ix<=8;ix++)for(iy=0;iy<=8;iy++){var X=gx0+(gx1-gx0)*ix/8,Y=gy0+(gy1-gy0)*iy/8,dz=planes[pi2].z+planeOff(planes[pi2],X,Y)-(planes[pi2-1].z+planeOff(planes[pi2-1],X,Y));if(dz<dmin)dmin=dz}
      if(dmin>=need)break;planes[pi2].s*=0.7;if(pi2>1)planes[pi2-1].s*=0.85}}
  return {pts:courses,heights:hs,sx:sx,sy:sy,cells:true,planes:planes,cx:cxw,cy:cyw}}
/* a polygon cut into convex parts: ear clipping, then neighbouring parts joined while they stay convex; an edge of a part that is an edge of the polygon keeps its index */
function convexParts(fp){var n=fp.length,V=[],i,tris=[],guard=0;for(i=0;i<n;i++)V.push(i);
  function cr(a,b,c){return (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])}
  while(V.length>3&&guard++<1000){var cut=false;for(i=0;i<V.length&&!cut;i++){var a=V[(i+V.length-1)%V.length],b=V[i],c=V[(i+1)%V.length];if(cr(fp[a],fp[b],fp[c])<=1e-9)continue;var ok=true;V.forEach(function(w){if(w===a||w===b||w===c)return;var Q=fp[w];if(cr(fp[a],fp[b],Q)>=-1e-9&&cr(fp[b],fp[c],Q)>=-1e-9&&cr(fp[c],fp[a],Q)>=-1e-9)ok=false});if(ok){tris.push([a,b,c]);V.splice(i,1);cut=true}}if(!cut)break}
  if(V.length===3)tris.push([V[0],V[1],V[2]]);
  var parts=tris,merged=true,mguard=0;
  while(merged&&mguard++<2000){merged=false;
    outer:for(var x=0;x<parts.length;x++)for(var y=x+1;y<parts.length;y++){var A=parts[x],B=parts[y];
      for(var pa=0;pa<A.length;pa++){var u=A[pa],v=A[(pa+1)%A.length];for(var qb=0;qb<B.length;qb++)if(B[qb]===v&&B[(qb+1)%B.length]===u){
        var M=[],t;for(t=1;t<=A.length;t++)M.push(A[(pa+t)%A.length]);for(t=2;t<B.length;t++)M.push(B[(qb+t)%B.length]);
        var convex=true;for(t=0;t<M.length;t++)if(cr(fp[M[t]],fp[M[(t+1)%M.length]],fp[M[(t+2)%M.length]])<-1e-9)convex=false;
        if(convex){parts.splice(y,1);parts[x]=M;merged=true;break outer}}}}}
  return parts.map(function(M){return {v:M.map(function(w){return fp[w]}),fi:M.map(function(w,t){var nx=M[(t+1)%M.length];return nx===(w+1)%n?w:-1})}})}
/* the cell of site k in a convex part of the footprint: the part's outer edges moved by the course's offsets, its cut edges joints, the bisectors of the other sites joints (grown by grow) */
function cellIn(sites,k,pt,off,grow,reach,seam){var p=sites[k],C=[{p:[-1e4,-1e4],e:'g'},{p:[1e4,-1e4],e:'g'},{p:[1e4,1e4],e:'g'},{p:[-1e4,1e4],e:'g'}],m=pt.v.length,i,j;
  for(i=0;i<m;i++){var a=pt.v[i],b=pt.v[(i+1)%m],e=[b[0]-a[0],b[1]-a[1]],l=Math.hypot(e[0],e[1]);if(l<1e-9)continue;var n=[e[1]/l,-e[0]/l],fi=pt.fi[i],s=fi>=0?off[fi]:grow;C=clipTagged(C,n,n[0]*a[0]+n[1]*a[1]+s,fi>=0?(seam[fi]?'x':'g'):'n');if(C.length<3)return null}
  for(j=0;j<sites.length;j++){if(j===k)continue;var q=sites[j],dx=q[0]-p[0],dy=q[1]-p[1],dd=Math.hypot(dx,dy);if(dd>reach||dd<1e-9)continue;var nn=[dx/dd,dy/dd],d=nn[0]*(p[0]+q[0])/2+nn[1]*(p[1]+q[1])/2+grow;C=clipTagged(C,nn,d,'n');if(C.length<3)return null}
  var D=[],ii;for(ii=0;ii<C.length;ii++){var nx2=C[(ii+1)%C.length];if(Math.abs(C[ii].p[0]-nx2.p[0])+Math.abs(C[ii].p[1]-nx2.p[1])<1e-7)continue;D.push(C[ii])}
  if(D.length<3)return null;
  return {pts:D.map(function(v){return v.p}),tags:D.map(function(v){return v.e})}}
/* RISE: NO OVERLAPS. A stone that rises through a row keeps its footprint there. The row's cells are the Voronoi cells of its sites (the rising stone's site stays as a ghost point), but the bisectors with a ghost point are not the stone's footprint, so a cell may reach into it. carveRising cuts each cell by the one edge line of the footprint that leaves it most (a convex cut, outside the footprint): the cell and the stone then share an edge (tagged a neighbour joint), never area. The grown copy keeps its overlap OVL past that edge, as every joint does. */
function carveRising(C,CX,rising){var i;
  for(i=0;i<rising.length&&C;i++){var F=rising[i].b.polyT;if(polyOverlap(C.pts,F)<0.02)continue;
    var sg=polyArea(F)>=0?1:-1,m=F.length,best=null,bestA=-1,e;
    for(e=0;e<m;e++){var a=F[e],b=F[(e+1)%m],ex=b[0]-a[0],ey=b[1]-a[1],l=Math.hypot(ex,ey)||1,n=[sg*ey/l,-sg*ex/l],d=n[0]*a[0]+n[1]*a[1];   /* n points out of the footprint, so keeping n.p >= d... clipCellTagged keeps n.p <= d: use -n */
      var X=clipCellTagged(C,[-n[0],-n[1]],-d,'n');if(!X)continue;var ar=Math.abs(polyArea(X.pts));if(ar>bestA){bestA=ar;best={n:n,d:d}}}
    if(!best)return null;
    C=clipCellTagged(C,[-best.n[0],-best.n[1]],-best.d,'n');CX=CX&&clipCellTagged(CX,[-best.n[0],-best.n[1]],-(best.d-OVL),'n')||CX&&null}
  return C?{C:C,CX:CX||C}:null}
/* SPAN MAKES ONE STONE. A merge of neighbours is a group of cells that is one stone (b.mg: the group's members, shared). The shared edge is an interior edge: tagged 'm', it is no joint and no free face, and the rock builds the group as one stone, the cells' parts cut apart along it (a crack inside the stone, found by the rock's cell). snapEdge makes the two cells' copies of the shared edge the same line exactly (the match that found it is within 1e-3; the rock reads shared edges to 1e-5). */
function snapEdge(A,ea,B,eb){var n=B.polyT.length,P=B.polyT.slice();P[eb]=A.polyT[(ea+1)%A.polyT.length].slice();P[(eb+1)%n]=A.polyT[ea].slice();B.polyT=P}
/* the group is not an enumerable field of a cell (it holds the cells that hold it): a build record, a job and a spec stay plain data */
function groupOf(b){if(!b.mg)Object.defineProperty(b,'mg',{value:{m:[b]},writable:true,enumerable:false,configurable:true});return b.mg}
function joinStone(A,B){var ga=groupOf(A),gb=groupOf(B);if(ga===gb)return;gb.m.forEach(function(x){x.mg=ga;ga.m.push(x)})}
/* a stone's cells share one top: the highest of them (raised, never lowered: the stones above rest on what is finally there). Their beds stay each on what it rests on. */
function sameTop(row){var seen=[];row.forEach(function(b){var G=b.mg;if(!G||seen.indexOf(G)>=0)return;seen.push(G);var hi=-1e9;G.m.forEach(function(x){if(x.zt>hi)hi=x.zt});G.m.forEach(function(x){if(x.zt<hi){x.hb+=hi-x.zt;x.zt=hi}})})}
/* the stone each cell belongs to, by key (the first cell of the group that is still standing); a cell alone is its own stone */
function stoneKeys(kept){kept.forEach(function(b){var G=b.mg,L=b;if(G)for(var i=0;i<G.m.length;i++)if(kept.indexOf(G.m[i])>=0){L=G.m[i];break}b.sk=L.c+'_'+L.i})}
/* the row a block ends on (a block that rises through several rows starts on c and ends on c2) */
function cE(b){return b.c2==null?b.c:b.c2}
function freeEdges(poly,tags){var fe=[],m=poly.length,i;for(i=0;i<m;i++)if(tags[i]!=='n'&&tags[i]!=='m'){var a=poly[i],b2=poly[(i+1)%m],e=[b2[0]-a[0],b2[1]-a[1]],l=Math.hypot(e[0],e[1]);if(l>0.6){var nn=[e[1]/l,-e[0]/l];fe.push({a:a,b:b2,n:nn,l:l,face:faceOf(nn),seam:tags[i]==='x'})}}return fe}
/* STEP 5a: the recess is each stone's (it was a step back of the whole row; the word was renamed because the meaning changed). After the stones of a course are made, carried up (rise) and joined (span), the stones that have a free front edge are set back by an amount of their own, drawn from the recess range by the graded rule along the course: the stones (a joined stone is one) are ranked by a smooth wobble along the front and each rank is mapped through the recess's weighting. A stone's front edges move back by its amount (its cell is cut by a plane parallel to each); a head joint that this opens, where a neighbour now stands further back, becomes a free face. A joined stone moves as one. A lip row keeps its own rule. */
function recessStones(p,g,row,merges,c,f,rep,prev){var lo=p.recess[0]*f,hi=p.recess[1]*f;if(!(hi>0)||!row.length)return;
  var par=row.map(function(b,i){return i});function find(i){while(par[i]!==i){par[i]=par[par[i]];i=par[i]}return i}
  merges.forEach(function(m){par[find(m[0])]=find(m[1])});
  /* the edges a recess moves: free (not a seam) and on a face whose switch is on */
  function onEdges(b){return b.fe.filter(function(e){return !e.seam&&faceOn(p,e.face)})}
  var groups={};row.forEach(function(b,i){var r=find(i);(groups[r]=groups[r]||[]).push(b)});
  var all=Object.keys(groups).map(function(k){return groups[k]}),amt=new Map(),setb=new Map();
  var sb=p.sbw||{w:'flat',bumps:2.5};
  /* one amount for each stone and each face direction it has a free edge on: the stones (a joined stone is one) that have such an edge are ranked along the face by a smooth wobble (the front's key is the legacy one, every other face adds its direction) and each rank is mapped through the recess's weighting */
  FACES.forEach(function(face,d){
    var gl=all.filter(function(G){return G.some(function(b){return onEdges(b).some(function(e){return e.face===face})})});if(!gl.length)return;
    var side=face==='left'||face==='right',ax=side?1:0;
    gl.forEach(function(G){var s=0;G.forEach(function(b){s+=b.cx[ax]});G.x=s/G.length});gl.sort(function(a,b){return a.x-b.x});
    var wob=GR.lineWobble(faceKey(p.seed,9301,c,face),sb.bumps||2.5,SQUARE*f),t=GR.ranked(gl.map(function(G){return wob(G.x)}),GR.wOf(sb));
    /* a stone never stands in front of the stone it rests on: its recess is at least the largest recess among the stones under it that it overlaps, on the same face (so there is no overhang from the recess itself, which is the stay-behind of the face band, per stone). The recess does not accumulate: it is the largest under it, never their sum, and never more than the range's top. */
    gl.forEach(function(G,gi){var s=lo+(hi-lo)*t[gi];
      if(prev)G.forEach(function(b){prev.forEach(function(o){if(o.recess&&o.recess[d]>s&&polyOverlap(b.polyT,o.polyT)>0.4)s=Math.min(o.recess[d],hi)})});
      var a=amt.get(G)||[0,0,0,0];a[d]=s;amt.set(G,a);G.forEach(function(b){b.recess=b.recess||[0,0,0,0];b.recess[d]=s})})});
  all.forEach(function(G){var a=amt.get(G);if(!a)return;
    G.forEach(function(b){setb.set(b,a.slice());
      var C={pts:b.polyT,tags:b.tags},X=b.CXo||{pts:b.poly,tags:b.tags},ok=true;
      onEdges(b).forEach(function(e){if(!ok)return;var s=a[faceD(e.face)],d=e.n[0]*e.a[0]+e.n[1]*e.a[1]-s,C2=clipCellTagged(C,e.n,d,'g'),X2=clipCellTagged(X,e.n,d,'g');if(!C2||!X2||Math.abs(polyArea(C2.pts))<1.0){ok=false;return}C=C2;X=X2});
      if(!ok){rep.recessSkipped=(rep.recessSkipped||0)+1;setb.set(b,[0,0,0,0]);return}
      b.polyT=C.pts;b.tags=C.tags;b.poly=X.pts;b.CXo=X;b.fe=freeEdges(b.polyT,b.tags);b.free=b.fe.length>0;b.cx=polyCentroid(b.polyT);var xs=b.polyT.map(function(q){return q[0]});b.x0=Math.min.apply(null,xs);b.x1=Math.max.apply(null,xs);rep.recess=(rep.recess||0)+1})});
  function deeper(so,sb2){if(!so)return false;sb2=sb2||[0,0,0,0];return so.some(function(v,d){return v>sb2[d]+0.1})}
  /* a joint whose neighbour now stands further back is open to the air along it: a free face */
  function along(b,i,o){var a=b.polyT[i],e=b.polyT[(i+1)%b.polyT.length],dx=e[0]-a[0],dy=e[1]-a[1],l=Math.hypot(dx,dy)||1,j;
    for(j=0;j<o.polyT.length;j++){var a2=o.polyT[j],e2=o.polyT[(j+1)%o.polyT.length],ex=e2[0]-a2[0],ey=e2[1]-a2[1],l2=Math.hypot(ex,ey)||1;if((dx*ex+dy*ey)/(l*l2)>-0.99)continue;
      if(Math.abs((a[0]-a2[0])*ey-(a[1]-a2[1])*ex)/l2>0.4)continue;var t0=((a2[0]-a[0])*dx+(a2[1]-a[1])*dy)/l,t1=((e2[0]-a[0])*dx+(e2[1]-a[1])*dy)/l;if(Math.min(l,Math.max(t0,t1))-Math.max(0,Math.min(t0,t1))>0.3)return true}return false}
  row.forEach(function(b){var ch=false;b.tags.forEach(function(tg,i){if(tg!=='n')return;row.forEach(function(o){if(o===b||find(row.indexOf(o))===find(row.indexOf(b)))return;if(deeper(setb.get(o),setb.get(b))&&along(b,i,o)){b.tags[i]='g';ch=true}})});
    if(ch){b.fe=freeEdges(b.polyT,b.tags);b.free=b.fe.length>0;rep.openJoints=(rep.openJoints||0)+1}})}
function L2c(p,g,j,rep){
  var f=g.f,blocks=[],z=DIRT-0.4,prev=null,c,i,k,reg=p.regularity==null?0.3:p.regularity,prevF={},parts=convexParts(g.fp),ends={},reserved=[],ncr=j.pts.length,rise=p.rise||0,span=p.span||0,rv=R(p.seed,520);
  var xl=g.xl,xr=g.xr,yb=g.yb;
  for(c=0;c<j.pts.length;c++){
    prev=c>0?(ends[c-1]||[]):null;
    var P=j.pts[c],h=j.heights[c],row=[],cur={},lipOn=!!p.lip&&p.lip[1]>0&&c>0&&(p.lipAt==='top'||p.lipAt==null?c===j.pts.length-1:(p.lipAt||[]).indexOf(c)>=0);
    /* RULE (the partition fills the polygon): a course's cells are the Voronoi cells of its sites clipped by the course's polygon, the footprint moved in by the course's setback (or out by its lip) on each outer edge: no ghost sites, no mirrors, every outer edge of a cell lies on the polygon, and its corners belong to cells. A polygon that is not convex is cut into convex parts first, and a cell is clipped by each part. Everything that gives the stone its face (facets, chips, bevels, grooves) is cut afterwards. */
    /* a course has no offset of its own: its area is the footprint. Only a lip row stands out (by its own draw); the recess is each stone's, applied after the merges (recessStones). */
    var rc=R(p.seed,210,c),off=g.edges.map(function(E){return lipOn&&!E.seam&&faceOn(p,E.face)?Math.min(draw(rc,[p.lip[0]*f,p.lip[1]*f,0]),E.maxOut):0});
    g.offs=g.offs||[];g.offs[c]=off.slice();   /* each course's offsets (only a lip's), for the front band (step 3a) */
    var resNow=reserved.filter(function(r){return r.end>=c}),sites=P.filter(function(q){return !resNow.some(function(r){return inPolyAny(r.poly,q[0],q[1])})}).map(function(q){return [q[0],q[1]]}),n=sites.length,reach=3.4*Math.max(j.sx,j.sy);
    /* a stone that rises through this row keeps its place: its site stays in the row as a ghost (it makes no cell), so the cells round it end on their bisectors with it, whole and convex, instead of being cut into wedges round its corners */
    P.forEach(function(q){if(resNow.some(function(r){return inPolyAny(r.poly,q[0],q[1])}))sites.push([q[0],q[1]])});
    /* RULE (a row is divided in the area the rows below left it): a stone that rises through this row keeps its footprint here, and the cells of the row are cut round it into convex parts */
    var CL=[];
    for(k=0;k<n;k++)for(var pq0=0;pq0<parts.length;pq0++){var C0=cellIn(sites,k,parts[pq0],off,0,reach,g.seam);if(!C0||Math.abs(polyArea(C0.pts))<1.0||minWidth2(C0.pts)<MINCELLW*f)continue;var CX0=cellIn(sites,k,parts[pq0],off,OVL,reach,g.seam)||C0;var CV=carveRising(C0,CX0,resNow);if(!CV)continue;C0=CV.C;CX0=CV.CX;if(Math.abs(polyArea(C0.pts))<1.0||minWidth2(C0.pts)<MINCELLW*f)continue;
      [{C:C0,CX:CX0}].forEach(function(q,qi){CL.push({k:k,pq:pq0*8+qi,C:q.C,CX:q.CX})})}
    for(var ci=0;ci<CL.length;ci++){k=CL[ci].k;var pq=CL[ci].pq,C=CL[ci].C,CX=CL[ci].CX;if(Math.abs(polyArea(C.pts))<1.0)continue;
      /* RULE (stand only on what holds you): a cell's footprint lies inside the footprints of the cells it rests on, on every free edge and over every hole. Where a cell would reach past them (more than 0.02 mm2), it is cut along its supports' own outlines into convex pieces, one for each support (a joint is carried up there, as the language's bond allows), so its bed is always over material and nothing hangs. The edges cut along a support's free edge are new free edges parallel to it. */
      var pieces=[{C:C,CX:CX}];
      var fg=lipOn?(p.lip[1]*f+0.3):0;
      if(prev){var ov=prev.filter(function(o){return polyOverlap(C.pts,o.polyT)>0.4||(lipOn&&clipByCell(C,o,0,fg,p)&&polyOverlap((clipByCell(C,o,0,fg,p)||{pts:[]}).pts,C.pts)>0.4)}),sa=0;
        ov.forEach(function(o){var X=clipByCell(C,o,0,fg,p);if(X)sa+=Math.abs(polyArea(X.pts))});
        if(!ov.length){rep.dropped++;rep.dNoSup=(rep.dNoSup||0)+1;continue}
        /* a lip row stands out on purpose, so it is still cut along its supports; every other cell stays one block, its Voronoi cell: what it stands on is the rock's business (a stone bears on the hull of the tops below it, step 8) */
        if(lipOn){pieces=[];ov.forEach(function(o){var pc=clipByCell(C,o,0,fg,p),px=pc?clipByCell(CX,o,OVL,fg,p):null;if(pc&&px&&Math.abs(polyArea(pc.pts))>=3&&(!lipOn||polyOverlap(pc.pts,o.polyT)>=Math.max(1.5,0.35*Math.abs(polyArea(pc.pts)))))pieces.push({C:pc,CX:px,sup:o})});rep.split=(rep.split||0)+1;
        /* RULE (a cell is merged, not deleted): when every part over a support is under 3 mm2, the largest part stays if it is a stone's size (1.2 mm2), taking its support's outline; the rest of the cell's area goes to the neighbours' own cells */
        if(!pieces.length&&!lipOn){var bestP=null,bestA=0;ov.forEach(function(o){var pc=clipByCell(C,o,0,fg,p),px=pc?clipByCell(CX,o,OVL,fg,p):null;if(pc&&px){var aa=Math.abs(polyArea(pc.pts));if(aa>bestA){bestA=aa;bestP={C:pc,CX:px,sup:o}}}});if(bestP&&bestA>=1.2){pieces.push(bestP);rep.merged=(rep.merged||0)+1}}
        if(!pieces.length){rep.dropped++;rep.dPiece=(rep.dPiece||0)+1;continue}}}
      pieces.forEach(function(pcs,pi){var C=pcs.C,CX=pcs.CX;if(minWidth2(C.pts)<MINCELLW*f){rep.dropped++;rep.dThin=(rep.dThin||0)+1;return}
      var poly=C.pts,xs=poly.map(function(q){return q[0]}),x0=Math.min.apply(null,xs),x1=Math.max.apply(null,xs);
      var fe=freeEdges(poly,C.tags),m=poly.length;
      var free=fe.length>0,rb=R(p.seed,300,c,(k*64+pq)*7+pi),hb=h*(1-p.wander)+h*(0.7+0.6*rb())*p.wander;
      row.push({cap:lipOn,supC:pcs.sup,c:c,i:(k*64+pq)*7+pi,x0:x0,x1:x1,off:0,zb:z,zt:z+hb,hb:hb,poly:CX.pts,polyT:poly,tags:C.tags,fe:fe,free:free,cx:polyCentroid(poly),CXo:CX,F:null})})}
    prevF=cur;
    if(prev){row.forEach(function(b){var hi=-1e9,ov=[];prev.forEach(function(o){if(polyOverlap(b.poly,o.poly)>0.02){ov.push(o);if(o.zt>hi)hi=o.zt}});
      if(hi>-1e8){var sup=0;/* RULE (a course is laid over the union of the tops below): every cell it overlaps is raised to the highest of them, so all of them hold it, and its support is their total, not only the tallest */
 ov.forEach(function(o){sup+=polyOverlap(b.polyT,o.polyT)});if(sup<1.5){b.dead=true;return}ov.forEach(function(o){if(o.zt<hi-1e-6){o.zt=hi;rep.raised++}});b.zb=hi;b.zt=hi+b.hb}});row=row.filter(function(b){if(b.dead){rep.dropped++;rep.dDead=(rep.dDead||0)+1}return !b.dead});
      /* RULE (a bed is the top of the highest cell under it, as the tops finally stand): raising a shared support for one cell can lift it past the bed another cell took earlier in this row, leaving that bed lower than the support's top and its groove unroofed; so beds and supports are settled together until none moves */
      var chg=true,it=0;while(chg&&it++<8){chg=false;row.forEach(function(b){var hi=-1e9,ov=[];prev.forEach(function(o){if(polyOverlap(b.poly,o.poly)>0.02){ov.push(o);if(o.zt>hi)hi=o.zt}});if(hi>-1e8&&Math.abs(b.zb-hi)>1e-6){b.zb=hi;b.zt=hi+b.hb;chg=true}ov.forEach(function(o){if(o.zt<hi-1e-6){o.zt=hi;rep.raised++;chg=true}})})}}
    row.forEach(function(b){b.zt=Math.max(b.zt,b.zb+MINSTONE*f)});
    /* a cell that would stand above its crest is dropped now, so the next course rests only on cells that stay */
    row=row.filter(function(b){if(envHeight(g,b)<MINSTONE*f-1e-6){rep.dropped++;rep.dCrest=(rep.dCrest||0)+1;return false}return true});
    /* vertical merge: a stone rolls, again and again, to carry on into the next row */
    row.forEach(function(b){var c2=c;if(!b.cap)while(c2<ncr-1&&rv()<rise)c2++;b.c2=c2;if(c2>c){var ex=0,q;for(q=c+1;q<=c2;q++)ex+=j.heights[q];b.zt+=ex;b.hb+=ex;reserved.push({poly:b.polyT,b:b,end:c2})}});
    /* horizontal merge: neighbours that start and end on the same rows join on a roll; the shared edge stops being a joint (no groove) */
    var merges=[];
    if(span>0){var ia,ib;for(ia=0;ia<row.length;ia++)for(ib=ia+1;ib<row.length;ib++){var A=row[ia],B=row[ib];if(A.c2!==B.c2||A.cap||B.cap)continue;var hit=false,ea,eb;
      for(ea=0;ea<A.polyT.length&&!hit;ea++){if(A.tags[ea]!=='n')continue;var a1=A.polyT[ea],a2=A.polyT[(ea+1)%A.polyT.length];for(eb=0;eb<B.polyT.length;eb++){if(B.tags[eb]!=='n')continue;var b1=B.polyT[eb],b2=B.polyT[(eb+1)%B.polyT.length];if(Math.hypot(a1[0]-b2[0],a1[1]-b2[1])<1e-3&&Math.hypot(a2[0]-b1[0],a2[1]-b1[1])<1e-3){hit=true;if(rv()<span){A.tags[ea]='m';B.tags[eb]='m';snapEdge(A,ea,B,eb);joinStone(A,B);A.fe=freeEdges(A.polyT,A.tags);B.fe=freeEdges(B.polyT,B.tags);A.free=A.fe.length>0;B.free=B.fe.length>0;rep.hmerged=(rep.hmerged||0)+1;merges.push([ia,ib])}break}}}}}
    sameTop(row);
    if(!lipOn)recessStones(p,g,row,merges,c,f,rep,prev);
    row.forEach(function(b){blocks.push(b);(ends[b.c2]=ends[b.c2]||[]).push(b)});z+=h}
  var kept=[];
  blocks.forEach(function(b){if(envHeight(g,b)<MINSTONE*f-1e-6){rep.dropped++;rep.dCrest2=(rep.dCrest2||0)+1;return}kept.push(b)});
  stoneKeys(kept);
  /* only a cell with nothing above it is cut to the crest: a cell under another is never lowered out from under it (that leaves a flat ceiling) */
  kept.forEach(function(b){var above=kept.some(function(o){return o.c>cE(b)&&polyOverlap(o.polyT,b.polyT)>0.2});if(!above)b.crest=true});
  return {blocks:kept}}
/* a convex prism of a polygon between two heights, as a polytope */
function prism(poly,zb,zt){var F=[],n=poly.length,i,cw=polyArea(poly)<0?poly.slice().reverse():poly;n=cw.length;
  F.push(mkFace([0,0,-1],cw.map(function(q){return [q[0],q[1],zb]}),'bed'));F.push(mkFace([0,0,1],cw.map(function(q){return [q[0],q[1],zt]}),'top'));
  for(i=0;i<n;i++){var a=cw[i],b=cw[(i+1)%n],e=[b[0]-a[0],b[1]-a[1]],l=Math.hypot(e[0],e[1]);if(l<1e-6)continue;var nn=[e[1]/l,-e[0]/l,0];
    F.push(mkFace(nn,[[a[0],a[1],zb],[b[0],b[1],zb],[b[0],b[1],zt],[a[0],a[1],zt]],'side'))}
  return {faces:F}}
/* ===== triangles for drawing: every face fan-triangulated, with its normal and overhang ===== */
function overAng(n){return n[2]<0?Math.asin(Math.min(1,-n[2]))*180/PI:0}
function trisOf(list,tag){var out=[],i,j,k,F,pts;
  for(i=0;i<list.length;i++){var P=list[i];if(P.mesh){meshTris(P,tag||'stone').forEach(function(q){out.push(q)});continue}for(j=0;j<P.faces.length;j++){F=P.faces[j];pts=F.pts;
    for(k=1;k<pts.length-1;k++)out.push({p:[pts[0],pts[k],pts[k+1]],n:F.n,tag:tag||(P.core?'core':P.dirt?'dirt':'stone'),kind:F.kind,id:(P.b?P.b.c*131+P.b.i*7:i),ang:overAng(F.n)})}}
  return out}
