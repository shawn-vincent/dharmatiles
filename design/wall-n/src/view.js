(function(){
var T=THREE,GEN=window.GEN;
var CAM={az:42,el:27,dist:138,fov:27,tx:19,ty:21,tz:15};   /* the picture's camera: tuned by eye against O */
(location.search.match(/[?&](az|el|dist|fov|tx|ty|tz)=(-?[\d.]+)/g)||[]).forEach(function(m){var k=m.slice(1).split('=');CAM[k[0]]=+k[1]});
var camNow=Object.assign({},CAM);
function cssv(n){return getComputedStyle(document.documentElement).getPropertyValue(n).trim()}
function sceneOf(w,opt){
  opt=opt||{};
  var sc=new T.Scene(),pos=[],col=[],nor=[];
  var RUB=false;var CORE=false;function tri(a,b,c,n,k){var cx=(b[1]-a[1])*(c[2]-a[2])-(b[2]-a[2])*(c[1]-a[1]),cy=(b[2]-a[2])*(c[0]-a[0])-(b[0]-a[0])*(c[2]-a[2]),cz=(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]);if(cx*n[0]+cy*n[1]+cz*n[2]<0){var t=b;b=c;c=t}pos.push(a[0],a[1],a[2],b[0],b[1],b[2],c[0],c[1],c[2]);for(var i=0;i<3;i++){nor.push(n[0],n[1],n[2]);col.push(k*(RUB?.9:.82),k*(RUB?.97:.95),k*(RUB?1.12:1.18))}}
  var r=GEN.rng(w.seed*31+5);
  w.blocks.forEach(function(b){
    var bt=b.rub?.55:.44+.06*r();
    RUB=!!b.rub;b.faces.forEach(function(f){
      var p=f.poly,tone=bt*(0.94+0.12*r());
      if(opt.stage&&opt.stage<=3)tone=.5+.4*((b.row*.37+(b.id||0)*.11)%1);
      var zc=0;p.forEach(function(q){zc+=q[2]});zc/=p.length;tone*=1-.13*Math.max(0,1-zc/22);
      if(f.tris){f.tris.forEach(function(T3){var ux=T3[1][0]-T3[0][0],uy=T3[1][1]-T3[0][1],uz=T3[1][2]-T3[0][2],vx=T3[2][0]-T3[0][0],vy=T3[2][1]-T3[0][1],vz=T3[2][2]-T3[0][2],nn=[uy*vz-uz*vy,uz*vx-ux*vz,ux*vy-uy*vx],l=Math.hypot(nn[0],nn[1],nn[2])||1;nn=[nn[0]/l,nn[1]/l,nn[2]/l];if(nn[0]*f.n[0]+nn[1]*f.n[1]+nn[2]*f.n[2]<0)nn=[-nn[0],-nn[1],-nn[2]];tri(T3[0],T3[1],T3[2],nn,tone*(.72+.56*r()))})}else{for(var i=1;i<p.length-1;i++)tri(p[0],p[i],p[i+1],f.n,tone)}})});
  (w.core||[]).forEach(function(f){var p=f.poly;for(var i=1;i<p.length-1;i++)tri(p[0],p[i],p[i+1],f.n,.07)});
  var g=new T.BufferGeometry();
  g.setAttribute('position',new T.Float32BufferAttribute(pos,3));g.setAttribute('normal',new T.Float32BufferAttribute(nor,3));g.setAttribute('color',new T.Float32BufferAttribute(col,3));
  var m=new T.Mesh(g,new T.MeshLambertMaterial({vertexColors:true}));sc.add(m);
  
  var plate=new T.Mesh(new T.BoxGeometry(35,35,3),new T.MeshLambertMaterial({color:0x4a3a2e}));plate.position.set(17.5,17.5,-1.5);sc.add(plate);
  if(opt.figure!==false){
    var fm=new T.MeshLambertMaterial({color:0xc9a27a}),fg=new T.Group();
    var base=new T.Mesh(new T.CylinderGeometry(8,8,1.5,24),new T.MeshLambertMaterial({color:0x222222}));base.rotation.x=Math.PI/2;base.position.set(0,0,.75);fg.add(base);
    var body=new T.Mesh(new T.CylinderGeometry(3.6,4.4,19,14),fm);body.rotation.x=Math.PI/2;body.position.set(0,0,11);fg.add(body);
    var head=new T.Mesh(new T.SphereGeometry(3,14,10),fm);head.position.set(0,0,24.9);fg.add(head);
    fg.position.set(5,3,0);fg.scale.set(.8,.8,.8);sc.add(fg)}
  sc.add(new T.HemisphereLight(0xcfd9ee,0x2c2a28,.5));
  var d=new T.DirectionalLight(0xfff4e6,1.25);d.position.set(-60,-80,110);sc.add(d);
  var d2=new T.DirectionalLight(0xbfd0ff,.18);d2.position.set(80,-20,30);sc.add(d2);
  return sc}
function camOf(c,aspect){
  var cam=new T.PerspectiveCamera(c.fov,aspect,1,800);cam.up.set(0,0,1);
  var a=c.az*Math.PI/180,e=c.el*Math.PI/180;
  cam.position.set(c.tx+Math.sin(a)*Math.cos(e)*c.dist,c.ty-Math.cos(a)*Math.cos(e)*c.dist,c.tz+Math.sin(e)*c.dist);cam.lookAt(c.tx,c.ty,c.tz);return cam}
var live=new T.WebGLRenderer({canvas:document.getElementById('live'),antialias:true,preserveDrawingBuffer:true});
var off=new T.WebGLRenderer({antialias:true,preserveDrawingBuffer:true});
function bg(){return new T.Color(cssv('--stage')||'#d9e0eb')}
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
