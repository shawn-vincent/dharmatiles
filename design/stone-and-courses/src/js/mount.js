(function(){var REFS={{REFS}};window.REFSU={{REFSU}};var X=new Explorer(document.getElementById('xp'),REFS);window.X=X;var XS={setMF:function(){}};window.XS=XS;
var el=document.getElementById('mfwasm');var bin=Uint8Array.from(atob(el.textContent.trim()),function(c){return c.charCodeAt(0)});
window.ManifoldModule({wasmBinary:bin.buffer}).then(function(w){w.setup();MF=w;X.setMF();XS.setMF()}).catch(function(e){console.error(e);document.getElementById('xp-st').insertAdjacentHTML('beforeend','<p class="bad">The mesh engine failed to start: '+String(e&&e.message||e)+'</p>')});})();
