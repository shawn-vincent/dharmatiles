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
  for(i=0;i<p.length;i++){var a=p[i],b=p[(i+1)%p.length],L=Math.hypot(b[0]-a[0],b[1]-a[1],b[2]-a[2]),k=Math.max(1,Math.round(L/5));
    for(var j=0;j<k;j++){var q=[a[0]+(b[0]-a[0])*j/k,a[1]+(b[1]-a[1])*j/k,a[2]+(b[2]-a[2])*j/k];P3.push(q);P2.push(to2(q));disp.push(0)}}
  var poly2=p.map(to2),mnx=1e9,mny=1e9,mxx=-1e9,mxy=-1e9;poly2.forEach(function(q){mnx=Math.min(mnx,q[0]);mxx=Math.max(mxx,q[0]);mny=Math.min(mny,q[1]);mxy=Math.max(mxy,q[1])});
  /* orientation of the polygon in 2D, to test "inside with margin" */
  var area=0;for(i=0;i<poly2.length;i++){var A=poly2[i],B=poly2[(i+1)%poly2.length];area+=A[0]*B[1]-B[0]*A[1]}var sg=area>0?1:-1;
  function inside(q,m){for(var i=0;i<poly2.length;i++){var A=poly2[i],B=poly2[(i+1)%poly2.length],ex=B[0]-A[0],ey=B[1]-A[1],l=Math.hypot(ex,ey)||1,dd=sg*(ex*(q[1]-A[1])-ey*(q[0]-A[0]))/l;if(dd<m)return false}return true}
  if(Math.min(mxx-mnx,mxy-mny)<3.6)return null;var sp=Math.max(1.7,Math.min(5,Math.min(mxx-mnx,mxy-mny)/3));amp=amp*Math.min(1,sp/4);
  for(var x=mnx+sp/2;x<mxx;x+=sp)for(var y=mny+sp/2;y<mxy;y+=sp){var q=[x+(R()-.5)*sp*.8,y+(R()-.5)*sp*.8];if(!inside(q,1.2))continue;
    var h=(R()-.5)*2*amp;P2.push(q);P3.push([o[0]+u[0]*q[0]+v[0]*q[1]+n[0]*h,o[1]+u[1]*q[0]+v[1]*q[1]+n[1]*h,o[2]+u[2]*q[0]+v[2]*q[1]+n[2]*h]);disp.push(h)}
  if(P2.length<4)return null;
  return delaunay(P2).map(function(tr){return[P3[tr[0]],P3[tr[1]],P3[tr[2]]]})}


