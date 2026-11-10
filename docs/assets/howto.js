/* dharmatiles docs: the step-by-step "how it is made" demos on the element pages.
   Simplified JavaScript stand-ins for each look's main idea, drawn in the browser:
   not the program's output. Each demo runs only where its element is on the page. */
/* ---------- shared helpers ---------- */
const NS = "http://www.w3.org/2000/svg";
function rng(s){ return function(){ s|=0; s=(s+0x6D2B79F5)|0; let t=Math.imul(s^(s>>>15),1|s); t=(t+Math.imul(t^(t>>>7),61|t))^t; return ((t^(t>>>14))>>>0)/4294967296; }; }
function h2(i,j,s){ let h=Math.imul(i,374761393)+Math.imul(j,668265263)+Math.imul(s,1442695041)|0; h=Math.imul(h^(h>>>13),1274126177); h^=h>>>16; return (h>>>0)/4294967296; }
function vn(x,y,s){ const i=Math.floor(x), j=Math.floor(y), fx=x-i, fy=y-j, u=fx*fx*(3-2*fx), v=fy*fy*(3-2*fy);
  const a=h2(i,j,s), b=h2(i+1,j,s), c=h2(i,j+1,s), d=h2(i+1,j+1,s); return a+(b-a)*u+(c-a)*v+(a-b-c+d)*u*v; }
function fbm(x,y,s,o=4){ let a=0,amp=1,f=1,n=0; for(let k=0;k<o;k++){ a+=amp*vn(x*f,y*f,s+k*17); n+=amp; amp*=.5; f*=2.03; } return a/n; }
const smooth=(e0,e1,x)=>{ const t=Math.min(1,Math.max(0,(x-e0)/(e1-e0))); return t*t*(3-2*t); };
function smax(a,b,k){ const h=Math.max(k-Math.abs(a-b),0)/k; return Math.max(a,b)+h*h*k*.25; }
/* shade a heightfield H (n x n, mm) with cell size c (mm) into canvas using colour fn */
function shade(cv, H, n, c, colAt){
  const off=document.createElement("canvas"); off.width=n; off.height=n; const ox=off.getContext("2d"); const img=ox.createImageData(n,n);
  const L=[-.55,-.6,.58]; const ln=Math.hypot(...L); L[0]/=ln; L[1]/=ln; L[2]/=ln;
  for(let j=0;j<n;j++) for(let i=0;i<n;i++){
    const k=j*n+i; const il=i>0?k-1:k, ir=i<n-1?k+1:k, ju=j>0?k-n:k, jd=j<n-1?k+n:k;
    const dx=(H[ir]-H[il])/(c*(ir-il||1)), dy=(H[jd]-H[ju])/(c*((jd-ju)/n||1));
    let nx=-dx, ny=-dy, nz=1; const m=Math.hypot(nx,ny,nz); nx/=m; ny/=m; nz/=m;
    const lam=Math.max(0,nx*L[0]+ny*L[1]+nz*L[2]); const col=colAt(i,j,k); const f=.28+.85*lam;
    img.data[k*4]=Math.min(255,col[0]*f); img.data[k*4+1]=Math.min(255,col[1]*f); img.data[k*4+2]=Math.min(255,col[2]*f); img.data[k*4+3]=255;
  }
  ox.putImageData(img,0,0); const ctx=cv.getContext("2d"); ctx.imageSmoothingEnabled=true; ctx.drawImage(off,0,0,cv.width,cv.height);
}
/* generic stepper */
function stepper(root, steps, draw){
  const ol=root.querySelector("ol.steps"); const btn=root.querySelector("[data-play]"); let cur=steps.length-1, timer=null;
  steps.forEach((s,i)=>{ const li=document.createElement("li"); li.textContent=s; li.tabIndex=0; li.addEventListener("click",()=>{stop(); go(i);}); li.addEventListener("keydown",e=>{ if(e.key==="Enter"||e.key===" "){ e.preventDefault(); stop(); go(i);} }); ol.appendChild(li); });
  function go(i){ cur=i; [...ol.children].forEach((li,k)=>li.classList.toggle("on",k===i)); draw(i); }
  function stop(){ if(timer){ clearInterval(timer); timer=null; btn.textContent="Play"; } }
  btn.addEventListener("click",()=>{ if(timer){ stop(); return; } btn.textContent="Pause"; go(0); timer=setInterval(()=>{ if(cur>=steps.length-1){ stop(); return; } go(cur+1); },2200); });
  go(cur);
}

