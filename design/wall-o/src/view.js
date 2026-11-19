(function(){
var T=THREE,GEN=window.GEN;
var GREY=2.0;
var CAM={az:-22,el:40,dist:146,fov:27,tx:12,ty:18,tz:16};   /* the picture's camera: tuned by eye against O */
(location.search.match(/[?&](az|el|dist)=(-?[\d.]+)/g)||[]).forEach(function(m){var k=m.slice(1).split('=');CAM[k[0]]=+k[1]});
var camNow=Object.assign({},CAM);
function cssv(n){return getComputedStyle(document.documentElement).getPropertyValue(n).trim()}
function sceneOf(w,opt){
  opt=opt||{};
  var sc=new T.Scene(),pos=[],col=[],nor=[];
  function tri(a,b,c,n,k){var cx=(b[1]-a[1])*(c[2]-a[2])-(b[2]-a[2])*(c[1]-a[1]),cy=(b[2]-a[2])*(c[0]-a[0])-(b[0]-a[0])*(c[2]-a[2]),cz=(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);if(cx*n[0]+cy*n[1]+cz*n[2]<0){var t=b;b=c;c=t}pos.push(a[0],a[1],a[2],b[0],b[1],b[2],c[0],c[1],c[2]);for(var i=0;i<3;i++){nor.push(n[0],n[1],n[2]);col.push(k*GREY,k*GREY,k*GREY*1.02)}}
  var r=GEN.rng(w.seed*31+5);
  w.blocks.forEach(function(b){
    var bt=(.27+.05*r())*(b.k||1);
    b.faces.forEach(function(f){
      var p=f.poly,tone=bt*(0.94+0.12*r());
      if(opt.stage&&opt.stage<=2)tone=.55+.35*((b.m*.37+b.row*.21)%1);
      var zc=0;p.forEach(function(q){zc+=q[2]});zc/=p.length;tone*=1-.13*Math.max(0,1-zc/22);
      if(f.tris){f.tris.forEach(function(T3){var ux=T3[1][0]-T3[0][0],uy=T3[1][1]-T3[0][1],uz=T3[1][2]-T3[0][2],vx=T3[2][0]-T3[0][0],vy=T3[2][1]-T3[0][1],vz=T3[2][2]-T3[0][2],nn=[uy*vz-uz*vy,uz*vx-ux*vz,ux*vy-uy*vx],l=Math.hypot(nn[0],nn[1],nn[2])||1;nn=[nn[0]/l,nn[1]/l,nn[2]/l];if(nn[0]*f.n[0]+nn[1]*f.n[1]+nn[2]*f.n[2]<0)nn=[-nn[0],-nn[1],-nn[2]];if(T3.nn)nn=T3.nn;tri(T3[0],T3[1],T3[2],nn,tone*((T3.k||(.92+.16*r()))*Math.max(.45,1+.35*Math.max(0,nn[2])+1.1*((nn[0]*-.33+nn[1]*-.74+nn[2]*.58)-(f.n[0]*-.33+f.n[1]*-.74+f.n[2]*.58)))))})}else{for(var i=1;i<p.length-1;i++)tri(p[0],p[i],p[i+1],f.n,tone)}})});
  if(opt.ao!==false){var plateT=[];[[0,0,0,35,0,0,35,35,0],[0,0,0,35,35,0,0,35,0]].forEach(function(q){plateT.push(q)});
    var ao=bakeAO(pos,nor,plateT,9),amt=opt.aoAmt||1.15;for(var vi=0;vi<ao.length;vi++){var f2=Math.pow(Math.max(0,ao[vi]),amt)*1.0;f2=.35+.65*f2*f2*1.0;col[vi*3]*=f2;col[vi*3+1]*=f2;col[vi*3+2]*=f2}}
  var g=new T.BufferGeometry();
  g.setAttribute('position',new T.Float32BufferAttribute(pos,3));g.setAttribute('normal',new T.Float32BufferAttribute(nor,3));g.setAttribute('color',new T.Float32BufferAttribute(col,3));
  var m=new T.Mesh(g,new T.MeshLambertMaterial({vertexColors:true,side:T.DoubleSide}));m.castShadow=true;m.receiveShadow=true;sc.add(m);
  [].forEach(function(c){var k=new T.Mesh(new T.BoxGeometry(c[1]-c[0]-.4,6,c[2]),new T.MeshLambertMaterial({color:0x6e7078}));k.position.set((c[0]+c[1])/2,31.2+3,c[2]/2);sc.add(k)});
  var plate=new T.Mesh(new T.BoxGeometry(35,35,3),new T.MeshLambertMaterial({color:0x9a9a9e}));plate.position.set(17.5,17.5,-1.5);plate.receiveShadow=true;sc.add(plate);
  if(opt.figure!==false){
    var fm=new T.MeshLambertMaterial({color:0xb4b4b8}),fg=new T.Group();
    var base=new T.Mesh(new T.CylinderGeometry(8,8,1.5,24),new T.MeshLambertMaterial({color:0x7a7a7e}));base.rotation.x=Math.PI/2;base.position.set(0,0,.75);fg.add(base);
    var body=new T.Mesh(new T.CylinderGeometry(3.6,4.4,19,14),fm);body.rotation.x=Math.PI/2;body.position.set(0,0,11);fg.add(body);
    var head=new T.Mesh(new T.SphereGeometry(3,14,10),fm);head.position.set(0,0,24.9);fg.add(head);
    fg.position.set(-7,4,0);sc.add(fg)}
  /* unpainted light grey, like the resin: a sky/ground fill, a soft key from above-front-left, and cavity darkening made from several weak shadow-casting lights spread over the upper hemisphere */
  sc.add(new T.HemisphereLight(0xf2f4f8,0x8a8a90,.62));
  var d=new T.DirectionalLight(0xffffff,.78);d.position.set(-40,-90,85);d.target.position.set(18,20,20);sc.add(d.target);d.castShadow=true;d.shadow.mapSize.set(2048,2048);var sh=d.shadow.camera;sh.left=-45;sh.right=45;sh.top=45;sh.bottom=-45;sh.near=10;sh.far=250;d.shadow.bias=-.0004;d.shadow.normalBias=.25;d.shadow.radius=3;sc.add(d);
  return sc}

/* cavity darkening: ambient occlusion baked per vertex by casting rays over the hemisphere against the wall's own triangles (a uniform grid keeps it fast) */
var AODIR=(function(){var a=[],n=14;for(var i=0;i<n;i++){var u=(i+.5)/n,v=(i*.618034)%1,r=Math.sqrt(u),ph=6.2832*v;a.push([r*Math.cos(ph),r*Math.sin(ph),Math.sqrt(1-u)])}return a})();
function bakeAO(pos,nor,extra,maxd){
  var nt=pos.length/9,CS=3,mn=[1e9,1e9,1e9],mx=[-1e9,-1e9,-1e9];
  var tris=[];for(var t=0;t<nt;t++)tris.push(pos.slice(t*9,t*9+9));extra.forEach(function(e){tris.push(e)});
  tris.forEach(function(q){for(var v=0;v<3;v++)for(var k=0;k<3;k++){mn[k]=Math.min(mn[k],q[v*3+k]);mx[k]=Math.max(mx[k],q[v*3+k])}});
  var D=[0,1,2].map(function(k){return Math.ceil((mx[k]-mn[k])/CS)+1}),grid=new Map();
  function key(ix,iy,iz){return(ix*D[1]+iy)*D[2]+iz}
  tris.forEach(function(q,ti){var lo=[0,1,2].map(function(k){return Math.floor((Math.min(q[k],q[3+k],q[6+k])-mn[k])/CS)}),hi=[0,1,2].map(function(k){return Math.floor((Math.max(q[k],q[3+k],q[6+k])-mn[k])/CS)});
    for(var x=lo[0];x<=hi[0];x++)for(var y=lo[1];y<=hi[1];y++)for(var z=lo[2];z<=hi[2];z++){var kk=key(x,y,z),l=grid.get(kk);if(!l){l=[];grid.set(kk,l)}l.push(ti)}});
  var stamp=new Int32Array(tris.length),sid=0;
  function hit(o,d,tmax){sid++;var best=1e9;
    for(var s=0;s<tmax;s+=CS*.5){var ix=Math.floor((o[0]+d[0]*s-mn[0])/CS),iy=Math.floor((o[1]+d[1]*s-mn[1])/CS),iz=Math.floor((o[2]+d[2]*s-mn[2])/CS);if(ix<0||iy<0||iz<0||ix>=D[0]||iy>=D[1]||iz>=D[2])continue;
      var l=grid.get(key(ix,iy,iz));if(!l)continue;
      for(var a=0;a<l.length;a++){var ti=l[a];if(stamp[ti]===sid)continue;stamp[ti]=sid;var q=tris[ti];
        var e1x=q[3]-q[0],e1y=q[4]-q[1],e1z=q[5]-q[2],e2x=q[6]-q[0],e2y=q[7]-q[1],e2z=q[8]-q[2],px=d[1]*e2z-d[2]*e2y,py=d[2]*e2x-d[0]*e2z,pz=d[0]*e2y-d[1]*e2x,det=e1x*px+e1y*py+e1z*pz;if(Math.abs(det)<1e-9)continue;var id=1/det,tx=o[0]-q[0],ty=o[1]-q[1],tz=o[2]-q[2],u=(tx*px+ty*py+tz*pz)*id;if(u<0||u>1)continue;var qx=ty*e1z-tz*e1y,qy=tz*e1x-tx*e1z,qz=tx*e1y-ty*e1x,v=(d[0]*qx+d[1]*qy+d[2]*qz)*id;if(v<0||u+v>1)continue;var tt=(e2x*qx+e2y*qy+e2z*qz)*id;if(tt>.02&&tt<best)best=tt}
      if(best<s+CS)break}
    return best}
  var out=new Float32Array(nt*3),cache=new Map();
  for(var t=0;t<nt;t++)for(var v=0;v<3;v++){var p=[pos[t*9+v*3],pos[t*9+v*3+1],pos[t*9+v*3+2]],n=[nor[t*9+v*3],nor[t*9+v*3+1],nor[t*9+v*3+2]],kk=p.map(function(x){return Math.round(x*2)}).concat(n.map(function(x){return Math.round(x*10)})).join(',');
    var c=cache.get(kk);if(c===undefined){
      var tt=Math.abs(n[2])<.9?[0,0,1]:[1,0,0],ux=n[1]*tt[2]-n[2]*tt[1],uy=n[2]*tt[0]-n[0]*tt[2],uz=n[0]*tt[1]-n[1]*tt[0],ul=Math.hypot(ux,uy,uz);ux/=ul;uy/=ul;uz/=ul;var vx=n[1]*uz-n[2]*uy,vy=n[2]*ux-n[0]*uz,vz=n[0]*uy-n[1]*ux,occ=0;
      var o=[p[0]+n[0]*.15,p[1]+n[1]*.15,p[2]+n[2]*.15];
      AODIR.forEach(function(a){var d=[ux*a[0]+vx*a[1]+n[0]*a[2],uy*a[0]+vy*a[1]+n[1]*a[2],uz*a[0]+vz*a[1]+n[2]*a[2]],h=hit(o,d,maxd);if(h<maxd)occ+=1-h/maxd});
      c=1-occ/AODIR.length;cache.set(kk,c)}
    out[t*3+v]=c}
  return out}
function camOf(c,aspect){
  var cam=new T.PerspectiveCamera(c.fov,aspect,1,800);cam.up.set(0,0,1);
  var a=c.az*Math.PI/180,e=c.el*Math.PI/180;
  cam.position.set(c.tx+Math.sin(a)*Math.cos(e)*c.dist,c.ty-Math.cos(a)*Math.cos(e)*c.dist,c.tz+Math.sin(e)*c.dist);cam.lookAt(c.tx,c.ty,c.tz);return cam}
var live=new T.WebGLRenderer({canvas:document.getElementById('live'),antialias:true,preserveDrawingBuffer:true});live.shadowMap.enabled=!/noshadow/.test(location.search);
var off=new T.WebGLRenderer({antialias:true,preserveDrawingBuffer:true});off.shadowMap.enabled=!/noshadow/.test(location.search);
function bg(){return new T.Color('#85878d')}
function shoot(w,opt,W,H){
  off.setPixelRatio(1);off.setSize(W,H);off.setClearColor(bg());
  off.render(sceneOf(w,opt),camOf(CAM,W/H));return off.domElement.toDataURL('image/jpeg',.9)}
var cur,seed=1;
function drawLive(){
  var cv=document.getElementById('live'),w=cv.clientWidth,h=cv.clientHeight;
  live.setPixelRatio(Math.min(2,devicePixelRatio||1));live.setSize(w,h,false);live.setClearColor(bg());
  live.render(sceneOf(cur,{stage:6}),camOf(camNow,w/h))}
function rebuild(){
  var t=performance.now();cur=GEN.build(seed,7);
  document.getElementById('ms').textContent=Math.round(performance.now()-t)+' ms, '+cur.blocks.length+' blocks, '+cur.blocks.reduce(function(s,b){return s+b.faces.length},0)+' faces';
  document.getElementById('seedv').value=seed;drawLive()}
function strip(){
  var el=document.getElementById('strip');el.innerHTML='';
  [seed,seed+1,seed+2,seed+3].forEach(function(s){var f=document.createElement('figure'),i=new Image();i.alt='seed '+s;i.src=shoot(GEN.build(s,7),{stage:6},420,360);var c=document.createElement('figcaption');c.textContent='seed '+s;f.appendChild(i);f.appendChild(c);el.appendChild(f)})}
function steps(){
  document.querySelectorAll('[data-stage]').forEach(function(img){var u=+img.getAttribute('data-stage');img.src=shoot(GEN.build(seed,u),{stage:u,figure:u>=7,ao:u>=7?false:undefined},520,420)})}
function all(){rebuild();if(/fast/.test(location.search))return;strip();steps()}
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
