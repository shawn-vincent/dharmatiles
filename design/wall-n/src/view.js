(function(){
var T=THREE,GEN=window.GEN;
var CAM={az:35,el:27,dist:138,fov:27,tx:19,ty:21,tz:15};   /* the picture's camera: tuned by eye against O */
(location.search.match(/[?&](az|el|dist|fov|tx|ty|tz)=(-?[\d.]+)/g)||[]).forEach(function(m){var k=m.slice(1).split('=');CAM[k[0]]=+k[1]});
var camNow=Object.assign({},CAM);
function cssv(n){return getComputedStyle(document.documentElement).getPropertyValue(n).trim()}

/* cavity darkening: each triangle's centroid shoots cosine-weighted rays into the hemisphere; the share that hit the mesh within AOR mm darkens it. A BVH keeps it fast. */
function bakeAO(pos,nor,col){
  var N=pos.length/9,AOR=+(/aor=([\d.]+)/.exec(location.search)||[0,5])[1],NR=+(/aon=(\d+)/.exec(location.search)||[0,24])[1],STR=+(/aos=([\d.]+)/.exec(location.search)||[0,.65])[1];
  var cen=new Float32Array(N*3),idx=[];
  for(var i=0;i<N;i++){cen[i*3]=(pos[i*9]+pos[i*9+3]+pos[i*9+6])/3;cen[i*3+1]=(pos[i*9+1]+pos[i*9+4]+pos[i*9+7])/3;cen[i*3+2]=(pos[i*9+2]+pos[i*9+5]+pos[i*9+8])/3;idx.push(i)}
  var nodes=[];
  function bb(ids){var b=[1e9,1e9,1e9,-1e9,-1e9,-1e9];ids.forEach(function(i){for(var v=0;v<3;v++)for(var a=0;a<3;a++){var x=pos[i*9+v*3+a];if(x<b[a])b[a]=x;if(x>b[a+3])b[a+3]=x}});return b}
  function build(ids){var b=bb(ids),node={b:b};
    if(ids.length<=4){node.t=ids;return node}
    var ax=0,e=[b[3]-b[0],b[4]-b[1],b[5]-b[2]];if(e[1]>e[ax])ax=1;if(e[2]>e[ax])ax=2;
    ids.sort(function(p,q){return cen[p*3+ax]-cen[q*3+ax]});var m=ids.length>>1;
    node.l=build(ids.slice(0,m));node.r=build(ids.slice(m));return node}
  var root=build(idx);
  function hitBox(b,o,inv,tmax){var t0=0,t1=tmax;for(var a=0;a<3;a++){var ta=(b[a]-o[a])*inv[a],tb=(b[a+3]-o[a])*inv[a];if(ta>tb){var t=ta;ta=tb;tb=t}if(ta>t0)t0=ta;if(tb<t1)t1=tb;if(t0>t1)return false}return true}
  function tri(i,o,d,tmax){var p=i*9,ax=pos[p+3]-pos[p],ay=pos[p+4]-pos[p+1],az=pos[p+5]-pos[p+2],bx=pos[p+6]-pos[p],by=pos[p+7]-pos[p+1],bz=pos[p+8]-pos[p+2];
    var px=d[1]*bz-d[2]*by,py=d[2]*bx-d[0]*bz,pz=d[0]*by-d[1]*bx,det=ax*px+ay*py+az*pz;if(Math.abs(det)<1e-9)return false;var id=1/det;
    var tx=o[0]-pos[p],ty=o[1]-pos[p+1],tz=o[2]-pos[p+2],u=(tx*px+ty*py+tz*pz)*id;if(u<0||u>1)return false;
    var qx=ty*az-tz*ay,qy=tz*ax-tx*az,qz=tx*ay-ty*ax,v=(d[0]*qx+d[1]*qy+d[2]*qz)*id;if(v<0||u+v>1)return false;
    var t=(bx*qx+by*qy+bz*qz)*id;return t>1e-3&&t<tmax}
  function any(n,o,d,inv,tmax){if(!hitBox(n.b,o,inv,tmax))return false;if(n.t){for(var k=0;k<n.t.length;k++)if(tri(n.t[k],o,d,tmax))return true;return false}return any(n.l,o,d,inv,tmax)||any(n.r,o,d,inv,tmax)}
  var rr=GEN.rng(12345);
  for(var i=0;i<N;i++){
    var n=[nor[i*9],nor[i*9+1],nor[i*9+2]],t1=Math.abs(n[2])<.9?[0,0,1]:[1,0,0],u=[n[1]*t1[2]-n[2]*t1[1],n[2]*t1[0]-n[0]*t1[2],n[0]*t1[1]-n[1]*t1[0]],ul=Math.hypot(u[0],u[1],u[2]);u=[u[0]/ul,u[1]/ul,u[2]/ul];
    var v=[n[1]*u[2]-n[2]*u[1],n[2]*u[0]-n[0]*u[2],n[0]*u[1]-n[1]*u[0]],o=[cen[i*3]+n[0]*.02,cen[i*3+1]+n[1]*.02,cen[i*3+2]+n[2]*.02],hit=0;
    for(var k=0;k<NR;k++){var r1=Math.sqrt((k+rr())/NR),ph=6.2832*rr(),x=r1*Math.cos(ph),y=r1*Math.sin(ph),z=Math.sqrt(Math.max(0,1-r1*r1));
      var d=[u[0]*x+v[0]*y+n[0]*z,u[1]*x+v[1]*y+n[1]*z,u[2]*x+v[2]*y+n[2]*z],inv=[1/(d[0]||1e-9),1/(d[1]||1e-9),1/(d[2]||1e-9)];
      if(any(root,o,d,inv,AOR))hit++}
    var f=1-STR*hit/NR;for(var c=0;c<9;c++)col[i*9+c]*=f}}
function sceneOf(w,opt){
  opt=opt||{};
  var sc=new T.Scene(),pos=[],col=[],nor=[];
  var RUB=false;var CORE=false;function tri(a,b,c,n,k){var cx=(b[1]-a[1])*(c[2]-a[2])-(b[2]-a[2])*(c[1]-a[1]),cy=(b[2]-a[2])*(c[0]-a[0])-(b[0]-a[0])*(c[2]-a[2]),cz=(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);if(cx*n[0]+cy*n[1]+cz*n[2]<0){var t=b;b=c;c=t}pos.push(a[0],a[1],a[2],b[0],b[1],b[2],c[0],c[1],c[2]);for(var i=0;i<3;i++){nor.push(n[0],n[1],n[2]);col.push(k,k,k)}}
  var r=GEN.rng(w.seed*31+5);
  w.blocks.forEach(function(b){var nf=/nofam=(\d)/.exec(location.search);if(nf&&b.fam==+nf[1]&&!b.rub)return;
    var bt=(b.rub?.55:.4+.06*r())*1.45;
    RUB=!!b.rub;b.faces.forEach(function(f){
      var p=f.poly,tone=bt*(0.94+0.12*r());if(/famdbg=1/.test(location.search))tone=[.25,.9,.5][b.fam||0];if(/dbg=1/.test(location.search)&&!b.rub)tone=f.top?.9:.12;
      if(opt.stage&&opt.stage<=3)tone=.5+.4*((b.row*.37+(b.id||0)*.11)%1);
      var zc=0;p.forEach(function(q){zc+=q[2]});zc/=p.length;tone*=1-.13*Math.max(0,1-zc/22);
      if(f.tris){f.tris.forEach(function(T3){var ux=T3[1][0]-T3[0][0],uy=T3[1][1]-T3[0][1],uz=T3[1][2]-T3[0][2],vx=T3[2][0]-T3[0][0],vy=T3[2][1]-T3[0][1],vz=T3[2][2]-T3[0][2],nn=[uy*vz-uz*vy,uz*vx-ux*vz,ux*vy-uy*vx],l=Math.hypot(nn[0],nn[1],nn[2])||1;nn=[nn[0]/l,nn[1]/l,nn[2]/l];if(nn[0]*f.n[0]+nn[1]*f.n[1]+nn[2]*f.n[2]<0)nn=[-nn[0],-nn[1],-nn[2]];tri(T3[0],T3[1],T3[2],nn,tone*(.72+.56*r()))})}else{for(var i=1;i<p.length-1;i++)tri(p[0],p[i],p[i+1],f.n,tone)}})});
  (w.core||[]).forEach(function(f){var p=f.poly;for(var i=1;i<p.length-1;i++)tri(p[0],p[i],p[i+1],f.n,.07)});
  if(!/noao=1/.test(location.search))bakeAO(pos,nor,col);
  var g=new T.BufferGeometry();
  g.setAttribute('position',new T.Float32BufferAttribute(pos,3));g.setAttribute('normal',new T.Float32BufferAttribute(nor,3));g.setAttribute('color',new T.Float32BufferAttribute(col,3));
  var m=new T.Mesh(g,new T.MeshLambertMaterial({vertexColors:true}));sc.add(m);
  
  var plate=new T.Mesh(new T.BoxGeometry(35,35,3),new T.MeshLambertMaterial({color:0x8a8a8e}));plate.position.set(17.5,17.5,-1.5);sc.add(plate);
  if(opt.figure!==false){
    var fm=new T.MeshLambertMaterial({color:0xb4b4b8}),fg=new T.Group();
    var base=new T.Mesh(new T.CylinderGeometry(8,8,1.5,24),new T.MeshLambertMaterial({color:0x55555a}));base.rotation.x=Math.PI/2;base.position.set(0,0,.75);fg.add(base);
    var body=new T.Mesh(new T.CylinderGeometry(3.6,4.4,19,14),fm);body.rotation.x=Math.PI/2;body.position.set(0,0,11);fg.add(body);
    var head=new T.Mesh(new T.SphereGeometry(3,14,10),fm);head.position.set(0,0,24.9);fg.add(head);
    fg.position.set(3,2,0);fg.scale.set(.8,.8,.8);sc.add(fg)}
  sc.add(new T.HemisphereLight(0xffffff,0x6e6e72,.8));
  var d=new T.DirectionalLight(0xffffff,.9);d.position.set(-40,-90,120);sc.add(d);
  var d2=new T.DirectionalLight(0xffffff,.22);d2.position.set(80,-20,30);sc.add(d2);
  return sc}
function camOf(c,aspect){
  var cam=new T.PerspectiveCamera(c.fov,aspect,1,800);cam.up.set(0,0,1);
  var a=c.az*Math.PI/180,e=c.el*Math.PI/180;
  cam.position.set(c.tx+Math.sin(a)*Math.cos(e)*c.dist,c.ty-Math.cos(a)*Math.cos(e)*c.dist,c.tz+Math.sin(e)*c.dist);cam.lookAt(c.tx,c.ty,c.tz);return cam}
var live=new T.WebGLRenderer({canvas:document.getElementById('live'),antialias:true,preserveDrawingBuffer:true});
var off=new T.WebGLRenderer({antialias:true,preserveDrawingBuffer:true});
function bg(){return new T.Color('#74747a')}
function shoot(w,opt,W,H){
  off.setPixelRatio(1);off.setSize(W,H);off.setClearColor(bg());
  off.render(sceneOf(w,opt),camOf(CAM,W/H));return off.domElement.toDataURL('image/jpeg',.9)}
var cur,seed=1;
function drawLive(){
  var cv=document.getElementById('live'),w=cv.clientWidth,h=cv.clientHeight;
  live.setPixelRatio(Math.min(2,devicePixelRatio||1));live.setSize(w,h,false);live.setClearColor(bg());
  live.render(sceneOf(cur,{stage:6}),camOf(camNow,w/h))}
function rebuild(){
  var t=performance.now();cur=GEN.build(seed,6);
  document.getElementById('ms').textContent=Math.round(performance.now()-t)+' ms, '+cur.blocks.length+' blocks, '+cur.blocks.reduce(function(s,b){return s+b.faces.length},0)+' faces';
  document.getElementById('seedv').value=seed;drawLive()}
function strip(){
  var el=document.getElementById('strip');el.innerHTML='';
  [seed,seed+1,seed+2,seed+3].forEach(function(s){var f=document.createElement('figure'),i=new Image();i.alt='seed '+s;i.src=shoot(GEN.build(s,6),{stage:6},420,360);var c=document.createElement('figcaption');c.textContent='seed '+s;f.appendChild(i);f.appendChild(c);el.appendChild(f)})}
function steps(){
  document.querySelectorAll('[data-stage]').forEach(function(img){var u=+img.getAttribute('data-stage');img.src=shoot(GEN.build(seed,u),{stage:u,figure:u>=5},520,420)})}
function all(){rebuild();strip();steps()}
window.addEventListener('resize',function(){drawLive()});
document.getElementById('match').onclick=function(){camNow=Object.assign({},CAM);drawLive()};
document.getElementById('seedv').onchange=function(){seed=Math.max(1,parseInt(this.value)||1);all()};
document.getElementById('nxt').onclick=function(){seed++;all()};
document.getElementById('prv').onclick=function(){seed=Math.max(1,seed-1);all()};
var cv=document.getElementById('live'),dr=null;
cv.addEventListener('pointerdown',function(e){dr=[e.clientX,e.clientY];cv.setPointerCapture(e.pointerId)});
cv.addEventListener('pointermove',function(e){if(!dr)return;camNow.az-=(e.clientX-dr[0])*.4;camNow.el=Math.max(2,Math.min(85,camNow.el+(e.clientY-dr[1])*.4));dr=[e.clientX,e.clientY];drawLive()});
cv.addEventListener('pointerup',function(){dr=null});
cv.addEventListener('wheel',function(e){e.preventDefault();camNow.dist=Math.max(60,Math.min(300,camNow.dist*(1+e.deltaY*.001)));drawLive()},{passive:false});
if(matchMedia)matchMedia('(prefers-color-scheme: dark)').addEventListener('change',function(){setTimeout(all,50)});
new MutationObserver(function(){setTimeout(all,50)}).observe(document.documentElement,{attributes:true,attributeFilter:['data-theme']});
var q=/[?&]seed=(\d+)/.exec(location.search);if(q)seed=+q[1];
all();
})();
