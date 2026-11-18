/* ===== the explorer: orbit view, presets by panel, dials as ranges, the gate on the union, Lines ===== */
var MF=null;
function figureTris(cx,cy,z0){
  var prof=[[0,6.0],[1.2,6.0],[1.2,2.4],[10,2.3],[12,2.9],[17,3.3],[21,3.8],[22.4,2.5],[23,1.4],[23.6,1.8],[25.5,2.6],[27,2.5],[28,1.3],[28,0.01]],out=[],S=14,i,j;
  function pt(r,z,k){var a=k/S*2*PI;return [cx+r*Math.cos(a),cy+r*Math.sin(a),z0+z]}
  for(i=0;i<prof.length-1;i++){var m=prof[i][0]<1.2||i<3?'figbase':(prof[i][0]>=22.5?'figskin':'fig');
    for(j=0;j<S;j++){var a=pt(prof[i][1],prof[i][0],j),b=pt(prof[i][1],prof[i][0],j+1),c=pt(prof[i+1][1],prof[i+1][0],j+1),d=pt(prof[i+1][1],prof[i+1][0],j);
      out.push({p:[a,b,c],tag:m},{p:[a,c,d],tag:m})}}
  out.forEach(function(t){t.n=unit(cross(sub(t.p[1],t.p[0]),sub(t.p[2],t.p[0])));t.ang=0;t.kind='fig'});
  return out}
function plateTris(W,T,z){var out=[],x0=-T/2,x1=T/2,y0=-W/2,y1=W/2,q=function(a,b,c,d,n){out.push({p:[a,b,c],n:n,tag:'dirt',kind:'dirt',ang:0},{p:[a,c,d],n:n,tag:'dirt',kind:'dirt',ang:0})};
  q([x0,y0,z],[x1,y0,z],[x1,y1,z],[x0,y1,z],[0,0,1]);return out}
var LIGHT=unit([-0.55,-0.5,0.7]),FILL=unit([0.8,-0.3,0.25]);
function pack(r,g,b){r=Math.max(0,Math.min(255,r|0));g=Math.max(0,Math.min(255,g|0));b=Math.max(0,Math.min(255,b|0));return (255<<24)|(b<<16)|(g<<8)|r}
var MAT={stone:[150,154,164],scree:[135,138,146],core:[96,101,110],dirt:[150,121,92],fig:[78,98,150],figskin:[226,186,156],hl:[214,60,160],figbase:[58,58,64]};
function colorOf(t,mode,dark){
  var lam=Math.max(0,dot(t.n,LIGHT)),base=MAT[t.tag]||MAT.stone,j=1+(((t.id|0)*37%11)-5)*0.015,k;
  if(t.tag==='hl'){k=0.75+0.3*lam;return pack(base[0]*k,base[1]*k,base[2]*k)}
  if(t.tag==='fig'||t.tag==='figskin'||t.tag==='figbase'){k=0.45+0.7*lam;return pack(base[0]*k,base[1]*k,base[2]*k)}
  if(mode==='brush'&&t.tag!=='fig'){var up=Math.max(0,t.n[2]),lit=Math.min(1,Math.pow(up,2.2)*0.66+lam*0.12);var dk=[52,54,64],lt=[238,232,210];
    if(t.tag==='dirt'){dk=[70,50,36];lt=[150,112,76]}return pack((dk[0]+(lt[0]-dk[0])*lit)*j,(dk[1]+(lt[1]-dk[1])*lit)*j,(dk[2]+(lt[2]-dk[2])*lit)*j)}
  if(mode==='over'&&t.tag!=='dirt'){if(t.bridge&&t.ang>OVER+1e-6)return pack(70,125,215);if(t.ang>STEEP+1e-6)return pack(215,50,40);if(t.ang>OVER+1e-6)return pack(240,150,40)}
  var fill=Math.max(0,dot(t.n,FILL));k=(0.16+1.05*Math.pow(lam,1.25)+0.22*fill)*j;return pack(base[0]*k,base[1]*k,base[2]*k)}