/* ---------- ROCK demo ---------- */
(() => {
  const root=document.getElementById("d-rock"); if(!root) return; const cv=root.querySelector("canvas");
  const n=240, mm=40/n; // 40 mm field
  const R=rng(11); const cx=20, cy=20, rx=12.5, ry=9.5, ht=9;
  // dome profile (never widens upward)
  const dome=new Float32Array(n*n), grad=[];
  const domeAt=(x,y)=>{ const ux=(x-cx)/rx, uy=(y-cy)/ry; const ang=Math.atan2(uy,ux); const lump=1+.06*Math.cos(2*ang+1)+.05*Math.cos(3*ang+2)+.03*Math.cos(5*ang);
    const r=Math.pow(Math.pow(Math.abs(ux),2.6)+Math.pow(Math.abs(uy),2.6),1/2.6)/lump; return r>=1?-1:ht*Math.pow(1-Math.pow(r,2.2),.55); };
  for(let j=0;j<n;j++) for(let i=0;i<n;i++) dome[j*n+i]=domeAt(i*mm,j*mm);
  // facet seeds: blue noise with mixed radii on dome
  const seeds=[]; for(let t=0;t<3000 && seeds.length<46;t++){ const x=cx+(R()*2-1)*rx, y=cy+(R()*2-1)*ry; const z=domeAt(x,y); if(z<.3) continue;
    const r=.9+R()*1.6; if(seeds.every(s=>Math.hypot(s.x-x,s.y-y)>(s.r+r)*1.25)) seeds.push({x,y,r,z}); }
  seeds.sort((a,b)=>b.r-a.r);
  const planes=seeds.map(s=>{ const e=.05; let gx=(domeAt(s.x+e,s.y)-domeAt(s.x-e,s.y))/(2*e), gy=(domeAt(s.x,s.y+e)-domeAt(s.x,s.y-e))/(2*e);
    const tilt=.25; gx+= (R()*2-1)*tilt; gy+=(R()*2-1)*tilt; return {x:s.x,y:s.y,z:s.z+.15,gx,gy}; });
  const chips=[]; for(let k=0;k<9;k++){ const a=R()*Math.PI*2; const x=cx+Math.cos(a)*rx*.82, y=cy+Math.sin(a)*ry*.82; const z=domeAt(x,y);
    if(z<=0) continue; chips.push({x,y,z:z-.35,gx:-Math.cos(a)*2.4,gy:-Math.sin(a)*2.4}); }
  const steps=["Profile body: a hull that never widens going up, with lumps that depend only on angle","Facet planes: blue-noise seeds with mixed radii, each a tilted tangent plane","Chips: small steep planes at sharp corners and edges, capped volume","Grain: fBm along the normal, faded toward the base","Seat: set into the ground under it, below the lowest point of its footprint"];
  function build(step, count){
    const H=new Float32Array(n*n), mat=new Uint8Array(n*n);
    for(let j=0;j<n;j++) for(let i=0;i<n;i++){ const k=j*n+i, x=i*mm, y=j*mm; let z=dome[k];
      if(step>=1 && z>0){ let m=Infinity; const N=Math.min(planes.length,count); for(let p=0;p<N;p++){ const P=planes[p]; const v=P.z+P.gx*(x-P.x)+P.gy*(y-P.y); if(v<m) m=v; } if(N>0) z=Math.min(Math.max(z, 0), m); z=Math.max(z, dome[k]>0?Math.min(dome[k],.0):-1); if(m<z) z=m; }
      if(step>=2 && z>0){ for(const C of chips){ const v=C.z+C.gx*(x-C.x)+C.gy*(y-C.y); if(v<z) z=v; } }
      if(step>=3 && z>0){ z+= (fbm(x/.9,y/.9,5,3)-.5)*.35*smooth(0,1.5,z); }
      let g = step>=4 ? 1.1+ (fbm(x/1.2,y/1.2,9,4)-.5)*.5 + Math.max(0,fbm(x/.5,y/.5,3,2)-.6)*.8 : 0;
      if(z>g-(step>=4?.55:0) && z>0){ H[k]=z; mat[k]=1; } else { H[k]=step>=4?g:0; mat[k]=0; }
    }
    return {H,mat};
  }
  let anim=null;
  function draw(step){
    if(anim) cancelAnimationFrame(anim);
    const paint=(count)=>{ const {H,mat}=build(step,count); const cv2=cv; cv2.width=480; cv2.height=480*0.75;
      const tmp=document.createElement("canvas"); tmp.width=n; tmp.height=n;
      shade(tmp,H,n,mm,(i,j,k)=>mat[k]?[168,172,182]:(step>=4?[150,116,80]:[205,208,200]));
      const ctx=cv.getContext("2d"); ctx.drawImage(tmp,0,n*.125,n,n*.75,0,0,cv.width,cv.height); };
    if(step===1){ let c=0; const tick=()=>{ c+=2; paint(c); if(c<planes.length) anim=requestAnimationFrame(tick); }; tick(); } else paint(planes.length);
  }
  stepper(root,steps,draw);
})();

