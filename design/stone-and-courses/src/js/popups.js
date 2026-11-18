/* The pop-up for a control: one card below the control and its label, on hover or keyboard focus (on a phone, a tap on the label). It is read from the controls table (controls.js), so it can never disagree with the "controls in this step" lists. */
(function(){
  var card=document.createElement('div');card.id='ctl-card';card.setAttribute('role','tooltip');card.hidden=true;document.body.appendChild(card);
  var cur=null,pinned=false,hideT=0;
  function esc(s){return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;')}
  function anchorOf(el){return el.closest('.ddl')||el.closest('.cl')||el.closest('.r')||el.closest('.ctl')||el.closest('.gh')||el.closest('.pnl>.phd')||el}
  function place(el){var a=anchorOf(el).getBoundingClientRect(),vw=document.documentElement.clientWidth,vh=window.innerHeight,w=Math.min(300,vw-16);card.style.width=w+'px';card.style.left='0px';card.style.top='0px';var h=card.offsetHeight;
    var left=Math.max(8,Math.min(a.left,vw-w-8)),top=a.bottom+6;if(top+h>vh-8&&a.top-h-6>8)top=a.top-h-6;card.style.left=left+'px';card.style.top=top+'px'}
  function show(el,pin){if(document.querySelector('.ddl:not([hidden])'))return;var k=el.getAttribute('data-ctl'),r=CTL&&CTL[k];if(!r)return;clearTimeout(hideT);cur=el;pinned=!!pin;
    card.innerHTML='<b>'+esc(r.name)+'</b><p>'+esc(r.text)+'</p><a class="cc-link" href="#'+r.step+'" data-step="'+r.step+'">How '+esc((STEPS[r.step]||r.step).replace(/^Step \d+, /,'').replace(/^the /,'the '))+' does this &darr;</a>';
    card.hidden=false;el.setAttribute('aria-describedby','ctl-card');place(el)}
  function hide(){if(pinned)return;card.hidden=true;if(cur){cur.removeAttribute('aria-describedby');cur=null}}
  function hideNow(){pinned=false;hide()}
  document.addEventListener('mouseover',function(e){var el=e.target.closest&&e.target.closest('[data-ctl]');if(el&&!(e.target.closest&&e.target.closest('#ctl-card'))&&window.matchMedia('(hover:hover)').matches)show(el,false);else if(e.target.closest&&e.target.closest('#ctl-card'))clearTimeout(hideT)});
  document.addEventListener('mouseout',function(e){if(pinned)return;var to=e.relatedTarget;if(to&&to.closest&&(to.closest('#ctl-card')||(cur&&cur.contains(to))))return;clearTimeout(hideT);hideT=setTimeout(hide,180)});
  document.addEventListener('focusin',function(e){var el=e.target.closest&&e.target.closest('[data-ctl]');if(el)show(el,false)});
  document.addEventListener('focusout',function(e){if(!pinned)hideT=setTimeout(hide,150)});
  document.addEventListener('keydown',function(e){if(e.key==='Escape')hideNow()});
  /* touch: a tap on the label opens the card and pins it; a tap anywhere else closes it */
  document.addEventListener('click',function(e){var t=e.target;
    var lk=t.closest&&t.closest('#ctl-card .cc-link');if(lk){e.preventDefault();var id=lk.getAttribute('data-step');hideNow();goStep(id);return}
    if(t.closest&&t.closest('#ctl-card'))return;
    var lab=t.closest&&t.closest('.cll,.ctl label,.swl,.rl');
    if(lab&&!window.matchMedia('(hover:hover)').matches){var el=lab.closest('[data-ctl]');if(el){if(cur===el&&pinned){hideNow()}else{pinned=false;show(el,true)}return}}
    if(pinned)hideNow()});
  window.addEventListener('scroll',function(){if(cur&&!card.hidden)place(cur)},{passive:true});window.addEventListener('resize',function(){if(cur&&!card.hidden)place(cur)});
  function goStep(id){var s=document.getElementById(id);if(!s)return;var d=s.closest&&s.closest('details');while(d){d.open=true;d=d.parentElement&&d.parentElement.closest&&d.parentElement.closest('details')}
    s.scrollIntoView({behavior:'smooth',block:'start'});s.classList.remove('stepflash');void s.offsetWidth;s.classList.add('stepflash');setTimeout(function(){s.classList.remove('stepflash')},2200);if(history.replaceState)history.replaceState(null,'','#'+id)}
  window.goStep=goStep;window.hideCtlCard=function(){pinned=false;card.hidden=true;if(cur){cur.removeAttribute('aria-describedby');cur=null}};
})();
/* the "controls in this step" lists are read from the same table as the pop-ups */
function fillStepControls(){var rows={};Object.keys(CTL).forEach(function(k){var s=CTL[k].step;(rows[s]=rows[s]||[]).push(k)});
  var dials={};if(typeof DIALS!=='undefined')DIALS.forEach(function(d){dials[d[0]]=d});
  [].forEach.call(document.querySelectorAll('.stepctl'),function(box){var id=box.getAttribute('data-step'),ks=rows[id]||[];if(!ks.length){box.innerHTML='<h4>Controls in this step</h4><p class="small">None of its own: it works from <b>setback</b> and <b>lip</b> and from the shapes the other steps make.</p>';return}
    var h='<h4>Controls in this step</h4><dl>';ks.forEach(function(k){var r=CTL[k],d=dials[k],meta='';if(d){var df=(typeof DEFAULTS!=='undefined')?DEFAULTS[k]:undefined;meta='<small>'+d[3]+' to '+d[4]+(d[6]?' '+d[6]:'')+(df!==undefined?'; default '+(Array.isArray(df)?df.slice(0,2).join(' to '):df):'')+'</small>'}
      var pl=r.panel==='ui'?(['Preset','Scale','seed'].indexOf(k)>=0?'Setup':'Viewer'):((PANELS.filter(function(p){return p.id===r.panel})[0]||{}).title||'');if(pl)meta+='<small>'+pl+(r.panel==='ui'?'':' panel')+'</small>';
      h+='<dt data-ctlrow="'+k+'">'+r.name+meta+'</dt><dd>'+r.text+'</dd>'});box.innerHTML=h+'</dl>'})}
window.addEventListener('load',function(){setTimeout(function(){fillStepControls();checkControls(document)},400)});

/* the table of words: the dials, read from the same table as the pop-ups */
window.addEventListener('load',function(){setTimeout(function(){var t=document.getElementById('wordtable');if(!t||typeof DIALS==='undefined')return;var h='<div class="tw"><table><tr><th>Word<th>Range<th>Default<th>Meaning (one)<th>Step</tr>';DIALS.forEach(function(d){var r=CTL[d[0]];if(!r)return;var df=DEFAULTS[d[0]];h+='<tr><td><code>'+d[0]+'</code><td>'+d[3]+' to '+d[4]+(d[6]?' '+d[6]:'')+'<td>'+(df===undefined?'':(Array.isArray(df)?df.slice(0,2).join(' to '):df))+'<td>'+r.text+'<td><a href="#'+r.step+'">'+(STEPS[r.step]||r.step)+'</a>'});t.innerHTML=h+'</table></div>'},500)});

/* the contents rail: the steps show under 'The algorithm' only while the reader is inside that section, and the current step is marked */
window.addEventListener('load',function(){var nav=document.querySelector('nav.toc'),sec=document.getElementById('algo');if(!nav||!sec||!window.IntersectionObserver)return;
  var links={};[].forEach.call(nav.querySelectorAll('.toc-sub a'),function(a){links[a.getAttribute('data-step')]=a});
  /* the current step is the last one whose top has passed a line 30% down the screen; one rule, read on every scroll */
  var steps=[].slice.call(document.querySelectorAll('article.step')),queued=false;
  function mark(){queued=false;var line=innerHeight*0.3,cur=null;steps.forEach(function(a){if(a.getBoundingClientRect().top<=line)cur=a.id});
    Object.keys(links).forEach(function(k){if(k===cur)links[k].setAttribute('aria-current','true');else links[k].removeAttribute('aria-current')})}
  addEventListener('scroll',function(){if(!queued){queued=true;requestAnimationFrame(mark)}},{passive:true});addEventListener('resize',mark);mark();
  new IntersectionObserver(function(es){es.forEach(function(e){nav.classList.toggle('in-algo',e.isIntersecting)})},{rootMargin:'-20% 0px -20% 0px'}).observe(sec)});
