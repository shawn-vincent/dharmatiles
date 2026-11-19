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
      if(area>0.05)out.push({poly:poly,n:n,i:i,cut:P.cut||0,area:area})}
  });
  return out}
function boxPlanes(b){return[{n:[1,0,0],d:b.x1},{n:[-1,0,0],d:-b.x0},{n:[0,1,0],d:b.y1},{n:[0,-1,0],d:-b.y0},{n:[0,0,1],d:b.z1},{n:[0,0,-1],d:-b.z0}]}
/* a cut: keep n.(p-q)<=0 where q is on the plane */
function cut(n,q,tag){n=norm(n);return{n:n,d:dot(n,q),cut:tag||1}}


/* HEW (v9): the face is cleaved again and again. A face polygon is split by a straight crack into two pieces; the far piece is a copy of the near one's plane, leaned by a crease and stepped by an offset. Each piece is split again at a smaller scale with a smaller offset. A crack may die inside the face: the offset covers only part of its run, ended by a riser, so beyond it the two pieces are one surface. Leaves are flat convex polygons; risers are drawn where neighbours differ in height. */
function clipPoly(poly,n,d){var r=[];for(var k=0;k<poly.length;k++){var a=poly[k],b=poly[(k+1)%poly.length],da=n[0]*a[0]+n[1]*a[1]-d,db=n[0]*b[0]+n[1]*b[1]-d;
  if(da<=1e-9)r.push(a);
  if((da<-1e-9&&db>1e-9)||(da>1e-9&&db<-1e-9)){var s=da/(da-db);r.push([a[0]+(b[0]-a[0])*s,a[1]+(b[1]-a[1])*s])}}return r}
function polyArea(p){var s=0;for(var i=0;i<p.length;i++){var a=p[i],b=p[(i+1)%p.length];s+=a[0]*b[1]-b[0]*a[1]}return Math.abs(s)/2}
function minW(P){var m=1e9;for(var i=0;i<P.length;i++){var a=P[i],b=P[(i+1)%P.length],ex=b[0]-a[0],ey=b[1]-a[1],l=Math.hypot(ex,ey)||1,mx=0;for(var j=0;j<P.length;j++){var d=Math.abs(ex*(P[j][1]-a[1])-ey*(P[j][0]-a[0]))/l;if(d>mx)mx=d}if(mx<m)m=mx}return m}
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
    var h=(fracture(q)*(f.cut?.12:.2)+(R()-.5)*amp*.7)*Math.min(1,edgeDist(q)/3);P2.push(q);P3.push([o[0]+u[0]*q[0]+v[0]*q[1]+n[0]*h,o[1]+u[1]*q[0]+v[1]*q[1]+n[1]*h,o[2]+u[2]*q[0]+v[2]*q[1]+n[2]*h]);disp.push(h)}
  if(P2.length<4)return null;
  return delaunay(P2).map(function(tr){return[P3[tr[0]],P3[tr[1]],P3[tr[2]]]})}