/* ---------- CLIFF demo ---------- */
(() => {
  const root=document.getElementById("d-cliff"); if(!root) return; const cv=root.querySelector("canvas"), ctx=cv.getContext("2d");
  const W=70, Hh=40, s=8; const R=rng(7);
  // target T(x): tiers + gully
  const T=[]; let x=0; const tiers=[]; while(x<W){ const w=8+R()*9; const h=[27,31,34,37][Math.floor(R()*4)]; tiers.push([x,Math.min(W,x+w),h]); x+=w; }
  for(let i=0;i<W;i++){ const t=tiers.find(t=>i>=t[0]&&i<t[1]); let h=t[2]; if(i>=40&&i<44) h-=7; T.push(h); }
  const zone=i=>Math.exp(-((i-22)**2)/(2*36)); // shattered zone near x=22
  // gravity stack
  const S=new Array(W).fill(0); const stones=[];
  for(let guard=0;guard<400;guard++){
    let best=-1,bz=1e9; for(let i=0;i<W;i++) if(S[i]<T[i]-.5 && S[i]<bz){ bz=S[i]; best=i; } if(best<0) break;
    const kind=R(); const sc=1-.45*zone(best);
    let w=Math.round((kind<.4? 4+R()*3 : kind<.75? 7+R()*6 : 9+R()*6)*sc); let h=(kind<.4? 12+R()*10 : kind<.75? 6+R()*6 : 3+R()*2.5)*sc;
    let a=best,b=best; while(b-a+1<w && b+1<W && Math.abs(S[b+1]-bz)<.6 && T[b+1]>bz+1) b++; while(b-a+1<w && a-1>=0 && Math.abs(S[a-1]-bz)<.6 && T[a-1]>bz+1) a--;
    let tmin=Infinity; for(let i=a;i<=b;i++) tmin=Math.min(tmin,T[i]);
    let top=bz+h; if(tmin-top<3.5) top=tmin; // merge crumb into top
    for(const st of stones){ if(st.x1>=a-1&&st.x0<=b+1 && Math.abs(st.z1-top)<1 && top<tmin) top=Math.min(tmin,top+1.6); }
    stones.push({x0:a,x1:b+1,z0:bz,z1:top,k:stones.length}); for(let i=a;i<=b;i++) S[i]=top;
  }
  // columns: union stones whose footprints overlap >=60%
  const par=stones.map((_,i)=>i); const f=i=>par[i]===i?i:(par[i]=f(par[i]));
  stones.forEach((A,i)=>stones.forEach((B,j)=>{ if(j<=i) return; if(Math.abs(B.z0-A.z1)<.01||Math.abs(A.z0-B.z1)<.01){ const ov=Math.min(A.x1,B.x1)-Math.max(A.x0,B.x0); if(ov>=.6*Math.min(A.x1-A.x0,B.x1-B.x0)) par[f(i)]=f(j); } }));
  stones.forEach(st=>{ st.col=f(st.k); st.d=[R()*.7,R()*.7,R()*.5,R()*.5]; });
  // cracks: random walks inside stones, mostly steep
  const cracks=[]; stones.forEach(st=>{ if((st.x1-st.x0)*(st.z1-st.z0)<30 || R()<.35) return; let px=st.x0+(st.x1-st.x0)*(.3+R()*.4), pz=st.z0+(st.z1-st.z0)*(.4+R()*.3); const pts=[[px,pz]]; let ang=Math.PI/2+(R()-.5)*.9;
    for(let k=0;k<60;k++){ ang+= (R()-.5)*.5; px+=Math.cos(ang)*.8; pz+=Math.sin(ang)*.8; pts.push([px,pz]); if(px<=st.x0+.2||px>=st.x1-.2||pz>=st.z1-.1||pz<=st.z0+.1) break; }
    cracks.push({pts,lip:R()<.5}); });
  const scree=[]; for(let k=0;k<60;k++){ const xx=R()*W, r=.4+R()*R()*2.2; scree.push({x:xx,r,a:R()*6}); }
  const steps=["Form: a target heightfield T with tiers and a gully; shattered zones shrink stone size","Gravity stack: fill the lowest pocket, snap footprints, redraw heights so tops never align","Columns: union-find over stones sharing at least 60% of a footprint","Hidden core: support eroded by 2 mm, so joints show a dark recess, not daylight","Dress: battered planes and chamfers, accepted only under the cut guard (no fins under 1.2 mm)","Crack network: random walks grow until both ends meet an edge, cut as V-grooves with optional lips","Grain, scree and dirt: an even fine grain; scree at the foot (a scatter against the cliff); dirt lodged on the tops"];
  const Y=z=>cv.height-14-z*s*.75, X=i=>i*s;
  let anim=null;
  function draw(step, upto){
    const css=getComputedStyle(document.documentElement); const fg=css.getPropertyValue("--fg").trim(), muted=css.getPropertyValue("--muted").trim(), alert=css.getPropertyValue("--alert").trim();
    ctx.clearRect(0,0,cv.width,cv.height);
    ctx.fillStyle="#9fc3dd"; ctx.globalAlpha=.25; ctx.fillRect(0,0,cv.width,cv.height); ctx.globalAlpha=1;
    // zone shading
    if(step===0){ for(let i=0;i<W;i++){ ctx.fillStyle=`rgba(181,61,44,${.35*zone(i)})`; ctx.fillRect(X(i),0,s,cv.height); } }
    // core
    if(step>=3){ ctx.fillStyle="#2b2f33"; ctx.beginPath(); ctx.moveTo(X(0),Y(0)); for(let i=0;i<W;i++){ ctx.lineTo(X(i),Y(Math.max(0,T[i]-2))); ctx.lineTo(X(i+1),Y(Math.max(0,T[i]-2))); } ctx.lineTo(X(W),Y(0)); ctx.fill(); }
    const N= step===1 && upto!=null ? upto : stones.length;
    if(step>=1){ const hues={}; for(let q=0;q<N;q++){ const st=stones[q]; const g=step>=4?.35:.12; const d=step>=4?st.d:[0,0,0,0];
      let base=[176,178,182]; if(step===2){ const h=(st.col*47)%7; base=[150+h*12,160+((h*3)%7)*8,175-h*6]; }
      ctx.fillStyle=`rgb(${base})`; ctx.strokeStyle=step>=3?"#2b2f33":"#555";
      ctx.lineWidth=step>=3?2.2:1; ctx.beginPath();
      ctx.moveTo(X(st.x0)+g*s,Y(st.z0)-g); ctx.lineTo(X(st.x0)+g*s+d[0]*s,Y(st.z1)+g*s+d[2]*s); ctx.lineTo(X(st.x0)+(.8+d[0])*s,Y(st.z1)+g); ctx.lineTo(X(st.x1)-(.8+d[1])*s,Y(st.z1)+g); ctx.lineTo(X(st.x1)-g*s-d[1]*s,Y(st.z1)+g*s+d[3]*s); ctx.lineTo(X(st.x1)-g*s,Y(st.z0)-g); ctx.closePath(); ctx.fill(); ctx.stroke();
      if(step>=4){ ctx.fillStyle="rgba(255,255,255,.25)"; ctx.fillRect(X(st.x0)+(.8+d[0])*s,Y(st.z1)+g,Math.max(0,X(st.x1)-X(st.x0)-(1.6+d[0]+d[1])*s),3); } } }
    if(step===0||step===1){ ctx.strokeStyle=alert; ctx.setLineDash([5,4]); ctx.lineWidth=1.5; ctx.beginPath(); for(let i=0;i<W;i++){ ctx.lineTo(X(i),Y(T[i])); ctx.lineTo(X(i+1),Y(T[i])); } ctx.stroke(); ctx.setLineDash([]); }
    if(step>=5){ for(const c of cracks){ ctx.strokeStyle="#2b2f33"; ctx.lineWidth=2.4; ctx.beginPath(); c.pts.forEach((p,i)=>i?ctx.lineTo(X(p[0]),Y(p[1])):ctx.moveTo(X(p[0]),Y(p[1]))); ctx.stroke();
      if(c.lip){ ctx.strokeStyle="rgba(255,255,255,.5)"; ctx.lineWidth=1; ctx.beginPath(); c.pts.forEach((p,i)=>i?ctx.lineTo(X(p[0])+2.5,Y(p[1])-1):ctx.moveTo(X(p[0])+2.5,Y(p[1])-1)); ctx.stroke(); } } }
    if(step>=6){ const R2=rng(3); for(let k=0;k<1800;k++){ const xx=R2()*cv.width, yy=R2()*cv.height; ctx.fillStyle=R2()<.5?"rgba(0,0,0,.08)":"rgba(255,255,255,.1)"; ctx.fillRect(xx,yy,2,2); }
      ctx.fillStyle="#8a6640"; ctx.fillRect(0,Y(0)-2,cv.width,16);
      for(const p of scree){ ctx.fillStyle="#b9bbbf"; ctx.strokeStyle="#555"; ctx.lineWidth=.8; ctx.beginPath(); for(let k=0;k<5;k++){ const a=p.a+k*1.26; ctx.lineTo(X(p.x)+Math.cos(a)*p.r*s, Y(0)-2-Math.abs(Math.sin(a))*p.r*s*.8); } ctx.closePath(); ctx.fill(); ctx.stroke(); }
      for(let i=0;i<W;i++){ ctx.fillStyle="#8a6640"; ctx.fillRect(X(i),Y(S[i])-2,s,3); } }
    ctx.fillStyle=fg; ctx.font="12px IBM Plex Mono, monospace"; ctx.fillText(step===0?"T(x), shattered zone shaded":step===1?`${N}/${stones.length} stones`:"",8,16);
  }
  function go(step){ if(anim) cancelAnimationFrame(anim); if(step===1){ let c=0; const tick=()=>{ c+=1; draw(1,c); if(c<stones.length) anim=requestAnimationFrame(()=>setTimeout(tick,22)); }; tick(); } else draw(step); }
  stepper(root,steps,go);
})();

