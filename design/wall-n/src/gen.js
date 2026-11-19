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

/* one mass: the wedge is a partition of the slope face into convex cells, recursively, each cell a prism from a common base up to its own top plane */
function clip2(poly,n,c,keepLE){var r=[];for(var k=0;k<poly.length;k++){var a=poly[k],b=poly[(k+1)%poly.length],da=(n[0]*a[0]+n[1]*a[1]-c)*(keepLE?1:-1),db=(n[0]*b[0]+n[1]*b[1]-c)*(keepLE?1:-1);
  if(da<=0)r.push(a);if((da<0&&db>0)||(da>0&&db<0)){var s=da/(da-db);r.push([a[0]+(b[0]-a[0])*s,a[1]+(b[1]-a[1])*s])}}return r}
function area2(p){var s=0;for(var i=0;i<p.length;i++){var a=p[i],b=p[(i+1)%p.length];s+=a[0]*b[1]-b[0]*a[1]}return Math.abs(s)/2}
function vnoise(R0,x,y){var xi=Math.floor(x),yi=Math.floor(y),fx=x-xi,fy=y-yi,h=function(i,j){var q=rng((i*73856093^j*19349663^R0)>>>0);q();return q()},sm=function(t){return t*t*(3-2*t)};
  var a=h(xi,yi),b=h(xi+1,yi),c=h(xi,yi+1),d=h(xi+1,yi+1);return a+(b-a)*sm(fx)+(c-a)*sm(fy)+(a-b-c+d)*sm(fx)*sm(fy)}