function Cam(a,e){this.az=a;this.el=e;this.az0=a;this.el0=e;this.zoom=1}
/* the one orbit control every preview uses: drag right turns the model to the right (the camera goes round to the left), drag down looks from higher up; the wheel and a two-finger pinch zoom; a double click or tap resets the view */
function bindOrbit(cv,cam,redraw,onzoom){var drag=null,pts={},pd=0;
  function setZ(z){cam.zoom=Math.max(0.35,Math.min(4,z));if(onzoom)onzoom(cam.zoom);redraw()}
  cam.reset=function(){cam.az=cam.az0;cam.el=cam.el0;setZ(1)};
  /* a pointer is live only while it keeps reporting: a new primary touch means every earlier contact has ended, a lost capture, a leave or a cancel ends one, and a pinch needs two pointers that both moved in the last 300 ms, so a stale entry can never turn a one-finger drag into a pinch */
  function live(){var now=Date.now(),ids=Object.keys(pts).filter(function(k){return now-pts[k][2]<300});return ids}
  function begin(e){drag=[e.clientX,e.clientY,cam.az,cam.el]}
  cv.addEventListener('pointerdown',function(e){if(e.isPrimary)pts={};pts[e.pointerId]=[e.clientX,e.clientY,Date.now()];try{cv.setPointerCapture(e.pointerId)}catch(x){}var ids=Object.keys(pts);if(ids.length>=2){var a=pts[ids[0]],b=pts[ids[1]];pd=Math.hypot(a[0]-b[0],a[1]-b[1]);drag=null}else{pd=0;begin(e)}});
  cv.addEventListener('pointermove',function(e){if(!pts[e.pointerId])return;pts[e.pointerId]=[e.clientX,e.clientY,Date.now()];var lv=live();if(lv.length>=2){var a=pts[lv[0]],b=pts[lv[1]],d=Math.hypot(a[0]-b[0],a[1]-b[1]);if(pd>0&&d>0)setZ(cam.zoom*d/pd);pd=d;drag=null;return}pd=0;if(!drag)begin(e);cam.az=drag[2]-(e.clientX-drag[0])*0.4;cam.el=Math.max(5,Math.min(80,drag[3]+(e.clientY-drag[1])*0.3));redraw()});
  function up(e){delete pts[e.pointerId];var ids=Object.keys(pts);pd=0;drag=ids.length===1?[pts[ids[0]][0],pts[ids[0]][1],cam.az,cam.el]:null}
  ['pointerup','pointercancel','lostpointercapture','pointerleave'].forEach(function(n){cv.addEventListener(n,up)});
  cv.addEventListener('wheel',function(e){e.preventDefault();setZ(cam.zoom*Math.exp(-e.deltaY*0.0015))},{passive:false});
  cv.addEventListener('dblclick',function(){cam.reset()});
  cv.style.touchAction='none'}