/* GROOVES (v14): a flat face is scored with thin V cracks. Cracks are polylines (near-vertical with wobble, one branch maybe); the face is a coarse grid, and only cells near a crack are subdivided finely, so the rest stays flat. Depth is a V of the distance to the nearest crack. */
function segDist(px,py,s){var dx=s[2]-s[0],dy=s[3]-s[1],l2=dx*dx+dy*dy||1,t=Math.max(0,Math.min(1,((px-s[0])*dx+(py-s[1])*dy)/l2));return Math.hypot(px-s[0]-dx*t,py-s[1]-dy*t)}
function grooves(f,R,opt){
  var p=f.poly,n=f.n,t=Math.abs(n[2])<.9?[0,0,1]:[1,0,0],u=norm(cross(n,t)),v=cross(n,u),o=p[0],i;
  var to2=function(q){var d=[q[0]-o[0],q[1]-o[1],q[2]-o[2]];return[dot(d,u),dot(d,v)]};
  var P=p.map(to2),mnx=1e9,mny=1e9,mxx=-1e9,mxy=-1e9;P.forEach(function(q){mnx=Math.min(mnx,q[0]);mxx=Math.max(mxx,q[0]);mny=Math.min(mny,q[1]);mxy=Math.max(mxy,q[1])});
  var area=polyArea(P),W=mxx-mnx,Hh=mxy-mny,top=n[2]>.9,nc=Math.min(5,Math.round(area/(top?45:30))+1);
  if(nc<1)return null;
  var segs=[],hw=opt.hw*(.8+.4*R()),D=opt.depth;
  for(i=0;i<nc;i++){
    var sx=mnx+W*(.12+.76*R()),sy=mny+Hh*(top?R():R()*.25),a=top?R()*3.14:ur_(R,-.22,.22),len=Hh*ur_(R,.6,1)*(top?.5:1),x=sx,y=sy,k=0,steps=Math.max(2,Math.round(len/1.6)),pts=[[x,y]];
    for(k=0;k<steps;k++){a+=ur_(R,-.14,.14);x+=Math.sin(a)*len/steps;y+=Math.cos(a)*len/steps;pts.push([x,y])}
    for(k=0;k<pts.length-1;k++)segs.push([pts[k][0],pts[k][1],pts[k+1][0],pts[k+1][1],1-.35*k/(pts.length-1)]);
    if(R()<.5&&pts.length>3){var bi=1+Math.floor(R()*(pts.length-2)),bx=pts[bi][0],by=pts[bi][1],ba=a+(R()<.5?-1:1)*ur_(R,.5,.9),bl=ur_(R,3,6);segs.push([bx,by,bx+Math.sin(ba)*bl,by+Math.cos(ba)*bl,.6])}
  }
  function edgeD(q){var m=1e9;for(var j=0;j<P.length;j++){var A=P[j],B=P[(j+1)%P.length];m=Math.min(m,segDist(q[0],q[1],[A[0],A[1],B[0],B[1]]))}return m}
  function depth(q){var best=0;for(var j=0;j<segs.length;j++){var s2=segs[j],h=hw*s2[4],d=segDist(q[0],q[1],s2);if(d<h){var dd=D*s2[4]*(1-d/h);if(dd>best)best=dd}}return best*Math.min(1,edgeD(q)/1.0)}
  var area2=0;for(i=0;i<P.length;i++){var A=P[i],B=P[(i+1)%P.length];area2+=A[0]*B[1]-B[0]*A[1]}
  var sg=area2>0?1:-1,tris=[];
  function clipCell(x0,y0,x1,y1){var c=P.slice();if(sg<0)c=c.slice().reverse();c=clipPoly2(c,[1,0],x1);c=clipPoly2(c,[-1,0],-x0);c=clipPoly2(c,[0,1],y1);c=clipPoly2(c,[0,-1],-y0);return c}
  function emit(c){if(c.length<3||polyArea(c)<1e-4)return;var ds=[],pts=c.map(function(q){var d=depth(q);ds.push(d);return[o[0]+u[0]*q[0]+v[0]*q[1]-n[0]*d,o[1]+u[1]*q[0]+v[1]*q[1]-n[1]*d,o[2]+u[2]*q[0]+v[2]*q[1]-n[2]*d]});for(var j=1;j<pts.length-1;j++){var T=[pts[0],pts[j],pts[j+1]],dm=(ds[0]+ds[j]+ds[j+1])/3;T.k=1-.75*Math.min(1,dm/D);tris.push(T)}}
  var CS=1,FS=.2,reach=hw+.6;
  for(var x=mnx;x<mxx;x+=CS)for(var y=mny;y<mxy;y+=CS){
    var c=clipCell(x,y,x+CS,y+CS);if(c.length<3)continue;
    var near=false,cx=x+CS/2,cy=y+CS/2;for(i=0;i<segs.length;i++)if(segDist(cx,cy,segs[i])<reach){near=true;break}
    if(!near){emit(c);continue}
    for(var x2=x;x2<x+CS-1e-6;x2+=FS)for(var y2=y;y2<y+CS-1e-6;y2+=FS)emit(clipCell(x2,y2,x2+FS,y2+FS))}
  return tris.length?tris:null}
