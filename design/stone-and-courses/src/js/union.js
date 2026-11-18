/* ===== the union lab: the cliff's join, measured by route. The same hulls go into every route. ===== */
function bbOf(m){var b=m.boundingBox();return {min:b.min,max:b.max}}
function bbTouch(a,b,e){return a.min[0]<=b.max[0]+e&&b.min[0]<=a.max[0]+e&&a.min[1]<=b.max[1]+e&&b.min[1]<=a.max[1]+e&&a.min[2]<=b.max[2]+e&&b.min[2]<=a.max[2]+e}
/* hulls of what the cliff owns: its blocks, the core behind them, and the ground it stands on; with the foot's stones when the scene's join is asked for */
function hullsOf(b,withFoot){
  var Mf=MF.Manifold,t0=now(),L=[],K=[];
  function H(P,key){if(P.mesh){var mm=meshManifold(P);L.push(mm);K.push(key);return}var v=verts(P);if(v.length<4)return;var m=Mf.hull(v);L.push(m);K.push(key)}
  b.polys.forEach(function(P){H(P,P.b?'c'+P.b.c:'x')});
  if(withFoot)b.scree.forEach(function(P){H(P,'foot')});
  H(b.core,'core');H(b.dirt,'ground');
  return {list:L,keys:K,ms:now()-t0}}
/* a rock's mesh as a solid: its vertices merged, then read as a manifold (a surface that is not closed is refused here, loudly) */
function meshManifold(P){var M=MF,m=P.mesh,mesh=new M.Mesh({numProp:3,vertProperties:m.pos,triVerts:m.idx});mesh.merge();var mf=new M.Manifold(mesh);if(mf.status()!=='NoError')throw new Error('a stone mesh is not a manifold: '+mf.status()+' (block '+(P.b?P.b.c+'_'+P.b.i:'?')+')');return mf}
function freeAll(a){a.forEach(function(m){try{m.delete()}catch(e){}})}
function med(a){a=a.slice().sort(function(x,y){return x-y});return a[Math.floor(a.length/2)]}
function timeIt(fn,reps){var ts=[],r,last=null;for(var i=0;i<reps;i++){var t=now();r=fn();r.numTri();ts.push(now()-t);if(i<reps-1&&r&&r.delete)r.delete();else last=r}return {ms:med(ts),res:last}}
/* touching pairs: bounding boxes that meet (a candidate), then the ones whose hulls really share volume or a face (an intersection with triangles) */
function pairsOf(list){var bb=list.map(bbOf),cand=0,real=0,i,j,M=MF.Manifold;
  for(i=0;i<list.length;i++)for(j=i+1;j<list.length;j++)if(bbTouch(bb[i],bb[j],1e-6)){cand++;var x=M.intersection(list[i],list[j]);if(x.numTri()>0)real++;x.delete()}
  return {cand:cand,real:real,bb:bb}}
/* connected groups by touching boxes (cheap, a superset of true contact) */
function groupsOf(bb){var n=bb.length,par=[],i,j;for(i=0;i<n;i++)par[i]=i;
  function find(a){while(par[a]!==a){par[a]=par[par[a]];a=par[a]}return a}
  for(i=0;i<n;i++)for(j=i+1;j<n;j++)if(bbTouch(bb[i],bb[j],1e-6))par[find(i)]=find(j);
  var G={};for(i=0;i<n;i++){var r=find(i);(G[r]=G[r]||[]).push(i)}return Object.keys(G).map(function(k){return G[k]})}
/* the routes. Every one returns the Manifold it made so its triangles can be compared. */
function routeBatch(h){return MF.Manifold.union(h.list)}
function routeSeq(h){var u=h.list[0],i;for(i=1;i<h.list.length;i++){var n=u.add(h.list[i]);n.numTri();if(i>1)u.delete();u=n}return u}
function routeRows(h){var by={},i,M=MF.Manifold,parts=[];h.list.forEach(function(m,i){(by[h.keys[i]]=by[h.keys[i]]||[]).push(m)});
  Object.keys(by).forEach(function(k){parts.push(by[k].length>1?M.union(by[k]):by[k][0])});return M.union(parts)}
function routeGroups(h,groups){var M=MF.Manifold,parts=groups.map(function(g){return g.length>1?M.union(g.map(function(i){return h.list[i]})):h.list[g[0]]});return parts.length>1?M.compose(parts):parts[0]}
/* one full measurement of a build: the cliff's union by each route */
function studyUnion(b,reps,withFoot){
  reps=reps||3;var h=hullsOf(b,withFoot),o={n:h.list.length,hullMs:h.ms},P=pairsOf(h.list);
  o.pairsBox=P.cand;o.pairs=P.real;
  var gr=groupsOf(P.bb);o.groups=gr.length;
  var t=timeIt(function(){return routeBatch(h)},reps);o.batchMs=t.ms;o.outTris=t.res.numTri();
  var t2=timeIt(function(){return routeSeq(h)},reps);o.seqMs=t2.ms;o.seqTris=t2.res.numTri();t2.res.delete();
  var t3=timeIt(function(){return routeRows(h)},reps);o.rowsMs=t3.ms;o.rowsTris=t3.res.numTri();t3.res.delete();
  var t4=timeIt(function(){return routeGroups(h,gr)},reps);o.groupsMs=t4.ms;o.groupsTris=t4.res.numTri();t4.res.delete();
  var t5=now();var mesh=t.res.getMesh();o.meshMs=now()-t5;
  o.inTris=0;h.list.forEach(function(m){o.inTris+=m.numTri()});
  o.status=t.res.status();o.genus=t.res.genus();
  t.res.delete();freeAll(h.list);return o}