Cam.prototype.basis=function(){var a=rad(this.az),e=rad(this.el),c=[Math.sin(a)*Math.cos(e),-Math.cos(a)*Math.cos(e),Math.sin(e)],r=[Math.cos(a),Math.sin(a),0];return {c:c,r:r,u:cross(c,r)}};
/* the editor's own ground and sky, drawn behind the model: the blue gradient of the editor's view (web/preview/style.css, #view), and a ground plane under the whole scene with a line at every square and every tenth (the editor's grid, tenths where it has fifths), anti-aliased in screen pixels, fading toward the horizon; the tile stands in front of it and it never hides the tile */
var SKY={light:{top:[93,159,202],mid:[155,203,226],hor:[227,239,240],cloud:[255,255,255,0.22],haze:[231,242,238,0.7],ink:[43,51,64]},dark:{top:[6,24,46],mid:[14,58,89],hor:[42,99,116],cloud:[139,205,218,0.09],haze:[46,105,116,0.48],ink:[199,227,236]}};
function isDarkTheme(){var t=document.documentElement.getAttribute('data-theme');if(t)return t==='dark';return !!(window.matchMedia&&window.matchMedia('(prefers-color-scheme: dark)').matches)}
/* the one projection every 3D view reads: perspective (a camera D in front of the centre, so the size at the centre is the same as the parallel view) or orthographic (parallel rays) */
var PROJ={persp:true};
function projOf(cam,H,s,center){var B=cam.basis(),D=3.2*H/(s/cam.zoom);return {persp:PROJ.persp,D:D,cp:[center[0]+B.c[0]*D,center[1]+B.c[1]*D,center[2]+B.c[2]*D]}}
function paintGround(buf,W,H,B,s,center,o){var K=SKY[o.dark?'dark':'light'],r=B.r,u=B.u,c=B.c,cz=c[2],x,y,cx=W/2,cy=H/2,ink=K.ink;
  var R0=1.2*o.size,R1=3*o.size,pr=o.pr;
  function hit(px,py){var xr=(px-cx)/s,yu2=(cy-py)/s;if(!pr.persp){var tz=(o.z-center[2]-r[2]*xr-u[2]*yu2)/cz;return [center[0]+r[0]*xr+u[0]*yu2+c[0]*tz,center[1]+r[1]*xr+u[1]*yu2+c[1]*tz]}
    var dz=-c[2]+r[2]*xr/pr.D+u[2]*yu2/pr.D;if(dz>-1e-6)return null;var t=(o.z-pr.cp[2])/dz;return [pr.cp[0]+t*(-c[0]+r[0]*xr/pr.D+u[0]*yu2/pr.D),pr.cp[1]+t*(-c[1]+r[1]*xr/pr.D+u[1]*yu2/pr.D)]}
  function ss(a,b,v){var t=Math.max(0,Math.min(1,(v-a)/(b-a)));return t*t*(3-2*t)}
  function ln(g,fw,w){var d=Math.abs(g-Math.round(g))/fw;return Math.max(0,Math.min(1,0.5*w+0.5-d))}
  var PXR=o.px||1,WM=1.6*PXR,Wm=0.9*PXR;
  for(y=0;y<H;y++){var t=y/(H-1),b0,b1,b2;if(t<0.56){var q=t/0.56;b0=K.top[0]+(K.mid[0]-K.top[0])*q;b1=K.top[1]+(K.mid[1]-K.top[1])*q;b2=K.top[2]+(K.mid[2]-K.top[2])*q}else{var q2=(t-0.56)/0.44;b0=K.mid[0]+(K.hor[0]-K.mid[0])*q2;b1=K.mid[1]+(K.hor[1]-K.mid[1])*q2;b2=K.mid[2]+(K.hor[2]-K.mid[2])*q2}
    var yu=(cy-(y+0.5))/s;
    for(x=0;x<W;x++){var R=b0,G=b1,Bl=b2,dx=(x/W-0.18)/0.48,dy=(y/H-0.55)/0.15,d=Math.sqrt(dx*dx+dy*dy),a=K.cloud[3]*Math.max(0,1-d/0.72);
      if(a>0){R+=(K.cloud[0]-R)*a;G+=(K.cloud[1]-G)*a;Bl+=(K.cloud[2]-Bl)*a}
      dx=(x/W-0.38)/0.75;dy=(y/H-1.08)/0.34;d=Math.sqrt(dx*dx+dy*dy);a=K.haze[3]*Math.max(0,1-d/0.72);if(a>0){R+=(K.haze[0]-R)*a;G+=(K.haze[1]-G)*a;Bl+=(K.haze[2]-Bl)*a}
      var h0=o.grid?hit(x+0.5,y+0.5):null;
      if(h0){var X=h0[0],Y=h0[1],h1=hit(x+1.5,y+0.5),h2=hit(x+0.5,y+1.5),fd=1;
        if(h1&&h2&&fd>0.002){var fwx=(Math.abs(h1[0]-X)+Math.abs(h2[0]-X))/o.S,fwy=(Math.abs(h1[1]-Y)+Math.abs(h2[1]-Y))/o.S,fm=Math.max(fwx,fwy,1e-9),gx=(X-o.ox)/o.S,gy=(Y-o.oy)/o.S,
          minorOn=Math.max(0,Math.min(1,(0.1/fm/PXR-4)/3)),majorOn=Math.max(0,Math.min(1,(1/fm/PXR-4)/3));
          var maj=Math.max(ln(gx,fwx,WM),ln(gy,fwy,WM))*0.45*majorOn,mn=minorOn>0?Math.max(ln(gx*10,fwx*10,Wm),ln(gy*10,fwy*10,Wm))*0.17*minorOn:0,al=Math.max(maj,mn)*fd;
          if(al>0.002){R+=(ink[0]-R)*al;G+=(ink[1]-G)*al;Bl+=(ink[2]-Bl)*al}}}
      buf[y*W+x]=(255<<24)|((Bl|0)<<16)|((G|0)<<8)|(R|0)}}}
