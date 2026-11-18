/* ===== graded areas: the one rule that draws the crest, a course's front and a stone's outline =====
   A smooth wobble along the line (or round the circle) with a BUMPS control for how many wobbles; rank it (sort by wobble value, rank (i + 0.5) / N); map each rank through the weighting's quantile to t in 0..1; a point's distance is inner + gap x t.
   The weighting says how far, bumps says how many: the line has exactly the weighting's mix of distances, only where they fall changes. It is the rule a dirt top uses (Band::rolled), bent along a line or round a circle.
   Three stages use it: the CREST (step 1), the FRONT BAND (step 3a: each course's front lies between today's front and one `depth` behind it) and the STONE OUTLINE (step 7a: each stone's footprint lies between a core and its cell).
   The weighting formulas are crates/dt-value/src/weighting.rs (low, high, bell, peak, ends). */
var GR=(function(){
  var TAU=Math.PI*2;
  function erf(x){var s=Math.sign(x);x=Math.abs(x);var t=1/(1+.3275911*x);return s*(1-(((((1.061405429*t-1.453152027)*t)+1.421413741)*t-0.284496736)*t+0.254829592)*t*Math.exp(-x*x))}
  function phi(z){return .5*(1+erf(z/Math.SQRT2))}
  function nquant(p){var lo=-8,hi=8,i;for(i=0;i<44;i++){var m=(lo+hi)/2;if(phi(m)<p)lo=m;else hi=m}return(lo+hi)/2}
  function lowU(u,r){return Math.abs(r-1)<1e-9?u:-Math.log1p(-u*((r-1)/r))/Math.log(r)}
  function bellU(u,s,at){var a=phi(-at/s),b=phi((1-at)/s),p=a+u*(b-a);if(p>=1)return 1;if(p<=0)return 0;return at+s*nquant(p)}
  function tentArea(x,c,s){if(x<=c-s)return 0;if(x<=c){var d=x-(c-s);return d*d/(2*s)}if(x<c+s){var d2=c+s-x;return s-d2*d2/(2*s)}return s}
  function peakU(u,s,c){var g0=tentArea(0,c,s),g1=tentArea(1,c,s),g=g0+u*(g1-g0);return g<=.5*s?c-s+Math.sqrt(2*s*g):c+s-Math.sqrt(2*s*Math.max(s-g,0))}
  function endsArea(x,at,p){var d=x-at,m=Math.pow(Math.abs(d),p)/p;return d<0?-m:m}
  function endsU(u,k,at){var p=k+1,g0=endsArea(0,at,p),g1=endsArea(1,at,p),g=g0+u*(g1-g0),m=Math.pow(Math.abs(g)*p,1/p);return g<0?at-m:at+m}
  /* the quantile of a weighting: u in 0..1 (a rank) to t in 0..1 (a place in the gap) */
  function unit(w,u){var t;switch(w.name){case'flat':t=u;break;case'low':t=lowU(u,w.a);break;case'high':t=lowU(u,1/w.a);break;case'bell':t=bellU(u,w.a,w.c);break;case'peak':t=peakU(u,w.a,w.c);break;case'ends':t=endsU(u,w.a,w.c);break;default:t=u}
    return isFinite(t)?Math.min(1,Math.max(0,t)):0}
  /* each weighting's own number (its word) and centre, and the defaults of the prototype page */
  var WORDS=['flat','low','high','bell','peak','ends'];
  var WDEF={flat:{},low:{a:12},high:{a:12},bell:{a:.12,c:.5},peak:{a:.25,c:.8},ends:{a:3,c:.5}};
  var WRANGE={low:[1.5,40,.5,'ratio'],high:[1.5,40,.5,'ratio'],bell:[.04,.4,.01,'spread'],peak:[.05,.6,.01,'width'],ends:[1,8,.5,'steepness']};
  var HASC={bell:1,peak:1,ends:1};
  function wOf(s){var d=WDEF[s.w]||{};return {name:s.w,a:s.a!=null&&WRANGE[s.w]?s.a:d.a,c:s.c!=null&&HASC[s.w]?s.c:d.c}}
  /* RANK: sort by wobble value (ties by place), rank (i + 0.5) / N, through the weighting's quantile */
  function ranked(vals,w){var N=vals.length,idx=[],i,t=new Array(N);for(i=0;i<N;i++)idx.push(i);idx.sort(function(a,b){return vals[a]-vals[b]||a-b});idx.forEach(function(k,rk){t[k]=unit(w,(rk+.5)/N)});return t}
  /* the wobble: a sum of 8 cosines (variance 1). On a circle: k or k+1 whole bumps round it. On a line: bps bumps a square, the rate spread 0.8 to 1.2 */
  function circleWobble(seed,bumps){var r=R(seed,1),ph0=r()*TAU,M=8,ks=[],ph=[],m;for(m=0;m<M;m++){ks.push(bumps+(r()<.5?0:1));ph.push(r()*TAU)}var a=Math.sqrt(2/M);
    return function(th){var v=0,m2;for(m2=0;m2<M;m2++)v+=Math.cos(ks[m2]*(th+ph0)+ph[m2]);return v*a}}
  function lineWobble(seed,bps,sq){var r=R(seed,2),M=8,ks=[],ph=[],m;for(m=0;m<M;m++){ks.push(bps*(0.8+0.4*r()));ph.push(r()*TAU)}var a=Math.sqrt(2/M);
    return function(x){var u=x/sq,v=0,m2;for(m2=0;m2<M;m2++)v+=Math.cos(TAU*ks[m2]*u+ph[m2]);return v*a}}
  var SPL=6;   /* points per bump along a line, and about the same round a stone */
  /* ===== the front band: course c's own graded line t_c(x), from its own keyed seed, in 0..1 (0 = today's front, 1 = `depth` behind it) ===== */
  function frontLine(p,g,c){
    var B=p.band,gg=g.gr=g.gr||{front:{}};if(gg.front[c])return gg.front[c];
    var sq=SQUARE*g.f,x0=g.x0,x1=g.x1,len=x1-x0,bps=B.bumps,N=Math.max(8,Math.ceil(len/sq*bps*SPL)),f=lineWobble(hash(p.seed,9101,c),bps,sq),xs=[],vals=[],k;
    for(k=0;k<N;k++){var x=x0+(k+.5)/N*len;xs.push(x);vals.push(f(x))}
    var t=ranked(vals,wOf(B));
    var fn={xs:xs,t:t,at:function(x){if(x<=xs[0])return t[0];if(x>=xs[N-1])return t[N-1];var u=(x-x0)/len*N-.5,i=Math.max(0,Math.min(N-2,Math.floor(u))),w2=(x-xs[i])/(xs[i+1]-xs[i]);return t[i]+(t[i+1]-t[i])*w2}};
    gg.front[c]=fn;return fn}
  /* the baseline of course c's front edge ei (a horizontal front edge of the footprint): today's front line, y = the edge's y less the course's offset */
  function baseY(g,c,ei){return g.edges[ei].a[1]-g.offs[c][ei]}
  function frontEdges(g){if(g.gr&&g.gr.fe)return g.gr.fe;var o=[];g.edges.forEach(function(E,ei){if(E.front&&Math.abs(E.n[0])<1e-6&&E.n[1]<0)o.push(ei)});g.gr=g.gr||{front:{}};g.gr.fe=o;return o}
  /* the course's front stands, at each of the line's nodes, between baseY and baseY + depth. With the rule `stay` (default on) a course never stands in front of the one below where the unbanded wall does not
     either, so the band adds no overhang: node by node G_c = max(own line, G_(c-1)); every course's line is the same nodes, so each polyline is above the one under it everywhere, not only at the nodes. A lip course, which stands out on purpose, is left alone. */
  function nodesG(p,g,c,ei){var B=p.band,gg=g.gr,key=c+'_'+ei;gg.G=gg.G||{};if(gg.G[key])return gg.G[key];
    var ln=frontLine(p,g,c),by=baseY(g,c,ei),D=B.depth*g.f,v=ln.t.map(function(t){return by+D*t});
    if(B.stay&&c>0&&by>=baseY(g,c-1,ei)-1e-6){var pv=nodesG(p,g,c-1,ei);v=v.map(function(y,k){return Math.max(y,pv[k])})}
    return gg.G[key]=v}
  function Gat(p,g,c,ei,x){var ln=frontLine(p,g,c),v=nodesG(p,g,c,ei),xs=ln.xs,N=xs.length;if(x<=xs[0])return v[0];if(x>=xs[N-1])return v[N-1];var u=(x-xs[0])/(xs[N-1]-xs[0])*(N-1),i=Math.max(0,Math.min(N-2,Math.floor(u)));return v[i]+(v[i+1]-v[i])*(x-xs[i])/(xs[i+1]-xs[i])}
  /* ===== plan geometry, on Manifold's CrossSection ===== */
  function CSof(rings){return MF.CrossSection.ofPolygons(rings)}
  function area2(r){var s=0,i;for(i=0;i<r.length;i++){var a=r[i],b=r[(i+1)%r.length];s+=a[0]*b[1]-b[0]*a[1]}return s/2}
  function ccw(r){return area2(r)<0?r.slice().reverse():r}
  function simplify(r){var out=r.slice(),ch=true,g=0;while(ch&&g++<200){ch=false;for(var i=0;i<out.length&&out.length>3;i++){var a=out[(i+out.length-1)%out.length],b=out[i],c=out[(i+1)%out.length],ex=c[0]-a[0],ey=c[1]-a[1],l=Math.hypot(ex,ey),d=Math.hypot(b[0]-a[0],b[1]-a[1]);
      if(d<1e-5||Math.abs((b[0]-a[0])*ey-(b[1]-a[1])*ex)<1e-5*Math.max(1,l)&&((b[0]-a[0])*ex+(b[1]-a[1])*ey)>0){out.splice(i,1);ch=true;break}}}return out}
  function distSeg(p,a,b){var ex=b[0]-a[0],ey=b[1]-a[1],l2=ex*ex+ey*ey,t=l2<1e-12?0:Math.max(0,Math.min(1,((p[0]-a[0])*ex+(p[1]-a[1])*ey)/l2));return Math.hypot(p[0]-a[0]-ex*t,p[1]-a[1]-ey*t)}
  /* an edge is a joint if it lies on a joint edge of the cell (within 0.08 mm, and parallel to it) */
  function jointOf(a,b,joints){var m=[(a[0]+b[0])/2,(a[1]+b[1])/2],i,ex=b[0]-a[0],ey=b[1]-a[1],l=Math.hypot(ex,ey)||1;
    for(i=0;i<joints.length;i++){var J=joints[i],jx=J[1][0]-J[0][0],jy=J[1][1]-J[0][1],jl=Math.hypot(jx,jy)||1;if(Math.abs(ex*jy-ey*jx)/(l*jl)<0.03&&distSeg(m,J[0],J[1])<0.08)return true}return false}
  /* ===== the stone's own outline: the rule round a circle. ring: CCW polygon; joints: the cell's joint edges (kept, never moved). Returns a new ring. ===== */
  function shrinkRing(ring,joints,O,s,seed,f){
    var n=ring.length,isJ=ring.map(function(q,i){return jointOf(q,ring[(i+1)%n],joints)}),i,k;
    var cx=0,cy=0,A=0;for(i=0;i<n;i++){var a=ring[i],b=ring[(i+1)%n],cr=a[0]*b[1]-b[0]*a[1];A+=cr;cx+=(a[0]+b[0])*cr;cy+=(a[1]+b[1])*cr}if(Math.abs(A)<1e-9)return ring;cx/=3*A;cy/=3*A;
    var rin=1e9;for(i=0;i<n;i++){var d=distSeg([cx,cy],ring[i],ring[(i+1)%n]);if(d<rin)rin=d}
    var rcore=O.core*rin,per=0;for(i=0;i<n;i++)per+=Math.hypot(ring[(i+1)%n][0]-ring[i][0],ring[(i+1)%n][1]-ring[i][1]);
    var Pn=Math.max(24,Math.round(SPL*O.bumps)),h=per/Pn,anyJ=isJ.some(function(v){return v});
    /* points: every vertex and subdivisions of the free edges; each says whether it can move and its arc distance from a pinned end */
    var pts=[],cur,fixedV=ring.map(function(q,i){return isJ[i]||isJ[(i+n-1)%n]});
    for(i=0;i<n;i++){pts.push({p:ring[i],mv:!fixedV[i],v:true,i:i});if(!isJ[i]){var a2=ring[i],b2=ring[(i+1)%n],l=Math.hypot(b2[0]-a2[0],b2[1]-a2[1]),m=Math.max(1,Math.ceil(l/h));for(k=1;k<m;k++)pts.push({p:[a2[0]+(b2[0]-a2[0])*k/m,a2[1]+(b2[1]-a2[1])*k/m],mv:true,v:false,i:i})}}
    var NP=pts.length,seg=[];   /* chains of consecutive points from one fixed point to the next, for the arc distance */
    var st=-1;for(i=0;i<NP;i++)if(!pts[i].mv){st=i;break}
    var dEnd=new Array(NP).fill(1e9);
    if(st>=0){var run=[],j;for(k=1;k<=NP;k++){var q=pts[(st+k)%NP];if(q.mv)run.push((st+k)%NP);else{
        if(run.length){var cum=[],L=0,prev=pts[(st+k-run.length-1+NP*2)%NP].p;run.forEach(function(ix){L+=Math.hypot(pts[ix].p[0]-prev[0],pts[ix].p[1]-prev[1]);prev=pts[ix].p;cum.push(L)});L+=Math.hypot(q.p[0]-prev[0],q.p[1]-prev[1]);
          run.forEach(function(ix,r2){dEnd[ix]=Math.min(cum[r2],L-cum[r2]);pts[ix].L=L})}
        run=[]}}}
    var wob=circleWobble(seed,O.bumps),w=wOf(O),mvIdx=[],vals=[];
    for(i=0;i<NP;i++)if(pts[i].mv){mvIdx.push(i);vals.push(wob(Math.atan2(pts[i].p[1]-cy,pts[i].p[0]-cx)))}
    var T=ranked(vals,w),out=[];mvIdx.forEach(function(ix,r2){pts[ix].t=T[r2]});
    function sm(x){x=Math.max(0,Math.min(1,x));return x*x*(3-2*x)}
    for(i=0;i<NP;i++){var P0=pts[i];if(!P0.mv){out.push(P0.p);continue}
      var tau=dEnd[i]>=1e8?1:sm(dEnd[i]/Math.max(1.5*h,0.15*(P0.L||1e9))),u=1-s*tau*(1-P0.t),dx=P0.p[0]-cx,dy=P0.p[1]-cy,rho=Math.hypot(dx,dy);
      if(rho<=rcore+1e-9){out.push(P0.p);continue}var r1=rcore+(rho-rcore)*u;out.push([cx+dx*r1/rho,cy+dy*r1/rho])}
    return out}
  /* ===== the area of a stone: front band, outline, held by what holds it. ctx: {build, memo}. Returns {rings,parts} or null (nothing changes this stone). ===== */
  function bandOn(p){var B=p.band;return !!B&&B.on&&B.depth>0}
  function outlOn(p){var O=p.outl;return !!O&&O.on&&O.s>0}
  function areaOn(p){return bandOn(p)||outlOn(p)}
  function supportsOf(ctx,b){return ctx.blocks.filter(function(o){return cE(o)===b.c-1&&b.c>0&&polyOverlap(b.poly,o.poly)>0.02})}
  function stoneArea(ctx,b,jointsOf){
    var key=b.c+'_'+b.i;if(ctx.memo[key]!==undefined)return ctx.memo[key];
    var p=ctx.p,g=ctx.g,rep=ctx.rep,P0=ccw(b.poly.map(function(q){return [q[0],q[1]]})),joints=jointsOf(b,P0),changed=false,rings=[P0],i;
    /* step 3a: the front band. The cell is cut to the epigraph of this course's front line over the free front edge it has. */
    if(bandOn(p)){var fe=frontEdges(g),f=g.f;
      var fr=null,xs=P0.map(function(q){return q[0]}),ys=P0.map(function(q){return q[1]}),x0=Math.min.apply(null,xs),x1=Math.max.apply(null,xs),y0=Math.min.apply(null,ys),y1=Math.max.apply(null,ys);
      P0.forEach(function(a,ii){var b2=P0[(ii+1)%P0.length],ex=b2[0]-a[0],ey=b2[1]-a[1],l=Math.hypot(ex,ey)||1;
        if(!jointOf(a,b2,joints)&&ex/l>0.9&&Math.abs(ey/l)<0.15&&fr===null){var xm=(a[0]+b2[0])/2,ym=(a[1]+b2[1])/2,best=null,bd=1e9;fe.forEach(function(ei){var E=g.edges[ei],lo=Math.min(E.a[0],E.b[0])-0.7,hi=Math.max(E.a[0],E.b[0])+0.7;if(xm<lo||xm>hi)return;var d=Math.abs(ym-baseY(g,b.c,ei));if(d<bd){bd=d;best=ei}});if(best!=null&&bd<3*f)fr=best}});
      if(fr!==null){var E=g.edges[fr],lo=Math.min(E.a[0],E.b[0]),hi=Math.max(E.a[0],E.b[0]),ln=frontLine(p,g,b.c),ylow=y0-3,bp=[x0-1],q;
        ln.xs.forEach(function(x){if(x>x0&&x<x1)bp.push(x)});bp.push(x1+1);
        var poly=bp.map(function(x){var y=Gat(p,g,b.c,fr,Math.max(lo,Math.min(hi,x)));return [x,Math.max(ylow,y)]});poly.push([x1+1,y1+4]);poly.push([x0-1,y1+4]);
        var A=CSof([ccw(P0)]),Bq=CSof([ccw(poly)]),I=A.intersect(Bq),pl=I.toPolygons();A.delete();Bq.delete();I.delete();
        if(pl.length){rings=pl.filter(function(r){return area2(r)>0});changed=true}}}
    /* step 7a: the stone's own outline, then cut to what holds it */
    if(outlOn(p)){var O=p.outl,f2=g.f,sd=hash(p.seed,9202,b.c,b.i);
      rings=rings.map(function(r){return shrinkRing(r,joints,{w:O.w,a:O.a,c:O.c,bumps:O.bumps,core:O.core},O.s,sd,f2)});changed=true;
      if(b.c>0){var sup=supportsOf(ctx,b),srings=[];sup.forEach(function(o){var A2=stoneArea(ctx,o,jointsOf);(A2?A2.rings:[ccw(o.poly.map(function(q){return [q[0],q[1]]}))]).forEach(function(r){srings.push(r)})});
        if(srings.length){var Ms=CSof(rings),Us=CSof(srings),I2=Ms.intersect(Us),pl2=I2.toPolygons();Ms.delete();Us.delete();I2.delete();rings=pl2.filter(function(r){return area2(r)>0})}}}
    if(!changed){ctx.memo[key]=null;return null}
    /* keep the largest component and any that is a real part of the stone; the rest are slivers (counted, not hidden) */
    rings=rings.map(function(r){return simplify(ccw(r))}).filter(function(r){return r.length>=3&&area2(r)>0.05});
    if(!rings.length){rep.areaLost=(rep.areaLost||0)+1;ctx.memo[key]=null;return null}
    var big=Math.max.apply(null,rings.map(area2));var kept=rings.filter(function(r){return area2(r)>=0.25*big});if(kept.length<rings.length)rep.areaDrops=(rep.areaDrops||0)+(rings.length-kept.length);rings=kept;
    var parts=[];rings.forEach(function(r){var roles=r.map(function(q,ii){return jointOf(q,r[(ii+1)%r.length],joints)?'joint':'surface'});
      convexParts(r).forEach(function(pt){parts.push({area:pt.v,edges:pt.fi.map(function(fi){return fi>=0?roles[fi]:'seam'})})})});
    rep.areaParts=(rep.areaParts||0)+parts.length;var res={rings:rings,parts:parts};ctx.memo[key]=res;return res}
  return {frontEdges:frontEdges,simplify:simplify,CSof:CSof,area2:area2,ccw:ccw,unit:unit,WORDS:WORDS,WDEF:WDEF,WRANGE:WRANGE,HASC:HASC,wOf:wOf,ranked:ranked,circleWobble:circleWobble,lineWobble:lineWobble,frontLine:frontLine,Gat:Gat,baseY:baseY,shrinkRing:shrinkRing,stoneArea:stoneArea,bandOn:bandOn,outlOn:outlOn,areaOn:areaOn,SPL:SPL}
})();
