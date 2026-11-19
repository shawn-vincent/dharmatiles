/* Wall generator aimed at reference O. Units mm; x right, y depth (back = large y), z up. */
(function(G){
function rng(seed){var a=seed>>>0;return function(){a|=0;a=a+0x6D2B79F5|0;var t=Math.imul(a^a>>>15,1|a);t=t+Math.imul(t^t>>>7,61|t)^t;return((t^t>>>14)>>>0)/4294967296}}
var norm=function(v){var l=Math.hypot(v[0],v[1],v[2])||1;return[v[0]/l,v[1]/l,v[2]/l]};
var dot=function(a,b){return a[0]*b[0]+a[1]*b[1]+a[2]*b[2]};
var cross=function(a,b){return[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]};
/* a convex solid is a list of half-spaces n.p<=d; its faces are found by clipping a huge quad on each plane by all the others */
function faces(planes){
  var out=[];
  planes.forEach(function(P,i){
    var n=P.n,t=Math.abs(n[2])<0.9?[0,0,1]:[1,0,0],u=norm(cross(n,t)),v=cross(n,u),c=[n[0]*P.d,n[1]*P.d,n[2]*P.d],B=400;
    var poly=[[-1,-1],[1,-1],[1,1],[-1,1]].map(function(s){return[c[0]+u[0]*s[0]*B+v[0]*s[1]*B,c[1]+u[1]*s[0]*B+v[1]*s[1]*B,c[2]+u[2]*s[0]*B+v[2]*s[1]*B]});
    for(var j=0;j<planes.length&&poly.length>=3;j++){if(j===i)continue;
      var Q=planes[j],r=[];
      for(var k=0;k<poly.length;k++){var a=poly[k],b=poly[(k+1)%poly.length],da=dot(Q.n,a)-Q.d,db=dot(Q.n,b)-Q.d;
        if(da<=1e-7)r.push(a);
        if((da<-1e-7&&db>1e-7)||(da>1e-7&&db<-1e-7)){var s=da/(da-db);r.push([a[0]+(b[0]-a[0])*s,a[1]+(b[1]-a[1])*s,a[2]+(b[2]-a[2])*s])}}
      poly=r}
    if(poly.length>=3){var area=0;for(var m=1;m<poly.length-1;m++){var w=cross([poly[m][0]-poly[0][0],poly[m][1]-poly[0][1],poly[m][2]-poly[0][2]],[poly[m+1][0]-poly[0][0],poly[m+1][1]-poly[0][1],poly[m+1][2]-poly[0][2]]);area+=Math.hypot(w[0],w[1],w[2])/2}
      if(area>0.05)out.push({poly:poly,n:n,i:i,cut:!!P.cut,area:area})}
  });
  return out}
function boxPlanes(b){return[{n:[1,0,0],d:b.x1},{n:[-1,0,0],d:-b.x0},{n:[0,1,0],d:b.y1},{n:[0,-1,0],d:-b.y0},{n:[0,0,1],d:b.z1},{n:[0,0,-1],d:-b.z0}]}
/* a cut: keep n.(p-q)<=0 where q is on the plane */
function cut(n,q,tag){n=norm(n);return{n:n,d:dot(n,q),cut:tag||1}}


/* HEW: relief facets. A face polygon is filled with jittered points (boundary kept flat so the block's edges stay square), the interior points are pushed along the normal, and the points are joined by a Delaunay triangulation. Every triangle is a flat facet. */
function delaunay(P){var n=P.length,S=[],i,j,k;
  var mnx=1e9,mny=1e9,mxx=-1e9,mxy=-1e9;P.forEach(function(p){mnx=Math.min(mnx,p[0]);mxx=Math.max(mxx,p[0]);mny=Math.min(mny,p[1]);mxy=Math.max(mxy,p[1])});
  var d=Math.max(mxx-mnx,mxy-mny)*20+10,mx=(mnx+mxx)/2,my=(mny+mxy)/2;
  var Q=P.concat([[mx-d,my-d],[mx+d,my-d],[mx,my+d]]);
  var T=[[n,n+1,n+2]];
  function cc(t){var a=Q[t[0]],b=Q[t[1]],c=Q[t[2]],D=2*(a[0]*(b[1]-c[1])+b[0]*(c[1]-a[1])+c[0]*(a[1]-b[1]));if(Math.abs(D)<1e-12)return null;
    var ux=((a[0]*a[0]+a[1]*a[1])*(b[1]-c[1])+(b[0]*b[0]+b[1]*b[1])*(c[1]-a[1])+(c[0]*c[0]+c[1]*c[1])*(a[1]-b[1]))/D,uy=((a[0]*a[0]+a[1]*a[1])*(c[0]-b[0])+(b[0]*b[0]+b[1]*b[1])*(a[0]-c[0])+(c[0]*c[0]+c[1]*c[1])*(b[0]-a[0]))/D;return[ux,uy,(a[0]-ux)*(a[0]-ux)+(a[1]-uy)*(a[1]-uy)]}
  for(i=0;i<n;i++){var p=Q[i],bad=[],keep=[];
    T.forEach(function(t){var c=cc(t);if(c&&(p[0]-c[0])*(p[0]-c[0])+(p[1]-c[1])*(p[1]-c[1])<c[2]-1e-9)bad.push(t);else keep.push(t)});
    var E={};bad.forEach(function(t){for(k=0;k<3;k++){var a=t[k],b=t[(k+1)%3],key=a<b?a+'_'+b:b+'_'+a;E[key]=(E[key]||[]).concat([[a,b]])}});
    Object.keys(E).forEach(function(key){if(E[key].length===1)keep.push([E[key][0][0],E[key][0][1],i])});T=keep}
  return T.filter(function(t){return t[0]<n&&t[1]<n&&t[2]<n})}
function hew(f,R,amp){
  var p=f.poly,n=f.n,t=Math.abs(n[2])<.9?[0,0,1]:[1,0,0],u=norm(cross(n,t)),v=cross(n,u),o=p[0];
  var to2=function(q){var d=[q[0]-o[0],q[1]-o[1],q[2]-o[2]];return[dot(d,u),dot(d,v)]};
  var P2=[],P3=[],disp=[],i;
  /* boundary, subdivided every ~4 mm, flat */
  for(i=0;i<p.length;i++){var a=p[i],b=p[(i+1)%p.length],L=Math.hypot(b[0]-a[0],b[1]-a[1],b[2]-a[2]),k=Math.max(1,Math.round(L/4));
    for(var j=0;j<k;j++){var q=[a[0]+(b[0]-a[0])*j/k,a[1]+(b[1]-a[1])*j/k,a[2]+(b[2]-a[2])*j/k];P3.push(q);P2.push(to2(q));disp.push(0)}}
  var poly2=p.map(to2),mnx=1e9,mny=1e9,mxx=-1e9,mxy=-1e9;poly2.forEach(function(q){mnx=Math.min(mnx,q[0]);mxx=Math.max(mxx,q[0]);mny=Math.min(mny,q[1]);mxy=Math.max(mxy,q[1])});
  /* orientation of the polygon in 2D, to test "inside with margin" */
  var area=0;for(i=0;i<poly2.length;i++){var A=poly2[i],B=poly2[(i+1)%poly2.length];area+=A[0]*B[1]-B[0]*A[1]}var sg=area>0?1:-1;
  function inside(q,m){for(var i=0;i<poly2.length;i++){var A=poly2[i],B=poly2[(i+1)%poly2.length],ex=B[0]-A[0],ey=B[1]-A[1],l=Math.hypot(ex,ey)||1,dd=sg*(ex*(q[1]-A[1])-ey*(q[0]-A[0]))/l;if(dd<m)return false}return true}
  /* FRACTURE: 3 planes across the face, each leaning by 0.12-0.3, combined by min (a crest) or max (a trough): the big planar facets and sharp cracks. */
  var fr=[],mode=R()<.5?1:-1,np=2+Math.floor(R()*2);
  for(i=0;i<np;i++){var an=R()*6.283,sl=.12+.18*R();fr.push({cx:mnx+(mxx-mnx)*R(),cy:mny+(mxy-mny)*R(),dx:Math.cos(an)*sl,dy:Math.sin(an)*sl,off:(R()-.5)*2.2})}
  function edgeDist(q){var m=1e9;for(var i=0;i<poly2.length;i++){var A=poly2[i],B=poly2[(i+1)%poly2.length],ex=B[0]-A[0],ey=B[1]-A[1],l=Math.hypot(ex,ey)||1,dd=Math.abs(ex*(q[1]-A[1])-ey*(q[0]-A[0]))/l;if(dd<m)m=dd}return m}
  function fracture(q){var best=null;fr.forEach(function(F){var v=F.dx*(q[0]-F.cx)+F.dy*(q[1]-F.cy)+F.off;if(best===null||(mode>0?v<best:v>best))best=v});return Math.max(-2.2,Math.min(2.2,best))}
  var sp=3.5;
  for(var x=mnx+sp/2;x<mxx;x+=sp)for(var y=mny+sp/2;y<mxy;y+=sp){var q=[x+(R()-.5)*sp*.8,y+(R()-.5)*sp*.8];if(!inside(q,1.2))continue;
    var h=(fracture(q)*(f.cut?.55:1)+(R()-.5)*amp*.7)*Math.min(1,edgeDist(q)/3);P2.push(q);P3.push([o[0]+u[0]*q[0]+v[0]*q[1]+n[0]*h,o[1]+u[1]*q[0]+v[1]*q[1]+n[1]*h,o[2]+u[2]*q[0]+v[2]*q[1]+n[2]*h]);disp.push(h)}
  if(P2.length<4)return null;
  return delaunay(P2).map(function(tr){return[P3[tr[0]],P3[tr[1]],P3[tr[2]]]})}

var STAGES=['mass','courses','joints','depth','cleave','chips','hew'];
G.STAGES=STAGES;
/* build the wall up to stage `upto` (1..6). Every stage works on the blocks of the one before. */
G.build=function(seed,upto){
  var R=rng(seed*7919+13),ur=function(a,b){return a+(b-a)*R()},pick=function(a){return a[Math.floor(R()*a.length)]};
  var W=35,YB=31,YF=14;               /* wall x 0..35, back at y=31, widest front at y=14 */
  /* 1 MASS: three big boxes, stepped in height (the skyline) and in depth */
  var H=[ur(38,42),ur(34,38),ur(37,41)],X=[0,ur(13,15),ur(24,26),W],F=[ur(YF-1,YF),ur(YF-.5,YF+1.5),ur(YF-3,YF-1)];
  var masses=[0,1,2].map(function(i){return{x0:X[i],x1:X[i+1],y0:F[i],y1:YB,z0:0,z1:H[i],m:i}});
  var blocks=masses.map(function(m){return{x0:m.x0,x1:m.x1,y0:m.y0,y1:m.y1,z0:m.z0,z1:m.z1,m:m.m,row:0,planes:[]}});
  if(upto<2)return fin(blocks,upto);
  /* 2 COURSES: each mass is cut by horizontal beds. Course heights are big (12-19 mm) and not shared between masses. */
  var rows=[];
  blocks.forEach(function(b){var z=0,k=0;
    while(z<b.z1-1){var h=ur(11.5,14.5);if(b.z1-(z+h)<6.5)h=b.z1-z;rows.push({x0:b.x0,x1:b.x1,y0:b.y0,y1:b.y1,z0:z,z1:z+h,m:b.m,row:k,planes:[]});z+=h;k++}});
  blocks=rows;
  if(upto<3)return fin(blocks,upto);
  /* 3 JOINTS: vertical joints split each course into blocks 9-17 mm wide. A joint may run on through the course above (same x), or be staggered. */
  var byMass={};blocks.forEach(function(b){(byMass[b.m]=byMass[b.m]||[]).push(b)});
  var nb=[];
  Object.keys(byMass).forEach(function(k){
    var list=byMass[k].sort(function(a,b){return a.row-b.row}),prev=[];
    list.forEach(function(b){
      var w=b.x1-b.x0,js=[];
      if(w>15){var cnt=Math.max(1,Math.round(w/14)-1);
        for(var i=0;i<cnt;i++){var x=b.x0+w*(i+1)/(cnt+1)+ur(-w*.18,w*.18);
          if(prev.length&&R()<.35){var pj=pick(prev);if(pj>b.x0+4&&pj<b.x1-4)x=pj}  /* through-joint */
          else{prev.forEach(function(pj){if(Math.abs(pj-x)<3.2)x=pj+(x>pj?3.2:-3.2)})}   /* otherwise stagger */
          js.push(x)}}
      js.sort(function(a,b){return a-b});
      var xs=[b.x0].concat(js,[b.x1]);
      for(var i=0;i<xs.length-1;i++)nb.push({x0:xs[i],x1:xs[i+1],y0:b.y0,y1:b.y1,z0:b.z0,z1:b.z1,m:b.m,row:b.row,planes:[]});
      prev=js})});
  blocks=nb;
  if(upto<4)return fin(blocks,upto);
  /* 4 DEPTH: each block's face is pushed back a different amount; tops and beds jitter, so the face steps in and out. A block on top of another never leans past it. */
  blocks.forEach(function(b){
    var step=pick([0,0,1,2,3.5]);
    if(b.m===0&&b.row===1)step=ur(6,8);          /* the deep recess in the middle course ... */
    if(b.m===0&&b.row>=2)step=ur(-3,-1.5);        /* ... under the block that overhangs it */
    if(b.m===1&&b.row===0)step=ur(2,3.5);
    b.y0=Math.min(b.y0+step,b.y1-7);
    if(b.row>0)b.z0-=0; var top=b.z1;
    b.z1=top+(b.z1>=H[b.m]-0.1?ur(-4.5,1.5):ur(-0.6,0.6));   /* the skyline blocks differ in height; beds stay level */
  });
  /* broken skyline: small cap blocks sit on the top course of each mass */
  blocks.slice().forEach(function(b){if(b.z1>=H[b.m]-6&&b.x1-b.x0>8&&R()<.6){var cw=ur(6,Math.min(10,b.x1-b.x0-1)),cx=ur(b.x0+.5,b.x1-cw-.5),ch=ur(3.5,6.5),cy=b.y0+ur(1,4);
    blocks.push({x0:cx,x1:cx+cw,y0:cy,y1:Math.min(b.y1,cy+ur(9,12)),z0:b.z1-.2,z1:b.z1+ch,m:b.m,row:b.row+1,planes:[]})}});
  /* FOOT: a few small blocks of the same rock lie loose at the base, turned about z */
  if(upto>=4){for(var fi=0,nf0=7;fi<nf0;fi++){var fw=ur(3.5,7),fd=ur(3.5,6.5),fh=ur(2.5,5),fx=ur(2,W-fw-1),fy=ur(5,12);
    blocks.push({x0:fx,x1:fx+fw,y0:fy,y1:fy+fd,z0:0,z1:fh,m:3,row:0,rot:ur(-.7,.7),planes:[]})}}
  if(upto<5)return fin(blocks,upto);
  /* 5 CLEAVE: planar fractures. The front face is broken by 1-2 planes tilted a few degrees: the crack is the line where facets meet. Tops tilt slightly; side faces are cracked too. */
  blocks.forEach(function(b){
    var cx=(b.x0+b.x1)/2,cz=(b.z0+b.z1)/2,w=b.x1-b.x0,h=b.z1-b.z0,nf=R()<.5?2:1;
    for(var i=0;i<nf;i++){
      var tx=ur(-.2,.2),tz=ur(-.2,.2),dep=ur(.3,2.4);
      var q=[b.x0+ur(.2,.8)*w,b.y0+dep,b.z0+ur(.2,.8)*h];
      b.planes.push(cut([tx,-1,tz],q));
    }
    if(R()<.7)b.planes.push(cut([ur(-.12,.12),ur(-.14,.1),1],[cx,(b.y0+b.y1)/2,b.z1-ur(.2,1.6)]));  /* tilted top */
  });
  if(upto<6)return fin(blocks,upto);
  /* 6 CHIPS: corner and edge bites. A bite is a plane through a corner or edge whose normal points out along the diagonal; depth 1.5-4 mm, tops and fronts mostly. */
  blocks.forEach(function(b){
    var n=Math.floor(ur(1,4));
    for(var i=0;i<n;i++){
      var sx=R()<.5?-1:1,top=R()<.72,sz=top?1:-1;
      var corner=[sx<0?b.x0:b.x1,b.y0,top?b.z1:b.z0+ur(1,(b.z1-b.z0)*.6)];
      var kind=R();
      var nn,dep=ur(1.2,3.4);
      if(kind<.4)nn=[sx*ur(.6,1),-ur(.5,1),top?ur(.5,1.2):ur(-.2,.5)];        /* corner bite */
      else if(kind<.75)nn=[ur(-.3,.3),-ur(.6,1),top?1:ur(.1,.5)];             /* edge bite along top-front */
      else nn=[sx,ur(-.3,.2),top?ur(.4,1):ur(0,.5)];                           /* side/top edge */
      nn=norm(nn);
      b.planes.push(cut(nn,[corner[0]-nn[0]*dep,corner[1]-nn[1]*dep,corner[2]-nn[2]*dep],2));
    }
  });
  /* 6b BEVELS: the big breaks along edges. A chamfer plane through the top-front edge (depth 1.6-3.4), often one through a top side edge and a vertical edge; the foot blocks get them too. */
  blocks.forEach(function(b){
    var d=b.z1-b.z0,pw=b.x1-b.x0;
    if(R()<.85)b.planes.push(cut([ur(-.15,.15),-1,ur(.9,1.5)],[(b.x0+b.x1)/2,b.y0+ur(0,.4),b.z1-ur(1.4,3.4)],3));
    if(R()<.5){var sx=R()<.5?-1:1;b.planes.push(cut([sx*ur(.8,1.2),ur(-.2,.2),ur(.7,1.3)],[sx<0?b.x0+ur(1.2,3):b.x1-ur(1.2,3),(b.y0+b.y1)/2,b.z1],3))}
    if(R()<.45){var sx2=R()<.5?-1:1;b.planes.push(cut([sx2*ur(.9,1.3),-1,ur(-.1,.25)],[sx2<0?b.x0:b.x1,b.y0+ur(1.2,3),(b.z0+b.z1)/2],3))}
    if(R()<.3)b.planes.push(cut([ur(-.4,.4),-1,-ur(.6,1.2)],[(b.x0+b.x1)/2,b.y0+ur(0,.5),b.z0+ur(1.2,3)],3));
  });
  if(upto<7)return fin(blocks,upto);
  return fin(blocks,upto);
  function fin(bl,up){
    var out=[];
    bl.forEach(function(b,i){
      /* a hair of shrink so the joint reads as a dark line against the core */
      var g=up>=3?.3:0,bb={x0:b.x0+g,x1:b.x1-g,y0:b.y0,y1:b.y1,z0:b.z0+(up>=3?.25:0),z1:b.z1};
      var pl=boxPlanes(bb).concat(b.planes);
      if(b.rot){var c0=[(bb.x0+bb.x1)/2,(bb.y0+bb.y1)/2,0],cs=Math.cos(b.rot),sn=Math.sin(b.rot);
        pl=pl.map(function(P){var n2=[P.n[0]*cs-P.n[1]*sn,P.n[0]*sn+P.n[1]*cs,P.n[2]];return{n:n2,d:P.d-dot(P.n,c0)+dot(n2,c0),cut:P.cut}})}
      var fc=faces(pl);if(up>=7){var Rh=rng(seed*101+i*7+3);fc.forEach(function(f){f.tris=hew(f,Rh,f.cut?.8:1.1)})}
      out.push({id:i,m:b.m,row:b.row,box:bb,faces:fc})});
    return{blocks:out,seed:seed,upto:up,H:H,cores:[[X[0],X[1],H[0]-3],[X[1],X[2],H[1]-3],[X[2],W,H[2]-3]]}}
};
G.rng=rng;
})(typeof window!=='undefined'?(window.GEN=window.GEN||{}):module.exports);