function render(cv,tris,cam,scale,center,bg,mode,ground){
  var W=cv.width,H=cv.height,ctx=cv.getContext('2d'),img=ctx.createImageData(W,H),buf=new Uint32Array(img.data.buffer),zb=new Float32Array(W*H),B=cam.basis(),c=B.c,r=B.r,u=B.u,i;
  LIGHT=unit(add(add(mul(r,-0.5),mul(u,0.55)),mul(c,0.75)));FILL=unit(add(add(mul(r,0.8),mul(u,0.1)),mul(c,0.3)));
  zb.fill(-1e9);var s=scale*W,cx=W/2,cy=H/2,PR=projOf(cam,H,s,center),PD=PR.D,PP=PR.persp;if(ground){ground.pr=PR;paintGround(buf,W,H,B,s,center,ground)}else buf.fill(bg);
  function pj(p){var d=sub(p,center),dc=dot(d,c);if(!PP)return [dot(d,r)*s+cx,cy-dot(d,u)*s,dot(p,c)];var w=PD-dc;if(w<0.1*PD)return null;var k=PD/w;return [dot(d,r)*s*k+cx,cy-dot(d,u)*s*k,k]}
  for(i=0;i<tris.length;i++){var t=tris[i];var P=t.p;if(PP?dot(t.n,sub(PR.cp,P[0]))<=0.0001:dot(t.n,c)<=0.0001)continue;var V0=pj(P[0]),V1=pj(P[1]),V2=pj(P[2]);if(!V0||!V1||!V2)continue;var
    x0=V0[0],y0=V0[1],z0=V0[2],x1=V1[0],y1=V1[1],z1=V1[2],x2=V2[0],y2=V2[1],z2=V2[2];
    var mnx=Math.max(0,Math.floor(Math.min(x0,x1,x2))),mxx=Math.min(W-1,Math.ceil(Math.max(x0,x1,x2))),mny=Math.max(0,Math.floor(Math.min(y0,y1,y2))),mxy=Math.min(H-1,Math.ceil(Math.max(y0,y1,y2)));
    var den=(y1-y2)*(x0-x2)+(x2-x1)*(y0-y2);if(Math.abs(den)<1e-9)continue;
    var ck=mode+'|'+Math.round(cam.az)+'|'+Math.round(cam.el);if(t.col===undefined||t.cm!==ck)t.col=colorOf(t,mode),t.cm=ck;var col=t.col,x,y;
    for(y=mny;y<=mxy;y++)for(x=mnx;x<=mxx;x++){var px=x+0.5,py=y+0.5,l0=((y1-y2)*(px-x2)+(x2-x1)*(py-y2))/den,l1=((y2-y0)*(px-x2)+(x0-x2)*(py-y2))/den,l2=1-l0-l1;
      if(l0<-1e-4||l1<-1e-4||l2<-1e-4)continue;var z=l0*z0+l1*z1+l2*z2,idx=y*W+x;if(z>zb[idx]){zb[idx]=z;buf[idx]=col}}}
  function zz(v){return PP?-PD/v:v}
  if(mode==='shaded'||mode==='brush'){
    /* cavity shade, screen space: a pixel with nearer surface on its sides (a joint, a crack, a facet's corner) is darkened. It reads the depth buffer only; no geometry changes. */
    var K=[[4,0],[-4,0],[0,4],[0,-4],[3,3],[-3,-3],[3,-3],[-3,3]],R2=[[9,0],[-9,0],[0,9],[0,-9]],out=new Uint32Array(buf),x2,y2,k2,sc2=scale*W*0.0125;
    for(y2=9;y2<H-9;y2++)for(x2=9;x2<W-9;x2++){var id=y2*W+x2,z0=zb[id];if(z0<-1e8)continue;z0=zz(z0);var occ=0,n=0;
      for(k2=0;k2<8;k2++){var zn=zb[(y2+K[k2][1])*W+x2+K[k2][0]];if(zn>-1e8){zn=zz(zn);var dz=zn-z0;if(dz>0.15)occ+=Math.min(1,dz/(sc2*40+1e-6));n++}}
      for(k2=0;k2<4;k2++){var zn2=zb[(y2+R2[k2][1])*W+x2+R2[k2][0]];if(zn2>-1e8){zn2=zz(zn2);var dz2=zn2-z0;if(dz2>0.4)occ+=0.6*Math.min(1,dz2/(sc2*60+1e-6));n++}}
      if(n&&occ>0){var f=1-Math.min(0.55,0.55*occ/6),c=buf[id],r=(c&255)*f,g=((c>>8)&255)*f,b2=((c>>16)&255)*f;out[id]=(255<<24)|(b2<<16)|(g<<8)|r}}
    buf.set(out)}
  ctx.putImageData(img,0,0)}
