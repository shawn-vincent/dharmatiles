/* ===== the control components: four kinds, each made by one function, each described by one row of the table in controls.js =====
   num      a number: label, number box and unit, one slider
   range    lo .. hi: label, two number boxes and the unit once, one slider with two handles
   choice   one of a few: up to three are a segmented group, more are a drop-down list; a yes/no is a two-way choice; the Level strip is a choice drawn as a strip
   group    a titled set of rows, with an optional on/off and Solo, and a "more" fold (a stage, a weighting, a panel are all groups)
   A row reads and writes the state by a path ('top[1]', 'band.depth'), so one component serves every row; nothing here knows what a row means. */
var Widgets=(function(){
  var refreshers=[],ddN=0;
  function reg(f){refreshers.push(f)}
  function refreshAll(){refreshers.forEach(function(f){f()})}
  /* ---- state paths: 'a', 'a.b', 'a[2]' ---- */
  function steps(path){return path.split(/[.\[\]]+/).filter(function(x){return x!==''}).map(function(x){return /^\d+$/.test(x)?+x:x})}
  function getPath(o,path){var s=steps(path),i,v=o;for(i=0;i<s.length;i++){if(v==null)return undefined;v=v[s[i]]}return v}
  function setPath(o,path,val){var s=steps(path),i,v=o;for(i=0;i<s.length-1;i++){if(v[s[i]]==null)v[s[i]]=typeof s[i+1]==='number'?[0,0]:{};v=v[s[i]]}v[s[s.length-1]]=val}
  function decimals(step){var t=String(step),i=t.indexOf('.');return i<0?0:t.length-i-1}
  function fmt(v,step){return (+v).toFixed(decimals(step))}
  function el(tag,cls,html){var e=document.createElement(tag);if(cls)e.className=cls;if(html!=null)e.innerHTML=html;return e}
  /* ---- icons (the Level strip, the View and Run choices) ---- */
  var SV=function(p){return '<svg viewBox="0 0 24 18" aria-hidden="true">'+p+'</svg>'};
  var ICONS={Level:{0:SV('<rect x="3" y="4" width="18" height="10" stroke-dasharray="3 2"/>'),1:SV('<path d="M3 4h18M3 9h18M3 14h18"/>'),2:SV('<rect x="3" y="3" width="18" height="12"/><path d="M12 3v12M3 9h18"/>'),3:SV('<rect x="3" y="3" width="8" height="5"/><rect x="13" y="3" width="8" height="5"/><rect x="3" y="10" width="18" height="5"/>'),4:SV('<path d="M3 15L8 3l6 4 7-4v12z"/><path d="M8 3l2 12M14 7l-2 8"/>'),5:SV('<rect x="3" y="2" width="18" height="8"/><circle cx="7" cy="14" r="1.3"/><circle cx="12" cy="14.6" r="1.1"/><circle cx="17" cy="14" r="1.3"/>'),6:SV('<rect x="3" y="3" width="18" height="12" fill="currentColor" fill-opacity=".3"/>')},
    Run:{1:SV('<rect x="8" y="5" width="8" height="8"/>'),2:SV('<rect x="4" y="5" width="16" height="8"/><path d="M12 5v8"/>'),4:SV('<rect x="1.5" y="6" width="21" height="6"/><path d="M6.75 6v6M12 6v6M17.25 6v6"/>')},
    View:{shaded:SV('<circle cx="12" cy="9" r="6" fill="currentColor" fill-opacity=".35"/>'),over:SV('<circle cx="12" cy="9" r="6"/><path d="M12 3a6 6 0 0 1 0 12z" fill="#d63a2b" stroke="none"/>'),brush:SV('<circle cx="12" cy="9" r="6"/><path d="M7 12l3-4 2 2 3-4" />'),lines:SV('<path d="M4 5h16M4 9h16M4 13h16"/>')}};
  var VIEWI={'3d':SV('<path d="M12 2l8 4v8l-8 4-8-4V6z"/><path d="M12 10l8-4M12 10v8M12 10L4 6"/>'),plan:SV('<rect x="3" y="3" width="18" height="12"/><path d="M9 3v12M15 3v12M3 9h18"/>'),elev:SV('<rect x="5" y="2" width="14" height="14"/><path d="M5 7h14M5 12h14M12 2v5M9 7v5M15 7v5"/>'),costs:SV('<path d="M4 15V9M9 15V4M14 15v-7M19 15V6"/>')};
  function iconFor(group,v){return ICONS[group]&&ICONS[group][v]||''}
  /* ---- choice: a drop-down list (a listbox: arrows, Home, End, Enter, Escape) ---- */
  function dropdown(cfg){var w=el('div','cl'+(cfg.wide?' wide':''));w.setAttribute('data-ctl',cfg.ctl);var id='dd'+(++ddN),act=-1;
    w.innerHTML=(cfg.nolabel?'':'<span class="cll">'+cfg.label+'</span>')+'<div class="dd"><button type="button" class="ddb" aria-haspopup="listbox" aria-expanded="false" aria-label="'+cfg.label+'"></button><ul class="ddl" role="listbox" tabindex="-1" aria-label="'+cfg.label+'" hidden></ul></div>';
    var btn=w.querySelector('.ddb'),ul=w.querySelector('.ddl');
    function list(){return cfg.opts()}
    function render(){var os=list(),cv0=cfg.get(),cur=os.filter(function(o){return o.v===cv0})[0];btn.innerHTML=(cur&&cur.img?'<span class="ddi">'+cur.img+'</span>':'')+'<span class="ddt">'+(cur?cur.text:(cfg.placeholder||''))+'</span><span class="ddc" aria-hidden="true">&#9662;</span>';
      ul.innerHTML='';os.forEach(function(o,i){var li=el('li');li.setAttribute('role','option');if(cfg.optctl)li.setAttribute('data-ctl',cfg.optctl+o.v);li.id=id+'-'+i;li.setAttribute('aria-selected',String(o.v===cv0));li.innerHTML=(o.img?'<span class="ddi">'+o.img+'</span>':'')+'<span class="ddt">'+o.text+(o.badge?' <small class="bdg">'+o.badge+'</small>':'')+'</span>';li.onmousedown=function(e){e.preventDefault()};li.onclick=function(){choose(o)};li.onmousemove=function(){setAct(i)};ul.appendChild(li)})}
    function setAct(i){act=i;[].forEach.call(ul.children,function(li,j){li.classList.toggle('act',j===i)});if(ul.children[i]){ul.setAttribute('aria-activedescendant',ul.children[i].id);if(ul.children[i].scrollIntoView)ul.children[i].scrollIntoView({block:'nearest'})}}
    function open(){if(window.hideCtlCard)window.hideCtlCard();render();ul.hidden=false;btn.setAttribute('aria-expanded','true');var os=list(),k=0;os.forEach(function(o,i){if(o.v===cfg.get())k=i});setAct(k);ul.focus()}
    function close(f){ul.hidden=true;btn.setAttribute('aria-expanded','false');if(f)btn.focus()}
    function choose(o){close(true);cfg.set(o.v);render()}
    btn.onclick=function(){ul.hidden?open():close(true)};
    btn.onkeydown=function(e){if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();open()}};
    ul.onkeydown=function(e){var os=list(),n=os.length;if(e.key==='ArrowDown'){e.preventDefault();setAct(Math.min(n-1,act+1))}else if(e.key==='ArrowUp'){e.preventDefault();setAct(Math.max(0,act-1))}else if(e.key==='Home'){e.preventDefault();setAct(0)}else if(e.key==='End'){e.preventDefault();setAct(n-1)}else if(e.key==='Enter'||e.key===' '){e.preventDefault();if(os[act])choose(os[act])}else if(e.key==='Escape'||e.key==='Tab'){close(e.key==='Escape')}};
    ul.onblur=function(){setTimeout(function(){if(!ul.hidden&&document.activeElement!==ul)close(false)},120)};
    w.render=render;reg(render);render();return w}
  /* ---- choice: up to three ways, a segmented group; a strip is the same group drawn as one line of icons ---- */
  function segmented(cfg){var w=el('div','cl'+(cfg.wide?' wide':'')+(cfg.strip?' strip':''));w.setAttribute('data-ctl',cfg.ctl);
    w.innerHTML=(cfg.nolabel?'':'<span class="cll">'+cfg.label+'</span>')+'<div class="seg2" role="group" aria-label="'+cfg.label+'"></div>';var g=w.querySelector('.seg2');
    function refresh(){[].forEach.call(g.children,function(x,i){x.setAttribute('aria-pressed',String(cfg.opts[i].v===cfg.get()))})}
    cfg.opts.forEach(function(o){var b=el('button');b.type='button';b.innerHTML=(o.img?'<span class="ddi">'+o.img+'</span>':'')+'<span>'+o.text+'</span>';if(o.title)b.title=o.title;if(cfg.optctl)b.setAttribute('data-ctl',cfg.optctl+o.v);b.onclick=function(){cfg.set(o.v);refresh()};g.appendChild(b)});
    w.refresh=refresh;reg(refresh);refresh();return w}
  /* ---- choice: the one maker. segmented up to three ways (and for a strip), else a drop-down ---- */
  function choice(cfg){return (cfg.opts().length<=3||cfg.strip)&&!cfg.list?segmented({ctl:cfg.ctl,label:cfg.label,wide:cfg.wide,strip:cfg.strip,nolabel:cfg.nolabel,optctl:cfg.optctl,get:cfg.get,set:cfg.set,opts:cfg.opts()}):dropdown(cfg)}
  /* ---- num ---- */
  function num(def,st){var w=el('div','r r-num');w.setAttribute('data-ctl',def.ctl);
    w.innerHTML='<div class="rh"><label class="rl"></label><span class="rv"><input type="number" class="nb" aria-label="'+def.label+'"><span class="un"></span></span></div><input type="range" class="rs" aria-label="'+def.label+'">';
    var lab=w.querySelector('.rl'),nb=w.querySelector('.nb'),un=w.querySelector('.un'),rs=w.querySelector('.rs'),spec=def;
    function apply(sp){spec=sp;lab.textContent=sp.label;un.textContent=sp.unit||'';[nb,rs].forEach(function(i){i.min=sp.min;i.max=sp.max;i.step=sp.step})}
    function show(){var v=st.get(def.path);if(v==null)v=spec.zero!=null?spec.zero:spec.min;nb.value=fmt(v,spec.step);rs.value=v}
    function put(v){v=Math.max(spec.min,Math.min(spec.max,+v));if(isNaN(v))return;st.set(def.path,v);show();st.change()}
    rs.oninput=function(){put(rs.value)};nb.onchange=function(){put(nb.value)};
    apply(def);show();w.apply=apply;w.refresh=show;reg(show);return w}
  /* ---- range: lo .. hi ---- */
  function range(def,st){var w=el('div','r r-range');w.setAttribute('data-ctl',def.ctl);
    w.innerHTML='<div class="rh"><label class="rl">'+def.label+'</label><span class="rv"><input type="number" class="nb lo" aria-label="'+def.label+' from"><span class="dash">to</span><input type="number" class="nb hi" aria-label="'+def.label+' to"><span class="un">'+(def.unit||'')+'</span></span></div><div class="dual"><input type="range" class="rs lo" aria-label="'+def.label+' from"><input type="range" class="rs hi" aria-label="'+def.label+' to"><i class="fill"></i></div>';
    var I=[].slice.call(w.querySelectorAll('input')),nlo=I[0],nhi=I[1],slo=I[2],shi=I[3],fill=w.querySelector('.fill');
    [nlo,nhi,slo,shi].forEach(function(i){i.min=def.min;i.max=def.max;i.step=def.step});
    function pair(){var v=st.get(def.path);return v?[+v[0],+v[1]]:[0,0]}
    function show(){var p=pair();nlo.value=fmt(p[0],def.step);nhi.value=fmt(p[1],def.step);slo.value=p[0];shi.value=p[1];var a=(p[0]-def.min)/(def.max-def.min),b=(p[1]-def.min)/(def.max-def.min);fill.style.left=(100*a)+'%';fill.style.right=(100*(1-b))+'%'}
    function put(i,v){v=Math.max(def.min,Math.min(def.max,+v));if(isNaN(v))return;var p=pair();p[i]=v;if(i===0&&p[0]>p[1])p[1]=p[0];if(i===1&&p[1]<p[0])p[0]=p[1];st.set(def.path+'[0]',p[0]);st.set(def.path+'[1]',p[1]);show();st.change()}
    slo.oninput=function(){put(0,slo.value)};shi.oninput=function(){put(1,shi.value)};nlo.onchange=function(){put(0,nlo.value)};nhi.onchange=function(){put(1,nhi.value)};
    show();w.refresh=show;reg(show);return w}
  /* ---- group: a title, its one line, an optional on/off and Solo, the main rows, and a "more" fold ---- */
  function group(def){var g=el('section','grp'+(def.cls?' '+def.cls:''));if(def.id)g.setAttribute('data-group',def.id);
    var head=el('div','gh');head.innerHTML='<b class="gt">'+def.title+'</b>'+(def.line?'<small class="gl">'+def.line+'</small>':'')+'<span class="gsum"></span>';g.appendChild(head);g.classList.add('collapsed');
    head.setAttribute('role','button');head.tabIndex=0;head.setAttribute('aria-expanded','false');
    function flip(){var c=g.classList.toggle('collapsed');head.setAttribute('aria-expanded',String(!c))}
    head.onclick=function(e){if(e.target.closest('.gtools'))return;flip()};head.onkeydown=function(e){if((e.key==='Enter'||e.key===' ')&&e.target===head){e.preventDefault();flip()}};
    var tools=el('span','gtools seg tri');head.insertBefore(tools,head.firstChild.nextSibling);
    var body=el('div','gb');g.appendChild(body);var more=null,morebody=null;
    function addMore(){if(more)return;more=el('details','more');more.innerHTML='<summary>more</summary>';morebody=el('div','gb');more.appendChild(morebody);g.appendChild(more)}
    return {el:g,head:head,tools:tools,body:body,sum:head.querySelector('.gsum'),flip:flip,add:function(row,isMore){if(isMore){addMore();morebody.appendChild(row)}else body.appendChild(row)}}}
  /* on/off (a switch) and Solo, for a stage: the two are one pair of toggles in the header */
  function stageToggles(g,id,st){
    var sw=el('button','sw swt');sw.type='button';sw.setAttribute('role','switch');sw.setAttribute('aria-label',g.title+' on or off');sw.innerHTML='<i></i>';
    var so=el('button','seg1');so.type='button';so.textContent='Solo';so.setAttribute('aria-pressed','false');
    g.tools.appendChild(sw);g.tools.appendChild(so);
    function refresh(){var on=st.on(id),solo=st.solo()===id;sw.classList.toggle('on',on);sw.setAttribute('aria-checked',String(on));so.setAttribute('aria-pressed',String(solo));g.el.classList.toggle('dim',!st.live(id))}
    sw.onclick=function(){st.toggle(id);refresh();st.change()};so.onclick=function(){st.setSolo(st.solo()===id?'':id);st.change()};
    reg(refresh);refresh();return {sw:sw,solo:so}}
  return {reg:reg,refreshAll:refreshAll,getPath:getPath,setPath:setPath,el:el,fmt:fmt,iconFor:iconFor,VIEWI:VIEWI,dropdown:dropdown,segmented:segmented,choice:choice,num:num,range:range,group:group,stageToggles:stageToggles}
})();