/* ---------- GRASS demo ---------- */
(() => {
  const root=document.getElementById("d-grass"); if(!root) return; const cv=root.querySelector("canvas"), ctx=cv.getContext("2d");
  const W=70, px=cv.width/W; const th0=-.45; const R=rng(21);
  const lenses=[{x:22,y:46,a:7,b:3.2},{x:50,y:22,a:6,b:2.8}];
  const lensM=(L,x,y)=>{ const c=Math.cos(th0), s=Math.sin(th0); const dx=x-L.x, dy=y-L.y; const u=dx*c+dy*s, v=-dx*s+dy*c; return {m:Math.abs(u)/L.a+(v/L.b)**2, u, v}; };
  function theta(x,y,mode){ let t=th0+(fbm(x/22,y/22,4,3)-.5)*1.4; if(mode>=1){ const sAlong=x*Math.cos(th0)+y*Math.sin(th0); t+=.36*Math.sin(2*Math.PI*sAlong/15+.7); } return t; }
  function dir(x,y,mode){ let t=theta(x,y,mode); let vx=Math.cos(t), vy=Math.sin(t);
    if(mode>=2){ for(const L of lenses){ const q=lensM(L,x,y); if(q.m<3.2){ const f=Math.exp(-(q.m-1)*1.3); const px_=-Math.sin(th0), py_=Math.cos(th0); const side=Math.tanh(q.v/L.b*1.5)||1; vx+=px_*side*f*1.4; vy+=py_*side*f*1.4; } } const m=Math.hypot(vx,vy); vx/=m; vy/=m; }
    return [vx,vy]; }
  const inLens=(x,y)=>lenses.some(L=>lensM(L,x,y).m<1);
  function hills(){ const n=140, H=new Float32Array(n*n); for(let j=0;j<n;j++) for(let i=0;i<n;i++){ const x=i/n*W,y=j/n*W; H[j*n+i]=7*Math.max(0,fbm(x/12,y/12,8,3)-.4); }
    const tmp=document.createElement("canvas"); tmp.width=cv.width; tmp.height=cv.height; shade(tmp,H,n,W/n,()=>[150,120,86]); ctx.drawImage(tmp,0,0); }
  function arrows(mode){ ctx.strokeStyle="rgba(20,30,25,.8)"; ctx.lineWidth=1.2; for(let y=2;y<W;y+=3.5) for(let x=2;x<W;x+=3.5){ if(mode>=2&&inLens(x,y)) continue; const [vx,vy]=dir(x,y,mode); const L=1.4;
      ctx.beginPath(); ctx.moveTo((x-vx*L)*px,(y-vy*L)*px); ctx.lineTo((x+vx*L)*px,(y+vy*L)*px); ctx.stroke(); ctx.beginPath(); ctx.arc((x+vx*L)*px,(y+vy*L)*px,1.6,0,7); ctx.fillStyle="rgba(20,30,25,.9)"; ctx.fill(); } }
  function lensDraw(){ for(const L of lenses){ ctx.save(); ctx.translate(L.x*px,L.y*px); ctx.rotate(th0); ctx.beginPath(); for(let k=0;k<=60;k++){ const u=-L.a+2*L.a*k/60; const v=L.b*Math.sqrt(Math.max(0,1-Math.abs(u)/L.a)); ctx.lineTo(u*px,v*px); } for(let k=60;k>=0;k--){ const u=-L.a+2*L.a*k/60; const v=-L.b*Math.sqrt(Math.max(0,1-Math.abs(u)/L.a)); ctx.lineTo(u*px,v*px); } ctx.closePath(); ctx.fillStyle="rgba(120,86,52,.85)"; ctx.fill(); ctx.restore(); } }
  // strands
  const seeds=[]; const sp=2.65; for(let y=0;y<W;y+=sp) for(let x=0;x<W;x+=sp) seeds.push([x+R()*sp, y+R()*sp, 15+R()*15]);
  function trace(x,y,len){ const pts=[[x,y]]; const h=.5; for(let d=0; d<len; d+=h){ const [a,b]=dir(x,y,2); const [c,e]=dir(x+a*h/2,y+b*h/2,2); x+=c*h; y+=e*h; if(x<1||y<1||x>W-1||y>W-1||inLens(x,y)) break; pts.push([x,y]); } return pts; }
  let strands=null;
  function getStrands(){ if(!strands) strands=seeds.map(s=>trace(s[0],s[1],s[2])); return strands; }
  function blades(){ const st=getStrands(); const Rb=rng(5); const all=[];
    st.forEach(p=>{ for(let r=0;r<p.length-3;r+=Math.round(3.4+Rb()*1.5)){ const L=Math.min(p.length-1-r, Math.round(8+Rb()*4)); if(L<3) continue; all.push({pts:p.slice(r,r+L+1),key:p[r][0]*Math.cos(th0)+p[r][1]*Math.sin(th0)}); } });
    all.sort((a,b)=>a.key-b.key);
    for(const b of all){ const q=b.pts, n=q.length; const left=[],right=[];
      for(let k=0;k<n;k++){ const a=q[Math.max(0,k-1)], c=q[Math.min(n-1,k+1)]; let tx=c[0]-a[0], ty=c[1]-a[1]; const m=Math.hypot(tx,ty)||1; tx/=m; ty/=m; const u=k/(n-1); const w=.5*Math.sin(Math.PI*Math.min(1,u*1.15))*(1-u*.4);
        left.push([(q[k][0]-ty*w)*px,(q[k][1]+tx*w)*px]); right.push([(q[k][0]+ty*w)*px,(q[k][1]-tx*w)*px]); }
      ctx.beginPath(); left.forEach((p,i)=>i?ctx.lineTo(p[0],p[1]):ctx.moveTo(p[0],p[1])); right.reverse().forEach(p=>ctx.lineTo(p[0],p[1])); ctx.closePath();
      ctx.fillStyle="#7f9a4c"; ctx.fill(); ctx.strokeStyle="rgba(30,45,20,.75)"; ctx.lineWidth=.9; ctx.stroke();
      ctx.strokeStyle="rgba(230,240,190,.55)"; ctx.lineWidth=.8; ctx.beginPath(); q.slice(0,n-1).forEach((p,i)=>i?ctx.lineTo(p[0]*px,p[1]*px):ctx.moveTo(p[0]*px,p[1]*px)); ctx.stroke(); } }
  const steps=["Hills: rises-only noise, slope capped at 34°, feathered to the 3.4 mm edge level","Direction field: one angle per point (regional noise), so no vortices are possible","In-phase meander: A·sin(2πs/15 mm) along the flow coordinate s","Clearings: eye-shaped lenses aligned to the flow, which parts upstream and rejoins","Strands: Poisson seeds 2.65 mm apart, traced 15–30 mm with RK2","Blades: fronds along each strand, shingled upstream first, shortened (never sliced) at clearings"];
  function draw(step){ ctx.clearRect(0,0,cv.width,cv.height); hills(); if(step>=3) lensDraw();
    if(step===1) arrows(0); else if(step===2) arrows(1); else if(step===3) arrows(2);
    if(step===4){ ctx.strokeStyle="rgba(25,40,20,.75)"; ctx.lineWidth=1; for(const p of getStrands()){ ctx.beginPath(); p.forEach((q,i)=>i?ctx.lineTo(q[0]*px,q[1]*px):ctx.moveTo(q[0]*px,q[1]*px)); ctx.stroke(); } }
    if(step>=5) blades(); }
  stepper(root,steps,draw);
  // blade cross-section svg
  const xs=document.getElementById("xsec"), sd=document.getElementById("side"); if(!xs||!sd) return; const layers=[2,3,4,4,3,1]; const lw=24, lh=20;
  layers.forEach((nl,k)=>{ const y=150-k*(lh+2); const w=nl*lw; const x0=130-w/2; for(let q=0;q<nl;q++){ const r=document.createElementNS(NS,"rect"); r.setAttribute("x",x0+q*lw+1); r.setAttribute("y",y-lh); r.setAttribute("width",lw-2); r.setAttribute("height",lh); r.setAttribute("rx",9); r.setAttribute("fill","var(--accent)"); r.setAttribute("fill-opacity",".55"); r.setAttribute("stroke","currentColor"); r.setAttribute("stroke-width",".8"); xs.appendChild(r);} const t=document.createElementNS(NS,"text"); t.setAttribute("x",x0-8); t.setAttribute("y",y-6); t.setAttribute("text-anchor","end"); t.textContent=nl; xs.appendChild(t); });
  const t1=document.createElementNS(NS,"text"); t1.setAttribute("x",200); t1.setAttribute("y",40); t1.textContent="0.45 mm line"; xs.appendChild(t1);
  const t2=document.createElementNS(NS,"text"); t2.setAttribute("x",200); t2.setAttribute("y",56); t2.textContent="× 0.2 mm layer"; xs.appendChild(t2);
  for(let k=0;k<6;k++){ const y=150-k*(lh+2); const L=200-k*14 - (k<5?0:0); const rake=(5-k)*8; const r=document.createElementNS(NS,"rect"); r.setAttribute("x",300+k*12); r.setAttribute("y",y-lh); r.setAttribute("width",Math.max(30,L-rake-k*6)); r.setAttribute("height",lh); r.setAttribute("rx",9); r.setAttribute("fill","var(--accent)"); r.setAttribute("fill-opacity",".45"); r.setAttribute("stroke","currentColor"); r.setAttribute("stroke-width",".8"); sd.appendChild(r); }
})();