/* ---- the gate on the union (Manifold), and the line floors on its slices ---- */
function unionOf(b,withFoot){
  /* L6: the join of what the cliff owns (its blocks, lean-to wedges, the core behind them and the ground under them). The foot's stones belong to the scene's scatter and join only when withFoot asks for the scene's join. */
  var Mf=MF.Manifold,i,t0=now(),h=hullsOf(b,withFoot),list=h.list,keep=list.slice();
  var t1=now(),u=Mf.union(list);u.numTri();var t2=now(),mesh=u.getMesh(),vp=mesh.vertProperties,np=mesh.numProp,tv=mesh.triVerts,tris=[];
  for(i=0;i<tv.length;i+=3){var a=[vp[tv[i]*np],vp[tv[i]*np+1],vp[tv[i]*np+2]],bb=[vp[tv[i+1]*np],vp[tv[i+1]*np+1],vp[tv[i+1]*np+2]],c=[vp[tv[i+2]*np],vp[tv[i+2]*np+1],vp[tv[i+2]*np+2]],n=unit(cross(sub(bb,a),sub(c,a)));
    tris.push({p:[a,bb,c],n:n,tag:'stone',kind:'union',ang:overAng(n),area:areaOf([a,bb,c])})}
  /* the float32 reload: a mesh is read back at 32 bits already (getMesh gives Float32Array), so watertight is asked of the manifold */
  var res={u:u,tris:tris,numTri:u.numTri(),genus:u.genus(),status:u.status(),tHull:h.ms,tUnion:t2-t1,tMesh:now()-t2,inputs:list.length};
  var top=0,side=0,down=0,bed=0,dA=0,ob=0,orr=0,A=0;
  tris.forEach(function(t){A+=t.area;if(t.p[0][2]<1e-4&&t.p[1][2]<1e-4&&t.p[2][2]<1e-4&&t.n[2]<-0.99){bed++;return}
    if(t.n[2]>0.3)top++;else if(t.n[2]<-0.3){down++;dA+=t.area;}else side++;
    if(t.n[2]<0&&t.ang>OVER+1e-6){if(t.ang>STEEP+1e-6)orr+=t.area;else ob+=t.area}});
  var downA=0;tris.forEach(function(t){if(t.n[2]<-1e-6&&!(t.p[0][2]<1e-4&&t.p[1][2]<1e-4&&t.p[2][2]<1e-4))downA+=t.area});
  res.split={top:top,side:side,under:down,bed:bed};res.downA=downA;res.band=ob;res.red=orr;res.bandPct=downA?100*ob/downA:0;res.redPct=downA?100*orr/downA:0;
  var parts=u.decompose();res.bodies=parts.filter(function(m){return m.volume()>1e-3}).length;res.voids=parts.length-res.bodies;parts.forEach(function(m){m.delete()});res.keep=keep;return res}
/* a wasm object is freed by hand: the union of a build, and the solids that went into it, once the next build replaces them (a page left open and played with must not fill the engine's memory) */
function freeGate(g){if(!g)return;try{if(g.u)g.u.delete()}catch(e){}try{if(g.keep)freeAll(g.keep)}catch(e){}g.u=null;g.keep=null}
function thinCheck(u,zmax){
  /* a part under one line: a piece of a slice that vanishes when eroded by half a line (0.225 mm). Knife edges of a joint are attached to a body and are counted as an area fraction instead. */
  var out={lostMax:0,lostFracMax:0,lostArea:0,gapArea:0,lostParts:0,gapParts:0,slices:0,t:0},t0=now(),z;
  for(z=DIRT+0.3;z<zmax;z+=0.6){
    var cs=u.slice(z);if(cs.isEmpty())continue;
    var A=cs.area(),er=cs.offset(-LINE/2,'Miter',2,8),op=er.offset(LINE/2,'Miter',2,8),lost=cs.subtract(op),
        cl=cs.offset(GAPMIN/2,'Miter',2,8).offset(-GAPMIN/2,'Miter',2,8),gap=cl.subtract(cs);
    var la=lost.area(),ga=gap.area();
    cs.decompose().forEach(function(c){var ca=c.area(),e2=c.offset(-LINE/2,'Miter',2,8),oa=e2.area();if(ca>0.05&&oa<1e-3){out.lostParts++;if(ca>out.lostMax)out.lostMax=ca}e2.delete();c.delete()});
    if(ga>0.02)gap.decompose().forEach(function(c){if(c.area()>0.15)out.gapParts++;c.delete()});
    if(A>0&&la/A>out.lostFracMax)out.lostFracMax=la/A;
    out.lostArea+=la;out.gapArea+=ga;out.slices++;[cs,er,op,lost,cl,gap].forEach(function(o){try{o.delete()}catch(e){}})}
  out.t=now()-t0;return out}

