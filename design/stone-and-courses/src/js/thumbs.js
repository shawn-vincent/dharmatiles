/* ===== the small magenta views on the Region and Rock region cards: top, side and front of the area, drawn 2D from the same geometry the target and the build use ===== */
(function(){
  var TW=56,TH=44;
  function colors(){var dk=isDarkTheme();return {line:dk?'255,122,224':'214,32,164',halo:dk?'6,24,46':'255,255,255'}}
  /* shapes: {pts:[[u,v]...], closed, fill, dash, w}; v is up. One fit for the whole view. */
  function paint(cv,shapes){var ctx=cv.getContext('2d'),W=cv.width,H=cv.height,C=colors(),i,j,u0=1e9,u1=-1e9,v0=1e9,v1=-1e9;ctx.clearRect(0,0,W,H);
    shapes.forEach(function(sh){sh.pts.forEach(function(q){if(q[0]<u0)u0=q[0];if(q[0]>u1)u1=q[0];if(q[1]<v0)v0=q[1];if(q[1]>v1)v1=q[1]})});
    if(u0>u1)return;var pad=4,k=Math.min((W-2*pad)/Math.max(1e-6,u1-u0),(H-2*pad)/Math.max(1e-6,v1-v0)),ox=(W-k*(u1-u0))/2-k*u0,oy=H-((H-k*(v1-v0))/2-k*v0);
    function X(q){return [ox+k*q[0],oy-k*q[1]]}
    [['halo',2.6],['line',1.2]].forEach(function(pass){shapes.forEach(function(sh){var p=sh.pts.map(X);if(p.length<2)return;ctx.beginPath();p.forEach(function(q,n){n?ctx.lineTo(q[0],q[1]):ctx.moveTo(q[0],q[1])});if(sh.closed)ctx.closePath();
      ctx.setLineDash(sh.dash?[3,2]:[]);ctx.lineWidth=(sh.w||1)*(pass[0]==='halo'?pass[1]:pass[1]);
      if(pass[0]==='line'&&sh.fill){ctx.fillStyle='rgba('+C.line+',.16)';ctx.fill()}
      ctx.strokeStyle='rgba('+(pass[0]==='halo'?C.halo:C.line)+','+(pass[0]==='halo'?0.5:(sh.faint?0.55:0.95))+')';ctx.stroke()})});ctx.setLineDash([])}
  function hull(P){var pts=P.slice().sort(function(a,b){return a[0]-b[0]||a[1]-b[1]}),cr=function(o,a,b){return (a[0]-o[0])*(b[1]-o[1])-(a[1]-o[1])*(b[0]-o[0])},lo=[],up=[],i;
    for(i=0;i<pts.length;i++){while(lo.length>1&&cr(lo[lo.length-2],lo[lo.length-1],pts[i])<=0)lo.pop();lo.push(pts[i])}
    for(i=pts.length-1;i>=0;i--){while(up.length>1&&cr(up[up.length-2],up[up.length-1],pts[i])<=0)up.pop();up.push(pts[i])}
    lo.pop();up.pop();return lo.concat(up)}
  /* REGION: the envelope the target draws: the footprint, the crest, and each course's front line */
  window.regionThumbs=function(cvs,b){var g=b.g,fp=g.fp,loft=faceLoft(b),top=[],side=[],front=[],i;
    top.push({pts:fp,closed:true});loft.forEach(function(L){top.push({pts:L.pts,faint:true,w:.7})});
    var ys=fp.map(function(q){return q[1]}),y0=Math.min.apply(null,ys),y1=Math.max.apply(null,ys),xs=fp.map(function(q){return q[0]}),x0=Math.min.apply(null,xs),x1=Math.max.apply(null,xs),zt=DIRT+g.topHi,crest=[];
    for(i=0;i<=40;i++){var x=x0+(x1-x0)*i/40;crest.push([x,g.crest(x)])}
    /* front: the crest over the footprint's width, the ground, and the row planes */
    front.push({pts:[[x0,DIRT]].concat(crest).concat([[x1,DIRT]]),closed:true,fill:true});
    (g.planes||[]).forEach(function(pl,c){if(c>0&&pl.z<zt)front.push({pts:[[x0,pl.z],[x1,pl.z]],faint:true,w:.6})});
    /* side: the ground, the crest's highest section and each course's front stepping back */
    var hi=0;crest.forEach(function(q){if(q[1]>hi)hi=q[1]});side.push({pts:[[y0,DIRT],[y1,DIRT],[y1,hi],[y0,hi]],closed:true,fill:true});
    var byC={};loft.filter(function(L){return L.face==='front'}).forEach(function(L){var my=1e9;L.pts.forEach(function(q){if(q[1]<my)my=q[1]});byC[L.c]=[my,L.z0,L.z1]});
    Object.keys(byC).forEach(function(c){var r=byC[c];side.push({pts:[[r[0],r[1]],[r[0],Math.min(r[2],hi)]],w:1.1})});
    paint(cvs[0],top);paint(cvs[1],side);paint(cvs[2],front)};
  /* ROCK REGION: one stone's region as the rock gets it: its cell, the graded outline, the support it bears on, and its top (the row's top under the crest planes) */
  function inPoly(P,x,y){var c=false,i,j;for(i=0,j=P.length-1;i<P.length;j=i++){if(((P[i][1]>y)!==(P[j][1]>y))&&(x<(P[j][0]-P[i][0])*(y-P[i][1])/(P[j][1]-P[i][1])+P[i][0]))c=!c}return c}
  window.stoneThumbs=function(cvs,b,job){var blk=job?job.block:null;if(!blk)return;var cx=blk.cx[0],cy=blk.cx[1],top=[],side=[],front=[];
    top.push({pts:blk.polyT.map(function(q){return [q[0]-cx,q[1]-cy]}),closed:true,dash:true,faint:true,w:.8});
    var sil=[],silY=[];
    (job.parts||[]).forEach(function(pt){var A=pt.area;top.push({pts:A,closed:true,fill:true});if(pt.bed&&pt.bed.bear)top.push({pts:pt.bed.bear,closed:true,w:1.3});
      var bed=pt.bed,tp=pt.top,pl=pt.planes||[],xs=A.map(function(q){return q[0]}),ys=A.map(function(q){return q[1]}),a0=Math.min.apply(null,xs),a1=Math.max.apply(null,xs),b0=Math.min.apply(null,ys),b1=Math.max.apply(null,ys),i,j;
      function zb(x,y){return bed?(bed.d-bed.n[0]*x-bed.n[1]*y)/bed.n[2]:0}
      function zt(x,y){var z=tp?(tp.d-tp.n[0]*x-tp.n[1]*y)/tp.n[2]:1e9;pl.forEach(function(q){if(q.n[2]>0.05){var v=(q.d-q.n[0]*x-q.n[1]*y)/q.n[2];if(v<z)z=v}});return z}
      for(i=0;i<=8;i++)for(j=0;j<=8;j++){var x=a0+(a1-a0)*i/8,y=b0+(b1-b0)*j/8;if(!inPoly(A,x,y))continue;var z0=zb(x,y),z1=zt(x,y);if(z1<z0||z1>1e8)continue;sil.push([x,z0],[x,z1]);silY.push([y,z0],[y,z1])}});
    if(sil.length>2){front.push({pts:hull(sil),closed:true,fill:true});side.push({pts:hull(silY),closed:true,fill:true})}
    paint(cvs[0],top);paint(cvs[1],side);paint(cvs[2],front)};
  window.stoneCell=function(cvs,b,blk){var cx=blk.cx[0],cy=blk.cx[1];paint(cvs[0],[{pts:blk.polyT.map(function(q){return [q[0]-cx,q[1]-cy]}),closed:true,fill:true}]);
    var xs=blk.polyT.map(function(q){return q[0]-cx}),ys=blk.polyT.map(function(q){return q[1]-cy}),x0=Math.min.apply(null,xs),x1=Math.max.apply(null,xs),y0=Math.min.apply(null,ys),y1=Math.max.apply(null,ys);
    paint(cvs[1],[{pts:[[y0,blk.zb],[y1,blk.zb],[y1,blk.zt],[y0,blk.zt]],closed:true,fill:true}]);paint(cvs[2],[{pts:[[x0,blk.zb],[x1,blk.zb],[x1,blk.zt],[x0,blk.zt]],closed:true,fill:true}])};
  /* the stone the card shows: the largest cell of the middle course (a stone the rock dropped is not offered) */
  window.pickStone=function(b){var cs={},list=b.l2.blocks;list.forEach(function(k){(cs[k.c]=cs[k.c]||[]).push(k)});var keys=Object.keys(cs).map(Number).sort(function(x,y){return x-y});if(!keys.length)return null;var mid=cs[keys[Math.floor((keys.length-1)/2)]],best=null,ba=-1;mid.forEach(function(k){var a=Math.abs(polyArea(k.polyT));if(a>ba){ba=a;best=k}});return best};
})();