var STAGES=['volume','beds','ends','push','chips','hew'];
G.STAGES=STAGES;
var D2R=Math.PI/180;
var sub=function(a,b){return[a[0]-b[0],a[1]-b[1],a[2]-b[2]]};
var add=function(a,b,s){s=s==null?1:s;return[a[0]+b[0]*s,a[1]+b[1]*s,a[2]+b[2]*s]};
/* a plane with normal n through point q, as a half-space keeping n.p <= d */
var plane=function(n,d,tag){return{n:n,d:d,cut:tag||0}};
var J=function(n,d){return{n:n,d:d,cut:0,j:1}};
/* a family of near-parallel planes: normal n, offsets from lo to hi, spacing drawn from [a,b] */
function family(R,n,lo,hi,a,b){var o=[],s=lo+R()*(b-a)*0.6;while(s<hi){o.push(s);s+=a+(b-a)*R()}return o}
function intervals(offs,lo,hi){var e=[lo].concat(offs.filter(function(x){return x>lo+1.5&&x<hi-1.5}),[hi]);var r=[];for(var i=0;i<e.length-1;i++)r.push([e[i],e[i+1]]);return r}
/* The wedge in reference N: a ramp rising to the back right. VOLUME = box ∩ slope plane ∩ flat cap. BEDS = a dipping family. ENDS = two cross families. PUSH = each cell's own offset along a normal. */
G.build=function(seed,upto,opt){
  opt=opt||{};
  if(typeof location!=='undefined'){(location.search.match(/[?&](lo|hi|p0x|p0y|cross|rand|dir|face|dip|drop|thick|carry|depth|px|py)=(-?[\w.]+)/g)||[]).forEach(function(m){var k=m.slice(1).split('=');opt[k[0]]=isNaN(+k[1])?k[1]:+k[1]})}
  var R=rng(seed*7919+13),ur=function(a,b){return a+(b-a)*R()};
  var dip=(opt.dip||52)*D2R,fac=(opt.face||215)*D2R,DIR=opt.dir==null?1:opt.dir;
  var nb=[Math.sin(dip)*Math.cos(fac),Math.sin(dip)*Math.sin(fac),Math.cos(dip)];   /* normal of the dip slope and of the beds: they are one plane */
  var st=[-Math.sin(fac),Math.cos(fac),0];
  var up=norm(cross(st,nb)); if(up[2]<0)up=[-up[0],-up[1],-up[2]];
  /* 1 VOLUME: a box under the dip slope; the slope passes through the front-left foot point */
  var X0=opt.px==null?3:opt.px,X1=35,Y0=opt.py==null?4:opt.py,Y1=35,ZM=60;
  var P0=[opt.p0x==null?5:opt.p0x,opt.p0y==null?7:opt.p0y,0],dOut=dot(nb,P0);
  var vol=[{n:[1,0,0],d:X1},{n:[-1,0,0],d:-X0},{n:[0,1,0],d:Y1},{n:[0,-1,0],d:-Y0},{n:norm([-.94,-.1,1]),d:dot(norm([-.94,-.1,1]),[3,10,13])},{n:[0,0,-1],d:0},{n:nb,d:dOut},{n:[0,0,1],d:opt.cap||43}];
  var ns=nb,dSs=dOut;
  var blocks=[{planes:vol,vol:1}];
  if(upto<2)return fin(blocks,upto);
  /* 2 BEDS: planes parallel to the slope, spacing 2..3 mm along the normal, only in the outer DEPTH mm; the rest is one core block */
  var depth=opt.depth||30,th=opt.thick||5.2;
  var offs=[];for(var o=dOut-depth;o<dOut-.5;o+=th*ur(.8,1.25))offs.push(o);
  offs.push(dOut);
  var slabs=[];for(var i=0;i<offs.length-1;i++)slabs.push([offs[i],offs[i+1]]);
  var cells=[{planes:vol.slice(0,6).concat([vol[7]]).concat([J(nb,offs[0])]),i:-1,core:1}];
  if(upto<3){return fin(cells.concat(slabs.map(function(sl,i){return{planes:vol.slice(0,6).concat([vol[7]]).concat([J(nb,sl[1]),J([-nb[0],-nb[1],-nb[2]],-sl[0])]),i:i}})),upto)}
  /* 3 EDGES: slab k ends at a line parallel to the crest (the up-dip direction): a plane with normal along the strike, lowered a drawn amount per slab. Cross set A (normal up-dip, leaning) cuts the slab into long pieces, sparse. */
  var pts=[];[X0,X1].forEach(function(x){[Y0,Y1].forEach(function(y){[0,ZM].forEach(function(z){pts.push([x,y,z])})})});
  var aMin=1e9,aMax=-1e9,bMin=1e9,bMax=-1e9;pts.forEach(function(p){var a=dot(up,p),b=dot(st,p);aMin=Math.min(aMin,a);aMax=Math.max(aMax,a);bMin=Math.min(bMin,b);bMax=Math.max(bMax,b)});
  var P0b=dot(st,P0),drop=opt.drop==null?2.4:opt.drop;
  var carry=opt.carry==null?.12:opt.carry,prevA=[];
  var n=slabs.length;
  slabs.forEach(function(sl,i){
    var k=n-1-i;                                         /* k = 0 for the outermost slab */
    var B=(i===0)?bMax:opt.rand!==0?P0b+DIR*ur(opt.lo==null?0:opt.lo,opt.hi==null?14:opt.hi):P0b+DIR*(k*drop+ur(-.5,.9))+ur(-.6,.6);        /* this slab's edge on the strike axis: outer slabs end earlier */
    var aOff=family(R,up,aMin,aMax,opt.cross||9,opt.cross?opt.cross*1.55:14).map(function(a,j){return prevA[j]!=null&&R()<carry?prevA[j]:a});prevA=aOff;
    intervals(aOff,aMin,aMax).forEach(function(ai){
      var tl=ur(-12,12)*D2R,th2=ur(-12,12)*D2R,mid=(sl[0]+sl[1])/2;
      var nLo=norm(add([-up[0],-up[1],-up[2]],nb,Math.tan(tl))),nHi=norm(add(up,nb,Math.tan(th2)));
      var pl=vol.slice(0,6).concat([vol[7]]);
      pl.push(J(nLo,dot(nLo,add(add([0,0,0],up,ai[0]),nb,mid))));pl.push(J(nHi,dot(nHi,add(add([0,0,0],up,ai[1]),nb,mid))));
      var ob=J(nb,sl[1]);ob.outer=1;pl.push(ob);pl.push(J([-nb[0],-nb[1],-nb[2]],-sl[0]));
      pl.push(J(st,B));                                  /* the edge: keep st.p <= B */
      cells.push({planes:pl,i:i})})});
  blocks=cells;
  if(upto<4)return fin(blocks,upto);
  /* 4 PUSH: each slab's outer face moves 0..0.8 mm along its own normal, so neighbours step against each other */
  blocks.forEach(function(c){if(c.core||c.apron)return;c.planes.forEach(function(p){if(p.outer)p.d+=ur(-.5,.5)})});
  if(upto<5)return fin(blocks,upto);
  /* 5 CHIPS */
  blocks.forEach(function(c){
    var nn0=Math.floor(ur(0,2.3));
    for(var k=0;k<nn0;k++){
      var fcs=faces(c.planes);if(!fcs.length)return;
      var f=fcs[Math.floor(R()*fcs.length)],p0=f.poly[Math.floor(R()*f.poly.length)];
      var nn=norm([f.n[0]+ur(-.6,.6),f.n[1]+ur(-.6,.6),f.n[2]+ur(0,.8)]),dep=ur(.8,1.9);
      c.planes.push(cut(nn,[p0[0]-nn[0]*dep,p0[1]-nn[1]*dep,p0[2]-nn[2]*dep],2))}
  });
  var rubble=[];
  for(var q=0;q<14;q++){var px=ur(9,33),py=ur(.8,3.2),sc=ur(1.4,2.9),th=ur(0,Math.PI),c=Math.cos(th),s2=Math.sin(th);
    if(px<10&&py<8)continue;
    var h=sc*ur(.7,1.1),a2=sc*ur(.8,1.3),pl=[plane([c,s2,0],px*c+py*s2+a2),plane([-c,-s2,0],-(px*c+py*s2)+a2),plane([-s2,c,0],-px*s2+py*c+sc),plane([s2,-c,0],px*s2-py*c+sc),plane([0,0,1],h),plane([0,0,-1],0.3),
      cut(norm([ur(-.5,.5),ur(-.5,.5),1]),[px,py,h-ur(.3,1)],2),cut(norm([ur(-.9,.9),ur(-.9,.9),.5]),[px+ur(-1,1),py+ur(-1,1),h*.5],2)];
    rubble.push({planes:pl,rub:1})}
  blocks=blocks.concat(rubble);
  if(upto<6)return fin(blocks,upto);
  return fin(blocks,upto);
  function fin(bl,up){
    var out=[];
    bl.forEach(function(b,i){
      var pl=b.planes;
      if(up>=3&&!b.rub){pl=pl.map(function(p){return p.j?{n:p.n,d:p.d-.3,cut:0,j:1}:p})}
      var fc=faces(pl);if(fc.length<4)return;var tot=0;fc.forEach(function(f){tot+=f.area});if(!b.rub&&!b.core&&tot<30)return;
      if(up>=6){var Rh=rng(seed*101+i*7+3);fc.forEach(function(f){f.tris=hew(f,Rh,f.cut?.5:.75)})}
      out.push({id:i,m:b.rub?1:0,row:b.i||0,rub:!!b.rub,faces:fc})});
    var core=[];
    return{blocks:out,seed:seed,upto:up,cores:[],core:core}}
};
G.rng=rng;
})(typeof window!=='undefined'?(window.GEN=window.GEN||{}):module.exports);