/* each course's front line, in plan (x, y), with the heights of the plane under it and over it: the same functions the build cuts to (GR.Gat with the front band on, today's straight front moved by the course's setback otherwise). The target draws it as a loft and the Region card draws it as thin lines, from this one place. */
function frontLoft(b){var g=b.g,out=[];if(!g.offs||!g.planes)return out;var f=g.f,band=typeof GR!=='undefined'&&b.p&&GR.bandOn(b.p);
  GR.frontEdges(g).forEach(function(ei){var E=g.edges[ei],lo=Math.min(E.a[0],E.b[0]),hi=Math.max(E.a[0],E.b[0]),c,nC=g.planes.length-1;
    for(c=0;c<nC;c++){var z0=g.planes[c].z,z1=g.planes[c+1].z,pts=[],x;if(z0>DIRT+g.topHi)break;
      for(x=lo;x<=hi+1e-6;x+=Math.max(0.6*f,(hi-lo)/80)){var xx=Math.min(hi,x);pts.push([xx,band?GR.Gat(b.p,g,c,ei,xx):GR.baseY(g,c,ei)])}
      out.push({c:c,z0:z0,z1:Math.min(z1,DIRT+g.topHi),pts:pts})}});return out}
/* the magenta target: the footprint, the squares and the crest envelope of a build, drawn over a canvas (the editor's Target switch and the step figures both call it) */
function drawTargetCore(cv,cam,b,sc,ce){var g=b.g,f=g.f,ctx=cv.getContext('2d'),B=cam.basis(),s=sc*cv.width,dk=isDarkTheme(),lw=Math.max(1,cv.width/640);
  var PR=projOf(cam,cv.height,s,ce);function P(x,y,z){var d=[x-ce[0],y-ce[1],z-ce[2]],k=PR.persp?PR.D/Math.max(0.05*PR.D,PR.D-(d[0]*B.c[0]+d[1]*B.c[1]+d[2]*B.c[2])):1;return [(d[0]*B.r[0]+d[1]*B.r[1]+d[2]*B.r[2])*s*k+cv.width/2,cv.height/2-(d[0]*B.u[0]+d[1]*B.u[1]+d[2]*B.u[2])*s*k]}
  var fp=g.fp,n=fp.length,col=dk?'255,122,224':'214,32,164',halo=dk?'6,24,46':'255,255,255',i,k;
  function line(pts,w,a,dash){ctx.setLineDash(dash||[]);[[halo,w+2.2*lw,0.55*a],[col,w,a]].forEach(function(q){ctx.beginPath();pts.forEach(function(p,j){if(j)ctx.lineTo(p[0],p[1]);else ctx.moveTo(p[0],p[1])});ctx.strokeStyle='rgba('+q[0]+','+q[2]+')';ctx.lineWidth=q[1];ctx.lineJoin='round';ctx.stroke()});ctx.setLineDash([])}
  function edgePts(a,bb,top){var L=Math.hypot(bb[0]-a[0],bb[1]-a[1]),m=Math.max(2,Math.ceil(L/(0.8*f))),o=[];for(var j=0;j<=m;j++){var t=j/m,x=a[0]+(bb[0]-a[0])*t,y=a[1]+(bb[1]-a[1])*t;o.push(P(x,y,top?g.crest(x):DIRT))}return o}
  /* the shell: each side of the footprint as a translucent wall from the ground to the crest, and the crest as lines across the top */
  ctx.save();
  for(i=0;i<n;i++){var a=fp[i],bb=fp[(i+1)%n],lo=edgePts(a,bb,false),hi=edgePts(a,bb,true);ctx.beginPath();lo.forEach(function(p,j){j?ctx.lineTo(p[0],p[1]):ctx.moveTo(p[0],p[1])});hi.slice().reverse().forEach(function(p){ctx.lineTo(p[0],p[1])});ctx.closePath();ctx.fillStyle='rgba('+col+',0.13)';ctx.fill()}
  var ys=fp.map(function(q){return q[1]}),y0=Math.min.apply(null,ys),y1=Math.max.apply(null,ys),xs=fp.map(function(q){return q[0]}),x0=Math.min.apply(null,xs),x1=Math.max.apply(null,xs);
  for(k=1;k<4;k++){var yy=y0+(y1-y0)*k/4,cr=[];for(i=0;i<n;i++){var a2=fp[i],b2=fp[(i+1)%n];if((a2[1]<=yy)!==(b2[1]<=yy))cr.push(a2[0]+(b2[0]-a2[0])*(yy-a2[1])/(b2[1]-a2[1]))}cr.sort(function(p,q){return p-q});for(i=0;i+1<cr.length;i+=2){var pts=[];for(var x=cr[i];x<=cr[i+1]+1e-6;x+=Math.max(0.8*f,(cr[i+1]-cr[i])/60))pts.push(P(x,yy,g.crest(x)));pts.push(P(cr[i+1],yy,g.crest(cr[i+1])));line(pts,0.8*lw,0.45)}}
  for(i=0;i<n;i++){line(edgePts(fp[i],fp[(i+1)%n],true),1.8*lw,0.95);line([P(fp[i][0],fp[i][1],DIRT),P(fp[i][0],fp[i][1],g.crest(fp[i][0]))],1.2*lw,0.8)}
  /* the front, course by course: with the front band on, each course's front edge is its own graded line between today's front and the outer front (the same functions the build cuts to: GR.Gat), drawn as a stepped loft from the ground up to the crest; with the band off (depth 0) there is nothing here and the target is the footprint's wall as before */
  if(typeof GR!=='undefined'&&b.p&&GR.bandOn(b.p)){frontLoft(b).forEach(function(L){var p0=L.pts.map(function(q){return P(q[0],q[1],L.z0)}),p1=L.pts.map(function(q){return P(q[0],q[1],L.z1)});
      ctx.beginPath();p0.forEach(function(q,j){j?ctx.lineTo(q[0],q[1]):ctx.moveTo(q[0],q[1])});p1.slice().reverse().forEach(function(q){ctx.lineTo(q[0],q[1])});ctx.closePath();ctx.fillStyle='rgba('+col+',0.10)';ctx.fill();
      line(p1,1.1*lw,0.6)})}
  /* the footprint on the ground, and the tile's squares */
  for(i=0;i<n;i++)line(edgePts(fp[i],fp[(i+1)%n],false),2.4*lw,1);
  var sq=SQUARE*f,xa=g.x0,xb=g.x1,ya=-g.W/2,yb=g.W/2;for(var xx=xa;xx<=xb+1e-6;xx+=sq)line([P(xx,ya,DIRT),P(xx,yb,DIRT)],1*lw,0.7,[6*lw,5*lw]);for(var yy2=ya;yy2<=yb+1e-6;yy2+=sq)line([P(xa,yy2,DIRT),P(xb,yy2,DIRT)],1*lw,0.7,[6*lw,5*lw]);
  ctx.restore()}

