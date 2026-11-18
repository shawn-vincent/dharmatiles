/* The explainer: one card, opened only by a small eye beside a label (never by hover, focus or a click on the control). It is read from the controls table (controls.js), so it can never disagree with the "controls in this step" lists. */
(function(){
  var card=document.createElement('div');card.id='ctl-card';card.setAttribute('role','dialog');card.setAttribute('aria-label','Explanation');card.hidden=true;document.body.appendChild(card);
  var cur=null;
  function esc(s){return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;')}
  function place(el){var a=el.getBoundingClientRect(),vw=document.documentElement.clientWidth,vh=window.innerHeight,w=Math.min(320,vw-16);card.style.width=w+'px';card.style.left='0px';card.style.top='0px';var h=card.offsetHeight;
    var left=Math.max(8,Math.min(a.left-8,vw-w-8)),top=a.bottom+6;if(top+h>vh-8&&a.top-h-6>8)top=a.top-h-6;card.style.left=left+'px';card.style.top=top+'px'}
  function close(){card.hidden=true;if(cur){cur.setAttribute('aria-expanded','false');cur=null}}
  /* the one explainer: opened only by its eye (a click or a tap), closed by the eye again, Escape or a click outside. Its content is a row of the controls table, or text the caller gives (the rock library's own explanations). */
  function open(btn){var k=btn.getAttribute('data-eye'),r=btn._about||(CTL&&CTL[k]);if(!r)return;
    var body=r.html!=null?r.html:'<p>'+esc(r.text||'')+'</p>',link=r.step?'<a class="cc-link" href="#'+r.step+'" data-step="'+r.step+'">How '+esc((STEPS[r.step]||r.step).replace(/^Step \d+[a-z]?, /,'').replace(/^the /,'the '))+' does this &darr;</a>':'';
    card.innerHTML='<b>'+esc(r.name)+'</b>'+body+link;card.hidden=false;cur=btn;btn.setAttribute('aria-expanded','true');place(btn)}
  var EYE='<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/></svg>';
  function eye(key,about){var b=document.createElement('button');b.type='button';b.className='eye';b.setAttribute('data-eye',key||'');b.setAttribute('aria-expanded','false');b.setAttribute('aria-label','What this is');b.title='What this is';b.innerHTML=EYE;if(about)b._about=about;return b}
  /* eyes: one per place, small, at the right of a label (a row, a choice, a group's title, a panel's header); read from the table */
  window.attachEyes=function(root){[].forEach.call(root.querySelectorAll('[data-ctl]'),function(h){var k=h.getAttribute('data-ctl');if(!CTL[k]||h.querySelector(':scope>.eye,:scope>.rl>.eye,:scope>.rh>.rl>.eye,:scope>.cll+.eye'))return;var b=eye(k);
    if(h.classList.contains('r')){var rl=h.querySelector('.rl');if(rl)rl.appendChild(b)}
    else if(h.classList.contains('cl')){var cll=h.querySelector('.cll');if(cll&&!h.classList.contains('strip'))cll.after(b)}
    else if(h.classList.contains('gh')){var gt=h.querySelector('.gt');if(gt)gt.after(b)}
    else if(h.classList.contains('ph0')){var phd=h.parentElement;phd.insertBefore(b,phd.querySelector('.prs'))}})};
  /* the rock library's own explanations (its small "i" with a pop-up on hover) become eyes in the cliff page: the library and Weathered Stone are untouched */
  window.eyeRock=function(root){[].forEach.call(root.querySelectorAll('.info'),function(i){var pop=i.querySelector('.pop'),lab=i.closest('label,.nm'),name=lab?(lab.querySelector('b')||lab.firstChild).textContent.trim():'',b=eye('',{name:name,html:'<p>'+esc(pop?pop.textContent:'')+'</p>'});i.replaceWith(b)})};
  window.eyeAbout=function(btn,about){btn._about=about};
  window.makeEye=eye;
  document.addEventListener('click',function(e){var t=e.target,lk=t.closest&&t.closest('#ctl-card .cc-link');
    if(lk){e.preventDefault();var id=lk.getAttribute('data-step');close();goStep(id);return}
    var b=t.closest&&t.closest('.eye');if(b){e.preventDefault();e.stopPropagation();if(cur===b)close();else{close();open(b)}return}
    if(t.closest&&t.closest('#ctl-card'))return;close()});
  document.addEventListener('keydown',function(e){if(e.key==='Escape'&&!card.hidden){close();if(cur)cur.focus()}});
  window.addEventListener('scroll',function(){if(cur&&!card.hidden)place(cur)},{passive:true});window.addEventListener('resize',function(){if(cur&&!card.hidden)place(cur)});
  function goStep(id){var s=document.getElementById(id);if(!s)return;var d=s.closest&&s.closest('details');while(d){d.open=true;d=d.parentElement&&d.parentElement.closest&&d.parentElement.closest('details')}
    s.scrollIntoView({behavior:'smooth',block:'start'});s.classList.remove('stepflash');void s.offsetWidth;s.classList.add('stepflash');setTimeout(function(){s.classList.remove('stepflash')},2200);if(history.replaceState)history.replaceState(null,'','#'+id)}
  window.goStep=goStep;window.hideCtlCard=close;
})();
/* the "controls in this step" lists are read from the same table as the pop-ups */
function fillStepControls(){var rows={};Object.keys(CTL).forEach(function(k){var s=CTL[k].step;(rows[s]=rows[s]||[]).push(k)});
  var dials={};if(typeof DIALS!=='undefined')DIALS.forEach(function(d){dials[d[0]]=d});
  [].forEach.call(document.querySelectorAll('.stepctl'),function(box){var id=box.getAttribute('data-step'),ks=rows[id]||[];if(!ks.length){box.innerHTML='<h4>Controls in this step</h4><p class="small">None of its own: it works from <b>recess</b> and <b>lip</b> and from the shapes the other steps make.</p>';return}
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