/* ---------- WATER demo ---------- */
(() => {
  const root=document.getElementById("d-water"); if(!root) return; const cv=root.querySelector("canvas");
  const n=200, W=70, c=W/n; let mode="sc", wob=1;
  const xs=y=>22+3*Math.sin(y/9)+4*(fbm(3,y/14,2,3)-.5)*2;
  const rc={x:44,y:34}; const rr=a=>5*(1+.12*Math.cos(3*a+1)+.08*Math.cos(5*a));
  function train(d,lo,hi,nc,p,x,y,sharpLong){ let s=0, n2=0; for(let k=0;k<nc;k++){ const lam= nc===1?lo: lo*Math.pow(hi/lo,k/(nc-1)); const a=Math.pow(lam,1.4);
      const D=30*Math.log(1+d/30); const wobA= wob*.06*d*Math.pow(lam/hi,-.3)*(fbm(x/9,y/9,p+k,2)-.5)*2;
      let ph=2*Math.PI*D/lam + wobA + k*1.7 + p; let cc=Math.cos(ph); const sh = lam<(lo+hi)/2 ? .9 : (sharpLong??.15); cc = cc + sh*.45*(cc*cc-.5); s+=a*cc; n2+=a; } return s/n2; }
  function compute(){ const H=new Float32Array(n*n), mat=new Uint8Array(n*n);
    for(let j=0;j<n;j++) for(let i=0;i<n;i++){ const k=j*n+i, x=i*c, y=j*c; const sx=xs(y); const dl=x-sx;
      const a=Math.atan2(y-rc.y,x-rc.x); const dr=Math.hypot(x-rc.x,y-rc.y)-rr(a);
      const land= 2.6-2.2+ 6*smooth(4,-6,dl);
      if(dr<0){ H[k]=2.6+4*Math.sqrt(Math.min(1,-dr/3)); mat[k]=2; continue; }
      if(dl<0 && land>2.6){ H[k]=land; mat[k]=1; continue; }
      let rel;
      if(mode==="single"){ rel=Math.cos(2*Math.PI*Math.max(0,dl)/9)*.5+Math.cos(2*Math.PI*dr/9)*.5; }
      else { const tl=train(Math.max(0,dl),5,16,4,1,x,y), tr=train(dr,2,4.5,3,2,x,y), tf=train(Math.hypot(x-250,y+150)-200,5,14,3,3,x,y);
        const wl=Math.exp(-Math.max(0,dl)/25), wr=1.15*Math.exp(-dr/8), wf=.22;
        if(mode==="land") rel=tl; else if(mode==="rock") rel=tr;
        else { const dom=(Math.pow(wl,8)*tl+Math.pow(wr,8)*tr+Math.pow(wf,8)*tf)/(Math.pow(wl,8)+Math.pow(wr,8)+Math.pow(wf,8));
          const ch=(wl*tl+wr*tr+wf*tf)/Math.sqrt(wl*wl+wr*wr+wf*wf);
          if(mode==="dom") rel=dom; else if(mode==="chaos") rel=ch; else { const t=smooth(.12,.45,Math.max(wl,wr)); rel=t*dom+(1-t)*ch; }
          const wake=Math.max(0,(x-rc.x))*Math.exp(-Math.abs(y-rc.y)/4)*Math.exp(-dr/14)*.03; rel+=wake*Math.sin(dr*1.4); } }
      const rim=smooth(0,4,Math.min(x,y,W-x,W-y));
      let z=2.6+rel*.8*(.4+.6*rim); z=Math.max(z, 2.6-1.1+Math.log1p(Math.exp((z-1.5)*4))/4-.0); H[k]=Math.max(land,z); mat[k]= H[k]>land+1e-6?0:1; }
    shade(cv,H,n,c*0.45,(i,j,k)=>mat[k]===2?[170,172,178]:mat[k]===1?[170,140,100]:[60,150,165]); }
  root.querySelectorAll("[data-mode]").forEach(b=>b.addEventListener("click",()=>{ mode=b.dataset.mode; root.querySelectorAll("[data-mode]").forEach(x=>x.setAttribute("aria-pressed",x===b)); compute(); }));
  const sl=document.getElementById("wob"), sv=document.getElementById("wobv"); sl.addEventListener("input",()=>{ wob=+sl.value; sv.textContent=wob.toFixed(1); compute(); });
  compute();
})();