/* the row planes of a build, drawn over a canvas (the editor's Rows switch and the step figures both call it) */
function drawRowsCore(cv,cam,b,sc,ce){var g=b.g,f=g.f,L=b.l1,ctx=cv.getContext('2d'),B=cam.basis(),s=sc*cv.width,dk=isDarkTheme(),lw=Math.max(1,cv.width/640);
    if(!L||!L.planes)return;var PR=projOf(cam,cv.height,s,ce);
    function P(x,y,z){var d=[x-ce[0],y-ce[1],z-ce[2]],k=PR.persp?PR.D/Math.max(0.05*PR.D,PR.D-(d[0]*B.c[0]+d[1]*B.c[1]+d[2]*B.c[2])):1;return [(d[0]*B.r[0]+d[1]*B.r[1]+d[2]*B.r[2])*s*k+cv.width/2,cv.height/2-(d[0]*B.u[0]+d[1]*B.u[1]+d[2]*B.u[2])*s*k]}
    var col=dk?'80,230,210':'0,128,120',halo=dk?'6,24,46':'255,255,255',fp=g.fp,n=fp.length;
    var xs=fp.map(function(q){return q[0]}),ys=fp.map(function(q){return q[1]}),X0=Math.min.apply(null,xs),X1=Math.max.apply(null,xs),Y0=Math.min.apply(null,ys),Y1=Math.max.apply(null,ys),st=2.5*f;
    function Z(pl,x,y){return pl.z+planeOff(pl,x,y)}
    function stroke(pts,w,a,closed){[[halo,w+2.2*lw,a*0.45],[col,w,a]].forEach(function(q){ctx.beginPath();pts.forEach(function(p,j){j?ctx.lineTo(p[0],p[1]):ctx.moveTo(p[0],p[1])});if(closed)ctx.closePath();ctx.strokeStyle='rgba('+q[0]+','+q[2]+')';ctx.lineWidth=q[1];ctx.lineJoin='round';ctx.stroke()})}
    L.planes.forEach(function(pl,ci){var pts=[],i,m=fp.length;
      for(i=0;i<m;i++){var a=fp[i],b2=fp[(i+1)%m],l=Math.hypot(b2[0]-a[0],b2[1]-a[1]),nn=Math.max(1,Math.ceil(l/st)),t;for(t=0;t<nn;t++){var x=a[0]+(b2[0]-a[0])*t/nn,y=a[1]+(b2[1]-a[1])*t/nn;pts.push(P(x,y,Z(pl,x,y)))}}
      ctx.beginPath();pts.forEach(function(p,j){j?ctx.lineTo(p[0],p[1]):ctx.moveTo(p[0],p[1])});ctx.closePath();ctx.fillStyle='rgba('+col+',0.06)';ctx.fill();
      stroke(pts,1.7*lw,0.95,true);
      /* the surface: lines across it, so a tilt or a wave can be read */
      var gs=9*f,x,y;
      for(x=X0+gs/2;x<X1;x+=gs){var seg=[];for(y=Y0;y<=Y1+1e-6;y+=st){if(inPolyAny(fp,x,y))seg.push(P(x,y,Z(pl,x,y)));else{if(seg.length>1)stroke(seg,0.7*lw,0.35);seg=[]}}if(seg.length>1)stroke(seg,0.7*lw,0.35)}
      for(y=Y0+gs/2;y<Y1;y+=gs){var seg2=[];for(x=X0;x<=X1+1e-6;x+=st){if(inPolyAny(fp,x,y))seg2.push(P(x,y,Z(pl,x,y)));else{if(seg2.length>1)stroke(seg2,0.7*lw,0.35);seg2=[]}}if(seg2.length>1)stroke(seg2,0.7*lw,0.35)}
    })}

