/* what the page catches while it runs: console errors and warnings, uncaught errors, failed promises; the spec's [problems] lists them */
window.__problems=[];
(function(){function add(s){try{if(window.__problems.length<40)window.__problems.push(String(s).slice(0,300))}catch(e){/* nothing to do if even this fails */}}
  ['error','warn'].forEach(function(k){var o=console[k];console[k]=function(){add(k+': '+[].map.call(arguments,function(a){return a&&a.stack?a.stack.split('\n')[0]:String(a)}).join(' '));return o.apply(console,arguments)}});
  window.addEventListener('error',function(e){add('error: '+e.message+(e.filename?' ('+e.filename.split('/').pop()+':'+e.lineno+')':''))});
  window.addEventListener('unhandledrejection',function(e){add('rejection: '+(e.reason&&e.reason.message||e.reason))})})();