G.build=function(seed,upto,opt){
  opt=opt||{};
  if(typeof location!=='undefined'){(location.search.match(/[?&](lo|hi|p0x|p0y|dip|face|px|py|amp|pock|chip|send|tilt|th|l1|spar|eb|ln|sg|gv|gw|gd|bw|s1|s2|s3|l2|l3|d2|d3|weld)=(-?[\w.]+)/g)||[]).forEach(function(m){var k=m.slice(1).split('=');opt[k[0]]=isNaN(+k[1])?k[1]:+k[1]})}
  var R=rng(seed*7919+13),ur=function(a,b){return a+(b-a)*R()};
  var dip=(opt.dip||52)*D2R,fac=(opt.face||262)*D2R,TH=(opt.th==null?20:opt.th)*D2R;
  var nb=[Math.sin(dip)*Math.cos(fac),Math.sin(dip)*Math.sin(fac),Math.cos(dip)];
  var st=[-Math.sin(fac),Math.cos(fac),0];
  var up=norm(cross(st,nb)); if(up[2]<0)up=[-up[0],-up[1],-up[2]];
  var X0=0,X1=35,Y0=0,Y1=35,ZM=60;
  var P0=[5,9,0],dOut=dot(nb,P0);
  var cr=norm([-.94,-.1,1]);
  var vol=[{n:[1,0,0],d:X1},{n:[-1,0,0],d:-X0},{n:[0,1,0],d:Y1},{n:[0,-1,0],d:-Y0},{n:cr,d:dot(cr,[3,10,13])},{n:[0,0,-1],d:0},{n:nb,d:dOut},{n:[0,0,1],d:opt.cap||36}];
  var side=vol.slice(0,6).concat([vol[7]]);
  var blocks=[{planes:vol,vol:1}];
  if(upto<2)return fin(blocks,upto);
  /* footprint of the free face in (strike, up-dip) */
  var ff=faces(vol).filter(function(f){return f.i===6})[0],root=ff.poly.map(function(q){return[dot(st,q),dot(up,q)]});
  var SE=Math.max.apply(0,root.map(function(q){return q[0]}))+(opt.send||0),endA=[4,1.6,.6];
  var T0=dOut-2.5,BASE=dOut-40;
  var AMP=opt.amp==null?2.0:opt.amp;
  /* the three levels: spacing along strike, along the dip, offset of a piece from its parent, joint width, lean of the walls */
  var LV=[{ws:opt.s1||12,wl:opt.l1||40,A:1.9*AMP,jw:0,lean:.16*(opt.ln==null?.4:opt.ln),diag:0,p:1},{ws:opt.s2||8,wl:opt.l2||18,A:.8*AMP,jw:0,lean:.2*(opt.ln==null?.4:opt.ln),diag:opt.d2==null?0:opt.d2,p:1},{ws:opt.s3||5,wl:opt.l3||9,A:.34*AMP,jw:0,lean:.25*(opt.ln==null?.4:opt.ln),diag:opt.d3==null?.08:opt.d3,p:opt.spar==null?.5:opt.spar}];
  var leaves=[];
  function mk(poly,lines,H,A,B,lvl,row){return{poly:poly,lines:lines,H:H,A:A,B:B,row:row}}
  function split(cell,L,depth,out,upLvl,FR){var st=FR.e1,up=FR.e2,nb=FR.n,T0=FR.T0,TH=FR.TH,SE=0;
    var P=cell.poly,mnS=1e9,mxS=-1e9,mnA=1e9,mxA=-1e9;P.forEach(function(q){mnS=Math.min(mnS,q[0]);mxS=Math.max(mxS,q[0]);mnA=Math.min(mnA,q[1]);mxA=Math.max(mxA,q[1])});
    var lv=LV[L],E=[Math.sin(TH),Math.cos(TH)],F=[Math.cos(TH),-Math.sin(TH)],ex=function(v){var m=1e9,M=-1e9;P.forEach(function(q){var t=v[0]*q[0]+v[1]*q[1];m=Math.min(m,t);M=Math.max(M,t)});return M-m};
    var rs=ex(F)/lv.ws,ra=ex(E)/lv.wl;
    if(cell.skip||(depth==0&&R()>lv.p)){cell.skip=0;out.push(cell);return}
    var stop=Math.max(rs,ra)<1.3||depth>7||area2(P)<.5;
    if(stop){out.push(cell);return}
    var phi,r=R();
    if(lv.diag&&r<lv.diag&&rs>1&&ra>1){phi=Math.atan2(F[1],F[0])+(R()<.5?1:-1)*ur(28,55)*D2R}
    else if(rs>=1.3&&(ra<1.3||R()<(opt.cx==null?.75:1-opt.cx))){phi=Math.atan2(F[1],F[0])+ur(-9,9)*D2R}      /* a crack along the dip: normal along strike */
    else phi=Math.atan2(E[1],E[0])+ur(-14,14)*D2R;                                   /* a cross crack: normal along the dip */
    var n2=[Math.cos(phi),Math.sin(phi)],lo=1e9,hi=-1e9;P.forEach(function(q){var t=n2[0]*q[0]+n2[1]*q[1];lo=Math.min(lo,t);hi=Math.max(hi,t)});
    var c=lo+(hi-lo)*ur(.3,.7),mp=[3.2,2,1.5][L];if(c-lo<mp||hi-c<mp){out.push(cell);return}
    var P1=clip2(P,n2,c,true),P2=clip2(P,n2,c,false);
    if(P1.length<3||P2.length<3||area2(P1)<.3||area2(P2)<.3){out.push(cell);return}
    var dr=ur(-lv.lean,lv.lean),w=ur(0,lv.jw),N3=norm([st[0]*n2[0]+up[0]*n2[1]+nb[0]*dr,st[1]*n2[0]+up[1]*n2[1]+nb[1]*dr,st[2]*n2[0]+up[2]*n2[1]+nb[2]*dr]);
    var d3=(c+dr*T0)*1/ (Math.hypot(n2[0],n2[1],dr)),
        kids=[[P1,{n:N3,d:d3-w/2,cut:0,j:1},1],[P2,{n:[-N3[0],-N3[1],-N3[2]],d:-(d3+w/2),cut:0,j:1},-1]];
    /* each side gets its own offset; one side also tilts along the crack so the step between the two dies out toward one end */
    var alongDip=Math.abs(n2[0]*F[0]+n2[1]*F[1])>.7,sgn=opt.sg==null?-1:opt.sg,hs=alongDip?(function(){var m=ur(.35,1)*lv.A*.8*FR.k*sgn;return[-m/2+ur(-.15,.15)*lv.A*FR.k,m/2+ur(-.15,.15)*lv.A*FR.k]})():[ur(-1,1)*lv.A*.3*FR.k,ur(-1,1)*lv.A*.3*FR.k],dir=[-n2[1],n2[0]],hd=hs[1]-hs[0],Lc=Math.max(2,(function(){var m=1e9,M=-1e9;P.forEach(function(q){var t=dir[0]*q[0]+dir[1]*q[1];m=Math.min(m,t);M=Math.max(M,t)});return M-m})());
    var gT=R()<.6?Math.max(-.04,Math.min(.04,-hd/Lc*ur(.8,1.6)*(R()<.5?1:-1))):0;
    kids.forEach(function(k,i){
      var pc=[0,0];k[0].forEach(function(q){pc[0]+=q[0]/k[0].length;pc[1]+=q[1]/k[0].length});
      var gs=(i?gT*dir[0]:0)+ur(-1,1)*[.1,.17,.24][L]*(opt.tilt==null?.3:opt.tilt),ga=(i?gT*dir[1]:0)+ur(-1,1)*[.1,.17,.24][L]*(opt.tilt==null?.3:opt.tilt);
      var cc={poly:k[0],lines:cell.lines.concat([k[1]]),H:cell.H+hs[i]-gs*pc[0]-ga*pc[1],A:cell.A+gs,B:cell.B+ga,row:cell.row};
      split(cc,L,depth+1,out,upLvl,FR)})}

  function surface(FR,root,side,BASEv,withPock){
    var cur=[{poly:root,lines:[],H:FR.T0,A:0,B:0,row:0}];
    for(var L=0;L<3;L++){
      var nx=[];cur.forEach(function(c){var o=[];split(c,L,0,o,L,FR);o.forEach(function(q){q.row=L==0?nx.length:q.row;nx.push(q)})});
      cur=nx;if(upto<L+3)break}
    if(upto>=5&&withPock&&opt.pock!==0){var pk=(opt.pock==null?.6:opt.pock),R0=seed*97;
      cur.forEach(function(c){var m=[0,0];c.poly.forEach(function(q){m[0]+=q[0]/c.poly.length;m[1]+=q[1]/c.poly.length});
        var v=vnoise(R0,m[0]/4.5,m[1]/10)*.65+vnoise(R0+5,m[0]/2.2,m[1]/4.6)*.35;
        if(v>.6){var dd=(v-.6)/.4*1.3*pk+.2;c.H-=dd;c.A+=(R()-.5)*.1;c.B+=(R()-.5)*.1}})}
    if(upto>=5&&withPock&&(opt.gv==null?.55:opt.gv)>0){
      /* grain: narrow grooves along the slab direction, each a strip cell lowered a little, cut by two parallel planes (a cleave with a joints width and no setback) */
      var TH2=FR.TH,Fd=[Math.cos(TH2),-Math.sin(TH2)],nx2=[];
      cur.forEach(function(c){var P=c.poly,lo=1e9,hi=-1e9;P.forEach(function(q){var t=Fd[0]*q[0]+Fd[1]*q[1];lo=Math.min(lo,t);hi=Math.max(hi,t)});
        if(hi-lo<4||R()>(opt.gv==null?.55:opt.gv)){nx2.push(c);return}
        var gw=ur(.25,.5)*(opt.gw||1),dp=ur(.3,.6),k=R()<.35?2:1,cuts=[],t=lo+(hi-lo)*ur(.25,.75);
        for(var g=0;g<k;g++){cuts.push(t);t+=ur(1.6,3.2);if(t>hi-1)break}
        var edges=[-1e9].concat(cuts.reduce(function(a,x){return a.concat([x,x+gw])},[]),[1e9]),
            nrm=function(ph){var a=Math.atan2(Fd[1],Fd[0])+ph;return[Math.cos(a),Math.sin(a)]};
        var n2=[Fd[0],Fd[1]];
        for(var e=0;e<edges.length-1;e++){
          var pc=c.poly;if(edges[e]>-1e8)pc=clip2(pc,n2,edges[e],false);if(edges[e+1]<1e8)pc=clip2(pc,n2,edges[e+1],true);
          if(pc.length<3||area2(pc)<.2)continue;
          var ln=c.lines.slice(),N3=norm([FR.e1[0]*n2[0]+FR.e2[0]*n2[1],FR.e1[1]*n2[0]+FR.e2[1]*n2[1],FR.e1[2]*n2[0]+FR.e2[2]*n2[1]]);
          if(edges[e]>-1e8)ln.push({n:[-N3[0],-N3[1],-N3[2]],d:-edges[e],cut:0,j:1});
          if(edges[e+1]<1e8)ln.push({n:N3,d:edges[e+1],cut:0,j:1});
          nx2.push({poly:pc,lines:ln,H:c.H-(e%2?dp:0)+0,A:c.A,B:c.B,row:c.row})}});
      cur=nx2}
    return cur.map(function(c){
      var pl=side.slice(),len=Math.hypot(c.A,c.B,1),e1=FR.e1,e2=FR.e2,n=FR.n,Nt=norm([-c.A*e1[0]-c.B*e2[0]+n[0],-c.A*e1[1]-c.B*e2[1]+n[1],-c.A*e1[2]-c.B*e2[2]+n[2]]);
      pl.push({n:Nt,d:c.H/len,cut:0,top:1});
      pl.push({n:[-n[0],-n[1],-n[2]],d:-BASEv,cut:0,j:1});
      c.lines.forEach(function(l){pl.push(l)});
      return{planes:pl,i:c.row,cell:1,e1:e1,fam:FR.k==1?0:(FR.k==.35?1:2)}})}
  var dcr=vol[4].d,sideS=[{n:[1,0,0],d:X1-2.5},vol[1],vol[2],vol[3],{n:cr,d:dcr-3.4},vol[5],{n:[0,0,1],d:(opt.cap||36)-2.5}];
  var polyOf=function(i,e1,e2){var f=faces(vol).filter(function(f){return f.i===i})[0];return f.poly.map(function(q){return[dot(e1,q),dot(e2,q)]})};
  var FRs=[{e1:st,e2:up,n:nb,T0:dOut-2.5,TH:TH,k:1}];
  blocks=surface(FRs[0],root,sideS,dOut-40,true);
  var ce1=norm(cross([0,1,0],cr)),ce2=cross(cr,ce1);
  var FRc={e1:ce1,e2:ce2,n:cr,T0:dcr-1.2,TH:70*D2R,k:.35};
  blocks=blocks.concat(surface(FRc,polyOf(4,ce1,ce2),[vol[0],vol[1],vol[2],vol[3],{n:nb,d:dOut-1.5},vol[5],vol[7]],dcr-12,false));
  var FRe={e1:[0,1,0],e2:[0,0,1],n:[1,0,0],T0:X1-1.6,TH:25*D2R,k:.45};
  blocks=blocks.concat(surface(FRe,polyOf(0,[0,1,0],[0,0,1]),[{n:[1,0,0],d:X1+.3},vol[1],vol[2],vol[3],{n:nb,d:dOut-1.5},{n:cr,d:dcr-10},vol[5],{n:[0,0,1],d:(opt.cap||36)-3}],X1-4.5,false));
  if(upto<5)return fin(blocks,upto);
    /* corner chips: at a corner of the top face, a plane along the averaged normal of the faces meeting there, tilted up to 25 degrees, taking 0.12 to 0.3 of the cell's narrow side; 45 per cent of corners */
  var CF=opt.chip==null?.3:opt.chip;
  blocks.forEach(function(b){
    var fcs=faces(b.planes),top=fcs.filter(function(f){return b.planes[f.i].top})[0];if(!top)return;
    var mn=1e9,mx=-1e9;top.poly.forEach(function(q){mn=Math.min(mn,dot(b.e1,q));mx=Math.max(mx,dot(b.e1,q))});
    var ext=Math.min(mx-mn,3.5);if(ext<1)return;
    top.poly.forEach(function(q){
      if(R()>CF)return;
      var vn=[0,0,0];fcs.forEach(function(f){if(f.poly.some(function(r){return Math.hypot(r[0]-q[0],r[1]-q[1],r[2]-q[2])<1e-4}))vn=add(vn,f.n)});
      var tt=ur(0,25)*D2R,ang=ur(0,6.283),n=norm(vn),t1=norm(cross(n,Math.abs(n[2])<.9?[0,0,1]:[1,0,0])),t2=cross(n,t1);
      n=norm(add(add(n,t1,Math.tan(tt)*Math.cos(ang)),t2,Math.tan(tt)*Math.sin(ang)));
      var dep=ur(.12,.3)*ext;b.planes.push(cut(n,[q[0]-n[0]*dep,q[1]-n[1]*dep,q[2]-n[2]*dep],2))})
    /* edge bevels (a worn lip): a plane through the middle of a top edge, tilted 35-55 degrees out of the face, taking 0.3-0.8 mm; a plane cut into the face's edge, so a pit stays a plane cut and no cell is placed inside a cell */
    var EB=opt.eb==null?.85:opt.eb;if(EB>0){var cen=[0,0,0];top.poly.forEach(function(q){cen=add(cen,q,1/top.poly.length)});
      for(var ei=0;ei<top.poly.length;ei++){var qa=top.poly[ei],qb=top.poly[(ei+1)%top.poly.length],el=Math.hypot(qb[0]-qa[0],qb[1]-qa[1],qb[2]-qa[2]);
        if(el<3||R()>EB)continue;
        var ed=norm(sub(qb,qa)),od=norm(cross(top.n,ed)),mid=add(qa,sub(qb,qa),.5);if(dot(od,sub(mid,cen))<0)od=[-od[0],-od[1],-od[2]];
        var bt=ur(28,52)*D2R,bn=norm(add(top.n,od,0)),nn=norm([top.n[0]*Math.cos(bt)+od[0]*Math.sin(bt),top.n[1]*Math.cos(bt)+od[1]*Math.sin(bt),top.n[2]*Math.cos(bt)+od[2]*Math.sin(bt)]),dd=Math.min(ur(.5,1.1)*(opt.bw||1),.3*ext,.18*el);
        b.planes.push(cut(nn,[mid[0]-nn[0]*dd,mid[1]-nn[1]*dd,mid[2]-nn[2]*dd],2))}}});
  /* foot blocks: the same rock in small pieces, flat-faced, with every top edge eased by a chamfer plane */
  var rubble=[];
  for(var q=0;q<11;q++){var px=ur(9,33),py=ur(.8,3.4),sc=ur(1.7,3),th=ur(0,Math.PI),c=Math.cos(th),s2=Math.sin(th);
    if(px<10&&py<8)continue;
    var h=sc*ur(.8,1.2),a2=sc*ur(.8,1.4),dirs=[[c,s2,a2],[-c,-s2,a2],[-s2,c,sc],[s2,-c,sc]],
      pl=[plane([c,s2,0],px*c+py*s2+a2),plane([-c,-s2,0],-(px*c+py*s2)+a2),plane([-s2,c,0],-px*s2+py*c+sc),plane([s2,-c,0],px*s2-py*c+sc),plane([0,0,1],h),plane([0,0,-1],0.3)];
    dirs.forEach(function(d){var nn=norm([d[0]*.8,d[1]*.8,1]),e=[px+d[0]*d[2],py+d[1]*d[2],h],dp=ur(.35,.75);pl.push(cut(nn,[e[0]-nn[0]*dp,e[1]-nn[1]*dp,e[2]-nn[2]*dp],2))});
    pl.push(cut(norm([ur(-.5,.5),ur(-.5,.5),1]),[px,py,h-ur(.5,1)],2));
    rubble.push({planes:pl,rub:1})}
  blocks=blocks.concat(rubble);
  return fin(blocks,upto);
  function fin(bl,up){
    var out=[];
    bl.forEach(function(b,i){
      var fc=faces(b.planes);if(fc.length<4)return;fc.forEach(function(f){f.top=!!b.planes[f.i].top;f.vs=b.planes[f.i]===undefined?0:(b.planes[f.i].n===vol[0].n?1:0)});
      if(up>=6&&b.rub&&opt.hewr){var Rh=rng(seed*101+i*7+3);fc.forEach(function(f){f.tris=hew(f,Rh,f.cut?.5:.75)})}
      out.push({id:i,fam:b.fam,m:b.rub?1:0,row:b.i||0,rub:!!b.rub,faces:fc})});
    return{blocks:out,seed:seed,upto:up,cores:[],core:[]}}
};
G.rng=rng;
})(typeof window!=='undefined'?(window.GEN=window.GEN||{}):module.exports);