/* ---------- DIRT demo ---------- */
(() => {
  const root=document.getElementById("d-dirt"); if(!root) return; const cv=root.querySelector("canvas");
  const n=220, W=35, c=W/n; const R=rng(9);
  const U=new Float32Array(n*n); for(let j=0;j<n;j++) for(let i=0;i<n;i++){ const x=i*c,y=j*c; U[j*n+i]=1.6*smooth(.45,.85,fbm(x/9,y/9,2,3)); }
  const clods=[]; for(let t=0;t<4000&&clods.length<520;t++){ const x=R()*W,y=R()*W; const P=Math.max(.3,smooth(.35,.7,fbm(x/7,y/7,6,3))); if(R()>P) continue; const r=.35+1.15*Math.pow(R(),2); clods.push({x,y,r,h:Math.min(.45,.32*r),a:R()*6,b:R()*6,cc:R()*6}); }
  clods.sort((a,b)=>b.r-a.r);
  const pebs=[]; for(let t=0;t<3000&&pebs.length<70;t++){ const x=R()*W,y=R()*W; const low=1-U[Math.floor(y/c)*n+Math.floor(x/c)]/1.6; if(R()>.25+low*.75) continue; const big=pebs.length<10; const r=big?1.1+R()*1.1:.25+R()*.5; const K=5+Math.floor(R()*4); const u=[]; for(let k=0;k<K;k++) u.push(2*Math.PI*k/K+R()*.5); pebs.push({x,y,r,K,u,s:u.map(()=>1+.35*R()),hp:r*(big?.42:.55)}); }
  const steps=["Undulation: rises-only swell, at most 1.6 mm, flat plains between","Soil roughness: fine noise over the whole surface","Clods: lumpy crumbs in patches (density never below 0.3), largest first, joined by smooth max","Grit: fine roughness of about 0.45 mm and sparse sand grains","Pebbles: faceted caps from tilted tangent planes, half-sunk in low ground, filleted with smooth max"];
  function draw(step){ const H=new Float32Array(n*n), mat=new Uint8Array(n*n);
    for(let k=0;k<n*n;k++){ const i=k%n,j=(k/n)|0, x=i*c,y=j*c; H[k]=U[k]+(step>=1?(fbm(x/1.1,y/1.1,3,3)-.5)*.22:0); }
    if(step>=2){ for(const q of clods){ const i0=Math.max(0,Math.floor((q.x-q.r*1.3)/c)), i1=Math.min(n-1,Math.ceil((q.x+q.r*1.3)/c)), j0=Math.max(0,Math.floor((q.y-q.r*1.3)/c)), j1=Math.min(n-1,Math.ceil((q.y+q.r*1.3)/c)); const base=H[Math.min(n-1,Math.floor(q.y/c))*n+Math.min(n-1,Math.floor(q.x/c))];
      for(let j=j0;j<=j1;j++) for(let i=i0;i<=i1;i++){ const dx=i*c-q.x, dy=j*c-q.y, a=Math.atan2(dy,dx); const lob=1+.15*Math.cos(2*a+q.a)+.1*Math.cos(3*a+q.b)+.06*Math.cos(5*a+q.cc); const rho=Math.hypot(dx,dy)/(q.r*lob); if(rho>=1) continue; const z=base+q.h*Math.pow(1-Math.pow(rho,2.6),.8); const k=j*n+i; H[k]=smax(H[k],z,.12); } } }
    if(step>=3){ for(let k=0;k<n*n;k++){ const i=k%n,j=(k/n)|0; H[k]+=(h2(i,j,77)-.5)*.05 + (h2(i,j,78)>.985?.09:0); } }
    if(step>=4){ for(const p of pebs){ const ext=p.r*1.2, i0=Math.max(0,Math.floor((p.x-ext)/c)), i1=Math.min(n-1,Math.ceil((p.x+ext)/c)), j0=Math.max(0,Math.floor((p.y-ext)/c)), j1=Math.min(n-1,Math.ceil((p.y+ext)/c)); const base=H[Math.min(n-1,Math.floor(p.y/c))*n+Math.min(n-1,Math.floor(p.x/c))]-p.hp*.35;
      for(let j=j0;j<=j1;j++) for(let i=i0;i<=i1;i++){ const dx=i*c-p.x, dy=j*c-p.y; let m=p.hp*.92; for(let q=0;q<p.K;q++){ const v=p.hp - p.s[q]*(p.hp/p.r)*(dx*Math.cos(p.u[q])+dy*Math.sin(p.u[q])); if(v<m) m=v; } const z=base+m; const k=j*n+i; if(z>H[k]-.02){ const nz=smax(H[k],z,.08); if(z>H[k]) mat[k]=1; H[k]=nz; } } } }
    shade(cv,H,n,c,(i,j,k)=>mat[k]?[160,162,168]:(()=>{ const t=Math.min(1,Math.max(0,H[k]/2)); return [128+40*t,96+30*t,64+20*t]; })()); }
  stepper(root,steps,draw);
})();