function ur_(R,a,b){return a+(b-a)*R()}
function clipPoly2(poly,n,d){return clipPoly(poly,n,d)}
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
    while(z<b.z1-1){var h=ur(11.5,14.5),sm=[];if(!(b.m===0&&k<3)&&k>0&&R()<0&&b.z1-(z+2*h)>=6.5){sm.push({z:z+h});h*=2}if(b.z1-(z+h)<6.5)h=b.z1-z;rows.push({x0:b.x0,x1:b.x1,y0:b.y0,y1:b.y1,z0:z,z1:z+h,m:b.m,row:k,planes:[],seams:sm});z+=h;k++}});
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
      var blind=js.map(function(){return R()<0}),keepJ=js.filter(function(x,i){return!blind[i]}),bj=js.filter(function(x,i){return blind[i]});
      var xs=[b.x0].concat(keepJ,[b.x1]);
      for(var i=0;i<xs.length-1;i++)nb.push({x0:xs[i],x1:xs[i+1],y0:b.y0,y1:b.y1,z0:b.z0,z1:b.z1,m:b.m,row:b.row,planes:[],seams:(b.seams||[]).filter(function(s){return 1}).concat(bj.filter(function(x){return x>xs[i]&&x<xs[i+1]}).map(function(x){return{x:x}}))});
      prev=keepJ})});
  blocks=nb;
  if(upto<4)return fin(blocks,upto);
  /* 4 DEPTH: each block's face is pushed back a different amount; tops and beds jitter, so the face steps in and out. A block on top of another never leans past it. */
  blocks.forEach(function(b){
    var step=pick([0,0,1,2,3.5]);
    if(b.m===0&&b.row===1)step=ur(6,8);          /* the deep recess in the middle course ... */
    if(b.m===0&&b.row>=2)step=ur(-3,-1.5);
    if(b.m>=1&&b.row>=1)step+=ur(-1.2,1.2);   /* each course sits a little proud of or behind the one below */
    if(b.m>=1&&b.row>=2)step-=1;        /* ... under the block that overhangs it */
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
  /* 6 CHIPS: done per vertex in fin(): a plane at each corner along its averaged normal, tilted up to 25 degrees */
  /* 6b BEVELS: the big breaks along edges. A chamfer plane through the top-front edge (depth 1.6-3.4), often one through a top side edge and a vertical edge; the foot blocks get them too. */
  blocks.forEach(function(b){
    var d=b.z1-b.z0,pw=b.x1-b.x0;
    /* eased edges: a chamfer of 1-2 dmm on the top-front edge, both top side edges and (mostly) both vertical front edges */
    b.planes.push(cut([ur(-.08,.08),-1,ur(.92,1.1)],[(b.x0+b.x1)/2,b.y0,b.z1-ur(1,2)],3));
    [-1,1].forEach(function(sx){
      b.planes.push(cut([sx*ur(.9,1.1),ur(-.1,.1),1],[sx<0?b.x0+ur(1,1.9):b.x1-ur(1,1.9),(b.y0+b.y1)/2,b.z1],3));
      if(R()<.8)b.planes.push(cut([sx*1,-ur(.9,1.1),ur(-.05,.05)],[sx<0?b.x0:b.x1,b.y0+ur(1,1.9),(b.z0+b.z1)/2],3));
    });
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
      var fc=faces(pl);
      function chipsOf(fcs,Rc,cap,pr,dmin,dmax){var vs={};fcs.forEach(function(f){f.poly.forEach(function(q){var k=q.map(function(x){return Math.round(x*50)}).join(',');var e=vs[k]||(vs[k]={p:q,n:[0,0,0],c:0});e.n[0]+=f.n[0];e.n[1]+=f.n[1];e.n[2]+=f.n[2];e.c++})});
        var added=[];
        Object.keys(vs).sort(function(){return Rc()-.5}).forEach(function(k){if(added.length>=cap)return;var e=vs[k];if(e.c<3||Rc()>pr||e.p[2]<.6)return;
          var nn=norm(e.n),tl=Math.tan(25*Math.PI/180)*Rc(),ang=Rc()*6.283,tt=Math.abs(nn[2])<.9?[0,0,1]:[1,0,0],uu=norm(cross(nn,tt)),vv=cross(nn,uu);
          nn=norm([nn[0]+(uu[0]*Math.cos(ang)+vv[0]*Math.sin(ang))*tl,nn[1]+(uu[1]*Math.cos(ang)+vv[1]*Math.sin(ang))*tl,nn[2]+(uu[2]*Math.cos(ang)+vv[2]*Math.sin(ang))*tl]);if(nn[2]<-.5)return;
          var dep=dmin+(dmax-dmin)*Rc();added.push(cut(nn,[e.p[0]-nn[0]*dep,e.p[1]-nn[1]*dep,e.p[2]-nn[2]*dep],2))});
        return added}
      if(up<7||b.rot){
        if(up>=6){var Rc0=rng(seed*53+i*11+9),ad=chipsOf(fc,Rc0,3+Math.floor(Rc0()*5),.55,.8,2.3);if(ad.length){pl=pl.concat(ad);fc=faces(pl)}}
        out.push({id:i,m:b.m,row:b.row,box:bb,faces:fc});return}
      /* SKIN: the outer few mm of the block (front and top) is cleaved into convex solid pieces; each is set back and turned a little; the core stays whole behind. */
      var Rs=rng(seed*131+i*17+5),ur2=function(a,b2){return a+(b2-a)*Rs()},base=pl.filter(function(P,ix){return ix!==2&&ix!==3}),extra=pl.slice(6);
      var T=ur2(3,4.2),T2=ur2(2,3);
      function cells(x0,x1,y0,y1,nmax){
        var cs=[[[x0,y0],[x1,y0],[x1,y1],[x0,y1]]],ns=nmax>0?1+Math.floor(Rs()*nmax):0,tries=0;
        while(ns>0&&tries<30){tries++;cs.sort(function(a,b2){return polyArea(b2)-polyArea(a)});var P=cs[0],cx=0,cy=0;P.forEach(function(q){cx+=q[0];cy+=q[1]});cx/=P.length;cy/=P.length;
          var th=Rs()<.6?Math.PI/2+ur2(-.12,.12):ur2(-.1,.1),n2=[-Math.sin(th),Math.cos(th)],rad=Math.sqrt(polyArea(P))*.25,px=cx+ur2(-rad,rad),py=cy+ur2(-rad,rad),d=n2[0]*px+n2[1]*py;
          var pa=clipPoly(P,n2,d),pb=clipPoly(P,[-n2[0],-n2[1]],-d);if(pa.length<3||pb.length<3)continue;var w0=minW(P);if(minW(pa)<.4*w0||minW(pb)<.4*w0)continue;
          cs.shift();cs.push(pa,pb);ns--}
        return cs}
      var rect=function(P,x0,x1,y0,y1){return P.every(function(q){return(Math.abs(q[0]-x0)<1e-6||Math.abs(q[0]-x1)<1e-6)&&false})};
      function pieceSolids(cs,axis,depthPl,setN,backPl,x0,x1,y0,y1){
        var res=[];
        cs.forEach(function(P){
          var area=0;for(var k=0;k<P.length;k++){var A=P[k],B=P[(k+1)%P.length];area+=A[0]*B[1]-B[0]*A[1]}if(area<0)P=P.slice().reverse();
          var pp=base.filter(function(P,ix){return axis==='y'||ix!==2}),cx=0,cy=0;P.forEach(function(q){cx+=q[0];cy+=q[1]});cx/=P.length;cy/=P.length;
          for(var k=0;k<P.length;k++){var A=P[k],B=P[(k+1)%P.length],ex=B[0]-A[0],ey=B[1]-A[1],l=Math.hypot(ex,ey)||1,nx=ey/l,ny=-ex/l,onB=(Math.abs(A[0]-x0)<1e-6&&Math.abs(B[0]-x0)<1e-6)||(Math.abs(A[0]-x1)<1e-6&&Math.abs(B[0]-x1)<1e-6)||(Math.abs(A[1]-y0)<1e-6&&Math.abs(B[1]-y0)<1e-6)||(Math.abs(A[1]-y1)<1e-6&&Math.abs(B[1]-y1)<1e-6);
            if(onB)continue;var d=nx*A[0]+ny*A[1]-.1;
            pp.push(axis==='y'?{n:[nx,0,ny],d:d}:{n:[nx,ny,0],d:d})}
          var sb=Rs()<.4?0:ur2(.1,.3),tl=Math.tan(ur2(.2,1.2)*Math.PI/180),ph=ur2(0,6.283);
          if(axis==='y'){var nf=norm([tl*Math.cos(ph),-1,tl*Math.sin(ph)]);pp.push({n:nf,d:dot(nf,[cx,bb.y0+sb,cy])});pp.push({n:[0,1,0],d:bb.y0+T});}
          else{var nf2=norm([tl*Math.cos(ph),tl*Math.sin(ph),1]);pp.push({n:nf2,d:dot(nf2,[cx,cy,bb.z1-sb])});pp.push({n:[0,0,-1],d:-(bb.z1-T2)});pp.push({n:[0,-1,0],d:-(bb.y0+T)});pp.push({n:[0,1,0],d:bb.y1})}
          pp=pp.concat(extra);
          var f2=faces(pp);if(!f2.length)return;
          var ad=chipsOf(f2,Rs,1,.45,.5,1.1);if(ad.length){pp=pp.concat(ad);f2=faces(pp)}
          res.push({id:i,m:b.m,row:b.row,box:bb,faces:f2,k:.92+.22*Rs()})})
        return res}
      var fcells=cells(bb.x0,bb.x1,bb.z0,bb.z1,(Rs()<.6?0:1)),
          tcells=cells(bb.x0,bb.x1,bb.y0+T,bb.y1,(Rs()<.7?0:1));
      pieceSolids(fcells,'y',0,0,0,bb.x0,bb.x1,bb.z0,bb.z1).forEach(function(o){out.push(o)});
      if(bb.y1-bb.y0-T>2)pieceSolids(tcells,'z',0,0,0,bb.x0,bb.x1,bb.y0+T,bb.y1).forEach(function(o){out.push(o)});
      var core=[{n:[1,0,0],d:bb.x1-.05},{n:[-1,0,0],d:-(bb.x0+.05)},{n:[0,1,0],d:bb.y1},{n:[0,-1,0],d:-(bb.y0+T-.1)},{n:[0,0,1],d:bb.z1-T2+.1},{n:[0,0,-1],d:-bb.z0}];
      out.push({id:i,m:b.m,row:b.row,box:bb,faces:faces(core),k:.72})});
    /* FINE: thin incised cracks on flat front and top faces */
    if(up>=7){var Rh=rng(seed*101+3);out.forEach(function(o){o.faces.forEach(function(f){if(f.area>8&&f.cut<=1&&(f.n[1]<-.9||f.n[2]>.9))f.tris=grooves(f,Rh,{hw:.5,depth:.42})})})}
    return{blocks:out,seed:seed,upto:up,H:H,cores:[[X[0],X[1],H[0]-3],[X[1],X[2],H[1]-3],[X[2],W,H[2]-3]]}}
};
G.rng=rng;
})(typeof window!=='undefined'?(window.GEN=window.GEN||{}):module.exports);
