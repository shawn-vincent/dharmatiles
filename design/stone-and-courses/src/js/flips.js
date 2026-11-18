/* Every reference photo flips between painted and unpainted by a click, a tap, or Enter and Space; a page-wide switch flips them all, and the preset thumbnails follow it. Nothing is written on the photo: the button's name says what the next press shows. */
(function(){
  function setOne(b,un){var im=b.querySelector('img');if(!im.getAttribute('data-p'))im.setAttribute('data-p',im.src);im.src=un?im.getAttribute('data-unp'):im.getAttribute('data-p');b.setAttribute('aria-pressed',String(un));b.setAttribute('aria-label',un?'Show painted':'Show unpainted')}
  document.addEventListener('click',function(e){var b=e.target.closest&&e.target.closest('button.flip');if(!b)return;setOne(b,b.getAttribute('aria-pressed')!=='true')});
  window.setAllUnpainted=function(v){window.UNP=!!v;[].forEach.call(document.querySelectorAll('button.flip'),function(b){setOne(b,!!v)});if(window.X&&X.refreshRefs)X.refreshRefs();var t=document.getElementById('unpall');if(t){t.setAttribute('aria-checked',String(!!v))}};
  window.addEventListener('load',function(){var t=document.getElementById('unpall');if(t)t.onclick=function(){window.setAllUnpainted(t.getAttribute('aria-checked')!=='true')}});
})();
