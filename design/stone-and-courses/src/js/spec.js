/* ===== THE SPEC: one text form of everything Shawn sees, read and written in this one place (the page and the Node harness both use it) =====
   Format: `key value ...` lines in sections, '#' starts a comment. A line is a key and its numbers or word; a range is its numbers ("top 48 60"; "width 5 13 0.8" has the skew third). The header has the build, preset, system, seed, view; then [region] [courses] [rockregion] [rock]. A value that differs from the preset ends with "# preset <its value>". [result] and [problems] follow and are never read back: copy, load, copy gives identical text up to [result]. */
var Spec=(function(){
  var NAME='stone-and-courses spec 7';   /* 7: row planes: lean and tilt (ranges) and planes.bumps/w/a/c join [courses]; dip is gone (a spec with dip is refused by the version check, not translated); 6: free faces: free.front, free.back, free.sides join [region]; faces (1 or 2) became wall (0 or 1); front band keys unchanged; 5: scree went (a key); 4: rock.preset joins [rock]; 3: quality moved to the header (it was a row of [rock]); 2: setback became recess. A spec of another version is refused, never read silently. */
  function num(v){return String(+(+v).toFixed(6))}
  function fmt(v){return Array.isArray(v)?v.map(num).join(' '):(typeof v==='number'?num(v):String(v))}
  function get(o,path){var s=path.split(/[.\[\]]+/).filter(function(x){return x!==''}),v=o,i;for(i=0;i<s.length;i++){if(v==null)return undefined;v=v[/^\d+$/.test(s[i])?+s[i]:s[i]]}return v}
  function set(o,path,val){var s=path.split(/[.\[\]]+/).filter(function(x){return x!==''}).map(function(x){return /^\d+$/.test(x)?+x:x}),i,v=o;for(i=0;i<s.length-1;i++){if(v[s[i]]==null)v[s[i]]=typeof s[i+1]==='number'?[0,0]:{};v=v[s[i]]}v[s[s.length-1]]=val}
  /* the keys of a panel, in the table's order; a weighting is three keys; a skew row lives in its range's line */
  function keysOf(T,panel){var out=[];T.ROWS.forEach(function(r){if(r.panel!==panel||!r.path||r.path.indexOf('[')>=0)return;if(r.kind==='weighting'){var b=r.path.slice(0,r.path.length-2);['w','a','c'].forEach(function(x){out.push(b+'.'+x)})}else out.push(r.path)});return out}
  /* m: {build,preset,system,seed,level,mode,target,rows,projection,off:[],solo,P,FP(the preset's P),rock,frock}; T: {ROWS,PANELS,rockKeys} */
  function write(m,T){var L=[NAME,'build '+m.build,'preset '+m.preset,'system '+m.system,'seed '+m.seed,'level '+m.level,'view '+m.mode,'target '+(m.target?1:0),'rows '+(m.rows?1:0),'projection '+m.projection,'quality '+m.tier];
    if(m.off.length)L.push('off '+m.off.join(' '));if(m.solo)L.push('solo '+m.solo);
    T.PANELS.forEach(function(p){L.push('','['+p.id+']');keysOf(T,p.id).forEach(function(k){var v=get(m.P,k),f=get(m.FP,k);
      if(v===undefined)v=Array.isArray(f)?[0,0]:0;var line=k+' '+fmt(v);if(JSON.stringify(v)!==JSON.stringify(f===undefined?v:f))line+='   # preset '+fmt(f);L.push(line)});
      if(p.id==='rock'){L.push('rock.preset '+(m.rockPreset||'none'));T.rockKeys.forEach(function(k){var v=m.rock[k],f=m.frock[k];var line=k+' '+fmt(v);if(v!==f)line+='   # preset '+fmt(f);L.push(line)})}});
    return L.join('\n')}
  /* read: the header and the values by key; anything from [result] on is not read */
  function read(text){var S={header:{},values:{},rock:{},section:{}},sec='',lines=text.split(/\r?\n/),i;var first=lines.filter(function(l){return l.trim()})[0]||'';if(first.trim()!==NAME)throw new Error('this is not a "'+NAME+'" spec (its first line is "'+first.trim()+'"); an older spec is not loaded silently');
    for(i=0;i<lines.length;i++){var l=lines[i].replace(/#.*$/,'').trim();if(!l)continue;var h=l.match(/^\[(\w+)\]$/);if(h){sec=h[1];if(sec==='result'||sec==='problems')break;continue}
      var t=l.split(/\s+/),k=t[0],v=t.slice(1);if(i===0&&l===NAME)continue;
      if(!sec){S.header[k]=v;continue}
      S.section[k]=sec;if(sec==='rock'&&/^(cuts|chunks|chips|pockets|grain|wear|patch)/.test(k)&&k!=='tier')S.rock[k]=v[0];else S.values[k]=v}
    return S}
  /* the P a spec means: the preset's overrides as nested objects (the harness builds with these) */
  function typed(v,f){if(Array.isArray(f))return v.map(Number);if(typeof f==='number')return +v[0];return v[0]}
  function overrides(S,FP){var p={};Object.keys(S.values).forEach(function(k){var f=get(FP,k);if(k==='tier'||k==='rock.preset')return;if(f===undefined&&S.values[k].length>1)f=[0,0];set(p,k,typed(S.values[k],f===undefined?0:f))});return p}
  return {NAME:NAME,write:write,read:read,overrides:overrides,get:get,set:set,keysOf:keysOf,fmt:fmt}
})();