/* a bridge: a downward face under a bed that spans a gap between its supports, inside the hull the bed bears on (step 8). The gate still counts it; the viewer paints it blue, and bridgeStats below measures it apart. The one place that decides it (the sweep calls this too). */
var BRIDGE_BAND=1.6;   /* mm: a downward face counts as the bridge's if it is this near the bed's height */
/* the gate's downward areas with the bridge faces counted apart: the same 55 and 65 degree budgets, over what is left when the bridged faces are set aside (the gate itself counts them; this is shown beside it, never instead) */
function bridgeStats(tris){var down=0,red=0,band=0,bridged=0,bred=0,bband=0;
  tris.forEach(function(t){if(!(t.n[2]<-1e-6)||(t.p[0][2]<1e-4&&t.p[1][2]<1e-4&&t.p[2][2]<1e-4))return;var a=t.area,r=t.ang>STEEP,b=t.ang>OVER&&t.ang<=STEEP;down+=a;if(r)red+=a;if(b)band+=a;if(t.bridge){bridged+=a;if(r)bred+=a;if(b)bband+=a}});
  var d2=down-bridged,rp=d2?100*(red-bred)/d2:0,bp=d2?100*(band-bband)/d2:0;return {down:down,bridged:bridged,redApart:rp,bandApart:bp,pass:rp<=0.1&&bp<=0.1}}
function markBridges(tris,br){if(!br||!br.length)return 0;var n=0;
  tris.forEach(function(t){t.bridge=false;if(t.ang<=OVER||t.n[2]>=0)return;var cx=(t.p[0][0]+t.p[1][0]+t.p[2][0])/3,cy=(t.p[0][1]+t.p[1][1]+t.p[2][1])/3,cz=(t.p[0][2]+t.p[1][2]+t.p[2][2])/3,k,B;
    for(k=0;k<br.length;k++){B=br[k];if(Math.abs(cz-B.zb)>BRIDGE_BAND)continue;if(B.rings.some(function(R){return inPolyAny(R,cx,cy)})){t.bridge=true;n++;return}}});return n}
